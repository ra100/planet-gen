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
            let base_sigma = 22.0 + fract(sin(fi * 73.1 + s * 0.7) * 19283.3) * 12.0;
            let storm_sigma = base_sigma / max(uniforms.storm_size * uniforms.storm_size, 0.1);
            let influence = exp(-d * d * storm_sigma);

            // Tangent rotation: strong vortex near center, Coriolis-correct
            let rotation_amount = influence * sign_y * 3.0 / max(d * 5.0, 0.2);
            let tangent = cross(center, sphere_pos);
            vortex_sphere = normalize(vortex_sphere + tangent * rotation_amount * 0.03);
        }
    }

    // === Step 1: Two noise bases with different character ===
    let p_base = vortex_sphere * 5.0 + seed_off;

    // --- Stratus layer: domain-warped fBm (smooth, flowing) ---
    let warp = vec3<f32>(
        snoise(p_base * 0.7 + vec3<f32>(31.7, 0.0, 0.0)),
        snoise(p_base * 0.7 + vec3<f32>(0.0, 47.3, 0.0)),
        snoise(p_base * 0.7 + vec3<f32>(0.0, 0.0, 73.1))
    ) * 0.5;
    let warped_p = p_base + warp;

    var fbm_val = 0.0;
    var freq = 1.0;
    var amp = 1.0;
    var amp_sum = 0.0;
    for (var i = 0; i < 5; i++) {
        fbm_val += snoise(warped_p * freq) * amp;
        amp_sum += amp;
        freq *= 2.1;
        amp *= 0.52;
    }
    fbm_val = fbm_val / amp_sum * 0.5 + 0.5; // [0, 1]

    // --- Cumulus layer: clamped-positive peaks (isolated blobs, NOT webby ridges) ---
    // max(noise, 0) keeps only positive peaks → scattered rounded blobs
    let cp = p_base + vec3<f32>(13.7, 7.3, 21.1); // decorrelate, NO domain warp
    var cumulus_val = 0.0;
    var c_freq = 1.0;
    var c_amp = 1.0;
    var c_amp_sum = 0.0;
    for (var i = 0; i < 4; i++) {
        cumulus_val += max(snoise(cp * c_freq), 0.0) * c_amp;
        c_amp_sum += c_amp;
        c_freq *= 2.3;
        c_amp *= 0.5;
    }
    cumulus_val = cumulus_val / c_amp_sum; // [0, ~0.5] — mostly low with isolated peaks
    cumulus_val = pow(cumulus_val, 1.3); // round off peaks for softer blobs

    // === Step 2: Spatial blend — smooth stratus vs puffy cumulus per region ===
    // cloud_type uniform (0=stratus, 1=cumulus) sets the center; spatial noise adds local variation
    let type_noise = snoise(sphere_pos * 1.8 + seed_off * 0.15 + vec3<f32>(97.0, 41.0, 63.0));
    let type_blend = clamp(type_noise * 0.35 + uniforms.cloud_type, 0.0, 1.0);
    let noise_val = mix(fbm_val, cumulus_val, type_blend);

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

    // Start with global coverage blended with moisture
    var local_coverage = mix(coverage, moisture_norm, 0.35);

    // NOTE: No per-pixel ocean/land coverage adjustment — it creates visible
    // terrain-following edges at coastlines. The moisture model already captures
    // climate differences between land and ocean.

    // Orographic lift: mountains force air up → condensation on windward side
    let tilt = uniforms.axial_tilt_rad;
    let tilted_y = sphere_pos.y * cos(tilt) + sphere_pos.z * sin(tilt);
    let lat_rad = asin(clamp(tilted_y, -1.0, 1.0));
    let wind = wind_direction_vec(lat_rad);
    let tangent_wind = normalize(wind - sphere_pos * dot(wind, sphere_pos));
    let upwind_pos = normalize(sphere_pos + tangent_wind * 0.04);
    let upwind_h = textureSample(height_tex, height_sampler, upwind_pos).r;
    let mountain_lift = smooth_step(0.04, 0.25, max(upwind_h - uniforms.ocean_level, 0.0));
    local_coverage += mountain_lift * 0.12;

    // Warm convection: warmer areas produce more convective cloud potential
    let convection_boost = smooth_step(15.0, 30.0, temp) * 0.05;
    local_coverage += convection_boost;

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
            let base_sigma = 22.0 + fract(sin(fi * 73.1 + s * 0.7) * 19283.3) * 12.0;
            let storm_sigma = base_sigma / max(uniforms.storm_size * uniforms.storm_size, 0.1);
            let falloff = exp(-d * d * storm_sigma);
            // Strong coverage boost — storms should be dramatically cloudier
            local_coverage += falloff * 0.7;
        }
    }

    local_coverage = clamp(local_coverage, 0.0, 1.0);

    // === Step 4: Multi-scale pattern variety ===
    let weather_scale = snoise(sphere_pos * 2.0 + seed_off * 0.2) * 0.15;
    let varied_noise = clamp(noise_val + weather_scale, 0.0, 1.0);

    // === Step 5: Schneider remap ===
    var density = cloud_remap(varied_noise, 1.0 - local_coverage, 1.0, 0.0, 1.0) * local_coverage;

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

            // Eye: clear center with noisy edge
            let eye_r = (0.03 + fract(sin(fi * 53.7 + s * 0.3) * 31415.9) * 0.02) * uniforms.storm_size;
            let eye_noise = snoise(sphere_pos * 30.0 + seed_off + vec3<f32>(fi * 11.0, 0.0, 0.0)) * eye_r * 0.4;
            let eye_mask = smooth_step((eye_r + eye_noise) * 0.3, (eye_r + eye_noise) * 1.2, d);

            // Dense eye wall ring
            let wall_dist = abs(d - eye_r * 1.3);
            let eye_wall_boost = exp(-wall_dist * wall_dist / (eye_r * eye_r * 0.5)) * near_storm * 0.4;
            density += eye_wall_boost;

            // Spiral arms: ADD dense bands + CARVE gaps (both needed for visibility)
            let spiral_phase = angle * sign_y - log(max(d, 0.003)) * 3.5;
            let spiral_raw = cos(spiral_phase * 2.0);
            let spiral_fade = smooth_step(eye_r * 1.5, eye_r * 4.0, d);
            let storm_fade2 = near_storm * near_storm; // squared = faster outer cutoff

            // Add dense TEXTURED cloud along spiral arms (not smooth arcs)
            let arm_tex = snoise(sphere_pos * 15.0 + seed_off + vec3<f32>(fi * 13.0, 0.0, 0.0)) * 0.3 + 0.7;
            let arm_strength = pow(max(spiral_raw, 0.0), 1.2) * arm_tex;
            let arm_boost = arm_strength * storm_fade2 * spiral_fade * 0.55;
            density += arm_boost;

            // Carve gaps between arms (only where near_storm is strong)
            let gap_depth = storm_fade2 * spiral_fade * 0.85;
            let arm_shape = spiral_raw * 0.5 + 0.5;
            let spiral_mask = 1.0 - gap_depth * (1.0 - max(arm_shape, arm_tex * 0.3));

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

    // Slight E-W elongation (jet stream alignment)
    let p = sphere_pos * vec3<f32>(1.0, 0.6, 1.0) * 6.0 + seed_off + vec3<f32>(50.0, 30.0, 70.0);

    // 3-octave high-frequency noise
    let ci = snoise(p) * 0.5
           + snoise(p * 2.1 + vec3<f32>(3.7, 1.1, 8.3)) * 0.3
           + snoise(p * 4.4 + vec3<f32>(1.3, 5.9, 2.1)) * 0.2;
    let ci_norm = ci * 0.5 + 0.5;

    // Cirrus more common at mid-to-high latitudes
    let tilt = uniforms.axial_tilt_rad;
    let tilted_y = sphere_pos.y * cos(tilt) + sphere_pos.z * sin(tilt);
    let lat_deg = abs(asin(clamp(tilted_y, -1.0, 1.0))) * 180.0 / 3.14159;
    let lat_boost = smooth_step(20.0, 50.0, lat_deg) * 0.15;

    let cirrus_cov = cov * 0.4 + lat_boost; // cirrus coverage is fraction of main
    let density = cloud_remap(ci_norm, 1.0 - cirrus_cov, 1.0, 0.0, 1.0) * cirrus_cov;
    return max(density, 0.0);
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

// ---- Terrain ambient occlusion ----
// Samples height neighbors to darken valleys and crevices.
fn compute_ao(sphere_pos: vec3<f32>) -> f32 {
    let step = 0.003; // sampling radius
    let h_center = textureSample(height_tex, height_sampler, sphere_pos).r;

    // 8 neighbor samples (cardinal + diagonal)
    var occlusion = 0.0;
    let offsets = array<vec3<f32>, 8>(
        vec3<f32>(step, 0.0, 0.0), vec3<f32>(-step, 0.0, 0.0),
        vec3<f32>(0.0, step, 0.0), vec3<f32>(0.0, -step, 0.0),
        vec3<f32>(step, step, 0.0) * 0.707, vec3<f32>(-step, step, 0.0) * 0.707,
        vec3<f32>(step, -step, 0.0) * 0.707, vec3<f32>(-step, -step, 0.0) * 0.707
    );
    for (var i = 0; i < 8; i++) {
        let neighbor = textureSample(height_tex, height_sampler, sphere_pos + offsets[i]).r;
        // If neighbor is higher, this pixel is in a crevice → occluded
        let height_diff = max(neighbor - h_center, 0.0);
        occlusion += smooth_step(0.0, 0.08, height_diff);
    }
    // Normalize: 0 = fully occluded (deep valley), 1 = fully exposed (ridge)
    let ao = 1.0 - occlusion * 0.1; // max darkening ~80%
    return clamp(ao, 0.2, 1.0);
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

    let urban_raw = score * city_pattern;
    let threshold = (1.0 - dev) * 0.35;
    return smooth_step(threshold, threshold + 0.06, urban_raw);
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
        if (!has_atm || uniforms.view_mode != 0u) {
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

    // Compute effective latitude for altitude zonation (consistent tilt model)
    let tilt_main = uniforms.axial_tilt_rad;
    let tilted_y_main = rotated.y * cos(tilt_main) + rotated.z * sin(tilt_main);
    let effective_lat = asin(clamp(tilted_y_main, -1.0, 1.0));

    var surface_color: vec3<f32>;
    var ice_amount = 0.0; // tracks ice coverage for HDR override

    if (is_ocean) {
        // Smooth ocean gradient: shallow → deep with continuous depth color
        let ocean_temp = compute_temperature(rotated, height, uniforms.season);
        let depth = clamp((uniforms.ocean_level - height) / max(uniforms.ocean_level + 1.0, 0.5), 0.0, 1.0);

        let near_shore = vec3<f32>(0.10, 0.35, 0.42);
        let mid_ocean  = vec3<f32>(0.06, 0.18, 0.42);
        let deep_ocean = vec3<f32>(0.02, 0.05, 0.20);
        let shelf = smoothstep(0.0, 0.15, depth);
        let abyss = smoothstep(0.15, 0.7, depth);
        var ocean_color = mix(near_shore, mix(mid_ocean, deep_ocean, abyss), shelf);
        ocean_color += vec3<f32>(0.0, 0.015, 0.02) * color_var;

        // Polar sea ice: noisy edge, sharper transition
        let ice_edge_noise = snoise(rotated * 12.0) * 1.5;
        let ice_temp_threshold = 3.0 + ice_edge_noise;
        let ice_blend = smooth_step(ice_temp_threshold, ice_temp_threshold - 5.0, ocean_temp); // sharper: 5°C band
        ice_amount = ice_blend;
        // Ice color: always bright HDR (the override handles PBR brightness)
        let ice_color = vec3<f32>(1.15, 1.18, 1.25) + vec3<f32>(0.015) * color_var;
        surface_color = mix(ocean_color, ice_color, ice_blend);
    } else {
        // Mean annual climate for biome classification (stable across seasons)
        let mean_temp = compute_temperature(rotated, height, 0.5);
        let mean_moisture = compute_moisture(rotated, height, 0.5);
        // Seasonal climate for color modulation only
        let seasonal_temp = compute_temperature(rotated, height, uniforms.season);

        // Continuous gradient coloring — biome type from mean, color shift from season
        surface_color = gradient_color(mean_temp, mean_moisture, seasonal_temp, color_var);

        // Land ice/snow: temperature is primary driver, altitude/moisture secondary
        let land_height = (height - uniforms.ocean_level) / max(1.0 - uniforms.ocean_level, 0.01);
        let land_ice_noise = snoise(rotated * 10.0) * 2.0;

        // Pure cold-based snow: very cold land gets snow regardless of moisture
        let cold_snow = smooth_step(-5.0 + land_ice_noise, -20.0, seasonal_temp);
        // Altitude bonus: high terrain gets snow easier
        let altitude_bonus = smooth_step(0.3, 0.6, land_height) * smooth_step(5.0, -5.0, seasonal_temp);
        let ice_blend = max(cold_snow, altitude_bonus);
        ice_amount = max(ice_amount, ice_blend);

        // HDR bright snow (override handles PBR brightness)
        let glacier_blue = vec3<f32>(1.05, 1.15, 1.25);
        let fresh_snow = vec3<f32>(1.20, 1.22, 1.25) + vec3<f32>(0.015) * color_var;
        let land_ice_color = mix(fresh_snow, glacier_blue, land_height * cold_snow);
        surface_color = mix(surface_color, land_ice_color, ice_blend);

        // Altitude zonation — only the highest peaks get snow/rock
        let snow_line = 0.85 + 0.10 * (1.0 - abs(effective_lat) / 1.5708);
        let rock_line = snow_line - 0.10;
        let alpine_line = rock_line - 0.10;

        if (land_height > snow_line && seasonal_temp < 15.0) {
            let blend = smooth_step(snow_line, snow_line + 0.10, land_height);
            surface_color = mix(surface_color, vec3<f32>(1.15, 1.18, 1.22), blend);
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

    // Day-side urban grey patches (dark concrete/asphalt tone)
    if (uniforms.night_lights > 0.0 && !is_ocean) {
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
    let ao = select(1.0, compute_ao(rotated), !is_ocean); // AO only on land
    let ambient = surface_color * (0.06 + 0.04 * max(dot(normal, light), 0.0)) * ao;

    // Combine — tint direct light by star color
    var lit_color = ambient + (diffuse + specular) * n_dot_l * s_color;

    // Cloud shadow on surface: darken where clouds above block sunlight
    if (uniforms.cloud_coverage > 0.001) {
        let shadow_sample_pos = normalize(rotated + sun_dir * 0.015);
        let shadow_sfc_h = textureSample(height_tex, height_sampler, shadow_sample_pos).r;
        let cloud_above = compute_cloud_density(shadow_sample_pos, shadow_sfc_h);
        let surface_shadow = exp(-cloud_above * 3.0);
        // Multiply existing lit_color by shadow (preserves ice override, ambient, etc.)
        lit_color *= mix(1.0, surface_shadow, 0.65);
    }

    // Ice/snow brightness override: uses tracked ice_amount (not surface_color detection)
    // so partial ice blend still gets brightened instead of staying grey.
    if (ice_amount > 0.01) {
        let ice_lit = s_color * (n_dot_l * 3.5 + 1.0); // HDR bright → tonemaps to white
        lit_color = mix(lit_color, ice_lit, ice_amount);
    }

    // ---- Night-side city lights (rendered before clouds so clouds occlude them) ----
    var city_glow_through = vec3<f32>(0.0); // stored for scatter through clouds
    var city_glow_amount = 0.0;
    if (uniforms.night_lights > 0.0 && !is_ocean) {
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

    // ---- Two-layer cloud rendering ----
    // Layer 1 (low): cumulus/stratus at ~0.01 planet radii above surface
    // Layer 2 (high): cirrus at ~0.03 planet radii (jet stream altitude)
    if (uniforms.cloud_coverage > 0.001) {
        // === Low cloud layer (cumulus / stratus / weather systems) ===
        let low_alt = max(uniforms.cloud_altitude, 0.01);
        let low_r = 1.0 + low_alt;
        let z_low = sqrt(max(low_r * low_r - r2, 0.0));
        let low_dir = normalize(vec3<f32>(ndc.x, ndc.y, z_low));
        let low_world = (uniforms.rotation * vec4<f32>(low_dir, 0.0)).xyz;

        let low_sfc_h = textureSample(height_tex, height_sampler, low_world).r;
        let low_density = compute_cloud_density(low_world, low_sfc_h);

        if (low_density > 0.01) {
            // Beer-Lambert opacity
            let low_alpha = 1.0 - exp(-low_density * 4.5);

            // Self-shadowing
            let shadow_pos = normalize(low_world + sun_dir * 0.03);
            let shadow_h = textureSample(height_tex, height_sampler, shadow_pos).r;
            let shadow_density = compute_cloud_density(shadow_pos, shadow_h);
            let shadow = exp(-shadow_density * 2.5);

            // Warm white (lit) → blue-grey (shadow), tinted by star color
            let lit_cloud = vec3<f32>(0.95, 0.95, 0.93) * s_color;
            let shadow_cloud = vec3<f32>(0.55, 0.58, 0.65);
            var low_color = mix(shadow_cloud, lit_cloud, shadow);

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
            let ci_alpha = 1.0 - exp(-cirrus_density * 2.0);

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

    // City light scatter through clouds: soft glow on top of cloud layer
    if (city_glow_amount > 0.05) {
        let scatter_strength = (1.0 - exp(-city_glow_amount * 2.0)) * 0.4; // thicker clouds scatter more
        lit_color += city_glow_through * scatter_strength;
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
        let edge_bg = starfield(ndc, sun_dir, s_color);
        let edge_bg_tm = edge_bg / (edge_bg + vec3<f32>(1.0));
        let edge_aa = 1.0 - smooth_step(0.99, 1.0, sqrt(r2));
        lit_color = mix(edge_bg_tm, lit_color, edge_aa);
    }

    return vec4<f32>(lit_color, 1.0);
}
