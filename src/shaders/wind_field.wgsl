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
    season: f32,         // 0=winter, 0.5=equinox, 1=summer
    smooth_weight: f32,  // smoothing strength per iteration (0.15 typical)
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

// === Cross-face sampling (same pattern as cloud_advect.wgsl) ===

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
    let px = clamp(u32(fuv.y * f32(res - 1u)), 0u, res - 1u);
    let py = clamp(u32(fuv.z * f32(res - 1u)), 0u, res - 1u);
    return src[face * res * res + py * res + px];
}

fn sample_height(dir: vec3<f32>) -> f32 {
    let fuv = sphere_to_face_uv(dir);
    let face = u32(fuv.x);
    let res = params.resolution;
    let px = clamp(u32(fuv.y * f32(res - 1u)), 0u, res - 1u);
    let py = clamp(u32(fuv.z * f32(res - 1u)), 0u, res - 1u);
    return height_data[face * res * res + py * res + px];
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
// 7-term pressure model inspired by planet_heightmap_generation.

fn compute_pressure(pos: vec3<f32>, idx: u32) {
    let continentality = src[idx]; // from smoothed continentality
    let height = height_data[idx];
    let is_ocean = height <= params.ocean_level;

    let tilt = params.axial_tilt_rad;
    let tilted_y = pos.y * cos(tilt) + pos.z * sin(tilt);
    let lat = asin(clamp(tilted_y, -1.0, 1.0));
    let lat_deg = lat / DEG;
    let abs_lat_deg = abs(lat_deg);

    // Longitude for ITCZ variation
    let lon = atan2(pos.x, pos.z);

    let season_sign = select(-1.0, 1.0, params.season > 0.5); // NH summer = +1

    var pressure = 1013.0; // baseline hPa

    // (a) ITCZ low — thermal equator shifts with season + longitude variation
    // ITCZ bows poleward over continents (monsoon effect via noise + continentality)
    let so = vec3<f32>(f32(params.seed), fract(f32(params.seed) * 0.001618) * 89.0, 0.0);
    let itcz_lon_noise = snoise(pos * 1.5 + so) * 4.0; // ±4° longitude variation
    let itcz_continent_pull = continentality * 8.0 * season_sign; // land pulls ITCZ poleward in summer
    let itcz_base = 5.0 * season_sign; // base 5° toward summer pole
    let itcz_lat = itcz_base + itcz_lon_noise + itcz_continent_pull;
    let d_itcz = lat_deg - itcz_lat;
    pressure -= 15.0 * exp(-0.5 * (d_itcz / 8.0) * (d_itcz / 8.0));

    // (b) Subtropical highs at ±30° — weakened over hot land
    let season_shift = season_sign * 5.0;
    let nh_sub = 30.0 + season_shift;
    let sh_sub = -(30.0 - season_shift);
    let high_intensity = 12.0 * (1.0 - 0.3 * continentality);
    pressure += high_intensity * exp(-0.5 * pow((lat_deg - nh_sub) / 10.0, 2.0));
    pressure += high_intensity * exp(-0.5 * pow((lat_deg - sh_sub) / 10.0, 2.0));

    // (c) Subpolar lows at ±60°
    pressure -= 10.0 * exp(-0.5 * pow((lat_deg - 60.0) / 10.0, 2.0));
    pressure -= 10.0 * exp(-0.5 * pow((lat_deg + 60.0) / 10.0, 2.0));

    // (d) Polar highs at ±85°
    pressure += 8.0 * exp(-0.5 * pow((lat_deg - 85.0) / 8.0, 2.0));
    pressure += 8.0 * exp(-0.5 * pow((lat_deg + 85.0) / 8.0, 2.0));

    // (e) Continental thermal modifier
    // Summer: thermal low over hot continent, Winter: thermal high
    let continental_scale = smooth_step(0.2, 0.5, continentality);
    if (continental_scale > 0.001) {
        // Latitude-dependent thermal effect (strongest at 30-60°)
        let lat_factor = smooth_step(15.0, 30.0, abs_lat_deg)
                       * smooth_step(90.0, 60.0, abs_lat_deg);
        let is_summer = (season_sign > 0.0 && lat > 0.0) || (season_sign < 0.0 && lat < 0.0);
        if (is_summer) {
            pressure -= 10.0 * lat_factor * continental_scale; // thermal low
        } else {
            pressure += 14.0 * lat_factor * continental_scale; // thermal high (Siberian)
        }
    }

    // (f) Elevation (barometric) — mild
    let elev_km = max(height - params.ocean_level, 0.0) * 5.0; // rough height mapping
    pressure -= 3.0 * elev_km;

    // (g) Noise perturbation (±2 hPa)
    pressure += snoise(pos * 2.0 + so * 0.5 + vec3<f32>(100.0, 0.0, 0.0)) * 2.0;

    dst[idx] = pressure;
}

// === Mode 3: Wind from pressure gradient + Coriolis ===

fn compute_wind(pos: vec3<f32>, idx: u32) {
    let res = params.resolution;

    // Build local east/north frame
    var up_ref = vec3<f32>(0.0, 1.0, 0.0);
    if (abs(pos.y) > 0.95) { up_ref = vec3<f32>(1.0, 0.0, 0.0); }
    let east = normalize(cross(up_ref, pos));
    let north = normalize(cross(pos, east));

    // Finite differences for pressure gradient
    let step = 1.5 / f32(res); // ~1.5 texels for smoother gradient
    let p_e = sample_src(normalize(pos + east * step));
    let p_w = sample_src(normalize(pos - east * step));
    let p_n = sample_src(normalize(pos + north * step));
    let p_s = sample_src(normalize(pos - north * step));

    // Pressure gradient force: from high to low = negative gradient
    let pgf_e = -(p_e - p_w) / (2.0 * step);
    let pgf_n = -(p_n - p_s) / (2.0 * step);

    // Coriolis deflection
    let tilt = params.axial_tilt_rad;
    let tilted_y = pos.y * cos(tilt) + pos.z * sin(tilt);
    let sin_lat = clamp(tilted_y, -1.0, 1.0);
    let abs_sin_lat = abs(sin_lat);

    // Geostrophic angle: 0° at equator → 70° at ≥5° latitude
    let geo_angle = 70.0 * DEG * smooth_step(0.0, sin(5.0 * DEG), abs_sin_lat);

    // Surface friction: 20° back toward low pressure
    let friction_angle = 20.0 * DEG;

    // NH: clockwise deflection (negative angle), SH: counterclockwise (positive)
    let hemisphere_sign = select(1.0, -1.0, sin_lat >= 0.0);
    let total_angle = hemisphere_sign * (geo_angle - friction_angle);

    let cos_a = cos(total_angle);
    let sin_a = sin(total_angle);

    // Rotate PGF and apply friction speed reduction (0.6×)
    let wind_e = (pgf_e * cos_a - pgf_n * sin_a) * 0.6;
    let wind_n = (pgf_e * sin_a + pgf_n * cos_a) * 0.6;

    // Convert to 3D tangent vector for advection shader
    let wind_3d = east * wind_e + north * wind_n;

    // Store as 3 components: [x, y, z] at offset 3*idx
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
