struct WeatherSnapshot {
    face: u32,
    resolution: u32,
    seed: u32,
    storm_count: u32,
    coverage: f32,
    moisture: f32,
    surface_pressure_bar: f32,
    base_temp_c: f32,
    ocean_level: f32,
    axial_tilt_rad: f32,
    season: f32,
    storm_size: f32,
    radius_km: f32,
    rotation_rate_rad_s: f32,
    _pad0: f32,
}

@group(0) @binding(0) var<uniform> params: WeatherSnapshot;
@group(0) @binding(1) var wind_tex: texture_cube<f32>;
@group(0) @binding(2) var pressure_tex: texture_cube<f32>;
@group(0) @binding(3) var weather_sampler: sampler;
@group(0) @binding(4) var<storage, read> height_data: array<f32>;
@group(0) @binding(5) var mass_tex: texture_storage_2d_array<rgba16float, write>;
@group(0) @binding(6) var geometry_tex: texture_storage_2d_array<rgba16float, write>;
@group(0) @binding(7) var spinup_state: texture_cube<f32>;

fn smooth_step(edge0: f32, edge1: f32, value: f32) -> f32 {
    let t = clamp((value - edge0) / (edge1 - edge0), 0.0, 1.0);
    return t * t * (3.0 - 2.0 * t);
}

fn soft_bound(value: f32, bound: f32) -> f32 {
    let nonnegative = max(value, 0.0);
    return bound * nonnegative / (bound + nonnegative);
}

fn sphere_to_face_uv(dir: vec3<f32>) -> vec3<f32> {
    let a = abs(dir);
    if (a.x >= a.y && a.x >= a.z) {
        return select(vec3<f32>(1.0, dir.z / a.x * 0.5 + 0.5, -dir.y / a.x * 0.5 + 0.5), vec3<f32>(0.0, -dir.z / a.x * 0.5 + 0.5, -dir.y / a.x * 0.5 + 0.5), dir.x > 0.0);
    }
    if (a.y >= a.x && a.y >= a.z) {
        return select(vec3<f32>(3.0, dir.x / a.y * 0.5 + 0.5, -dir.z / a.y * 0.5 + 0.5), vec3<f32>(2.0, dir.x / a.y * 0.5 + 0.5, dir.z / a.y * 0.5 + 0.5), dir.y > 0.0);
    }
    return select(vec3<f32>(5.0, -dir.x / a.z * 0.5 + 0.5, -dir.y / a.z * 0.5 + 0.5), vec3<f32>(4.0, dir.x / a.z * 0.5 + 0.5, -dir.y / a.z * 0.5 + 0.5), dir.z > 0.0);
}

fn sample_height(dir: vec3<f32>) -> f32 {
    let fuv = sphere_to_face_uv(dir);
    let res = params.resolution;
    let x = min(u32(fuv.y * f32(res - 1u)), res - 1u);
    let y = min(u32(fuv.z * f32(res - 1u)), res - 1u);
    return height_data[u32(fuv.x) * res * res + y * res + x];
}

fn tangent_basis(pos: vec3<f32>) -> mat2x3<f32> {
    let reference = select(vec3<f32>(0.0, 1.0, 0.0), vec3<f32>(1.0, 0.0, 0.0), abs(pos.y) > 0.9);
    let east = normalize(cross(reference, pos));
    return mat2x3<f32>(east, normalize(cross(pos, east)));
}

fn physical_latitude(pos: vec3<f32>) -> f32 {
    let tilted_y = pos.y * cos(params.axial_tilt_rad) + pos.z * sin(params.axial_tilt_rad);
    return abs(asin(clamp(tilted_y, -1.0, 1.0))) / 1.5707963;
}

fn temperature_at(pos: vec3<f32>) -> f32 {
    let tilted_y = pos.y * cos(params.axial_tilt_rad) + pos.z * sin(params.axial_tilt_rad);
    let latitude = physical_latitude(pos);
    let season_shift = (params.season - 0.5) * 2.0 * sin(params.axial_tilt_rad);
    let elevation_km = max(sample_height(pos) - params.ocean_level, 0.0) * 5.0;
    let continentality = textureSampleLevel(wind_tex, weather_sampler, pos, 0.0).a;
    return params.base_temp_c - latitude * 35.0 + season_shift * tilted_y * 16.0
        - elevation_km * 6.5 + continentality * season_shift * 5.0;
}

@compute @workgroup_size(16, 16)
fn main(@builtin(global_invocation_id) id: vec3<u32>) {
    let res = params.resolution;
    if (id.x >= res || id.y >= res) { return; }
    let uv = vec2<f32>(f32(id.x) / f32(res - 1u), f32(id.y) / f32(res - 1u));
    let pos = cube_to_sphere(params.face, uv);
    let wind = textureSampleLevel(wind_tex, weather_sampler, pos, 0.0);
    let pressure = textureSampleLevel(pressure_tex, weather_sampler, pos, 0.0).r;
    let height = sample_height(pos);
    let wind_tangent = wind.xyz - pos * dot(wind.xyz, pos);
    let wind_speed = length(wind_tangent);
    let wind_dir = wind_tangent / max(wind_speed, 0.0001);
    let basis = tangent_basis(pos);
    let east = basis[0];
    let north = basis[1];
    let diagnostic_step = clamp(300.0 / max(params.radius_km, 1.0), 0.02, 0.08);
    let east_pos = normalize(pos + east * diagnostic_step);
    let west_pos = normalize(pos - east * diagnostic_step);
    let north_pos = normalize(pos + north * diagnostic_step);
    let south_pos = normalize(pos - north * diagnostic_step);
    let east_wind = textureSampleLevel(wind_tex, weather_sampler, east_pos, 0.0).xyz;
    let west_wind = textureSampleLevel(wind_tex, weather_sampler, west_pos, 0.0).xyz;
    let north_wind = textureSampleLevel(wind_tex, weather_sampler, north_pos, 0.0).xyz;
    let south_wind = textureSampleLevel(wind_tex, weather_sampler, south_pos, 0.0).xyz;
    let divergence = (dot(east_wind - west_wind, east) + dot(north_wind - south_wind, north))
        / (2.0 * diagnostic_step);
    let convergence = clamp(-divergence * 0.2, -1.0, 1.0);

    let terrain_step = diagnostic_step * 1.5;
    let upwind_height = (
        sample_height(normalize(pos - wind_dir * terrain_step))
            + sample_height(normalize(pos - wind_dir * terrain_step * 2.0))
    ) * 0.5;
    let downwind_height = (
        sample_height(normalize(pos + wind_dir * terrain_step))
            + sample_height(normalize(pos + wind_dir * terrain_step * 2.0))
    ) * 0.5;
    let terrain_gradient = downwind_height - upwind_height;
    let terrain_wind = smooth_step(0.03, 0.2, wind_speed);
    let terrain_lift = smooth_step(0.005, 0.08, terrain_gradient) * terrain_wind;
    let rain_shadow = smooth_step(0.005, 0.08, -terrain_gradient) * terrain_wind;
    let latitude = physical_latitude(pos);
    let temperature = temperature_at(pos);
    let thermal = smooth_step(-25.0, 30.0, temperature);
    let continentality = wind.a;
    let pressure_factor = smooth_step(0.05, 0.3, params.surface_pressure_bar);
    let surface_supply = mix(1.0, 0.55, smooth_step(0.15, 0.85, continentality));
    let moisture = clamp(params.moisture, 0.0, 1.0) * pressure_factor * surface_supply;

    let pressure_east = textureSampleLevel(pressure_tex, weather_sampler, east_pos, 0.0).r;
    let pressure_west = textureSampleLevel(pressure_tex, weather_sampler, west_pos, 0.0).r;
    let pressure_north = textureSampleLevel(pressure_tex, weather_sampler, north_pos, 0.0).r;
    let pressure_south = textureSampleLevel(pressure_tex, weather_sampler, south_pos, 0.0).r;
    let pressure_delta = vec2<f32>(pressure_east - pressure_west, pressure_north - pressure_south);
    let temperature_delta = vec2<f32>(
        temperature_at(east_pos) - temperature_at(west_pos),
        temperature_at(north_pos) - temperature_at(south_pos),
    );
    let pressure_gradient = length(pressure_delta);
    let zonal_structure = smooth_step(0.5, 3.0, abs(pressure_delta.x));
    let temperature_gradient = length(temperature_delta);
    let frontal_alignment = dot(pressure_delta, temperature_delta)
        / max(pressure_gradient * temperature_gradient, 0.0001);
    let frontal_side = smooth_step(-0.45, 0.45, -frontal_alignment);
    let rotation_ratio = clamp(abs(params.rotation_rate_rad_s) / 0.00007292116, 0.1, 4.0);
    let coriolis = smooth_step(0.02, 0.35, latitude * rotation_ratio);
    let frontal = smooth_step(2.0, 12.0, pressure_gradient)
        * smooth_step(1.0, 12.0, temperature_gradient)
        * mix(0.15, 1.0, frontal_side)
        * mix(0.35, 1.0, coriolis);
    let convergent_lift = smooth_step(0.02, 0.35, convergence);
    let divergent_drying = smooth_step(0.02, 0.35, -convergence);
    let inversion = clamp(
        (1.0 - thermal) * 0.45
            + smooth_step(1010.0, 1030.0, pressure) * 0.25
            + continentality * 0.2
            - terrain_lift * 0.25,
        0.0,
        1.0,
    );
    let instability = clamp(thermal * (1.0 - inversion) + terrain_lift * 0.2, 0.0, 1.0);
    let frontal_eligibility = frontal * mix(0.35, 1.0, convergent_lift);
    let orographic_eligibility = terrain_lift * (1.0 - rain_shadow);
    let organized_lift = max(convergent_lift, max(frontal_eligibility, orographic_eligibility));
    let condensation = moisture * clamp(
        0.55 + inversion * 0.45 - divergent_drying * 0.4 - rain_shadow * 0.35,
        0.0,
        1.0,
    );
    let deck_organization = max(zonal_structure, organized_lift);
    let deck_eligibility = condensation * inversion
        * smooth_step(0.04, 0.5, deck_organization);
    let shallow_eligibility = condensation * (1.0 - inversion)
        * smooth_step(0.1, 0.7, instability)
        * smooth_step(0.02, 0.45, organized_lift);
    let low_eligibility = clamp(
        deck_eligibility * 0.65 + shallow_eligibility * 0.45
            + moisture * (frontal_eligibility * 0.3 + orographic_eligibility * 0.45),
        0.0,
        1.0,
    );
    let deep_trigger = clamp(convergent_lift * 0.75 + orographic_eligibility * 0.45
        + frontal_eligibility * 0.35, 0.0, 1.0);
    let storm_potential = moisture * instability * deep_trigger;
    let storm_radius = clamp(params.storm_size, 0.3, 3.0) * 0.16;
    var storm_locality = 0.0;
    for (var storm = 0u; storm < min(params.storm_count, 8u); storm++) {
        let center = normalize(noise_seed_offset(params.seed, 100u + storm) - vec3<f32>(50.0));
        storm_locality = max(
            storm_locality,
            smooth_step(cos(storm_radius), cos(storm_radius * 0.45), dot(pos, center)),
        );
    }
    let deep_eligibility = clamp(
        storm_potential * (1.0 + storm_locality * 3.0),
        0.0,
        1.0,
    );
    let high_eligibility = moisture
        * clamp(frontal_eligibility * 0.65 + deep_eligibility * 0.7, 0.0, 1.0);
    let physical_eligibility = max(max(low_eligibility, deep_eligibility), high_eligibility);
    let boundary = snoise(pos + noise_seed_offset(params.seed, 31u)) * 0.18
        + snoise(pos * 4.0 + noise_seed_offset(params.seed, 32u)) * 0.04;
    let coverage = clamp(params.coverage, 0.0, 1.0);
    let threshold = mix(0.7, 0.08, coverage) + divergent_drying * 0.14;
    let coverage_shape = smooth_step(
        threshold - 0.18,
        threshold + 0.18,
        physical_eligibility + boundary,
    );
    let occupancy = coverage * smooth_step(0.01, 0.2, physical_eligibility)
        * coverage_shape;
    let low_deep_total = max(low_eligibility + deep_eligibility, 0.000001);
    let low_mass = occupancy * low_eligibility / low_deep_total;
    let deep_mass = occupancy * deep_eligibility / low_deep_total;
    let high_mass = occupancy * clamp(high_eligibility, 0.0, 1.0);

    let base_altitude_km = clamp(
        mix(0.45, 1.8, 1.0 - moisture) + latitude * 0.45 + (1.0 - pressure_factor) * 0.8
            - terrain_lift * 0.25,
        0.2,
        3.0,
    );
    let low_family_total = max(
        deck_eligibility + shallow_eligibility + frontal_eligibility + orographic_eligibility,
        0.0001,
    );
    let low_depth_km = (
        deck_eligibility * 0.45 + shallow_eligibility * 1.8
            + frontal_eligibility * 1.2 + orographic_eligibility * 0.8
    ) / low_family_total;
    let low_top_km = base_altitude_km + clamp(low_depth_km, 0.3, 2.5);
    let deep_top_km = max(low_top_km, base_altitude_km + mix(1.0, 12.0, deep_eligibility * thermal));
    let tropopause_km = mix(8.0, 16.0, thermal) * mix(0.9, 1.05, 1.0 / sqrt(rotation_ratio));
    let high_top_km = max(deep_top_km, clamp(tropopause_km, 7.0, 18.0));
    textureStore(mass_tex, vec2<i32>(id.xy), i32(params.face), vec4<f32>(low_mass, deep_mass, high_mass, occupancy));
    textureStore(geometry_tex, vec2<i32>(id.xy), i32(params.face), vec4<f32>(base_altitude_km, low_top_km, deep_top_km, high_top_km));
}

@compute @workgroup_size(16, 16)
fn diagnose(@builtin(global_invocation_id) id: vec3<u32>) {
    let res = params.resolution;
    if (id.x >= res || id.y >= res) { return; }
    let uv = vec2<f32>(f32(id.x) / f32(res - 1u), f32(id.y) / f32(res - 1u));
    let pos = cube_to_sphere(params.face, uv);
    if (params.coverage <= 0.0 || params.moisture <= 0.0) {
        textureStore(mass_tex, vec2<i32>(id.xy), i32(params.face), vec4<f32>(0.0));
        textureStore(geometry_tex, vec2<i32>(id.xy), i32(params.face), vec4<f32>(0.0));
        return;
    }

    let wind = textureSampleLevel(wind_tex, weather_sampler, pos, 0.0);
    let continentality = clamp(wind.a, 0.0, 1.0);
    let marine_fraction = 1.0 - smooth_step(0.15, 0.85, continentality);
    let thermal = smooth_step(-25.0, 30.0, temperature_at(pos));
    let marine_stability = marine_fraction * smooth_step(0.05, 0.8, 1.0 - thermal);
    let trade_cumulus = marine_fraction * thermal * (1.0 - marine_stability);
    let state = textureSampleLevel(spinup_state, weather_sampler, pos, 0.0);

    let wind_tangent = wind.xyz - pos * dot(wind.xyz, pos);
    let wind_speed = length(wind_tangent);
    let wind_dir = wind_tangent / max(wind_speed, 0.0001);
    let terrain_step = clamp(300.0 / max(params.radius_km, 1.0), 0.02, 0.08);
    let terrain_gradient = sample_height(normalize(pos + wind_dir * terrain_step * 1.5))
        - sample_height(normalize(pos - wind_dir * terrain_step * 1.5));
    let terrain_lift = smooth_step(0.005, 0.08, terrain_gradient)
        * smooth_step(0.03, 0.2, wind_speed);
    let rain_shadow = smooth_step(0.005, 0.08, -terrain_gradient)
        * smooth_step(0.03, 0.2, wind_speed);

    // Erosion shapes existing warm trade mass; it never creates an occupancy threshold.
    let trade_erosion = 0.05 + 0.95 * smooth_step(
        -0.5,
        0.5,
        snoise(pos * 5.0 + noise_seed_offset(params.seed, 61u)),
    );
    let detail_erosion = 0.55 + 0.45 * smooth_step(
        -0.5,
        0.5,
        snoise(pos * 3.0 + noise_seed_offset(params.seed, 62u)),
    );
    let seed_erosion = 0.45 + 0.55 * smooth_step(
        -0.5,
        0.5,
        snoise(pos + noise_seed_offset(params.seed, 31u)),
    );
    let marine_low_factor = 0.72 + marine_stability * 1.45 + trade_cumulus * 6.0 + terrain_lift * 0.65;
    // The transported vapor state supplies weak land cloud mass even in calm test
    // fixtures; this is continuous evapotranspiration, not an occupancy threshold.
    let cool_land_source = smooth_step(0.35, 0.7, 1.0 - thermal);
    let inland_low_source = (1.0 - marine_fraction) * state.x * 0.3 * cool_land_source;
    let marine_deck_source = marine_stability * state.x * 0.25;
    let warm_trade_source = smooth_step(0.75, 0.9, thermal);
    let trade_low_source = trade_cumulus * state.x * trade_erosion * 0.6 * warm_trade_source;
    let low = soft_bound(
        state.y * detail_erosion * seed_erosion
            * mix(1.8 + terrain_lift * 3.0 - rain_shadow * 1.2, marine_low_factor, marine_fraction)
            * mix(1.0, trade_erosion, trade_cumulus)
            + inland_low_source
            + marine_deck_source
            + trade_low_source
            + terrain_lift * clamp(params.moisture, 0.0, 1.0) * clamp(params.coverage, 0.0, 1.0) * 1.35,
        1.0,
    );
    let deep = soft_bound(
        state.z * (1.0 - marine_stability * 0.78) * (1.0 + terrain_lift * 0.35)
            + marine_stability * state.x * 0.065,
        1.0 - low,
    );
    let occupancy = clamp(low + deep, 0.0, 1.0);
    let high = clamp(state.w * (1.0 - marine_stability * 0.4), 0.0, occupancy);

    let deck_base_km = 0.35 + (1.0 - marine_stability) * 0.2;
    let deck_thickness_km = 0.3 + 0.9 * (1.0 - thermal) * marine_fraction;
    let trade_top_km = mix(1.0, 3.0, thermal) * marine_fraction;
    let base_altitude_km = mix(0.75, deck_base_km, marine_stability);
    let low_top_km = mix(
        base_altitude_km + 0.8 + terrain_lift * 0.3,
        mix(base_altitude_km + deck_thickness_km, trade_top_km, trade_cumulus),
        marine_fraction,
    );
    let deep_top_km = max(low_top_km, low_top_km + 2.0 + state.z * 8.0);
    let high_top_km = max(deep_top_km, 8.0 + thermal * 8.0);
    textureStore(mass_tex, vec2<i32>(id.xy), i32(params.face), vec4<f32>(low, deep, high, occupancy));
    textureStore(geometry_tex, vec2<i32>(id.xy), i32(params.face), vec4<f32>(base_altitude_km, low_top_km, deep_top_km, high_top_km));
}
