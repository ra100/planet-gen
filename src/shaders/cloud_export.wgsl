struct Uniforms {
    rotation: mat4x4<f32>, light_dir: vec3<f32>, ocean_level: f32,
    base_temp_c: f32, ocean_fraction: f32, axial_tilt_rad: f32, view_mode: u32,
    season: f32, atmosphere_density: f32, atmosphere_height: f32, height_scale: f32,
    zoom: f32, pan_x: f32, pan_y: f32, cloud_coverage: f32, cloud_seed: u32,
    night_lights: f32, star_color_temp: f32, city_light_hue: f32, show_ao: f32,
    show_water: f32, show_ice: f32, show_biomes: f32, show_clouds: f32,
    show_atmosphere_layer: f32, show_cities: f32, cloud_opacity: f32,
    cloud_advection: f32, rotation_rate: f32, atm_pressure: f32, _pad4: f32,
    lava_glow: f32, ring_inner: f32, ring_outer: f32, ring_tilt: f32,
    ring_opacity: f32, planet_radius_km: f32, show_cloud_shadows: f32, _pad5: f32,
}

struct CloudExportParams {
    face: u32, tile_offset_x: u32, tile_offset_y: u32, tile_width: u32,
    tile_height: u32, full_resolution: u32, _pad0: u32, _pad1: u32,
}

struct CloudOutput {
    y: f32, coverage: f32, base_km: f32, thickness_km: f32, character: f32, cirrus: f32,
}

fn smooth_step(edge0: f32, edge1: f32, value: f32) -> f32 {
    let t = clamp((value - edge0) / max(edge1 - edge0, 1.0e-6), 0.0, 1.0);
    return t * t * (3.0 - 2.0 * t);
}

@group(0) @binding(0) var<uniform> uniforms: Uniforms;
@group(0) @binding(1) var height_tex: texture_cube<f32>;
@group(0) @binding(2) var height_sampler: sampler;
@group(0) @binding(3) var cloud_tex: texture_cube<f32>;
@group(0) @binding(4) var weather_mass_tex: texture_cube<f32>;
@group(0) @binding(5) var weather_geometry_tex: texture_cube<f32>;
@group(0) @binding(6) var<uniform> params: CloudExportParams;
@group(0) @binding(7) var<storage, read_write> output: array<CloudOutput>;

struct WindTangent { direction: vec3<f32>, magnitude: f32, }

fn wind_height_sample(direction: vec3<f32>) -> f32 {
    return textureSampleLevel(height_tex, height_sampler, direction, 0.0).r;
}

fn tangent_basis(direction: vec3<f32>) -> mat2x3<f32> {
    let reference = select(vec3<f32>(0.0, 1.0, 0.0), vec3<f32>(1.0, 0.0, 0.0), abs(direction.y) > 0.9);
    let first = cross(reference, direction);
    let tangent_x = first / max(length(first), 1.0e-6);
    return mat2x3<f32>(tangent_x, cross(direction, tangent_x));
}

fn sample_wind_tangent_data(sphere_pos: vec3<f32>) -> WindTangent {
    if (uniforms.cloud_advection > 0.5) {
        let basis = tangent_basis(sphere_pos);
        let r = 0.05;
        let w = textureSampleLevel(cloud_tex, height_sampler, sphere_pos, 0.0).xyz * 2.0
            + textureSampleLevel(cloud_tex, height_sampler, normalize(sphere_pos + (basis[0] + basis[1]) * r), 0.0).xyz
            + textureSampleLevel(cloud_tex, height_sampler, normalize(sphere_pos - (basis[0] + basis[1]) * r), 0.0).xyz;
        let tangent = w - sphere_pos * dot(w, sphere_pos);
        let speed = length(tangent);
        if (speed > 0.003) { return WindTangent(tangent / speed, speed * 0.25); }
    }
    let tilted_y = sphere_pos.y * cos(uniforms.axial_tilt_rad) + sphere_pos.z * sin(uniforms.axial_tilt_rad);
    let latitude = asin(clamp(tilted_y, -1.0, 1.0));
    return WindTangent(wind_direction_at(sphere_pos, latitude), 0.0);
}

fn cloud_layer_geometry(geometry: vec4<f32>, layers: CloudLayers) -> vec4<f32> {
    let deep_base = mix(geometry.r, geometry.g, 0.28);
    let deep_top = max(geometry.b, deep_base + 0.5);
    let high_base = max(geometry.b, geometry.a - 3.0);
    let low_weight = layers.low * 0.90;
    let deep_weight = layers.deep * 1.65;
    let high_weight = layers.high * 0.32;
    let weight = low_weight + deep_weight + high_weight;
    let base = (low_weight * geometry.r + deep_weight * deep_base + high_weight * high_base) / max(weight, 1.0e-6);
    let thickness = (low_weight * max(geometry.g - geometry.r, 0.0)
        + deep_weight * max(deep_top - deep_base, 0.0)
        + high_weight * max(geometry.a - high_base, 0.0)) / max(weight, 1.0e-6);
    return vec4<f32>(weight, base, thickness, deep_weight);
}

@compute @workgroup_size(8, 8)
fn main(@builtin(global_invocation_id) id: vec3<u32>) {
    if (id.x >= params.tile_width || id.y >= params.tile_height) { return; }
    let pixel = id.y * params.tile_width + id.x;
    let uv = vec2<f32>(
        f32(params.tile_offset_x + id.x) / f32(params.full_resolution - 1u),
        f32(params.tile_offset_y + id.y) / f32(params.full_resolution - 1u),
    );
    let direction = cube_to_sphere(params.face, uv);
    let mass = textureSampleLevel(weather_mass_tex, height_sampler, direction, 0.0);
    if (max(max(mass.r, mass.g), mass.b) <= 0.0) { output[pixel] = CloudOutput(0.0, 0.0, 0.0, 0.0, 0.0, 0.0); return; }
    let geometry = textureSampleLevel(weather_geometry_tex, height_sampler, direction, 0.0);
    let radius_km = max(uniforms.planet_radius_km, 1.0);
    let top_km = max(geometry.a, 0.0);
    let step_km = top_km / 8.0;
    let footprint = 2.0 / f32(params.full_resolution);
    var optical_depth = 0.0;
    var geometry_weight = 0.0;
    var base_sum = 0.0;
    var thickness_sum = 0.0;
    var low_sum = 0.0;
    var deep_sum = 0.0;
    for (var sample_index = 0u; sample_index < 8u; sample_index++) {
        let altitude_km = (f32(sample_index) + 0.5) * step_km;
        // FE-089: export marches radially per texel direction (no camera ray),
        // so each sample's segment is the radial slice between its altitude
        // band edges. This calls the SAME shared land-segment function preview
        // uses, closing the documented preview/export divergence over land
        // (882b7cb NOTE). Ocean texels hit the land gate early return and are
        // bit-exact unchanged; radial points satisfy length(p) = 1 + h/radius_km.
        let segment_start = direction * (1.0 + f32(sample_index) * step_km / radius_km);
        let segment_end = direction * (1.0 + (f32(sample_index) + 1.0) * step_km / radius_km);
        let sample = weather_cloud_layers_land_segment(
            direction, altitude_km, segment_start, segment_end, radius_km, footprint,
        );
        let packed = cloud_layer_geometry(sample.geometry, sample.layers);
        optical_depth += packed.x * step_km * CLOUD_LIGHT_EXTINCTION;
        geometry_weight += packed.x;
        base_sum += packed.x * packed.y;
        thickness_sum += packed.x * packed.z;
        low_sum += sample.layers.low;
        deep_sum += sample.layers.deep;
    }
    if (geometry_weight <= 0.0) { output[pixel] = CloudOutput(0.0, 0.0, 0.0, 0.0, 0.0, 0.0); return; }
    output[pixel] = CloudOutput(
        optical_depth,
        clamp(mass.r + mass.g + mass.b, 0.0, 1.0),
        base_sum / geometry_weight,
        thickness_sum / geometry_weight,
        select(0.0, clamp(deep_sum / (low_sum + deep_sum), 0.0, 1.0), low_sum + deep_sum > 0.0),
        clamp(mass.b, 0.0, 1.0),
    );
}
