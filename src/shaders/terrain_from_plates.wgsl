// Pass 3: Generate terrain from plate assignment + JFA distance fields.
// Uses plate velocities for stress, boundary classification, and asymmetric profiles.
// Includes noise.wgsl and cube_sphere.wgsl (concatenated at load time).

struct Plate {
    center: vec3<f32>,
    plate_type: f32,
    velocity: vec3<f32>,
    _pad: f32,
}

struct GenParams {
    face: u32,
    resolution: u32,
    num_plates: u32,
    seed: u32,
    amplitude: f32,
    frequency: f32,
    octaves: u32,
    gain: f32,
    lacunarity: f32,
    tile_offset_x: u32,
    tile_offset_y: u32,
    full_resolution: u32,
    mountain_scale: f32,
    boundary_width: f32,
    warp_strength: f32,
    detail_scale: f32,
    surface_gravity: f32,
    tectonics_factor: f32,
    surface_age: f32,
    continental_scale: f32,
}

struct JfaSeed {
    seed_x: i32,
    seed_y: i32,
    plate_a: i32,
    plate_b: i32,
}

@group(0) @binding(0) var<storage, read> plates: array<Plate>;
@group(0) @binding(1) var<uniform> params: GenParams;
@group(0) @binding(2) var<storage, read> plate_idx: array<u32>;
@group(0) @binding(3) var<storage, read> jfa_data: array<JfaSeed>;
@group(0) @binding(4) var<storage, read_write> heightmap: array<f32>;

// PCG hash
fn pcg_hash(input: u32) -> u32 {
    var h = input * 747796405u + 2891336453u;
    h = ((h >> ((h >> 28u) + 4u)) ^ h) * 277803737u;
    h = (h >> 22u) ^ h;
    return h;
}

fn seed_offset(s: u32) -> vec3<f32> {
    let h1 = pcg_hash(s);
    let h2 = pcg_hash(s + 1u);
    let h3 = pcg_hash(s + 2u);
    return vec3<f32>(
        f32(h1 & 0xFFFFu) / 655.35,
        f32(h2 & 0xFFFFu) / 655.35,
        f32(h3 & 0xFFFFu) / 655.35
    );
}

fn smooth_step(edge0: f32, edge1: f32, x: f32) -> f32 {
    let t = clamp((x - edge0) / (edge1 - edge0), 0.0, 1.0);
    return t * t * (3.0 - 2.0 * t);
}

// Ridged multifractal for mountain detail
fn ridged_multifractal(p: vec3<f32>, octaves: i32, lacunarity: f32, gain: f32, offset: f32) -> f32 {
    var sum = 0.0;
    var freq = 1.0;
    var amp = 0.5;
    var weight = 1.0;
    var max_val = 0.0;

    for (var i = 0; i < octaves; i++) {
        var signal = snoise(p * freq);
        signal = offset - abs(signal);
        signal *= signal;
        signal *= weight;
        weight = clamp(signal * gain, 0.0, 1.0);
        sum += signal * amp;
        max_val += amp;
        freq *= lacunarity;
        amp *= 0.5;
    }
    return sum / max_val;
}

// Domain warping (seed-dependent)
fn warp_position(pos: vec3<f32>) -> vec3<f32> {
    let warp1 = vec3<f32>(
        snoise(pos * 1.2 + seed_offset(params.seed + 5000u)),
        snoise(pos * 1.2 + seed_offset(params.seed + 5001u)),
        snoise(pos * 1.2 + seed_offset(params.seed + 5002u))
    ) * 0.20;
    let warp2 = vec3<f32>(
        snoise(pos * 3.0 + seed_offset(params.seed + 5010u)),
        snoise(pos * 3.0 + seed_offset(params.seed + 5011u)),
        snoise(pos * 3.0 + seed_offset(params.seed + 5012u))
    ) * 0.10;
    let w = params.warp_strength;
    return normalize(pos + (warp1 + warp2) * w);
}

// Classify boundary: convergent (-1), transform (0), divergent (+1)
fn classify_boundary(pos: vec3<f32>, plate_a_idx: u32, plate_b_idx: u32) -> f32 {
    let rel_velocity = plates[plate_a_idx].velocity - plates[plate_b_idx].velocity;
    let boundary_normal = normalize(plates[plate_b_idx].center - plates[plate_a_idx].center);
    let convergence = dot(rel_velocity, boundary_normal);
    return clamp(convergence * 5.0, -1.0, 1.0);
}

// Compute collision stress magnitude (0 = no stress, 1 = max collision)
fn compute_stress(plate_a_idx: u32, plate_b_idx: u32) -> f32 {
    let rel_vel = plates[plate_a_idx].velocity - plates[plate_b_idx].velocity;
    let boundary_normal = normalize(plates[plate_b_idx].center - plates[plate_a_idx].center);
    let approach_speed = -dot(rel_vel, boundary_normal); // positive when converging
    return clamp(approach_speed * 3.0, 0.0, 1.0);
}

// Subduction factor: 0 = symmetric, <0.5 = plate_a subducts, >0.5 = plate_b subducts
fn compute_subduction_factor(plate_a_idx: u32, plate_b_idx: u32) -> f32 {
    let density_a = select(2.7, 3.0, plates[plate_a_idx].plate_type < 0.5); // oceanic = denser
    let density_b = select(2.7, 3.0, plates[plate_b_idx].plate_type < 0.5);
    // Denser plate subducts (goes under)
    return clamp((density_a - density_b) * 2.0 + 0.5, 0.0, 1.0);
}

@compute @workgroup_size(16, 16)
fn main(@builtin(global_invocation_id) id: vec3<u32>) {
    let res = params.resolution;
    if (id.x >= res || id.y >= res) { return; }

    let uv = vec2<f32>(
        f32(id.x) / f32(res - 1u),
        f32(id.y) / f32(res - 1u)
    );
    let raw_pos = cube_to_sphere(params.face, uv);
    let wpos = warp_position(raw_pos);

    let idx = id.y * res + id.x;
    let my_plate = plate_idx[idx];
    let jfa = jfa_data[idx];

    // Distance to nearest boundary (normalized by resolution)
    let pixel_pos = vec2<f32>(f32(id.x), f32(id.y));
    var boundary_dist = f32(res); // default: far from boundary
    if (jfa.seed_x >= 0) {
        let seed_pos = vec2<f32>(f32(jfa.seed_x), f32(jfa.seed_y));
        boundary_dist = length(pixel_pos - seed_pos);
    }
    // Normalize to [0, ~1] range where 1 = far from boundary
    let norm_dist = boundary_dist / f32(res);

    // Physics
    let gravity_factor = 9.81 / max(params.surface_gravity, 1.0);
    let age = clamp(params.surface_age, 0.0, 1.0);
    let high_freq_weight = 1.0 - age * 0.7;
    let tect = params.tectonics_factor;

    // === Base terrain: plate type is PRIMARY land/ocean driver ===
    // Plate assignment determines continent placement. Noise adds coastline shape and detail.
    let is_continental = plates[my_plate].plate_type > 0.5;

    // Strong elevation from plate type: continental = raised, oceanic = depressed
    let plate_base = select(-0.35, 0.35, is_continental);
    // Smooth blend at plate boundaries (over ~25 pixels) for natural coastlines
    let edge_blend = smooth_step(0.0, 25.0, boundary_dist);
    let plate_height = plate_base * edge_blend;

    // Noise adds coastline irregularity and surface texture (secondary role)
    let cs = params.continental_scale;
    let c1 = snoise(wpos * cs + seed_offset(params.seed + 1000u));
    let c2 = snoise(wpos * cs * 2.0 + seed_offset(params.seed + 1010u)) * 0.20;
    let c3 = snoise(wpos * cs * 4.0 + seed_offset(params.seed + 1020u)) * 0.06;
    let c4 = snoise(wpos * cs * 8.0 + seed_offset(params.seed + 1030u)) * 0.02 * high_freq_weight;
    let continental_raw = (c1 + c2 + c3 + c4) / 1.28;
    let continental = sign(continental_raw) * pow(abs(continental_raw), 0.35);
    let noise_detail = continental * params.amplitude * 0.12;

    var height = plate_height * params.amplitude + noise_detail;

    // Highland/lowland variation within continents
    let on_land = smooth_step(-0.05, 0.05, height);
    let highland = snoise(wpos * 4.0 + seed_offset(params.seed + 1100u)) * 0.10
                 + snoise(wpos * 8.0 + seed_offset(params.seed + 1110u)) * 0.04 * high_freq_weight;
    height += highland * on_land;

    // === Boundary features (only where JFA found a boundary) ===
    if (jfa.plate_b >= 0 && jfa.seed_x >= 0) {
        let pa = u32(jfa.plate_a);
        let pb = u32(jfa.plate_b);
        let btype = classify_boundary(wpos, pa, pb);
        let stress = compute_stress(pa, pb);
        let sf = compute_subduction_factor(pa, pb);

        // Convergent: mountains
        let convergence = smooth_step(0.0, -0.5, btype);
        if (convergence > 0.01) {
            // Mountain profile: Gaussian falloff from boundary, modulated by stress
            let mountain_width = 0.08 + stress * 0.12; // wider mountains at high stress
            let mountain_falloff = exp(-norm_dist * norm_dist / (mountain_width * mountain_width));
            let base_mountain_h = convergence * stress * mountain_falloff
                * params.mountain_scale * 0.6 * gravity_factor * tect;

            // R6: Asymmetric profile — subduction creates steeper vs gentler sides
            let on_subducting_side = select(1.0, 0.0, u32(jfa.plate_a) == my_plate);
            // Subducting side (oceanic): steeper, narrower, with trench
            let asymmetry = mix(1.0, 0.6, on_subducting_side * sf);
            height += base_mountain_h * asymmetry;

            // Trench on subducting side
            let trench_zone = smooth_step(0.02, 0.005, norm_dist) * on_subducting_side * sf;
            height -= trench_zone * stress * 0.15 * tect;

            // R7: Fold ridges parallel to plate motion (Euler pole direction)
            let plate_vel = plates[my_plate].velocity;
            let vel_dir = normalize(plate_vel + vec3<f32>(0.001, 0.001, 0.001)); // avoid zero
            let fold_freq = 45.0;
            let fold_alignment = abs(dot(normalize(wpos), vel_dir));
            let fold_ridges = snoise(wpos * fold_freq + seed_offset(params.seed + 7000u));
            let fold_val = abs(fold_ridges) * fold_alignment;
            height += fold_val * mountain_falloff * convergence * stress * 0.08 * tect;
        }

        // R8: Divergent boundaries — mid-ocean ridges and rift valleys
        let divergence = smooth_step(0.0, 0.5, btype);
        if (divergence > 0.01) {
            let rift_width = 0.03;
            let rift_falloff = exp(-norm_dist * norm_dist / (rift_width * rift_width));
            let on_land = select(0.0, 1.0, height > 0.0);
            // Ocean: subtle ridge. Land: rift valley depression
            let rift_h = mix(0.06, -0.10, on_land) * divergence * rift_falloff * tect;
            height += rift_h;
        }
    }

    // === R9: Continental shelves near coastlines ===
    let is_coast = height > -0.15 && height < 0.05;
    if (is_coast) {
        let coast_zone = smooth_step(-0.15, -0.02, height) * smooth_step(0.05, 0.0, height);
        height = mix(height, -0.02, coast_zone * 0.3);
    }

    // === R11/R12: Stress-driven roughness ===
    // Compute local stress for detail amplitude scaling
    var local_stress = 0.0;
    if (jfa.plate_b >= 0 && jfa.seed_x >= 0) {
        local_stress = compute_stress(u32(jfa.plate_a), u32(jfa.plate_b));
        // Stress decays with distance from boundary
        let stress_reach = 0.15;
        local_stress *= exp(-norm_dist / stress_reach);
    }

    // Detail noise: amplitude scales with stress (craggy orogens, smooth cratons)
    let base_detail_amp = 0.3 + local_stress * 0.9; // 0.3 base → up to 1.2 near orogens
    let mt_warp = vec3<f32>(
        snoise(raw_pos * 2.0 + seed_offset(params.seed + 6000u)),
        snoise(raw_pos * 2.0 + seed_offset(params.seed + 6001u)),
        snoise(raw_pos * 2.0 + seed_offset(params.seed + 6002u))
    ) * 0.12;
    let mpos = raw_pos + mt_warp;
    let detail = ridged_multifractal(
        mpos * 5.0 + seed_offset(params.seed + 2100u),
        5, 2.2, 2.0, 1.0
    );
    height += detail * base_detail_amp * params.detail_scale * high_freq_weight * gravity_factor * 0.12;

    // Additional fBm detail for texture
    var fval = 0.0;
    var freq = params.frequency;
    var amp = params.amplitude * 0.04;
    let fp = raw_pos + seed_offset(params.seed);
    for (var i = 0u; i < params.octaves; i++) {
        fval += amp * snoise(fp * freq);
        freq *= params.lacunarity;
        amp *= params.gain;
    }
    height += fval * high_freq_weight * gravity_factor;

    // Volcanic hotspots
    let hotspot_count = u32(3.0 + (1.0 - tect) * 5.0);
    let hotspot_radius = 0.02 + (1.0 - tect) * 0.04;
    let hotspot_height = 0.4 * gravity_factor;
    for (var h = 0u; h < hotspot_count; h++) {
        let hx = seed_offset(params.seed + 500u + h * 10u);
        let hotspot_center = normalize(hx);
        let hotspot_dist = 1.0 - dot(wpos, hotspot_center);
        if (hotspot_dist < hotspot_radius) {
            let volcano_h = hotspot_height * (1.0 - hotspot_dist / hotspot_radius);
            height = max(height, volcano_h);
        }
    }

    // Hypsometric shaping
    if (height > 0.0) {
        let h_cap = 1.5 * max(params.mountain_scale, 1.0) * gravity_factor;
        let h_norm = min(height, h_cap);
        let t = h_norm / h_cap;
        height = pow(t, 1.3) * h_cap;
    } else {
        height *= 1.2;
    }

    heightmap[idx] = height;
}
