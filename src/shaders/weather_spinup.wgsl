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
    wind_scale: f32,
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
const PHYSICAL_INTERVAL_SECONDS: f32 = 1600.0;
const MAX_WIND_MPS: f32 = 50.0;
const MAX_SUBSTEP_TEXELS: f32 = 0.60;

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

fn convective_catalyst(pos: vec3<f32>) -> f32 {
    let active_count = min(params.storm_count, 8u);
    let radius = mix(0.055, 0.16, clamp((params.storm_size - 0.3) / 2.7, 0.0, 1.0));
    var response = 0.0;
    for (var index = 0u; index < active_count; index++) {
        let rank = f32(reverseBits(index) >> 29u);
        let z = 1.0 - 2.0 * (rank + 0.5) / 8.0;
        let phase = f32(params.seed & 0xffffu) / 65536.0 * 6.2831853;
        let angle = rank * 2.3999632 + phase;
        let base = vec3<f32>(sqrt(max(1.0 - z * z, 0.0)) * cos(angle), z, sqrt(max(1.0 - z * z, 0.0)) * sin(angle));
        let basis = tangent_basis(base);
        let jitter = (noise_seed_offset(params.seed, 201u + index).xy * 2.0 - 1.0) * 0.12;
        let center = normalize(base + basis[0] * jitter.x + basis[1] * jitter.y);
        let wind = textureSampleLevel(wind_tex, spinup_sampler, center, 0.0).xyz;
        let tangent_wind = wind - center * dot(wind, center);
        let along = normalize(tangent_wind + basis[0] * 0.0001);
        let across = normalize(cross(center, along));
        let delta = pos - center * dot(pos, center);
        let warp_a = snoise(pos * 19.0 + noise_seed_offset(params.seed, 301u));
        let warp_b = snoise(pos * 37.0 + noise_seed_offset(params.seed, 302u));
        let major = radius * 1.45 * (1.0 + warp_a * 0.13);
        let minor = radius * 0.78 * (1.0 + warp_b * 0.13);
        let ellipse = pow(dot(delta, along) / max(major, 0.001), 2.0)
            + pow(dot(delta, across) / max(minor, 0.001), 2.0);
        response = max(response, smooth_step(1.0, 0.62, ellipse));
    }
    return response;
}

fn transport_substeps(resolution: u32) -> f32 {
    let texel_angle = (PI * 0.5) / f32(resolution);
    let displacement = MAX_WIND_MPS * params.wind_scale * PHYSICAL_INTERVAL_SECONDS
        / max(params.radius_km * 1000.0, 1.0);
    return max(ceil(displacement / (texel_angle * MAX_SUBSTEP_TEXELS)), 1.0);
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
    let continentality = textureSampleLevel(wind_tex, spinup_sampler, pos, 0.0).a;
    let marine_fraction = 1.0 - smooth_step(0.15, 0.85, continentality);
    let supply = clamp(params.coverage, 0.0, 1.0) * clamp(params.moisture, 0.0, 1.0)
        * pressure * mix(0.8, 1.25, marine_fraction);
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
    let effective_speed = select(normalized_speed * params.wind_scale, 0.0, normalized_speed * params.wind_scale < 0.01);
    let wind_dir = tangent_wind / max(normalized_speed, 0.0001);
    let texel_angle = (PI * 0.5) / f32(res);
    let wind_mps = effective_speed * 50.0;
    let substep_count = transport_substeps(res);
    let step_fraction = 1.0 / substep_count;
    let angular_step = wind_mps * PHYSICAL_INTERVAL_SECONDS * step_fraction
        / max(params.radius_km * 1000.0, 1.0);
    // Cube-face bilinear sampling shortens the measured geodesic trace slightly.
    let trace_step = angular_step * 1.09;
    let midpoint = normalize(pos - wind_dir * trace_step * 0.5);
    let midpoint_wind = textureSampleLevel(wind_tex, spinup_sampler, midpoint, 0.0).xyz;
    let midpoint_tangent = midpoint_wind - midpoint * dot(midpoint_wind, midpoint);
    let midpoint_dir = normalize(midpoint_tangent + wind_dir * 0.0001);
    let backtrace = normalize(pos - midpoint_dir * trace_step);
    let local_state = textureSampleLevel(state_in, spinup_sampler, pos, 0.0);
    let back_state = textureSampleLevel(state_in, spinup_sampler, backtrace, 0.0);
    let forward_state = textureSampleLevel(
        state_in,
        spinup_sampler,
        normalize(pos + midpoint_dir * trace_step),
        0.0,
    );
    // A bounded local/back/forward MacCormack approximation recovers some
    // semi-Lagrangian loss without requiring another state texture.
    let state_min = min(local_state, min(back_state, forward_state));
    let state_max = max(local_state, max(back_state, forward_state));
    let corrected_state = clamp(
        back_state + (local_state - forward_state) * 0.5,
        state_min,
        state_max,
    );
    // Compensate the known cubemap interpolation volume loss by the bounded
    // angular step; the extrema clamp still prevents new local overshoots.
    var state = min(corrected_state * (1.0 + angular_step * 0.80), state_max);

    let basis = tangent_basis(pos);
    let diagnostic_step = max(texel_angle * 1.5, 0.01);
    let east = basis[0];
    let north = basis[1];
    let east_wind = textureSampleLevel(wind_tex, spinup_sampler, normalize(pos + east * diagnostic_step), 0.0).xyz * params.wind_scale;
    let west_wind = textureSampleLevel(wind_tex, spinup_sampler, normalize(pos - east * diagnostic_step), 0.0).xyz * params.wind_scale;
    let north_wind = textureSampleLevel(wind_tex, spinup_sampler, normalize(pos + north * diagnostic_step), 0.0).xyz * params.wind_scale;
    let south_wind = textureSampleLevel(wind_tex, spinup_sampler, normalize(pos - north * diagnostic_step), 0.0).xyz * params.wind_scale;
    let divergence = (dot(east_wind - west_wind, east) + dot(north_wind - south_wind, north)) / (2.0 * diagnostic_step);
    let convergence = smooth_step(0.01, 0.3, -divergence * 0.2);

    let upwind_height = sample_height(normalize(pos - wind_dir * diagnostic_step * 1.5));
    let downwind_height = sample_height(normalize(pos + wind_dir * diagnostic_step * 1.5));
    let terrain_response = (downwind_height - upwind_height)
        * smooth_step(0.03, 0.2, effective_speed);
    let terrain_lift = smooth_step(0.005, 0.08, terrain_response);
    let rain_shadow = smooth_step(0.005, 0.08, -terrain_response);
    let lift = clamp(convergence * 0.7 + terrain_lift * 0.8 - rain_shadow * 0.1, 0.0, 1.0);
    let thermal = smooth_step(-25.0, 30.0, temperature_at(pos));
    let pressure_factor = smooth_step(0.05, 0.3, params.surface_pressure_bar);
    let local_pressure = clamp(textureSampleLevel(pressure_tex, spinup_sampler, pos, 0.0).r / 1013.0, 0.8, 1.2);
    let marine_fraction = 1.0 - smooth_step(0.15, 0.85, wind.a);
    if ((params.diagnostic_flags & DIAGNOSTIC_NO_SOURCE) == 0u) {
        let target_vapor = clamp(params.coverage, 0.0, 1.0) * clamp(params.moisture, 0.0, 1.0)
            * pressure_factor * mix(0.18, 0.36, marine_fraction);
        state.x += max(target_vapor - state.x, 0.008) * mix(0.006, 0.03, marine_fraction) * step_fraction;
    }

    if ((params.diagnostic_flags & DIAGNOSTIC_NO_PHASE_CHANGE) == 0u) {
        let marine_trade = marine_fraction * thermal;
        let saturation = mix(0.16, 0.68, thermal) * clamp(params.coverage, 0.0, 1.0)
            * pressure_factor * local_pressure * mix(1.0, 0.45, marine_trade);
        let catalyst = convective_catalyst(pos);
        // Moist, warm air is required everywhere; resolved convergence increases,
        // but does not manufacture, the catalyst response.
        let convective_eligibility = smooth_step(0.08, 0.55, state.x) * thermal
            * mix(0.20, 1.0, smooth_step(0.004, 0.20, lift));
        let catalytic_response = catalyst * convective_eligibility;
        let condensation = min(state.x, (max(state.x - saturation, 0.0) * 0.22
            + state.x * lift * 0.055) * step_fraction);
        let deep_fraction = clamp(lift * thermal * 0.72, 0.0, 0.75);
        state.x -= condensation;
        state.y += condensation * (1.0 - deep_fraction);
        state.z += condensation * deep_fraction;

        // Catalysts redistribute existing condensate only in physically eligible air.
        let promoted = min(state.y, 0.18 * catalytic_response * step_fraction);
        state.y -= promoted;
        state.z += promoted;

        let evaporation = min(state.y, max(saturation - state.x, 0.0) * 0.012 * step_fraction);
        state.x += evaporation;
        state.y -= evaporation;
        // Detrain only this pass's baseline deep production and catalyst promotion.
        let new_deep = condensation * deep_fraction + promoted;
        let detrainment = min(
            state.z,
            new_deep * mix(0.03, 0.18, thermal) * (lift + catalytic_response) * step_fraction,
        );
        state.z -= detrainment;
        state.w += detrainment;
    }

    if ((params.diagnostic_flags & DIAGNOSTIC_NO_SINK) == 0u) {
        let rainout = (max(state.y + state.z - 0.4, 0.0) * 0.1 + state.z * 0.013) * step_fraction;
        let rainout_scale = min(rainout / max(state.y + state.z, 0.0001), 1.0);
        state.y *= 1.0 - rainout_scale;
        state.z *= 1.0 - rainout_scale;
    }
    if ((params.diagnostic_flags & DIAGNOSTIC_NO_RELAXATION) == 0u) {
        state.w *= 1.0 - 0.005 * step_fraction;
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
    let high = clamp(state.w, 0.0, 1.0);
    let occupancy = max(low, max(deep, high));
    textureStore(
        mass_out,
        vec2<i32>(id.xy),
        i32(id.z),
        vec4<f32>(low, deep, high, occupancy),
    );
}
