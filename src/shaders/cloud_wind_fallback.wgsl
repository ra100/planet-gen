// Shared zero-wind cloud-detail fallback. `wind_height_sample` is supplied by
// each stage: fragment preview uses implicit LOD, compute export uses LOD 0.
fn preview_hadley_top() -> f32 {
    let omega = max(uniforms.rotation_rate, 0.1);
    var base = min(30.0 / pow(omega, 0.3), 70.0);
    let temp_c = uniforms.base_temp_c;
    if (temp_c <= 21.0) {
        base += clamp(temp_c - 15.0, -20.0, 6.0) * 0.25;
    } else {
        base += 1.5 - clamp(temp_c - 21.0, 0.0, 14.0) * 0.25;
    }
    return clamp(base, 15.0, 70.0);
}

fn preview_subpolar_lat() -> f32 {
    return min(60.0 / pow(max(uniforms.rotation_rate, 0.1), 0.2), 80.0);
}

fn wind_direction_vec(latitude_rad: f32) -> vec3<f32> {
    let hemisphere = sign(latitude_rad + 0.0001);
    let hadley_lat = preview_hadley_top();
    let polar_lat = preview_subpolar_lat();
    let trade_top = hadley_lat * 0.75;
    let trade_full = hadley_lat * 1.05;
    let west_start = hadley_lat * 0.9;
    let west_end = polar_lat * 0.92;
    let polar_start = polar_lat * 0.95;
    let season_shift = uniforms.axial_tilt_rad * ((uniforms.season - 0.5) * 2.0) * 0.4;
    let lat_deg = abs(latitude_rad - season_shift) * 180.0 / 3.14159;
    let trade = (1.0 - smooth_step(trade_top, trade_full, lat_deg)) * -0.8;
    let westerly = smooth_step(west_start, west_start + 10.0, lat_deg)
        * (1.0 - smooth_step(west_end - 5.0, west_end + 8.0, lat_deg)) * 0.85;
    let polar_east = smooth_step(polar_start, polar_start + 10.0, lat_deg) * -0.45;
    let hadley_meridional = -smooth_step(8.0, hadley_lat * 0.7, lat_deg)
        * (1.0 - smooth_step(hadley_lat * 0.9, hadley_lat * 1.2, lat_deg)) * 0.35;
    let ferrel_center = (hadley_lat + polar_lat) * 0.5;
    let ferrel_meridional = smooth_step(ferrel_center - 10.0, ferrel_center, lat_deg)
        * (1.0 - smooth_step(ferrel_center, ferrel_center + 10.0, lat_deg)) * 0.25;
    return normalize(vec3<f32>(trade + westerly + polar_east, (hadley_meridional + ferrel_meridional) * hemisphere, 0.1));
}

fn wind_direction_at(sphere_pos: vec3<f32>, latitude_rad: f32) -> vec3<f32> {
    let wind = wind_direction_vec(latitude_rad);
    let tangent_wind = normalize(wind - sphere_pos * dot(wind, sphere_pos));
    let perp = normalize(cross(sphere_pos, tangent_wind));
    let terrain_gradient = (wind_height_sample(normalize(sphere_pos + perp * 0.04))
        - wind_height_sample(normalize(sphere_pos - perp * 0.04))) * 3.0;
    return normalize(tangent_wind + perp * clamp(terrain_gradient, -0.4, 0.4));
}
