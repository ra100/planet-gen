// Wind field compute shader: pressure-based wind from terrain + continentality.
// 4-mode pipeline produces physically-derived wind that varies by longitude.
// Includes cube_sphere.wgsl and noise.wgsl (concatenated at load time).
//
// Mode 0: Init continentality (ocean=0, land=1)
// Mode 1: Smooth continentality (diffuse, ocean stays 0)
// Mode 2: Pressure field (7-term: ITCZ, subtropical, continental thermal, etc.)
// Mode 3: Wind from pressure gradient + Coriolis deflection

struct WindFieldParams {
    face: u32,
    resolution: u32,
    mode: u32,
    seed: u32,
    ocean_level: f32,
    axial_tilt_rad: f32,
    season: f32,           // 0=winter, 0.5=equinox, 1=summer
    smooth_weight: f32,    // smoothing strength per iteration (0.15 typical)
    rotation_rate: f32,    // relative to Earth (1.0 = 24h, 0.5 = 48h, 2.0 = 12h)
    base_temp_c: f32,      // planet mean temperature °C (15 = Earth)
    atm_pressure: f32,     // atmospheric pressure in bar (1.0 = Earth)
    _pad0: u32,
}

@group(0) @binding(0) var<uniform> params: WindFieldParams;
@group(0) @binding(1) var<storage, read> src: array<f32>;     // source data (all 6 faces)
@group(0) @binding(2) var<storage, read_write> dst: array<f32>; // destination (all 6 faces)
@group(0) @binding(3) var<storage, read> height_data: array<f32>; // terrain height (all 6 faces)

const PI: f32 = 3.14159265;
const DEG: f32 = 0.01745329; // PI / 180

fn smooth_step(edge0: f32, edge1: f32, x: f32) -> f32 {
    let t = clamp((x - edge0) / (edge1 - edge0), 0.0, 1.0);
    return t * t * (3.0 - 2.0 * t);
}

// === Cross-face sampling with bilinear interpolation ===

fn sphere_to_face_uv(dir: vec3<f32>) -> vec3<f32> {
    let abs_dir = abs(dir);
    var face_idx: f32;
    var u: f32;
    var v: f32;

    if (abs_dir.x >= abs_dir.y && abs_dir.x >= abs_dir.z) {
        if (dir.x > 0.0) {
            face_idx = 0.0; u = -dir.z / abs_dir.x; v = -dir.y / abs_dir.x;
        } else {
            face_idx = 1.0; u = dir.z / abs_dir.x; v = -dir.y / abs_dir.x;
        }
    } else if (abs_dir.y >= abs_dir.x && abs_dir.y >= abs_dir.z) {
        if (dir.y > 0.0) {
            face_idx = 2.0; u = dir.x / abs_dir.y; v = dir.z / abs_dir.y;
        } else {
            face_idx = 3.0; u = dir.x / abs_dir.y; v = -dir.z / abs_dir.y;
        }
    } else {
        if (dir.z > 0.0) {
            face_idx = 4.0; u = dir.x / abs_dir.z; v = -dir.y / abs_dir.z;
        } else {
            face_idx = 5.0; u = -dir.x / abs_dir.z; v = -dir.y / abs_dir.z;
        }
    }
    return vec3<f32>(face_idx, u * 0.5 + 0.5, v * 0.5 + 0.5);
}

fn sample_src(dir: vec3<f32>) -> f32 {
    let fuv = sphere_to_face_uv(dir);
    let face = u32(fuv.x);
    let res = params.resolution;
    // Bilinear interpolation prevents discontinuities at cubemap face boundaries.
    let fx = fuv.y * f32(res - 1u);
    let fy = fuv.z * f32(res - 1u);
    let x0 = clamp(u32(fx), 0u, res - 2u);
    let y0 = clamp(u32(fy), 0u, res - 2u);
    let tx = fx - f32(x0);
    let ty = fy - f32(y0);
    let base = face * res * res;
    let v00 = src[base + y0 * res + x0];
    let v10 = src[base + y0 * res + x0 + 1u];
    let v01 = src[base + (y0 + 1u) * res + x0];
    let v11 = src[base + (y0 + 1u) * res + x0 + 1u];
    return mix(mix(v00, v10, tx), mix(v01, v11, tx), ty);
}

fn sample_height(dir: vec3<f32>) -> f32 {
    let fuv = sphere_to_face_uv(dir);
    let face = u32(fuv.x);
    let res = params.resolution;
    let fx = fuv.y * f32(res - 1u);
    let fy = fuv.z * f32(res - 1u);
    let x0 = clamp(u32(fx), 0u, res - 2u);
    let y0 = clamp(u32(fy), 0u, res - 2u);
    let tx = fx - f32(x0);
    let ty = fy - f32(y0);
    let base = face * res * res;
    let v00 = height_data[base + y0 * res + x0];
    let v10 = height_data[base + y0 * res + x0 + 1u];
    let v01 = height_data[base + (y0 + 1u) * res + x0];
    let v11 = height_data[base + (y0 + 1u) * res + x0 + 1u];
    return mix(mix(v00, v10, tx), mix(v01, v11, tx), ty);
}

// === Mode 0: Initialize continentality ===

fn init_continentality(pos: vec3<f32>, idx: u32) {
    let h = height_data[idx];
    // Ocean = 0, Land = 1
    dst[idx] = select(0.0, 1.0, h > params.ocean_level);
}

// === Mode 1: Smooth continentality ===
// Iterative diffusion: each land cell averages with neighbors.
// Ocean cells are clamped to 0 (act as sinks).
// After ~40 iterations, coastal land ≈ 0.1, deep interior ≈ 0.8-1.0.

fn smooth_continentality(pos: vec3<f32>, idx: u32) {
    let h = height_data[idx];
    if (h <= params.ocean_level) {
        dst[idx] = 0.0; // Ocean always 0
        return;
    }

    let current = src[idx];
    let res = params.resolution;

    // Build tangent-plane offsets for 4 cardinal neighbors
    var up_ref = vec3<f32>(0.0, 1.0, 0.0);
    if (abs(pos.y) > 0.95) { up_ref = vec3<f32>(1.0, 0.0, 0.0); }
    let east = normalize(cross(up_ref, pos));
    let north = normalize(cross(pos, east));
    let step = 1.2 / f32(res); // ~1 texel in sphere coords

    let n0 = sample_src(normalize(pos + east * step));
    let n1 = sample_src(normalize(pos - east * step));
    let n2 = sample_src(normalize(pos + north * step));
    let n3 = sample_src(normalize(pos - north * step));

    let avg = (n0 + n1 + n2 + n3) * 0.25;
    let w = params.smooth_weight;
    dst[idx] = current * (1.0 - w) + avg * w;
}

// === Mode 2: Pressure field ===
// Rotation-rate-dependent cell boundaries (Kaspi & Showman 2015).
// Temperature-dependent Hadley width (+1° per 4°C, reverses at 21°C).
// Pressure-dependent wind scaling from ExoPlaSim data.

// Compute Hadley cell top latitude from rotation rate and temperature.
// Kaspi & Showman: Earth (Omega=1) → 30°; slow rotation → wider cells.
// Temperature: widens 1° per 4°C warming up to 21°C global mean.
fn hadley_top_lat() -> f32 {
    let omega = max(params.rotation_rate, 0.1);
    // Base from rotation: 30°/Omega^0.3, capped at 70°
    var base = min(30.0 / pow(omega, 0.3), 70.0);
    // Temperature adjustment: +1° per 4°C above 15°C, reverses above 21°C
    // (melting ice caps reduce pole-equator ΔT → Hadley cell shrinks back)
    let temp_c = params.base_temp_c;
    if (temp_c <= 21.0) {
        let temp_excess = clamp(temp_c - 15.0, -20.0, 6.0);
        base += temp_excess * 0.25;
    } else {
        let overshoot = clamp(temp_c - 21.0, 0.0, 14.0);
        base += 1.5 - overshoot * 0.25; // peaks at 21°C (+1.5°), shrinks above
    }
    return clamp(base, 15.0, 70.0);
}

// Subpolar low latitude from rotation rate
fn subpolar_lat() -> f32 {
    let omega = max(params.rotation_rate, 0.1);
    return min(60.0 / pow(omega, 0.2), 80.0);
}

fn compute_pressure(pos: vec3<f32>, idx: u32) {
    let continentality = src[idx]; // from smoothed continentality
    let height = height_data[idx];

    let tilt = params.axial_tilt_rad;
    let tilted_y = pos.y * cos(tilt) + pos.z * sin(tilt);
    let lat = asin(clamp(tilted_y, -1.0, 1.0));
    let lat_deg = lat / DEG;
    let abs_lat_deg = abs(lat_deg);

    let season_sign = select(-1.0, 1.0, params.season > 0.5);

    // Rotation-dependent cell boundaries
    let hadley_lat = hadley_top_lat();
    let polar_lat = subpolar_lat();

    var pressure = 1013.0;

    // (a) ITCZ low — longitude-varying, follows thermal equator
    // Monsoon: ITCZ shifts 15-20° poleward over large continents in summer
    let so = vec3<f32>(f32(params.seed), fract(f32(params.seed) * 0.001618) * 89.0, 0.0);
    let itcz_lon_noise = snoise(pos * 1.5 + so) * 5.0;
    // Stronger continent pull: up to 15° poleward (monsoon)
    let monsoon_pull = continentality * 15.0 * season_sign;
    let itcz_base = 5.0 * season_sign * (tilt / (23.4 * DEG)); // scale with tilt
    let itcz_lat = itcz_base + itcz_lon_noise + monsoon_pull;
    let d_itcz = lat_deg - itcz_lat;
    pressure -= 15.0 * exp(-0.5 * pow(d_itcz / 8.0, 2.0));

    // (b) Subtropical highs at ±hadley_lat — weakened over hot land
    // Wide Gaussians (sigma 14°) for smooth pressure transitions → no sharp wind lines
    let season_shift = season_sign * 5.0;
    let nh_sub = hadley_lat + season_shift;
    let sh_sub = -(hadley_lat - season_shift);
    let high_intensity = 12.0 * (1.0 - 0.35 * continentality);
    pressure += high_intensity * exp(-0.5 * pow((lat_deg - nh_sub) / 14.0, 2.0));
    pressure += high_intensity * exp(-0.5 * pow((lat_deg - sh_sub) / 14.0, 2.0));

    // (c) Subpolar lows at ±polar_lat — wide Gaussians
    pressure -= 10.0 * exp(-0.5 * pow((lat_deg - polar_lat) / 14.0, 2.0));
    pressure -= 10.0 * exp(-0.5 * pow((lat_deg + polar_lat) / 14.0, 2.0));

    // (d) Polar highs at ±85°
    pressure += 8.0 * exp(-0.5 * pow((lat_deg - 85.0) / 8.0, 2.0));
    pressure += 8.0 * exp(-0.5 * pow((lat_deg + 85.0) / 8.0, 2.0));

    // (e) Continental thermal modifier (monsoon driver)
    let continental_scale = smooth_step(0.2, 0.5, continentality);
    if (continental_scale > 0.001) {
        let lat_factor = smooth_step(15.0, 30.0, abs_lat_deg)
                       * smooth_step(90.0, 60.0, abs_lat_deg);
        let is_summer = (season_sign > 0.0 && lat > 0.0) || (season_sign < 0.0 && lat < 0.0);
        if (is_summer) {
            pressure -= 10.0 * lat_factor * continental_scale; // thermal low
        } else {
            pressure += 14.0 * lat_factor * continental_scale; // thermal high (Siberian)
        }
    }

    // (f) Semi-permanent ocean pressure cells — breaks latitude bands over water.
    // On Earth, subtropical highs cluster into 3-5 distinct cells per hemisphere
    // (Azores, Pacific, etc.) rather than forming a continuous belt.
    // Low-freq noise seeded by position creates persistent cell structure.
    if (continentality < 0.15) {
        // Subtropical cell splitting: noise modulates high intensity by longitude
        let cell_noise = snoise(pos * 2.5 + so * 0.7 + vec3<f32>(50.0, 0.0, 0.0)) * 0.5
                       + snoise(pos * 1.2 + so * 0.4 + vec3<f32>(0.0, 70.0, 0.0)) * 0.3;
        let sub_belt = smooth_step(hadley_lat - 12.0, hadley_lat, abs_lat_deg)
                     * smooth_step(hadley_lat + 15.0, hadley_lat + 5.0, abs_lat_deg);
        // Cells: ±5 hPa variation within subtropical belt
        pressure += cell_noise * 5.0 * sub_belt;

        // Mid-latitude storm track undulation: subpolar lows meander
        let storm_track = smooth_step(polar_lat - 15.0, polar_lat - 5.0, abs_lat_deg)
                        * smooth_step(polar_lat + 10.0, polar_lat, abs_lat_deg);
        let meander = snoise(pos * 3.0 + so * 1.1 + vec3<f32>(30.0, 0.0, 50.0));
        pressure += meander * 4.0 * storm_track;
    }

    // (g) Elevation (barometric) — mild
    let elev_km = max(height - params.ocean_level, 0.0) * 5.0; // rough height mapping
    pressure -= 3.0 * elev_km;

    // (h) Noise perturbation (±3 hPa, slightly stronger than before)
    pressure += snoise(pos * 2.0 + so * 0.5 + vec3<f32>(100.0, 0.0, 0.0)) * 3.0;

    dst[idx] = pressure;
}

// === Mode 3: Direct analytical wind (no pressure gradients) ===
// Computes wind directly from latitude + noise + terrain deflection.
// Smooth by construction: uses smooth_step for cell boundaries, not finite differences.
// src buffer contains continentality (for monsoon/land effects).

fn compute_wind(pos: vec3<f32>, idx: u32) {
    let res = params.resolution;

    // Tilt setup — needed by everything below
    let tilt = params.axial_tilt_rad;
    let ct = cos(tilt);
    let st = sin(tilt);

    // Local east/north frame aligned with TILTED pole.
    // Smooth blend near poles prevents the frame singularity ring.
    let tilted_pole = vec3<f32>(0.0, ct, st);
    let pole_closeness = abs(dot(pos, tilted_pole));
    let up_blend = smooth_step(0.80, 0.98, pole_closeness);
    let up_ref = normalize(mix(tilted_pole, vec3<f32>(1.0, 0.0, 0.0), up_blend));
    let east = normalize(cross(up_ref, pos));
    let north = normalize(cross(pos, east));

    // Latitude in tilted frame
    let tilted_y = pos.y * ct + pos.z * st;
    let lat = asin(clamp(tilted_y, -1.0, 1.0));
    let hemisphere = sign(lat + 0.0001);
    // Position rotated into tilted frame for noise alignment
    let tilted_pos = vec3<f32>(pos.x, pos.y * ct + pos.z * st, -pos.y * st + pos.z * ct);

    // Rotation-dependent cell boundaries (same as preview shader)
    let hadley = hadley_top_lat();
    let polar = subpolar_lat();

    // Seasonal shift
    let season_shift = params.axial_tilt_rad * ((params.season - 0.5) * 2.0) * 0.4;
    let shifted_lat = lat - season_shift;

    // Cell boundary wobble: shifts effective latitude to break concentric rings.
    // Three components: noise + continental (monsoon) + elevation (orographic).
    let so = vec3<f32>(f32(params.seed), fract(f32(params.seed) * 0.001618) * 89.0, 0.0);
    let pole_boost = 1.0 + 5.0 * abs(sin(lat));
    let noise_wobble = snoise(tilted_pos * (2.0 * pole_boost) + so + vec3<f32>(150.0, 0.0, 0.0)) * 8.0;

    // Continental wobble (monsoon): Hadley cell extends poleward over large continents.
    // Reads smoothed continentality (80-iteration diffusion = no coastline edges).
    // Has a base effect even at equinox (thermal forcing) + seasonal amplification.
    let continentality = sample_src(pos);
    let season_amp = (params.season - 0.5) * 2.0; // -1 to +1
    let thermal_base = continentality * 2.0; // always-on: continents are warmer → shift
    let seasonal_boost = continentality * 5.0 * season_amp * sign(lat + 0.0001);
    let continental_wobble = thermal_base + seasonal_boost;

    // Elevation wobble: major mountain ranges (>2km) shift cell boundaries.
    // High threshold ensures flat coasts contribute zero → no coastline ghosting.
    let height = height_data[idx];
    let elevation = max(height - params.ocean_level, 0.0);
    let mountain_wobble = smooth_step(0.06, 0.20, elevation) * 5.0;

    let lat_deg = abs(shifted_lat) / DEG + noise_wobble + continental_wobble + mountain_wobble;

    // === Three-cell zonal wind (WIDE smooth_step for gentle transitions) ===
    // Wide transitions (15-20°) prevent sharp rings visible from pole view.
    let trade_top = hadley * 0.6;
    let trade_full = hadley * 1.2;
    let west_start = hadley * 0.7;
    let west_end = polar * 0.85;
    let polar_start = polar * 0.8;

    let trade = (1.0 - smooth_step(trade_top, trade_full, lat_deg)) * -0.8;
    let westerly = smooth_step(west_start, west_start + 15.0, lat_deg)
                 * (1.0 - smooth_step(west_end - 8.0, west_end + 12.0, lat_deg)) * 0.85;
    let polar_east = smooth_step(polar_start, polar_start + 15.0, lat_deg) * -0.45;
    var wind_e = trade + westerly + polar_east;

    // Meridional component (also widened)
    let hadley_m = -smooth_step(5.0, hadley * 0.6, lat_deg)
                  * (1.0 - smooth_step(hadley * 0.8, hadley * 1.3, lat_deg)) * 0.35;
    let ferrel_center = (hadley + polar) * 0.5;
    let ferrel_m = smooth_step(ferrel_center - 15.0, ferrel_center, lat_deg)
                  * (1.0 - smooth_step(ferrel_center, ferrel_center + 15.0, lat_deg)) * 0.25;
    var wind_n = (hadley_m + ferrel_m) * hemisphere;

    // === Longitude variation (gentle speed noise, boundary wobble handles ring-breaking) ===
    let lon_var = snoise(tilted_pos * 2.0 + so + vec3<f32>(100.0, 0.0, 0.0));
    let lon_var2 = snoise(tilted_pos * 1.0 + so + vec3<f32>(0.0, 100.0, 0.0));
    wind_e += lon_var * 0.10;
    wind_n += lon_var2 * 0.08;

    // Mountain speed boost: wind accelerates near high terrain (venturi/gap wind)
    let mountain_speed = 1.0 + smooth_step(0.08, 0.20, elevation) * 0.3;

    // Pressure-dependent speed scaling
    let p = max(params.atm_pressure, 0.05);
    let speed_scale = pow(1.0 / p, 0.15) * mountain_speed;

    // Convert to 3D tangent vector
    let wind_3d = (east * wind_e + north * wind_n) * speed_scale;

    let base = idx * 3u;
    dst[base] = wind_3d.x;
    dst[base + 1u] = wind_3d.y;
    dst[base + 2u] = wind_3d.z;
}

// === Main dispatch ===

@compute @workgroup_size(16, 16)
fn main(@builtin(global_invocation_id) id: vec3<u32>) {
    let res = params.resolution;
    if (id.x >= res || id.y >= res) { return; }

    let uv = vec2<f32>(
        f32(id.x) / f32(res - 1u),
        f32(id.y) / f32(res - 1u)
    );
    let pos = cube_to_sphere(params.face, uv);
    let face_offset = params.face * res * res;
    let idx = face_offset + id.y * res + id.x;

    switch (params.mode) {
        case 0u: { init_continentality(pos, idx); }
        case 1u: { smooth_continentality(pos, idx); }
        case 2u: { compute_pressure(pos, idx); }
        case 3u: { compute_wind(pos, idx); }
        default: {}
    }
}
