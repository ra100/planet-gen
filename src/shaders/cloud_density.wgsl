// Shared weather-driven density functions. Preview and export compile this unchanged.
// The weather field owns the large-scale mass layout. Subgrid boundary
// displacement stays inside its support; it is not another cloud-coverage mask.
const LOW_DETAIL_STRENGTH: f32 = 1.0;
const DEEP_DETAIL_STRENGTH: f32 = 1.0;
// High cirrus keeps its supported, wind-owned edge observable to U3.
const HIGH_DETAIL_STRENGTH: f32 = 1.0;
const LOW_OPTICAL_WEIGHT: f32 = 0.50;
const CLOUD_PHASE_G: f32 = 0.55;
const CLOUD_PHASE_MAX: f32 = 0.62;
// Conversion from the solver's normalized condensate columns to optical depth.
// This is an appearance calibration, not a measured microphysical coefficient.
// 1.2 left the default decks as translucent haze; 6.0 over-occluded them.
const CLOUD_LIGHT_EXTINCTION: f32 = 3.0;

struct CloudLayers {
    low: f32,
    deep: f32,
    high: f32,
}

struct WeatherCloudSample {
    layers: CloudLayers,
    mass: vec4<f32>,
    geometry: vec4<f32>,
    height: f32,
    low_multiplier: f32,
    deep_multiplier: f32,
    high_multiplier: f32,
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

fn cloud_phase(cos_theta: f32) -> f32 {
    let g2 = CLOUD_PHASE_G * CLOUD_PHASE_G;
    let denominator = max(1.0 + g2 - 2.0 * CLOUD_PHASE_G * clamp(cos_theta, -1.0, 1.0), 1.0e-4);
    return min((1.0 - g2) / (4.0 * 3.14159 * pow(denominator, 1.5)), CLOUD_PHASE_MAX);
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
    let higher_weights = weights.yz * band_limit.yz;
    let dominant = wind_filtered_dominant_noise(direction, frequencies.x, stretch, seed);
    let higher = vec2<f32>(
        snoise(direction * frequencies.y + noise_seed_offset(uniforms.cloud_seed, seed + 1u)),
        snoise(direction * frequencies.z + noise_seed_offset(uniforms.cloud_seed, seed + 2u)),
    );
    return vec2<f32>(
        dominant * band_limit.x,
        dot(higher, higher_weights) / max(dot(weights.yz, vec2<f32>(1.0)), 1.0e-4),
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
        noise.x * band_limit.x,
        dot(noise.yz, weighted.yz) / max(dot(weights.yz, vec2<f32>(1.0)), 1.0e-4),
    );
}

// Detail is a small perturbation of the weather field, not a second occupancy
// mask. Thresholding noise across every column made repeated camouflage cells.
// Preserve dense decks; let tenuous fringes carry most of the subgrid texture.
fn cloud_detail_modulation(detail: vec2<f32>, mass: f32, strength: f32, amplitude: vec2<f32>) -> f32 {
    let fringe = 1.0 - smooth_step(0.08, 0.40, mass);
    let perturbation = dot(clamp(detail, vec2<f32>(-1.0), vec2<f32>(1.0)), amplitude);
    return 1.0 + clamp(strength, 0.0, 1.0) * (0.20 + 0.80 * fringe) * perturbation;
}

// Differential of the orthographic unit-sphere intersection. A normalized
// (x,y,0.5) proxy doubled the footprint at disk center and underestimated it
// near the limb, filtering away fine central structure while aliasing the rim.
fn cloud_sphere_pixel_footprint(projected: vec2<f32>, pixel_dx: vec2<f32>, pixel_dy: vec2<f32>) -> f32 {
    let z = sqrt(max(1.0 - dot(projected, projected), 0.0016));
    let tangent_dx = vec3<f32>(pixel_dx, -dot(projected, pixel_dx) / z);
    let tangent_dy = vec3<f32>(pixel_dy, -dot(projected, pixel_dy) / z);
    return max(length(tangent_dx), length(tangent_dy));
}

fn cloud_boundary_displacement(direction: vec3<f32>, footprint: f32) -> vec3<f32> {
    var displacement = vec3<f32>(0.0);
    let frequencies = vec3<f32>(13.0, 43.0, 119.0);
    // Keep the existing streams, but stop the broadest octave dominating the
    // outline. Finer boundary structure is resolved by the sphere footprint.
    let amplitudes = vec3<f32>(0.018, 0.014, 0.005);
    for (var octave = 0u; octave < 3u; octave++) {
        let p = direction * frequencies[octave] + noise_seed_offset(uniforms.cloud_seed, 80u + octave);
        let resolved = 1.0 - smooth_step(0.12, 0.50, frequencies[octave] * footprint);
        displacement += vec3<f32>(snoise(p), snoise(p + vec3<f32>(17.1, 3.7, 9.2)), snoise(p + vec3<f32>(5.3, 23.8, 1.6)))
            * amplitudes[octave] * resolved;
    }
    return displacement - direction * dot(displacement, direction);
}

// Compact, overlapping spherical puffs rather than thresholded fBm. The
// integral of (1-r^2)^3 over the unit ball is 64*pi/315; normalization retains
// mean column density in a homogeneous field. No new weather support is added.
fn cloud_puff_field(direction: vec3<f32>, frequency: f32, footprint: f32, seed: u32, stream: u32) -> f32 {
    let resolved = 1.0 - smooth_step(0.12, 0.45, frequency * footprint);
    if (resolved <= 0.0) { return 1.0; }
    let p = direction * frequency + noise_seed_offset(seed, stream);
    let cell = floor(p);
    var puffs = 0.0;
    for (var z = -1; z <= 1; z++) {
        for (var y = -1; y <= 1; y++) {
            for (var x = -1; x <= 1; x++) {
                let neighbor = cell + vec3<f32>(f32(x), f32(y), f32(z));
                var h = fract(neighbor * 0.1031);
                h += vec3<f32>(dot(h, h.yzx + vec3<f32>(33.33)));
                let jitter = fract((h.xxy + h.yzz) * h.zyx);
                let center = neighbor + jitter;
                let delta = p - center;
                let radius = 0.65 + 0.35 * fract(dot(jitter, vec3<f32>(13.7, 7.3, 23.1)));
                let lobe = max(1.0 - dot(delta, delta) / (radius * radius), 0.0);
                puffs += lobe * lobe * lobe;
            }
        }
    }
    // E[radius^3] for uniform radii in [0.65, 1] is 0.58678.
    return mix(1.0, 0.15 + 0.85 * puffs / (0.63829184 * 0.58678), resolved);
}

// A hierarchy of connected banks, lobes, and restrained internal detail. Fine
// puffs do not contribute an independent blanket of equal-sized bright dots.
fn cloud_cluster_field(direction: vec3<f32>, footprint: f32, seed: u32, stream: u32) -> f32 {
    let banks = cloud_puff_field(direction, 14.0, footprint, seed, stream);
    let lobes = cloud_puff_field(direction, 32.0, footprint, seed, stream + 1u);
    let interior = smooth_step(0.8, 1.8, banks);
    var fine = 1.0;
    if (interior > 0.0) {
        fine = cloud_puff_field(direction, 73.0, footprint, seed, stream + 2u);
    }
    return banks * mix(1.0, lobes, 0.25)
        * mix(1.0, fine, 0.08 * interior);
}

fn weather_cloud_sample(dir: vec3<f32>, altitude_km: f32, angular_pixel_footprint: f32) -> WeatherCloudSample {
    let direction = normalize(dir);
    let authored_mass = textureSampleLevel(weather_mass_tex, height_sampler, direction, 0.0);
    let geometry = textureSampleLevel(weather_geometry_tex, height_sampler, direction, 0.0);
    let height = textureSampleLevel(height_tex, height_sampler, direction, 0.0).r;
    if (max(max(authored_mass.r, authored_mass.g), authored_mass.b) <= 0.0) {
        return WeatherCloudSample(CloudLayers(0.0, 0.0, 0.0), authored_mass, geometry, height, 1.0, 1.0, 1.0);
    }
    let detail_weight = clamp(uniforms.cloud_advection, 0.0, 1.0);
    // Refine the boundary of the resolved weather, not its opacity everywhere.
    // A uniform deck remains uniform; an empty weather field remains empty.
    let strengths = vec3<f32>(LOW_DETAIL_STRENGTH, DEEP_DETAIL_STRENGTH, HIGH_DETAIL_STRENGTH) * detail_weight;
    var mass = authored_mass;
    if (max(max(strengths.x, strengths.y), strengths.z) > 0.0) {
        let displaced_direction = normalize(direction + cloud_boundary_displacement(direction, angular_pixel_footprint));
        let displaced_mass = textureSampleLevel(weather_mass_tex, height_sampler, displaced_direction, 0.0);
        let refined = mix(authored_mass.rgb, displaced_mass.rgb, clamp(strengths, vec3<f32>(0.0), vec3<f32>(1.0)));
        mass = vec4<f32>(select(refined, vec3<f32>(0.0), authored_mass.rgb <= vec3<f32>(0.0)), authored_mass.a);
    }
    var sample = WeatherCloudSample(CloudLayers(0.0, 0.0, 0.0), mass, geometry, height, 1.0, 1.0, 1.0);
    if (max(max(mass.r, mass.g), mass.b) <= 0.0) { return sample; }

    let deep_base = mix(geometry.r, geometry.g, 0.28);
    let deep_top = max(geometry.b, deep_base + 0.5);
    let deep_height_fraction = clamp((altitude_km - deep_base) / max(deep_top - deep_base, 0.1), 0.0, 1.0);
    let low_detail = isotropic_noise(
        direction, vec3<f32>(11.0, 37.0, 101.0),
        vec3<f32>(0.50, 0.32, 0.18), angular_pixel_footprint, 40u,
    );
    let deep_detail = isotropic_noise(
        direction, vec3<f32>(13.0, 41.0, 107.0),
        vec3<f32>(0.46, 0.34, 0.20), angular_pixel_footprint, 50u,
    );
    let tower_lobes = isotropic_noise(
        direction, vec3<f32>(17.0, 47.0, 127.0),
        vec3<f32>(0.52, 0.30, 0.18), angular_pixel_footprint, 60u,
    );
    sample.low_multiplier = cloud_detail_modulation(
        low_detail, mass.r, LOW_DETAIL_STRENGTH * detail_weight, vec2<f32>(0.035, 0.07),
    );
    let deep_combined_detail = mix(deep_detail, tower_lobes, deep_height_fraction);
    let deep_multiplier = cloud_detail_modulation(
        deep_combined_detail, mass.g, DEEP_DETAIL_STRENGTH * detail_weight, vec2<f32>(0.045, 0.065),
    );
    let low_depth_km = geometry.g - geometry.r;
    let shallow_family = smooth_step(0.7, 1.8, low_depth_km);
    // Thin stable decks stay continuous; deeper, dilute low-cloud layers form
    // connected banks with smaller lobes. Dense overcast stays intact.
    let low_puff_weight = shallow_family * (1.0 - smooth_step(0.20, 0.65, mass.r))
        * (1.0 - 0.65 * smooth_step(0.03, 0.18, mass.g))
        * 0.35 * LOW_DETAIL_STRENGTH * detail_weight;
    if (low_puff_weight > 0.0 && mass.r > 0.0) {
        sample.low_multiplier *= mix(1.0,
            cloud_cluster_field(direction, angular_pixel_footprint, uniforms.cloud_seed, 110u), low_puff_weight);
    }
    // Geometry comes from the weather field.  Do not let the terrain height
    // switch integration/profile at the coastline: that made identical weather
    // columns acquire an artificial land outline.
    let detached_base = mix(geometry.r, mix(geometry.r, geometry.g, 0.16), shallow_family);
    let low_profile = layer_profile(altitude_km, detached_base, geometry.g);
    sample.layers.low = max(
        mass.r * sample.low_multiplier * LOW_OPTICAL_WEIGHT
            * low_profile,
        0.0,
    );
    sample.deep_multiplier = deep_multiplier * mix(1.18, 0.68, deep_height_fraction);
    // Storm columns have broader lobes than shallow cumulus. The broad form
    // stays vertically coherent rather than changing noise at every ray step.
    let tower_weight = (1.0 - smooth_step(0.30, 0.75, mass.g))
        * 0.65 * DEEP_DETAIL_STRENGTH * detail_weight;
    if (tower_weight > 0.0 && mass.g > 0.0) {
        sample.deep_multiplier *= mix(1.0,
            cloud_puff_field(direction, 24.0, angular_pixel_footprint, uniforms.cloud_seed, 120u), tower_weight);
    }
    sample.layers.deep = max(
        mass.g * sample.deep_multiplier * layer_profile(altitude_km, deep_base, deep_top),
        0.0,
    );

    let fibres = filtered_noise(
        direction, vec3<f32>(17.0, 53.0, 113.0),
        vec3<f32>(0.54, 0.30, 0.16), 1.85, angular_pixel_footprint, 70u,
    );
    let high_base = max(geometry.b, geometry.a - 3.0);
    let high_modulation = cloud_detail_modulation(
        fibres, mass.b, HIGH_DETAIL_STRENGTH * detail_weight, vec2<f32>(0.12, 0.12),
    );
    // Optically thin cirrus has stronger wind-filtered filament contrast than
    // either liquid layer; retain the existing dense-sheet limit.
    let filament_weight = (1.0 - smooth_step(0.20, 0.65, mass.b))
        * HIGH_DETAIL_STRENGTH * detail_weight;
    let filament = clamp(1.0 + 2.4 * fibres.x + 1.2 * fibres.y, 0.15, 2.5);
    sample.high_multiplier = high_modulation * mix(1.0, filament, filament_weight) * 0.28;
    sample.layers.high = max(mass.b * layer_profile(altitude_km, high_base, geometry.a) * sample.high_multiplier, 0.0);
    return sample;
}

fn weather_cloud_layers(dir: vec3<f32>, altitude_km: f32, angular_pixel_footprint: f32) -> CloudLayers {
    return weather_cloud_sample(dir, altitude_km, angular_pixel_footprint).layers;
}

fn weather_cloud_layers_land_segment(
    dir: vec3<f32>,
    altitude_km: f32,
    segment_start: vec3<f32>,
    segment_end: vec3<f32>,
    radius_km: f32,
    angular_pixel_footprint: f32,
) -> WeatherCloudSample {
    // Integrate thin profiles on every surface, not only above a land mask.
    // Formation mass and geometry alone own cloud shape. Both renderers use this.
    var sample = weather_cloud_sample(normalize(dir), altitude_km, angular_pixel_footprint);
    let g = sample.geometry;
    let detached_base = mix(g.r, mix(g.r, g.g, 0.16), smooth_step(0.7, 1.8, g.g - g.r));
    let deep_base = mix(g.r, g.g, 0.28);
    sample.layers.low = sample.mass.r * sample.low_multiplier * LOW_OPTICAL_WEIGHT
        * layer_profile_segment_mean(segment_start, segment_end, detached_base, g.g, radius_km);
    sample.layers.deep = sample.mass.g * sample.deep_multiplier
        * layer_profile_segment_mean(segment_start, segment_end, deep_base, max(g.b, deep_base + 0.5), radius_km);
    sample.layers.high = sample.mass.b * sample.high_multiplier
        * layer_profile_segment_mean(segment_start, segment_end, max(g.b, g.a - 3.0), g.a, radius_km);
    return sample;
}

fn weather_column_density_raw(dir: vec3<f32>, angular_pixel_footprint: f32) -> f32 {
    let direction = normalize(dir);
    let mass = textureSampleLevel(weather_mass_tex, height_sampler, direction, 0.0);
    if (max(max(mass.r, mass.g), mass.b) <= 0.0) { return 0.0; }
    let geometry = textureSampleLevel(weather_geometry_tex, height_sampler, direction, 0.0);
    let low_mid = (geometry.r + geometry.g) * 0.5;
    let deep_base = mix(geometry.r, geometry.g, 0.28);
    let deep_mid = (deep_base + max(geometry.b, deep_base + 0.5)) * 0.5;
    let high_mid = (max(geometry.b, geometry.a - 3.0) + geometry.a) * 0.5;
    let low = weather_cloud_layers(direction, low_mid, angular_pixel_footprint).low;
    let deep = weather_cloud_layers(direction, deep_mid, angular_pixel_footprint).deep;
    let high = weather_cloud_layers(direction, high_mid, angular_pixel_footprint).high;
    return clamp(low * 0.9 + deep * 1.65 + high * 0.32, 0.0, 1.0);
}

fn weather_column_density(dir: vec3<f32>, angular_pixel_footprint: f32) -> f32 {
    return weather_column_density_raw(dir, angular_pixel_footprint) * cloud_display_scale();
}

fn ray_sphere_positive_intersection(origin: vec3<f32>, direction: vec3<f32>, radius: f32) -> f32 {
    let projection = dot(origin, direction);
    let discriminant = projection * projection + radius * radius - dot(origin, origin);
    if (discriminant <= 1.0e-6) { return -1.0; }
    let root = sqrt(discriminant);
    let near = -projection - root;
    if (near > 1.0e-6) { return near; }
    let far = -projection + root;
    return select(-1.0, far, far > 1.0e-6);
}

fn cloud_beer_lambert(optical_depth: f32) -> f32 {
    return exp(-max(optical_depth, 0.0) * CLOUD_LIGHT_EXTINCTION);
}

fn cloud_surface_shadow_offset(height_km: f32, radius_km: f32) -> f32 {
    return max(height_km, 0.0) / max(radius_km, 1.0);
}

fn cloud_surface_shadow_spread(thickness_km: f32, radius_km: f32) -> f32 {
    return min(max(thickness_km, 0.0) * 0.5 / max(radius_km, 1.0), 0.04);
}

fn cloud_sun_path_transmittance(
    world_pos: vec3<f32>, sun_dir: vec3<f32>, radius_km: f32, layers: CloudLayers, geometry: vec4<f32>,
) -> f32 {
    let deep_base = mix(geometry.r, geometry.g, 0.28);
    let deep_top = max(geometry.b, deep_base + 0.5);
    let high_base = max(geometry.b, geometry.a - 3.0);
    let planet_hit = ray_sphere_positive_intersection(world_pos, sun_dir, 1.0);
    if (planet_hit >= 0.0) { return 0.0; }
    let weighted_layers = vec3<f32>(layers.low * 0.90, layers.deep * 1.65, layers.high * 0.32);
    let weight = dot(weighted_layers, vec3<f32>(1.0));
    if (weight <= 1.0e-6) { return 1.0; }
    let base_km = dot(weighted_layers, vec3<f32>(geometry.r, deep_base, high_base)) / weight;
    let top_km = dot(weighted_layers, vec3<f32>(geometry.g, deep_top, geometry.a)) / weight;
    let inner_radius = 1.0 + max(base_km, 0.0) / radius_km;
    let outer_radius = 1.0 + max(top_km, base_km) / radius_km;
    let projection = dot(world_pos, sun_dir);
    let exit_distance = max(-projection + sqrt(max(projection * projection + outer_radius * outer_radius - dot(world_pos, world_pos), 0.0)), 0.0);
    let inner_hit = ray_sphere_positive_intersection(world_pos, sun_dir, inner_radius);
    let path_km = min(exit_distance, select(exit_distance, inner_hit, inner_hit >= 0.0)) * radius_km;
    let tau = min(cloud_display_scale() * weight * max(path_km, 0.0), 20.0);
    return clamp(cloud_beer_lambert(tau), 0.0, 1.0);
}

fn cloud_surface_shadow(
    world_direction: vec3<f32>, sun_dir: vec3<f32>, radius_km: f32, angular_pixel_footprint: f32,
) -> f32 {
    let direction = normalize(world_direction);
    let mass = textureSampleLevel(weather_mass_tex, height_sampler, direction, 0.0);
    let weight = mass.r + mass.g + mass.b;
    if (weight <= 0.0) { return 1.0; }
    let geometry = textureSampleLevel(weather_geometry_tex, height_sampler, direction, 0.0);
    let deep_base = mix(geometry.r, geometry.g, 0.28);
    let deep_top = max(geometry.b, deep_base + 0.5);
    let high_base = max(geometry.b, geometry.a - 3.0);
    let height_km = (mass.r * (geometry.r + geometry.g) * 0.5
        + mass.g * (deep_base + deep_top) * 0.5 + mass.b * (high_base + geometry.a) * 0.5) / weight;
    let thickness_km = (mass.r * (geometry.g - geometry.r) + mass.g * (deep_top - deep_base)
        + mass.b * (geometry.a - high_base)) / weight;
    let center = normalize(direction + sun_dir * cloud_surface_shadow_offset(height_km, radius_km));
    let spread = sun_dir * cloud_surface_shadow_spread(thickness_km, radius_km);
    let density = (weather_column_density_raw(normalize(center - spread), angular_pixel_footprint)
        + weather_column_density_raw(center, angular_pixel_footprint)
        + weather_column_density_raw(normalize(center + spread), angular_pixel_footprint)) / 3.0;
    let shadow_scale = clamp(uniforms.cloud_coverage, 0.0, 1.0) * clamp(uniforms.cloud_opacity, 0.0, 1.0);
    return exp(-density * shadow_scale * 3.5);
}
