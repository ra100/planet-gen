// Cloud density map for export.
// Computes per-pixel cloud density using climate + noise pipeline.
// Includes cube_sphere.wgsl and noise.wgsl at load time.

struct CloudMapParams {
    face: u32,
    resolution: u32,
    seed: u32,
    base_temp_c: f32,
    ocean_level: f32,
    ocean_fraction: f32,
    axial_tilt_rad: f32,
    season: f32,
    cloud_coverage: f32,
    cloud_type: f32,
    tile_offset_x: u32,
    tile_offset_y: u32,
    full_resolution: u32,
    _pad0: u32,
    _pad1: u32,
    _pad2: u32,
}

@group(0) @binding(0) var<storage, read> heightmap: array<f32>;
@group(0) @binding(1) var<storage, read_write> cloud_out: array<f32>;
@group(0) @binding(2) var<uniform> params: CloudMapParams;

fn smooth_step(edge0: f32, edge1: f32, x: f32) -> f32 {
    let t = clamp((x - edge0) / (edge1 - edge0), 0.0, 1.0);
    return t * t * (3.0 - 2.0 * t);
}

fn cloud_remap(value: f32, old_min: f32, old_max: f32, new_min: f32, new_max: f32) -> f32 {
    return new_min + (clamp(value, old_min, old_max) - old_min)
           / max(old_max - old_min, 0.001) * (new_max - new_min);
}

// Simplified temperature for cloud coverage threshold
fn compute_temp(sphere_pos: vec3<f32>, height: f32) -> f32 {
    let tilt = params.axial_tilt_rad;
    let tilted_y = sphere_pos.y * cos(tilt) + sphere_pos.z * sin(tilt);
    let effective_lat = asin(clamp(tilted_y, -1.0, 1.0));
    let season_angle = (params.season - 0.5) * 2.0;
    let sub_solar_lat = tilt * season_angle;
    let thermal_lat = effective_lat - sub_solar_lat;
    let thermal_deg = min(abs(thermal_lat) * 180.0 / 3.14159, 90.0);
    let lat_norm = thermal_deg / 90.0;
    let temp_drop = 50.0 * (0.4 * lat_norm + 0.6 * lat_norm * lat_norm);
    let base_temp = 30.0 - temp_drop + (params.base_temp_c - 15.0);
    let land_frac = max(height - params.ocean_level, 0.0) / max(1.0 - params.ocean_level, 0.01);
    let lapse = -6.5 * land_frac * 5.0;
    return base_temp + lapse + snoise(sphere_pos * 3.0) * 3.0;
}

// Wind direction for cloud stretching
fn wind_vec(latitude_rad: f32) -> vec3<f32> {
    let hemisphere = sign(latitude_rad + 0.0001);
    let season_shift = params.axial_tilt_rad * ((params.season - 0.5) * 2.0) * 0.4;
    let shifted_lat = latitude_rad - season_shift;
    let lat_deg = abs(shifted_lat) * 180.0 / 3.14159;

    let trade = (1.0 - smooth_step(22.0, 33.0, lat_deg)) * -0.8;
    let westerly = smooth_step(28.0, 42.0, lat_deg) * (1.0 - smooth_step(55.0, 68.0, lat_deg)) * 0.85;
    let polar_east = smooth_step(62.0, 75.0, lat_deg) * -0.45;
    let wind_x = trade + westerly + polar_east;

    let hadley_m = -smooth_step(8.0, 25.0, lat_deg) * (1.0 - smooth_step(28.0, 38.0, lat_deg)) * 0.35;
    let ferrel_m = smooth_step(38.0, 48.0, lat_deg) * (1.0 - smooth_step(55.0, 65.0, lat_deg)) * 0.25;
    let wind_y = (hadley_m + ferrel_m) * hemisphere;

    return normalize(vec3<f32>(wind_x, wind_y, 0.1));
}

// Hadley cell moisture for coverage threshold
fn hadley_moisture(latitude_rad: f32) -> f32 {
    let lat_deg = abs(latitude_rad) * 180.0 / 3.14159;
    let itcz_wet = exp(-lat_deg * lat_deg / 200.0) * 200.0;
    let subtropical_dry = -80.0 * exp(-((lat_deg - 28.0) * (lat_deg - 28.0)) / 60.0);
    let polar_front_wet = 90.0 * exp(-((lat_deg - 50.0) * (lat_deg - 50.0)) / 200.0);
    let polar_dry = -60.0 * smooth_step(65.0, 85.0, lat_deg);
    return max(itcz_wet + subtropical_dry + polar_front_wet + polar_dry + 90.0, 10.0);
}

@compute @workgroup_size(16, 16)
fn main(@builtin(global_invocation_id) id: vec3<u32>) {
    let res = params.resolution;
    if (id.x >= res || id.y >= res) { return; }

    let full_res = params.full_resolution;
    let global_x = params.tile_offset_x + id.x;
    let global_y = params.tile_offset_y + id.y;
    let height_idx = global_y * full_res + global_x;
    let height = heightmap[height_idx];
    let is_ocean = height < params.ocean_level;

    let uv = vec2<f32>(
        f32(global_x) / f32(full_res - 1u),
        f32(global_y) / f32(full_res - 1u)
    );
    let sphere_pos = cube_to_sphere(params.face, uv);

    let cov_slider = params.cloud_coverage;
    if (cov_slider <= 0.0) {
        cloud_out[id.y * res + id.x] = 0.0;
        return;
    }
    let coverage = pow(cov_slider, 0.8);

    // Seed offset
    let s = f32(params.seed);
    let seed_off = vec3<f32>(s, fract(s * 0.001618) * 89.0, fract(s * 0.002618) * 83.0);

    // Latitude for climate/wind
    let tilt = params.axial_tilt_rad;
    let tilted_y = sphere_pos.y * cos(tilt) + sphere_pos.z * sin(tilt);
    let cloud_lat = asin(clamp(tilted_y, -1.0, 1.0));
    let cloud_lat_deg = abs(cloud_lat) * 180.0 / 3.14159;

    // Wind-aligned stretching
    let wind = wind_vec(cloud_lat);
    let wind_speed = length(vec2<f32>(wind.x, wind.y));
    let tangent_wind = normalize(wind - sphere_pos * dot(wind, sphere_pos));
    let wind_stretch = tangent_wind * wind_speed * 0.08;

    // Cloud type region selector
    let region_raw = snoise(sphere_pos * 0.6 + seed_off * 0.2 + vec3<f32>(131.0, 71.0, 0.0)) * 0.7
                   + snoise(sphere_pos * 1.2 + seed_off * 0.3 + vec3<f32>(71.0, 131.0, 0.0)) * 0.3;
    let itcz_factor = exp(-cloud_lat_deg * cloud_lat_deg / 150.0);
    let polar_c = smooth_step(55.0, 70.0, cloud_lat_deg);
    let lat_type_bias = itcz_factor * 0.4 - polar_c * 0.3
        - smooth_step(15.0, 28.0, cloud_lat_deg) * smooth_step(38.0, 28.0, cloud_lat_deg) * 0.3;
    let region_type = clamp(region_raw * 0.4 + 0.5 + lat_type_bias + params.cloud_type * 0.2, 0.0, 1.0);

    // Cloud noise — stratus + cumulus + thin blend
    let p_base = sphere_pos * 7.0 + seed_off;

    // Stratus: flowing sheets
    let s_warp = vec3<f32>(
        snoise(p_base * 0.5 + vec3<f32>(31.7, 0.0, 0.0)),
        snoise(p_base * 0.5 + vec3<f32>(0.0, 47.3, 0.0)),
        snoise(p_base * 0.5 + vec3<f32>(0.0, 0.0, 73.1))
    ) * 0.5 + wind_stretch;
    let s_p = p_base + s_warp;
    let stratus_val = (snoise(s_p) * 0.50 + snoise(s_p * 2.1) * 0.25
        + snoise(s_p * 4.2) * 0.13 + snoise(s_p * 8.4) * 0.07
        + snoise(s_p * 16.8) * 0.05) * 0.5 + 0.5;

    // Cumulus: puffy blobs
    let c_p = p_base + vec3<f32>(13.7, 7.3, 21.1) + wind_stretch * 0.5;
    var cumulus_val = 0.0;
    var c_freq = 1.0;
    var c_amp = 1.0;
    var c_amp_sum = 0.0;
    for (var i = 0; i < 5; i++) {
        let n = snoise(c_p * c_freq);
        cumulus_val += smooth_step(-0.1, 0.15, n) * n * c_amp;
        c_amp_sum += c_amp;
        c_freq *= 2.2;
        c_amp *= 0.45;
    }
    cumulus_val = cumulus_val / c_amp_sum;
    let cu_detail = snoise(c_p * 8.0) * 0.04;
    cumulus_val = pow(max(cumulus_val, 0.0), 0.9) * 1.3 + cu_detail * cumulus_val;

    // Thin/wispy
    let t_p = p_base * 0.8 + vec3<f32>(51.0, 23.0, 87.0) + wind_stretch * 2.0;
    let thin_val = (snoise(t_p) * 0.7 + snoise(t_p * 3.0) * 0.3) * 0.3 + 0.3;

    // Blend cloud types
    let rt = region_type;
    let w_stratus = smooth_step(0.65, 0.20, rt);
    let w_cumulus = smooth_step(0.35, 0.80, rt);
    let thin_mix = smooth_step(0.35, 0.0, rt) * 0.5 + 0.12;
    let w_total = max(w_stratus + w_cumulus + thin_mix, 0.01);
    let noise_val = (w_stratus * stratus_val + w_cumulus * cumulus_val + thin_mix * thin_val) / w_total;

    // Weather regions
    let weather_region = snoise(sphere_pos * 1.5 + seed_off * 0.3 + vec3<f32>(77.0, 0.0, 0.0));
    let noise_val_w = noise_val * (0.75 + 0.25 * (weather_region * 0.5 + 0.5));

    // Climate coverage threshold
    let season_angle = (params.season - 0.5) * 2.0;
    let sub_solar = tilt * season_angle;
    let thermal_lat = cloud_lat - sub_solar;
    let ocean_scale = 0.25 + 0.75 * params.ocean_fraction;
    let moisture = hadley_moisture(thermal_lat) * ocean_scale;
    let moisture_norm = clamp(moisture / 300.0, 0.0, 1.0);

    let subtropical = smooth_step(15.0, 25.0, cloud_lat_deg) * smooth_step(40.0, 30.0, cloud_lat_deg);
    let midlat = smooth_step(30.0, 45.0, cloud_lat_deg) * smooth_step(65.0, 55.0, cloud_lat_deg);
    let lat_climate = itcz_factor * 0.30 - subtropical * 0.12 + midlat * 0.12 + polar_c * 0.08;
    let climate_coverage = (0.35 + lat_climate) * coverage + moisture_norm * 0.25;
    var local_coverage = max(climate_coverage, coverage * 0.85);

    // Mountain lift / foehn
    let land_factor = smooth_step(params.ocean_level - 0.03, params.ocean_level + 0.12, height);
    if (land_factor > 0.01) {
        let elev = max(height - params.ocean_level, 0.0);
        let mountain_lift = smooth_step(0.06, 0.25, elev);
        local_coverage += mountain_lift * 0.25 * land_factor;
    }

    let temp = compute_temp(sphere_pos, height);
    let convection_boost = smooth_step(15.0, 30.0, temp) * 0.06;
    local_coverage = clamp(local_coverage + convection_boost, 0.0, 1.0);

    // Weather scale variation
    let weather_scale = snoise(sphere_pos * 2.0 + seed_off * 0.2) * 0.15;
    let varied_noise = clamp(noise_val_w + weather_scale, 0.0, 1.0);

    // Schneider remap
    let remapped = cloud_remap(varied_noise, 1.0 - local_coverage, 1.0, 0.0, 1.0) * local_coverage;
    let thin_veil = max(varied_noise - 0.40, 0.0) * 0.4 * local_coverage;
    var density = max(remapped, thin_veil);
    density = pow(density, 0.8);

    cloud_out[id.y * res + id.x] = max(density, 0.0);
}
