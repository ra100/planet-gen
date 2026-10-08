// Albedo map generation from heightmap + climate model.
// Shares surface_material.wgsl with the preview; neighborhoods use the tile halo.

struct AlbedoParams {
    face: u32,
    resolution: u32,
    seed: u32,
    base_temp_c: f32,
    ocean_level: f32,
    ocean_fraction: f32,
    axial_tilt_rad: f32,
    season: f32,
    tile_offset_x: u32,
    tile_offset_y: u32,
    full_resolution: u32,
    local_height: u32,
}

@group(0) @binding(0) var<storage, read> heightmap: array<f32>;
@group(0) @binding(1) var<storage, read_write> albedo: array<vec4<f32>>;
@group(0) @binding(2) var<uniform> params: AlbedoParams;

// ---- Temperature (matches preview shader) ----
fn compute_temperature(sphere_pos: vec3<f32>, height: f32) -> f32 {
    return climate_temperature(sphere_pos, height, params.ocean_level, params.base_temp_c, params.axial_tilt_rad, params.season);
}

// ---- Hadley cell moisture ----
fn hadley_cell_moisture(lat_rad: f32) -> f32 {
    let lat = abs(lat_rad) * 180.0 / 3.14159;
    let tropical = 200.0 * exp(-lat * lat / 200.0);
    let dry = -80.0 * exp(-(lat - 30.0) * (lat - 30.0) / 60.0);
    let temperate = 90.0 * exp(-(lat - 45.0) * (lat - 45.0) / 200.0);
    return max(10.0, tropical + dry + temperate + 90.0 - 60.0 * smooth_step(65.0, 85.0, lat));
}

fn compute_moisture(sphere_pos: vec3<f32>, height: f32) -> f32 {
    let tilt = params.axial_tilt_rad;
    let tilted_y = sphere_pos.y * cos(tilt) + sphere_pos.z * sin(tilt);
    let effective_lat = asin(clamp(tilted_y, -1.0, 1.0));

    // Shift Hadley cells with thermal equator (matches preview shader)
    let thermal_lat = effective_lat;

    let ocean_scale = 0.25 + 0.75 * params.ocean_fraction;
    let hadley_base = hadley_cell_moisture(thermal_lat) * ocean_scale;

    let noise1 = snoise(sphere_pos * 3.0 + noise_seed_offset(params.seed, 73u));
    let local_var = noise1 * 0.5;
    var moisture = hadley_base * (0.55 + 0.45 * (local_var + 0.5));
    moisture += 50.0 * (local_var + 0.5) * ocean_scale;

    // Simplified continentality (no cubemap sampling - use local height proxy)
    let is_land = height > params.ocean_level;
    if (is_land) {
        let land_height = (height - params.ocean_level) / max(1.0 - params.ocean_level, 0.01);
        moisture *= 0.6 + 0.4 * (1.0 - clamp(land_height, 0.0, 1.0));
    }

    // Tile-local export has no wind cubemap; elevation limits inland moisture.
    if (is_land) {
        let land_height = (height - params.ocean_level) / max(1.0 - params.ocean_level, 0.01);
        if (land_height > 0.3) {
            let shadow = clamp((land_height - 0.3) / 0.4, 0.0, 1.0);
            moisture *= 1.0 - shadow * 0.4;
        }
    }

    moisture *= 1.0 + snoise(sphere_pos * 0.7 + noise_seed_offset(params.seed, 74u)) * 0.25;
    return clamp(moisture * (0.5 + params.ocean_fraction), 0.0, 400.0);
}

fn smooth_step(edge0: f32, edge1: f32, x: f32) -> f32 {
    let t = clamp((x - edge0) / (edge1 - edge0), 0.0, 1.0);
    return t * t * (3.0 - 2.0 * t);
}

fn albedo_height(x: i32, y: i32) -> f32 {
    let px = u32(clamp(x, 0, i32(params.resolution) - 1));
    let py = u32(clamp(y, 0, i32(params.local_height) - 1));
    return heightmap[py * params.resolution + px];
}

fn albedo_relief(x: u32, y: u32, height: f32, uv: vec2<f32>) -> vec2<f32> {
    let radius = max(1, i32(params.full_resolution / 500u));
    let e = albedo_height(i32(x) + radius, i32(y));
    let w = albedo_height(i32(x) - radius, i32(y));
    let n = albedo_height(i32(x), i32(y) + radius);
    let s = albedo_height(i32(x), i32(y) - radius);
    let du = f32(radius) / f32(params.full_resolution - 1u);
    let dx = distance(cube_to_sphere(params.face, uv + vec2<f32>(du, 0.0)),
        cube_to_sphere(params.face, uv - vec2<f32>(du, 0.0)));
    let dy = distance(cube_to_sphere(params.face, uv + vec2<f32>(0.0, du)),
        cube_to_sphere(params.face, uv - vec2<f32>(0.0, du)));
    let slope = length(vec2<f32>((e - w) / max(dx, 0.00001), (n - s) / max(dy, 0.00001)));
    let valley = material_ramp(0.001, 0.012, max(min(e, w), min(n, s)) - height);
    return vec2<f32>(slope, valley);
}

@compute @workgroup_size(16, 16)
fn main(@builtin(global_invocation_id) id: vec3<u32>) {
    let res = params.resolution;
    if (id.x >= res || id.y >= params.local_height) {
        return;
    }

    let idx = id.y * res + id.x;

    // Compute global UV using tile offsets
    let full_res = params.full_resolution;
    let global_x = params.tile_offset_x + id.x;
    let global_y = params.tile_offset_y + id.y;
    let uv = vec2<f32>(
        f32(global_x) / f32(full_res - 1u),
        f32(global_y) / f32(full_res - 1u)
    );
    let sphere_pos = cube_to_sphere(params.face, uv);

    let height = heightmap[idx];
    let is_ocean = height < params.ocean_level;

    let color_var = snoise(sphere_pos * 8.0 + noise_seed_offset(params.seed, 72u));
    let temp = compute_temperature(sphere_pos, height);
    var surface_color: vec3<f32>;
    if (is_ocean) {
        let depth = clamp((params.ocean_level - height) / max(params.ocean_level + 1.0, 0.5), 0.0, 1.0);
        let ocean_color = surface_ocean_albedo(sphere_pos, params.seed, depth, temp);
        let ice = material_ramp(1.0, -8.0, temp);
        surface_color = mix(ocean_color, vec3<f32>(0.72, 0.82, 0.89), ice);
    } else {
        let mean_temp = climate_temperature(sphere_pos, height, params.ocean_level,
            params.base_temp_c, params.axial_tilt_rad, 0.5);
        let moisture = compute_moisture(sphere_pos, height);
        let land_height = clamp((height - params.ocean_level) / max(1.0 - params.ocean_level, 0.01), 0.0, 1.0);
        let relief = albedo_relief(id.x, id.y, height, uv);
        surface_color = surface_land_albedo(sphere_pos, params.seed, mean_temp,
            moisture, temp, land_height, relief.x, relief.y, params.ocean_fraction);
        let snow = material_ramp(0.34, 0.82, land_height) * material_ramp(2.0, -14.0, temp)
            * material_ramp(5.5, 1.5, relief.x) * mix(0.12, 1.0, material_ramp(15.0, 75.0, moisture));
        let polar = material_ramp(-8.0, -20.0, temp) * material_ramp(8.0, 30.0, moisture)
            * (1.0 - material_ramp(0.2, 0.4, land_height));
        let snow_color = mix(vec3<f32>(0.90, 0.93, 0.96), vec3<f32>(0.78, 0.86, 0.93), land_height * snow);
        surface_color = mix(surface_color, snow_color + vec3<f32>(color_var * 0.012), snow);
        surface_color = mix(surface_color, vec3<f32>(0.72, 0.82, 0.89), polar);
    }

    albedo[idx] = vec4<f32>(surface_color, 1.0);
}
