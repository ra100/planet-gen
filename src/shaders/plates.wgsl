// Tectonic plate compute shader.
// Pass 1: Assign each pixel to nearest plate, compute boundary info.
// Pass 2: Generate height from plate type + boundary type + detail noise.
// Includes noise.wgsl and cube_sphere.wgsl (concatenated at load time).

struct Plate {
    center: vec3<f32>,
    plate_type: f32, // 1.0 = continental, 0.0 = oceanic
    velocity: vec3<f32>, // Euler angular velocity (ω)
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

// Hash to [0, 1] — for per-plate character parameters
fn hash_f32_01(input: u32) -> f32 {
    return f32(pcg_hash(input) & 0xFFFFu) / 65535.0;
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
    elevation: f32,
    character: vec3<f32>,
}

// Plate history (tectonic_history.rs) appends records after the real plates:
// a meta record (seed and feature counts as u32 bits), one record per seed
// (position, owning plate) and two per feature. A plate with several seeds
// stores seed_start | seed_count << 16 in _pad; _pad == 0 means the plate is
// its center alone, which is every plate when no history was simulated.
fn has_history() -> bool {
    return arrayLength(&plates) > params.num_plates;
}

fn history_seed_total() -> u32 {
    return bitcast<u32>(plates[params.num_plates].center.x);
}

fn history_seed(index: u32) -> vec3<f32> {
    return plates[params.num_plates + 1u + index].center;
}

// 1 - cos(angle) to the plate's nearest seed.
fn plate_seed_gap(pos: vec3<f32>, index: u32) -> f32 {
    let packed = bitcast<u32>(plates[index]._pad);
    if (packed == 0u) {
        return 1.0 - dot(pos, plates[index].center);
    }
    let start = packed & 0xffffu;
    let count = packed >> 16u;
    var nearest = -2.0;
    for (var k = 0u; k < count; k++) {
        nearest = max(nearest, dot(pos, history_seed(start + k)));
    }
    return 1.0 - nearest;
}

// The plate's seed nearest to pos (its center when it has a single seed).
fn plate_anchor(pos: vec3<f32>, index: u32) -> vec3<f32> {
    let packed = bitcast<u32>(plates[index]._pad);
    if (packed == 0u) {
        return plates[index].center;
    }
    let start = packed & 0xffffu;
    let count = packed >> 16u;
    var anchor = history_seed(start);
    var nearest = dot(pos, anchor);
    for (var k = 1u; k < count; k++) {
        let seed = history_seed(start + k);
        let d = dot(pos, seed);
        if (d > nearest) {
            nearest = d;
            anchor = seed;
        }
    }
    return anchor;
}

fn plate_distance(pos: vec3<f32>, index: u32) -> f32 {
    let offset = vec3<f32>(f32(index) * 17.3, f32(index) * 31.7, f32(index) * 43.1);
    return plate_seed_gap(pos, index)
        + snoise(pos * 2.0 + offset) * 0.08
        + snoise(pos * 4.5 + offset * 2.0) * 0.04;
}

// Angle from p to the great-circle segment a-b.
fn segment_angle(p: vec3<f32>, a: vec3<f32>, b: vec3<f32>) -> f32 {
    let n_raw = cross(a, b);
    let n_len = length(n_raw);
    if (n_len < 0.000001) {
        return acos(clamp(dot(p, a), -1.0, 1.0));
    }
    let n = n_raw / n_len;
    let off = dot(p, n);
    let q = p - n * off;
    if (dot(cross(a, q), n) >= 0.0 && dot(cross(q, b), n) >= 0.0) {
        return asin(clamp(abs(off), 0.0, 1.0));
    }
    return acos(clamp(max(dot(p, a), dot(p, b)), -1.0, 1.0));
}

// Relief left by the plate history: eroded sutures where continents
// collided, volcanic arcs over old subduction zones, and rift troughs where
// continents split. Amplitude already includes erosion with age.
fn history_relief(pos: vec3<f32>, ridge: f32) -> f32 {
    let base = params.num_plates + 1u + history_seed_total();
    let features = bitcast<u32>(plates[params.num_plates].center.y);
    var relief = 0.0;
    for (var f = 0u; f < features; f++) {
        let shape = plates[base + 2u * f];
        let extent = plates[base + 2u * f + 1u];
        let d = segment_angle(pos, shape.center, shape.velocity) / max(extent.center.x, 0.0001);
        if (d > 3.0) { continue; }
        let belt = exp(-d * d) * shape._pad;
        let kind = u32(shape.plate_type + 0.5);
        if (kind == 0u) {
            relief += belt * (0.06 + 0.10 * ridge);
        } else if (kind == 1u) {
            relief += belt * (0.06 + 0.22 * ridge);
        } else {
            relief -= belt * 0.05;
        }
    }
    return relief;
}

fn find_nearest_plates(sphere_pos: vec3<f32>) -> PlateInfo {
    var info: PlateInfo;
    info.nearest_dist = 100.0;
    info.second_dist = 100.0;
    info.nearest_idx = 0u;
    info.second_idx = 0u;
    info.elevation = 0.0;
    info.character = vec3<f32>(0.0);
    var weight_sum = 0.0;

    for (var i = 0u; i < params.num_plates; i++) {
        let d = plate_distance(sphere_pos, i);
        // An online soft partition stays continuous when either of the two
        // nearest plate indices changes, including three-plate junctions.
        let minimum = min(d, info.nearest_dist);
        let rescale = exp((minimum - info.nearest_dist) / 0.012);
        let weight = exp((minimum - d) / 0.012);
        weight_sum = weight_sum * rescale + weight;
        info.elevation = info.elevation * rescale
            + weight * select(-0.30, 0.30, plates[i].plate_type > 0.5);
        let character = vec3<f32>(
            hash_f32_01(params.seed + i * 7u + 100u),
            hash_f32_01(params.seed + i * 7u + 101u),
            hash_f32_01(params.seed + i * 7u + 102u),
        );
        info.character = info.character * rescale + weight * character;

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
    info.elevation /= max(weight_sum, 0.000001);
    info.character /= max(weight_sum, 0.000001);
    return info;
}

// Classify boundary type from relative plate motion
// Returns: -1 = convergent, 0 = transform, 1 = divergent
fn classify_boundary(sphere_pos: vec3<f32>, plate_a: u32, plate_b: u32) -> f32 {
    let velocity_a = cross(plates[plate_a].velocity, sphere_pos);
    let velocity_b = cross(plates[plate_b].velocity, sphere_pos);
    let rel_velocity = velocity_a - velocity_b;
    // Boundary normal: direction from one plate center to the other
    let boundary_normal = normalize(plate_anchor(sphere_pos, plate_b) - plate_anchor(sphere_pos, plate_a));
    let approach_speed = dot(rel_velocity, boundary_normal);
    return -clamp(approach_speed * 5.0, -1.0, 1.0);
}

// Gaussian-like falloff from boundary
fn boundary_influence(dist_diff: f32, sigma: f32) -> f32 {
    let d = dist_diff / max(sigma, 0.0001);
    return exp(-d * d);
}

fn boundary_plate_weight(distance: f32, minimum: f32) -> f32 {
    let sigma = max(params.boundary_width, 0.0001);
    let gap = max(distance - minimum, 0.0);
    return boundary_influence(gap, sigma)
        * (1.0 - smooth_step(2.0 * sigma, 3.0 * sigma, gap));
}

fn boundary_relief(pos: vec3<f32>, minimum: f32, ridge: f32) -> f32 {
    var weight_sum = 0.0;
    var relief = 0.0;
    for (var i = 0u; i < params.num_plates; i++) {
        let di = plate_distance(pos, i);
        let wi = boundary_plate_weight(di, minimum);
        weight_sum += wi;
        if (wi == 0.0) { continue; }
        for (var j = i + 1u; j < params.num_plates; j++) {
            let dj = plate_distance(pos, j);
            let wj = boundary_plate_weight(dj, minimum);
            if (wj == 0.0) { continue; }
            let stress = classify_boundary(pos, i, j);
            let convergence = max(-stress, 0.0);
            let divergence = max(stress, 0.0);
            let ti = plates[i].plate_type;
            let tj = plates[j].plate_type;
            let continental_pair = (ti + tj) * 0.5;
            let rel_v = cross(plates[i].velocity - plates[j].velocity, pos);
            let shear = max(length(rel_v) * 2.0 - abs(stress), 0.0);
            let collision = convergence * mix(0.10, 0.48, continental_pair) * (0.35 + ridge);
            let spreading = divergence * mix(0.14, -0.12, continental_pair);
            let ocean_side = 1.0 - (wi * ti + wj * tj) / (wi + wj);
            let trench = convergence * (1.0 - abs(ti + tj - 1.0)) * ocean_side * 0.12
                * smooth_step(0.0, 0.025, abs(di - dj));
            relief += wi * wj * (collision + spreading - trench
                + min(shear, 1.0) * (ridge - 0.4) * 0.05);
        }
    }
    // Two equally weighted plates reproduce the pair's full boundary relief.
    // Other pair contributions vanish smoothly as they leave the local belt.
    return 4.0 * relief / max(weight_sum * weight_sum, 0.000001);
}

// Detail fBm noise (layered on top of plate structure)
fn detail_noise(pos: vec3<f32>) -> f32 {
    var value = 0.0;
    var freq = params.frequency;
    var amp = params.amplitude * 0.12; // Detail is small relative to plate structure
    let p = pos + seed_offset(params.seed);

    for (var i = 0u; i < params.octaves; i++) {
        // Suppress unresolved octaves instead of baking aliases into the
        // height cubemap. Full resolution keeps tiled exports consistent.
        let octave_weight = 1.0 - smooth_step(0.25, 0.75, freq * 2.0 / f32(params.full_resolution));
        value += amp * snoise(p * freq) * octave_weight;
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

    let plate_height = info.elevation;

    // === Layer 1b: Noise adds coastline shape and surface texture (secondary) ===
    let cs = params.continental_scale;
    let c1 = snoise(wpos * cs + seed_offset(params.seed + 1000u));
    let c2 = snoise(wpos * cs * 2.0 + seed_offset(params.seed + 1010u)) * 0.20;
    let c3 = snoise(wpos * cs * 4.0 + seed_offset(params.seed + 1020u)) * 0.06;
    let c4 = snoise(wpos * cs * 8.0 + seed_offset(params.seed + 1030u)) * 0.02 * high_freq_weight;
    let continental_raw = (c1 + c2 + c3 + c4) / 1.28;
    let continental = continental_elevation(continental_raw);
    let noise_detail = continental * params.amplitude * 0.20;

    var height = plate_height * params.amplitude + noise_detail;

    let plate_char = info.character;

    // === Layer 2: Highland/lowland with valleys and foothills ===
    let on_land = smooth_step(-0.05, 0.05, height);

    // Broad highland zones — per-plate highland coverage varies
    let highland_scale = 0.10 + plate_char.z * 0.10; // 0.10-0.20 amplitude
    let hl1 = snoise(wpos * 2.5 + seed_offset(params.seed + 1100u)) * highland_scale;
    let hl2 = snoise(wpos * 5.0 + seed_offset(params.seed + 1110u)) * 0.05 * high_freq_weight;
    height += (hl1 + hl2) * on_land;

    // Valleys: negative noise carved between highlands
    let valley_noise = snoise(wpos * 3.5 + seed_offset(params.seed + 1200u));
    let valley_depth = smooth_step(0.3, 0.6, -valley_noise) * 0.08; // carve where noise is negative
    height -= valley_depth * on_land;

    // Foothills: gradual transition from plains to mountain zones
    let foothill_noise = snoise(wpos * 4.0 + seed_offset(params.seed + 1300u));
    let foothill_zone = smooth_step(0.0, 0.15, height) * smooth_step(0.30, 0.10, height);
    height += abs(foothill_noise) * 0.04 * foothill_zone;

    // === Layer 3: Mountain ranges with per-plate character ===
    let mz1 = snoise(wpos * 2.0 + seed_offset(params.seed + 2000u));
    let mz2 = snoise(wpos * 4.0 + seed_offset(params.seed + 2010u)) * 0.3;
    // Subdued inherited intraplate ranges; active belts are added below.
    let mountain_zone = smooth_step(0.1, 0.5, (mz1 + mz2) * 0.5 + 0.5) * tect * 0.45;

    // Domain-warped ridged multifractal — per-plate warp strength varies
    let mt_so = seed_offset(params.seed + 6000u);
    let warp_str = 0.10 + plate_char.x * 0.10; // rugged plates get more warp
    let mt_warp = vec3<f32>(
        snoise(raw_pos * 2.0 + vec3<f32>(53.7, 0.0, 0.0) + mt_so),
        snoise(raw_pos * 2.0 + vec3<f32>(0.0, 71.3, 0.0) + mt_so),
        snoise(raw_pos * 2.0 + vec3<f32>(0.0, 0.0, 97.1) + mt_so)
    ) * warp_str;
    let mpos = raw_pos + mt_warp;

    // Per-plate mountain style: vary octaves and lacunarity
    let ridge_lacunarity = 2.0 + plate_char.x * 0.4; // 2.0-2.4
    let ridge = ridged_multifractal(
        mpos * 5.0 + seed_offset(params.seed + 2100u),
        5, ridge_lacunarity, 2.0, 1.0
    );

    // Plateau shaping: some plates get flat-topped mountains
    let plateau_factor = plate_char.y;
    let ridge_shaped = mix(ridge, smooth_step(0.2, 0.5, ridge), plateau_factor * 0.7);

    let mountain_base = smooth_step(-0.02, 0.05, height);
    height += ridge_shaped * mountain_zone * mountain_base
        * params.mountain_scale * 0.35 * gravity_factor;

    // Active velocity-driven boundary relief. Relative Euler motion projected
    // onto the boundary normal separates compression, opening, and shear.
    // Symmetric plate-pair quantities keep the field continuous across the edge.
    let belt = boundary_influence(info.second_dist - info.nearest_dist, params.boundary_width);
    let boundary_ridges = ridged_multifractal(wpos * 18.0 + seed_offset(params.seed + 2200u), 4, 2.1, 2.0, 1.0);
    height += belt * tect * params.mountain_scale * gravity_factor
        * boundary_relief(wpos, info.nearest_dist, boundary_ridges);
    if (has_history()) {
        height += history_relief(wpos, ridge) * params.mountain_scale * gravity_factor;
    }

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
        // Uniform solid angle: azimuth is uniform and polar cosine is uniform.
        let hs = params.seed + 500u + h * 10u;
        let hy = hash_f32_01(hs) * 2.0 - 1.0;
        let azimuth = hash_f32_01(hs + 1u) * 6.2831853;
        let radial = sqrt(max(1.0 - hy * hy, 0.0));
        let hotspot_center = vec3<f32>(radial * cos(azimuth), hy, radial * sin(azimuth));
        let hotspot_dist = 1.0 - dot(wpos, hotspot_center);
        height = hotspot_elevation(height, hotspot_dist, hotspot_radius, hotspot_height_val);
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
