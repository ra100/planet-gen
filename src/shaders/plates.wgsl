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

// Domain warping for natural plate boundaries — multi-octave for fractal coastlines
fn warp_position(pos: vec3<f32>) -> vec3<f32> {
    // Two octaves of warping: large-scale bends + fine-scale irregularity
    let warp1 = vec3<f32>(
        snoise(pos * 1.8 + vec3<f32>(31.7, 0.0, 0.0)),
        snoise(pos * 1.8 + vec3<f32>(0.0, 47.3, 0.0)),
        snoise(pos * 1.8 + vec3<f32>(0.0, 0.0, 73.1))
    ) * 0.15; // Large-scale curves
    let warp2 = vec3<f32>(
        snoise(pos * 5.0 + vec3<f32>(13.1, 0.0, 0.0)),
        snoise(pos * 5.0 + vec3<f32>(0.0, 19.7, 0.0)),
        snoise(pos * 5.0 + vec3<f32>(0.0, 0.0, 29.3))
    ) * 0.06; // Fine-scale irregularity
    return normalize(pos + warp1 + warp2);
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
        let plate_offset = vec3<f32>(f32(i) * 17.3, f32(i) * 31.7, f32(i) * 43.1);
        let bias = snoise(sphere_pos * 4.0 + plate_offset) * 0.04
                 + snoise(sphere_pos * 8.0 + plate_offset * 2.0) * 0.02;
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

    // --- Pass 2: Generate height ---
    // Architecture: plate type BIASES the elevation but doesn't determine it.
    // A noise-based continental mask creates the actual coastlines — continental
    // plates are mostly land but can have bays/gulfs/inland seas. Oceanic plates
    // are mostly ocean but can have island chains.

    // Step 1: Plate type bias (continental plates tend to be higher)
    let plate_bias: f32 = select(-0.25, 0.25, is_continental);

    // Step 2: Domain-warped continental noise creates the actual coastline shapes
    // This is INDEPENDENT of Voronoi cell boundaries — creates bays, gulfs,
    // peninsulas, and inland seas that break the convex cell shape.
    let coast_warp_val = snoise(raw_pos * 1.5 + seed_offset(params.seed + 600u));
    let coast_pos = raw_pos + vec3<f32>(coast_warp_val) * 0.15;

    // Two octaves for coastline shape (keep noise calls low to avoid GPU limits)
    let coast_n1 = snoise(coast_pos * 2.5 + seed_offset(params.seed + 700u));
    let coast_n2 = snoise(coast_pos * 5.0 + seed_offset(params.seed + 710u)) * 0.4;
    let coastal_noise = coast_n1 + coast_n2;

    // Combine plate bias + coastal noise for elevation base
    // Continental plates: bias +0.25 + noise → mostly positive (land)
    // Oceanic plates: bias -0.25 + noise → mostly negative (ocean)
    // The noise creates crossovers: bays on continental, islands on oceanic
    var height = plate_bias + coastal_noise * 0.35;

    // Step 3: Regional geology within the established land/ocean pattern
    let interior_factor = clamp(boundary_dist * 8.0, 0.0, 1.0);
    let is_land = height > 0.0;

    if (is_land) {
        // Continental highlands and basins (single warped noise)
        let highland = snoise(raw_pos * 4.0 + seed_offset(params.seed + 800u));
        height += highland * 0.12 * interior_factor;
        if (highland > 0.5) { height += 0.08; }
    } else {
        // Seamounts: rare underwater volcanoes that can breach surface
        let seamount = snoise(raw_pos * 8.0 + seed_offset(params.seed + 900u));
        if (seamount > 0.75) {
            height += (seamount - 0.75) * 1.0;
        }
    }

    // Continental shelf: smooth transition near the coastline
    let coast_dist = abs(height);
    if (coast_dist < 0.08) {
        // Near sea level: gentle shelf, less steep transition
        let shelf_smooth = coast_dist / 0.08;
        height *= 0.5 + 0.5 * shelf_smooth;
    }

    // --- Boundary terrain (R4-R7, R10) ---
    let b_influence = boundary_influence(boundary_dist, 0.12); // Wider influence for gradual terrain

    if (boundary_type < -0.3) {
        // CONVERGENT boundary
        if (is_continental && neighbor_continental) {
            // R4: Continental-continental collision → mountain range (Himalayas)
            let ridge_height = 0.6 * b_influence * abs(boundary_type);
            let ridge_noise = snoise(raw_pos * 15.0 + seed_offset(params.seed + 100u)) * 0.1;
            height += ridge_height + ridge_noise * b_influence;
        } else if (is_continental && !neighbor_continental) {
            // R5: Oceanic-continental convergence → volcanic chain (Andes)
            let volcanic_height = 0.45 * b_influence * abs(boundary_type);
            let volcanic_noise = snoise(raw_pos * 12.0 + seed_offset(params.seed + 200u)) * 0.12;
            height += volcanic_height + volcanic_noise * b_influence;
        } else if (!is_continental && neighbor_continental) {
            // R5: Ocean trench on the oceanic side of subduction
            height -= 0.25 * b_influence * abs(boundary_type);
        } else {
            // R10: Oceanic-oceanic convergence → island arc
            let arc_height = 0.35 * b_influence * abs(boundary_type);
            // Create arc-shaped elevation (islands above sea level)
            let arc_noise = snoise(raw_pos * 10.0 + seed_offset(params.seed + 300u));
            if (arc_noise > 0.0) {
                height += arc_height * arc_noise;
            }
        }
    } else if (boundary_type > 0.3) {
        // R6: DIVERGENT boundary
        if (is_continental) {
            // Rift valley on land (East Africa)
            height -= 0.15 * b_influence * boundary_type;
        } else {
            // Mid-ocean ridge (underwater)
            height += 0.15 * b_influence * boundary_type;
        }
    } else {
        // R7: TRANSFORM boundary — subtle offset
        let offset_noise = snoise(raw_pos * 8.0 + seed_offset(params.seed + 400u));
        height += offset_noise * 0.05 * b_influence;
    }

    // Continental margin transition (R9)
    // Smooth slope from continental shelf to ocean basin
    if (is_continental) {
        let margin_width = 0.04;
        if (boundary_dist < margin_width && !neighbor_continental) {
            let margin_t = boundary_dist / margin_width;
            // Passive margin (smooth) vs active margin (steep) based on boundary type
            if (boundary_type < -0.2) {
                // Active margin (steep, convergent)
                height = mix(-0.2, height, margin_t * margin_t);
            } else {
                // Passive margin (gentle slope)
                height = mix(-0.1, height, margin_t);
            }
        }
    }

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

    // R13: fBm detail noise on top of geological structure
    // Elevation-dependent: mountains get more detail, plains less
    let detail = detail_noise(raw_pos);
    var detail_scale: f32;
    if (height > 0.3) {
        detail_scale = 1.5; // Mountains: craggy
    } else if (height > 0.0) {
        detail_scale = 0.8; // Land: moderate
    } else {
        detail_scale = 0.3; // Ocean: smooth
    }
    height += detail * detail_scale;

    let idx = id.y * res + id.x;
    heightmap[idx] = height;
}
