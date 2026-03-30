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
    let hemisphere = select(-1.0, 1.0, effective_lat > 0.0);
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

// ---- Whittaker biome lookup ----
fn whittaker_lookup(temp_c: f32, moisture_cm: f32) -> u32 {
    if (temp_c < 0.0 && moisture_cm < 15.0) { return 4u; }
    if (temp_c < 0.0) { if (moisture_cm < 50.0) { return 0u; } return 1u; }
    if (temp_c < 5.0) { if (moisture_cm < 25.0) { return 1u; } return 2u; }
    if (temp_c < 10.0) { if (moisture_cm < 10.0) { return 1u; } return 2u; }
    if (temp_c < 15.0) {
        if (moisture_cm < 10.0) { return 4u; }
        if (moisture_cm < 25.0) { return 5u; }
        if (moisture_cm < 50.0) { return 6u; }
        if (moisture_cm < 150.0) { return 3u; }
        return 10u;
    }
    if (temp_c < 20.0) {
        if (moisture_cm < 10.0) { return 4u; }
        if (moisture_cm < 25.0) { return 5u; }
        if (moisture_cm < 50.0) { return 6u; }
        if (moisture_cm < 100.0) { return 8u; }
        if (moisture_cm < 150.0) { return 9u; }
        return 10u;
    }
    if (temp_c < 24.0) {
        if (moisture_cm < 25.0) { return 4u; }
        if (moisture_cm < 50.0) { return 7u; }
        if (moisture_cm < 100.0) { return 7u; }
        if (moisture_cm < 150.0) { return 11u; }
        return 12u;
    }
    if (moisture_cm < 25.0) { return 4u; }
    if (moisture_cm < 50.0) { return 7u; }
    if (moisture_cm < 100.0) { return 7u; }
    if (moisture_cm < 200.0) { return 11u; }
    return 12u;
}

fn smooth_step(edge0: f32, edge1: f32, x: f32) -> f32 {
    let t = clamp((x - edge0) / (edge1 - edge0), 0.0, 1.0);
    return t * t * (3.0 - 2.0 * t);
}

fn biome_color(biome: u32, variation: f32, temp_c: f32) -> vec3<f32> {
    var base: vec3<f32>;
    switch (biome) {
        case 0u:  { base = vec3<f32>(0.94, 0.96, 1.00); } // Tundra
        case 1u:  { base = vec3<f32>(0.55, 0.63, 0.51); } // Boreal/Taiga
        case 2u:  { base = vec3<f32>(0.24, 0.31, 0.18); } // Conifer forest
        case 3u:  { base = vec3<f32>(0.16, 0.27, 0.14); } // Temperate rainforest
        case 4u:  { base = vec3<f32>(0.82, 0.71, 0.55); } // Desert
        case 5u:  { base = vec3<f32>(0.65, 0.60, 0.40); } // Scrubland
        case 6u:  { base = vec3<f32>(0.34, 0.51, 0.20); } // Grassland
        case 7u:  { base = vec3<f32>(0.63, 0.59, 0.24); } // Savanna
        case 8u:  { base = vec3<f32>(0.30, 0.45, 0.20); } // Temperate forest
        case 9u:  { base = vec3<f32>(0.20, 0.39, 0.12); } // Temperate deciduous
        case 10u: { base = vec3<f32>(0.15, 0.35, 0.10); } // Tropical seasonal
        case 11u: { base = vec3<f32>(0.18, 0.38, 0.10); } // Tropical deciduous
        case 12u: { base = vec3<f32>(0.13, 0.31, 0.08); } // Tropical rainforest
        default:  { base = vec3<f32>(0.50, 0.50, 0.50); }
    }

    // Cold desert override
    if (biome == 4u) {
        if (temp_c < 5.0) {
            base = vec3<f32>(0.60, 0.32, 0.18);
        } else if (temp_c < 15.0) {
            let t = (temp_c - 5.0) / 10.0;
            base = mix(vec3<f32>(0.60, 0.32, 0.18), vec3<f32>(0.75, 0.55, 0.35), t);
        }
    }
    if (biome == 5u && temp_c < 10.0) {
        base = mix(vec3<f32>(0.55, 0.40, 0.25), base, clamp(temp_c / 10.0, 0.0, 1.0));
    }

    // Seasonal shifts
    let season_factor = params.season;
    if (biome == 9u || biome == 11u) {
        let winter_color = vec3<f32>(0.45, 0.35, 0.20);
        base = mix(winter_color, base, season_factor);
    }
    if (biome == 6u || biome == 7u) {
        let dry_color = vec3<f32>(0.55, 0.50, 0.25);
        base = mix(dry_color, base, season_factor);
    }

    return base + vec3<f32>(variation * 0.04);
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
        if (ocean_temp < -2.0) {
            let ice_intensity = clamp((-2.0 - ocean_temp) / 10.0, 0.0, 1.0);
            surface_color = mix(vec3<f32>(0.55, 0.70, 0.82), vec3<f32>(0.85, 0.90, 0.95), ice_intensity);
        } else if (ocean_temp < 2.0) {
            let blend = (ocean_temp + 2.0) / 4.0;
            let depth = (params.ocean_level - height) / max(params.ocean_level + 1.0, 0.5);
            let water = mix(vec3<f32>(0.06, 0.18, 0.50), vec3<f32>(0.02, 0.05, 0.25), clamp(depth, 0.0, 1.0));
            surface_color = mix(vec3<f32>(0.55, 0.70, 0.82), water, blend);
        } else {
            let depth = (params.ocean_level - height) / max(params.ocean_level + 1.0, 0.5);
            surface_color = mix(vec3<f32>(0.06, 0.18, 0.50), vec3<f32>(0.02, 0.05, 0.25), clamp(depth, 0.0, 1.0));
            surface_color += vec3<f32>(0.0, 0.02, 0.03) * color_var;
        }
    } else {
        let temp = compute_temperature(sphere_pos, height);
        let moisture = compute_moisture(sphere_pos, height);

        let ice_moisture_threshold = 15.0 + 40.0 * (1.0 - params.ocean_fraction);
        if (temp < -15.0 && moisture > ice_moisture_threshold) {
            surface_color = vec3<f32>(0.92, 0.94, 0.98) + vec3<f32>(0.03) * color_var;
        } else {
            let biome = whittaker_lookup(temp, moisture);
            surface_color = biome_color(biome, color_var, temp);

            // Altitude zonation
            let land_height = (height - params.ocean_level) / max(1.0 - params.ocean_level, 0.01);
            let snow_line = 0.65 + 0.25 * (1.0 - abs(effective_lat) / 1.5708);
            let rock_line = snow_line - 0.15;
            let alpine_line = rock_line - 0.15;

            if (land_height > snow_line && temp < 15.0) {
                let blend = smooth_step(snow_line, snow_line + 0.08, land_height);
                surface_color = mix(surface_color, vec3<f32>(0.94, 0.96, 1.0), blend);
            } else if (land_height > rock_line) {
                let blend = smooth_step(rock_line, rock_line + 0.08, land_height);
                surface_color = mix(surface_color, vec3<f32>(0.45, 0.40, 0.35) + vec3<f32>(0.05) * color_var, blend);
            } else if (land_height > alpine_line) {
                let blend = smooth_step(alpine_line, alpine_line + 0.08, land_height);
                let alpine_color = mix(surface_color, vec3<f32>(0.40, 0.45, 0.30), 0.6);
                surface_color = mix(surface_color, alpine_color, blend);
            }

            // Beach transition
            if (land_height < 0.03) {
                let beach_blend = 1.0 - clamp(land_height / 0.03, 0.0, 1.0);
                surface_color = mix(surface_color, vec3<f32>(0.76, 0.70, 0.50), beach_blend * 0.7);
            }
        }
    }

    albedo[idx] = vec4<f32>(surface_color, 1.0);
}
