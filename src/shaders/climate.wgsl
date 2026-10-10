// Shared planetary climate baseline. Concatenate after noise.wgsl.
// base_temp_c is a global reference temperature (Earth ~15 C), not the
// equatorial temperature. Terrain height units represent 5 km independently
// of sea level: changing water coverage must not stretch mountain altitude.

fn climate_latitude(pos: vec3<f32>, axial_tilt_rad: f32) -> f32 {
    let pole_component = pos.y * cos(axial_tilt_rad) + pos.z * sin(axial_tilt_rad);
    return asin(clamp(pole_component, -1.0, 1.0));
}

fn climate_thermal_latitude(pos: vec3<f32>, axial_tilt_rad: f32, season: f32) -> f32 {
    let subsolar_latitude = axial_tilt_rad * clamp((season - 0.5) * 2.0, -1.0, 1.0);
    return climate_latitude(pos, axial_tilt_rad) - subsolar_latitude;
}

fn climate_elevation_km(height: f32, ocean_level: f32) -> f32 {
    return max(height - ocean_level, 0.0) * 5.0;
}

fn climate_sea_level_temperature(
    pos: vec3<f32>, base_temp_c: f32, axial_tilt_rad: f32, season: f32,
) -> f32 {
    let thermal_latitude = climate_thermal_latitude(pos, axial_tilt_rad, season);
    let latitude_fraction = min(abs(thermal_latitude) / 1.57079632679, 1.0);
    let drop = 50.0 * (0.4 * latitude_fraction + 0.6 * latitude_fraction * latitude_fraction);
    // These stationary regional anomalies are shared with weather/export;
    // they do not change when the viewing direction or cloud seed changes.
    let local_anomaly = snoise(pos * 2.0) * 2.0;
    let regional_anomaly = snoise(pos * 0.6 + vec3<f32>(0.0, 400.0, 0.0)) * 4.0;
    return base_temp_c + 15.0 - drop + local_anomaly + regional_anomaly;
}

fn climate_temperature(
    pos: vec3<f32>, height: f32, ocean_level: f32,
    base_temp_c: f32, axial_tilt_rad: f32, season: f32,
) -> f32 {
    return climate_sea_level_temperature(pos, base_temp_c, axial_tilt_rad, season)
        - 6.5 * climate_elevation_km(height, ocean_level);
}

fn climate_hadley_moisture(latitude_rad: f32, hadley_lat: f32, polar_lat: f32) -> f32 {
    let lat_deg = abs(latitude_rad) * 180.0 / 3.14159;
    let itcz = 200.0 * exp(-lat_deg * lat_deg / 200.0);
    let subtropical = -80.0 * exp(-pow(lat_deg - hadley_lat, 2.0) / 60.0);
    let midlatitude = 90.0 * exp(-pow(lat_deg - (hadley_lat + polar_lat) * 0.5, 2.0) / 200.0);
    let polar = -60.0 * smoothstep(polar_lat + 5.0, polar_lat + 25.0, lat_deg);
    return max(itcz + subtropical + midlatitude + polar + 90.0, 10.0);
}

// Annual moisture used by both surface vegetation and land vapor supply.
fn climate_regional_moisture(
    pos: vec3<f32>, latitude: f32, hadley_lat: f32, polar_lat: f32,
    surface_seed: u32, ocean_fraction: f32,
) -> f32 {
    let ocean_scale = 0.25 + 0.75 * ocean_fraction;
    let local = snoise(pos * 3.0 + noise_seed_offset(surface_seed, 73u)) * 0.5;
    let baseline = climate_hadley_moisture(latitude, hadley_lat, polar_lat) * ocean_scale;
    let region = snoise(pos * 0.7 + noise_seed_offset(surface_seed, 74u));
    return (baseline * (0.55 + 0.45 * (local + 0.5)) + 50.0 * (local + 0.5) * ocean_scale)
        * (1.0 + region * 0.25) * (0.5 + ocean_fraction);
}
