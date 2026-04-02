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
    _pad0: u32,
    _pad1: u32,
    _pad2: u32,
    _pad3: u32,
}

@group(0) @binding(0) var<storage, read> plates: array<Plate>;
@group(0) @binding(1) var<uniform> params: GenParams;
@group(0) @binding(2) var<storage, read_write> heightmap: array<f32>;

// Hash seed to offset for fBm detail
fn seed_offset(s: u32) -> vec3<f32> {
    let phi = 1.618033988;
    let x = fract(f32(s) * phi) * 97.0;
    let y = fract(f32(s) * phi * phi) * 89.0;
    let z = fract(f32(s) * phi * phi * phi) * 83.0;
    return vec3<f32>(x, y, z);
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

// Domain warping for natural plate boundaries — multi-octave for fractal coastlines
fn warp_position(pos: vec3<f32>) -> vec3<f32> {
    // Three octaves of warping: large sweeps + medium bends + fine irregularity
    let warp1 = vec3<f32>(
        snoise(pos * 1.2 + vec3<f32>(31.7, 0.0, 0.0)),
        snoise(pos * 1.2 + vec3<f32>(0.0, 47.3, 0.0)),
        snoise(pos * 1.2 + vec3<f32>(0.0, 0.0, 73.1))
    ) * 0.20; // Large-scale sweeps
    let warp2 = vec3<f32>(
        snoise(pos * 3.0 + vec3<f32>(13.1, 0.0, 0.0)),
        snoise(pos * 3.0 + vec3<f32>(0.0, 19.7, 0.0)),
        snoise(pos * 3.0 + vec3<f32>(0.0, 0.0, 29.3))
    ) * 0.10; // Medium-scale bends
    let warp3 = vec3<f32>(
        snoise(pos * 7.0 + vec3<f32>(7.3, 0.0, 0.0)),
        snoise(pos * 7.0 + vec3<f32>(0.0, 11.9, 0.0)),
        snoise(pos * 7.0 + vec3<f32>(0.0, 0.0, 17.1))
    ) * 0.04; // Fine-scale irregularity
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
    let sphere_pos = warp_position(raw_pos); // Domain warping for natural boundaries

    // --- Pass 1: Find plates and boundary info ---
    let info = find_nearest_plates(sphere_pos);

    let boundary_dist = info.second_dist - info.nearest_dist; // Small = near boundary
    let boundary_type = classify_boundary(sphere_pos, info.nearest_idx, info.second_idx);

    let is_continental = info.nearest_type > 0.5;
    let neighbor_continental = info.second_type > 0.5;

    // --- Pass 2: Generate height via crustal thickness + isostasy ---
    // Each plate carries a crustal thickness field that varies via per-plate noise.
    // Isostasy converts thickness to surface elevation. This produces a natural
    // bimodal height distribution where water level changes reveal terrain
    // gradually — no puzzle pieces.

    // Step 1: Base crustal thickness per plate type (conceptual km)
    // Global low-frequency noise offsets the base thickness smoothly across the globe.
    // This prevents puzzle-piece behavior (different areas at different heights)
    // without creating discrete per-plate jumps that cause enclave artifacts.
    let base_offset = snoise(raw_pos * 1.5 + seed_offset(params.seed + 2000u));
    let base_thickness: f32 = select(7.0, 35.0, is_continental)
        + base_offset * select(0.5, 4.0, is_continental);
    let neighbor_base: f32 = select(7.0, 35.0, neighbor_continental)
        + base_offset * select(0.5, 4.0, neighbor_continental);

    // Step 2: Gentle intra-plate thickness variation (plains, low hills, basins)
    // Low frequencies + low amplitude = broad smooth terrain, no speckle
    let t_n1 = snoise(raw_pos * 2.0 + seed_offset(params.seed + 1000u));
    let t_n2 = snoise(raw_pos * 4.0 + seed_offset(params.seed + 1010u)) * 0.4;
    let thickness_noise = (t_n1 + t_n2) / 1.4; // ~-1..+1

    // Step 3: Tectonic boundary forces
    let convergence = smoothstep(0.0, -0.5, boundary_type);
    let divergence = smoothstep(0.0, 0.5, boundary_type);
    let broad_b = smoothstep(0.25, 0.0, boundary_dist);
    let b_influence = boundary_influence(boundary_dist, params.boundary_width);

    // Gentle base variation (continental plains ±2km, oceanic abyssal plains ±0.3km)
    let base_amp: f32 = select(0.3, 2.0, is_continental);
    let neighbor_base_amp: f32 = select(0.3, 2.0, neighbor_continental);

    // Mountain ranges: ridged multifractal concentrated at convergent boundaries
    // Creates linear ridge features (not uniform noise) where plates collide
    let mountain_zone = convergence * broad_b;
    let ridge = ridged_multifractal(
        raw_pos * 3.5 + seed_offset(params.seed + 2000u), 5, 2.2, 2.0, 1.0
    );
    let mountain_add: f32 = ridge * mountain_zone * params.mountain_scale
        * select(4.0, 14.0, is_continental);
    let neighbor_mountain: f32 = ridge * mountain_zone * params.mountain_scale
        * select(4.0, 14.0, neighbor_continental);

    // Rift thinning at divergent boundaries
    let rift_thin = divergence * broad_b;

    let own_thickness = base_thickness + thickness_noise * base_amp
        + mountain_add - rift_thin * 2.0;
    let neighbor_thickness = neighbor_base + thickness_noise * neighbor_base_amp
        + neighbor_mountain - rift_thin * 2.0;

    // Step 4: Margin blending — smooth transition at boundaries
    let margin_blend = smoothstep(0.0, 0.10, boundary_dist);
    let boundary_mid = (own_thickness + neighbor_thickness) * 0.5;
    var thickness = mix(boundary_mid, own_thickness, margin_blend);

    // Narrow ocean features (trenches, ridges, island arcs)
    if (convergence > 0.01) {
        if (!is_continental && neighbor_continental) {
            thickness -= 3.0 * b_influence * convergence;
        } else if (!is_continental && !neighbor_continental) {
            let arc_noise = snoise(raw_pos * 10.0 + seed_offset(params.seed + 300u));
            if (arc_noise > 0.0) {
                thickness += 6.0 * params.mountain_scale * b_influence * convergence * arc_noise;
            }
        }
    }
    if (divergence > 0.01 && !is_continental) {
        thickness += 2.0 * b_influence * divergence;
    }

    // Step 5: Isostatic conversion — thickness → surface elevation
    let T_ref: f32 = 18.0;
    var height = (thickness - T_ref) * 0.025;

    // Step 6: Coastline detail — gentle irregularity at continental margins
    let coast_warp_val = snoise(raw_pos * 1.5 + seed_offset(params.seed + 600u));
    let coast_pos = raw_pos + vec3<f32>(coast_warp_val) * 0.12;
    let coast_n1 = snoise(coast_pos * 2.5 + seed_offset(params.seed + 700u));
    let coast_n2 = snoise(coast_pos * 5.0 + seed_offset(params.seed + 710u)) * 0.3;
    let coastal_noise = coast_n1 + coast_n2;
    height += coastal_noise * 0.04;

    // Step 7: Additional geology
    let land_weight = smooth_step(-0.05, 0.05, height);

    // Seamounts: rare underwater volcanoes
    let seamount = snoise(raw_pos * 8.0 + seed_offset(params.seed + 900u));
    let seamount_h = smooth_step(0.78, 0.95, seamount) * 0.3;
    height += seamount_h * (1.0 - land_weight);

    // R11: Volcanic hotspots (1-3 independent of plates)
    let hotspot_count = 2u;
    for (var h = 0u; h < hotspot_count; h++) {
        let hx = seed_offset(params.seed + 500u + h * 10u);
        let hotspot_center = normalize(hx);
        let hotspot_dist = 1.0 - dot(sphere_pos, hotspot_center);
        if (hotspot_dist < 0.02) {
            // Shield volcano profile
            let volcano_h = 0.4 * (1.0 - hotspot_dist / 0.02);
            height = max(height, volcano_h);
        }
    }

    // R13: fBm detail noise — subtle texture on top of geological structure
    let detail = detail_noise(raw_pos);
    let detail_mix = mix(0.1, 1.2, smooth_step(-0.1, 0.5, height));
    height += detail * detail_mix * params.detail_scale;

    // Hypsometric profile: flatten plains, amplify mountains, deepen oceans
    if (height > 0.0) {
        let h_cap = 1.5 * max(params.mountain_scale, 1.0);
        let h_norm = min(height, h_cap);
        let t = h_norm / h_cap;
        // Power 1.8: strong flattening of low areas (plains), sharp mountain peaks
        height = pow(t, 1.8) * h_cap * 1.5;
    } else {
        height *= 1.3;
    }

    let idx = id.y * res + id.x;
    heightmap[idx] = height;
}
