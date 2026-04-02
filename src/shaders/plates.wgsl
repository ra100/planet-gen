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
    _pad0: u32,
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

    // --- Pass 2: Physics-based elevation from continuous noise + tectonic forces ---
    // Planet physics drive terrain character:
    //   gravity → max mountain height (Mars: low g → Olympus Mons 21km)
    //   tectonics → boundary force strength (stagnant lid → no convergent uplift)
    //   age → terrain smoothness (young=sharp, old=peneplain)

    // Gravity factor: lower gravity allows taller mountains
    // Earth g=9.81 → factor 1.0, Mars g=3.72 → factor 2.6
    let gravity_factor = 9.81 / max(params.surface_gravity, 1.0);

    // Age controls noise character: young = more high-freq detail, old = smoother
    let age = clamp(params.surface_age, 0.0, 1.0);
    let high_freq_weight = 1.0 - age * 0.7; // young=1.0, old=0.3

    // Step 1: Multi-octave global noise — gentle continent-scale terrain
    // Base noise creates broad swells (continental shelves, ocean basins)
    // Mountains come from plate boundaries, not base noise
    let n1 = snoise(raw_pos * 1.2 + seed_offset(params.seed + 1000u));       // continent-scale
    let n2 = snoise(raw_pos * 2.5 + seed_offset(params.seed + 1010u)) * 0.4; // sub-continent
    let n3 = snoise(raw_pos * 5.0 + seed_offset(params.seed + 1020u)) * 0.15 * high_freq_weight;
    let base_noise = (n1 + n2 + n3) / 1.55;
    // Moderate base amplitude — mountains will add the dramatic features
    var thickness = 20.0 + base_noise * 7.0 * gravity_factor;

    // Step 2: Tectonic boundary forces — main source of mountain ranges
    let convergence = smoothstep(0.0, -0.5, boundary_type);
    let divergence = smoothstep(0.0, 0.5, boundary_type);
    let broad_b = smoothstep(0.35, 0.0, boundary_dist); // wide mountain zone
    let tect = params.tectonics_factor;

    // Noise modulation along boundaries — creates peaks, passes, and gaps
    // Without this, uplift would be uniform along the boundary (unnatural)
    let m_n1 = snoise(raw_pos * 3.0 + seed_offset(params.seed + 3000u)) * 0.5 + 0.5; // 0..1
    let m_n2 = snoise(raw_pos * 7.0 + seed_offset(params.seed + 3010u)) * 0.25 + 0.75; // 0.5..1.0
    let uplift_vary = m_n1 * m_n2; // creates natural variation along ranges

    // Convergent uplift — THE main mountain builder
    // Earth: up to 10km uplift → through isostasy ≈ 0.25 height units
    thickness += convergence * broad_b * uplift_vary
        * params.mountain_scale * 10.0 * tect * gravity_factor;

    // Divergent: rift valleys and mid-ocean ridges
    thickness -= divergence * broad_b * 3.0 * tect;

    // Step 3: Isostatic conversion — thickness → surface elevation
    let T_ref: f32 = 20.0;
    var height = (thickness - T_ref) * 0.025;

    // Step 4: Coastline detail — reduced for old planets
    let coast_warp_val = snoise(raw_pos * 1.5 + seed_offset(params.seed + 600u));
    let coast_pos = raw_pos + vec3<f32>(coast_warp_val) * 0.12;
    let coast_n1 = snoise(coast_pos * 2.5 + seed_offset(params.seed + 700u));
    let coast_n2 = snoise(coast_pos * 5.0 + seed_offset(params.seed + 710u)) * 0.3 * high_freq_weight;
    let coastal_noise = coast_n1 + coast_n2;
    height += coastal_noise * 0.04;

    // Step 5: Additional geology
    let land_weight = smooth_step(-0.05, 0.05, height);

    // Seamounts: rare underwater volcanoes
    let seamount = snoise(raw_pos * 8.0 + seed_offset(params.seed + 900u));
    let seamount_h = smooth_step(0.78, 0.95, seamount) * 0.3;
    height += seamount_h * (1.0 - land_weight);

    // Volcanic hotspots — more numerous and larger on stagnant lid worlds (Mars, Venus)
    // Low tectonics → heat escapes through fewer, bigger plumes instead of spreading along ridges
    let hotspot_count = u32(3.0 + (1.0 - tect) * 5.0); // 3 (active tectonics) to 8 (stagnant lid)
    let hotspot_radius = 0.02 + (1.0 - tect) * 0.04;    // wider on stagnant lid
    let hotspot_height = 0.4 * gravity_factor;             // taller on low-gravity worlds
    for (var h = 0u; h < hotspot_count; h++) {
        let hx = seed_offset(params.seed + 500u + h * 10u);
        let hotspot_center = normalize(hx);
        let hotspot_dist = 1.0 - dot(sphere_pos, hotspot_center);
        if (hotspot_dist < hotspot_radius) {
            let volcano_h = hotspot_height * (1.0 - hotspot_dist / hotspot_radius);
            height = max(height, volcano_h);
        }
    }

    // R13: fBm detail noise — scaled by age (old planets smoother) and gravity
    let detail = detail_noise(raw_pos);
    let detail_mix = mix(0.1, 1.2, smooth_step(-0.1, 0.5, height));
    height += detail * detail_mix * params.detail_scale * high_freq_weight * gravity_factor;

    // Hypsometric profile: gentle shaping — preserve mountain/plain contrast
    if (height > 0.0) {
        let h_cap = 1.5 * max(params.mountain_scale, 1.0) * gravity_factor;
        let h_norm = min(height, h_cap);
        let t = h_norm / h_cap;
        // Power 1.3: mild compression, mountains stay prominent
        height = pow(t, 1.3) * h_cap;
    } else {
        height *= 1.2;
    }

    let idx = id.y * res + id.x;
    heightmap[idx] = height;
}
