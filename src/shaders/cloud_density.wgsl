// Shared weather-driven density functions. Preview and export compile this unchanged.
fn weather_density(dir: vec3<f32>, height_fraction: f32) -> f32 {
    let weather = textureSample(weather_tex, height_sampler, normalize(dir));
    if (weather.r <= 0.0) { return 0.0; }
    let vertical = smooth_step(0.0, 0.18, height_fraction) * (1.0 - smooth_step(0.72, 1.0, height_fraction));
    let formation = snoise(normalize(dir) * 7.0 + vec3<f32>(uniforms.cloud_seed * 0.13));
    let erosion = smooth_step(-0.45, 0.25, formation + weather.a * 0.35);
    return weather.r * vertical * erosion;
}

fn weather_cirrus_density(dir: vec3<f32>) -> f32 {
    let weather = textureSample(weather_tex, height_sampler, normalize(dir));
    let streaks = smooth_step(0.35, 0.7, snoise(normalize(dir) * 18.0 + vec3<f32>(uniforms.cloud_seed)));
    return weather.r * weather.a * streaks * 0.22;
}
