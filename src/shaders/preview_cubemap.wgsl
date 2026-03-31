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
    _pad2: vec2<f32>,
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

fn compute_moisture(sphere_pos: vec3<f32>, height: f32, season: f32) -> f32 {
    let tilt = uniforms.axial_tilt_rad;
    let tilted_y = sphere_pos.y * cos(tilt) + sphere_pos.z * sin(tilt);
    let effective_lat = asin(clamp(tilted_y, -1.0, 1.0));

    // Shift Hadley cells with thermal equator (same sub-solar shift as temperature)
    let season_angle = (season - 0.5) * 2.0;
    let sub_solar_lat = tilt * season_angle;
    let thermal_lat = effective_lat - sub_solar_lat;

    // Hadley cell base moisture — scaled by ocean fraction FIRST.
    let ocean_scale = 0.05 + 0.95 * uniforms.ocean_fraction;
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

// ---- Cloud density function ----
// Multi-layer cloud system: cyclone spirals, ITCZ convection, marine stratocumulus,
// fair-weather cumulus, cirrus wisps. Climate-driven by moisture + temperature.

fn compute_cloud_density(sphere_pos: vec3<f32>, height: f32) -> f32 {
    let coverage = uniforms.cloud_coverage;
    if (coverage <= 0.0) { return 0.0; }

    // Seed offset: pre-hashed on CPU via seed_to_offset() → always in [0, 97)
    let s = uniforms.cloud_seed;
    let seed_off = vec3<f32>(s, fract(s * 1.618) * 89.0, fract(s * 2.618) * 83.0);

    // --- Climate data ---
    let tilt = uniforms.axial_tilt_rad;
    let tilted_y = sphere_pos.y * cos(tilt) + sphere_pos.z * sin(tilt);
    let lat_rad = asin(clamp(tilted_y, -1.0, 1.0));
    let lat_deg = abs(lat_rad) * 180.0 / 3.14159;
    let moisture = compute_moisture(sphere_pos, height, 0.5);
    let temp = compute_temperature(sphere_pos, height, 0.5);
    let is_ocean = height < uniforms.ocean_level;
    let moisture_w = smooth_step(30.0, 120.0, moisture);

    // ================================================================
    // WEATHER PATTERN — large-scale clear/cloudy zones (breaks banding)
    // ================================================================
    let wp = sphere_pos + seed_off * 0.3;
    let weather = snoise(wp * 1.5) * 0.45
                + snoise(wp * 3.0 + vec3<f32>(17.0, 5.0, 31.0)) * 0.35
                + snoise(wp * 6.0 + vec3<f32>(7.0, 23.0, 11.0)) * 0.20;
    // Subtropical highs: force clear sky at ~25-35° (descending Hadley air)
    let subtropical_clear = exp(-((lat_deg - 30.0) * (lat_deg - 30.0)) / 80.0) * 0.5;
    let weather_mask = smooth_step(-0.15, 0.35, weather - subtropical_clear);

    // ================================================================
    // CYCLONE SYSTEMS — cloud masses with loose spiral tendency
    // ================================================================
    var storm = 0.0;
    for (var i = 0; i < 4; i++) {
        let fi = f32(i);
        let slat = (32.0 + fract(sin(fi * 127.1 + s) * 43758.5) * 22.0) * 3.14159 / 180.0;
        let slon = fract(sin(fi * 311.7 + s * 1.3) * 23421.6) * 6.28318;
        let sign_y = select(-1.0, 1.0, i % 2 == 0);
        let center = normalize(vec3<f32>(
            cos(slat) * cos(slon),
            sin(slat) * sign_y,
            cos(slat) * sin(slon)
        ));

        let d = acos(clamp(dot(sphere_pos, center), -1.0, 1.0));

        // Tangent plane (avoid degenerate cross product near poles)
        let up = select(vec3<f32>(0.0, 0.0, 1.0), vec3<f32>(0.0, 1.0, 0.0), abs(center.y) < 0.9);
        let tx = normalize(cross(up, center));
        let ty = cross(center, tx);
        let to_pt = sphere_pos - center * dot(sphere_pos, center);
        let angle = atan2(dot(to_pt, ty), dot(to_pt, tx));

        // Storm envelope
        let storm_radius = 20.0 + fract(sin(fi * 73.1 + s * 0.7) * 19283.3) * 15.0;
        let falloff = exp(-d * d * storm_radius);

        // PRIMARY: cloud texture (ridged noise = puffy cloud shapes)
        let cp = sphere_pos + seed_off + vec3<f32>(fi * 17.0, 0.0, 0.0);
        let cloud_tex = (1.0 - abs(snoise(cp * 8.0))) * 0.45
                      + (1.0 - abs(snoise(cp * 16.0 + vec3<f32>(3.1, 7.2, 1.5)))) * 0.35
                      + (1.0 - abs(snoise(cp * 32.0 + vec3<f32>(5.7, 2.3, 8.1)))) * 0.20;

        // SECONDARY: spiral is just a soft density bias (NOT hard bands)
        let spiral_phase = angle * sign_y - log(max(d, 0.005)) * 3.5;
        let spiral_bias = cos(spiral_phase * 2.0) * 0.25 + 0.75; // range 0.5–1.0

        // Small eye
        let eye = smooth_step(0.015, 0.05, d);

        storm += falloff * cloud_tex * spiral_bias * eye;
    }
    storm = clamp(storm, 0.0, 1.0);

    // ================================================================
    // ITCZ CONVECTION — towering cumulonimbus clusters near equator
    // ================================================================
    let p = sphere_pos + seed_off;
    let cb1 = 1.0 - abs(snoise(p * 4.0));
    let cb2 = 1.0 - abs(snoise(p * 8.0 + vec3<f32>(5.1, 3.3, 1.7)));
    let cb3 = snoise(p * 16.0 + vec3<f32>(2.4, 6.1, 1.3)) * 0.15;
    let convective = cb1 * 0.5 + cb2 * 0.35 + cb3;
    let conv_shaped = max(convective * convective - 0.1, 0.0) * 1.2; // sharpen, more clear gaps
    let itcz_w = smooth_step(18.0, 5.0, lat_deg) * smooth_step(18.0, 26.0, temp);
    let itcz_cloud = conv_shaped * itcz_w;

    // ================================================================
    // STRATOCUMULUS — marine patches over cool oceans
    // ================================================================
    let ps = sphere_pos + seed_off * 0.6 + vec3<f32>(33.0, 7.0, 51.0);
    let sc1 = snoise(ps * 10.0);
    let sc2 = snoise(ps * 20.0 + vec3<f32>(4.1, 2.7, 6.3)) * 0.4;
    let sc_pattern = smooth_step(-0.1, 0.35, sc1 + sc2);
    let sc_temp_w = smooth_step(8.0, 14.0, temp) * smooth_step(22.0, 16.0, temp);
    let sc_lat_w = smooth_step(15.0, 25.0, lat_deg) * smooth_step(45.0, 35.0, lat_deg);
    let ocean_f = select(0.0, 1.0, is_ocean);
    let stratocumulus = sc_pattern * sc_temp_w * sc_lat_w * ocean_f * 0.55;

    // ================================================================
    // FAIR-WEATHER CUMULUS — small isolated puffs
    // ================================================================
    let pp = sphere_pos + seed_off * 0.8 + vec3<f32>(41.0, 17.0, 63.0);
    let puff1 = 1.0 - abs(snoise(pp * 12.0));
    let puff2 = 1.0 - abs(snoise(pp * 24.0 + vec3<f32>(7.3, 1.9, 4.1)));
    let puff = smooth_step(0.7, 0.9, puff1 * 0.6 + puff2 * 0.4);
    let land_f = select(0.0, 1.0, !is_ocean);
    let warm_w = smooth_step(10.0, 25.0, temp);
    let fair_cu = puff * (0.2 + 0.3 * land_f * warm_w) * moisture_w;

    // ================================================================
    // CIRRUS — thin ice-crystal wisps at jet stream
    // ================================================================
    let pc = sphere_pos * vec3<f32>(1.0, 0.7, 1.0) + seed_off + vec3<f32>(50.0, 30.0, 70.0);
    let ci1 = snoise(pc * 6.0);
    let ci2 = snoise(pc * 12.0 + vec3<f32>(3.7, 1.1, 8.3)) * 0.5;
    let cirrus = smooth_step(0.4, 0.75, (ci1 + ci2) * 0.4 + 0.5) * 0.35;
    let cirrus_w = smooth_step(25.0, 50.0, lat_deg);

    // ================================================================
    // Combine all layers — weather pattern modulates, not gates
    // ================================================================
    // Moisture softens but doesn't zero out (even deserts have some clouds)
    let moist_mod = 0.3 + 0.7 * moisture_w;
    // Weather pattern provides spatial variation, not hard cutoff
    let weather_mod = 0.2 + 0.8 * weather_mask;

    var density = storm  // cyclones are self-contained
               + (itcz_cloud * 0.8
                + stratocumulus * 0.7
                + fair_cu * 0.6
                + cirrus * cirrus_w * 0.5) * weather_mod * moist_mod;

    // Orographic lift
    let wind = wind_direction_vec(lat_rad);
    let tangent_wind = normalize(wind - sphere_pos * dot(wind, sphere_pos));
    let upwind_pos = normalize(sphere_pos + tangent_wind * 0.04);
    let upwind_h = textureSample(height_tex, height_sampler, upwind_pos).r;
    density += smooth_step(0.04, 0.22, max(upwind_h - uniforms.ocean_level, 0.0)) * 0.15;

    // Global base cloud layer — fills in at high coverage values
    // fBm noise that provides ubiquitous cloud potential
    let gp = sphere_pos + seed_off * 0.4 + vec3<f32>(71.0, 13.0, 47.0);
    let global_base = snoise(gp * 3.0) * 0.4
                    + snoise(gp * 7.0 + vec3<f32>(3.0, 9.0, 1.0)) * 0.3
                    + snoise(gp * 14.0 + vec3<f32>(6.0, 2.0, 8.0)) * 0.2;
    let global_cloud = smooth_step(-0.1, 0.4, global_base * 0.5 + 0.5) * moist_mod * 0.5;
    density = max(density, global_cloud);

    density = clamp(density, 0.0, 1.0);

    // Coverage threshold — quadratic for usable range
    let threshold = (1.0 - coverage) * (1.0 - coverage);
    return smooth_step(threshold, threshold + 0.12, density);
}

// ---- Continuous gradient biome coloring ----
// Replaces discrete Whittaker lookup with smooth 2D interpolation.
// Temperature × moisture → color via 3×2 anchor grid.

fn gradient_color(mean_temp: f32, mean_moisture: f32, seasonal_temp: f32, variation: f32) -> vec3<f32> {
    // Biome classification uses MEAN ANNUAL temperature/moisture
    // so biome type stays stable across seasons
    let t_cold = smooth_step(-8.0, 10.0, mean_temp);   // 0 = cold, 1 = temperate+
    let t_hot = smooth_step(10.0, 28.0, mean_temp);     // 0 = temperate, 1 = hot

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
    let moist_t = smooth_step(10.0, 90.0, mean_moisture);
    var base = mix(dry_color, wet_color, moist_t);

    // Seasonal color modulation — subtle shifts, NOT biome changes
    // Uses temperature deviation from annual mean
    let temp_deviation = seasonal_temp - mean_temp;
    let green_amount = max(base.g - max(base.r, base.b), 0.0);
    if (green_amount > 0.05) {
        // Winter (negative deviation): subtle golden-brown (deciduous leaf loss)
        // Summer (positive deviation): slightly more vibrant
        let winter_factor = clamp(-temp_deviation / 20.0, 0.0, 1.0);
        let summer_factor = clamp(temp_deviation / 20.0, 0.0, 1.0);
        let winter_shift = vec3<f32>(0.06, -0.02, -0.03) * winter_factor * green_amount * 2.0;
        let summer_shift = vec3<f32>(-0.01, 0.02, 0.0) * summer_factor * green_amount;
        base += winter_shift + summer_shift;
    }
    if (seasonal_temp < 5.0 && mean_temp < 15.0) {
        // Cold regions: whiter in winter (snow cover is seasonal)
        let cold_winter = clamp(-temp_deviation / 15.0, 0.0, 1.0);
        base = mix(base, vec3<f32>(0.80, 0.82, 0.85), cold_winter * 0.25 * (1.0 - t_cold));
    }

    // Per-pixel noise variation for natural texture
    base += base * variation * 0.10;

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
        return 0.10; // Smooth water — wide enough specular to be visible at planet scale
    }
    // Land roughness from climate
    if (temp_c < 0.0) { return 0.20; } // Snow/ice
    if (temp_c < 10.0) { return 0.55; } // Tundra/Taiga
    if (moisture_cm < 25.0) { return 0.80; } // Desert
    if (moisture_cm < 100.0) { return 0.55; } // Grassland
    return 0.45; // Forest
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

// ---- Main fragment shader ----
@fragment
fn fs_main(in: VertexOutput) -> @location(0) vec4<f32> {
    let background = vec3<f32>(0.0, 0.0, 0.0);
    let pan = vec2<f32>(uniforms.pan_x, uniforms.pan_y);
    let ndc = ((in.uv - 0.5) * 2.0 / 0.85 - pan) / uniforms.zoom;
    let r2 = dot(ndc, ndc);

    let atm_h = uniforms.atmosphere_height;
    let atm_radius = 1.0 + atm_h;
    let has_atm = uniforms.atmosphere_density > 0.001 && atm_h > 0.001;
    let outer_r = select(1.005, atm_radius + 0.015, has_atm);

    // Miss everything — outside both planet and atmosphere
    if (r2 > outer_r * outer_r) {
        return vec4<f32>(background, 1.0);
    }

    let sun_dir = normalize(uniforms.light_dir);
    let hit_planet = r2 < 1.0;

    // Atmosphere-only ring (between planet edge and outer atmosphere boundary)
    if (!hit_planet) {
        if (!has_atm || uniforms.view_mode != 0u) {
            return vec4<f32>(background, 1.0);
        }
        let z_atm = sqrt(max(atm_radius * atm_radius - r2, 0.0));
        let scatter = ray_march_atmosphere(ndc, z_atm, -z_atm, sun_dir);
        var ring_color = scatter.in_scatter;
        ring_color = ring_color / (ring_color + vec3<f32>(1.0)); // tonemap
        let edge = 1.0 - smooth_step(atm_radius - 0.015, atm_radius, sqrt(r2));
        return vec4<f32>(mix(background, ring_color, edge), 1.0);
    }

    // Planet surface hit
    let normal = normalize(vec3<f32>(ndc.x, ndc.y, sqrt(1.0 - r2)));
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
        let ocean_temp = compute_temperature(rotated, height, uniforms.season);
        let depth = clamp((uniforms.ocean_level - height) / max(uniforms.ocean_level + 1.0, 0.5), 0.0, 1.0);

        // 3-stop depth gradient: turquoise shelf → mid blue → deep navy
        let near_shore = vec3<f32>(0.10, 0.35, 0.42);
        let mid_ocean  = vec3<f32>(0.06, 0.18, 0.42);
        let deep_ocean = vec3<f32>(0.02, 0.05, 0.20);
        let shelf = smoothstep(0.0, 0.15, depth);
        let abyss = smoothstep(0.15, 0.7, depth);
        var ocean_color = mix(near_shore, mix(mid_ocean, deep_ocean, abyss), shelf);
        ocean_color += vec3<f32>(0.0, 0.015, 0.02) * color_var;

        // Smooth ice transition: no hard cutoff, gradual freeze
        let ice_blend = smooth_step(3.0, -8.0, ocean_temp); // starts blending at 3°C, full ice at -8°C
        let ice_color = mix(vec3<f32>(0.65, 0.75, 0.85), vec3<f32>(0.88, 0.92, 0.96), clamp(-ocean_temp / 15.0, 0.0, 1.0));
        surface_color = mix(ocean_color, ice_color, ice_blend);
    } else {
        // Mean annual climate for biome classification (stable across seasons)
        let mean_temp = compute_temperature(rotated, height, 0.5);
        let mean_moisture = compute_moisture(rotated, height, 0.5);
        // Seasonal climate for color modulation only
        let seasonal_temp = compute_temperature(rotated, height, uniforms.season);

        // Continuous gradient coloring — biome type from mean, color shift from season
        surface_color = gradient_color(mean_temp, mean_moisture, seasonal_temp, color_var);

        // Ice/snow overlay: only for genuinely glaciated land (very cold + high altitude or extreme cold)
        // Tundra (cold flat land) stays brown/tan from gradient_color, NOT white
        let land_height = (height - uniforms.ocean_level) / max(1.0 - uniforms.ocean_level, 0.01);
        let ice_moisture_threshold = 15.0 + 40.0 * (1.0 - uniforms.ocean_fraction);
        // Ice requires BOTH extreme cold AND either high altitude or very high moisture
        let altitude_ice = smooth_step(0.3, 0.6, land_height); // high terrain gets ice easier
        let cold_factor = smooth_step(-15.0, -30.0, seasonal_temp);
        let ice_blend = cold_factor * max(altitude_ice, smooth_step(ice_moisture_threshold * 0.7, ice_moisture_threshold, mean_moisture));
        let ice_color = vec3<f32>(0.90, 0.93, 0.97) + vec3<f32>(0.02) * color_var;
        surface_color = mix(surface_color, ice_color, ice_blend);

        // Altitude zonation — only the highest peaks get snow/rock
        let snow_line = 0.85 + 0.10 * (1.0 - abs(effective_lat) / 1.5708);
        let rock_line = snow_line - 0.10;
        let alpine_line = rock_line - 0.10;

        if (land_height > snow_line && seasonal_temp < 15.0) {
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
                debug_color = gradient_color(mean_t, mean_m, mean_t, 0.0) * 1.3;
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
                // Roughness visualization
                if (is_ocean) {
                    debug_color = vec3<f32>(0.05, 0.05, 0.1); // Water = very smooth (dark)
                } else {
                    let rt = compute_temperature(rotated, height, uniforms.season);
                    let rm = compute_moisture(rotated, height, uniforms.season);
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
    let ambient = surface_color * (0.06 + 0.04 * max(dot(normal, light), 0.0));

    // Combine
    var lit_color = ambient + (diffuse + specular) * n_dot_l;

    // ---- Cloud shell (rendered before atmosphere so atm scattering wraps over clouds) ----
    if (uniforms.cloud_coverage > 0.001) {
        // Find where the view ray hits the cloud shell sphere (always above surface)
        let cloud_r = 1.0 + uniforms.cloud_altitude;
        let z_cloud = sqrt(max(cloud_r * cloud_r - r2, 0.0));
        let cloud_dir = normalize(vec3<f32>(ndc.x, ndc.y, z_cloud));
        let cloud_world = (uniforms.rotation * vec4<f32>(cloud_dir, 0.0)).xyz;

        let cloud_sfc_h = textureSample(height_tex, height_sampler, cloud_world).r;
        let cloud_density = compute_cloud_density(cloud_world, cloud_sfc_h);

        if (cloud_density > 0.03) {
            let sun_dot = max(dot(cloud_dir, sun_dir), 0.0);
            let cloud_lit = smooth_step(-0.05, 0.2, sun_dot);

            // Cloud-top roughness noise: breaks up flat surfaces
            let ctn = snoise(cloud_world * 20.0) * 0.12
                    + snoise(cloud_world * 40.0) * 0.08;
            // Self-shadowing: thick clouds have darker bases, brighter tops
            let thickness = cloud_density;
            let top_bright = 0.95 + ctn; // bright cloud tops with bumpy texture
            let base_shadow = mix(0.6, top_bright, thickness); // thin=darker, thick=brighter tops

            let brightness = base_shadow * (cloud_lit * 0.82 + 0.15);
            let cloud_color = vec3<f32>(brightness, brightness, brightness * 1.01);

            // Alpha: continuous ramp, thin clouds translucent
            let cloud_alpha = smooth_step(0.03, 0.4, cloud_density) * 0.93;
            lit_color = mix(lit_color, cloud_color, cloud_alpha);
        }
    }

    // Ray-marched atmosphere (in HDR space, before tonemapping)
    if (has_atm) {
        let z_atm = sqrt(max(atm_radius * atm_radius - r2, 0.0));
        let z_surface = sqrt(1.0 - r2);
        let scatter = ray_march_atmosphere(ndc, z_atm, z_surface, sun_dir);
        lit_color = lit_color * scatter.transmittance + scatter.in_scatter;
    }

    // Tonemap (Reinhard)
    lit_color = lit_color / (lit_color + vec3<f32>(1.0));

    // Edge AA at planet boundary (when no atmosphere provides the transition)
    if (!has_atm) {
        let edge_aa = 1.0 - smooth_step(0.99, 1.0, sqrt(r2));
        lit_color = mix(background, lit_color, edge_aa);
    }

    return vec4<f32>(lit_color, 1.0);
}
