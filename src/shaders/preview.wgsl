// Preview renderer with full biome pipeline.
// Computes per-pixel: height → temperature → moisture → biome → color.
// All noise evaluated directly in fragment shader — no cubemap, no seams.

struct Uniforms {
    rotation: mat4x4<f32>,
    light_dir: vec3<f32>,
    ocean_level: f32,
    // Terrain params
    seed_offset: vec3<f32>,
    frequency: f32,
    lacunarity: f32,
    gain: f32,
    amplitude: f32,
    octaves: u32,
    // Planet properties
    base_temp_c: f32,
    ocean_fraction: f32,
    axial_tilt_rad: f32,
    _pad: f32,
}

@group(0) @binding(0) var<uniform> uniforms: Uniforms;

struct VertexOutput {
    @builtin(position) position: vec4<f32>,
    @location(0) uv: vec2<f32>,
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

// Ray-sphere intersection — sphere fills ~85% of viewport
fn intersect_sphere(uv: vec2<f32>) -> vec3<f32> {
    let ndc = (uv - 0.5) * 2.0 / 0.85;
    let r2 = dot(ndc, ndc);
    if (r2 > 1.0) {
        return vec3<f32>(0.0, 0.0, 0.0);
    }
    let z = sqrt(1.0 - r2);
    return vec3<f32>(ndc.x, ndc.y, z);
}

// ---- Terrain height (fBm) ----

fn fbm_preview(pos: vec3<f32>) -> f32 {
    var value = 0.0;
    var freq = uniforms.frequency;
    var amp = uniforms.amplitude;
    let p = pos + uniforms.seed_offset;

    for (var i = 0u; i < uniforms.octaves; i++) {
        value += amp * snoise(p * freq);
        freq *= uniforms.lacunarity;
        amp *= uniforms.gain;
    }
    return value;
}

// ---- Temperature (research section 13.2) ----
// baseTemp = 30 - abs(latitude) * 60, then lapse rate + noise

fn compute_temperature(sphere_pos: vec3<f32>, height: f32) -> f32 {
    let latitude = asin(clamp(sphere_pos.y, -1.0, 1.0)); // radians
    let lat_deg = abs(latitude) * 180.0 / 3.14159;

    // Base temperature from latitude: equator ~30°C, poles ~-30°C
    // Scaled by planet's base temperature relative to Earth's 15°C
    let temp_scale = uniforms.base_temp_c / 15.0;
    let base_temp = 30.0 * temp_scale - lat_deg * (60.0 * temp_scale / 90.0);

    // Lapse rate: -6.5°C per km elevation (normalized height → km estimate)
    let elevation_km = max(height - uniforms.ocean_level, 0.0) * 8.0; // scale factor
    let lapse = -6.5 * elevation_km;

    // Small noise variation
    let temp_noise = snoise(sphere_pos * 3.0 + uniforms.seed_offset * 0.5) * 5.0;

    return base_temp + lapse + temp_noise;
}

// ---- Moisture (research section 13.2) ----
// noise-based + ocean proximity (height below sea level = near ocean)

fn compute_moisture(sphere_pos: vec3<f32>, height: f32) -> f32 {
    // Base moisture from noise (0-200 cm/yr range)
    let moisture_noise = snoise(sphere_pos * 2.5 + uniforms.seed_offset * 1.7 + vec3<f32>(100.0, 0.0, 0.0));
    var moisture = (moisture_noise * 0.5 + 0.5) * 200.0;

    // Ocean proximity bonus: areas near sea level get more moisture
    let dist_from_sea = abs(height - uniforms.ocean_level);
    let ocean_bonus = 100.0 * exp(-dist_from_sea * 5.0);
    moisture += ocean_bonus;

    // Rain shadow: reduce moisture on the leeward side of mountains
    // Simplified: high terrain blocks moisture
    let upwind_height = height;
    if (upwind_height > uniforms.ocean_level + 0.3) {
        moisture *= 0.3; // Mountains block rain
    }

    // More ocean = more global moisture
    moisture *= 0.5 + uniforms.ocean_fraction;

    return clamp(moisture, 0.0, 400.0);
}

// ---- Whittaker Biome Lookup (research section 13.1) ----
// Returns biome ID: 0=ice, 1=tundra, 2=taiga, 3=boreal, 4=desert,
// 5=semiarid/steppe, 6=grassland, 7=savanna, 8=woodland,
// 9=temperate_df, 10=temperate_rf, 11=tropical_df, 12=tropical_rf

fn whittaker_lookup(temp_c: f32, moisture_cm: f32) -> u32 {
    // Temperature bands
    if (temp_c < 0.0) {
        if (moisture_cm < 50.0) { return 0u; } // ICE
        return 1u; // TUNDRA
    }
    if (temp_c < 5.0) {
        if (moisture_cm < 25.0) { return 1u; } // TUNDRA
        return 2u; // TAIGA
    }
    if (temp_c < 10.0) {
        if (moisture_cm < 10.0) { return 1u; } // TUNDRA
        return 2u; // TAIGA
    }
    if (temp_c < 15.0) {
        if (moisture_cm < 10.0) { return 4u; }  // DESERT
        if (moisture_cm < 25.0) { return 5u; }  // STEPPE
        if (moisture_cm < 50.0) { return 6u; }  // GRASSLAND
        if (moisture_cm < 150.0) { return 3u; } // BOREAL
        return 10u; // TEMPERATE_RF
    }
    if (temp_c < 20.0) {
        if (moisture_cm < 10.0) { return 4u; }  // DESERT
        if (moisture_cm < 25.0) { return 5u; }  // SEMIARID
        if (moisture_cm < 50.0) { return 6u; }  // GRASSLAND
        if (moisture_cm < 100.0) { return 8u; } // WOODLAND
        if (moisture_cm < 150.0) { return 9u; } // TEMPERATE_DF
        return 10u; // TEMPERATE_RF
    }
    if (temp_c < 24.0) {
        if (moisture_cm < 25.0) { return 4u; }  // DESERT
        if (moisture_cm < 50.0) { return 7u; }  // THORN_SAVANNA
        if (moisture_cm < 100.0) { return 7u; } // SAVANNA
        if (moisture_cm < 150.0) { return 11u; } // TROPICAL_DF
        return 12u; // TROPICAL_RF
    }
    // > 24°C
    if (moisture_cm < 25.0) { return 4u; }  // DESERT
    if (moisture_cm < 50.0) { return 7u; }  // THORN_SAVANNA
    if (moisture_cm < 100.0) { return 7u; } // DRY_SAVANNA
    if (moisture_cm < 150.0) { return 7u; } // WET_SAVANNA
    if (moisture_cm < 200.0) { return 11u; } // TROPICAL_DF
    return 12u; // TROPICAL_RF
}

// ---- Biome Colors (research section 13.3) ----

fn biome_color(biome: u32, variation: f32) -> vec3<f32> {
    var base: vec3<f32>;
    switch (biome) {
        case 0u:  { base = vec3<f32>(0.94, 0.96, 1.00); } // Ice
        case 1u:  { base = vec3<f32>(0.55, 0.63, 0.51); } // Tundra
        case 2u:  { base = vec3<f32>(0.24, 0.31, 0.18); } // Taiga
        case 3u:  { base = vec3<f32>(0.16, 0.27, 0.14); } // Boreal
        case 4u:  { base = vec3<f32>(0.82, 0.71, 0.55); } // Desert
        case 5u:  { base = vec3<f32>(0.65, 0.60, 0.40); } // Semiarid/Steppe
        case 6u:  { base = vec3<f32>(0.34, 0.51, 0.20); } // Grassland
        case 7u:  { base = vec3<f32>(0.63, 0.59, 0.24); } // Savanna
        case 8u:  { base = vec3<f32>(0.30, 0.45, 0.20); } // Woodland
        case 9u:  { base = vec3<f32>(0.20, 0.39, 0.12); } // Temperate deciduous
        case 10u: { base = vec3<f32>(0.15, 0.35, 0.10); } // Temperate rainforest
        case 11u: { base = vec3<f32>(0.18, 0.38, 0.10); } // Tropical deciduous
        case 12u: { base = vec3<f32>(0.13, 0.31, 0.08); } // Tropical rainforest
        default:  { base = vec3<f32>(0.50, 0.50, 0.50); }
    }

    // ±12% color variation per channel
    return base + base * variation * 0.12;
}

fn biome_roughness(biome: u32) -> f32 {
    switch (biome) {
        case 0u:  { return 0.15; } // Ice - smooth
        case 1u:  { return 0.50; } // Tundra
        case 2u:  { return 0.55; } // Taiga
        case 3u:  { return 0.60; } // Boreal
        case 4u:  { return 0.85; } // Desert - rough
        case 5u:  { return 0.70; } // Semiarid
        case 6u:  { return 0.55; } // Grassland
        case 7u:  { return 0.65; } // Savanna
        case 8u:  { return 0.55; } // Woodland
        case 9u:  { return 0.55; } // Temperate deciduous
        case 10u: { return 0.50; } // Temperate rainforest
        case 11u: { return 0.50; } // Tropical deciduous
        case 12u: { return 0.50; } // Tropical rainforest
        default:  { return 0.60; }
    }
}

// ---- Main fragment shader ----

@fragment
fn fs_main(in: VertexOutput) -> @location(0) vec4<f32> {
    let hit = intersect_sphere(in.uv);

    if (length(hit) < 0.01) {
        return vec4<f32>(0.02, 0.02, 0.05, 1.0); // Space background
    }

    let normal = normalize(hit);
    let rotated = (uniforms.rotation * vec4<f32>(normal, 0.0)).xyz;

    // 1. Terrain height
    let raw_height = fbm_preview(rotated);
    let height = clamp(raw_height / (uniforms.amplitude * 1.5), -1.0, 1.0);

    // 2. Ocean check
    let is_ocean = height < uniforms.ocean_level;

    // Color variation noise (independent of terrain)
    let color_var = snoise(rotated * 8.0 + uniforms.seed_offset * 2.0);

    var surface_color: vec3<f32>;

    if (is_ocean) {
        // Ocean coloring
        let depth = (uniforms.ocean_level - height) / max(uniforms.ocean_level + 1.0, 0.5);
        let deep = vec3<f32>(0.02, 0.05, 0.25);
        let shallow = vec3<f32>(0.06, 0.18, 0.50);
        surface_color = mix(shallow, deep, clamp(depth, 0.0, 1.0));
        // Slight color variation in ocean
        surface_color += vec3<f32>(0.0, 0.02, 0.03) * color_var;
    } else {
        // 3. Temperature
        let temp = compute_temperature(rotated, height);

        // 4. Moisture
        let moisture = compute_moisture(rotated, height);

        // Ice caps: very cold → ice regardless of biome
        if (temp < -15.0) {
            surface_color = vec3<f32>(0.92, 0.94, 0.98) + vec3<f32>(0.03) * color_var;
        } else {
            // 5. Biome lookup
            let biome = whittaker_lookup(temp, moisture);

            // 6. Biome color with variation
            surface_color = biome_color(biome, color_var);

            // Mountain rock at very high elevation
            let land_height = (height - uniforms.ocean_level) / max(1.0 - uniforms.ocean_level, 0.01);
            if (land_height > 0.7) {
                let rock = vec3<f32>(0.50, 0.50, 0.50) + vec3<f32>(0.05) * color_var;
                let rock_blend = clamp((land_height - 0.7) / 0.15, 0.0, 1.0);
                surface_color = mix(surface_color, rock, rock_blend);
            }

            // Snow on very high peaks
            if (land_height > 0.85 && temp < 10.0) {
                let snow = vec3<f32>(0.94, 0.96, 1.0);
                let snow_blend = clamp((land_height - 0.85) / 0.1, 0.0, 1.0);
                surface_color = mix(surface_color, snow, snow_blend);
            }

            // Beach at coastline
            if (land_height < 0.03) {
                let beach = vec3<f32>(0.76, 0.70, 0.50);
                let beach_blend = 1.0 - clamp(land_height / 0.03, 0.0, 1.0);
                surface_color = mix(surface_color, beach, beach_blend * 0.7);
            }
        }
    }

    // Lighting
    let light = normalize(uniforms.light_dir);
    let ndotl = max(dot(normal, light), 0.0);
    let ambient = 0.15;
    let lit_color = surface_color * (ambient + (1.0 - ambient) * ndotl);

    return vec4<f32>(lit_color, 1.0);
}
