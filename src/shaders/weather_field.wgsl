struct WeatherSnapshot {
    face: u32,
    resolution: u32,
    seed: u32,
    storm_count: u32,
    coverage: f32,
    moisture: f32,
    atm_pressure: f32,
    base_temp_c: f32,
    ocean_level: f32,
    axial_tilt_rad: f32,
    season: f32,
    storm_size: f32,
    cloud_character: f32,
    _pad0: f32,
    _pad1: f32,
    _pad2: f32,
}

@group(0) @binding(0) var<uniform> params: WeatherSnapshot;
@group(0) @binding(1) var wind_tex: texture_cube<f32>;
@group(0) @binding(2) var pressure_tex: texture_cube<f32>;
@group(0) @binding(3) var weather_sampler: sampler;
@group(0) @binding(4) var<storage, read> height_data: array<f32>;
@group(0) @binding(5) var output_tex: texture_storage_2d_array<rgba16float, write>;

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

fn storm_bonus(pos: vec3<f32>, seed: f32) -> f32 {
    var total = 0.0;
    let count = min(params.storm_count, 8u);
    for (var i = 0u; i < count; i++) {
        let n = f32(i) + seed * 0.017;
        let center = normalize(vec3<f32>(sin(n * 2.17), sin(n * 1.31) * 0.45, cos(n * 2.17)));
        let radius = 0.08 * params.storm_size;
        total += exp(-dot(pos - center, pos - center) / max(radius * radius, 0.001));
    }
    return min(total, 1.0);
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
    let wind_dir = normalize(wind.xyz + vec3<f32>(0.0001));
    let upwind_height = sample_height(normalize(pos - wind_dir * 0.025));
    let terrain_lift = smooth_step(0.0, 0.08, height - upwind_height);
    let rain_shadow = smooth_step(0.0, 0.08, upwind_height - height);
    let tilted_y = pos.y * cos(params.axial_tilt_rad) + pos.z * sin(params.axial_tilt_rad);
    let latitude = abs(asin(clamp(tilted_y, -1.0, 1.0))) / 1.5707963;
    let season_shift = sin((params.season - 0.5) * 6.2831853) * sin(params.axial_tilt_rad);
    let thermal = smooth_step(-25.0, 30.0, params.base_temp_c - latitude * 35.0 + season_shift * tilted_y * 16.0);
    let convergence = 1.0 - smooth_step(1010.0, 1030.0, pressure);
    let continentality = wind.a;
    let systems = snoise(pos * 0.9 + weather_seed) * 0.55 + snoise(pos * 2.7 + weather_seed * 0.43) * 0.25 + 0.5;
    let moisture = params.moisture * smooth_step(0.05, 0.3, params.atm_pressure);
    let forcing = moisture * (0.45 + 0.3 * convergence + 0.2 * terrain_lift + 0.15 * thermal - 0.2 * rain_shadow - 0.08 * continentality) + storm_bonus(pos, f32(params.seed));
    let coverage_field = clamp(systems * 0.75 + forcing * 0.45, 0.0, 1.0);
    // ponytail: keep zero coverage an exact early-out; U3 consumes this occupancy before ray marching.
    let occupancy = select(0.0, smooth_step(1.0 - params.coverage, 1.0, coverage_field), params.coverage > 0.0 && moisture > 0.0);
    let base_altitude_km = 0.7 + 1.1 * latitude + 0.7 * terrain_lift + 0.4 * (1.0 - thermal);
    let thickness_km = 0.25 + occupancy * (1.0 + 2.2 * convergence + 0.7 * terrain_lift) * (0.5 + 0.5 * thermal);
    let character = clamp(0.45 + params.cloud_character * 0.3 + snoise(pos * 5.0 + weather_seed) * 0.2 + terrain_lift * 0.2, 0.0, 1.0);
    textureStore(output_tex, vec2<i32>(id.xy), i32(params.face), vec4<f32>(occupancy, base_altitude_km, thickness_km, character));
}
