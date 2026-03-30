// Preview renderer: samples pre-computed height cubemap + applies biome pipeline.
// Height comes from the tectonic compute pipeline (plates.wgsl).
// Temperature, moisture, biomes computed per-pixel in fragment shader.

struct Uniforms {
    rotation: mat4x4<f32>,
    light_dir: vec3<f32>,
    ocean_level: f32,
    base_temp_c: f32,
    ocean_fraction: f32,
    axial_tilt_rad: f32,
    view_mode: u32,
    season: f32, // 0=winter, 0.5=equinox, 1=summer
    _pad0: f32,
    _pad1: f32,
    _pad2: f32,
}

@group(0) @binding(0) var<uniform> uniforms: Uniforms;
@group(0) @binding(1) var height_tex: texture_cube<f32>;
@group(0) @binding(2) var height_sampler: sampler;

struct VertexOutput {
    @builtin(position) position: vec4<f32>,
    @location(0) uv: vec2<f32>,
}

fn smooth_step(edge0: f32, edge1: f32, x: f32) -> f32 {
    let t = clamp((x - edge0) / (edge1 - edge0), 0.0, 1.0);
    return t * t * (3.0 - 2.0 * t);
}

@vertex
fn vs_main(@builtin(vertex_index) idx: u32) -> VertexOutput {
    var out: VertexOutput;
    let x = f32(i32(idx) / 2) * 4.0 - 1.0;
    let y = f32(i32(idx) % 2) * 4.0 - 1.0;
    out.position = vec4<f32>(x, y, 0.0, 1.0);
    out.uv = vec2<f32>(x, -y) * 0.5 + 0.5;
    return out;
}

fn intersect_sphere(uv: vec2<f32>) -> vec3<f32> {
    let ndc = (uv - 0.5) * 2.0 / 0.85;
    let r2 = dot(ndc, ndc);
    if (r2 > 1.0) { return vec3<f32>(0.0, 0.0, 0.0); }
    let z = sqrt(1.0 - r2);
    return vec3<f32>(ndc.x, ndc.y, z);
}

// ---- Temperature ----
fn compute_temperature(sphere_pos: vec3<f32>, height: f32) -> f32 {
    let latitude = asin(clamp(sphere_pos.y, -1.0, 1.0));
    // Axial tilt: rotate the "solar axis" rather than shifting latitude by sin(lon).
    // This models the sub-solar point offset without creating V-shaped artifacts.
    // The tilt shifts effective latitude smoothly across the whole sphere.
    let tilt = uniforms.axial_tilt_rad;
    let ct = cos(tilt);
    let st = sin(tilt);
    // Tilted Y-axis: the "effective pole" is rotated
    let tilted_y = sphere_pos.y * ct + sphere_pos.z * st;
    let effective_lat = asin(clamp(tilted_y, -1.0, 1.0));
    let lat_deg = abs(effective_lat) * 180.0 / 3.14159;

    // Temperature gradient: ~50°C range (30°C equator to -20°C poles)
    // Non-linear: flatter at mid-latitudes (ocean heat transport), steeper near poles
    let lat_normalized = lat_deg / 90.0;
    let temp_drop = 50.0 * (0.4 * lat_normalized + 0.6 * lat_normalized * lat_normalized);
    let temp_offset = uniforms.base_temp_c - 15.0;

    // Season modifies temperature: summer hemisphere warmer, winter hemisphere colder
    // season=0 (winter): north colder, south warmer. season=1 (summer): north warmer
    let season_angle = (uniforms.season - 0.5) * 2.0; // [-1, 1]
    // Smooth hemisphere factor using sin(lat) — no discontinuity at equator
    let hemisphere = sin(effective_lat); // smooth -1..+1 transition through equator
    let season_shift = season_angle * hemisphere * uniforms.axial_tilt_rad * 15.0;

    let base_temp = 30.0 - temp_drop + temp_offset + season_shift;

    let land_fraction = max(height - uniforms.ocean_level, 0.0) / max(1.0 - uniforms.ocean_level, 0.01);
    let elevation_km = land_fraction * 5.0;
    let lapse = -6.5 * elevation_km;

    let temp_noise = snoise(sphere_pos * 3.0) * 3.0;
    return base_temp + lapse + temp_noise;
}

// ---- Hadley cell moisture ----
fn hadley_cell_moisture(latitude_rad: f32) -> f32 {
    let lat_deg = abs(latitude_rad) * 180.0 / 3.14159;
    // ITCZ: tropical wet belt
    let itcz_wet = exp(-lat_deg * lat_deg / 200.0) * 200.0;
    // Subtropical dry: reduced intensity, narrower — deserts are regional, not planet-wide
    let subtropical_dry = -80.0 * exp(-((lat_deg - 28.0) * (lat_deg - 28.0)) / 60.0);
    // Mid-latitude wet belt (westerlies)
    let polar_front_wet = 90.0 * exp(-((lat_deg - 50.0) * (lat_deg - 50.0)) / 200.0);
    // Polar drying
    let polar_dry = -60.0 * smooth_step(65.0, 85.0, lat_deg);
    // Higher base ensures most temperate land has enough moisture for vegetation
    return max(itcz_wet + subtropical_dry + polar_front_wet + polar_dry + 90.0, 10.0);
}

// Wind direction from Hadley cells for rain shadow
fn wind_direction_vec(latitude_rad: f32) -> vec3<f32> {
    let lat_deg = abs(latitude_rad) * 180.0 / 3.14159;
    // Trade winds (0-30°): from east. Westerlies (30-60°): from west. Polar (60-90°): from east.
    var wind_x: f32;
    if (lat_deg < 30.0) { wind_x = -0.8; }  // Easterly
    else if (lat_deg < 60.0) { wind_x = 0.8; } // Westerly
    else { wind_x = -0.6; } // Polar easterly
    // Smooth poleward component — no discontinuity at equator
    let wind_y = -0.2 * sin(latitude_rad);
    return normalize(vec3<f32>(wind_x, wind_y, 0.3));
}

fn compute_moisture(sphere_pos: vec3<f32>, height: f32) -> f32 {
    let tilt = uniforms.axial_tilt_rad;
    let tilted_y = sphere_pos.y * cos(tilt) + sphere_pos.z * sin(tilt);
    let effective_lat = asin(clamp(tilted_y, -1.0, 1.0));

    // Hadley cell base moisture — scaled by ocean fraction FIRST.
    // No ocean = no evaporation = no atmospheric moisture (dry world like Mars)
    let ocean_scale = 0.05 + 0.95 * uniforms.ocean_fraction; // 5% minimum (sublimation)
    let hadley_base = hadley_cell_moisture(effective_lat) * ocean_scale;

    // Local noise variation (breaks latitude bands)
    let noise1 = snoise(sphere_pos * 3.0 + vec3<f32>(100.0, 0.0, 0.0));
    let local_var = noise1 * 0.5;
    var moisture = hadley_base * (0.55 + 0.45 * (local_var + 0.5));
    moisture += 50.0 * (local_var + 0.5) * ocean_scale;

    // === CUBEMAP-BASED CONTINENTALITY (Unit 2) ===
    // Sample height at neighboring positions to determine coast vs interior
    let step = 0.06;
    let h_east = textureSample(height_tex, height_sampler, sphere_pos + vec3<f32>(step, 0.0, 0.0)).r;
    let h_west = textureSample(height_tex, height_sampler, sphere_pos + vec3<f32>(-step, 0.0, 0.0)).r;
    let h_north = textureSample(height_tex, height_sampler, sphere_pos + vec3<f32>(0.0, step, 0.0)).r;
    let h_south = textureSample(height_tex, height_sampler, sphere_pos + vec3<f32>(0.0, -step, 0.0)).r;

    var ocean_count = 0.0;
    if (h_east < uniforms.ocean_level) { ocean_count += 1.0; }
    if (h_west < uniforms.ocean_level) { ocean_count += 1.0; }
    if (h_north < uniforms.ocean_level) { ocean_count += 1.0; }
    if (h_south < uniforms.ocean_level) { ocean_count += 1.0; }

    let is_land = height > uniforms.ocean_level;
    if (is_land) {
        // Continentality: more land neighbors = drier interior
        let coastal_factor = ocean_count / 4.0; // 0 = deep interior, 1 = surrounded by ocean
        moisture *= 0.7 + 0.4 * coastal_factor; // Interior: ×0.7, coast: ×1.1
    } else {
        moisture *= 1.3; // Over ocean
    }

    // === CUBEMAP-BASED RAIN SHADOW (Unit 1) ===
    // Sample upwind terrain to detect mountains blocking moisture
    if (is_land) {
        let wind = wind_direction_vec(effective_lat);
        // Project wind to be tangent to sphere at this position
        let tangent_wind = normalize(wind - sphere_pos * dot(wind, sphere_pos));
        let upwind_pos = normalize(sphere_pos + tangent_wind * 0.05);

        let upwind_h = textureSample(height_tex, height_sampler, upwind_pos).r;
        let upwind_elevation = max(upwind_h - uniforms.ocean_level, 0.0);
        let my_elevation = max(height - uniforms.ocean_level, 0.0);

        // If upwind terrain is higher → we're in a rain shadow
        if (upwind_elevation > my_elevation + 0.05) {
            let shadow_strength = clamp((upwind_elevation - my_elevation) * 4.0, 0.0, 0.6);
            moisture *= (1.0 - shadow_strength);
        }
    }

    moisture *= 0.5 + uniforms.ocean_fraction;
    return clamp(moisture, 0.0, 400.0);
}

// ---- Continuous gradient biome coloring ----
// Replaces discrete Whittaker lookup with smooth 2D interpolation.
// Temperature × moisture → color via 3×2 anchor grid.

fn gradient_color(temp_c: f32, moisture_cm: f32, variation: f32) -> vec3<f32> {
    // Temperature interpolation weights
    let t_cold = smooth_step(-8.0, 10.0, temp_c);   // 0 = cold, 1 = temperate+
    let t_hot = smooth_step(10.0, 28.0, temp_c);     // 0 = temperate, 1 = hot

    // Anchor colors (3 temp levels × 2 moisture levels)
    let cold_dry = vec3<f32>(0.58, 0.38, 0.25);  // Rust/Mars-like cold desert
    let cold_wet = vec3<f32>(0.75, 0.80, 0.85);  // Snow fields / icy tundra
    let mid_dry  = vec3<f32>(0.55, 0.50, 0.30);  // Dry steppe / scrubland
    let mid_wet  = vec3<f32>(0.16, 0.40, 0.10);  // Rich temperate forest
    let hot_dry  = vec3<f32>(0.82, 0.55, 0.30);  // Orange-red desert
    let hot_wet  = vec3<f32>(0.08, 0.30, 0.05);  // Deep tropical jungle

    // Interpolate along temperature axis (cold → mid → hot)
    let dry_color = mix(cold_dry, mix(mid_dry, hot_dry, t_hot), t_cold);
    let wet_color = mix(cold_wet, mix(mid_wet, hot_wet, t_hot), t_cold);

    // Moisture interpolation — low threshold so typical temperate land (50-100cm) is green
    let moist_t = smooth_step(10.0, 90.0, moisture_cm);
    var base = mix(dry_color, wet_color, moist_t);

    // Season modulation: winter shifts green→brown, cold→whiter
    let season = uniforms.season; // 0=winter, 1=summer
    let green_amount = max(base.g - max(base.r, base.b), 0.0);
    if (green_amount > 0.05) {
        // Vegetated areas: shift toward golden-brown in winter
        let winter_shift = vec3<f32>(0.12, -0.04, -0.06) * (1.0 - season) * green_amount * 3.0;
        base += winter_shift;
    }
    if (temp_c < 5.0) {
        // Cold regions: whiter in winter
        let cold_winter = mix(base, vec3<f32>(0.80, 0.82, 0.85), (1.0 - season) * 0.3 * (1.0 - t_cold));
        base = cold_winter;
    }

    // Per-pixel noise variation for natural texture
    base += base * variation * 0.10;

    return base;
}

// ---- Main fragment shader ----
@fragment
fn fs_main(in: VertexOutput) -> @location(0) vec4<f32> {
    let hit = intersect_sphere(in.uv);
    if (length(hit) < 0.01) {
        return vec4<f32>(0.02, 0.02, 0.05, 1.0);
    }

    let normal = normalize(hit);
    let rotated = (uniforms.rotation * vec4<f32>(normal, 0.0)).xyz;

    // Sample height from pre-computed cubemap
    let height = textureSample(height_tex, height_sampler, rotated).r;
    let is_ocean = height < uniforms.ocean_level;

    let color_var = snoise(rotated * 8.0);

    // Compute effective latitude for altitude zonation (consistent tilt model)
    let tilt_main = uniforms.axial_tilt_rad;
    let tilted_y_main = rotated.y * cos(tilt_main) + rotated.z * sin(tilt_main);
    let effective_lat = asin(clamp(tilted_y_main, -1.0, 1.0));

    var surface_color: vec3<f32>;

    if (is_ocean) {
        // Smooth ocean gradient: shallow → deep with continuous depth color
        let ocean_temp = compute_temperature(rotated, height);
        let depth = clamp((uniforms.ocean_level - height) / max(uniforms.ocean_level + 1.0, 0.5), 0.0, 1.0);

        // Continuous depth color: shallow turquoise → mid blue → deep navy
        let shallow = vec3<f32>(0.08, 0.22, 0.48);
        let deep = vec3<f32>(0.02, 0.05, 0.22);
        var ocean_color = mix(shallow, deep, depth);
        ocean_color += vec3<f32>(0.0, 0.015, 0.02) * color_var; // subtle variation

        // Smooth ice transition: no hard cutoff, gradual freeze
        let ice_blend = smooth_step(3.0, -8.0, ocean_temp); // starts blending at 3°C, full ice at -8°C
        let ice_color = mix(vec3<f32>(0.65, 0.75, 0.85), vec3<f32>(0.88, 0.92, 0.96), clamp(-ocean_temp / 15.0, 0.0, 1.0));
        surface_color = mix(ocean_color, ice_color, ice_blend);
    } else {
        let temp = compute_temperature(rotated, height);
        let moisture = compute_moisture(rotated, height);

        // Continuous gradient coloring — no biome IDs, no boundaries
        surface_color = gradient_color(temp, moisture, color_var);

        // Smooth ice/snow overlay for very cold land
        let ice_moisture_threshold = 15.0 + 40.0 * (1.0 - uniforms.ocean_fraction);
        let ice_blend = smooth_step(-8.0, -20.0, temp) * smooth_step(ice_moisture_threshold * 0.5, ice_moisture_threshold, moisture);
        let ice_color = vec3<f32>(0.90, 0.93, 0.97) + vec3<f32>(0.02) * color_var;
        surface_color = mix(surface_color, ice_color, ice_blend);

        // Altitude zonation (smooth blending on top of gradient base)
        let land_height = (height - uniforms.ocean_level) / max(1.0 - uniforms.ocean_level, 0.01);
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

        // Beach transition
        if (land_height < 0.03) {
            let beach_blend = smooth_step(0.03, 0.0, land_height);
            surface_color = mix(surface_color, vec3<f32>(0.74, 0.68, 0.48), beach_blend * 0.6);
        }
    }

    // Debug views
    if (uniforms.view_mode > 0u) {
        let temp = compute_temperature(rotated, height);
        let moisture = compute_moisture(rotated, height);
        var debug_color: vec3<f32>;

        switch (uniforms.view_mode) {
            case 1u: { let h = (height + 1.0) * 0.5; debug_color = vec3<f32>(h, h, h); }
            case 2u: {
                let t = clamp(temp / 50.0, -1.0, 1.0);
                if (t < 0.0) { debug_color = mix(vec3<f32>(0.0, 0.0, 1.0), vec3<f32>(1.0), t + 1.0); }
                else { debug_color = mix(vec3<f32>(1.0), vec3<f32>(1.0, 0.0, 0.0), t); }
            }
            case 3u: {
                let m = clamp(moisture / 300.0, 0.0, 1.0);
                if (m < 0.5) { debug_color = mix(vec3<f32>(0.6, 0.4, 0.1), vec3<f32>(0.1, 0.6, 0.1), m * 2.0); }
                else { debug_color = mix(vec3<f32>(0.1, 0.6, 0.1), vec3<f32>(0.1, 0.2, 0.8), (m - 0.5) * 2.0); }
            }
            case 4u: { debug_color = gradient_color(temp, moisture, 0.0) * 1.3; }
            case 5u: {
                if (is_ocean) {
                    if (compute_temperature(rotated, height) < -2.0) { debug_color = vec3<f32>(1.0); }
                    else { debug_color = vec3<f32>(0.0, 0.2, 0.8); }
                } else {
                    if (temp < -15.0) { debug_color = vec3<f32>(0.9, 0.95, 1.0); }
                    else {
                        let lh = clamp((height - uniforms.ocean_level) / max(1.0 - uniforms.ocean_level, 0.01), 0.0, 1.0);
                        debug_color = mix(vec3<f32>(0.2, 0.6, 0.1), vec3<f32>(0.5, 0.3, 0.1), lh);
                    }
                }
            }
            case 6u: {
                // Plate structure: height with contour lines at boundaries
                // Sample neighboring heights to detect edges (plate boundaries)
                let step = 0.01;
                let h_r = textureSample(height_tex, height_sampler, rotated + vec3<f32>(step, 0.0, 0.0)).r;
                let h_u = textureSample(height_tex, height_sampler, rotated + vec3<f32>(0.0, step, 0.0)).r;
                let gradient = abs(h_r - height) + abs(h_u - height);

                // Base: color by elevation (blue ocean, tan/green land)
                let h = (height + 0.5) / 1.0;
                if (height < uniforms.ocean_level) {
                    debug_color = mix(vec3<f32>(0.05, 0.1, 0.3), vec3<f32>(0.1, 0.2, 0.5), clamp(h + 0.5, 0.0, 1.0));
                } else {
                    debug_color = mix(vec3<f32>(0.3, 0.5, 0.2), vec3<f32>(0.7, 0.6, 0.4), clamp((height - uniforms.ocean_level) * 3.0, 0.0, 1.0));
                }

                // Overlay bright lines at plate boundaries (sharp height gradients)
                if (gradient > 0.02) {
                    let edge_strength = clamp((gradient - 0.02) * 20.0, 0.0, 1.0);
                    debug_color = mix(debug_color, vec3<f32>(1.0, 0.3, 0.1), edge_strength);
                }

                // Contour lines at regular height intervals
                let contour = fract(height * 8.0);
                if (contour < 0.05 || contour > 0.95) {
                    debug_color *= 0.7;
                }
            }
            case 7u: {
                // Roughness visualization
                if (is_ocean) {
                    debug_color = vec3<f32>(0.05, 0.05, 0.1); // Water = very smooth (dark)
                } else {
                    let rt = compute_temperature(rotated, height);
                    let rm = compute_moisture(rotated, height);
                    var r: f32;
                    if (rt < 0.0) { r = 0.15; }
                    else if (rt < 10.0) { r = 0.55; }
                    else if (rm < 25.0) { r = 0.85; }
                    else if (rm < 100.0) { r = 0.60; }
                    else { r = 0.50; }
                    r += snoise(rotated * 10.0) * 0.1;
                    r = clamp(r, 0.0, 1.0);
                    debug_color = vec3<f32>(r, r, r);
                }
            }
            default: { debug_color = surface_color; }
        }
        return vec4<f32>(debug_color, 1.0);
    }

    // Lighting
    let light = normalize(uniforms.light_dir);
    let ndotl = max(dot(normal, light), 0.0);
    let lit_color = surface_color * (0.15 + 0.85 * ndotl);
    return vec4<f32>(lit_color, 1.0);
}
