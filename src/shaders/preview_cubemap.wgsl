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
    atmosphere_density: f32, // 0.0 = none, 1.0 = Earth-like (reserved)
    atmosphere_height: f32,  // scale height in planet radii (reserved)
    height_scale: f32,       // normal map height exaggeration
    zoom: f32,               // viewport zoom (1.0 = default)
    pan_x: f32,              // viewport pan in NDC units
    pan_y: f32,
    cloud_coverage: f32,     // 0.0 = clear, 1.0 = overcast
    cloud_seed: f32,
    cloud_altitude: f32,
    cloud_type: f32,         // 0.0 = smooth stratus, 1.0 = puffy cumulus
    storm_count: f32,        // 0-8 cyclone systems
    storm_size: f32,         // storm radius multiplier
    night_lights: f32,       // 0.0 = pristine, 1.0 = urbanized
    star_color_temp: f32,    // 0.0 = blue, 0.5 = sun, 1.0 = red dwarf
    city_light_hue: f32,    // 0.0 = warm amber, 0.5 = white, 1.0 = cool blue
    show_ao: f32,           // 1.0 = enabled, 0.0 = disabled
    // Layer toggles (1.0 = enabled, 0.0 = disabled)
    show_water: f32,
    show_ice: f32,
    show_biomes: f32,
    show_clouds: f32,
    show_atmosphere_layer: f32,
    show_cities: f32,
    cloud_opacity: f32,    // 0.0 = transparent, 1.0 = full opacity
}

@group(0) @binding(0) var<uniform> uniforms: Uniforms;
@group(0) @binding(1) var height_tex: texture_cube<f32>;
@group(0) @binding(2) var height_sampler: sampler;

struct VertexOutput {
    @builtin(position) position: vec4<f32>,
    @location(0) uv: vec2<f32>,
}

// Star color from temperature slider: 0=blue O-star, 0.5=sun G-star, 1.0=red M-dwarf
fn star_color(temp: f32) -> vec3<f32> {
    // Blue (O/B) → White (A/F) → Yellow (G) → Orange (K) → Red (M)
    let blue = vec3<f32>(0.6, 0.7, 1.0);
    let white = vec3<f32>(1.0, 1.0, 1.0);
    let yellow = vec3<f32>(1.0, 0.95, 0.85);
    let orange = vec3<f32>(1.0, 0.75, 0.5);
    let red = vec3<f32>(1.0, 0.5, 0.3);

    if (temp < 0.25) {
        return mix(blue, white, temp * 4.0);
    } else if (temp < 0.5) {
        return mix(white, yellow, (temp - 0.25) * 4.0);
    } else if (temp < 0.75) {
        return mix(yellow, orange, (temp - 0.5) * 4.0);
    }
    return mix(orange, red, (temp - 0.75) * 4.0);
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

// ---- Ray-marched atmosphere ----

struct ScatterResult {
    in_scatter: vec3<f32>,
    transmittance: vec3<f32>,
}

// Henyey-Greenstein phase function for Mie scattering.
// g > 0: strong forward scattering (bright glow around sun).
fn henyey_greenstein(cos_theta: f32, g: f32) -> f32 {
    let g2 = g * g;
    return (1.0 - g2) / (4.0 * 3.14159 * pow(1.0 + g2 - 2.0 * g * cos_theta, 1.5));
}

fn ray_march_atmosphere(
    ndc: vec2<f32>,
    z_start: f32,
    z_end: f32,
    sun_dir: vec3<f32>,
) -> ScatterResult {
    let atm_density = uniforms.atmosphere_density;

    // Rayleigh: wavelength-dependent (λ^-4), blue sky + red sunsets
    let beta_r = vec3<f32>(1.0, 2.4, 5.8) * atm_density;
    let scale_h_r = max(uniforms.atmosphere_height * 0.5, 0.003);

    // Mie: wavelength-independent (white/gray haze), concentrated near surface
    let beta_m = vec3<f32>(1.0, 1.0, 1.0) * atm_density * 0.35;
    let scale_h_m = scale_h_r * 0.25; // lower scale height — haze near ground

    // Phase functions
    let cos_theta = sun_dir.z; // angle between sun and view direction (0,0,1)
    let phase_r = 0.05968 * (1.0 + cos_theta * cos_theta); // Rayleigh: 3/(16π)
    let phase_m = henyey_greenstein(cos_theta, 0.76);        // Mie: forward-peaked

    let steps = 8;
    let step_len = (z_start - z_end) / f32(steps);

    var optical_depth = vec3<f32>(0.0);
    var in_scatter = vec3<f32>(0.0);

    for (var i = 0; i < steps; i++) {
        let z = z_start - (f32(i) + 0.5) * step_len;
        let pos = vec3<f32>(ndc.x, ndc.y, z);
        let altitude = length(pos) - 1.0;

        if (altitude < 0.0) { continue; }

        let density_r = exp(-altitude / scale_h_r);
        let density_m = exp(-altitude / scale_h_m);

        // Combined optical depth (extinction)
        let ext = beta_r * density_r + beta_m * density_m;
        optical_depth += ext * abs(step_len);

        // Sun illumination: smooth day/night transition at terminator
        let raw_sun_cos = dot(normalize(pos), sun_dir);
        let sun_factor = smoothstep(-0.1, 0.2, raw_sun_cos);

        if (sun_factor > 0.001) {
            let sun_od_r = beta_r * density_r * scale_h_r / max(raw_sun_cos, 0.12);
            let sun_od_m = beta_m * density_m * scale_h_m / max(raw_sun_cos, 0.12);
            let view_transmit = exp(-optical_depth);
            let sun_transmit = exp(-(sun_od_r + sun_od_m));

            // Rayleigh + Mie in-scatter combined
            let scatter_r = density_r * phase_r * beta_r;
            let scatter_m = density_m * phase_m * beta_m;
            in_scatter += view_transmit * sun_transmit * sun_factor * (scatter_r + scatter_m) * abs(step_len);
        }
    }

    var result: ScatterResult;
    result.in_scatter = in_scatter * 25.0;
    result.transmittance = exp(-optical_depth);
    return result;
}

// ---- Temperature ----
fn compute_temperature(sphere_pos: vec3<f32>, height: f32, season: f32) -> f32 {
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

    // Sub-solar latitude: shifts the thermal equator with season and tilt.
    // At equinox (season=0.5): sub-solar at equator. At solstice: sub-solar at ±tilt.
    // At 90° tilt + summer: the north pole is the hottest point (Uranus-like).
    let season_angle = (season - 0.5) * 2.0; // [-1, 1]
    let sub_solar_lat = uniforms.axial_tilt_rad * season_angle;

    // Thermal latitude: angular distance from the sub-solar point
    let thermal_lat = effective_lat - sub_solar_lat;
    let thermal_deg = min(abs(thermal_lat) * 180.0 / 3.14159, 90.0);

    // Temperature gradient: ~50°C range from thermal equator to thermal poles
    let lat_normalized = thermal_deg / 90.0;
    let temp_drop = 50.0 * (0.4 * lat_normalized + 0.6 * lat_normalized * lat_normalized);
    let temp_offset = uniforms.base_temp_c - 15.0;

    let base_temp = 30.0 - temp_drop + temp_offset;

    let land_fraction = max(height - uniforms.ocean_level, 0.0) / max(1.0 - uniforms.ocean_level, 0.01);
    let elevation_km = land_fraction * 5.0;
    let lapse = -6.5 * elevation_km;

    let temp_noise = snoise(sphere_pos * 3.0) * 3.0;

    // Regional temperature character: low-freq noise gives some regions warmer/cooler personality
    let region_temp_bias = snoise(sphere_pos * 0.6 + vec3<f32>(0.0, 400.0, 0.0)) * 4.0; // ±4°C

    // === Ocean current approximation ===
    // Western ocean coasts get warm poleward currents (Gulf Stream, Kuroshio)
    // Eastern ocean coasts get cold equatorward currents (California, Benguela)
    // Approximated from wind direction: wind pushes surface water, Coriolis deflects it
    var current_temp = 0.0;
    let is_ocean = height < uniforms.ocean_level;
    if (is_ocean) {
        let wind = wind_direction_vec(effective_lat);
        let tangent_wind = normalize(wind - sphere_pos * dot(wind, sphere_pos));

        // Check for nearby land to determine ocean basin margins
        // Use poleward-perpendicular sampling (not wind direction) for E/W coast detection
        // On a sphere, "east" is cross(north, position) and "west" is the opposite
        let north_pole = vec3<f32>(0.0, 1.0, 0.0);
        let east_dir = normalize(cross(north_pole, sphere_pos));
        let coast_step = 0.08;
        let h_east = textureSample(height_tex, height_sampler, normalize(sphere_pos + east_dir * coast_step)).r;
        let h_west = textureSample(height_tex, height_sampler, normalize(sphere_pos - east_dir * coast_step)).r;
        let land_to_east = h_east > uniforms.ocean_level; // continent to the east
        let land_to_west = h_west > uniforms.ocean_level; // continent to the west

        let season_angle = (uniforms.season - 0.5) * 2.0;
        let winter_boost = 1.0 + clamp(-effective_lat * season_angle * 2.0, 0.0, 0.5);
        let lat_strength = 1.0 - abs(lat_normalized);

        // Western boundary current (Gulf Stream): warm, EAST coast of continent
        // = western side of ocean basin, land is to the WEST
        if (land_to_west) {
            current_temp = 4.0 * lat_strength * winter_boost;
        }
        // Eastern boundary current (California): cold upwelling, WEST coast of continent
        // = eastern side of ocean basin, land is to the EAST
        if (land_to_east) {
            current_temp = max(current_temp, 0.0) - 3.0 * lat_strength * winter_boost;
        }
    }

    return base_temp + lapse + temp_noise + region_temp_bias + current_temp;
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

// Wind direction from Hadley/Ferrel/Polar cells — smooth transitions, Coriolis curvature
// Cell boundaries shift with thermal equator (seasonal migration)
fn wind_direction_vec(latitude_rad: f32) -> vec3<f32> {
    let hemisphere = sign(latitude_rad + 0.0001);

    // Seasonal shift: thermal equator moves with sub-solar point
    // Summer hemisphere trades expand 5-10°, winter contracts
    let season_shift = uniforms.axial_tilt_rad * ((uniforms.season - 0.5) * 2.0) * 0.4;
    let shifted_lat = latitude_rad - season_shift; // shift cells toward summer hemisphere
    let lat_deg = abs(shifted_lat) * 180.0 / 3.14159;

    // Three-cell zonal wind with smooth_step blending
    let trade = (1.0 - smooth_step(20.0, 35.0, lat_deg)) * -0.8;
    let westerly = smooth_step(25.0, 40.0, lat_deg) * (1.0 - smooth_step(55.0, 70.0, lat_deg)) * 0.9;
    let polar_east = smooth_step(60.0, 75.0, lat_deg) * -0.5;
    let wind_x = trade + westerly + polar_east;

    // Coriolis-deflected meridional flow
    let hadley_meridional = -smooth_step(5.0, 25.0, lat_deg) * (1.0 - smooth_step(25.0, 35.0, lat_deg)) * 0.4;
    let ferrel_meridional = smooth_step(35.0, 45.0, lat_deg) * (1.0 - smooth_step(55.0, 65.0, lat_deg)) * 0.3;
    let wind_y = (hadley_meridional + ferrel_meridional) * hemisphere;

    return normalize(vec3<f32>(wind_x, wind_y, 0.15));
}

// Enhanced wind with terrain deflection — samples heightmap to bend flow around mountains
fn wind_direction_at(sphere_pos: vec3<f32>, latitude_rad: f32) -> vec3<f32> {
    var wind = wind_direction_vec(latitude_rad);
    // Project to sphere tangent plane
    let tangent_wind = normalize(wind - sphere_pos * dot(wind, sphere_pos));

    // Terrain deflection: sample height gradient perpendicular to wind
    let perp = normalize(cross(sphere_pos, tangent_wind));
    let step_d = 0.04;
    let h_left = textureSample(height_tex, height_sampler, normalize(sphere_pos + perp * step_d)).r;
    let h_right = textureSample(height_tex, height_sampler, normalize(sphere_pos - perp * step_d)).r;
    let terrain_gradient = (h_left - h_right) * 3.0; // how much terrain slopes across wind path

    // Wind deflects away from high terrain (flows around mountains, not through them)
    let deflected = normalize(tangent_wind + perp * clamp(terrain_gradient, -0.4, 0.4));
    return deflected;
}

fn compute_moisture(sphere_pos: vec3<f32>, height: f32, season: f32) -> f32 {
    let tilt = uniforms.axial_tilt_rad;
    let tilted_y = sphere_pos.y * cos(tilt) + sphere_pos.z * sin(tilt);
    let effective_lat = asin(clamp(tilted_y, -1.0, 1.0));

    // Shift Hadley cells with thermal equator (same sub-solar shift as temperature)
    let season_angle = (season - 0.5) * 2.0;
    let sub_solar_lat = tilt * season_angle;
    let thermal_lat = effective_lat - sub_solar_lat;

    // Hadley cell base moisture — scaled by ocean fraction FIRST.
    // Softened ocean scaling: low-water worlds still get some moisture (was 0.05 + 0.95*)
    let ocean_scale = 0.25 + 0.75 * uniforms.ocean_fraction;
    let hadley_base = hadley_cell_moisture(thermal_lat) * ocean_scale;

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
    // Rain shadow + interior drying (2 extra texture reads, reuses neighbor data from above)
    if (is_land) {
        let wind = wind_direction_vec(effective_lat);
        let tangent_wind = normalize(wind - sphere_pos * dot(wind, sphere_pos));

        // Single upwind sample at moderate distance (replaces 3-step loop)
        let upwind_pos = normalize(sphere_pos + tangent_wind * 0.08);
        let upwind_h = textureSample(height_tex, height_sampler, upwind_pos).r;
        let upwind_elev = max(upwind_h - uniforms.ocean_level, 0.0);
        let my_elevation = max(height - uniforms.ocean_level, 0.0);

        if (upwind_elev > my_elevation + 0.03) {
            let shadow_strength = clamp((upwind_elev - my_elevation) * 5.0, 0.0, 0.7);
            moisture *= (1.0 - shadow_strength);
        }

        // Interior drying: reuse the 4 neighbor samples already fetched for continentality
        // ocean_count is from the near step (0.06), add one far sample for depth
        let far_pos = normalize(sphere_pos + tangent_wind * 0.15);
        let h_far = textureSample(height_tex, height_sampler, far_pos).r;
        let far_is_land = select(0.0, 1.0, h_far > uniforms.ocean_level);
        let deep_interior = (1.0 - ocean_count / 4.0) * far_is_land;
        moisture *= 1.0 - deep_interior * 0.3;
    }

    // === Regional moisture character ===
    // Low-frequency noise gives each region a wet or dry personality.
    // This creates "jungle continents" vs "desert continents" at similar latitudes.
    let region_moisture_bias = snoise(sphere_pos * 0.7 + vec3<f32>(500.0, 0.0, 0.0));
    moisture *= 1.0 + region_moisture_bias * 0.25; // ±25% regional variation

    moisture *= 0.5 + uniforms.ocean_fraction;
    return clamp(moisture, 0.0, 400.0);
}

// ---- Cloud density (Schneider remap + domain-warped fBm) ----
// Based on HZD/Quilez/Skybolt research. See docs/research/cloud-layer-rendering.md
//
// Key technique: climate controls the coverage THRESHOLD, not density amplitude.
// This prevents latitude banding while keeping climate-correlated placement.

fn cloud_remap(value: f32, old_min: f32, old_max: f32, new_min: f32, new_max: f32) -> f32 {
    return new_min + (clamp(value, old_min, old_max) - old_min)
           / max(old_max - old_min, 0.001) * (new_max - new_min);
}

fn compute_cloud_density(sphere_pos: vec3<f32>, height: f32) -> f32 {
    let cov_slider = uniforms.cloud_coverage;
    if (cov_slider <= 0.0) { return 0.0; }

    // Linearize slider response (slight expansion at low end)
    let coverage = pow(cov_slider, 0.8);

    // Seed offset: pre-hashed on CPU via seed_to_offset() → always in [0, 97)
    let s = uniforms.cloud_seed;
    let seed_off = vec3<f32>(s, fract(s * 1.618) * 89.0, fract(s * 2.618) * 83.0);

    // === Step 0b: Vortex warp — storms twist the noise field ===
    // Rotate sample position around storm centers so cloud texture itself spirals.
    var vortex_sphere = sphere_pos;
    let n_storms_warp = i32(min(uniforms.storm_count, 8.0));
    if (n_storms_warp > 0) {
        for (var i = 0; i < 8; i++) {
            if (i >= n_storms_warp) { break; }
            let fi = f32(i);
            let slat = (30.0 + fract(sin(fi * 127.1 + s) * 43758.5) * 25.0) * 3.14159 / 180.0;
            let slon = fract(sin(fi * 311.7 + s * 1.3) * 23421.6) * 6.28318;
            let sign_y = select(-1.0, 1.0, i % 2 == 0);
            let raw_center = vec3<f32>(
                cos(slat) * cos(slon), sin(slat) * sign_y, cos(slat) * sin(slon)
            );
            // Rotate storm center by axial tilt so storms track tilted circulation
            let ct = cos(uniforms.axial_tilt_rad);
            let st = sin(uniforms.axial_tilt_rad);
            let center = normalize(vec3<f32>(
                raw_center.x,
                raw_center.y * ct - raw_center.z * st,
                raw_center.y * st + raw_center.z * ct
            ));
            let d = acos(clamp(dot(sphere_pos, center), -1.0, 1.0));
            // Per-storm unique size: 0.5x-2.0x of base, tropical storms tighter
            let per_storm_size = 0.5 + fract(sin(fi * 73.1 + s * 0.7) * 19283.3) * 1.5;
            let lat_tightness = mix(1.0, 1.5, smooth_step(15.0, 35.0, slat * 180.0 / 3.14159));
            let storm_sigma = (18.0 + per_storm_size * 10.0) * lat_tightness / max(uniforms.storm_size * uniforms.storm_size, 0.1);
            let influence = exp(-d * d * storm_sigma);

            // Tangent rotation: reduced strength to avoid over-stretching noise
            let rotation_amount = influence * sign_y * (1.5 + per_storm_size * 0.5) / max(d * 6.0, 0.3);
            let tangent = cross(center, sphere_pos);
            vortex_sphere = normalize(vortex_sphere + tangent * rotation_amount * 0.02);
        }
    }

    // === Step 0c: Subtle wind alignment — gentle stretch, not aggressive warp ===
    let tilt_c = uniforms.axial_tilt_rad;
    let tilted_y_c = vortex_sphere.y * cos(tilt_c) + vortex_sphere.z * sin(tilt_c);
    let cloud_lat = asin(clamp(tilted_y_c, -1.0, 1.0));
    let cloud_lat_deg = abs(cloud_lat) * 180.0 / 3.14159;

    // === Cloud type REGION selector — low-freq noise creates distinct zones ===
    // region_type: 0 = thin/wispy, 0.5 = stratus sheets, 1.0 = puffy cumulus
    let region_raw = snoise(vortex_sphere * 0.8 + seed_off * 0.2 + vec3<f32>(131.0, 71.0, 0.0));
    let itcz_factor = exp(-cloud_lat_deg * cloud_lat_deg / 150.0);
    let polar_c = smooth_step(55.0, 70.0, cloud_lat_deg);
    let lat_type_bias = itcz_factor * 0.4 - polar_c * 0.3
        - smooth_step(15.0, 28.0, cloud_lat_deg) * smooth_step(38.0, 28.0, cloud_lat_deg) * 0.3;
    let region_type = clamp(region_raw * 0.4 + 0.5 + lat_type_bias + uniforms.cloud_type * 0.2, 0.0, 1.0);

    // === Step 1: Three fundamentally different noise patterns ===
    let p_base = vortex_sphere * 7.0 + seed_off;

    // Subtle wind-aligned bias in domain warp (clouds elongate along flow)
    let wind_cv = wind_direction_vec(cloud_lat);
    let tangent_cv = normalize(wind_cv - vortex_sphere * dot(wind_cv, vortex_sphere));
    let wind_bias = tangent_cv * 0.06; // gentle directional component

    // --- STRATUS: flowing sheets with texture (5 octaves, heavy warp) ---
    let s_warp = vec3<f32>(
        snoise(p_base * 0.5 + vec3<f32>(31.7, 0.0, 0.0)),
        snoise(p_base * 0.5 + vec3<f32>(0.0, 47.3, 0.0)),
        snoise(p_base * 0.5 + vec3<f32>(0.0, 0.0, 73.1))
    ) * 0.5 + wind_bias;
    let s_p = p_base + s_warp;
    let stratus_val = (snoise(s_p) * 0.50 + snoise(s_p * 2.1) * 0.25
        + snoise(s_p * 4.2) * 0.13 + snoise(s_p * 8.4) * 0.07
        + snoise(s_p * 16.8) * 0.05) * 0.5 + 0.5;

    // --- CUMULUS: isolated puffy blobs — soft ramp instead of hard max(noise,0) ---
    let c_p = p_base + vec3<f32>(13.7, 7.3, 21.1) + wind_bias * 0.5;
    var cumulus_val = 0.0;
    var c_freq = 1.0;
    var c_amp = 1.0;
    var c_amp_sum = 0.0;
    for (var i = 0; i < 5; i++) {
        // smooth_step ramp instead of hard max — eliminates pixelated zero-crossing edges
        let n = snoise(c_p * c_freq);
        cumulus_val += smooth_step(-0.1, 0.15, n) * n * c_amp;
        c_amp_sum += c_amp;
        c_freq *= 2.2;
        c_amp *= 0.45;
    }
    cumulus_val = cumulus_val / c_amp_sum;
    cumulus_val = pow(max(cumulus_val, 0.0), 0.9) * 1.5;

    // --- THIN/WISPY: sparse, low-density streaks ---
    let t_p = p_base * 0.8 + vec3<f32>(51.0, 23.0, 87.0) + wind_bias * 2.0; // strong wind stretch
    let thin_val = (snoise(t_p) * 0.7 + snoise(t_p * 3.0) * 0.3) * 0.3 + 0.3; // low amplitude

    // === Step 2: Region SELECTS cloud type (climate-adjusted) ===
    let rt = region_type; // climate adjusts below after moisture/temp are available
    let w_stratus = smooth_step(0.60, 0.30, rt);
    let w_cumulus = smooth_step(0.40, 0.70, rt);
    let thin_mix = smooth_step(0.3, 0.0, rt) * 0.6 + 0.15;
    let w_total = max(w_stratus + w_cumulus + thin_mix, 0.01);
    let noise_val = (w_stratus * stratus_val + w_cumulus * cumulus_val + thin_mix * thin_val) / w_total;

    // Weather-scale clear/cloudy regions
    let weather_region = snoise(vortex_sphere * 1.5 + seed_off * 0.3 + vec3<f32>(77.0, 0.0, 0.0));
    let noise_val_w = noise_val * (0.75 + 0.25 * (weather_region * 0.5 + 0.5));

    // === Step 3: Climate + terrain modulated local coverage ===
    // All influences adjust the Schneider THRESHOLD, not density amplitude.

    // Domain-warp climate lookup to break latitude bands
    let climate_warp = vec3<f32>(
        snoise(sphere_pos * 2.5 + vec3<f32>(200.0, 0.0, 0.0)),
        snoise(sphere_pos * 2.5 + vec3<f32>(0.0, 300.0, 0.0)),
        snoise(sphere_pos * 2.5 + vec3<f32>(0.0, 0.0, 400.0))
    ) * 0.12;
    let warped_climate_pos = normalize(sphere_pos + climate_warp);

    // Use ocean_level (not sampled height) for climate — avoids cubemap face seams
    let cloud_h = uniforms.ocean_level;
    let moisture = compute_moisture(warped_climate_pos, cloud_h, uniforms.season);
    let moisture_norm = clamp(moisture / 300.0, 0.0, 1.0);
    let temp = compute_temperature(warped_climate_pos, cloud_h, uniforms.season);

    // Climate-driven cloud coverage boost: warm+moist areas get more clouds
    let warm_moist_boost = smooth_step(15.0, 28.0, temp) * smooth_step(0.3, 0.6, moisture_norm) * 0.10;

    // === Latitude-banded cloud distribution ===
    let subtropical = smooth_step(15.0, 25.0, cloud_lat_deg) * smooth_step(40.0, 30.0, cloud_lat_deg);
    let midlat = smooth_step(30.0, 45.0, cloud_lat_deg) * smooth_step(65.0, 55.0, cloud_lat_deg);
    let lat_climate = itcz_factor * 0.30               // ITCZ: heavy clouds
        - subtropical * 0.12                             // Subtropical: clear zone (reduced suppression)
        + midlat * 0.12                                  // Mid-latitude: frontal
        + polar_c * 0.08;                                // Polar: thin overcast

    // Coverage formula: slider=1.0 overrides climate suppression for ~95% cover
    // At low coverage, climate zones modulate; at high coverage, everything fills in
    let climate_coverage = (0.35 + lat_climate) * coverage + moisture_norm * 0.25;
    var local_coverage = max(climate_coverage, coverage * 0.85); // floor ensures slider=1 → ~85%+

    // === Mountain clouds (orographic lift + föhn gap) — smooth transitions ===
    let tilt = uniforms.axial_tilt_rad;
    let tilted_y = sphere_pos.y * cos(tilt) + sphere_pos.z * sin(tilt);
    let lat_rad = asin(clamp(tilted_y, -1.0, 1.0));
    // Sample height at CLIMATE-warped position (smooths out terrain pixelation in clouds)
    let current_h = textureSample(height_tex, height_sampler, warped_climate_pos).r;
    // Very wide smooth transition at coastline — no sharp edges
    let land_factor = smooth_step(uniforms.ocean_level - 0.03, uniforms.ocean_level + 0.12, current_h);
    if (land_factor > 0.01) {
        let wind = wind_direction_vec(lat_rad);
        let tangent_wind = normalize(wind - sphere_pos * dot(wind, sphere_pos));

        // Windward: orographic lift — sample at wider offset to smooth terrain pixels
        let upwind_pos = normalize(warped_climate_pos + tangent_wind * 0.07);
        let upwind_h = textureSample(height_tex, height_sampler, upwind_pos).r;
        let mountain_lift = smooth_step(0.06, 0.25, max(upwind_h - uniforms.ocean_level, 0.0));
        local_coverage += mountain_lift * 0.25 * land_factor;

        // Leeward: föhn gap — also at warped position
        let downwind_pos = normalize(warped_climate_pos - tangent_wind * 0.07);
        let downwind_h = textureSample(height_tex, height_sampler, downwind_pos).r;
        let my_elev = max(current_h - uniforms.ocean_level, 0.0);
        let foehn_raw = clamp((downwind_h - my_elev - 0.03) * 3.0, 0.0, 0.25);
        local_coverage -= foehn_raw * land_factor;
    }

    // Warm convection + climate boost
    let convection_boost = smooth_step(15.0, 30.0, temp) * 0.06;
    local_coverage += convection_boost + warm_moist_boost;

    // === Step 3b: Cyclone storm coverage boost ===
    // Phase 1: storms boost local_coverage to make the area cloudier.
    let n_storms = i32(min(uniforms.storm_count, 8.0));
    if (n_storms > 0) {
        for (var i = 0; i < 8; i++) {
            if (i >= n_storms) { break; }
            let fi = f32(i);
            let slat = (30.0 + fract(sin(fi * 127.1 + s) * 43758.5) * 25.0) * 3.14159 / 180.0;
            let slon = fract(sin(fi * 311.7 + s * 1.3) * 23421.6) * 6.28318;
            let sign_y = select(-1.0, 1.0, i % 2 == 0);
            let raw_center = vec3<f32>(
                cos(slat) * cos(slon), sin(slat) * sign_y, cos(slat) * sin(slon)
            );
            // Rotate storm center by axial tilt so storms track tilted circulation
            let ct = cos(uniforms.axial_tilt_rad);
            let st = sin(uniforms.axial_tilt_rad);
            let center = normalize(vec3<f32>(
                raw_center.x,
                raw_center.y * ct - raw_center.z * st,
                raw_center.y * st + raw_center.z * ct
            ));
            let d = acos(clamp(dot(sphere_pos, center), -1.0, 1.0));
            // Per-storm unique size (matching vortex warp section)
            let per_storm_size = 0.5 + fract(sin(fi * 73.1 + s * 0.7) * 19283.3) * 1.5;
            let storm_sigma_cov = (18.0 + per_storm_size * 10.0) / max(uniforms.storm_size * uniforms.storm_size, 0.1);
            let falloff = exp(-d * d * storm_sigma_cov);

            // Strong coverage boost with clear eye
            let eye_radius = 0.02 / max(per_storm_size, 0.3); // smaller storms = relatively larger eye
            let eye_clear = smooth_step(eye_radius, eye_radius * 2.5, d);
            local_coverage += falloff * 0.7 * eye_clear;
        }
    }

    local_coverage = clamp(local_coverage, 0.0, 1.0);

    // === Step 4: Multi-scale pattern variety + storm puffiness ===
    let weather_scale = snoise(sphere_pos * 2.0 + seed_off * 0.2) * 0.15;
    // Inside storm zones, boost cumulus peaks for puffier towering clouds
    var storm_cumulus_boost = 0.0;
    if (n_storms > 0) {
        for (var i = 0; i < 8; i++) {
            if (i >= n_storms) { break; }
            let fi = f32(i);
            let slat = (30.0 + fract(sin(fi * 127.1 + s) * 43758.5) * 25.0) * 3.14159 / 180.0;
            let slon = fract(sin(fi * 311.7 + s * 1.3) * 23421.6) * 6.28318;
            let sign_y = select(-1.0, 1.0, i % 2 == 0);
            let raw_c = vec3<f32>(cos(slat) * cos(slon), sin(slat) * sign_y, cos(slat) * sin(slon));
            let ct2 = cos(uniforms.axial_tilt_rad);
            let st2 = sin(uniforms.axial_tilt_rad);
            let cntr = normalize(vec3<f32>(raw_c.x, raw_c.y * ct2 - raw_c.z * st2, raw_c.y * st2 + raw_c.z * ct2));
            let sd = acos(clamp(dot(sphere_pos, cntr), -1.0, 1.0));
            let ps = 0.5 + fract(sin(fi * 73.1 + s * 0.7) * 19283.3) * 1.5;
            let inf = exp(-sd * sd * (18.0 + ps * 10.0) / max(uniforms.storm_size * uniforms.storm_size, 0.1));
            storm_cumulus_boost = max(storm_cumulus_boost, inf * 0.3);
        }
    }
    // Add cumulus peaks inside storm zones — use ORIGINAL position (not vortex-warped)
    // to maintain texture detail instead of smooth vortex-stretched streaks
    let storm_detail = snoise(sphere_pos * 18.0 + seed_off) * 0.5
                     + snoise(sphere_pos * 35.0 + seed_off * 1.3) * 0.3;
    let storm_peaks = max(storm_detail, 0.0) * storm_cumulus_boost;
    let varied_noise = clamp(noise_val_w + weather_scale + storm_peaks, 0.0, 1.0);

    // === Step 5: Schneider remap + thin veil for graduated edges ===
    let remapped = cloud_remap(varied_noise, 1.0 - local_coverage, 1.0, 0.0, 1.0) * local_coverage;

    // Thin veil: graduated opacity around cloud edges + scattered wisps in gaps
    // Lower threshold (0.40) creates wide soft transition instead of hard cutoff
    let thin_veil = max(varied_noise - 0.40, 0.0) * 0.4 * local_coverage;

    // Combine: thick clouds from remap + thin clouds from veil
    var density = max(remapped, thin_veil);

    // Soft power curve: pulls density toward more mid-range values (less binary)
    density = pow(density, 0.8);

    // === Step 6: Carve spiral arms + eye into density (post-remap) ===
    // This directly sculpts the visible cloud density, making spiral structure visible.
    if (n_storms > 0) {
        for (var i = 0; i < 8; i++) {
            if (i >= n_storms) { break; }
            let fi = f32(i);
            let slat = (30.0 + fract(sin(fi * 127.1 + s) * 43758.5) * 25.0) * 3.14159 / 180.0;
            let slon = fract(sin(fi * 311.7 + s * 1.3) * 23421.6) * 6.28318;
            let sign_y = select(-1.0, 1.0, i % 2 == 0);
            let raw_center = vec3<f32>(
                cos(slat) * cos(slon), sin(slat) * sign_y, cos(slat) * sin(slon)
            );
            // Rotate storm center by axial tilt so storms track tilted circulation
            let ct = cos(uniforms.axial_tilt_rad);
            let st = sin(uniforms.axial_tilt_rad);
            let center = normalize(vec3<f32>(
                raw_center.x,
                raw_center.y * ct - raw_center.z * st,
                raw_center.y * st + raw_center.z * ct
            ));
            let d = acos(clamp(dot(sphere_pos, center), -1.0, 1.0));
            let base_sigma = 22.0 + fract(sin(fi * 73.1 + s * 0.7) * 19283.3) * 12.0;
            let storm_sigma = base_sigma / max(uniforms.storm_size * uniforms.storm_size, 0.1);
            let near_storm = exp(-d * d * storm_sigma);

            // Tangent-plane angle for spiral
            let up_s = select(vec3<f32>(0.0, 0.0, 1.0), vec3<f32>(0.0, 1.0, 0.0), abs(center.y) < 0.9);
            let tx = normalize(cross(up_s, center));
            let ty = cross(center, tx);
            let to_pt = sphere_pos - center * dot(sphere_pos, center);
            let angle = atan2(dot(to_pt, ty), dot(to_pt, tx));

            // Eye: clear center with high-detail noisy edge
            let eye_r = (0.03 + fract(sin(fi * 53.7 + s * 0.3) * 31415.9) * 0.02) * uniforms.storm_size;
            let eye_n1 = snoise(sphere_pos * 60.0 + seed_off + vec3<f32>(fi * 11.0, 0.0, 0.0));
            let eye_n2 = snoise(sphere_pos * 120.0 + seed_off + vec3<f32>(fi * 23.0, 0.0, 0.0));
            let eye_noise = (eye_n1 * 0.6 + eye_n2 * 0.4) * eye_r * 0.35;
            let eye_mask = smooth_step((eye_r + eye_noise) * 0.25, (eye_r + eye_noise) * 1.3, d);

            // Dense eye wall ring — thicker, more detailed
            let wall_dist = abs(d - eye_r * 1.4);
            let wall_noise = snoise(sphere_pos * 50.0 + vec3<f32>(fi * 31.0)) * eye_r * 0.15;
            let eye_wall_boost = exp(-(wall_dist + wall_noise) * (wall_dist + wall_noise) / (eye_r * eye_r * 0.6)) * near_storm * 0.45;
            density += eye_wall_boost;

            // Spiral arms with TURBULENT edges (noise-perturbed angle)
            let arm_noise = snoise(sphere_pos * 20.0 + seed_off + vec3<f32>(fi * 17.0, 0.0, 0.0));
            let perturbed_angle = angle + arm_noise * 0.35; // turbulent arm edges
            let spiral_phase = perturbed_angle * sign_y - log(max(d, 0.005)) * 3.0;
            let spiral_raw = cos(spiral_phase * 2.0);
            let spiral_fade = smooth_step(eye_r * 1.5, eye_r * 3.5, d);
            // Cap vortex influence at minimum distance to prevent tails
            let storm_fade2 = near_storm * smooth_step(0.0, 0.03, d); // no influence at d=0

            // Dense textured cloud along spiral arms
            let arm_tex = snoise(sphere_pos * 15.0 + seed_off + vec3<f32>(fi * 13.0, 0.0, 0.0)) * 0.3 + 0.7;
            let arm_strength = pow(max(spiral_raw, 0.0), 1.5) * arm_tex; // sharper arm peaks
            let arm_boost = arm_strength * storm_fade2 * spiral_fade * 0.45;
            density += arm_boost;

            // Softer gaps between arms (reduced contrast)
            let gap_depth = storm_fade2 * spiral_fade * 0.60; // was 0.85, less aggressive
            let arm_shape = spiral_raw * 0.5 + 0.5;
            let spiral_mask = 1.0 - gap_depth * (1.0 - max(arm_shape, arm_tex * 0.4));

            density *= mix(1.0, spiral_mask * eye_mask, near_storm);
        }
    }

    return max(density, 0.0);
}

// High-altitude cirrus: thin ice-crystal wisps at jet stream altitudes.
// Separate from main cloud layer — rendered at a higher shell.
fn compute_cirrus_density(sphere_pos: vec3<f32>) -> f32 {
    let cov = uniforms.cloud_coverage;
    if (cov <= 0.0) { return 0.0; }

    let s = uniforms.cloud_seed;
    let seed_off = vec3<f32>(s, fract(s * 1.618) * 89.0, fract(s * 2.618) * 83.0);

    // Wind-aligned stretching: cirrus follows jet stream (strong at mid-latitudes)
    let tilt = uniforms.axial_tilt_rad;
    let tilted_y = sphere_pos.y * cos(tilt) + sphere_pos.z * sin(tilt);
    let ci_lat = asin(clamp(tilted_y, -1.0, 1.0));
    let lat_deg = abs(ci_lat) * 180.0 / 3.14159;
    let jet_wind = wind_direction_vec(ci_lat);
    let jet_tangent = normalize(jet_wind - sphere_pos * dot(jet_wind, sphere_pos));
    // Gentle jet stream stretch at mid-latitudes
    let jet_stretch = smooth_step(20.0, 45.0, lat_deg) * 0.08;
    let ci_sphere = normalize(sphere_pos + jet_tangent * jet_stretch);

    let p = ci_sphere * 6.0 + seed_off + vec3<f32>(50.0, 30.0, 70.0);

    // 3-octave high-frequency wispy noise
    let ci = snoise(p) * 0.5
           + snoise(p * 2.1 + vec3<f32>(3.7, 1.1, 8.3)) * 0.3
           + snoise(p * 4.4 + vec3<f32>(1.3, 5.9, 2.1)) * 0.2;
    let ci_norm = ci * 0.5 + 0.5;

    // Cirrus common at mid-to-high latitudes (jet stream), rare at equator and poles
    let lat_boost = smooth_step(20.0, 45.0, lat_deg) * smooth_step(75.0, 60.0, lat_deg) * 0.18;

    let cirrus_cov = cov * 0.35 + lat_boost;
    let density = cloud_remap(ci_norm, 1.0 - cirrus_cov, 1.0, 0.0, 1.0) * cirrus_cov;
    return max(density, 0.0);
}

// Clean grayscale elevation — pure height visualization
// Maps terrain range (~-0.5 to ~0.8) to full 0..1 grayscale
fn height_color(h: f32, ocean_level: f32) -> vec3<f32> {
    let v = clamp((h + 0.5) / 1.3, 0.0, 1.0);
    return vec3<f32>(v, v, v);
}

// ---- Continuous gradient biome coloring ----
// Replaces discrete Whittaker lookup with smooth 2D interpolation.
// Temperature × moisture → color via 3×2 anchor grid.

fn gradient_color(mean_temp: f32, mean_moisture: f32, seasonal_temp: f32, variation: f32, region_noise: f32) -> vec3<f32> {
    // 12-biome system: 4 temperature bands × 3 moisture levels
    // Biome classification uses MEAN ANNUAL values for stability
    let r = region_noise; // [0,1] regional sub-variant selector

    // Temperature bands (smooth interpolation weights)
    let t_polar   = 1.0 - smooth_step(-15.0, 0.0, mean_temp);    // <0°C: ice/tundra
    let t_boreal  = smooth_step(-10.0, 2.0, mean_temp) * (1.0 - smooth_step(8.0, 18.0, mean_temp));
    let t_temperate = smooth_step(5.0, 15.0, mean_temp) * (1.0 - smooth_step(20.0, 30.0, mean_temp));
    let t_tropical = smooth_step(18.0, 28.0, mean_temp);

    // Moisture bands
    let m_arid = 1.0 - smooth_step(15.0, 40.0, mean_moisture);   // <25mm: desert
    let m_semi = smooth_step(15.0, 35.0, mean_moisture) * (1.0 - smooth_step(55.0, 90.0, mean_moisture));
    let m_wet  = smooth_step(50.0, 90.0, mean_moisture);          // >70mm: forest/jungle

    // === 12 biome anchor colors with regional sub-variants ===
    // Polar
    let ice_desert    = mix(vec3<f32>(0.72, 0.75, 0.80), vec3<f32>(0.60, 0.58, 0.55), r); // cold dry
    let tundra        = mix(vec3<f32>(0.55, 0.58, 0.45), vec3<f32>(0.48, 0.52, 0.38), r); // cold semi: lichen/moss
    let polar_wet     = mix(vec3<f32>(0.62, 0.68, 0.65), vec3<f32>(0.52, 0.60, 0.50), r); // cold wet: boggy tundra

    // Boreal
    let cold_steppe   = mix(vec3<f32>(0.58, 0.48, 0.32), vec3<f32>(0.52, 0.42, 0.28), r); // cool dry steppe
    let boreal_forest = mix(vec3<f32>(0.12, 0.28, 0.10), vec3<f32>(0.18, 0.32, 0.14), r); // dark conifer
    let boreal_bog    = mix(vec3<f32>(0.15, 0.30, 0.12), vec3<f32>(0.22, 0.35, 0.18), r); // wet taiga

    // Temperate
    let med_scrub     = mix(mix(vec3<f32>(0.55, 0.50, 0.30), vec3<f32>(0.62, 0.42, 0.24), r),
                             vec3<f32>(0.48, 0.44, 0.28), smooth_step(0.7, 1.0, r)); // Mediterranean
    let temp_forest   = mix(mix(vec3<f32>(0.14, 0.38, 0.10), vec3<f32>(0.22, 0.42, 0.15), r),
                             vec3<f32>(0.10, 0.30, 0.08), smooth_step(0.6, 1.0, r)); // deciduous/mixed
    let temp_rain     = mix(vec3<f32>(0.08, 0.34, 0.08), vec3<f32>(0.12, 0.38, 0.10), r); // temperate rainforest

    // Tropical
    let hot_desert    = mix(mix(vec3<f32>(0.85, 0.75, 0.55), vec3<f32>(0.75, 0.45, 0.25), r),
                             vec3<f32>(0.40, 0.32, 0.25), smooth_step(0.7, 1.0, r)); // sand/red/volcanic
    let savanna       = mix(vec3<f32>(0.52, 0.48, 0.22), vec3<f32>(0.42, 0.40, 0.18), r); // dry grassland
    let tropical_rain = mix(mix(vec3<f32>(0.06, 0.30, 0.04), vec3<f32>(0.04, 0.24, 0.03), r),
                             vec3<f32>(0.10, 0.28, 0.06), smooth_step(0.5, 1.0, r)); // deep jungle

    // Blend across moisture within each temperature band
    let polar_color = m_arid * ice_desert + m_semi * tundra + m_wet * polar_wet;
    let boreal_color = m_arid * cold_steppe + m_semi * boreal_forest + m_wet * boreal_bog;
    let temp_color = m_arid * med_scrub + m_semi * temp_forest + m_wet * temp_rain;
    let trop_color = m_arid * hot_desert + m_semi * savanna + m_wet * tropical_rain;

    // Blend across temperature bands
    var base = t_polar * polar_color + t_boreal * boreal_color
             + t_temperate * temp_color + t_tropical * trop_color;
    // Normalize blending weights (they don't always sum to 1 due to overlapping smooth_steps)
    let w_sum = t_polar + t_boreal + t_temperate + t_tropical;
    base /= max(w_sum, 0.25); // floor at 0.25 prevents color spikes at band boundaries

    // === Seasonal color modulation ===
    let temp_deviation = seasonal_temp - mean_temp;
    let green_amount = max(base.g - max(base.r, base.b), 0.0);
    if (green_amount > 0.05) {
        let winter_factor = clamp(-temp_deviation / 20.0, 0.0, 1.0);
        let summer_factor = clamp(temp_deviation / 20.0, 0.0, 1.0);
        base += vec3<f32>(0.06, -0.02, -0.03) * winter_factor * green_amount * 2.0;
        base += vec3<f32>(-0.01, 0.02, 0.0) * summer_factor * green_amount;
    }
    if (seasonal_temp < 5.0 && mean_temp < 15.0) {
        let cold_winter = clamp(-temp_deviation / 15.0, 0.0, 1.0);
        base = mix(base, vec3<f32>(0.80, 0.82, 0.85), cold_winter * 0.25 * t_polar);
    }

    // Per-pixel noise for natural texture
    base += base * variation * 0.12;

    return base;
}

// ---- Terrain normal from height cubemap ----
fn compute_terrain_normal(sphere_pos: vec3<f32>, geo_normal: vec3<f32>) -> vec3<f32> {
    let step = 0.0015; // ~1 texel at 512 cubemap resolution

    // Build tangent frame in CUBEMAP space (consistent with height sampling)
    var up = vec3<f32>(0.0, 1.0, 0.0);
    if (abs(dot(sphere_pos, up)) > 0.99) { up = vec3<f32>(1.0, 0.0, 0.0); }
    let tan_world = normalize(cross(up, sphere_pos));
    let bitan_world = normalize(cross(sphere_pos, tan_world));

    // Sample 4 neighbors in cubemap space
    let h_right = textureSample(height_tex, height_sampler, sphere_pos + tan_world * step).r;
    let h_left  = textureSample(height_tex, height_sampler, sphere_pos - tan_world * step).r;
    let h_up    = textureSample(height_tex, height_sampler, sphere_pos + bitan_world * step).r;
    let h_down  = textureSample(height_tex, height_sampler, sphere_pos - bitan_world * step).r;

    // Central differences → height gradient in cubemap space
    let height_scale = uniforms.height_scale;
    let dx = (h_right - h_left) * height_scale;
    let dy = (h_up - h_down) * height_scale;

    // Perturbed normal in cubemap/world space
    let perturbed_world = normalize(sphere_pos - tan_world * dx - bitan_world * dy);

    // Transform back to view space using inverse rotation (transpose of orthogonal matrix)
    let inv_rot = transpose(uniforms.rotation);
    let perturbed_view = normalize((inv_rot * vec4<f32>(perturbed_world, 0.0)).xyz);
    return perturbed_view;
}

// ---- Surface roughness from climate ----
fn compute_roughness(temp_c: f32, moisture_cm: f32, is_ocean: bool, is_ice: bool) -> f32 {
    if (is_ocean) {
        if (is_ice) { return 0.15; }
        return 0.10;
    }
    // Continuous land roughness from temperature + moisture (no hard thresholds)
    // Cold → smooth (snow/ice), hot+dry → rough (desert), wet → moderate (vegetation)
    let snow_smooth = smooth_step(5.0, -5.0, temp_c) * 0.55; // snow: pulls toward 0.20
    let desert_rough = smooth_step(50.0, 15.0, moisture_cm) * smooth_step(5.0, 20.0, temp_c); // dry+warm
    let vegetation = smooth_step(30.0, 120.0, moisture_cm) * smooth_step(5.0, 15.0, temp_c); // wet+warm
    var roughness = 0.55; // base: moderate
    roughness -= snow_smooth; // snow smooths it
    roughness += desert_rough * 0.25; // desert roughens
    roughness -= vegetation * 0.15; // dense vegetation slightly smoother
    // Per-pixel noise from spatial position (NOT temp/moisture which creates sharp biome edges)
    roughness += snoise(vec3<f32>(temp_c * 0.02 + moisture_cm * 0.005, moisture_cm * 0.01 - temp_c * 0.01, temp_c * 0.015)) * 0.06;
    return clamp(roughness, 0.15, 0.85);
}

// ---- Terrain ambient occlusion ----
// Samples height neighbors to darken valleys and crevices.
fn compute_ao(sphere_pos: vec3<f32>) -> f32 {
    let h_center = textureSample(height_tex, height_sampler, sphere_pos).r;

    // Two-radius sampling: wide for broad valleys, narrow for crevices
    // Both use soft thresholds to avoid pixelated edges
    var occlusion = 0.0;

    // Wide radius: catches broad valley shading (smooth)
    let wide = 0.012;
    let w_offsets = array<vec3<f32>, 4>(
        vec3<f32>(wide, 0.0, 0.0), vec3<f32>(-wide, 0.0, 0.0),
        vec3<f32>(0.0, wide, 0.0), vec3<f32>(0.0, -wide, 0.0)
    );
    for (var i = 0; i < 4; i++) {
        let neighbor = textureSample(height_tex, height_sampler, sphere_pos + w_offsets[i]).r;
        let height_diff = max(neighbor - h_center, 0.0);
        occlusion += smooth_step(0.0, 0.15, height_diff) * 0.5; // gentle, wide contribution
    }

    // Narrow radius: catches local detail (subtle)
    let narrow = 0.005;
    let n_offsets = array<vec3<f32>, 4>(
        vec3<f32>(narrow, narrow, 0.0) * 0.707, vec3<f32>(-narrow, narrow, 0.0) * 0.707,
        vec3<f32>(narrow, -narrow, 0.0) * 0.707, vec3<f32>(-narrow, -narrow, 0.0) * 0.707
    );
    for (var j = 0; j < 4; j++) {
        let neighbor = textureSample(height_tex, height_sampler, sphere_pos + n_offsets[j]).r;
        let height_diff = max(neighbor - h_center, 0.0);
        occlusion += smooth_step(0.0, 0.10, height_diff) * 0.3; // subtle, tight contribution
    }

    // Softer darkening: max ~60% darken (was 80%), higher floor
    let ao = 1.0 - occlusion * 0.08;
    return clamp(ao, 0.4, 1.0);
}

// ---- PBR: GGX normal distribution ----
fn ggx_distribution(n_dot_h: f32, roughness: f32) -> f32 {
    let a = roughness * roughness;
    let a2 = a * a;
    let d = n_dot_h * n_dot_h * (a2 - 1.0) + 1.0;
    return a2 / (3.14159 * d * d + 0.0001);
}

// ---- PBR: Schlick Fresnel ----
fn fresnel_schlick(h_dot_v: f32, f0: f32) -> f32 {
    return f0 + (1.0 - f0) * pow(1.0 - h_dot_v, 5.0);
}

// ---- Urban density: procedural city placement ----
// Returns 0.0 (wilderness) to 1.0 (dense urban). Based on climate habitability.
fn compute_urban_density(sphere_pos: vec3<f32>, height: f32) -> f32 {
    let dev = uniforms.night_lights;
    if (dev <= 0.0) { return 0.0; }

    // Only on land
    if (height <= uniforms.ocean_level) { return 0.0; }

    let land_h = (height - uniforms.ocean_level) / max(1.0 - uniforms.ocean_level, 0.01);
    let temp = compute_temperature(sphere_pos, height, 0.5);

    // Habitability score — restrict to temperate zones (no arctic/cold cities)
    var score = 0.0;
    // Temperate climate: must be warm enough (8-25°C sweet spot)
    score += smooth_step(8.0, 18.0, temp) * smooth_step(35.0, 22.0, temp) * 0.4;
    // Low elevation preferred
    score += (1.0 - smooth_step(0.0, 0.25, land_h)) * 0.2;
    // Coastal boost
    let stp = 0.06;
    let h_e = textureSample(height_tex, height_sampler, sphere_pos + vec3<f32>(stp, 0.0, 0.0)).r;
    let h_w = textureSample(height_tex, height_sampler, sphere_pos + vec3<f32>(-stp, 0.0, 0.0)).r;
    let h_n = textureSample(height_tex, height_sampler, sphere_pos + vec3<f32>(0.0, stp, 0.0)).r;
    let h_s = textureSample(height_tex, height_sampler, sphere_pos + vec3<f32>(0.0, -stp, 0.0)).r;
    var ocean_near = 0.0;
    if (h_e < uniforms.ocean_level) { ocean_near += 1.0; }
    if (h_w < uniforms.ocean_level) { ocean_near += 1.0; }
    if (h_n < uniforms.ocean_level) { ocean_near += 1.0; }
    if (h_s < uniforms.ocean_level) { ocean_near += 1.0; }
    score += min(ocean_near / 2.0, 1.0) * 0.25;
    // Hard cutoff: no cities below 5°C mean annual
    score *= smooth_step(3.0, 10.0, temp);

    // City pattern: web/dot network instead of blobs
    // Very high frequency for tiny dots
    let dots = snoise(sphere_pos * 120.0) * 0.5 + 0.5;
    let dots2 = snoise(sphere_pos * 250.0 + vec3<f32>(7.3, 2.1, 5.9)) * 0.5 + 0.5;
    // Web-like connections: abs(noise) creates thin lines at zero crossings
    let web1 = 1.0 - abs(snoise(sphere_pos * 60.0 + vec3<f32>(3.1, 8.7, 1.3))) * 2.0;
    let web2 = 1.0 - abs(snoise(sphere_pos * 130.0 + vec3<f32>(11.3, 4.7, 7.1))) * 2.0;
    let webs = max(max(web1, 0.0), max(web2, 0.0));
    // Combine dots + webs
    let city_pattern = max(dots * dots2 * 1.5, webs * 0.6);

    // Cubic dev ramp: very sparse at low values, rapidly grows near 1.0
    // 0.01→0.000001, 0.1→0.001, 0.5→0.125, 1.0→1.0
    let dev_scaled = dev * dev * dev;
    let urban_raw = score * city_pattern;
    let threshold = (1.0 - dev_scaled) * 0.45;
    return smooth_step(threshold, threshold + 0.04, urban_raw);
}

// ---- Starfield + sun orb background ----
fn starfield(ndc: vec2<f32>, sun_dir: vec3<f32>, sun_color: vec3<f32>) -> vec3<f32> {
    var bg = vec3<f32>(0.0, 0.0, 0.0); // pure black space

    // Stars: hash-based bright dots at pseudo-random positions
    // Quantize ndc to a grid, hash each cell to decide if it has a star
    let star_scale = 120.0; // density: higher = more stars
    let cell = floor(ndc * star_scale);
    let cell_uv = fract(ndc * star_scale); // position within cell [0,1]

    // Hash cell coordinates to get pseudo-random star position + brightness
    let h1 = fract(sin(dot(cell, vec2<f32>(127.1, 311.7))) * 43758.5453);
    let h2 = fract(sin(dot(cell, vec2<f32>(269.5, 183.3))) * 28461.6432);
    let h3 = fract(sin(dot(cell, vec2<f32>(419.2, 371.9))) * 59182.7314);

    // Star exists if hash exceeds threshold (~15% of cells have a star)
    if (h1 > 0.85) {
        let star_pos = vec2<f32>(h2, h3); // random position in cell
        let dist = length(cell_uv - star_pos);
        let star_size = 0.03 + h1 * 0.04; // tiny points
        let brightness = (1.0 - smooth_step(0.0, star_size, dist)) * (0.4 + h2 * 0.6);
        // Slight color variation: warm (h3<0.3), blue (h3>0.7), white (middle)
        var star_color = vec3<f32>(1.0);
        if (h3 < 0.3) { star_color = vec3<f32>(1.0, 0.9, 0.7); }
        else if (h3 > 0.7) { star_color = vec3<f32>(0.7, 0.85, 1.0); }
        bg += star_color * brightness;
    }

    // Sun orb: project sun direction to screen space for perfect circle
    if (sun_dir.z < -0.01) { // sun is behind the planet (visible in background)
        let sun_screen = vec2<f32>(sun_dir.x, sun_dir.y) / (-sun_dir.z);
        let sun_dist = length(ndc - sun_screen);

        // Sun disc — colored by star type
        let sun_radius = 0.06;
        let sun_core = 1.0 - smooth_step(0.0, sun_radius, sun_dist);
        bg += sun_color * 3.0 * sun_core;

        // Tight glow halo
        let glow = exp(-sun_dist * sun_dist * 30.0) * 0.2;
        bg += sun_color * glow;
    }

    return bg;
}

// ---- Main fragment shader ----
@fragment
fn fs_main(in: VertexOutput) -> @location(0) vec4<f32> {
    let pan = vec2<f32>(uniforms.pan_x, uniforms.pan_y);
    let ndc = ((in.uv - 0.5) * 2.0 / 0.85 - pan) / uniforms.zoom;
    let r2 = dot(ndc, ndc);

    let sun_dir = normalize(uniforms.light_dir);
    let s_color = star_color(uniforms.star_color_temp);

    let atm_h = uniforms.atmosphere_height;
    let atm_radius = 1.0 + atm_h;
    let has_atm = uniforms.atmosphere_density > 0.001 && atm_h > 0.001;
    let outer_r = select(1.005, atm_radius + 0.015, has_atm);

    // Miss everything — outside both planet and atmosphere → show starfield
    if (r2 > outer_r * outer_r) {
        let bg = starfield(ndc, sun_dir, s_color);
        let bg_tm = bg / (bg + vec3<f32>(1.0)); // tonemap sun HDR
        return vec4<f32>(bg_tm, 1.0);
    }

    let hit_planet = r2 < 1.0;

    // Atmosphere-only ring (between planet edge and outer atmosphere boundary)
    if (!hit_planet) {
        let bg = starfield(ndc, sun_dir, s_color);
        let bg_tm = bg / (bg + vec3<f32>(1.0));
        if (!has_atm || uniforms.show_atmosphere_layer < 0.5 || uniforms.view_mode != 0u) {
            return vec4<f32>(bg_tm, 1.0);
        }
        let z_atm = sqrt(max(atm_radius * atm_radius - r2, 0.0));
        let scatter = ray_march_atmosphere(ndc, z_atm, -z_atm, sun_dir);
        var ring_color = scatter.in_scatter;
        ring_color = ring_color / (ring_color + vec3<f32>(1.0)); // tonemap
        let edge = 1.0 - smooth_step(atm_radius - 0.015, atm_radius, sqrt(r2));
        return vec4<f32>(mix(bg_tm, ring_color, edge), 1.0);
    }

    // Planet surface hit
    let normal = normalize(vec3<f32>(ndc.x, ndc.y, sqrt(1.0 - r2)));
    let rotated = (uniforms.rotation * vec4<f32>(normal, 0.0)).xyz;

    // Sample height from pre-computed cubemap
    let height = textureSample(height_tex, height_sampler, rotated).r;
    let is_ocean = height < uniforms.ocean_level;

    let color_var = snoise(rotated * 8.0);
    // Regional color variance: low-freq noise for spatially coherent biome sub-variants
    let region_noise = snoise(rotated * 0.8 + vec3<f32>(200.0, 0.0, 0.0)) * 0.5
                     + snoise(rotated * 1.6 + vec3<f32>(0.0, 300.0, 0.0)) * 0.25;
    let region_val = clamp(region_noise + 0.5, 0.0, 1.0);

    // Compute effective latitude for altitude zonation (consistent tilt model)
    let tilt_main = uniforms.axial_tilt_rad;
    let tilted_y_main = rotated.y * cos(tilt_main) + rotated.z * sin(tilt_main);
    let effective_lat = asin(clamp(tilted_y_main, -1.0, 1.0));

    var surface_color: vec3<f32>;
    var ice_amount = 0.0; // tracks ice coverage for HDR override

    // Base layer: when biomes OFF, always show clean grayscale elevation for everything
    if (uniforms.show_biomes < 0.5 && uniforms.show_water < 0.5) {
        // Pure elevation mode — no ocean/land distinction, just height
        surface_color = height_color(height, uniforms.ocean_level);
    } else if (is_ocean && uniforms.show_water > 0.5) {
        // Smooth ocean gradient: shallow → deep with continuous depth color
        let ocean_temp = compute_temperature(rotated, height, uniforms.season);
        let raw_depth = (uniforms.ocean_level - height) / max(uniforms.ocean_level + 1.0, 0.5);
        let depth = clamp(raw_depth, 0.0, 1.0);
        let depth_noise = snoise(rotated * 8.0) * 0.02;

        let near_shore = vec3<f32>(0.07, 0.22, 0.38);
        let mid_ocean  = vec3<f32>(0.04, 0.14, 0.36);
        let deep_ocean = vec3<f32>(0.02, 0.06, 0.22);
        let shelf = smoothstep(0.02, 0.18, depth + depth_noise);
        let abyss = smoothstep(0.18, 0.55, depth);
        var ocean_color = mix(near_shore, mix(mid_ocean, deep_ocean, abyss), shelf);
        ocean_color += vec3<f32>(0.0, 0.015, 0.02) * color_var;

        // Polar sea ice (gated by show_ice)
        if (uniforms.show_ice > 0.5) {
            let ice_edge_noise = snoise(rotated * 12.0) * 1.5;
            let ice_temp_threshold = 3.0 + ice_edge_noise;
            let ice_blend = smooth_step(ice_temp_threshold, ice_temp_threshold - 5.0, ocean_temp);
            ice_amount = ice_blend;

            let ice_thickness = smooth_step(ice_temp_threshold - 1.0, ice_temp_threshold - 8.0, ocean_temp);
            let thin_ice = vec3<f32>(0.85, 0.92, 1.05);
            let thick_ice = vec3<f32>(1.15, 1.18, 1.22);
            var ice_color = mix(thin_ice, thick_ice, ice_thickness);
            let ridge_noise = snoise(rotated * 25.0) * 0.06 + snoise(rotated * 50.0) * 0.03;
            ice_color += vec3<f32>(ridge_noise) * ice_thickness;
            ice_color += vec3<f32>(0.015) * color_var;
            ocean_color = mix(ocean_color, ice_color, ice_blend);
        }
        surface_color = ocean_color;
    } else if (is_ocean) {
        // Water OFF but biomes ON: show height-based grayscale for below-sea-level
        surface_color = height_color(height, uniforms.ocean_level);
    } else {
        // Land: biome coloring or height ramp
        let seasonal_temp = compute_temperature(rotated, height, uniforms.season);
        if (uniforms.show_biomes > 0.5) {
            let mean_temp = compute_temperature(rotated, height, 0.5);
            let mean_moisture = compute_moisture(rotated, height, 0.5);
            surface_color = gradient_color(mean_temp, mean_moisture, seasonal_temp, color_var, region_val);
        } else {
            surface_color = height_color(height, uniforms.ocean_level);
        }

        // Elevation tinting: darken lowlands, lighten highlands
        // Uses raw height for strong contrast on dry worlds (Mars, Venus)
        let land_height = (height - uniforms.ocean_level) / max(1.0 - uniforms.ocean_level, 0.01);
        let h_for_tint = clamp((height + 0.5) / 1.0, 0.0, 1.0);
        let elev_tint = mix(0.65, 1.35, h_for_tint);
        surface_color *= elev_tint;

        // Slope computation for snow reduction (finer sampling step for less patchiness)
        let stp_s = 0.015;
        let sh_e = textureSample(height_tex, height_sampler, rotated + vec3<f32>(stp_s, 0.0, 0.0)).r;
        let sh_w = textureSample(height_tex, height_sampler, rotated + vec3<f32>(-stp_s, 0.0, 0.0)).r;
        let sh_n = textureSample(height_tex, height_sampler, rotated + vec3<f32>(0.0, stp_s, 0.0)).r;
        let sh_s = textureSample(height_tex, height_sampler, rotated + vec3<f32>(0.0, -stp_s, 0.0)).r;
        let slope = max(abs(sh_e - sh_w), abs(sh_n - sh_s)) / (2.0 * stp_s);
        // Subtle snow reduction on slopes (not elimination — mix with 0.4 floor)
        let slope_factor = mix(0.4, 1.0, smooth_step(6.0, 2.5, slope));
        // Only the very highest peaks lose snow (threshold raised from 0.75 to 0.9)
        let altitude_dryness = smooth_step(0.95, 0.80, land_height);

        // Land ice/snow (gated by show_ice)
        if (uniforms.show_ice > 0.5) {
            let land_ice_noise = snoise(rotated * 10.0) * 2.0;
            // Snow requires sustained freezing — threshold -3°C (was 2°C which is too warm,
            // caused snow to trace coastline because any slight elevation cooled below 2°C)
            let cold_snow = smooth_step(-3.0 + land_ice_noise, -12.0, seasonal_temp);
            let altitude_bonus = smooth_step(0.3, 0.6, land_height) * smooth_step(5.0, -5.0, seasonal_temp);
            var ice_blend = max(cold_snow, altitude_bonus);
            ice_blend *= slope_factor * altitude_dryness;
            ice_amount = max(ice_amount, ice_blend);

            let glacier_blue = vec3<f32>(1.05, 1.15, 1.25);
            let fresh_snow = vec3<f32>(1.20, 1.22, 1.25) + vec3<f32>(0.015) * color_var;
            let land_ice_color = mix(fresh_snow, glacier_blue, land_height * cold_snow);
            surface_color = mix(surface_color, land_ice_color, ice_blend);
        }

        // Altitude zonation — biome-aware: tropical and arid mountains differ from polar
        let abs_lat = abs(effective_lat);
        let lat_factor = abs_lat / 1.5708; // 0 at equator, 1 at pole
        let snow_line = 0.50 + 0.20 * (1.0 - lat_factor); // higher at equator
        let rock_line = snow_line - 0.10;
        let alpine_line = rock_line - 0.12;
        let highland_line = 0.12;

        let seasonal_temp_local = compute_temperature(rotated, height, uniforms.season);
        let mean_moisture_local = compute_moisture(rotated, height, 0.5);
        let is_arid = mean_moisture_local < 30.0;

        // Highland zone: climate-dependent coloring
        if (land_height > highland_line && land_height <= alpine_line) {
            let blend = smooth_step(highland_line, highland_line + 0.12, land_height);
            // Arid highlands: lighter rocky brown; wet highlands: darker forest-brown
            let highland_arid = surface_color * vec3<f32>(0.90, 0.82, 0.72);
            let highland_wet = surface_color * vec3<f32>(0.78, 0.75, 0.65);
            let highland_color = mix(highland_wet, highland_arid, smooth_step(30.0, 15.0, mean_moisture_local));
            surface_color = mix(surface_color, highland_color, blend * 0.6);
        }

        // Alpine zone: varies with climate
        if (land_height > alpine_line && land_height <= rock_line) {
            let blend = smooth_step(alpine_line, alpine_line + 0.08, land_height);
            // Tropical alpine: green meadow; temperate: grey-green; arid: brown-grey
            let alpine_tropical = vec3<f32>(0.38, 0.48, 0.28); // paramo/alpine meadow
            let alpine_temperate = vec3<f32>(0.42, 0.45, 0.32); // alpine grassland
            let alpine_arid = vec3<f32>(0.52, 0.46, 0.36); // dry alpine scree
            var alpine_color = mix(alpine_temperate, alpine_tropical, smooth_step(15.0, 25.0, seasonal_temp_local));
            alpine_color = mix(alpine_color, alpine_arid, smooth_step(30.0, 12.0, mean_moisture_local));
            surface_color = mix(surface_color, alpine_color, blend);
        }

        // Rock zone
        if (land_height > rock_line && land_height <= snow_line) {
            let blend = smooth_step(rock_line, rock_line + 0.06, land_height);
            let rock_color = vec3<f32>(0.48, 0.46, 0.42) + vec3<f32>(0.04) * color_var;
            surface_color = mix(surface_color, rock_color, blend);
        }

        // Snow line
        if (land_height > snow_line && seasonal_temp_local < 15.0) {
            let blend = smooth_step(snow_line, snow_line + 0.06, land_height)
                      * slope_factor * altitude_dryness;
            surface_color = mix(surface_color, vec3<f32>(1.15, 1.18, 1.22), blend);
        }

        // Beach transition — very subtle, only at close zoom
        if (land_height < 0.015) {
            let beach_blend = smooth_step(0.015, 0.0, land_height);
            surface_color = mix(surface_color, vec3<f32>(0.55, 0.52, 0.42), beach_blend * 0.3);
        }
    }

    // Polar coastline softening (gated by show_ice)
    if (uniforms.show_ice > 0.5 && ice_amount > 0.3 && is_ocean == false) {
        let coast_temp = compute_temperature(rotated, uniforms.ocean_level, uniforms.season);
        if (coast_temp < 0.0) { // only if the ocean here would also be frozen
            let coast_dist = abs(height - uniforms.ocean_level);
            let coast_soften = 1.0 - smooth_step(0.0, 0.06, coast_dist);
            let uniform_ice = vec3<f32>(1.12, 1.16, 1.22);
            surface_color = mix(surface_color, uniform_ice, coast_soften * ice_amount * 0.8);
        }
    }

    // Day-side urban grey patches (gated by show_cities)
    if (uniforms.show_cities > 0.5 && uniforms.night_lights > 0.0 && !is_ocean) {
        let urban = compute_urban_density(rotated, height);
        if (urban > 0.01) {
            let concrete = vec3<f32>(0.30, 0.30, 0.31); // cool dark grey
            // Darken toward concrete rather than full replace — preserves some surface variation
            let darkened = mix(surface_color * 0.5, concrete, 0.6);
            surface_color = mix(surface_color, darkened, urban * uniforms.night_lights);
        }
    }

    // Debug views
    if (uniforms.view_mode > 0u) {
        let temp = compute_temperature(rotated, height, uniforms.season);
        let moisture = compute_moisture(rotated, height, uniforms.season);
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
            case 4u: {
                let mean_t = compute_temperature(rotated, height, 0.5);
                let mean_m = compute_moisture(rotated, height, 0.5);
                debug_color = gradient_color(mean_t, mean_m, mean_t, 0.0, region_val) * 1.3;
            }
            case 5u: {
                if (is_ocean) {
                    if (compute_temperature(rotated, height, uniforms.season) < -2.0) { debug_color = vec3<f32>(1.0); }
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
                // Roughness visualization (uses actual compute_roughness)
                let rt = compute_temperature(rotated, height, uniforms.season);
                let rm = compute_moisture(rotated, height, uniforms.season);
                let r = compute_roughness(rt, rm, is_ocean, is_ocean && rt < -2.0);
                debug_color = vec3<f32>(r, r, r);
            }
            case 8u: {
                // AO visualization
                let ao_val = select(1.0, compute_ao(rotated), !is_ocean);
                debug_color = vec3<f32>(ao_val, ao_val, ao_val);
            }
            case 9u: {
                // Cloud density visualization
                let cd = compute_cloud_density(rotated, height);
                debug_color = vec3<f32>(cd, cd, cd);
            }
            case 10u: {
                // Emission export: city lights only, everything else black
                let urban = compute_urban_density(rotated, height);
                debug_color = vec3<f32>(urban, urban * 0.8, urban * 0.3);
            }
            case 11u: {
                // Boundary type proxy: use height gradient magnitude and sign to visualize
                // tectonic boundary character. Red=convergent (steep rise), blue=divergent
                // (rift drop), green=transform (lateral offset, low gradient).
                // Samples neighbours to compute gradient vector, then classifies.
                let bstep = 0.008;
                let h_r = textureSample(height_tex, height_sampler, rotated + vec3<f32>(bstep, 0.0, 0.0)).r;
                let h_l = textureSample(height_tex, height_sampler, rotated - vec3<f32>(bstep, 0.0, 0.0)).r;
                let h_u = textureSample(height_tex, height_sampler, rotated + vec3<f32>(0.0, bstep, 0.0)).r;
                let h_d = textureSample(height_tex, height_sampler, rotated - vec3<f32>(0.0, bstep, 0.0)).r;
                let grad_x = (h_r - h_l) * 0.5;
                let grad_y = (h_u - h_d) * 0.5;
                let grad_mag = sqrt(grad_x * grad_x + grad_y * grad_y);
                // Classify: strong positive rise → convergent (red), strong negative → divergent (blue),
                // high grad_mag but mixed sign → transform (green).
                let rise = (h_r - h_l + h_u - h_d) * 0.25; // net rise
                let convergent_str = clamp(rise * 20.0, 0.0, 1.0);
                let divergent_str  = clamp(-rise * 20.0, 0.0, 1.0);
                let transform_str  = clamp(grad_mag * 15.0 - convergent_str - divergent_str, 0.0, 1.0);
                debug_color = vec3<f32>(convergent_str, transform_str, divergent_str);
            }
            case 12u: {
                // Snow/ice coverage — show ice_amount as grayscale
                debug_color = vec3<f32>(ice_amount, ice_amount, ice_amount);
            }
            case 13u: {
                // Terrain normals — visualize shading_normal as RGB (normal map style)
                var n: vec3<f32>;
                if (is_ocean) {
                    n = normal;
                } else {
                    n = compute_terrain_normal(rotated, normal);
                }
                // Remap from [-1,1] to [0,1] for display
                debug_color = n * 0.5 + vec3<f32>(0.5);
            }
            case 14u: {
                // Wind direction: terrain-deflected, color-coded
                let wind_lat = asin(clamp(rotated.y, -1.0, 1.0));
                let tangent_w = wind_direction_at(rotated, wind_lat);
                // R = eastward, B = westward, G = poleward/equatorward
                debug_color = vec3<f32>(
                    max(tangent_w.x, 0.0),
                    abs(tangent_w.y) * 0.8,
                    max(-tangent_w.x, 0.0)
                ) + vec3<f32>(0.08);
            }
            case 15u: {
                // Ocean currents: warm (red) vs cold (blue) current zones
                if (is_ocean) {
                    let oc_temp = compute_temperature(rotated, height, uniforms.season);
                    let oc_lat = asin(clamp(rotated.y, -1.0, 1.0));
                    let oc_lat_norm = abs(oc_lat) / 1.5708;
                    // Expected temp at this latitude without currents
                    let expected_temp = 30.0 - 50.0 * (0.4 * oc_lat_norm + 0.6 * oc_lat_norm * oc_lat_norm);
                    let anomaly = oc_temp - expected_temp;
                    // Warm anomaly → red, cold → blue, neutral → grey
                    let warm = clamp(anomaly / 8.0, 0.0, 1.0);
                    let cold = clamp(-anomaly / 6.0, 0.0, 1.0);
                    debug_color = mix(vec3<f32>(0.3, 0.3, 0.4), vec3<f32>(0.9, 0.2, 0.1), warm);
                    debug_color = mix(debug_color, vec3<f32>(0.1, 0.3, 0.9), cold);
                } else {
                    let lh = clamp((height - uniforms.ocean_level) * 3.0, 0.0, 1.0);
                    debug_color = vec3<f32>(lh * 0.3 + 0.1);
                }
            }
            default: { debug_color = surface_color; }
        }
        return vec4<f32>(debug_color, 1.0);
    }

    // ---- PBR Lighting ----
    let light = normalize(uniforms.light_dir);
    let view_dir = vec3<f32>(0.0, 0.0, 1.0); // Camera looks along -Z, view = +Z
    let half_vec = normalize(light + view_dir);

    // Compute terrain-perturbed normal (flat for ocean — water surface is smooth)
    var shading_normal: vec3<f32>;
    if (is_ocean) {
        shading_normal = normal; // Geometric sphere normal — flat water
    } else {
        shading_normal = compute_terrain_normal(rotated, normal);
    }

    // PBR inputs
    let n_dot_l = max(dot(shading_normal, light), 0.0);
    let n_dot_v = max(dot(shading_normal, view_dir), 0.001);
    let n_dot_h = max(dot(shading_normal, half_vec), 0.0);
    let h_dot_v = max(dot(half_vec, view_dir), 0.0);

    // Roughness and Fresnel base reflectance
    let ocean_temp = compute_temperature(rotated, height, uniforms.season);
    let ocean_ice = is_ocean && ocean_temp < -2.0;
    let temp_for_rough = compute_temperature(rotated, height, uniforms.season);
    let moist_for_rough = compute_moisture(rotated, height, uniforms.season);
    let roughness = compute_roughness(temp_for_rough, moist_for_rough, is_ocean, ocean_ice);
    let f0 = select(0.04, 0.06, is_ocean); // Water has stronger Fresnel at glancing angles

    // GGX specular
    let d = ggx_distribution(n_dot_h, roughness);
    let f = fresnel_schlick(h_dot_v, f0);
    let specular = d * f / (4.0 * n_dot_v * n_dot_l + 0.001);

    // Diffuse (energy-conserving: reduce diffuse where specular is strong)
    let diffuse = surface_color * (1.0 - f) / 3.14159;

    // Ambient (subtle, directional — slightly brighter on the lit hemisphere)
    let ao = select(1.0, compute_ao(rotated), !is_ocean && uniforms.show_ao > 0.5);
    let ambient = surface_color * (0.06 + 0.04 * max(dot(normal, light), 0.0)) * ao;

    // Combine — tint direct light by star color
    var lit_color = ambient + (diffuse + specular) * n_dot_l * s_color;

    // Cloud shadow on surface (gated by show_clouds)
    if (uniforms.show_clouds > 0.5 && uniforms.cloud_coverage > 0.001) {
        let shadow_sample_pos = normalize(rotated + sun_dir * 0.015);
        let shadow_sfc_h = textureSample(height_tex, height_sampler, shadow_sample_pos).r;
        let cloud_above = compute_cloud_density(shadow_sample_pos, shadow_sfc_h);
        let surface_shadow = exp(-cloud_above * 3.0);
        // Multiply existing lit_color by shadow (preserves ice override, ambient, etc.)
        lit_color *= mix(1.0, surface_shadow, 0.65);
    }

    // Ice/snow brightness override (gated by show_ice)
    if (uniforms.show_ice > 0.5 && ice_amount > 0.01) {
        let ice_lit = s_color * (n_dot_l * 3.5 + 1.0); // HDR bright → tonemaps to white
        lit_color = mix(lit_color, ice_lit, ice_amount);
    }

    // ---- Night-side city lights (gated by show_cities) ----
    var city_glow_through = vec3<f32>(0.0);
    var city_glow_amount = 0.0;
    if (uniforms.show_cities > 0.5 && uniforms.night_lights > 0.0 && !is_ocean) {
        let night_factor = smooth_step(0.05, -0.1, n_dot_l);
        if (night_factor > 0.01) {
            let urban = compute_urban_density(rotated, height);
            if (urban > 0.01) {
                let sparkle = snoise(rotated * 300.0) * 0.3 + snoise(rotated * 600.0) * 0.2 + 0.5;
                let light_intensity = urban * night_factor * uniforms.night_lights * max(sparkle, 0.3);
                // City color from hue slider
                let amber = vec3<f32>(1.2, 0.85, 0.3);
                let white_led = vec3<f32>(1.1, 1.05, 1.0);
                let cool_blue = vec3<f32>(0.5, 0.7, 1.2);
                let hue = uniforms.city_light_hue;
                var city_col: vec3<f32>;
                if (hue < 0.5) {
                    city_col = mix(amber, white_led, hue * 2.0);
                } else {
                    city_col = mix(white_led, cool_blue, (hue - 0.5) * 2.0);
                }
                // Dim lights under cloud cover
                let cloud_above = compute_cloud_density(rotated, height);
                let cloud_block = exp(-cloud_above * 4.0); // thick clouds block most light
                lit_color += city_col * light_intensity * 1.2 * cloud_block;
                // Save glow for scatter through clouds
                city_glow_through = city_col * light_intensity * 0.3;
                city_glow_amount = cloud_above;
            }
        }
    }

    // ---- Two-layer cloud rendering (gated by show_clouds) ----
    if (uniforms.show_clouds > 0.5 && uniforms.cloud_coverage > 0.001) {
        // === Low cloud layer (cumulus / stratus / weather systems) ===
        let low_alt = max(uniforms.cloud_altitude, 0.01);
        let low_r = 1.0 + low_alt;
        let z_low = sqrt(max(low_r * low_r - r2, 0.0));
        let low_dir = normalize(vec3<f32>(ndc.x, ndc.y, z_low));
        let low_world = (uniforms.rotation * vec4<f32>(low_dir, 0.0)).xyz;

        let low_sfc_h = textureSample(height_tex, height_sampler, low_world).r;
        let low_density = compute_cloud_density(low_world, low_sfc_h);

        if (low_density > 0.005) {
            // Beer-Lambert with density-dependent thickness:
            // Thin clouds (density<0.3): low optical depth → translucent
            // Thick clouds (density>0.7): high optical depth → opaque white
            let thickness = mix(2.0, 6.0, low_density); // thin=2, thick=6
            let low_alpha = (1.0 - exp(-low_density * thickness)) * uniforms.cloud_opacity;

            // Self-shadowing — STRONGER contrast for 3D appearance
            let shadow_pos = normalize(low_world + sun_dir * 0.035);
            let shadow_h = textureSample(height_tex, height_sampler, shadow_pos).r;
            let shadow_density = compute_cloud_density(shadow_pos, shadow_h);
            let shadow = exp(-shadow_density * 3.5); // stronger shadow (was 2.5)

            // Cloud color: bright white → deep blue-grey shadow
            let lit_cloud = vec3<f32>(0.97, 0.97, 0.95) * s_color;
            let shadow_cloud = vec3<f32>(0.45, 0.50, 0.60); // darker shadow (was 0.55,0.58,0.65)
            var low_color = mix(shadow_cloud, lit_cloud, shadow);

            // Internal texture: density-proportional variation
            let cloud_tex = snoise(low_world * 25.0 + vec3<f32>(uniforms.cloud_seed * 5.3)) * 0.06
                          + snoise(low_world * 50.0 + vec3<f32>(uniforms.cloud_seed * 9.1)) * 0.03;
            low_color *= 1.0 + cloud_tex * low_density;

            // Day/night terminator
            let sun_facing = max(dot(low_dir, sun_dir), 0.0);
            let day_factor = smooth_step(-0.05, 0.2, sun_facing);
            low_color *= day_factor * 0.85 + 0.12;

            // Silver lining
            let cos_theta = dot(normalize(low_world), sun_dir);
            let hg = henyey_greenstein(cos_theta, 0.7);
            low_color += vec3<f32>(hg * low_density * 0.12);

            lit_color = mix(lit_color, low_color, low_alpha);
        }

        // === High cloud layer (cirrus — thin, icy, translucent) ===
        let high_alt = low_alt * 3.0; // cirrus at ~3x the low cloud altitude
        let high_r = 1.0 + high_alt;
        let z_high = sqrt(max(high_r * high_r - r2, 0.0));
        let high_dir = normalize(vec3<f32>(ndc.x, ndc.y, z_high));
        let high_world = (uniforms.rotation * vec4<f32>(high_dir, 0.0)).xyz;

        let cirrus_density = compute_cirrus_density(high_world);

        if (cirrus_density > 0.01) {
            // Cirrus: much thinner optical depth, more translucent
            let ci_alpha = (1.0 - exp(-cirrus_density * 2.0)) * uniforms.cloud_opacity;

            // Cirrus color: ice-white, less self-shadowing (thin layer)
            let ci_sun = max(dot(high_dir, sun_dir), 0.0);
            let ci_day = smooth_step(-0.05, 0.2, ci_sun);
            var ci_color = vec3<f32>(0.92, 0.93, 0.96) * s_color * (ci_day * 0.8 + 0.15);

            // Forward scattering stronger for thin ice crystals
            let ci_cos = dot(normalize(high_world), sun_dir);
            let ci_hg = henyey_greenstein(ci_cos, 0.8);
            ci_color += vec3<f32>(ci_hg * cirrus_density * 0.18);

            lit_color = mix(lit_color, ci_color, ci_alpha * 0.7);
        }
    }

    // City light scatter through clouds (needs both cities and clouds)
    if (uniforms.show_cities > 0.5 && uniforms.show_clouds > 0.5 && city_glow_amount > 0.05) {
        let scatter_strength = (1.0 - exp(-city_glow_amount * 2.0)) * 0.4; // thicker clouds scatter more
        lit_color += city_glow_through * scatter_strength;
    }

    // Ray-marched atmosphere (gated by show_atmosphere_layer)
    if (has_atm && uniforms.show_atmosphere_layer > 0.5) {
        let z_atm = sqrt(max(atm_radius * atm_radius - r2, 0.0));
        let z_surface = sqrt(1.0 - r2);
        let scatter = ray_march_atmosphere(ndc, z_atm, z_surface, sun_dir);
        lit_color = lit_color * scatter.transmittance + scatter.in_scatter;
    }

    // Tonemap (Reinhard)
    lit_color = lit_color / (lit_color + vec3<f32>(1.0));

    // Edge AA at planet boundary (when no atmosphere provides the transition)
    if (!has_atm) {
        let edge_bg = starfield(ndc, sun_dir, s_color);
        let edge_bg_tm = edge_bg / (edge_bg + vec3<f32>(1.0));
        let edge_aa = 1.0 - smooth_step(0.99, 1.0, sqrt(r2));
        lit_color = mix(edge_bg_tm, lit_color, edge_aa);
    }

    return vec4<f32>(lit_color, 1.0);
}
