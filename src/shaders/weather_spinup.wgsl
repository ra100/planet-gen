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
@group(0) @binding(6) var state_in: texture_2d_array<f32>;
@group(0) @binding(7) var state_out: texture_storage_2d_array<rgba16float, write>;
@group(0) @binding(8) var mass_out: texture_storage_2d_array<rgba16float, write>;

const PI: f32 = 3.14159265;
const DIAGNOSTIC_NO_SOURCE: u32 = 1u;
const DIAGNOSTIC_NO_SINK: u32 = 2u;
const DIAGNOSTIC_NO_PHASE_CHANGE: u32 = 4u;
const DIAGNOSTIC_NO_RELAXATION: u32 = 8u;
const PHYSICAL_INTERVAL_SECONDS: f32 = 1600.0;
const MAX_WIND_MPS: f32 = 50.0;
const MAX_SUBSTEP_TEXELS: f32 = 0.85;
const CATALYST_TARGET_SHARE_ALPHA: f32 = 0.70;
const CATALYST_TARGET_SHARE_MAX: f32 = 0.92;
const CATALYST_TARGET_TRANSFER_K: f32 = 2.0;
const CATALYST_TARGET_ORGANIZING_ELIGIBILITY: f32 = 0.025;
const STORM_RECHARGE_LAND: f32 = 0.06;
const STORM_RECHARGE_MARINE: f32 = 0.18;
const STORM_RECHARGE_HARD_CAP: f32 = 0.24;

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

fn sample_state(dir: vec3<f32>) -> vec4<f32> {
    let fuv = sphere_to_face_uv(dir);
    let res = i32(params.spin_resolution - 1u);
    let texel = vec2<i32>(
        clamp(i32(round(fuv.y * f32(res))), 0, res),
        clamp(i32(round(fuv.z * f32(res))), 0, res),
    );
    return textureLoad(state_in, texel, i32(fuv.x), 0);
}

fn face_angle(a: vec3<f32>, b: vec3<f32>) -> f32 {
    return acos(clamp(dot(a, b), -1.0, 1.0));
}

fn min_face_angle(resolution: u32) -> f32 {
    let step = 2.0 / f32(resolution - 1u);
    let neighbor_length = sqrt(3.0 - 2.0 * step + step * step);
    return acos(clamp((3.0 - step) / (sqrt(3.0) * neighbor_length), -1.0, 1.0));
}

fn tangent_basis(pos: vec3<f32>) -> mat2x3<f32> {
    let reference = select(vec3<f32>(0.0, 1.0, 0.0), vec3<f32>(1.0, 0.0, 0.0), abs(pos.y) > 0.9);
    let east = normalize(cross(reference, pos));
    return mat2x3<f32>(east, normalize(cross(pos, east)));
}

fn catalyst_center(index: u32) -> vec3<f32> {
    let rank = f32(reverseBits(index) >> 29u);
    let z = 1.0 - 2.0 * (rank + 0.5) / 8.0;
    let phase = f32(params.seed & 0xffffu) / 65536.0 * 6.2831853;
    let angle = rank * 2.3999632 + phase;
    let base = vec3<f32>(sqrt(max(1.0 - z * z, 0.0)) * cos(angle), z, sqrt(max(1.0 - z * z, 0.0)) * sin(angle));
    let basis = tangent_basis(base);
    let jitter = (noise_seed_offset(params.seed, 201u + index).xy * 2.0 - 1.0) * 0.12;
    return normalize(base + basis[0] * jitter.x + basis[1] * jitter.y);
}

fn convective_catalyst(pos: vec3<f32>) -> f32 {
    let active_count = min(params.storm_count, 8u);
    let size_t = clamp((params.storm_size - 0.3) / 2.7, 0.0, 1.0);
    let radius = 0.085 + (0.20 - 0.085) * pow(size_t, 2.0135171);
    var response = 0.0;
    for (var index = 0u; index < active_count; index++) {
        let center = catalyst_center(index);
        let basis = tangent_basis(center);
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
        response = max(response, smooth_step(1.0, 0.72, ellipse));
    }
    return response;
}

fn transport_substeps(resolution: u32) -> f32 {
    let displacement = 2.0 * MAX_WIND_MPS * clamp(params.wind_scale, 0.0, 2.0) * PHYSICAL_INTERVAL_SECONDS
        / max(params.radius_km * 1000.0, 1.0);
    return max(ceil(displacement / (min_face_angle(resolution) * MAX_SUBSTEP_TEXELS)), 1.0);
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

fn source_potential(
    marine_fraction: f32,
    convergence: f32,
    terrain_lift: f32,
    rain_shadow: f32,
    thermal_stability: f32,
) -> f32 {
    let surface_supply = mix(0.50, 0.60, marine_fraction);
    return clamp(
        surface_supply + convergence * 0.25 + terrain_lift * 10.0 - rain_shadow * 1.0
            + thermal_stability * 0.04,
        0.0,
        1.0,
    );
}

struct SourceBudgets {
    supply: f32,
    phase: f32,
    source_envelope: f32,
}

// Coverage controls source supply and vapor phase conversion independently.
fn source_budgets(
    coverage: f32,
    marine_fraction: f32,
    convergence: f32,
    terrain_lift: f32,
    rain_shadow: f32,
    thermal_stability: f32,
    persistent_ice: f32,
) -> SourceBudgets {
    let local_potential = source_potential(
        marine_fraction,
        convergence,
        terrain_lift,
        rain_shadow,
        thermal_stability,
    );
    let c = clamp(coverage, 0.0, 1.0);
    let source_envelope = smoothstep(0.0, 0.25, c);
    let surface_supply = mix(0.50, 0.60, marine_fraction);
    let thermal_regime = mix(0.14, 0.04, thermal_stability);
    let phase_rank = clamp(
        surface_supply + 0.25 * convergence + thermal_regime - 0.15 * persistent_ice,
        0.0,
        1.0,
    );
    let center = 0.88 - 0.32 * c;
    let logit = clamp((phase_rank - center) / 0.02, -8.0, 8.0);
    let eligibility = 1.0 / (1.0 + exp(-logit));
    let ranked_phase = 1.0 - exp(-8.0 * c * pow(phase_rank, 4.0));
    let phase = eligibility * 8.0 * ranked_phase / (1.0 + 7.0 * ranked_phase);
    return SourceBudgets(
        local_potential * source_envelope,
        phase,
        source_envelope,
    );
}

fn direction(id: vec3<u32>, resolution: u32) -> vec3<f32> {
    let uv = vec2<f32>(id.xy) / f32(resolution - 1u);
    return cube_to_sphere(id.z, uv);
}

fn mc_slope(left: vec4<f32>, center: vec4<f32>, right: vec4<f32>) -> vec4<f32> {
    let backward = center - left;
    let forward = right - center;
    return 0.5 * (sign(backward) + sign(forward))
        * min(0.5 * abs(backward + forward), min(2.0 * abs(backward), 2.0 * abs(forward)));
}

struct Reconstruction {
    center: vec4<f32>, west: vec4<f32>, east: vec4<f32>, south: vec4<f32>, north: vec4<f32>,
}

fn cell_area(pos: vec3<f32>) -> f32 {
    let cube = sphere_to_face_uv(pos).yz * 2.0 - 1.0;
    return pow(1.0 + dot(cube, cube), -1.5);
}

fn reconstruct(center: vec3<f32>) -> Reconstruction {
    let fuv = sphere_to_face_uv(center);
    let face = u32(fuv.x);
    let uv = fuv.yz;
    let step = 1.0 / f32(params.spin_resolution - 1u);
    let state = sample_state(center);
    let west = sample_state(cube_to_sphere(face, uv - vec2<f32>(step, 0.0)));
    let east = sample_state(cube_to_sphere(face, uv + vec2<f32>(step, 0.0)));
    let south = sample_state(cube_to_sphere(face, uv - vec2<f32>(0.0, step)));
    let north = sample_state(cube_to_sphere(face, uv + vec2<f32>(0.0, step)));
    let slope_s = mc_slope(west, state, east);
    let slope_t = mc_slope(south, state, north);
    let raw_west = state - 0.5 * slope_s;
    let raw_east = state + 0.5 * slope_s;
    let raw_south = state - 0.5 * slope_t;
    let raw_north = state + 0.5 * slope_t;
    let face_min = min(min(raw_west, raw_east), min(raw_south, raw_north));
    let theta_components = select(vec4<f32>(1.0), state / max(state - face_min, vec4<f32>(0.000001)), face_min < vec4<f32>(0.0));
    let theta = clamp(theta_components, vec4<f32>(0.0), vec4<f32>(1.0));
    return Reconstruction(
        state,
        state + theta * (raw_west - state),
        state + theta * (raw_east - state),
        state + theta * (raw_south - state),
        state + theta * (raw_north - state),
    );
}

fn velocity(pos: vec3<f32>, normal: vec3<f32>) -> f32 {
    let wind = textureSampleLevel(wind_tex, spinup_sampler, pos, 0.0).xyz * params.wind_scale;
    return dot(wind - pos * dot(wind, pos), normal) * MAX_WIND_MPS;
}

fn hancock(center: vec3<f32>) -> Reconstruction {
    let fuv = sphere_to_face_uv(center);
    let face = u32(fuv.x);
    let uv = fuv.yz;
    let step = 1.0 / f32(params.spin_resolution - 1u);
    let east_pos = cube_to_sphere(face, uv + vec2<f32>(step, 0.0));
    let west_pos = cube_to_sphere(face, uv - vec2<f32>(step, 0.0));
    let north_pos = cube_to_sphere(face, uv + vec2<f32>(0.0, step));
    let south_pos = cube_to_sphere(face, uv - vec2<f32>(0.0, step));
    let east_normal = normalize(east_pos - center * dot(east_pos, center));
    let north_normal = normalize(north_pos - center * dot(north_pos, center));
    let reconstructed = reconstruct(center);
    let param_step = 2.0 * step;
    let area = cell_area(center);
    let east_flux = velocity(normalize(center + east_pos), east_normal) * reconstructed.east;
    let west_flux = velocity(normalize(center + west_pos), east_normal) * reconstructed.west;
    let north_flux = velocity(normalize(center + north_pos), north_normal) * reconstructed.north;
    let south_flux = velocity(normalize(center + south_pos), north_normal) * reconstructed.south;
    let divergence = ((east_flux * 0.5 * (area + cell_area(east_pos)) / max(face_angle(center, east_pos) / param_step, 0.0001)
        - west_flux * 0.5 * (area + cell_area(west_pos)) / max(face_angle(center, west_pos) / param_step, 0.0001))
        + (north_flux * 0.5 * (area + cell_area(north_pos)) / max(face_angle(center, north_pos) / param_step, 0.0001)
        - south_flux * 0.5 * (area + cell_area(south_pos)) / max(face_angle(center, south_pos) / param_step, 0.0001))) / max(area * param_step, 0.0001);
    let half = reconstructed.center - 0.5 * PHYSICAL_INTERVAL_SECONDS / max(params.radius_km * 1000.0, 1.0) / transport_substeps(params.spin_resolution) * divergence;
    return Reconstruction(half, half + reconstructed.west - reconstructed.center, half + reconstructed.east - reconstructed.center, half + reconstructed.south - reconstructed.center, half + reconstructed.north - reconstructed.center);
}

fn state_toward(center: vec3<f32>, toward: vec3<f32>) -> vec4<f32> {
    let reconstructed = hancock(center);
    let fuv = sphere_to_face_uv(center);
    let face = u32(fuv.x);
    let uv = fuv.yz;
    let step = 1.0 / f32(params.spin_resolution - 1u);
    let s_pos = cube_to_sphere(face, uv + vec2<f32>(step, 0.0));
    let t_pos = cube_to_sphere(face, uv + vec2<f32>(0.0, step));
    let s = normalize(s_pos - center * dot(s_pos, center));
    let t = normalize(t_pos - center * dot(t_pos, center));
    if (abs(dot(toward, s)) >= abs(dot(toward, t))) {
        return select(reconstructed.west, reconstructed.east, dot(toward, s) > 0.0);
    }
    return select(reconstructed.south, reconstructed.north, dot(toward, t) > 0.0);
}

fn a_is_minus(a: vec3<f32>, b: vec3<f32>) -> bool {
    let a_face_uv = sphere_to_face_uv(a);
    let b_face_uv = sphere_to_face_uv(b);
    if (a_face_uv.x != b_face_uv.x) { return a_face_uv.x < b_face_uv.x; }
    if (a_face_uv.y != b_face_uv.y) { return a_face_uv.y < b_face_uv.y; }
    return a_face_uv.z < b_face_uv.z;
}

fn shared_flux(a: vec3<f32>, a_state: vec4<f32>, b: vec3<f32>, b_state: vec4<f32>) -> vec4<f32> {
    let midpoint = normalize(a + b);
    let a_is_canonical_minus = a_is_minus(a, b);
    let minus = select(b, a, a_is_canonical_minus);
    let plus = select(a, b, a_is_canonical_minus);
    let minus_state = select(b_state, a_state, a_is_canonical_minus);
    let plus_state = select(a_state, b_state, a_is_canonical_minus);
    let normal = normalize(plus - midpoint * dot(plus, midpoint));
    let minus_velocity = velocity(minus, normal);
    let plus_velocity = velocity(plus, normal);
    let same_sign = minus_velocity * plus_velocity >= 0.0;
    let average_velocity = 0.5 * (minus_velocity + plus_velocity);
    let upwind = select(plus_state, minus_state, average_velocity >= 0.0);
    let rusanov = 0.5 * (minus_velocity * minus_state + plus_velocity * plus_state - max(abs(minus_velocity), abs(plus_velocity)) * (plus_state - minus_state));
    return select(-select(rusanov, average_velocity * upwind, same_sign), select(rusanov, average_velocity * upwind, same_sign), a_is_canonical_minus);
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
    let pressure = smooth_step(0.05, 0.3, params.surface_pressure_bar);
    let continentality = textureSampleLevel(wind_tex, spinup_sampler, pos, 0.0).a;
    let marine_fraction = 1.0 - smooth_step(0.15, 0.85, continentality);
    let thermal_stability = smooth_step(-25.0, 30.0, temperature_at(pos));
    let persistent_ice = marine_fraction * (1.0 - smoothstep(-15.0, -6.0, temperature_at(pos)));
    let budgets = source_budgets(
        params.coverage,
        marine_fraction,
        0.0,
        0.0,
        0.0,
        thermal_stability,
        persistent_ice,
    );
    let surface_supply_factor = 1.0 - 0.25 * persistent_ice;
    let vapor = select(
        budgets.supply * surface_supply_factor * clamp(params.moisture, 0.0, 1.0)
            * pressure * mix(0.18, 0.36, marine_fraction),
        0.0,
        (params.diagnostic_flags & DIAGNOSTIC_NO_SOURCE) != 0u,
    );
    textureStore(
        state_out,
        vec2<i32>(id.xy),
        i32(id.z),
        vec4<f32>(vapor, 0.0, 0.0, 0.0),
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
    let forcing_scale = pow(max(params.wind_scale, 0.0), 0.3);
    let effective_speed = select(normalized_speed * forcing_scale, 0.0, normalized_speed * forcing_scale < 0.01);
    let wind_dir = tangent_wind / max(normalized_speed, 0.0001);
    let substep_count = transport_substeps(res);
    let step_fraction = 1.0 / substep_count;
    let texel_angle = (PI * 0.5) / f32(res);
    let fuv = sphere_to_face_uv(pos);
    let face = u32(fuv.x);
    let uv = fuv.yz;
    let grid_step = 1.0 / f32(res - 1u);
    let metric_step = 2.0 * grid_step;
    let east_pos = cube_to_sphere(face, uv + vec2<f32>(grid_step, 0.0));
    let west_pos = cube_to_sphere(face, uv - vec2<f32>(grid_step, 0.0));
    let north_pos = cube_to_sphere(face, uv + vec2<f32>(0.0, grid_step));
    let south_pos = cube_to_sphere(face, uv - vec2<f32>(0.0, grid_step));
    let predicted = hancock(pos);
    let east_flux = shared_flux(pos, predicted.east, east_pos, state_toward(east_pos, pos));
    let west_flux = shared_flux(west_pos, state_toward(west_pos, pos), pos, predicted.west);
    let north_flux = shared_flux(pos, predicted.north, north_pos, state_toward(north_pos, pos));
    let south_flux = shared_flux(south_pos, state_toward(south_pos, pos), pos, predicted.south);
    let transport_divergence = (east_flux * face_angle(pos, east_pos)
        - west_flux * face_angle(west_pos, pos)
        + north_flux * face_angle(pos, north_pos)
        - south_flux * face_angle(south_pos, pos))
        / max(cell_area(pos) * metric_step * metric_step, 0.000001);
    var state = predicted.center - PHYSICAL_INTERVAL_SECONDS * step_fraction
        / max(params.radius_km * 1000.0, 1.0) * transport_divergence;

    let diagnostic_step = max(texel_angle * 1.5, 0.01);
    let basis = tangent_basis(pos);
    let east = basis[0];
    let north = basis[1];
    let divergence_east_wind = textureSampleLevel(wind_tex, spinup_sampler, normalize(pos + east * diagnostic_step), 0.0).xyz * forcing_scale;
    let divergence_west_wind = textureSampleLevel(wind_tex, spinup_sampler, normalize(pos - east * diagnostic_step), 0.0).xyz * forcing_scale;
    let divergence_north_wind = textureSampleLevel(wind_tex, spinup_sampler, normalize(pos + north * diagnostic_step), 0.0).xyz * forcing_scale;
    let divergence_south_wind = textureSampleLevel(wind_tex, spinup_sampler, normalize(pos - north * diagnostic_step), 0.0).xyz * forcing_scale;
    let divergence = (dot(divergence_east_wind - divergence_west_wind, east) + dot(divergence_north_wind - divergence_south_wind, north)) / (2.0 * diagnostic_step);
    let convergence = smooth_step(0.01, 0.3, -divergence * 0.2);

    let terrain_lookahead = 1.5 * clamp(300.0 / max(params.radius_km, 1.0), 0.02, 0.08);
    let upwind_height = sample_height(normalize(pos - wind_dir * terrain_lookahead));
    let downwind_height = sample_height(normalize(pos + wind_dir * terrain_lookahead));
    let terrain_response = (downwind_height - upwind_height)
        * smooth_step(0.03, 0.2, effective_speed);
    let terrain_lift = smooth_step(0.005, 0.08, terrain_response);
    let rain_shadow = smooth_step(0.005, 0.08, -terrain_response);
    let thermal = smooth_step(-25.0, 30.0, temperature_at(pos));
    let pressure_factor = smooth_step(0.05, 0.3, params.surface_pressure_bar);
    let local_pressure = clamp(textureSampleLevel(pressure_tex, spinup_sampler, pos, 0.0).r / 1013.0, 0.8, 1.2);
    let marine_fraction = 1.0 - smooth_step(0.15, 0.85, wind.a);
    let cold = 1.0 - thermal;
    let q_sat = mix(0.16, 0.68, thermal) * pressure_factor * local_pressure;
    let persistent_ice = marine_fraction * (1.0 - smoothstep(-15.0, -6.0, temperature_at(pos)));
    let budgets = source_budgets(
        params.coverage,
        marine_fraction,
        convergence,
        terrain_lift,
        rain_shadow,
        thermal,
        persistent_ice,
    );
    let surface_supply_factor = 1.0 - 0.25 * persistent_ice;
    let supply_budget = budgets.supply * surface_supply_factor;
    let phase_budget = budgets.phase;
    let relative_humidity_target = clamp(
        mix(0.45, 0.72, marine_fraction) + marine_fraction * cold * 0.18,
        0.0,
        0.92,
    );
    let q_target = q_sat * supply_budget * clamp(params.moisture, 0.0, 1.0)
        * relative_humidity_target;
    if ((params.diagnostic_flags & DIAGNOSTIC_NO_SOURCE) == 0u) {
        let recharge = max(q_target - state.x, 0.0)
            * mix(0.006, 0.030, marine_fraction) * step_fraction;
        state.x += recharge;
    }

    let storm_catalyst = convective_catalyst(pos);
    if ((params.diagnostic_flags & DIAGNOSTIC_NO_SOURCE) == 0u
        && storm_catalyst > 0.0
        && budgets.source_envelope > 0.0
        && params.moisture > 0.0) {
        let storm_lcl_lift = clamp(
            convergence * 0.70 + terrain_lift * 1.75
                + marine_fraction * (0.04 + cold * 0.16) - rain_shadow * 0.14,
            0.0,
            1.0,
        );
        let storm_warm_gate = thermal * smooth_step(0.10, 0.20, storm_lcl_lift);
        let storm_humidity = smooth_step(0.45, 0.95, state.x / max(q_sat, 0.0001));
        let storm_physical_eligibility = storm_warm_gate
            * smooth_step(0.12, 0.75, storm_lcl_lift) * storm_humidity;
        if (storm_physical_eligibility > 0.0) {
            let catalyst_activation = storm_catalyst * budgets.source_envelope;
            let organizing = smooth_step(
                0.0,
                CATALYST_TARGET_ORGANIZING_ELIGIBILITY,
                storm_physical_eligibility,
            );
            let storm_gate = catalyst_activation * organizing * phase_budget;
            let recharge_fraction = clamp(
                mix(STORM_RECHARGE_LAND, STORM_RECHARGE_MARINE, marine_fraction)
                    * storm_gate * step_fraction,
                0.0,
                STORM_RECHARGE_HARD_CAP,
            );
            state.x += max(q_target - state.x, 0.0) * recharge_fraction;
        }
    }

    if ((params.diagnostic_flags & DIAGNOSTIC_NO_PHASE_CHANGE) == 0u) {
        let catalyst = storm_catalyst;
        let pressure_east = textureSampleLevel(pressure_tex, spinup_sampler, normalize(pos + east * diagnostic_step), 0.0).r;
        let pressure_west = textureSampleLevel(pressure_tex, spinup_sampler, normalize(pos - east * diagnostic_step), 0.0).r;
        let pressure_north = textureSampleLevel(pressure_tex, spinup_sampler, normalize(pos + north * diagnostic_step), 0.0).r;
        let pressure_south = textureSampleLevel(pressure_tex, spinup_sampler, normalize(pos - north * diagnostic_step), 0.0).r;
        let pressure_delta = vec2<f32>(pressure_east - pressure_west, pressure_north - pressure_south);
        let temperature_delta = vec2<f32>(
            temperature_at(normalize(pos + east * diagnostic_step)) - temperature_at(normalize(pos - east * diagnostic_step)),
            temperature_at(normalize(pos + north * diagnostic_step)) - temperature_at(normalize(pos - north * diagnostic_step)),
        );
        let pressure_gradient = length(pressure_delta);
        let temperature_gradient = length(temperature_delta);
        let frontal_alignment = dot(pressure_delta, temperature_delta)
            / max(pressure_gradient * temperature_gradient, 0.0001);
        let frontal_lift = smooth_step(2.0, 12.0, pressure_gradient)
            * smooth_step(1.0, 12.0, temperature_gradient)
            * smooth_step(-0.45, 0.45, frontal_alignment)
            * convergence;
        let marine_lift = marine_fraction * (0.04 + cold * 0.16);
        let lcl_lift = clamp(
            convergence * 0.70 + terrain_lift * 1.75 + frontal_lift * 0.35
                + marine_lift - rain_shadow * 0.14,
            0.0,
            1.0,
        );
        let humidity_gate = smooth_step(0.45, 0.95, state.x / max(q_sat, 0.0001));
        let warm_gate = thermal * smooth_step(0.10, 0.20, lcl_lift);
        let convective_lift = clamp(
            lcl_lift + catalyst * humidity_gate * warm_gate * 0.40,
            0.0,
            1.0,
        );
        // Lift cools an air parcel to its LCL; only vapor above that capacity changes phase.
        let q_lcl = min(
            q_sat * (1.0 - mix(0.08, 0.42, convective_lift)),
            q_target * 0.70,
        ) * (1.0 - terrain_lift * 0.65);
        let condensation = min(
            state.x,
            max(state.x - q_lcl, 0.0) * mix(0.16, 0.56, convective_lift)
                * phase_budget * step_fraction,
        );
        let physical_convective_eligibility = warm_gate * smooth_step(0.12, 0.75, lcl_lift)
            * humidity_gate;
        let final_convective_eligibility = budgets.source_envelope * physical_convective_eligibility;
        let deep_fraction = clamp(
            physical_convective_eligibility * (0.30 + catalyst * 0.45),
            0.0,
            0.75,
        );
        state.x -= condensation;
        state.y += condensation * (1.0 - deep_fraction);
        state.z += condensation * deep_fraction;

        let terrain_wind_support = smooth_step(0.03, 0.20, effective_speed);
        let orographic_condensation = min(
            state.x,
            state.x * terrain_lift * terrain_wind_support * phase_budget * 0.28 * step_fraction,
        );
        let orographic_deep_fraction = select(0.0, deep_fraction, catalyst > 0.0);
        state.x -= orographic_condensation;
        state.y += orographic_condensation * (1.0 - orographic_deep_fraction);
        state.z += orographic_condensation * orographic_deep_fraction;

        let evaporation = min(state.y, max(q_target - state.x, 0.0) * 0.012 * step_fraction);
        state.x += evaporation;
        state.y -= evaporation;
        let catalyst_activation = catalyst * budgets.source_envelope;
        let q_up = q_sat * (1.0 - 0.79 * catalyst);
        let vapor_excess = max(state.x - q_up, 0.0);
        let total_condensate = vapor_excess + state.y + state.z;
        let organizing_lift = max(lcl_lift, catalyst * 0.20);
        let organizing_eligibility = thermal * smooth_step(0.10, 0.20, organizing_lift)
            * smooth_step(0.12, 0.75, organizing_lift) * humidity_gate;
        let organizing = smooth_step(
            0.0,
            CATALYST_TARGET_ORGANIZING_ELIGIBILITY,
            organizing_eligibility,
        );
        let target_deep_fraction = clamp(
            0.30 * physical_convective_eligibility
                + CATALYST_TARGET_SHARE_ALPHA * catalyst_activation * organizing,
            0.0,
            CATALYST_TARGET_SHARE_MAX,
        );
        let deep_demand = min(
            vapor_excess + state.y,
            max(target_deep_fraction * total_condensate - state.z, 0.0),
        );
        let transfer = (1.0 - exp(-CATALYST_TARGET_TRANSFER_K * catalyst_activation * step_fraction))
            * deep_demand;
        let vapor_transfer = min(vapor_excess, transfer);
        let low_transfer = transfer - vapor_transfer;
        state.x -= vapor_transfer;
        state.y -= low_transfer;
        state.z += transfer;
        // The deep reservoir detrainment is conservative and supplies the high reservoir.
        let detrainment = min(
            state.z,
            state.z * final_convective_eligibility
                * (0.18 + frontal_lift * 0.12) * step_fraction,
        );
        state.z -= detrainment;
        state.w += detrainment;
    }

    if ((params.diagnostic_flags & DIAGNOSTIC_NO_SINK) == 0u) {
        let condensate = state.y + state.z;
        let rainout = min(
            condensate,
            (max(condensate - q_sat * relative_humidity_target, 0.0) * 0.22
                + state.z * (0.01 + 0.08 * thermal)
                + state.y * marine_fraction * cold * 0.055) * step_fraction,
        );
        let rainout_scale = rainout / max(condensate, 0.0001);
        state.y *= 1.0 - rainout_scale;
        state.z *= 1.0 - rainout_scale;
    }
    if ((params.diagnostic_flags & DIAGNOSTIC_NO_RELAXATION) == 0u) {
        let sublimation = min(state.w * 0.005 * step_fraction, 1.0 - state.x);
        state.w -= sublimation;
        state.x += sublimation;
    }
    textureStore(state_out, vec2<i32>(id.xy), i32(id.z), state);
}

@compute @workgroup_size(8, 8, 1)
fn finalize(@builtin(global_invocation_id) id: vec3<u32>) {
    let res = params.output_resolution;
    if (id.x >= res || id.y >= res || id.z >= 6u) { return; }
    let pos = direction(id, res);
    let state = sample_state(pos);
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
