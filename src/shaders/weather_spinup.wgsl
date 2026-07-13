struct SpinupParams {
    spin_resolution: u32,
    output_resolution: u32,
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
    diagnostic_flags: u32,
    _pad0: u32,
}

@group(0) @binding(0) var<uniform> params: SpinupParams;
@group(0) @binding(1) var wind_tex: texture_cube<f32>;
@group(0) @binding(2) var pressure_tex: texture_cube<f32>;
@group(0) @binding(3) var spinup_sampler: sampler;
@group(0) @binding(4) var<storage, read> height_data: array<f32>;
@group(0) @binding(5) var baseline_mass: texture_cube<f32>;
@group(0) @binding(6) var state_in: texture_cube<f32>;
@group(0) @binding(7) var state_out: texture_storage_2d_array<rgba16float, write>;
@group(0) @binding(8) var mass_out: texture_storage_2d_array<rgba16float, write>;

const PI: f32 = 3.14159265;
const DIAGNOSTIC_NO_SOURCE: u32 = 1u;
const DIAGNOSTIC_NO_SINK: u32 = 2u;
const DIAGNOSTIC_NO_PHASE_CHANGE: u32 = 4u;
const DIAGNOSTIC_NO_RELAXATION: u32 = 8u;

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
    let res = params.output_resolution;
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
    let latitude = abs(asin(clamp(tilted_y, -1.0, 1.0))) / (PI * 0.5);
    let season_shift = (params.season - 0.5) * 2.0 * sin(params.axial_tilt_rad);
    let elevation_km = max(sample_height(pos) - params.ocean_level, 0.0) * 5.0;
    let continentality = textureSampleLevel(wind_tex, spinup_sampler, pos, 0.0).a;
    return params.base_temp_c - latitude * 35.0 + season_shift * tilted_y * 16.0
        - elevation_km * 6.5 + continentality * season_shift * 5.0;
}

fn direction(id: vec3<u32>, resolution: u32) -> vec3<f32> {
    let uv = vec2<f32>(id.xy) / f32(resolution - 1u);
    return cube_to_sphere(id.z, uv);
}

@compute @workgroup_size(8, 8, 1)
fn init(@builtin(global_invocation_id) id: vec3<u32>) {
    let res = params.spin_resolution;
    if (id.x >= res || id.y >= res || id.z >= 6u) { return; }
    if (params.coverage <= 0.0 || params.moisture <= 0.0) {
        textureStore(state_out, vec2<i32>(id.xy), i32(id.z), vec4<f32>(0.0));
        return;
    }

    let pos = direction(id, res);
    let baseline = textureSampleLevel(baseline_mass, spinup_sampler, pos, 0.0);
    let pressure = smooth_step(0.05, 0.3, params.surface_pressure_bar);
    let supply = clamp(params.coverage, 0.0, 1.0) * clamp(params.moisture, 0.0, 1.0) * pressure;
    let vapor = supply * (0.12 + baseline.a * 0.5);
    let tilted_y = pos.y * cos(params.axial_tilt_rad) + pos.z * sin(params.axial_tilt_rad);
    let latitude = abs(asin(clamp(tilted_y, -1.0, 1.0))) / (PI * 0.5);
    let cold_air_capacity = mix(
        1.0,
        mix(1.0, 0.4, smooth_step(0.45, 0.9, latitude)),
        smooth_step(0.01, 0.05, baseline.a),
    );
    textureStore(
        state_out,
        vec2<i32>(id.xy),
        i32(id.z),
        vec4<f32>(vapor, baseline.r * cold_air_capacity, baseline.g * cold_air_capacity, baseline.b * 0.75 * cold_air_capacity),
    );
}

@compute @workgroup_size(8, 8, 1)
fn transport(@builtin(global_invocation_id) id: vec3<u32>) {
    let res = params.spin_resolution;
    if (id.x >= res || id.y >= res || id.z >= 6u) { return; }
    if (params.coverage <= 0.0 || params.moisture <= 0.0) {
        textureStore(state_out, vec2<i32>(id.xy), i32(id.z), vec4<f32>(0.0));
        return;
    }

    let pos = direction(id, res);
    let wind = textureSampleLevel(wind_tex, spinup_sampler, pos, 0.0);
    let tangent_wind = wind.xyz - pos * dot(wind.xyz, pos);
    let normalized_speed = length(tangent_wind);
    let wind_dir = tangent_wind / max(normalized_speed, 0.0001);
    let texel_angle = (PI * 0.5) / f32(res);
    let wind_mps = normalized_speed * 50.0;
    let angular_step = min(wind_mps * 1800.0 / max(params.radius_km * 1000.0, 1.0), texel_angle * 0.6);
    let backtrace = normalize(pos - wind_dir * angular_step);
    var state = textureSampleLevel(state_in, spinup_sampler, backtrace, 0.0);

    let basis = tangent_basis(pos);
    let diagnostic_step = max(texel_angle * 1.5, 0.01);
    let east = basis[0];
    let north = basis[1];
    let east_wind = textureSampleLevel(wind_tex, spinup_sampler, normalize(pos + east * diagnostic_step), 0.0).xyz;
    let west_wind = textureSampleLevel(wind_tex, spinup_sampler, normalize(pos - east * diagnostic_step), 0.0).xyz;
    let north_wind = textureSampleLevel(wind_tex, spinup_sampler, normalize(pos + north * diagnostic_step), 0.0).xyz;
    let south_wind = textureSampleLevel(wind_tex, spinup_sampler, normalize(pos - north * diagnostic_step), 0.0).xyz;
    let divergence = (dot(east_wind - west_wind, east) + dot(north_wind - south_wind, north)) / (2.0 * diagnostic_step);
    let convergence = smooth_step(0.01, 0.3, -divergence * 0.2);

    let upwind_height = sample_height(normalize(pos - wind_dir * diagnostic_step * 1.5));
    let downwind_height = sample_height(normalize(pos + wind_dir * diagnostic_step * 1.5));
    let terrain_lift = smooth_step(0.005, 0.08, downwind_height - upwind_height)
        * smooth_step(0.03, 0.2, normalized_speed);
    let lift = clamp(convergence * 0.7 + terrain_lift * 0.8, 0.0, 1.0);
    let thermal = smooth_step(-25.0, 30.0, temperature_at(pos));
    let pressure_factor = smooth_step(0.05, 0.3, params.surface_pressure_bar);
    let local_pressure = clamp(textureSampleLevel(pressure_tex, spinup_sampler, pos, 0.0).r / 1013.0, 0.8, 1.2);
    if ((params.diagnostic_flags & DIAGNOSTIC_NO_SOURCE) == 0u) {
        let target_vapor = clamp(params.coverage, 0.0, 1.0) * clamp(params.moisture, 0.0, 1.0)
            * pressure_factor * mix(0.22, 0.12, wind.a);
        state.x += max(target_vapor - state.x, 0.0) * mix(0.025, 0.006, wind.a);
    }

    if ((params.diagnostic_flags & DIAGNOSTIC_NO_PHASE_CHANGE) == 0u) {
        let saturation = mix(0.16, 0.68, thermal) * clamp(params.coverage, 0.0, 1.0) * pressure_factor * local_pressure;
        let condensation = min(state.x, max(state.x - saturation, 0.0) * 0.22 + state.x * lift * 0.055);
        let deep_fraction = clamp(lift * thermal * 0.72, 0.0, 0.75);
        state.x -= condensation;
        state.y += condensation * (1.0 - deep_fraction);
        state.z += condensation * deep_fraction;

        let evaporation = min(state.y, max(saturation - state.x, 0.0) * 0.012);
        state.x += evaporation;
        state.y -= evaporation;
        let detrainment = state.z * mix(0.012, 0.04, 1.0 - thermal) * lift;
        state.z -= detrainment;
        state.w += detrainment;
    }

    if ((params.diagnostic_flags & DIAGNOSTIC_NO_SINK) == 0u) {
        let rainout = max(state.y + state.z - 0.4, 0.0) * 0.1 + state.z * 0.013;
        let rainout_scale = min(rainout / max(state.y + state.z, 0.0001), 1.0);
        state.y *= 1.0 - rainout_scale;
        state.z *= 1.0 - rainout_scale;
    }
    if ((params.diagnostic_flags & DIAGNOSTIC_NO_RELAXATION) == 0u) {
        state.w *= 0.995;
    }
    textureStore(state_out, vec2<i32>(id.xy), i32(id.z), clamp(state, vec4<f32>(0.0), vec4<f32>(1.0)));
}

@compute @workgroup_size(8, 8, 1)
fn finalize(@builtin(global_invocation_id) id: vec3<u32>) {
    let res = params.output_resolution;
    if (id.x >= res || id.y >= res || id.z >= 6u) { return; }
    let pos = direction(id, res);
    let state = textureSampleLevel(state_in, spinup_sampler, pos, 0.0);
    let low = clamp(state.y * 1.25, 0.0, 1.0);
    let deep = clamp(state.z * 1.5, 0.0, 1.0 - low);
    let occupancy = low + deep;
    let high = min(clamp(state.w, 0.0, 1.0), occupancy);
    textureStore(
        mass_out,
        vec2<i32>(id.xy),
        i32(id.z),
        vec4<f32>(low, deep, high, occupancy),
    );
}
