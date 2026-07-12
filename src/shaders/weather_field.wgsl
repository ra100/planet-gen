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

fn smooth_step(edge0: f32, edge1: f32, value: f32) -> f32 {
    let t = clamp((value - edge0) / (edge1 - edge0), 0.0, 1.0);
    return t * t * (3.0 - 2.0 * t);
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

fn temperature_at(pos: vec3<f32>) -> f32 {
    let tilted_y = pos.y * cos(params.axial_tilt_rad) + pos.z * sin(params.axial_tilt_rad);
    let latitude = abs(asin(clamp(tilted_y, -1.0, 1.0))) / 1.5707963;
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
    let weather_seed = vec3<f32>(f32(params.seed), fract(f32(params.seed) * 0.001618) * 89.0, 0.0);
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
    let tilted_y = pos.y * cos(params.axial_tilt_rad) + pos.z * sin(params.axial_tilt_rad);
    let latitude = abs(asin(clamp(tilted_y, -1.0, 1.0))) / 1.5707963;
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
    let storm_strength = clamp(f32(params.storm_count) / 8.0, 0.0, 1.0);
    let storm_control = storm_strength * smooth_step(
        mix(0.95, 0.35, storm_strength),
        1.0,
        snoise(pos * (2.1 / max(params.storm_size, 0.3)) + weather_seed * 0.07) * 0.5 + 0.5,
    );

    let deck_eligibility = inversion * (1.0 - instability * 0.6);
    let shallow_eligibility = (1.0 - inversion) * smooth_step(0.15, 0.7, instability);
    let frontal_eligibility = frontal * mix(0.45, 1.0, convergent_lift);
    let orographic_eligibility = terrain_lift * (1.0 - rain_shadow);
    let low_eligibility = moisture * clamp(
        0.12 + deck_eligibility * 0.5 + shallow_eligibility * 0.35
            + convergent_lift * 0.2 + frontal_eligibility * 0.3
            + orographic_eligibility * 0.45 - rain_shadow * 0.55
            - divergent_drying * 0.45,
        0.0,
        1.0,
    );
    let deep_eligibility = moisture * instability
        * clamp(convergent_lift * 0.75 + orographic_eligibility * 0.45
            + frontal_eligibility * 0.35
            + storm_control * convergent_lift * 0.3 - divergent_drying * 0.5, 0.0, 1.0);
    let high_eligibility = moisture
        * clamp(frontal_eligibility * 0.65 + deep_eligibility * 0.7, 0.0, 1.0);
    let physical_eligibility = max(max(low_eligibility, deep_eligibility), high_eligibility);
    let boundary = snoise(pos * 3.0 + weather_seed * 0.11) * 0.10
        + snoise(pos * 7.0 + weather_seed * 0.19) * 0.04;
    let coverage = clamp(params.coverage, 0.0, 1.0);
    let threshold = mix(0.72, 0.08, coverage);
    let coverage_shape = smooth_step(
        threshold - 0.12,
        threshold + 0.12,
        physical_eligibility + boundary,
    );
    let occupancy = coverage * smooth_step(0.0, 0.08, physical_eligibility)
        * mix(0.05, 1.0, coverage_shape);
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
