// Tectonic plate compute shader.
// Pass 1: Assign each pixel to nearest plate, compute boundary info.
// Pass 2: Generate height from plate type + boundary type + detail noise.
// Includes noise.wgsl and cube_sphere.wgsl (concatenated at load time).

struct Plate {
    center: vec3<f32>,
    plate_type: f32, // 1.0 = continental, 0.0 = oceanic
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
    mountain_scale: f32,   // multiplier for tectonic mountain height
    boundary_width: f32,   // sigma for boundary influence spread
    warp_strength: f32,    // domain warp intensity
    detail_scale: f32,     // fBm detail noise intensity
    surface_gravity: f32,  // m/s² (lower gravity → taller mountains)
    tectonics_factor: f32, // [0,1]: boundary force strength
    surface_age: f32,      // [0,1]: 0=young/sharp, 1=old/smooth
    continental_scale: f32, // noise frequency for continent size
}

@group(0) @binding(0) var<storage, read> plates: array<Plate>;
@group(0) @binding(1) var<uniform> params: GenParams;
@group(0) @binding(2) var<storage, read_write> heightmap: array<f32>;

// PCG-style hash for seed offsets — much better randomization than golden ratio
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
        f32(h1 & 0xFFFFu) / 655.35,  // 0-100 range
        f32(h2 & 0xFFFFu) / 655.35,
        f32(h3 & 0xFFFFu) / 655.35
    );
}

fn smooth_step(edge0: f32, edge1: f32, x: f32) -> f32 {
    let t = clamp((x - edge0) / (edge1 - edge0), 0.0, 1.0);
    return t * t * (3.0 - 2.0 * t);
}

// Ridged multifractal noise — sharp peaks at zero crossings with weight feedback.
// Produces natural mountain ridges: sharp crests, multi-scale detail, gaps at valleys.
// offset: controls ridge sharpness (1.0 = sharp ridges at zero crossings)
// gain:   how strongly each octave weight is fed back (2.0 = strong cascading detail)
fn ridged_multifractal(p: vec3<f32>, octaves: i32, lacunarity: f32, gain: f32, offset: f32) -> f32 {
    var sum = 0.0;
    var freq = 1.0;
    var amp = 0.5;
    var weight = 1.0;
    var max_val = 0.0;

    for (var i = 0; i < octaves; i++) {
        var signal = snoise(p * freq);
        signal = offset - abs(signal); // Sharp ridges at zero crossings
        signal *= signal;              // Square: sharpens peaks, flattens valleys
        signal *= weight;              // Weight feedback: peaks attract finer detail
        weight = clamp(signal * gain, 0.0, 1.0);

        sum += signal * amp;
        max_val += amp;
        freq *= lacunarity;
        amp *= 0.5;
    }
    return sum / max_val; // Normalise to ~0–1
}

// Domain warping for fractal coastlines — seed-dependent
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
    let warp3 = vec3<f32>(
        snoise(pos * 7.0 + seed_offset(params.seed + 5020u)),
        snoise(pos * 7.0 + seed_offset(params.seed + 5021u)),
        snoise(pos * 7.0 + seed_offset(params.seed + 5022u))
    ) * 0.04;
    let w = params.warp_strength;
    return normalize(pos + (warp1 + warp2 + warp3) * w);
}

// Find the two nearest plates and compute boundary info
struct PlateInfo {
    nearest_idx: u32,
    second_idx: u32,
    nearest_dist: f32,
    second_dist: f32,
    nearest_type: f32,
    second_type: f32,
}

fn find_nearest_plates(sphere_pos: vec3<f32>) -> PlateInfo {
    var info: PlateInfo;
    info.nearest_dist = 100.0;
    info.second_dist = 100.0;
    info.nearest_idx = 0u;
    info.second_idx = 0u;

    for (var i = 0u; i < params.num_plates; i++) {
        // Great-circle distance approximation using dot product
        var d = 1.0 - dot(sphere_pos, plates[i].center);

        // Per-plate noise bias: each plate has a unique noise field that
        // pushes its boundary outward in some directions and inward in others.
        // This creates concave coastlines, peninsulas, and bays instead of convex polygons.
        // Three octaves: large-scale concavities + medium bends + fine irregularity.
        let plate_offset = vec3<f32>(f32(i) * 17.3, f32(i) * 31.7, f32(i) * 43.1);
        let bias = snoise(sphere_pos * 2.0 + plate_offset) * 0.08
                 + snoise(sphere_pos * 4.5 + plate_offset * 2.0) * 0.04;
        d += bias;

        if (d < info.nearest_dist) {
            info.second_dist = info.nearest_dist;
            info.second_idx = info.nearest_idx;
            info.second_type = info.nearest_type;
            info.nearest_dist = d;
            info.nearest_idx = i;
            info.nearest_type = plates[i].plate_type;
        } else if (d < info.second_dist) {
            info.second_dist = d;
            info.second_idx = i;
            info.second_type = plates[i].plate_type;
        }
    }
    return info;
}

// Classify boundary type from relative plate motion
// Returns: -1 = convergent, 0 = transform, 1 = divergent
fn classify_boundary(sphere_pos: vec3<f32>, plate_a: u32, plate_b: u32) -> f32 {
    let rel_velocity = plates[plate_a].velocity - plates[plate_b].velocity;
    // Boundary normal: direction from one plate center to the other
    let boundary_normal = normalize(plates[plate_b].center - plates[plate_a].center);
    let convergence = dot(rel_velocity, boundary_normal);

    // Positive = plates moving apart (divergent)
    // Negative = plates moving together (convergent)
    return clamp(convergence * 5.0, -1.0, 1.0);
}

// Gaussian-like falloff from boundary
fn boundary_influence(dist_diff: f32, sigma: f32) -> f32 {
    let d = dist_diff / sigma;
    return exp(-d * d);
}

// Detail fBm noise (layered on top of plate structure)
fn detail_noise(pos: vec3<f32>) -> f32 {
    var value = 0.0;
    var freq = params.frequency;
    var amp = params.amplitude * 0.12; // Detail is small relative to plate structure
    let p = pos + seed_offset(params.seed);

    for (var i = 0u; i < params.octaves; i++) {
        value += amp * snoise(p * freq);
        freq *= params.lacunarity;
        amp *= params.gain;
    }
    return value;
}

@compute @workgroup_size(16, 16)
fn main(@builtin(global_invocation_id) id: vec3<u32>) {
    let res = params.resolution;
    if (id.x >= res || id.y >= res) {
        return;
    }

    let full_res = params.full_resolution;
    let global_x = params.tile_offset_x + id.x;
    let global_y = params.tile_offset_y + id.y;
    let uv = vec2<f32>(
        f32(global_x) / f32(full_res - 1u),
        f32(global_y) / f32(full_res - 1u)
    );

    let raw_pos = cube_to_sphere(params.face, uv);

    // Physics parameters
    let gravity_factor = 9.81 / max(params.surface_gravity, 1.0);
    let age = clamp(params.surface_age, 0.0, 1.0);
    let high_freq_weight = 1.0 - age * 0.7;
    let tect = params.tectonics_factor;

    // Domain warp for complex geological shapes (coastlines, mountain curves)
    let wpos = warp_position(raw_pos);

    // === Layer 1: Plate-driven elevation — PRIMARY land/ocean geography ===
    let info = find_nearest_plates(wpos);
    let is_continental = info.nearest_type > 0.5;

    // Each plate's elevation: continental = raised, oceanic = depressed
    let elev_nearest = select(-0.30, 0.30, info.nearest_type > 0.5);
    let elev_second = select(-0.30, 0.30, info.second_type > 0.5);

    // Blend between plates at boundaries. At the edge (equidistant), use the average.
    // Far from boundary, use the nearest plate's value fully.
    // This ensures ocean-ocean boundaries stay negative, continent-continent stay positive.
    let boundary_t = smooth_step(0.0, 0.06, info.second_dist - info.nearest_dist);
    let plate_height = mix((elev_nearest + elev_second) * 0.5, elev_nearest, boundary_t);

    // === Layer 1b: Noise adds coastline shape and surface texture (secondary) ===
    let cs = params.continental_scale;
    let c1 = snoise(wpos * cs + seed_offset(params.seed + 1000u));
    let c2 = snoise(wpos * cs * 2.0 + seed_offset(params.seed + 1010u)) * 0.20;
    let c3 = snoise(wpos * cs * 4.0 + seed_offset(params.seed + 1020u)) * 0.06;
    let c4 = snoise(wpos * cs * 8.0 + seed_offset(params.seed + 1030u)) * 0.02 * high_freq_weight;
    let continental_raw = (c1 + c2 + c3 + c4) / 1.28;
    let continental = sign(continental_raw) * pow(abs(continental_raw), 0.35);
    let noise_detail = continental * params.amplitude * 0.20;

    var height = plate_height * params.amplitude + noise_detail;

    // === Layer 2: Highland/lowland variation within continents ===
    let highland = snoise(wpos * 3.0 + seed_offset(params.seed + 1100u)) * 0.14
                 + snoise(wpos * 6.0 + seed_offset(params.seed + 1110u)) * 0.06 * high_freq_weight;
    let on_land = smooth_step(-0.05, 0.05, height);
    height += highland * on_land;

    // === Layer 3: Mountain ranges — noise-positioned, NOT plate boundaries ===
    // Mountain zone noise: creates broad bands where mountains can form
    // tectonics_factor controls how mountainous the planet is
    let mz1 = snoise(wpos * 2.0 + seed_offset(params.seed + 2000u));
    let mz2 = snoise(wpos * 4.0 + seed_offset(params.seed + 2010u)) * 0.3;
    let mountain_zone = smooth_step(0.1, 0.5, (mz1 + mz2) * 0.5 + 0.5) * tect;

    // Domain-warped ridged multifractal — warp breaks grid alignment
    let mt_so = seed_offset(params.seed + 6000u);
    let mt_warp = vec3<f32>(
        snoise(raw_pos * 2.0 + vec3<f32>(53.7, 0.0, 0.0) + mt_so),
        snoise(raw_pos * 2.0 + vec3<f32>(0.0, 71.3, 0.0) + mt_so),
        snoise(raw_pos * 2.0 + vec3<f32>(0.0, 0.0, 97.1) + mt_so)
    ) * 0.15;
    let mpos = raw_pos + mt_warp;
    let ridge = ridged_multifractal(
        mpos * 5.0 + seed_offset(params.seed + 2100u),
        5, 2.2, 2.0, 1.0
    );

    // Mountains only on land (not ocean floor or thin boundary strips)
    let mountain_base = smooth_step(-0.02, 0.05, height);
    height += ridge * mountain_zone * mountain_base
        * params.mountain_scale * 0.35 * gravity_factor;

    // === Layer 4: Ocean floor variation ===
    let ocean_floor = smooth_step(0.0, -0.1, height);
    let of1 = snoise(raw_pos * 3.5 + seed_offset(params.seed + 3000u)) * 0.04;
    let of2 = snoise(raw_pos * 7.0 + seed_offset(params.seed + 3010u)) * 0.015;
    // Mid-ocean ridge: subtle elevation in deep ocean only
    let mor = smooth_step(0.3, 0.7, snoise(raw_pos * 1.8 + seed_offset(params.seed + 3100u)) * 0.5 + 0.5) * 0.03;
    height += (of1 + of2 + mor) * ocean_floor;

    // === Layer 5: Seamounts and volcanic hotspots ===
    let seamount = snoise(raw_pos * 8.0 + seed_offset(params.seed + 900u));
    let seamount_h = smooth_step(0.78, 0.95, seamount) * 0.3;
    height += seamount_h * ocean_floor;

    let hotspot_count = u32(3.0 + (1.0 - tect) * 5.0);
    let hotspot_radius = 0.02 + (1.0 - tect) * 0.04;
    let hotspot_height_val = 0.4 * gravity_factor;
    for (var h = 0u; h < hotspot_count; h++) {
        let hx = seed_offset(params.seed + 500u + h * 10u);
        let hotspot_center = normalize(hx);
        let hotspot_dist = 1.0 - dot(wpos, hotspot_center);
        if (hotspot_dist < hotspot_radius) {
            let volcano_h = hotspot_height_val * (1.0 - hotspot_dist / hotspot_radius);
            height = max(height, volcano_h);
        }
    }

    // === Layer 6: Fine detail — elevation-dependent ===
    // Elevation-dependent detail: still varies but plains aren't dead smooth
    let detail = detail_noise(raw_pos);
    let detail_weight = mix(0.5, 1.3, smooth_step(-0.02, 0.2, height));
    height += detail * detail_weight * params.detail_scale * high_freq_weight * gravity_factor;

    // === Coastline detail — domain-warped for natural shorelines ===
    let coast_warp_val = snoise(raw_pos * 1.5 + seed_offset(params.seed + 600u));
    let coast_pos = raw_pos + vec3<f32>(coast_warp_val) * 0.12;
    let coast_n = snoise(coast_pos * 2.5 + seed_offset(params.seed + 700u))
                + snoise(coast_pos * 5.0 + seed_offset(params.seed + 710u)) * 0.3 * high_freq_weight;
    let near_coast = smooth_step(0.05, 0.0, abs(height));
    height += coast_n * 0.02 * near_coast;

    // === Hypsometric shaping ===
    if (height > 0.0) {
        let h_cap = 1.5 * max(params.mountain_scale, 1.0) * gravity_factor;
        let h_norm = min(height, h_cap);
        let t = h_norm / h_cap;
        height = pow(t, 1.3) * h_cap;
    } else {
        height *= 1.2;
    }

    let idx = id.y * res + id.x;
    heightmap[idx] = height;
}
