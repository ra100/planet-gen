// Shared weather-driven density functions. Preview and export compile this unchanged.
const CLOUD_DETAIL_STRENGTH: f32 = 1.0;
const LOW_OPTICAL_WEIGHT: f32 = 0.50;

fn layer_profile(altitude_km: f32, base_km: f32, top_km: f32) -> f32 {
    let thickness = max(top_km - base_km, 0.05);
    // The paired ramps integrate to approximately 0.8 * thickness. Normalize so
    // mass remains a column quantity instead of scaling with layer depth.
    let profile = smooth_step(base_km, base_km + thickness * 0.18, altitude_km)
        * (1.0 - smooth_step(top_km - thickness * 0.22, top_km, altitude_km));
    return profile / max(0.8 * thickness, 0.05);
}

fn cloud_display_scale() -> f32 {
    if (uniforms.show_clouds < 0.5 || uniforms.cloud_coverage <= 0.001) { return 0.0; }
    return clamp(uniforms.cloud_opacity, 0.0, 1.0);
}

fn weather_detail_direction(direction: vec3<f32>, trail: f32) -> vec3<f32> {
    if (uniforms.cloud_advection <= 0.5) { return direction; }
    let wind = textureSample(cloud_tex, height_sampler, direction).xyz;
    let tangent = wind - direction * dot(wind, direction);
    if (length(tangent) <= 0.003) { return direction; }
    // Wind changes only procedural coordinates; weather mass stays in place.
    return normalize(direction - normalize(tangent) * trail);
}

fn filtered_noise(
    direction: vec3<f32>,
    frequencies: vec3<f32>,
    weights: vec3<f32>,
    angular_pixel_footprint: f32,
    seed: u32,
) -> f32 {
    let footprint_frequency = frequencies * angular_pixel_footprint;
    let band_limit = vec3<f32>(
        1.0 - smooth_step(0.12, 0.50, footprint_frequency.x),
        1.0 - smooth_step(0.12, 0.50, footprint_frequency.y),
        1.0 - smooth_step(0.12, 0.50, footprint_frequency.z),
    );
    let filtered_weights = weights * band_limit;
    let weight_sum = max(dot(filtered_weights, vec3<f32>(1.0)), 0.0001);
    let samples = vec3<f32>(
        snoise(direction * frequencies.x + noise_seed_offset(uniforms.cloud_seed, seed)),
        snoise(direction * frequencies.y + noise_seed_offset(uniforms.cloud_seed, seed + 1u)),
        snoise(direction * frequencies.z + noise_seed_offset(uniforms.cloud_seed, seed + 2u)),
    );
    return dot(samples, filtered_weights) / weight_sum;
}

fn weather_density(dir: vec3<f32>, altitude_km: f32, angular_pixel_footprint: f32) -> f32 {
    let direction = normalize(dir);
    let mass = textureSample(weather_mass_tex, height_sampler, direction);
    if (mass.a <= 0.0) { return 0.0; }
    let geometry = textureSample(weather_geometry_tex, height_sampler, direction);

    let deep_height_fraction = clamp((altitude_km - geometry.r) / max(geometry.b - geometry.r, 0.1), 0.0, 1.0);
    let low_height_fraction = clamp((altitude_km - geometry.r) / max(geometry.g - geometry.r, 0.1), 0.0, 1.0);
    let low_detail = filtered_noise(
        weather_detail_direction(direction, 0.012), vec3<f32>(6.0, 12.0, 24.0),
        vec3<f32>(0.50, 0.32, 0.18), angular_pixel_footprint, 40u,
    );
    let deep_detail = filtered_noise(
        weather_detail_direction(direction, 0.018), vec3<f32>(7.0, 14.0, 24.0),
        vec3<f32>(0.46, 0.34, 0.20), angular_pixel_footprint, 50u,
    );
    let low_depth_km = geometry.g - geometry.r;
    let shallow_family = smooth_step(0.7, 1.8, low_depth_km);
    let normalized_occupancy = clamp(
        mass.a / max(uniforms.cloud_coverage, 0.001),
        0.0,
        1.0,
    );
    let causal_fringe = 1.0 - smooth_step(0.12, 0.52, normalized_occupancy);
    let low_vertical_fringe = 1.0 - smooth_step(0.04, 0.24, min(low_height_fraction, 1.0 - low_height_fraction));
    let deep_vertical_fringe = 1.0 - smooth_step(0.04, 0.24, min(deep_height_fraction, 1.0 - deep_height_fraction));
    // Detail only attenuates continuous causal/vertical fringes; it never decides occupancy.
    let erosion_field = smooth_step(-0.7, 0.8, low_detail);
    let low_erosion = 1.0 - CLOUD_DETAIL_STRENGTH * causal_fringe * low_vertical_fringe
        * (0.06 + 0.10 * erosion_field);
    let deep_erosion = 1.0 - CLOUD_DETAIL_STRENGTH * causal_fringe * deep_vertical_fringe
        * (0.05 + 0.09 * erosion_field);
    let low_multiplier = mix(1.0, 0.76 + smooth_step(-0.75, 0.75, low_detail) * 0.48, CLOUD_DETAIL_STRENGTH);
    let deep_multiplier = mix(1.0, 0.74 + smooth_step(-0.75, 0.75, deep_detail) * 0.52, CLOUD_DETAIL_STRENGTH);
    let tower_lobes = filtered_noise(
        weather_detail_direction(direction, 0.022), vec3<f32>(7.0, 14.0, 24.0),
        vec3<f32>(0.52, 0.30, 0.18), angular_pixel_footprint, 60u,
    );
    let tower_taper = mix(1.0, 0.58 + smooth_step(-0.70, 0.70, tower_lobes) * 0.34, deep_height_fraction);
    let detailed_low_mass = mass.r * low_erosion * low_multiplier;
    let detailed_deep_mass = mass.g * deep_erosion * deep_multiplier * tower_taper;
    let terrain_family = smooth_step(
        0.05,
        0.22,
        textureSample(height_tex, height_sampler, direction).r,
    ) * (1.0 - smooth_step(0.05, 0.35, mass.g));
    let frontal_family = smooth_step(0.03, 0.3, mass.b)
        * (1.0 - smooth_step(0.15, 0.55, mass.g));
    let detached_base = mix(geometry.r, mix(geometry.r, geometry.g, 0.16), shallow_family);
    let low_profile = layer_profile(altitude_km, detached_base, geometry.g);
    let terrain_cap = layer_profile(
        altitude_km,
        mix(geometry.r, geometry.g, 0.15),
        mix(geometry.r, geometry.g, 0.8),
    );
    let frontal_layer = layer_profile(altitude_km, geometry.r, max(geometry.g, geometry.b * 0.65));
    let low = detailed_low_mass * LOW_OPTICAL_WEIGHT * mix(
        mix(low_profile, terrain_cap, terrain_family * 0.45),
        frontal_layer,
        frontal_family * 0.35,
    );
    let deep = detailed_deep_mass * 1.2 * layer_profile(altitude_km, geometry.r, geometry.b)
        * mix(1.08, 0.72, deep_height_fraction);
    return max(low + deep, 0.0);
}

fn weather_cirrus_density(dir: vec3<f32>, altitude_km: f32, angular_pixel_footprint: f32) -> f32 {
    let direction = normalize(dir);
    let mass = textureSample(weather_mass_tex, height_sampler, direction);
    if (mass.b <= 0.0 || mass.a <= 0.0) { return 0.0; }
    let geometry = textureSample(weather_geometry_tex, height_sampler, direction);
    let fibres = filtered_noise(
        weather_detail_direction(direction, 0.030), vec3<f32>(9.0, 18.0, 24.0),
        vec3<f32>(0.50, 0.32, 0.18), angular_pixel_footprint, 70u,
    );
    let high_base = max(geometry.b, geometry.a - 3.0);
    let high_fraction = clamp((altitude_km - high_base) / max(geometry.a - high_base, 0.1), 0.0, 1.0);
    let high_modulation = mix(1.0, 0.82 + smooth_step(-0.72, 0.72, fibres) * 0.38 + (high_fraction - 0.5) * 0.08, CLOUD_DETAIL_STRENGTH);
    return mass.b * layer_profile(altitude_km, high_base, geometry.a) * high_modulation * 0.35;
}

fn weather_column_density(dir: vec3<f32>, angular_pixel_footprint: f32) -> f32 {
    let direction = normalize(dir);
    let mass = textureSample(weather_mass_tex, height_sampler, direction);
    if (mass.a <= 0.0) { return 0.0; }
    let geometry = textureSample(weather_geometry_tex, height_sampler, direction);
    let low_mid = (geometry.r + max(geometry.g, geometry.b)) * 0.5;
    let high_mid = (max(geometry.b, geometry.a - 3.0) + geometry.a) * 0.5;
    return clamp(weather_density(direction, low_mid, angular_pixel_footprint) + weather_cirrus_density(direction, high_mid, angular_pixel_footprint), 0.0, 1.0)
        * cloud_display_scale();
}
