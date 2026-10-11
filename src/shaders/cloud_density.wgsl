// Shared weather-driven density functions. Preview and export compile this unchanged.
// The weather field owns cloud coverage, condensate and vertical geometry.
const LOW_OPTICAL_WEIGHT: f32 = 0.50;
const CLOUD_PHASE_G: f32 = 0.55;
const CLOUD_PHASE_MAX: f32 = 0.62;
// Conversion from the solver's normalized condensate columns to optical depth.
// This is an appearance calibration, not a measured microphysical coefficient.
// Liquid banks need substantial reflection rather than a translucent gray veil.
const CLOUD_LIGHT_EXTINCTION: f32 = 4.5;

struct CloudLayers {
    low: f32,
    deep: f32,
    high: f32,
}

struct CloudColumn {
    y: f32, coverage: f32, base_km: f32, thickness_km: f32, character: f32, cirrus: f32,
}

fn cloud_layer_geometry(geometry: vec4<f32>, layers: CloudLayers) -> vec4<f32> {
    let deep_base = mix(geometry.r, geometry.g, 0.28);
    let deep_top = max(geometry.b, deep_base + 0.5);
    let high_base = max(geometry.b, geometry.a - 3.0);
    let low_weight = layers.low * 0.90;
    let deep_weight = layers.deep * 1.65;
    let high_weight = layers.high * 0.32;
    let weight = low_weight + deep_weight + high_weight;
    let base = (low_weight * geometry.r + deep_weight * deep_base + high_weight * high_base) / max(weight, 1.0e-12);
    let thickness = (low_weight * max(geometry.g - geometry.r, 0.0)
        + deep_weight * max(deep_top - deep_base, 0.0)
        + high_weight * max(geometry.a - high_base, 0.0)) / max(weight, 1.0e-12);
    return vec4<f32>(weight, base, thickness, deep_weight);
}

fn cloud_geometry_moments(direction: vec3<f32>) -> vec3<f32> {
    let mass = sample_weather_cube(weather_mass_tex, direction);
    let geometry = sample_weather_cube(weather_geometry_tex, direction);
    let packed = cloud_layer_geometry(geometry, CloudLayers(
        mass.r * LOW_OPTICAL_WEIGHT, mass.g, mass.b * 0.28,
    ));
    return vec3<f32>(packed.x, packed.x * packed.y, packed.x * packed.z);
}

// Filter optical moments before dividing by mass. Filtering altitude alone lets
// nearly empty high clouds pull a low cloud bank upward by kilometres.
fn filtered_cloud_geometry(direction: vec3<f32>) -> vec2<f32> {
    let reference = select(vec3<f32>(0.0, 1.0, 0.0), vec3<f32>(1.0, 0.0, 0.0), abs(direction.y) > 0.9);
    let tangent_x = normalize(cross(reference, direction));
    let tangent_y = cross(direction, tangent_x);
    let step = 2.0 / f32(textureDimensions(weather_mass_tex).x);
    var moments = vec3<f32>(0.0);
    for (var y = -1; y <= 1; y++) {
        for (var x = -1; x <= 1; x++) {
            let weight = select(1.0, 2.0, x == 0) * select(1.0, 2.0, y == 0);
            let sample_direction = normalize(direction + step * (f32(x) * tangent_x + f32(y) * tangent_y));
            moments += weight * cloud_geometry_moments(sample_direction);
        }
    }
    return moments.yz / max(moments.x, 1.0e-12);
}

// Radial columns give texture inspection and export the same camera-independent values.
fn cloud_column_sample(direction: vec3<f32>, footprint: f32) -> CloudColumn {
    let mass = sample_weather_cube(weather_mass_tex, direction);
    if (max(max(mass.r, mass.g), mass.b) <= 0.0) { return CloudColumn(0.0, 0.0, 0.0, 0.0, 0.0, 0.0); }
    let geometry = sample_weather_cube(weather_geometry_tex, direction);
    let radius_km = max(uniforms.planet_radius_km, 1.0);
    let step_km = max(geometry.a, 0.0) / 8.0;
    var optical_depth = 0.0;
    var geometry_weight = 0.0;
    var low_sum = 0.0;
    var deep_sum = 0.0;
    for (var sample_index = 0u; sample_index < 8u; sample_index++) {
        let altitude_km = (f32(sample_index) + 0.5) * step_km;
        let segment_start = direction * (1.0 + f32(sample_index) * step_km / radius_km);
        let segment_end = direction * (1.0 + (f32(sample_index) + 1.0) * step_km / radius_km);
        let sample = weather_cloud_layers_land_segment(
            direction, altitude_km, segment_start, segment_end, radius_km, footprint,
        );
        let packed = cloud_layer_geometry(sample.geometry, sample.layers);
        optical_depth += packed.x * step_km * CLOUD_LIGHT_EXTINCTION;
        geometry_weight += packed.x;
        low_sum += sample.layers.low;
        deep_sum += sample.layers.deep;
    }
    if (geometry_weight <= 0.0) { return CloudColumn(0.0, 0.0, 0.0, 0.0, 0.0, 0.0); }
    let filtered_geometry = filtered_cloud_geometry(direction);
    return CloudColumn(
        optical_depth,
        clamp(mass.r + mass.g + mass.b, 0.0, 1.0),
        filtered_geometry.x,
        filtered_geometry.y,
        select(0.0, clamp(deep_sum / (low_sum + deep_sum), 0.0, 1.0), low_sum + deep_sum > 0.0),
        clamp(mass.b, 0.0, 1.0),
    );
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

// Differential of the orthographic unit-sphere intersection. A normalized
// (x,y,0.5) proxy doubled the footprint at disk center and underestimated it
// near the limb, filtering away fine central structure while aliasing the rim.
fn cloud_sphere_pixel_footprint(projected: vec2<f32>, pixel_dx: vec2<f32>, pixel_dy: vec2<f32>) -> f32 {
    let z = sqrt(max(1.0 - dot(projected, projected), 0.0016));
    let tangent_dx = vec3<f32>(pixel_dx, -dot(projected, pixel_dx) / z);
    let tangent_dy = vec3<f32>(pixel_dy, -dot(projected, pixel_dy) / z);
    return max(length(tangent_dx), length(tangent_dy));
}


// Two normalized components share the same column mass: a compact lower deck
// and a lofted upper body. Thin layers retain the original single profile.
fn low_cloud_profile(altitude: f32, base: f32, top: f32) -> f32 {
    let depth = max(top - base, 0.05);
    let layered = smooth_step(0.7, 1.8, depth) * 0.65;
    let bodies = 0.35 * layer_profile(altitude, base, base + depth * 0.38)
        + 0.65 * layer_profile(altitude, base + depth * 0.30, top);
    return mix(layer_profile(altitude, base, top), bodies, layered);
}

fn low_cloud_segment(p0: vec3<f32>, p1: vec3<f32>, base: f32, top: f32, radius: f32) -> f32 {
    let depth = max(top - base, 0.05);
    let layered = smooth_step(0.7, 1.8, depth) * 0.65;
    let bodies = 0.35 * layer_profile_segment_mean(p0, p1, base, base + depth * 0.38, radius)
        + 0.65 * layer_profile_segment_mean(p0, p1, base + depth * 0.30, top, radius);
    return mix(layer_profile_segment_mean(p0, p1, base, top, radius), bodies, layered);
}

// Rendering follows transported condensate and diagnosed layer heights directly.
// Seed and wind affect weather formation; they do not stamp an opacity texture.
// Weather generation places the first/last samples on the face edges, while
// hardware cube filtering assumes texel centers. Move face coordinates into
// that center lattice so duplicate edge samples do not form a flat seam band.
fn sample_weather_cube(field: texture_cube<f32>, direction: vec3<f32>) -> vec4<f32> {
    let a = abs(direction);
    let major = max(a.x, max(a.y, a.z));
    var face_normal: vec3<f32>;
    if (a.x >= a.y && a.x >= a.z) {
        face_normal = vec3<f32>(select(-1.0, 1.0, direction.x >= 0.0), 0.0, 0.0);
    } else if (a.y >= a.z) {
        face_normal = vec3<f32>(0.0, select(-1.0, 1.0, direction.y >= 0.0), 0.0);
    } else {
        face_normal = vec3<f32>(0.0, 0.0, select(-1.0, 1.0, direction.z >= 0.0));
    }
    let res = f32(textureDimensions(field).x);
    let centered = direction * ((res - 1.0) / res) + face_normal * (major / res);
    return textureSampleLevel(field, height_sampler, centered, 0.0);
}

fn weather_cloud_sample(dir: vec3<f32>, altitude_km: f32, angular_pixel_footprint: f32) -> WeatherCloudSample {
    let direction = normalize(dir);
    let mass = sample_weather_cube(weather_mass_tex, direction);
    let geometry = sample_weather_cube(weather_geometry_tex, direction);
    let height = textureSampleLevel(height_tex, height_sampler, direction, 0.0).r;
    var sample = WeatherCloudSample(CloudLayers(0.0, 0.0, 0.0), mass, geometry, height, 1.0, 1.0, 0.28);
    if (max(max(mass.r, mass.g), mass.b) <= 0.0) { return sample; }
    let detached_base = mix(geometry.r, mix(geometry.r, geometry.g, 0.16),
        smooth_step(0.7, 1.8, geometry.g - geometry.r));
    let deep_base = mix(geometry.r, geometry.g, 0.28);
    sample.layers.low = mass.r * LOW_OPTICAL_WEIGHT
        * low_cloud_profile(altitude_km, detached_base, geometry.g);
    sample.layers.deep = mass.g
        * layer_profile(altitude_km, deep_base, max(geometry.b, deep_base + 0.5));
    sample.layers.high = mass.b * sample.high_multiplier
        * layer_profile(altitude_km, max(geometry.b, geometry.a - 3.0), geometry.a);
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
        * low_cloud_segment(segment_start, segment_end, detached_base, g.g, radius_km);
    sample.layers.deep = sample.mass.g * sample.deep_multiplier
        * layer_profile_segment_mean(segment_start, segment_end, deep_base, max(g.b, deep_base + 0.5), radius_km);
    sample.layers.high = sample.mass.b * sample.high_multiplier
        * layer_profile_segment_mean(segment_start, segment_end, max(g.b, g.a - 3.0), g.a, radius_km);
    return sample;
}

fn weather_column_density_raw(dir: vec3<f32>, angular_pixel_footprint: f32) -> f32 {
    let direction = normalize(dir);
    let mass = sample_weather_cube(weather_mass_tex, direction);
    if (max(max(mass.r, mass.g), mass.b) <= 0.0) { return 0.0; }
    let geometry = sample_weather_cube(weather_geometry_tex, direction);
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
    world_pos: vec3<f32>, sun_dir: vec3<f32>, radius_km: f32, sample: WeatherCloudSample,
) -> f32 {
    let geometry = sample.geometry;
    let deep_base = mix(geometry.r, geometry.g, 0.28);
    let deep_top = max(geometry.b, deep_base + 0.5);
    let high_base = max(geometry.b, geometry.a - 3.0);
    let planet_hit = ray_sphere_positive_intersection(world_pos, sun_dir, 1.0);
    if (planet_hit >= 0.0) { return 0.0; }
    // Integrate every occupied layer above the sample. A local-density times
    // distance estimate missed detached decks/cirrus whenever the shading
    // point lay below them, making stacked clouds look like a single sheet.
    let outer_radius = 1.0 + max(max(geometry.a, deep_top), geometry.g) / radius_km;
    let projection = dot(world_pos, sun_dir);
    let exit_distance = max(-projection + sqrt(max(projection * projection + outer_radius * outer_radius - dot(world_pos, world_pos), 0.0)), 0.0);
    let end = world_pos + sun_dir * exit_distance;
    let detached_base = mix(geometry.r, mix(geometry.r, geometry.g, 0.16), smooth_step(0.7, 1.8, geometry.g - geometry.r));
    let low = sample.mass.r * sample.low_multiplier * LOW_OPTICAL_WEIGHT * 0.90
        * low_cloud_segment(world_pos, end, detached_base, geometry.g, radius_km);
    let deep = sample.mass.g * sample.deep_multiplier * 1.65
        * layer_profile_segment_mean(world_pos, end, deep_base, deep_top, radius_km);
    let high = sample.mass.b * sample.high_multiplier * 0.32
        * layer_profile_segment_mean(world_pos, end, high_base, geometry.a, radius_km);
    let tau = min(cloud_display_scale() * (low + deep + high) * exit_distance * radius_km, 20.0);
    return clamp(cloud_beer_lambert(tau), 0.0, 1.0);
}

fn cloud_surface_shadow(
    world_direction: vec3<f32>, sun_dir: vec3<f32>, radius_km: f32, angular_pixel_footprint: f32,
) -> f32 {
    let direction = normalize(world_direction);
    let mass = sample_weather_cube(weather_mass_tex, direction);
    let weight = mass.r + mass.g + mass.b;
    if (weight <= 0.0) { return 1.0; }
    let geometry = sample_weather_cube(weather_geometry_tex, direction);
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
    return exp(-density * shadow_scale * CLOUD_LIGHT_EXTINCTION * (3.5 / 3.0));
}
