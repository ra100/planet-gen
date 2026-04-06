// Emission map for export: city lights / night-side illumination.
// Outputs per-pixel urban density (0 = wilderness, 1 = dense city).
// Includes cube_sphere.wgsl and noise.wgsl at load time.

struct EmissionMapParams {
    face: u32,
    resolution: u32,
    seed: u32,
    base_temp_c: f32,
    ocean_level: f32,
    night_lights: f32,   // development level (0-1)
    axial_tilt_rad: f32,
    tile_offset_x: u32,
    tile_offset_y: u32,
    full_resolution: u32,
    _pad0: u32,
    _pad1: u32,
}

@group(0) @binding(0) var<storage, read> heightmap: array<f32>;
@group(0) @binding(1) var<storage, read_write> emission_out: array<f32>;
@group(0) @binding(2) var<uniform> params: EmissionMapParams;

fn smooth_step(edge0: f32, edge1: f32, x: f32) -> f32 {
    let t = clamp((x - edge0) / (edge1 - edge0), 0.0, 1.0);
    return t * t * (3.0 - 2.0 * t);
}

fn sample_height(gx: i32, gy: i32) -> f32 {
    let full = i32(params.full_resolution);
    let cx = clamp(gx, 0, full - 1);
    let cy = clamp(gy, 0, full - 1);
    return heightmap[u32(cy) * params.full_resolution + u32(cx)];
}

// Simplified temperature from latitude + altitude
fn compute_temp(sphere_pos: vec3<f32>, height: f32) -> f32 {
    let tilt = params.axial_tilt_rad;
    let tilted_y = sphere_pos.y * cos(tilt) + sphere_pos.z * sin(tilt);
    let effective_lat = asin(clamp(tilted_y, -1.0, 1.0));
    let lat_deg = abs(effective_lat) * 180.0 / 3.14159;
    let lat_norm = lat_deg / 90.0;
    let temp_drop = 50.0 * (0.4 * lat_norm + 0.6 * lat_norm * lat_norm);
    let base_temp = 30.0 - temp_drop + (params.base_temp_c - 15.0);
    let land_frac = max(height - params.ocean_level, 0.0) / max(1.0 - params.ocean_level, 0.01);
    let lapse = -6.5 * land_frac * 5.0;
    return base_temp + lapse;
}

@compute @workgroup_size(16, 16)
fn main(@builtin(global_invocation_id) id: vec3<u32>) {
    let res = params.resolution;
    if (id.x >= res || id.y >= res) { return; }

    let full_res = params.full_resolution;
    let global_x = params.tile_offset_x + id.x;
    let global_y = params.tile_offset_y + id.y;
    let height_idx = global_y * full_res + global_x;
    let height = heightmap[height_idx];

    let dev = params.night_lights;
    if (dev <= 0.0 || height <= params.ocean_level) {
        emission_out[id.y * res + id.x] = 0.0;
        return;
    }

    let uv = vec2<f32>(
        f32(global_x) / f32(full_res - 1u),
        f32(global_y) / f32(full_res - 1u)
    );
    let sphere_pos = cube_to_sphere(params.face, uv);
    let temp = compute_temp(sphere_pos, height);
    let land_h = (height - params.ocean_level) / max(1.0 - params.ocean_level, 0.01);

    // Habitability score
    var score = 0.0;
    score += smooth_step(8.0, 18.0, temp) * smooth_step(35.0, 22.0, temp) * 0.4;
    score += (1.0 - smooth_step(0.0, 0.25, land_h)) * 0.2;

    // Coastal proximity check
    let gx = i32(global_x);
    let gy = i32(global_y);
    let stp = i32(max(full_res / 100u, 3u));
    var ocean_near = 0.0;
    if (sample_height(gx + stp, gy) < params.ocean_level) { ocean_near += 1.0; }
    if (sample_height(gx - stp, gy) < params.ocean_level) { ocean_near += 1.0; }
    if (sample_height(gx, gy + stp) < params.ocean_level) { ocean_near += 1.0; }
    if (sample_height(gx, gy - stp) < params.ocean_level) { ocean_near += 1.0; }
    score += min(ocean_near / 2.0, 1.0) * 0.25;
    score *= smooth_step(3.0, 10.0, temp);

    // City pattern: dot + web network
    let dots = snoise(sphere_pos * 120.0) * 0.5 + 0.5;
    let dots2 = snoise(sphere_pos * 250.0 + vec3<f32>(7.3, 2.1, 5.9)) * 0.5 + 0.5;
    let web1 = 1.0 - abs(snoise(sphere_pos * 60.0 + vec3<f32>(3.1, 8.7, 1.3))) * 2.0;
    let web2 = 1.0 - abs(snoise(sphere_pos * 130.0 + vec3<f32>(11.3, 4.7, 7.1))) * 2.0;
    let webs = max(max(web1, 0.0), max(web2, 0.0));
    let city_pattern = max(dots * dots2 * 1.5, webs * 0.6);

    let dev_scaled = dev * dev * dev;
    let urban_raw = score * city_pattern;
    let threshold = (1.0 - dev_scaled) * 0.45;
    let urban = smooth_step(threshold, threshold + 0.04, urban_raw);

    emission_out[id.y * res + id.x] = urban;
}
