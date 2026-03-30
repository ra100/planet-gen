// Albedo map generation from heightmap + climate model.
// Replicates biome coloring from preview shader as a compute shader.
// Includes cube_sphere.wgsl and noise.wgsl at load time.

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
    _pad0: u32,
}

@group(0) @binding(0) var<storage, read> heightmap: array<f32>;
@group(0) @binding(1) var<storage, read_write> albedo: array<vec4<f32>>;
@group(0) @binding(2) var<uniform> params: AlbedoParams;

// ---- Temperature (matches preview shader) ----
fn compute_temperature(sphere_pos: vec3<f32>, height: f32) -> f32 {
    let tilt = params.axial_tilt_rad;
    let ct = cos(tilt);
    let st = sin(tilt);
    let tilted_y = sphere_pos.y * ct + sphere_pos.z * st;
    let effective_lat = asin(clamp(tilted_y, -1.0, 1.0));
    let lat_deg = abs(effective_lat) * 180.0 / 3.14159;

    let lat_normalized = lat_deg / 90.0;
    let temp_drop = 50.0 * (0.4 * lat_normalized + 0.6 * lat_normalized * lat_normalized);
    let temp_offset = params.base_temp_c - 15.0;

    let season_angle = (params.season - 0.5) * 2.0;
    let hemisphere = sin(effective_lat);
    let season_shift = season_angle * hemisphere * params.axial_tilt_rad * 15.0;

    let base_temp = 30.0 - temp_drop + temp_offset + season_shift;

    let land_fraction = max(height - params.ocean_level, 0.0) / max(1.0 - params.ocean_level, 0.01);
    let elevation_km = land_fraction * 5.0;
    return base_temp - elevation_km * 6.5;
}

// ---- Hadley cell moisture ----
fn hadley_cell_moisture(lat_rad: f32) -> f32 {
    let lat = abs(lat_rad) * 180.0 / 3.14159;
    if (lat < 10.0) { return 250.0; }
    if (lat < 20.0) { return 250.0 - (lat - 10.0) * 12.0; }
    if (lat < 35.0) { return 130.0 - (lat - 20.0) * 7.0; }
    if (lat < 50.0) { return 25.0 + (lat - 35.0) * 5.0; }
    if (lat < 65.0) { return 100.0 - (lat - 50.0) * 3.0; }
    return 55.0 - (lat - 65.0) * 1.5;
}

fn compute_moisture(sphere_pos: vec3<f32>, height: f32) -> f32 {
    let tilt = params.axial_tilt_rad;
    let tilted_y = sphere_pos.y * cos(tilt) + sphere_pos.z * sin(tilt);
    let effective_lat = asin(clamp(tilted_y, -1.0, 1.0));

    let ocean_scale = 0.05 + 0.95 * params.ocean_fraction;
    let hadley_base = hadley_cell_moisture(effective_lat) * ocean_scale;

    let noise1 = snoise(sphere_pos * 3.0 + vec3<f32>(100.0, 0.0, 0.0));
    let local_var = noise1 * 0.5;
    var moisture = hadley_base * (0.55 + 0.45 * (local_var + 0.5));
    moisture += 50.0 * (local_var + 0.5) * ocean_scale;

    // Simplified continentality (no cubemap sampling - use local height proxy)
    let is_land = height > params.ocean_level;
    if (is_land) {
        let land_height = (height - params.ocean_level) / max(1.0 - params.ocean_level, 0.01);
        moisture *= 0.6 + 0.4 * (1.0 - clamp(land_height, 0.0, 1.0));
    }

    // Rain shadow approximation
    let lat_deg = abs(effective_lat) * 180.0 / 3.14159;
    var wind_x: f32 = 0.8;
    if (lat_deg < 30.0) { wind_x = -0.8; }
    else if (lat_deg < 60.0) { wind_x = 0.8; }
    else { wind_x = -0.6; }

    if (is_land) {
        let land_height = (height - params.ocean_level) / max(1.0 - params.ocean_level, 0.01);
        if (land_height > 0.3) {
            let shadow = clamp((land_height - 0.3) / 0.4, 0.0, 1.0);
            moisture *= 1.0 - shadow * 0.4;
        }
    }

    return clamp(moisture, 0.0, 400.0);
}

fn smooth_step(edge0: f32, edge1: f32, x: f32) -> f32 {
    let t = clamp((x - edge0) / (edge1 - edge0), 0.0, 1.0);
    return t * t * (3.0 - 2.0 * t);
}

// ---- Continuous gradient biome coloring ----
fn gradient_color(temp_c: f32, moisture_cm: f32, variation: f32) -> vec3<f32> {
    let t_cold = smooth_step(-5.0, 12.0, temp_c);
    let t_hot = smooth_step(12.0, 30.0, temp_c);
    let m = smooth_step(15.0, 180.0, temp_c + moisture_cm * 0.5);

    let cold_dry = vec3<f32>(0.62, 0.55, 0.45);
    let cold_wet = vec3<f32>(0.78, 0.82, 0.86);
    let mid_dry  = vec3<f32>(0.68, 0.58, 0.36);
    let mid_wet  = vec3<f32>(0.22, 0.40, 0.15);
    let hot_dry  = vec3<f32>(0.80, 0.66, 0.40);
    let hot_wet  = vec3<f32>(0.10, 0.28, 0.06);

    let dry_color = mix(cold_dry, mix(mid_dry, hot_dry, t_hot), t_cold);
    let wet_color = mix(cold_wet, mix(mid_wet, hot_wet, t_hot), t_cold);

    let moist_t = smooth_step(20.0, 160.0, moisture_cm);
    var base = mix(dry_color, wet_color, moist_t);

    let season = params.season;
    let green_amount = max(base.g - max(base.r, base.b), 0.0);
    if (green_amount > 0.05) {
        let winter_shift = vec3<f32>(0.12, -0.04, -0.06) * (1.0 - season) * green_amount * 3.0;
        base += winter_shift;
    }
    if (temp_c < 5.0) {
        let cold_winter = mix(base, vec3<f32>(0.80, 0.82, 0.85), (1.0 - season) * 0.3 * (1.0 - t_cold));
        base = cold_winter;
    }

    base += base * variation * 0.10;
    return base;
}

@compute @workgroup_size(16, 16)
fn main(@builtin(global_invocation_id) id: vec3<u32>) {
    let res = params.resolution;
    if (id.x >= res || id.y >= res) {
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

    // Read height from the full-resolution heightmap
    // The heightmap index uses full_resolution stride
    let height_idx = global_y * full_res + global_x;
    let height = heightmap[height_idx];
    let is_ocean = height < params.ocean_level;

    let color_var = snoise(sphere_pos * 8.0);

    // Compute effective latitude for altitude zonation
    let tilt_main = params.axial_tilt_rad;
    let tilted_y_main = sphere_pos.y * cos(tilt_main) + sphere_pos.z * sin(tilt_main);
    let effective_lat = asin(clamp(tilted_y_main, -1.0, 1.0));

    var surface_color: vec3<f32>;

    if (is_ocean) {
        let ocean_temp = compute_temperature(sphere_pos, height);
        let depth = clamp((params.ocean_level - height) / max(params.ocean_level + 1.0, 0.5), 0.0, 1.0);

        let shallow = vec3<f32>(0.08, 0.22, 0.48);
        let deep = vec3<f32>(0.02, 0.05, 0.22);
        var ocean_color = mix(shallow, deep, depth);
        ocean_color += vec3<f32>(0.0, 0.015, 0.02) * color_var;

        let ice_blend = smooth_step(3.0, -8.0, ocean_temp);
        let ice_color = mix(vec3<f32>(0.65, 0.75, 0.85), vec3<f32>(0.88, 0.92, 0.96), clamp(-ocean_temp / 15.0, 0.0, 1.0));
        surface_color = mix(ocean_color, ice_color, ice_blend);
    } else {
        let temp = compute_temperature(sphere_pos, height);
        let moisture = compute_moisture(sphere_pos, height);

        surface_color = gradient_color(temp, moisture, color_var);

        // Smooth ice/snow overlay
        let ice_moisture_threshold = 15.0 + 40.0 * (1.0 - params.ocean_fraction);
        let ice_blend = smooth_step(-8.0, -20.0, temp) * smooth_step(ice_moisture_threshold * 0.5, ice_moisture_threshold, moisture);
        let ice_color = vec3<f32>(0.90, 0.93, 0.97) + vec3<f32>(0.02) * color_var;
        surface_color = mix(surface_color, ice_color, ice_blend);

        // Altitude zonation
        let land_height = (height - params.ocean_level) / max(1.0 - params.ocean_level, 0.01);
        let snow_line = 0.65 + 0.25 * (1.0 - abs(effective_lat) / 1.5708);
        let rock_line = snow_line - 0.15;
        let alpine_line = rock_line - 0.15;

        if (land_height > snow_line && temp < 15.0) {
            let blend = smooth_step(snow_line, snow_line + 0.10, land_height);
            surface_color = mix(surface_color, vec3<f32>(0.92, 0.94, 0.98), blend);
        } else if (land_height > rock_line) {
            let blend = smooth_step(rock_line, rock_line + 0.10, land_height);
            surface_color = mix(surface_color, vec3<f32>(0.50, 0.48, 0.44) + vec3<f32>(0.04) * color_var, blend);
        } else if (land_height > alpine_line) {
            let blend = smooth_step(alpine_line, alpine_line + 0.10, land_height);
            let alpine = mix(surface_color, vec3<f32>(0.48, 0.52, 0.35), 0.5);
            surface_color = mix(surface_color, alpine, blend);
        }

        if (land_height < 0.03) {
            let beach_blend = smooth_step(0.03, 0.0, land_height);
            surface_color = mix(surface_color, vec3<f32>(0.74, 0.68, 0.48), beach_blend * 0.6);
        }
    }

    albedo[idx] = vec4<f32>(surface_color, 1.0);
}
