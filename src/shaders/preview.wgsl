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
    tectonics_factor: f32, // 0.0 = stagnant lid (unimodal), 1.0 = plate tectonics (bimodal)
    continental_scale: f32, // multiplier for continental noise frequency (lower = bigger continents)
    view_mode: u32, // 0=normal, 1=height, 2=temperature, 3=moisture, 4=biome, 5=ocean/ice
    _pad0: f32,
    _pad1: f32,
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

fn smooth_step(edge0: f32, edge1: f32, x: f32) -> f32 {
    let t = clamp((x - edge0) / (edge1 - edge0), 0.0, 1.0);
    return t * t * (3.0 - 2.0 * t);
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

// ---- Continental structure (research section 7.3) ----
// Layer 1: Low-frequency noise creates continent-scale landmasses
// Layer 2: Detail fBm adds terrain features on top

fn continental_base(pos: vec3<f32>) -> f32 {
    let p = pos + uniforms.seed_offset;

    // Domain warping: offset sample position by another noise field
    // Creates geological-looking curved ridges, winding coastlines, bays & peninsulas
    let warp = vec3<f32>(
        snoise(p * 1.5 + vec3<f32>(31.7, 0.0, 0.0)),
        snoise(p * 1.5 + vec3<f32>(0.0, 47.3, 0.0)),
        snoise(p * 1.5 + vec3<f32>(0.0, 0.0, 73.1))
    ) * 0.35;
    let wp = p + warp; // warped position

    // Low-frequency noise for continent shapes (freq ≈ 2-3 per research)
    let cs = uniforms.continental_scale;
    let n1 = snoise(wp * 2.0 * cs);
    let n2 = snoise(wp * 3.5 * cs + vec3<f32>(50.0, 0.0, 0.0)) * 0.5;
    let n3 = snoise(wp * 5.0 * cs + vec3<f32>(0.0, 50.0, 0.0)) * 0.25;
    var continental = n1 + n2 + n3;

    // Plate tectonics → bimodal elevation (distinct continents + ocean basins)
    // Earth has two elevation modes: ocean floor (~-4km) and continental shelves (~0-1km)
    let bimodal = uniforms.tectonics_factor;

    // Step 1: Sharpen the land/ocean boundary
    let sharpened = sign(continental) * pow(abs(continental), 0.5);
    continental = mix(continental, sharpened, bimodal * 0.6);

    // Step 2: Flatten plateaus (continental shelves and ocean floors)
    // Land areas get a raised plateau; ocean floors get a depressed basin
    // This creates the bimodal elevation distribution like Earth
    if (continental > 0.0) {
        // Continental shelf: flatten above threshold, add slight plateau
        let shelf = clamp(continental, 0.0, 1.5);
        let plateau = 0.3 + shelf * 0.5; // Continent base elevation
        continental = mix(continental, plateau, bimodal * 0.5);
    } else {
        // Ocean floor: flatten below threshold, depress to basin
        let basin = clamp(continental, -1.5, 0.0);
        let floor_level = -0.4 + basin * 0.3; // Ocean floor depression
        continental = mix(continental, floor_level, bimodal * 0.5);
    }

    // Step 3: Add continental edge detail (coastline irregularity)
    let edge_dist = abs(continental);
    if (edge_dist < 0.3) {
        let edge_noise = snoise(p * 12.0) * 0.15 + snoise(p * 20.0) * 0.08;
        continental += edge_noise * (1.0 - edge_dist / 0.3);
    }

    return continental;
}

fn detail_fbm(pos: vec3<f32>) -> f32 {
    var value = 0.0;
    var freq = uniforms.frequency;
    var amp = uniforms.amplitude * 0.3; // Detail is smaller than continental scale
    let p = pos + uniforms.seed_offset;

    for (var i = 0u; i < uniforms.octaves; i++) {
        value += amp * snoise(p * freq);
        freq *= uniforms.lacunarity;
        amp *= uniforms.gain;
    }
    return value;
}

fn fbm_preview(pos: vec3<f32>) -> f32 {
    // Continental base layer (large-scale landmass structure)
    let continental = continental_base(pos) * uniforms.amplitude;

    // Detail terrain on top — elevation-dependent roughness (R9)
    let detail = detail_fbm(pos);

    // Mountains are craggy (more detail), lowlands are smoother (erosion fills valleys)
    var detail_scale: f32;
    if (continental > 0.2) {
        detail_scale = 1.0 + continental * 0.5; // Mountains: up to 1.75x detail
    } else if (continental < -0.1) {
        detail_scale = 0.3; // Ocean floor: mostly smooth
    } else {
        detail_scale = 0.6; // Plains: moderate
    }

    return continental + detail * detail_scale;
}

// ---- Temperature (research section 13.2) ----
// Research: baseTemp = 30 - abs(latitude) * 60 for Earth
// We shift the entire curve by the planet's average temperature offset from Earth

fn compute_temperature(sphere_pos: vec3<f32>, height: f32) -> f32 {
    let latitude = asin(clamp(sphere_pos.y, -1.0, 1.0)); // radians

    // Apply axial tilt: shift effective latitude based on longitude and tilt
    // This creates asymmetric heating patterns (seasons frozen in time)
    let longitude = atan2(sphere_pos.z, sphere_pos.x);
    let tilt_effect = sin(longitude) * uniforms.axial_tilt_rad;
    let effective_lat = clamp(latitude + tilt_effect, -1.5708, 1.5708);
    let lat_deg = abs(effective_lat) * 180.0 / 3.14159;

    // Earth baseline: equator 30°C, poles -30°C (range = 60°C)
    // For other planets: shift the whole curve by (planet_avg - earth_avg)
    // Earth avg ≈ 15°C
    let temp_offset = uniforms.base_temp_c - 15.0;
    let base_temp = 30.0 - lat_deg * (60.0 / 90.0) + temp_offset;

    // Lapse rate: -6.5°C per km elevation
    // Normalized height [0, 1] above sea level → ~0-5 km range
    let land_fraction = max(height - uniforms.ocean_level, 0.0) / max(1.0 - uniforms.ocean_level, 0.01);
    let elevation_km = land_fraction * 5.0;
    let lapse = -6.5 * elevation_km;

    // Small noise variation (±3°C)
    let temp_noise = snoise(sphere_pos * 3.0 + uniforms.seed_offset * 0.5) * 3.0;

    return base_temp + lapse + temp_noise;
}

// ---- Hadley cell atmospheric circulation (R1-R4) ----
// Three cells per hemisphere determine surface wind direction and moisture tendency.

fn hadley_cell_moisture(latitude_rad: f32) -> f32 {
    // Three-cell model: moisture peaks at equator (ITCZ) and ~60° (polar front)
    // Dry at ~30° (subtropical high) and poles
    // Using sin-based approximation of the three-cell pattern
    let lat = abs(latitude_rad);
    let lat_deg = lat * 180.0 / 3.14159;

    // ITCZ (0-10°): very wet (rising air)
    // Subtropical high (25-35°): dry (sinking air) — desert belt
    // Mid-latitude (40-55°): moderately wet (polar front)
    // Polar (65-90°): dry (cold air holds little moisture)

    // Smooth curve: wet-dry-wet-dry from equator to pole
    let itcz_wet = exp(-lat_deg * lat_deg / 200.0) * 250.0; // Peak at equator
    let subtropical_dry = -150.0 * exp(-((lat_deg - 30.0) * (lat_deg - 30.0)) / 100.0); // Trough at 30°
    let polar_front_wet = 80.0 * exp(-((lat_deg - 55.0) * (lat_deg - 55.0)) / 150.0); // Bump at 55°
    let polar_dry = -100.0 * smooth_step(65.0, 85.0, lat_deg); // Drop at poles

    return max(itcz_wet + subtropical_dry + polar_front_wet + polar_dry + 50.0, 5.0);
}

fn wind_direction(latitude_rad: f32) -> vec2<f32> {
    // Surface wind direction from Hadley cells:
    // 0-30°: trade winds (easterly) — wind from east
    // 30-60°: westerlies — wind from west
    // 60-90°: polar easterlies — wind from east
    let lat_deg = abs(latitude_rad) * 180.0 / 3.14159;

    if (lat_deg < 30.0) {
        return vec2<f32>(-1.0, 0.0); // Easterly (from east)
    } else if (lat_deg < 60.0) {
        return vec2<f32>(1.0, 0.0); // Westerly (from west)
    } else {
        return vec2<f32>(-1.0, 0.0); // Polar easterly
    }
}

// ---- Moisture with Hadley cells + rain shadow (R1-R6) ----

fn compute_moisture(sphere_pos: vec3<f32>, height: f32) -> f32 {
    let latitude = asin(clamp(sphere_pos.y, -1.0, 1.0));

    // Apply tilt to shift ITCZ and circulation bands (R4)
    let longitude = atan2(sphere_pos.z, sphere_pos.x);
    let tilt_effect = sin(longitude) * uniforms.axial_tilt_rad;
    let effective_lat = clamp(latitude + tilt_effect, -1.5708, 1.5708);

    // Hadley cell sets the circulation tendency (R1-R3)
    let hadley_base = hadley_cell_moisture(effective_lat);

    // Local factors create the actual variation — terrain, noise, and geography
    // break the latitude bands so biomes aren't just horizontal stripes
    let moisture_noise1 = snoise(sphere_pos * 3.0 + uniforms.seed_offset * 1.7 + vec3<f32>(100.0, 0.0, 0.0));
    let moisture_noise2 = snoise(sphere_pos * 6.0 + uniforms.seed_offset * 2.3 + vec3<f32>(0.0, 80.0, 0.0)) * 0.5;
    let local_variation = (moisture_noise1 + moisture_noise2) * 0.5; // [-0.75, 0.75]

    // Blend: Hadley tendency (60%) + local variation (40%)
    var moisture = hadley_base * (0.6 + 0.4 * (local_variation * 0.5 + 0.5));
    // Add absolute noise contribution to create local wet/dry patches
    moisture += 60.0 * (local_variation * 0.5 + 0.5);

    // Ocean proximity / continentality effect (R6)
    // Sample height at a few offsets to estimate distance from ocean
    let is_land = height > uniforms.ocean_level;
    if (is_land) {
        // Check neighbors: how many nearby points are also land?
        let step = 0.08;
        var land_count = 0.0;
        let h1 = fbm_preview(sphere_pos + vec3<f32>(step, 0.0, 0.0));
        let h2 = fbm_preview(sphere_pos + vec3<f32>(-step, 0.0, 0.0));
        let h3 = fbm_preview(sphere_pos + vec3<f32>(0.0, step, 0.0));
        let h4 = fbm_preview(sphere_pos + vec3<f32>(0.0, 0.0, step));
        let norm_h = uniforms.amplitude * 1.5;
        if (h1 / norm_h > uniforms.ocean_level) { land_count += 1.0; }
        if (h2 / norm_h > uniforms.ocean_level) { land_count += 1.0; }
        if (h3 / norm_h > uniforms.ocean_level) { land_count += 1.0; }
        if (h4 / norm_h > uniforms.ocean_level) { land_count += 1.0; }
        // More land neighbors = more continental = drier
        let continentality = land_count / 4.0;
        moisture *= 1.0 - continentality * 0.4;
    } else {
        // Over ocean: moisture is high
        moisture *= 1.3;
    }

    // Rain shadow from wind-terrain interaction (R5)
    let wind = wind_direction(effective_lat);
    // Sample upwind terrain height
    let upwind_offset = normalize(vec3<f32>(wind.x, 0.0, wind.y)) * 0.06;
    let upwind_h = fbm_preview(sphere_pos + upwind_offset) / (uniforms.amplitude * 1.5);
    let upwind_land = max(upwind_h - uniforms.ocean_level, 0.0) / max(1.0 - uniforms.ocean_level, 0.01);
    // If upwind terrain is high, this is the leeward side → dry
    let rain_shadow = 1.0 - 0.5 * smooth_step(0.2, 0.6, upwind_land);
    moisture *= rain_shadow;

    // Global moisture scales with ocean coverage
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
        return vec4<f32>(0.0, 0.0, 0.0, 1.0); // Space background
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
        // Check for polar ocean ice (seawater freezes at ~-2°C)
        let ocean_temp = compute_temperature(rotated, height);
        if (ocean_temp < -2.0) {
            // Frozen ocean — sea ice
            let ice_intensity = clamp((-2.0 - ocean_temp) / 10.0, 0.0, 1.0);
            let sea_ice = vec3<f32>(0.85, 0.90, 0.95);
            let thin_ice = vec3<f32>(0.55, 0.70, 0.82);
            surface_color = mix(thin_ice, sea_ice, ice_intensity);
            surface_color += vec3<f32>(0.02) * color_var;
        } else if (ocean_temp < 2.0) {
            // Transition zone: mix between ice and water
            let blend = (ocean_temp + 2.0) / 4.0;
            let ice_color = vec3<f32>(0.55, 0.70, 0.82);
            let depth = clamp((uniforms.ocean_level - height) / max(uniforms.ocean_level + 1.0, 0.5), 0.0, 1.0);
            let water = mix(vec3<f32>(0.10, 0.35, 0.42), vec3<f32>(0.02, 0.05, 0.20), smoothstep(0.0, 0.5, depth));
            surface_color = mix(ice_color, water, blend);
        } else {
            // Open ocean — 3-stop depth gradient: turquoise shelf → mid blue → deep navy
            let depth = clamp((uniforms.ocean_level - height) / max(uniforms.ocean_level + 1.0, 0.5), 0.0, 1.0);
            let near_shore = vec3<f32>(0.10, 0.35, 0.42);
            let mid_ocean  = vec3<f32>(0.06, 0.18, 0.42);
            let deep_ocean = vec3<f32>(0.02, 0.05, 0.20);
            let shelf = smoothstep(0.0, 0.15, depth);
            let abyss = smoothstep(0.15, 0.7, depth);
            surface_color = mix(near_shore, mix(mid_ocean, deep_ocean, abyss), shelf);
            surface_color += vec3<f32>(0.0, 0.02, 0.03) * color_var;
        }
    } else {
        // Compute effective latitude for altitude zonation and temperature
        let latitude = asin(clamp(rotated.y, -1.0, 1.0));
        let longitude = atan2(rotated.z, rotated.x);
        let tilt_shift = sin(longitude) * uniforms.axial_tilt_rad;
        let effective_lat = clamp(latitude + tilt_shift, -1.5708, 1.5708);

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

            // 6. Base biome color
            surface_color = biome_color(biome, color_var);

            // 7. Altitude zonation (R7) — progressive transitions up the mountain
            let land_height = (height - uniforms.ocean_level) / max(1.0 - uniforms.ocean_level, 0.01);

            // Snow line varies with latitude: lower at poles, higher at equator
            let snow_line = 0.65 + 0.25 * (1.0 - abs(effective_lat) / 1.5708);
            let rock_line = snow_line - 0.15;
            let alpine_line = rock_line - 0.15;

            if (land_height > snow_line && temp < 15.0) {
                // Permanent snow
                let snow = vec3<f32>(0.94, 0.96, 1.0);
                let blend = smooth_step(snow_line, snow_line + 0.08, land_height);
                surface_color = mix(surface_color, snow, blend);
            } else if (land_height > rock_line) {
                // Bare rock / scree
                let rock = vec3<f32>(0.48, 0.46, 0.44) + vec3<f32>(0.05) * color_var;
                let blend = smooth_step(rock_line, rock_line + 0.08, land_height);
                surface_color = mix(surface_color, rock, blend);
            } else if (land_height > alpine_line) {
                // Alpine meadow — lighter green/brown
                let alpine = vec3<f32>(0.45, 0.50, 0.30) + vec3<f32>(0.04) * color_var;
                let blend = smooth_step(alpine_line, alpine_line + 0.08, land_height);
                surface_color = mix(surface_color, alpine, blend * 0.7);
            }

            // Beach at coastline
            if (land_height < 0.03) {
                let beach = vec3<f32>(0.76, 0.70, 0.50);
                let beach_blend = 1.0 - clamp(land_height / 0.03, 0.0, 1.0);
                surface_color = mix(surface_color, beach, beach_blend * 0.7);
            }
        }
    }

    // Debug view modes
    if (uniforms.view_mode > 0u) {
        let temp = compute_temperature(rotated, height);
        let moisture = compute_moisture(rotated, height);

        var debug_color: vec3<f32>;

        switch (uniforms.view_mode) {
            case 1u: {
                // Height: black (low) → white (high)
                let h = (height + 1.0) * 0.5; // map [-1,1] to [0,1]
                debug_color = vec3<f32>(h, h, h);
            }
            case 2u: {
                // Temperature: blue (cold) → white (0°C) → red (hot)
                let t = clamp(temp / 50.0, -1.0, 1.0); // normalize to [-1,1]
                if (t < 0.0) {
                    debug_color = mix(vec3<f32>(0.0, 0.0, 1.0), vec3<f32>(1.0, 1.0, 1.0), t + 1.0);
                } else {
                    debug_color = mix(vec3<f32>(1.0, 1.0, 1.0), vec3<f32>(1.0, 0.0, 0.0), t);
                }
            }
            case 3u: {
                // Moisture: brown (dry) → green (wet) → blue (very wet)
                let m = clamp(moisture / 300.0, 0.0, 1.0);
                if (m < 0.5) {
                    debug_color = mix(vec3<f32>(0.6, 0.4, 0.1), vec3<f32>(0.1, 0.6, 0.1), m * 2.0);
                } else {
                    debug_color = mix(vec3<f32>(0.1, 0.6, 0.1), vec3<f32>(0.1, 0.2, 0.8), (m - 0.5) * 2.0);
                }
            }
            case 4u: {
                // Biome: distinct color per biome ID
                let biome = whittaker_lookup(temp, moisture);
                debug_color = biome_color(biome, 0.0);
                // Boost saturation for visibility
                debug_color = debug_color * 1.3;
            }
            case 5u: {
                // Ocean/Ice mask: blue=ocean, white=ice, green=land, brown=high land
                if (is_ocean) {
                    let ocean_temp = compute_temperature(rotated, height);
                    if (ocean_temp < -2.0) {
                        debug_color = vec3<f32>(1.0, 1.0, 1.0); // Ice
                    } else {
                        debug_color = vec3<f32>(0.0, 0.2, 0.8); // Ocean
                    }
                } else {
                    let land_h = (height - uniforms.ocean_level) / max(1.0 - uniforms.ocean_level, 0.01);
                    if (temp < -15.0) {
                        debug_color = vec3<f32>(0.9, 0.95, 1.0); // Land ice
                    } else {
                        debug_color = mix(vec3<f32>(0.2, 0.6, 0.1), vec3<f32>(0.5, 0.3, 0.1), clamp(land_h, 0.0, 1.0));
                    }
                }
            }
            default: {
                debug_color = surface_color;
            }
        }

        return vec4<f32>(debug_color, 1.0);
    }

    // Normal view: apply lighting
    let light = normalize(uniforms.light_dir);
    let ndotl = max(dot(normal, light), 0.0);
    let ambient = 0.15;
    let lit_color = surface_color * (ambient + (1.0 - ambient) * ndotl);

    return vec4<f32>(lit_color, 1.0);
}
