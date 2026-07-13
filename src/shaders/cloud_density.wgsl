// Shared weather-driven density functions. Preview and export compile this unchanged.
const CLOUD_DETAIL_STRENGTH: f32 = 1.0;

fn layer_profile(altitude_km: f32, base_km: f32, top_km: f32) -> f32 {
    let thickness = max(top_km - base_km, 0.05);
    return smooth_step(base_km, base_km + thickness * 0.18, altitude_km)
        * (1.0 - smooth_step(top_km - thickness * 0.22, top_km, altitude_km));
}

fn cloud_display_scale() -> f32 {
    if (uniforms.show_clouds < 0.5 || uniforms.cloud_coverage <= 0.001) { return 0.0; }
    return clamp(uniforms.cloud_opacity, 0.0, 1.0);
}

fn weather_density(dir: vec3<f32>, altitude_km: f32) -> f32 {
    let direction = normalize(dir);
    let mass = textureSample(weather_mass_tex, height_sampler, direction);
    if (mass.a <= 0.0) { return 0.0; }
    let geometry = textureSample(weather_geometry_tex, height_sampler, direction);

    let height_fraction = clamp((altitude_km - geometry.r) / max(geometry.b - geometry.r, 0.1), 0.0, 1.0);
    let mesoscale = snoise(direction * 18.0 + noise_seed_offset(uniforms.cloud_seed, 40u)) * 0.5 + 0.5;
    let cellular = snoise(direction * 36.0 + noise_seed_offset(uniforms.cloud_seed, 41u)) * 0.5 + 0.5;
    let fine = snoise(direction * 72.0 + noise_seed_offset(uniforms.cloud_seed, 42u)) * 0.5 + 0.5;
    let erosion_field = mesoscale * 0.62 + cellular * 0.34 + fine * 0.04;
    let low_depth_km = geometry.g - geometry.r;
    let shallow_family = smooth_step(0.7, 1.8, low_depth_km);
    let normalized_occupancy = clamp(
        mass.a / max(uniforms.cloud_coverage, 0.001),
        0.0,
        1.0,
    );
    let causal_fringe = 1.0 - smooth_step(0.16, 0.56, normalized_occupancy);
    let erosion_cutoff = mix(0.42, 0.57, normalized_occupancy);
    let fringe_erosion = causal_fringe * smooth_step(
        erosion_cutoff + 0.14,
        erosion_cutoff - 0.14,
        erosion_field,
    );
    let shallow_holes = causal_fringe * shallow_family
        * smooth_step(0.42, 0.18, cellular);
    let low_core_guard = 1.0 - smooth_step(0.28, 0.56, mass.r);
    let deep_core_guard = 1.0 - smooth_step(0.25, 0.52, mass.g);
    let detailed_low_mass = max(
        mass.r - (fringe_erosion * 0.14 + shallow_holes * 0.08)
            * low_core_guard * CLOUD_DETAIL_STRENGTH,
        0.0,
    );
    let detailed_deep_mass = max(
        mass.g - fringe_erosion * 0.13 * deep_core_guard * CLOUD_DETAIL_STRENGTH,
        0.0,
    );
    let terrain_family = smooth_step(
        0.05,
        0.22,
        textureSample(height_tex, height_sampler, direction).r,
    ) * (1.0 - smooth_step(0.05, 0.35, mass.g));
    let frontal_family = smooth_step(0.03, 0.3, mass.b)
        * (1.0 - smooth_step(0.15, 0.55, mass.g));
    let density_texture = mix(1.0, 0.9 + mesoscale * 0.1, CLOUD_DETAIL_STRENGTH);
    let low_detail = mix(1.0, density_texture, low_core_guard);
    let deep_detail = mix(1.0, density_texture, deep_core_guard);
    let low_family_detail = mix(1.0, low_detail, shallow_family);
    let low_profile = layer_profile(altitude_km, geometry.r, geometry.g);
    let terrain_cap = layer_profile(
        altitude_km,
        mix(geometry.r, geometry.g, 0.15),
        mix(geometry.r, geometry.g, 0.8),
    );
    let frontal_layer = layer_profile(altitude_km, geometry.r, max(geometry.g, geometry.b * 0.65));
    let low = detailed_low_mass * mix(
        mix(low_profile * low_family_detail, terrain_cap * low_detail, terrain_family * 0.45),
        frontal_layer * low_detail,
        frontal_family * 0.35,
    );
    let deep = detailed_deep_mass * 1.2 * layer_profile(altitude_km, geometry.r, geometry.b)
        * mix(1.0, 0.35, height_fraction) * deep_detail;
    return clamp(low + deep, 0.0, 1.0);
}

fn weather_cirrus_density(dir: vec3<f32>, altitude_km: f32) -> f32 {
    let direction = normalize(dir);
    let mass = textureSample(weather_mass_tex, height_sampler, direction);
    if (mass.b <= 0.0 || mass.a <= 0.0) { return 0.0; }
    let geometry = textureSample(weather_geometry_tex, height_sampler, direction);
    let wind_sample = textureSample(cloud_tex, height_sampler, direction).xyz;
    let tangent = normalize(wind_sample - direction * dot(wind_sample, direction) + vec3<f32>(0.0001));
    let seed = noise_seed_offset(uniforms.cloud_seed, 43u);
    let center = snoise(direction * 35.0 + seed);
    let ahead = snoise(normalize(direction + tangent * 0.008) * 35.0 + seed);
    let far_ahead = snoise(normalize(direction + tangent * 0.016) * 35.0 + seed);
    let behind = snoise(normalize(direction - tangent * 0.008) * 35.0 + seed);
    let far_behind = snoise(normalize(direction - tangent * 0.016) * 35.0 + seed);
    let fibres = smooth_step(0.08, 0.48, (center + ahead + far_ahead + behind + far_behind) / 5.0);
    let high_base = max(geometry.b, geometry.a - 3.0);
    return mass.b * layer_profile(altitude_km, high_base, geometry.a) * mix(0.7, 1.0, fibres) * 0.35;
}

fn weather_column_density(dir: vec3<f32>) -> f32 {
    let direction = normalize(dir);
    let mass = textureSample(weather_mass_tex, height_sampler, direction);
    if (mass.a <= 0.0) { return 0.0; }
    let geometry = textureSample(weather_geometry_tex, height_sampler, direction);
    let low_mid = (geometry.r + max(geometry.g, geometry.b)) * 0.5;
    let high_mid = (max(geometry.b, geometry.a - 3.0) + geometry.a) * 0.5;
    return clamp(weather_density(direction, low_mid) + weather_cirrus_density(direction, high_mid), 0.0, 1.0)
        * cloud_display_scale();
}
