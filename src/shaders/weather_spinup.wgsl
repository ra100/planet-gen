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
@group(0) @binding(9) var provenance_in: texture_2d_array<f32>;
@group(0) @binding(10) var provenance_out: texture_storage_2d_array<r16float, write>;
// FE-089 owns these transient ping-pong fields. They are intentionally absent
// from WeatherTextures: preview/export only receive finalized mass/geometry.
@group(0) @binding(11) var aux_in: texture_2d_array<f32>;
@group(0) @binding(12) var aux_out: texture_storage_2d_array<rgba16float, write>;
@group(0) @binding(13) var provenance_tail_in: texture_2d_array<f32>;
@group(0) @binding(14) var provenance_tail_out: texture_storage_2d_array<rgba16float, write>;

const PI: f32 = 3.14159265;
const DIAGNOSTIC_NO_SOURCE: u32 = 1u;
const DIAGNOSTIC_NO_SINK: u32 = 2u;
const DIAGNOSTIC_NO_PHASE_CHANGE: u32 = 4u;
const DIAGNOSTIC_NO_RELAXATION: u32 = 8u;
const DIAGNOSTIC_ALL_OCEAN_COMPAT: u32 = 32u;
const PHYSICAL_INTERVAL_SECONDS: f32 = 1280.0;
const ALL_OCEAN_PHYSICAL_INTERVAL_SECONDS: f32 = 1600.0;
const MAX_WIND_MPS: f32 = 50.0;
const MAX_SUBSTEP_TEXELS: f32 = 0.85;
const CATALYST_TARGET_SHARE_ALPHA: f32 = 0.70;
const CATALYST_TARGET_SHARE_MAX: f32 = 0.92;
const CATALYST_TARGET_TRANSFER_K: f32 = 2.0;
const CATALYST_TARGET_ORGANIZING_ELIGIBILITY: f32 = 0.025;
// ARCH-090 first-run baseline. Boundary-layer vapor follows the resolved wind;
// free-tropospheric vapor moves only on the slower circulation exchange.
const K_FT_TO_BL: f32 = 0.08;
const K_BL_TO_FT: f32 = 0.03;
const K_DEEP_TO_FT: f32 = 0.035;
const TRANSFER_CAP_PER_SUBSTEP: f32 = 0.12;
const STORM_RECHARGE_LAND: f32 = 0.06;
const STORM_RECHARGE_MARINE: f32 = 0.18;
const STORM_RECHARGE_HARD_CAP: f32 = 0.24;
// DS-046 #1: bounded eddy diffusion κ∇²(state). κ targets √(κ·25600s) ≈ 450 km
// (4–8 texels @128) so advection-only filaments mix across-wind instead of
// smearing. The explicit blend clamps at 0.24 (< the 0.25 two-dimensional
// monotonicity limit), so extrema never amplify at any resolution/substep count.
const EDDY_DIFFUSIVITY_M2_S: f32 = 8000000.0;
const EDDY_DIFFUSION_MAX_BLEND: f32 = 0.24;

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

fn sample_aux(dir: vec3<f32>) -> vec4<f32> {
    let fuv = sphere_to_face_uv(dir);
    let res = i32(params.spin_resolution - 1u);
    let texel = vec2<i32>(
        clamp(i32(round(fuv.y * f32(res))), 0, res),
        clamp(i32(round(fuv.z * f32(res))), 0, res),
    );
    return textureLoad(aux_in, texel, i32(fuv.x), 0);
}

fn sample_tail(dir: vec3<f32>) -> vec4<f32> {
    let fuv = sphere_to_face_uv(dir);
    let res = i32(params.spin_resolution - 1u);
    let texel = vec2<i32>(
        clamp(i32(round(fuv.y * f32(res))), 0, res),
        clamp(i32(round(fuv.z * f32(res))), 0, res),
    );
    return textureLoad(provenance_tail_in, texel, i32(fuv.x), 0);
}

fn capped_transfer(source: f32, rate: f32, step_fraction: f32) -> f32 {
    // No source-independent floor: an empty reservoir remains bit-zero.
    return min(source, min(source * rate * step_fraction, TRANSFER_CAP_PER_SUBSTEP));
}

fn proportional_transfer(source: f32, provenance: f32, amount: f32) -> f32 {
    return select(0.0, amount * provenance / source, source > 0.0 && amount > 0.0);
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

fn catalyst_owner_steering(index: u32) -> vec3<f32> {
    let center = catalyst_center(index);
    let basis = tangent_basis(center);
    let wind = textureSampleLevel(wind_tex, spinup_sampler, center, 0.0).xyz;
    let tangent_wind = wind - center * dot(wind, center);
    return normalize(tangent_wind + basis[0] * 0.0001);
}

fn catalyst_support(pos: vec3<f32>, index: u32) -> f32 {
    let center = catalyst_center(index);
    let along = catalyst_owner_steering(index);
    let across = normalize(cross(center, along));
    let size_t = clamp((params.storm_size - 0.3) / 2.7, 0.0, 1.0);
    let radius = 0.085 + (0.20 - 0.085) * pow(size_t, 2.0135171);
    let delta = pos - center * dot(pos, center);
    let warp_a = snoise(pos * 19.0 + noise_seed_offset(params.seed, 301u));
    let warp_b = snoise(pos * 37.0 + noise_seed_offset(params.seed, 302u));
    let major = radius * 1.45 * (1.0 + warp_a * 0.13);
    let along_distance = dot(delta, along);
    let width_t = smooth_step(-radius * 0.35, radius * 0.55, along_distance);
    let minor = radius * mix(0.56, 0.88, width_t) * (1.0 + warp_b * 0.13);
    let ellipse = pow(along_distance / max(major, 0.001), 2.0)
        + pow(dot(delta, across) / max(minor, 0.001), 2.0);
    return smooth_step(1.0, 0.72, ellipse);
}

fn convective_catalyst(pos: vec3<f32>) -> f32 {
    let active_count = min(params.storm_count, 8u);
    var response = 0.0;
    for (var index = 0u; index < active_count; index++) {
        response = max(response, catalyst_support(pos, index));
    }
    return response;
}

fn all_ocean_compat() -> bool {
    return (params.diagnostic_flags & DIAGNOSTIC_ALL_OCEAN_COMPAT) != 0u;
}

fn physical_interval_seconds() -> f32 {
    return select(PHYSICAL_INTERVAL_SECONDS, ALL_OCEAN_PHYSICAL_INTERVAL_SECONDS, all_ocean_compat());
}

// Per-dispatch eddy blend: κ·Δt_sub/d² in texel units, capped for monotonicity.
fn eddy_blend(resolution: u32) -> f32 {
    let texel_m = max(params.radius_km * 1000.0, 1.0) * PI * 0.5 / f32(resolution);
    let dt = physical_interval_seconds() / transport_substeps(resolution);
    return min(EDDY_DIFFUSIVITY_M2_S * dt / (texel_m * texel_m), EDDY_DIFFUSION_MAX_BLEND);
}

fn transport_substeps(resolution: u32) -> f32 {
    // RV-003: cap raised from 2.0 to 4.0 (user request — extend how far wind
    // carries clouds). Bit-exact for every wind_scale <= 2.0; above that the
    // displacement grows and this scheduler adds substeps in lockstep so the
    // per-substep step stays within MAX_SUBSTEP_TEXELS. The Rust mirror
    // (weather.rs outgoing_cfl) lifts its clamp to match.
    let displacement = 2.0 * MAX_WIND_MPS * clamp(params.wind_scale, 0.0, 4.0) * physical_interval_seconds()
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

// Terrain selects where transported marine vapor can settle into a calm deck.
fn calm_wet_land_mask(
    pos: vec3<f32>,
    marine_fraction: f32,
    thermal_stability: f32,
    rain_shadow: f32,
    effective_speed: f32,
    et_capacity: f32,
) -> f32 {
    let height = sample_height(pos);
    let land_height = height - params.ocean_level;
    let exposed_land = select(
        0.0,
        smooth_step(0.0, 0.01, land_height),
        land_height > 0.0,
    );
    let elevation_km = max(height - params.ocean_level, 0.0) * 5.0;
    let lowland = 1.0 - smooth_step(0.35, 1.8, elevation_km);
    let continentality = 1.0 - marine_fraction;
    // FE-084: transpiring surfaces count as wet ground — vegetation/soil
    // moisture extends the calm-wet stratiform band (standard land-surface
    // feedback), so the rank gains a bounded ET share.
    let climate_rank = thermal_stability * 0.58 + lowland * 0.27 + continentality * 0.15
        - rain_shadow * 0.35
        + 0.10 * et_capacity;
    return exposed_land * continentality * lowland * smooth_step(0.46, 0.70, climate_rank);
}

fn source_potential(
    marine_fraction: f32,
    thermal: f32,
) -> f32 {
    // Vapor enters this field only over open water. Land can only convert
    // transported vapor already held in state.x.
    return marine_fraction * mix(0.42, 0.68, thermal);
}

// DS-045 A1: water presence is a blurred proximity field, not per-cell
// classification. The ring stencil mirrors wind_field's continentality
// diffusion (bounded iterations, cheap) so sourcing strength fades over
// hundreds of texels instead of tracing the coastline.
const WATER_EDGE_SOFTNESS: f32 = 0.004;
// Residual bootstrap humidity so purely-land worlds still seed sub-saturated
// vapor; the shore-keyed fetch ramp dominates wherever water exists.
const SEED_RESIDUAL_FRACTION: f32 = 0.05;
// FE-085: bounded land-surface evapotranspiration strength. Peak budget
// (coverage=moisture=thermal=1) is 0.27 vs the 0.68 peak open-ocean supply
// potential, i.e. ~40% — raised from 0.17 to lift land cloud decks toward
// ≥25% while keeping the fetch-scaled marine source dominant.
// RV-002 A1b (validation runs 3–5): raising this to 0.35/0.45/0.68 left every
// A-metric and the all-land doctrine value byte-identical — land q_target
// stays far below q_sat, so sub-saturated ET vapor cannot condense without
// convergence; the strength constant is not the binding limit for A1b.
// Reverted to 0.27; A1b needs a gate re-spec or phase-change change (user
// decision).
const LAND_ET_STRENGTH: f32 = 0.27;
// The source-owned tracer mints land evaporation at a reduced share while ocean
// evaporation mints at 1.0. It is therefore ocean-dominant provenance, not a
// marine-origin tracer once land ET is enabled. Kept above zero-but-below-one:
// reduced source ownership downstream of continents
// would otherwise thin the lee-side marine decks at wind_scale=2 (the A1c
// backstop watches exactly that).
const LAND_ET_PROVENANCE_SHARE: f32 = 0.3;

struct WaterField {
    local: f32,
    fetch: f32,
}

fn water_indicator(pos: vec3<f32>) -> f32 {
    return 1.0 - smooth_step(
        params.ocean_level - WATER_EDGE_SOFTNESS,
        params.ocean_level + WATER_EDGE_SOFTNESS,
        sample_height(pos),
    );
}

fn water_field(pos: vec3<f32>) -> WaterField {
    let basis = tangent_basis(pos);
    // Maritime-fetch rings: influence fades over ~10³ km, i.e. tens of texels
    // at preview resolutions, so sourcing never traces individual coastlines.
    let near = clamp(500.0 / max(params.radius_km, 1.0), 0.01, 0.12);
    let far = clamp(2000.0 / max(params.radius_km, 1.0), 0.04, 0.45);
    var blurred_extent = 0.0;
    for (var index = 0u; index < 8u; index++) {
        let angle = f32(index) * 0.7853981633974483;
        let offset = basis[0] * cos(angle) + basis[1] * sin(angle);
        blurred_extent += water_indicator(normalize(pos + offset * near));
        blurred_extent += water_indicator(normalize(pos + offset * far));
    }
    // Mean of the 16 spoke samples (the center feeds `local` instead), clamped
    // as the fetch proxy: small lakes source weaker than open ocean.
    return WaterField(water_indicator(pos), min(blurred_extent * 0.0625, 1.0));
}

// A sphere-space, low-frequency perturbation only changes conservative phase
// partitioning. Clamping keeps it within the stated ±12% range.
fn phase_rainout_modulation(pos: vec3<f32>) -> f32 {
    if (all_ocean_compat()) {
        return 1.0;
    }
    return clamp(
        1.0 + 0.12 * snoise(pos * 2.0 + noise_seed_offset(params.seed, 607u)),
        0.88,
        1.12,
    );
}

struct SourceBudgets {
    supply: f32,
    phase: f32,
    source_envelope: f32,
}

// Coverage controls source supply and vapor phase conversion independently.
// FE-084 note: an earlier draft routed evapotranspiration into phase_rank as
// well. Measured on the DS-046 validation scenes, boosting land phase
// capacity intercepts transported marine vapor at ws=2 hard enough that
// lee-side ocean decks thin and the raw connected cloud-free region (A1c
// backstop) grows past its gate, so sourcing stays bounded to the supply/
// humidity channels below.
fn source_budgets(
    coverage: f32,
    marine_fraction: f32,
    convergence: f32,
    thermal: f32,
    persistent_ice: f32,
) -> SourceBudgets {
    let local_potential = source_potential(
        marine_fraction,
        thermal,
    );
    let c = clamp(coverage, 0.0, 1.0);
    let source_envelope = smoothstep(0.0, 0.25, c);
    let surface_supply = source_potential(
        marine_fraction,
        thermal,
    );
    let thermal_regime = mix(0.14, 0.04, thermal);
    // Phase capacity is independent of local supply: dry land has no source,
    // but humid marine air may be transported there before it condenses.
    let phase_rank = clamp(
        surface_supply + (1.0 - marine_fraction) * 0.45 + 0.25 * convergence
            + thermal_regime - 0.15 * persistent_ice,
        0.0,
        1.0,
    );
    let center = 0.88 - 0.40 * c;
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

struct TerrainTransect {
    ascent: f32,
    lee_drying: f32,
}

// Four bounded upwind samples retain terrain history long enough for a lee
// shadow without adding a second weather field or an unbounded integration.
fn terrain_transect(pos: vec3<f32>, wind_dir: vec3<f32>, wind_speed: f32) -> TerrainTransect {
    let step = clamp(220.0 / max(params.radius_km, 1.0), 0.018, 0.065);
    let h0 = sample_height(pos);
    let h1 = sample_height(normalize(pos - wind_dir * step));
    let h2 = sample_height(normalize(pos - wind_dir * step * 2.0));
    let h3 = sample_height(normalize(pos - wind_dir * step * 3.0));
    let h4 = sample_height(normalize(pos - wind_dir * step * 4.0));
    let wind_gate = smooth_step(0.03, 0.20, wind_speed);
    // Only the immediately upwind slope lifts this column. Looking farther
    // upstream crosses a ridge from its lee side and incorrectly creates lift.
    let ascent = max(h0 - h1, 0.0);
    let lee_height = max(max(h1, h2), max(h3, h4)) - h0;
    return TerrainTransect(
        smooth_step(0.0005, 0.015, ascent) * wind_gate,
        smooth_step(0.003, 0.040, lee_height) * wind_gate,
    );
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
    let half = reconstructed.center - 0.5 * physical_interval_seconds() / max(params.radius_km * 1000.0, 1.0) / transport_substeps(params.spin_resolution) * divergence;
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

fn provenance_sample(dir: vec3<f32>) -> f32 {
    let fuv = sphere_to_face_uv(dir);
    let res = i32(params.spin_resolution - 1u);
    let texel = vec2<i32>(
        clamp(i32(round(fuv.y * f32(res))), 0, res),
        clamp(i32(round(fuv.z * f32(res))), 0, res),
    );
    return textureLoad(provenance_in, texel, i32(fuv.x), 0).x;
}

fn provenance_reconstruct(center: vec3<f32>) -> Reconstruction {
    let fuv = sphere_to_face_uv(center);
    let face = u32(fuv.x);
    let uv = fuv.yz;
    let step = 1.0 / f32(params.spin_resolution - 1u);
    let state = provenance_sample(center);
    let west = provenance_sample(cube_to_sphere(face, uv - vec2<f32>(step, 0.0)));
    let east = provenance_sample(cube_to_sphere(face, uv + vec2<f32>(step, 0.0)));
    let south = provenance_sample(cube_to_sphere(face, uv - vec2<f32>(0.0, step)));
    let north = provenance_sample(cube_to_sphere(face, uv + vec2<f32>(0.0, step)));
    let slope_s = mc_slope(vec4<f32>(west), vec4<f32>(state), vec4<f32>(east)).x;
    let slope_t = mc_slope(vec4<f32>(south), vec4<f32>(state), vec4<f32>(north)).x;
    let raw_west = state - 0.5 * slope_s;
    let raw_east = state + 0.5 * slope_s;
    let raw_south = state - 0.5 * slope_t;
    let raw_north = state + 0.5 * slope_t;
    let minimum = min(min(raw_west, raw_east), min(raw_south, raw_north));
    let theta = select(1.0, clamp(state / max(state - minimum, 0.000001), 0.0, 1.0), minimum < 0.0);
    return Reconstruction(
        vec4<f32>(state),
        vec4<f32>(state + theta * (raw_west - state)),
        vec4<f32>(state + theta * (raw_east - state)),
        vec4<f32>(state + theta * (raw_south - state)),
        vec4<f32>(state + theta * (raw_north - state)),
    );
}

fn provenance_toward(center: vec3<f32>, toward: vec3<f32>) -> vec4<f32> {
    let reconstructed = provenance_reconstruct(center);
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

fn provenance_transport(pos: vec3<f32>) -> f32 {
    let fuv = sphere_to_face_uv(pos);
    let face = u32(fuv.x);
    let uv = fuv.yz;
    let step = 1.0 / f32(params.spin_resolution - 1u);
    let metric_step = 2.0 * step;
    let east_pos = cube_to_sphere(face, uv + vec2<f32>(step, 0.0));
    let west_pos = cube_to_sphere(face, uv - vec2<f32>(step, 0.0));
    let north_pos = cube_to_sphere(face, uv + vec2<f32>(0.0, step));
    let south_pos = cube_to_sphere(face, uv - vec2<f32>(0.0, step));
    let predicted = provenance_reconstruct(pos);
    let east_flux = shared_flux(pos, predicted.east, east_pos, provenance_toward(east_pos, pos)).x;
    let west_flux = shared_flux(west_pos, provenance_toward(west_pos, pos), pos, predicted.west).x;
    let north_flux = shared_flux(pos, predicted.north, north_pos, provenance_toward(north_pos, pos)).x;
    let south_flux = shared_flux(south_pos, provenance_toward(south_pos, pos), pos, predicted.south).x;
    let divergence = (east_flux * face_angle(pos, east_pos)
        - west_flux * face_angle(west_pos, pos)
        + north_flux * face_angle(pos, north_pos)
        - south_flux * face_angle(south_pos, pos))
        / max(cell_area(pos) * metric_step * metric_step, 0.000001);
    return predicted.center.x - physical_interval_seconds() / max(params.radius_km * 1000.0, 1.0)
        / transport_substeps(params.spin_resolution) * divergence;
}

// Per-invocation record of exactly what advance_state applied to the local
// budget, so the tracer mirrors the real transition instead of recomputing a
// second, drift-prone copy of the same terms.
var<private> provenance_source_evaporation: f32 = 0.0;
var<private> provenance_rainout_scale: f32 = 0.0;

fn bounded_provenance(value: f32, state: vec4<f32>) -> f32 {
    // R16 provenance and Rgba16 state round independently to half precision, so
    // tolerate one relative f16 ULP of storage headroom instead of subtracting
    // fixed mass: P == T survives pure-ocean init, and T == 0 still forces P == 0.
    // Headroom derives from total alone — never value — so an empty budget
    // clamps P to exactly zero even when value is nonzero (keeps 0 <= P <= T).
    let total = state.x + state.y + state.z + state.w;
    if (!(total > 0.0)) {
        return 0.0;
    }
    return clamp(value, 0.0, total * 1.0009765625);
}

@compute @workgroup_size(8, 8, 1)
fn init(@builtin(global_invocation_id) id: vec3<u32>) {
    let res = params.spin_resolution;
    if (id.x >= res || id.y >= res || id.z >= 6u) { return; }
    if (params.coverage <= 0.0 || params.moisture <= 0.0) {
        textureStore(state_out, vec2<i32>(id.xy), i32(id.z), vec4<f32>(0.0));
        textureStore(aux_out, vec2<i32>(id.xy), i32(id.z), vec4<f32>(0.0));
        textureStore(provenance_tail_out, vec2<i32>(id.xy), i32(id.z), vec4<f32>(0.0));
        return;
    }

    let pos = direction(id, res);
    let pressure = smooth_step(0.05, 0.3, params.surface_pressure_bar);
    let continentality = textureSampleLevel(wind_tex, spinup_sampler, pos, 0.0).a;
    let marine_fraction = 1.0 - smooth_step(0.15, 0.85, continentality);
    let water = water_field(pos);
    let marine_climate = max(marine_fraction, water.local);
    let thermal_stability = smooth_step(-25.0, 30.0, temperature_at(pos));
    let ice_fraction = 1.0 - smoothstep(-15.0, -6.0, temperature_at(pos));
    let persistent_ice = marine_fraction * ice_fraction;
    let source_ice = water.local * ice_fraction;
    let budgets = source_budgets(
        params.coverage,
        marine_climate,
        0.0,
        thermal_stability,
        persistent_ice,
    );
    let surface_supply_factor = 1.0 - 0.25 * source_ice;
    let marine_vapor = select(
        budgets.supply * water.local * water.fetch * surface_supply_factor
            * clamp(params.moisture, 0.0, 1.0) * pressure * 0.42,
        0.0,
        (params.diagnostic_flags & DIAGNOSTIC_NO_SOURCE) != 0u,
    );
    // DS-045: one-time sub-saturated bootstrap with a monotonic shore-anchored
    // decay keyed to blurred water proximity — no coastal band. Transport never
    // recharges it on land.
    let land_seed = (1.0 - water.local)
        * (SEED_RESIDUAL_FRACTION + (1.0 - SEED_RESIDUAL_FRACTION) * water.fetch)
        * thermal_stability
        * clamp(params.coverage, 0.0, 1.0) * clamp(params.moisture, 0.0, 1.0)
        * pressure * 0.10;
    let seeded_vapor = select(
        min(marine_vapor + land_seed, 0.70 * mix(0.16, 0.68, thermal_stability) * pressure),
        marine_vapor,
        all_ocean_compat(),
    );
    let vapor = select(
        seeded_vapor,
        0.0,
        (params.diagnostic_flags & DIAGNOSTIC_NO_SOURCE) != 0u,
    );
    textureStore(
        state_out,
        vec2<i32>(id.xy),
        i32(id.z),
        vec4<f32>(vapor, 0.0, 0.0, 0.0),
    );
    textureStore(aux_out, vec2<i32>(id.xy), i32(id.z), vec4<f32>(0.0));
    textureStore(provenance_tail_out, vec2<i32>(id.xy), i32(id.z), vec4<f32>(0.0));
}

@compute @workgroup_size(8, 8, 1)
fn init_with_provenance(@builtin(global_invocation_id) id: vec3<u32>) {
    let res = params.spin_resolution;
    if (id.x >= res || id.y >= res || id.z >= 6u) { return; }
    if (params.coverage <= 0.0 || params.moisture <= 0.0) {
        textureStore(state_out, vec2<i32>(id.xy), i32(id.z), vec4<f32>(0.0));
        textureStore(provenance_out, vec2<i32>(id.xy), i32(id.z), vec4<f32>(0.0));
        textureStore(aux_out, vec2<i32>(id.xy), i32(id.z), vec4<f32>(0.0));
        textureStore(provenance_tail_out, vec2<i32>(id.xy), i32(id.z), vec4<f32>(0.0));
        return;
    }
    let pos = direction(id, res);
    let pressure = smooth_step(0.05, 0.3, params.surface_pressure_bar);
    let continentality = textureSampleLevel(wind_tex, spinup_sampler, pos, 0.0).a;
    let marine_fraction = 1.0 - smooth_step(0.15, 0.85, continentality);
    let water = water_field(pos);
    let marine_climate = max(marine_fraction, water.local);
    let thermal_stability = smooth_step(-25.0, 30.0, temperature_at(pos));
    let ice_fraction = 1.0 - smoothstep(-15.0, -6.0, temperature_at(pos));
    let persistent_ice = marine_fraction * ice_fraction;
    let source_ice = water.local * ice_fraction;
    let budgets = source_budgets(params.coverage, marine_climate, 0.0, thermal_stability, persistent_ice);
    let surface_supply_factor = 1.0 - 0.25 * source_ice;
    let marine_vapor = select(
        budgets.supply * water.local * water.fetch * surface_supply_factor
            * clamp(params.moisture, 0.0, 1.0) * pressure * 0.42,
        0.0,
        (params.diagnostic_flags & DIAGNOSTIC_NO_SOURCE) != 0u,
    );
    // DS-045: monotonic shore-anchored bootstrap decay — no coastal band.
    let land_seed = (1.0 - water.local)
        * (SEED_RESIDUAL_FRACTION + (1.0 - SEED_RESIDUAL_FRACTION) * water.fetch)
        * thermal_stability
        * clamp(params.coverage, 0.0, 1.0) * clamp(params.moisture, 0.0, 1.0)
        * pressure * 0.10;
    let seeded_vapor = select(
        min(marine_vapor + land_seed, 0.70 * mix(0.16, 0.68, thermal_stability) * pressure),
        marine_vapor,
        all_ocean_compat(),
    );
    let vapor = select(seeded_vapor, 0.0, (params.diagnostic_flags & DIAGNOSTIC_NO_SOURCE) != 0u);
    let state = vec4<f32>(vapor, 0.0, 0.0, 0.0);
    textureStore(state_out, vec2<i32>(id.xy), i32(id.z), state);
    // Land's finite bootstrap is deliberately unowned; only open-ocean vapor is P.
    textureStore(provenance_out, vec2<i32>(id.xy), i32(id.z), vec4<f32>(bounded_provenance(marine_vapor, state)));
    textureStore(aux_out, vec2<i32>(id.xy), i32(id.z), vec4<f32>(0.0));
    textureStore(provenance_tail_out, vec2<i32>(id.xy), i32(id.z), vec4<f32>(0.0));
}

struct SpinupAdvance {
    state: vec4<f32>,
    aux: vec4<f32>,
    tail: vec4<f32>,
}

fn advance_state(pos: vec3<f32>, provenance_mass: f32, enable_vertical: bool) -> SpinupAdvance {
    let res = params.spin_resolution;
    let wind = textureSampleLevel(wind_tex, spinup_sampler, pos, 0.0);
    let tangent_wind = wind.xyz - pos * dot(wind.xyz, pos);
    let normalized_speed = length(tangent_wind);
    // DS-046 #3: forcing tracks the transport coupling. Transport velocity has
    // always scaled linearly with wind_scale (fn velocity), while lift and
    // convergence used wind_scale^0.3 — at wind_scale 2 vapor crossed continents
    // before any lift could act (interior drain); at 0.25 lift outran transport
    // (coast-only geography). 0.85 closes most of the gap while leaving headroom
    // against the raised convergence gain; wind_scale = 1 stays the physical
    // baseline untouched (pow(1, x) = 1).
    let forcing_scale = pow(max(params.wind_scale, 0.0), 0.85);
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
    // DS-046 #1: bounded local eddy stabilization of the pre-transport
    // neighborhood. This cubemap stencil is not area-weighted or proven
    // conservative across seams; the blend cap only bounds local extrema.
    let eddy = eddy_blend(res);
    let neighborhood = 0.25 * (
        sample_state(cube_to_sphere(face, uv + vec2<f32>(grid_step, 0.0)))
            + sample_state(cube_to_sphere(face, uv - vec2<f32>(grid_step, 0.0)))
            + sample_state(cube_to_sphere(face, uv + vec2<f32>(0.0, grid_step)))
            + sample_state(cube_to_sphere(face, uv - vec2<f32>(0.0, grid_step))));
    var state = predicted.center
        - physical_interval_seconds() * step_fraction
            / max(params.radius_km * 1000.0, 1.0) * transport_divergence
        + eddy * (neighborhood - predicted.center);
    // state=(q_bl,c_low,c_deep,c_high),
    // aux=(q_ft,P_bl,P_low,P_deep), tail=(P_ft,P_high,0,0).
    // q_ft takes the slower circulation path (0.35 of the full-wind rate,
    // scaled by wind_scale like every other reservoir — RV-002 P0); all
    // condensate and q_bl use the existing full-wind finite-volume path above.
    let ft_backtrace = normalize(
        pos - tangent_wind * (0.35 * MAX_WIND_MPS * clamp(params.wind_scale, 0.0, 4.0)
            * physical_interval_seconds()
            * step_fraction / max(params.radius_km * 1000.0, 1.0)),
    );
    var aux = sample_aux(ft_backtrace);
    var tail = sample_tail(ft_backtrace);
    // The legacy scalar remains the published source-owned aggregate. The
    // detailed reservoirs mirror it from q_bl whenever no detailed history is
    // available (normal Mode-Off path), without minting any mass.
    if (enable_vertical && aux.y == 0.0 && provenance_mass > 0.0 && state.x > 0.0) {
        aux.y = min(provenance_mass, state.x);
    }
    if (enable_vertical) {
        let ft_to_bl = capped_transfer(aux.x, K_FT_TO_BL, step_fraction);
        let p_ft_to_bl = proportional_transfer(aux.x, tail.x, ft_to_bl);
        aux.x -= ft_to_bl;
        tail.x -= p_ft_to_bl;
        state.x += ft_to_bl;
        aux.y += p_ft_to_bl;
        let bl_to_ft = capped_transfer(state.x, K_BL_TO_FT, step_fraction);
        let p_bl_to_ft = proportional_transfer(state.x, aux.y, bl_to_ft);
        state.x -= bl_to_ft;
        aux.y -= p_bl_to_ft;
        aux.x += bl_to_ft;
        tail.x += p_bl_to_ft;
    }

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

    let terrain = terrain_transect(pos, wind_dir, effective_speed);
    let terrain_lift = terrain.ascent;
    let rain_shadow = terrain.lee_drying;
    let thermal = smooth_step(-25.0, 30.0, temperature_at(pos));
    let pressure_factor = smooth_step(0.05, 0.3, params.surface_pressure_bar);
    let local_pressure = clamp(textureSampleLevel(pressure_tex, spinup_sampler, pos, 0.0).r / 1013.0, 0.8, 1.2);
    let marine_fraction = 1.0 - smooth_step(0.15, 0.85, wind.a);
    let water = water_field(pos);
    let marine_climate = max(marine_fraction, water.local);
    let cold = 1.0 - thermal;
    let q_sat = mix(0.16, 0.68, thermal) * pressure_factor * local_pressure;
    let ice_fraction = 1.0 - smoothstep(-15.0, -6.0, temperature_at(pos));
    let persistent_ice = marine_fraction * ice_fraction;
    let source_ice = water.local * ice_fraction;
    // FE-090 (S1): per-texel vegetation density from existing fields — no new
    // fields, no layout bump. Vegetation needs per-texel moisture (coasts wet,
    // interiors dry; lee slopes dried by rain shadow), an altitude low enough
    // for a treeline, and no persistent ice. Bounded in [0,1]. The 6..22 °C
    // warmth window stays in et_capacity below (single source of truth); this
    // proxy adds the moisture/relief structure it currently lacks. wind.a is
    // the continentality channel (same inversion as marine_fraction above), so
    // coasts get dense vegetation and deep interiors sparse — a Saharan cell
    // transpires less than an Amazonian one at the same temperature.
    let elevation_km = max(sample_height(pos) - params.ocean_level, 0.0) * 5.0;
    let continentality = smooth_step(0.15, 0.85, wind.a);
    let veg_moisture = (1.0 - rain_shadow * 0.6)
        * mix(0.10, 1.0, 1.0 - continentality);
    let vegetation_density = veg_moisture
        * (1.0 - smooth_step(2.2, 3.4, elevation_km))   // treeline cap ~3 km
        * (1.0 - ice_fraction);                         // frozen ground: no ET
    // FE-086: bounded land-only evapotranspiration capacity. It gates the
    // supplemental source below by coverage, moisture, surface temperature,
    // and per-texel vegetation density (FE-090 S1). It is zero over standing
    // water (water.local != 0), with no coverage or moisture, on cold ground,
    // and where vegetation is absent; the thermal window starts at 6 °C because
    // evapotranspiration is negligible below ~5 °C (frozen/inactive soil).
    let et_capacity = f32(water.local == 0.0)
        * clamp(params.coverage, 0.0, 1.0)
        * clamp(params.moisture, 0.0, 1.0)
        * smooth_step(6.0, 22.0, temperature_at(pos))
        * vegetation_density;
    let budgets = source_budgets(
        params.coverage,
        marine_climate,
        convergence,
        thermal,
        persistent_ice,
    );
    let surface_supply_factor = 1.0 - 0.25 * source_ice;
    // FE-086: land receives a bounded supplemental ET source; ocean supply
    // remains fetch-scaled. LAND_ET_STRENGTH = 0.27 is ~40% of the 0.68
    // open-ocean peak. Land-origin vapor contributes at the 0.3 provenance
    // share below, preserving its distinction from full-strength marine source.
    let ocean_supply = budgets.supply * water.local * water.fetch * surface_supply_factor;
    let et_budget = LAND_ET_STRENGTH * et_capacity;
    let supply_budget = ocean_supply + et_budget;
    let phase_budget = budgets.phase;
    // FE-078 (DS-043): advected source-owned provenance raises the inland
    // humidity ceiling toward the marine value so wind-aligned plumes survive
    // farther inland. This is ocean-dominant, not marine-origin evidence when
    // land ET is active. Pure-ocean cells keep marine_climate=1 (bit-identical).
    let source_owned_share = clamp(
        provenance_mass / max(state.x + state.y + state.z + state.w, 0.0001),
        0.0,
        1.0,
    );
    // FE-084: a transpiring surface holds its boundary layer near the marine
    // humidity regime — evapotranspiration replaces the moisture that
    // turbulence exports, so the local climate target approaches the marine
    // value in proportion to surface wetness (et_capacity). The SOURCE budget
    // stays capped; only the equilibrium humidity rises toward what ocean
    // cells already use. Ocean cells keep the marine blend bit-exact.
    let effective_relative_humidity_target = clamp(
        mix(
            0.45,
            0.72,
            max(
                max(marine_climate, source_owned_share),
                min(et_capacity * 2.0, 1.0),
            ),
        ) + marine_climate * cold * 0.18,
        0.0,
        0.92,
    );
    let q_target = q_sat * supply_budget * clamp(params.moisture, 0.0, 1.0)
        * effective_relative_humidity_target;
    // DS-045: flow/history signals replace static marine classification.
    // Saturation of transported vapor relative to its climate target is smooth
    // by advection; source ownership carries its weighted source history.
    let saturation_ratio = state.x / max(q_sat, 0.0001);
    let transported_saturation = clamp(
        saturation_ratio / max(0.88 * effective_relative_humidity_target, 0.0001),
        0.0,
        1.0,
    );
    // FE-084: where evapotranspiration is the active surface source, air
    // matures against its LOCAL climate ceiling q_target, not deep-marine
    // saturation q_sat — a transpiring boundary layer reaches deck maturity
    // at far lower absolute humidity than maritime air requires. Pure ocean
    // and every diagnostic-kill path keep the marine ratio bit-exact.
    let climate_maturity = select(
        0.0,
        smooth_step(0.30, 0.85, min(state.x / max(q_target, 0.0001), 1.0)),
        et_capacity > 0.0,
    );
        if ((params.diagnostic_flags & DIAGNOSTIC_NO_SOURCE) == 0u) {
        let evaporation = max(q_target - state.x, 0.0) * 0.030 * step_fraction;
        state.x += evaporation;
        let land_supply_share = et_budget / max(supply_budget, 0.0001);
        // Ownership rule: provenance mints the source-step evaporation.
        // Marine-sourced vapor mints in full; the FE-084 land
        // evapotranspiration share mints at LAND_ET_PROVENANCE_SHARE so
        // land-origin plumes stay distinguishable from marine ones. The
        // supply gate above encodes water extent exactly once (FE-079):
        // attributing by budget share never double-gates transitional cells,
        // and when both budgets are zero q_target is zero, so evaporation is
        // zero and the share below is never applied to nonzero mass.
            provenance_source_evaporation = evaporation
                * (1.0 - (1.0 - LAND_ET_PROVENANCE_SHARE) * land_supply_share);
            aux.y += provenance_source_evaporation;
    }


    let calm_wet_land = calm_wet_land_mask(
        pos,
        marine_fraction,
        thermal,
        rain_shadow,
        effective_speed,
        et_capacity,
    );
    let stratiform_wind = 1.0 - smooth_step(0.35, 0.70, effective_speed);
    let stratiform_regime = calm_wet_land * stratiform_wind;
    let storm_catalyst = convective_catalyst(pos) * (1.0 - stratiform_regime);
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
        let warm_surface_lift = max(
            max(transported_saturation, source_owned_share),
            climate_maturity,
        ) * thermal
            * smooth_step(0.05, 0.30, 1.0 - cold);
        // DS-046 #4: mesoscale steering (#2) gives convergence real sub-synoptic
        // signal, so its lift weight rises from 0.70 to 0.95.
        // Once source-owned vapor has been transported near saturation — or its
        // weighted source share is high —
        // allow lowland/uplift conversion instead of applying the humidity gate
        // a second time.
        // FE-084: locally-transpired air counts as mature inland humidity —
        // it is not second-hand maritime moisture and still respects the lee
        // rain-shadow multiplier below.
        let inland_provenance = max(
            max(smoothstep(0.03, 0.60, saturation_ratio), source_owned_share),
            climate_maturity,
        ) * (1.0 - (1.0 - source_owned_share) * smoothstep(0.0001, 0.01, rain_shadow));
        // DS-046 #5: interior thermal convection (deployed because A1 spread
        // still failed after #1–#4). Hot lowlands with source-owned provenance get
        // buoyant uplift, repartitioning already-transported vapor into
        // condensate; it mints nothing and stays gated by inland_provenance.
        // The hot band is relative to the planetary mean so any climate reaches
        // its own convective belt.
        let elevation_km = max(sample_height(pos) - params.ocean_level, 0.0) * 5.0;
        let hot_lowland_uplift = inland_provenance
            * (1.0 - smooth_step(0.35, 1.8, elevation_km))
            * smooth_step(
                params.base_temp_c - 2.0,
                params.base_temp_c + 6.0,
                temperature_at(pos),
            );
        let lcl_lift = clamp(
            convergence * 0.95 + terrain_lift * 1.75 + frontal_lift * 0.35
                + warm_surface_lift - rain_shadow * 0.55 + hot_lowland_uplift * 0.45,
            0.0,
            1.0,
        );
        let humidity_gate = max(
            smooth_step(0.45, 0.95, state.x / max(q_sat, 0.0001)),
            climate_maturity,
        );
        let warm_gate = thermal * smooth_step(0.10, 0.20, lcl_lift);
        let physical_lift = smooth_step(0.12, 0.75, max(convergence, max(frontal_lift, max(warm_surface_lift, max(terrain_lift, hot_lowland_uplift)))));
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
                * phase_budget * inland_provenance * step_fraction,
        );
        let p_condensation = proportional_transfer(state.x, aux.y, condensation);
        let physical_convective_eligibility = warm_gate * physical_lift
            * humidity_gate;
        let final_convective_eligibility = budgets.source_envelope * physical_convective_eligibility;
        let deep_fraction = clamp(
            physical_convective_eligibility * (0.30 + catalyst * 0.45)
                * phase_rainout_modulation(pos),
            0.0,
            0.75,
        );
        state.x -= condensation;
        aux.y -= p_condensation;
        state.y += condensation * (1.0 - deep_fraction);
        state.z += condensation * deep_fraction;
        aux.z += p_condensation * (1.0 - deep_fraction);
        aux.w += p_condensation * deep_fraction;

        if (stratiform_regime > 0.0) {
            let low_target = min(0.12, q_sat * 0.24 * calm_wet_land);
            let approach = 1.0 - exp(
                -0.75 * humidity_gate * stratiform_wind * step_fraction,
            );
            let calm_stratiform = min(
                state.x,
                max(low_target - state.y, 0.0) * approach,
            );
            state.x -= calm_stratiform;
            state.y += calm_stratiform;
            let p_calm_stratiform = proportional_transfer(state.x + calm_stratiform, aux.y, calm_stratiform);
            aux.y -= p_calm_stratiform;
            aux.z += p_calm_stratiform;
        }

        let terrain_wind_support = smooth_step(0.03, 0.20, effective_speed);
        let orographic_condensation = min(
            state.x,
            state.x * terrain_lift * terrain_wind_support * phase_budget * 0.28 * step_fraction,
        );
        let orographic_deep_fraction = select(0.0, deep_fraction, catalyst > 0.0);
        state.x -= orographic_condensation;
        let p_orographic = proportional_transfer(state.x + orographic_condensation, aux.y, orographic_condensation);
        aux.y -= p_orographic;
        state.y += orographic_condensation * (1.0 - orographic_deep_fraction);
        state.z += orographic_condensation * orographic_deep_fraction;
        aux.z += p_orographic * (1.0 - orographic_deep_fraction);
        aux.w += p_orographic * orographic_deep_fraction;

        let evaporation = min(state.y, max(q_target - state.x, 0.0) * 0.012 * step_fraction);
        state.x += evaporation;
        state.y -= evaporation;
        let p_low_evaporation = proportional_transfer(state.y + evaporation, aux.z, evaporation);
        aux.z -= p_low_evaporation;
        aux.y += p_low_evaporation;
        let catalyst_activation = catalyst * budgets.source_envelope;
        let q_up = q_sat * (1.0 - 0.79 * catalyst);
        let vapor_excess = max(state.x - q_up, 0.0);
        let total_condensate = vapor_excess + state.y + state.z;
        let organizing_lift = max(lcl_lift, catalyst * 0.20);
        let organizing_eligibility = thermal * smooth_step(0.10, 0.20, organizing_lift)
            * smooth_step(0.12, 0.75, organizing_lift) * humidity_gate;
        let organizing = smooth_step(0.0, CATALYST_TARGET_ORGANIZING_ELIGIBILITY, organizing_eligibility);
        let target_deep_fraction = clamp(0.30 * physical_convective_eligibility + CATALYST_TARGET_SHARE_ALPHA * catalyst_activation * organizing, 0.0, CATALYST_TARGET_SHARE_MAX);
        let deep_demand = min(vapor_excess + state.y, max(target_deep_fraction * total_condensate - state.z, 0.0));
        let transfer = (1.0 - exp(-CATALYST_TARGET_TRANSFER_K * catalyst_activation * step_fraction)) * deep_demand;
        let vapor_transfer = min(vapor_excess, transfer);
        let low_transfer = transfer - vapor_transfer;
        state.x -= vapor_transfer;
        state.y -= low_transfer;
        state.z += transfer;
        let p_vapor_transfer = proportional_transfer(state.x + vapor_transfer, aux.y, vapor_transfer);
        let p_low_transfer = proportional_transfer(state.y + low_transfer, aux.z, low_transfer);
        aux.y -= p_vapor_transfer;
        aux.z -= p_low_transfer;
        aux.w += p_vapor_transfer + p_low_transfer;

        // The deep reservoir detrainment is conservative and supplies the high reservoir.
        let detrainment = min(
            state.z,
            state.z * final_convective_eligibility
                * (0.18 + frontal_lift * 0.12) * step_fraction,
        );
        state.z -= detrainment;
        state.w += detrainment;
        let p_detrainment = proportional_transfer(state.z + detrainment, aux.w, detrainment);
        aux.w -= p_detrainment;
        tail.y += p_detrainment;

    }

    if ((params.diagnostic_flags & DIAGNOSTIC_NO_SINK) == 0u) {
        let condensate = state.y + state.z;
        // DS-045: low-cloud dissipation follows physical proxies in scope —
        // cold air and wind-driven entrainment — never the static marine class.
        let low_dissipation = cold
            * (0.025 + 0.055 * smooth_step(0.15, 0.70, effective_speed));
        let rainout = min(
            condensate,
            (max(condensate - q_sat * effective_relative_humidity_target, 0.0) * 0.22
                + state.z * (0.01 + 0.08 * thermal)
                + state.y * low_dissipation) * step_fraction,
        );
        let rainout_scale = rainout / max(condensate, 0.0001);
        // Explicit rain sink: only c_low/c_deep (and their source ownership)
        // leave the column here; no source-independent cleanup is applied.
        state.y *= 1.0 - rainout_scale;
        state.z *= 1.0 - rainout_scale;
        aux.z *= 1.0 - rainout_scale;
        aux.w *= 1.0 - rainout_scale;
        provenance_rainout_scale = rainout_scale;
    }
    if ((params.diagnostic_flags & DIAGNOSTIC_NO_RELAXATION) == 0u) {
        if (enable_vertical) {
            let deep_return = capped_transfer(state.z, K_DEEP_TO_FT, step_fraction);
            let p_deep_return = proportional_transfer(state.z, aux.w, deep_return);
            state.z -= deep_return;
            aux.w -= p_deep_return;
            aux.x += deep_return;
            tail.x += p_deep_return;
            let high_return = capped_transfer(state.w, 0.005, step_fraction);
            let p_high_return = proportional_transfer(state.w, tail.y, high_return);
            state.w -= high_return;
            tail.y -= p_high_return;
            aux.x += high_return;
            tail.x += p_high_return;
        } else {
            let sublimation = min(state.w * 0.005 * step_fraction, 1.0 - state.x);
            state.w -= sublimation;
            state.x += sublimation;
        }
    }
    return SpinupAdvance(state, aux, tail);
}

@compute @workgroup_size(8, 8, 1)
fn transport(@builtin(global_invocation_id) id: vec3<u32>) {
    let res = params.spin_resolution;
    if (id.x >= res || id.y >= res || id.z >= 6u) { return; }
    if (params.coverage <= 0.0 || params.moisture <= 0.0) {
        textureStore(state_out, vec2<i32>(id.xy), i32(id.z), vec4<f32>(0.0));
        textureStore(aux_out, vec2<i32>(id.xy), i32(id.z), vec4<f32>(0.0));
        textureStore(provenance_tail_out, vec2<i32>(id.xy), i32(id.z), vec4<f32>(0.0));
        return;
    }
    // Provenance texture may be unbound in this pass: neutral mass keeps the
    // local humidity target (0 share → no blend).
    let advance = advance_state(direction(id, res), 0.0, false);
    textureStore(state_out, vec2<i32>(id.xy), i32(id.z), advance.state);
    textureStore(aux_out, vec2<i32>(id.xy), i32(id.z), advance.aux);
    textureStore(provenance_tail_out, vec2<i32>(id.xy), i32(id.z), advance.tail);
}

@compute @workgroup_size(8, 8, 1)
fn transport_with_provenance(@builtin(global_invocation_id) id: vec3<u32>) {
    let res = params.spin_resolution;
    if (id.x >= res || id.y >= res || id.z >= 6u) { return; }
    if (params.coverage <= 0.0 || params.moisture <= 0.0) {
        textureStore(state_out, vec2<i32>(id.xy), i32(id.z), vec4<f32>(0.0));
        textureStore(provenance_out, vec2<i32>(id.xy), i32(id.z), vec4<f32>(0.0));
        textureStore(aux_out, vec2<i32>(id.xy), i32(id.z), vec4<f32>(0.0));
        textureStore(provenance_tail_out, vec2<i32>(id.xy), i32(id.z), vec4<f32>(0.0));
        return;
    }

    // The normal state pass is intentionally duplicated by the selected pipeline below.
    // P follows its own conservative finite-volume flux; source and rainout reuse the
    // exact amounts advance_state applied to this cell's pre-rainout transition, so
    // ownership never drifts from the published budget and never feeds weather
    // production.
    let pos = direction(id, res);
    let transported = max(provenance_transport(pos), 0.0);
    let advance = advance_state(pos, transported, !all_ocean_compat());
    let state = advance.state;
    // q_target already carries the open-ocean gate; add the recorded evaporation once.
    let p_after_source = transported + provenance_source_evaporation;
    let p_after_rainout = p_after_source * (1.0 - provenance_rainout_scale);
    textureStore(state_out, vec2<i32>(id.xy), i32(id.z), state);
    textureStore(aux_out, vec2<i32>(id.xy), i32(id.z), advance.aux);
    textureStore(provenance_tail_out, vec2<i32>(id.xy), i32(id.z), advance.tail);
    textureStore(provenance_out, vec2<i32>(id.xy), i32(id.z), vec4<f32>(bounded_provenance(p_after_rainout, state)));
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
