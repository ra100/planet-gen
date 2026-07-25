// Shared weather-driven density functions. Preview and export compile this unchanged.
const LOW_DETAIL_STRENGTH: f32 = 1.0;
const DEEP_DETAIL_STRENGTH: f32 = 1.0;
const HIGH_DETAIL_STRENGTH: f32 = 1.0;
const LOW_OPTICAL_WEIGHT: f32 = 0.50;

struct CloudLayers {
    low: f32,
    deep: f32,
    high: f32,
}

fn layer_profile(altitude_km: f32, base_km: f32, top_km: f32) -> f32 {
    let thickness = max(top_km - base_km, 0.05);
    // The paired ramps integrate to approximately 0.8 * thickness. Normalize so
    // mass remains a column quantity instead of scaling with layer depth.
    let profile = smooth_step(base_km, base_km + thickness * 0.18, altitude_km)
        * (1.0 - smooth_step(top_km - thickness * 0.22, top_km, altitude_km));
    return profile / max(0.8 * thickness, 0.05);
}

fn layer_profile_cdf(altitude_km: f32, base_km: f32, top_km: f32) -> f32 {
    let h = max(top_km - base_km, 0.05);
    let n = max(0.8 * h, 0.05);
    let scale = h / n;
    let t = (altitude_km - base_km) / h;
    if (t <= 0.0) { return 0.0; }
    if (t < 0.18) {
        return scale * (t * t * t / (0.18 * 0.18) - t * t * t * t / (2.0 * 0.18 * 0.18 * 0.18));
    }
    if (t < 0.78) { return scale * (0.09 + t - 0.18); }
    if (t >= 1.0) { return scale * 0.8; }
    let u = (t - 0.78) / 0.22;
    return scale * (0.69 + 0.22 * (u - u * u * u + 0.5 * u * u * u * u));
}

fn layer_profile_monotonic_segment_mean(
    p0: vec3<f32>,
    p1: vec3<f32>,
    base_km: f32,
    top_km: f32,
    radius_km: f32,
) -> f32 {
    let spatial_length = length(p1 - p0);
    if (spatial_length <= 1.0e-6) { return 0.0; }
    let altitude_start_km = max((length(p0) - 1.0) * radius_km, 0.0);
    let altitude_end_km = max((length(p1) - 1.0) * radius_km, 0.0);
    let altitude_span = abs(altitude_end_km - altitude_start_km);
    if (altitude_span <= 1.0e-6) {
        let midpoint_altitude_km = max((length((p0 + p1) * 0.5) - 1.0) * radius_km, 0.0);
        return layer_profile(midpoint_altitude_km, base_km, top_km);
    }
    return abs(
        layer_profile_cdf(altitude_end_km, base_km, top_km)
            - layer_profile_cdf(altitude_start_km, base_km, top_km)
    ) / altitude_span;
}

fn layer_profile_segment_mean(
    p0: vec3<f32>,
    p1: vec3<f32>,
    base_km: f32,
    top_km: f32,
    radius_km: f32,
) -> f32 {
    let v = p1 - p0;
    let length_squared = dot(v, v);
    if (length_squared <= 1.0e-12) { return 0.0; }
    let t_min = clamp(-dot(p0, v) / length_squared, 0.0, 1.0);
    let closest = p0 + v * t_min;
    let closest_altitude_km = max((length(closest) - 1.0) * radius_km, 0.0);
    let start_altitude_km = max((length(p0) - 1.0) * radius_km, 0.0);
    let end_altitude_km = max((length(p1) - 1.0) * radius_km, 0.0);
    if (t_min > 0.0 && t_min < 1.0
        && closest_altitude_km < start_altitude_km && closest_altitude_km < end_altitude_km) {
        let first_length = length(closest - p0);
        let second_length = length(p1 - closest);
        let total_length = first_length + second_length;
        return (
            layer_profile_monotonic_segment_mean(p0, closest, base_km, top_km, radius_km) * first_length
                + layer_profile_monotonic_segment_mean(closest, p1, base_km, top_km, radius_km) * second_length
        ) / total_length;
    }
    return layer_profile_monotonic_segment_mean(p0, p1, base_km, top_km, radius_km);
}

fn cloud_display_scale() -> f32 {
    if (uniforms.show_clouds < 0.5 || uniforms.cloud_coverage <= 0.001) { return 0.0; }
    return clamp(uniforms.cloud_opacity, 0.0, 1.0);
}

fn wind_filtered_dominant_noise(
    direction: vec3<f32>,
    frequency: f32,
    stretch: f32,
    seed: u32,
) -> f32 {
    let offset = noise_seed_offset(uniforms.cloud_seed, seed);
    let center = snoise(direction * frequency + offset);
    let amount = clamp(stretch - 1.0, 0.0, 1.0);
    if (amount <= 0.0) { return center; }
    let wind = sample_wind_tangent_data(direction);
    let tangent = wind.direction - direction * dot(wind.direction, direction);
    let tangent_length = length(tangent);
    if (tangent_length <= 1.0e-6) { return center; }

    // A finite symmetric geodesic filter smooths the first two octaves along wind.
    let step = clamp(0.60 / frequency, 1.0e-4, 0.10);
    let along = tangent / tangent_length;
    let forward = normalize(direction * cos(step) + along * sin(step));
    let backward = normalize(direction * cos(step) - along * sin(step));
    let far_forward = normalize(direction * cos(2.0 * step) + along * sin(2.0 * step));
    let far_backward = normalize(direction * cos(2.0 * step) - along * sin(2.0 * step));
    return mix(
        center,
        0.05 * center
            + 0.20 * snoise(forward * frequency + offset)
            + 0.20 * snoise(backward * frequency + offset)
            + 0.275 * snoise(far_forward * frequency + offset)
            + 0.275 * snoise(far_backward * frequency + offset),
        amount,
    );
}

fn filtered_noise(
    direction: vec3<f32>,
    frequencies: vec3<f32>,
    weights: vec3<f32>,
    stretch: f32,
    angular_pixel_footprint: f32,
    seed: u32,
) -> vec2<f32> {
    let footprint_frequency = frequencies * angular_pixel_footprint;
    let band_limit = vec3<f32>(
        1.0 - smooth_step(0.12, 0.50, footprint_frequency.x),
        1.0 - smooth_step(0.12, 0.50, footprint_frequency.y),
        1.0 - smooth_step(0.12, 0.50, footprint_frequency.z),
    );
    let dominant_weight = weights.x * band_limit.x;
    let higher_weights = weights.yz * band_limit.yz;
    let dominant = wind_filtered_dominant_noise(direction, frequencies.x, stretch, seed);
    let higher = vec2<f32>(
        snoise(direction * frequencies.y + noise_seed_offset(uniforms.cloud_seed, seed + 1u)),
        snoise(direction * frequencies.z + noise_seed_offset(uniforms.cloud_seed, seed + 2u)),
    );
    return vec2<f32>(
        dominant * dominant_weight / max(dominant_weight, 1.0e-4),
        dot(higher, higher_weights) / max(dot(higher_weights, vec2<f32>(1.0)), 1.0e-4),
    );
}

fn isotropic_noise(
    direction: vec3<f32>,
    frequencies: vec3<f32>,
    weights: vec3<f32>,
    angular_pixel_footprint: f32,
    seed: u32,
) -> vec2<f32> {
    let footprint_frequency = frequencies * angular_pixel_footprint;
    let band_limit = vec3<f32>(
        1.0 - smooth_step(0.12, 0.50, footprint_frequency.x),
        1.0 - smooth_step(0.12, 0.50, footprint_frequency.y),
        1.0 - smooth_step(0.12, 0.50, footprint_frequency.z),
    );
    let weighted = weights * band_limit;
    let noise = vec3<f32>(
        snoise(direction * frequencies.x + noise_seed_offset(uniforms.cloud_seed, seed)),
        snoise(direction * frequencies.y + noise_seed_offset(uniforms.cloud_seed, seed + 1u)),
        snoise(direction * frequencies.z + noise_seed_offset(uniforms.cloud_seed, seed + 2u)),
    );
    return vec2<f32>(
        noise.x,
        dot(noise.yz, weighted.yz) / max(dot(weighted.yz, vec2<f32>(1.0)), 1.0e-4),
    );
}

fn weather_cloud_layers(dir: vec3<f32>, altitude_km: f32, angular_pixel_footprint: f32) -> CloudLayers {
    let direction = normalize(dir);
    let mass = textureSample(weather_mass_tex, height_sampler, direction);
    var layers = CloudLayers(0.0, 0.0, 0.0);
    if (max(max(mass.r, mass.g), mass.b) <= 0.0) { return layers; }
    let geometry = textureSample(weather_geometry_tex, height_sampler, direction);

    let deep_base = mix(geometry.r, geometry.g, 0.28);
    let deep_top = max(geometry.b, deep_base + 0.5);
    let deep_height_fraction = clamp((altitude_km - deep_base) / max(deep_top - deep_base, 0.1), 0.0, 1.0);
    let detail_weight = clamp(uniforms.cloud_advection, 0.0, 1.0);
    let low_detail = isotropic_noise(
        direction, vec3<f32>(6.0, 12.0, 24.0),
        vec3<f32>(0.50, 0.32, 0.18), angular_pixel_footprint, 40u,
    );
    let deep_detail = isotropic_noise(
        direction, vec3<f32>(7.0, 14.0, 24.0),
        vec3<f32>(0.46, 0.34, 0.20), angular_pixel_footprint, 50u,
    );
    let tower_lobes = isotropic_noise(
        direction, vec3<f32>(7.0, 14.0, 24.0),
        vec3<f32>(0.52, 0.30, 0.18), angular_pixel_footprint, 60u,
    );
    let low_multiplier = clamp(
        1.0 + LOW_DETAIL_STRENGTH * detail_weight * (0.15 * low_detail.x + 0.13 * low_detail.y),
        0.72,
        1.22,
    );
    let deep_combined_detail = mix(deep_detail, tower_lobes, deep_height_fraction);
    let deep_multiplier = clamp(
        1.0 + DEEP_DETAIL_STRENGTH * detail_weight * (0.16 * deep_combined_detail.x + 0.06 * deep_combined_detail.y),
        0.72,
        1.22,
    );
    let low_depth_km = geometry.g - geometry.r;
    let shallow_family = smooth_step(0.7, 1.8, low_depth_km);
    let terrain_family = smooth_step(0.05, 0.22, textureSample(height_tex, height_sampler, direction).r)
        * (1.0 - smooth_step(0.05, 0.35, mass.g));
    let detached_base = mix(geometry.r, mix(geometry.r, geometry.g, 0.16), shallow_family);
    let low_profile = layer_profile(altitude_km, detached_base, geometry.g);
    let terrain_cap = layer_profile(
        altitude_km,
        mix(geometry.r, geometry.g, 0.15),
        mix(geometry.r, geometry.g, 0.8),
    );
    layers.low = max(
        mass.r * low_multiplier * LOW_OPTICAL_WEIGHT
            * mix(low_profile, terrain_cap, terrain_family * 0.45),
        0.0,
    );
    layers.deep = max(
        mass.g * deep_multiplier * layer_profile(altitude_km, deep_base, deep_top)
            * mix(1.18, 0.68, deep_height_fraction),
        0.0,
    );

    let fibres = filtered_noise(
        direction, vec3<f32>(10.0, 20.0, 32.0),
        vec3<f32>(0.54, 0.30, 0.16), 1.20, angular_pixel_footprint, 70u,
    );
    let high_base = max(geometry.b, geometry.a - 3.0);
    let high_modulation = clamp(
        1.0 + HIGH_DETAIL_STRENGTH * detail_weight * (0.60 * fibres.x + 0.05 * fibres.y),
        0.72,
        1.22,
    );
    layers.high = max(mass.b * layer_profile(altitude_km, high_base, geometry.a) * high_modulation * 0.28, 0.0);
    return layers;
}

fn weather_cloud_layers_land_segment(
    dir: vec3<f32>,
    altitude_km: f32,
    segment_start: vec3<f32>,
    segment_end: vec3<f32>,
    radius_km: f32,
    angular_pixel_footprint: f32,
) -> CloudLayers {
    let direction = normalize(dir);
    let height = textureSample(height_tex, height_sampler, direction).r;
    let land_factor = smooth_step(0.01, 0.05, height - uniforms.ocean_level);
    var layers = weather_cloud_layers(direction, altitude_km, angular_pixel_footprint);
    if (land_factor <= 0.0) { return layers; }

    let mass = textureSample(weather_mass_tex, height_sampler, direction);
    if (mass.r <= 0.0) { return layers; }
    let geometry = textureSample(weather_geometry_tex, height_sampler, direction);
    let detail_weight = clamp(uniforms.cloud_advection, 0.0, 1.0);
    let low_detail = isotropic_noise(
        direction, vec3<f32>(6.0, 12.0, 24.0),
        vec3<f32>(0.50, 0.32, 0.18), angular_pixel_footprint, 40u,
    );
    let low_multiplier = clamp(
        1.0 + LOW_DETAIL_STRENGTH * detail_weight * (0.15 * low_detail.x + 0.13 * low_detail.y),
        0.72,
        1.22,
    );
    let low_depth = geometry.g - geometry.r;
    let shallow_family = smooth_step(0.7, 1.8, low_depth);
    let terrain_family = smooth_step(0.05, 0.22, height)
        * (1.0 - smooth_step(0.05, 0.35, mass.g));
    let detached_base = mix(geometry.r, mix(geometry.r, geometry.g, 0.16), shallow_family);
    let low_profile = layer_profile_segment_mean(
        segment_start, segment_end, detached_base, geometry.g, radius_km,
    );
    let terrain_cap = layer_profile_segment_mean(
        segment_start, segment_end, mix(geometry.r, geometry.g, 0.15), mix(geometry.r, geometry.g, 0.8), radius_km,
    );
    let exact_low = mass.r * low_multiplier * LOW_OPTICAL_WEIGHT
        * mix(low_profile, terrain_cap, terrain_family * 0.45);
    layers.low = mix(layers.low, exact_low, land_factor);
    return layers;
}

fn weather_column_density(dir: vec3<f32>, angular_pixel_footprint: f32) -> f32 {
    let direction = normalize(dir);
    let mass = textureSample(weather_mass_tex, height_sampler, direction);
    if (max(max(mass.r, mass.g), mass.b) <= 0.0) { return 0.0; }
    let geometry = textureSample(weather_geometry_tex, height_sampler, direction);
    let low_mid = (geometry.r + geometry.g) * 0.5;
    let deep_base = mix(geometry.r, geometry.g, 0.28);
    let deep_mid = (deep_base + max(geometry.b, deep_base + 0.5)) * 0.5;
    let high_mid = (max(geometry.b, geometry.a - 3.0) + geometry.a) * 0.5;
    let low = weather_cloud_layers(direction, low_mid, angular_pixel_footprint).low;
    let deep = weather_cloud_layers(direction, deep_mid, angular_pixel_footprint).deep;
    let high = weather_cloud_layers(direction, high_mid, angular_pixel_footprint).high;
    return clamp(low * 0.9 + deep * 1.65 + high * 0.32, 0.0, 1.0) * cloud_display_scale();
}
