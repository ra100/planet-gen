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
