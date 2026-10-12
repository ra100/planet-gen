// Low-frequency cloud driver maps for external renderers (the Blender bridge).
// Concatenate after climate.wgsl and cloud_export.wgsl. Each record holds three
// RGBA texels for one equirectangular pixel, sampled at the pixel centre:
//   cloud:   low, deep, high condensate mass, combined coverage
//   climate: land fraction, thermal (0 at -25 C, 1 at 30 C), cloud top / 20 km, continentality
//   wind:    tangent wind xyz in planet space (Y up), wind speed
// CloudExportParams carries the band: tile_width = map width, full_resolution =
// map height, tile_offset_y = first row, tile_height = row count.
// These are the weather solver's own fields, before any detail noise, so a
// renderer can add its own high-frequency structure on top.
struct CloudDriverTexel {
    cloud: vec4<f32>,
    climate: vec4<f32>,
    wind: vec4<f32>,
}

@group(0) @binding(7) var<storage, read_write> driver_output: array<CloudDriverTexel>;

const CLOUD_DRIVER_TOP_SCALE_KM: f32 = 20.0;

fn equirect_center_direction(x: u32, y: u32, width: u32, height: u32) -> vec3<f32> {
    let lon = 6.2831853 * (f32(x) + 0.5) / f32(width) - 3.1415927;
    let lat = 3.1415927 * (0.5 - (f32(y) + 0.5) / f32(height));
    return vec3<f32>(cos(lat) * sin(lon), sin(lat), cos(lat) * cos(lon));
}

@compute @workgroup_size(8, 8)
fn drivers(@builtin(global_invocation_id) id: vec3<u32>) {
    if (id.x >= params.tile_width || id.y >= params.tile_height) { return; }
    let direction = equirect_center_direction(
        id.x, params.tile_offset_y + id.y, params.tile_width, params.full_resolution,
    );

    let mass = sample_weather_cube(weather_mass_tex, direction);
    let low = clamp(mass.r, 0.0, 1.0);
    let deep = clamp(mass.g, 0.0, 1.0);
    let high = clamp(mass.b, 0.0, 1.0);
    let coverage = clamp(mass.r + mass.g + mass.b, 0.0, 1.0);
    var top = 0.0;
    if (coverage > 0.0) {
        let geometry = filtered_cloud_geometry(direction);
        top = clamp((geometry.x + geometry.y) / CLOUD_DRIVER_TOP_SCALE_KM, 0.0, 1.0);
    }

    let height = textureSampleLevel(height_tex, height_sampler, direction, 0.0).r;
    // One height unit is 5 km; a 25 m band keeps the coastline soft at map resolution.
    let land = smooth_step(uniforms.ocean_level - 0.005, uniforms.ocean_level + 0.005, height);
    let temperature = climate_temperature(direction, height, uniforms.ocean_level,
        uniforms.base_temp_c, uniforms.axial_tilt_rad, uniforms.season);
    let thermal = smooth_step(-25.0, 30.0, temperature);

    let wind = textureSampleLevel(cloud_tex, height_sampler, direction, 0.0);
    let tangent = wind.xyz - direction * dot(wind.xyz, direction);

    driver_output[id.y * params.tile_width + id.x] = CloudDriverTexel(
        vec4<f32>(low, deep, high, coverage),
        vec4<f32>(land, thermal, top, clamp(wind.a, 0.0, 1.0)),
        vec4<f32>(tangent, length(tangent)),
    );
}
