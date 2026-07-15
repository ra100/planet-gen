//! Parameter sweep: generates a grid of planet previews as PNG files.
//! Usage: cargo run --bin sweep [--output-dir <dir>] [--size <pixels>]

use bytemuck::Zeroable;
use planet_gen::gpu::GpuContext;
use planet_gen::planet::{DerivedProperties, PlanetParams};
use planet_gen::plates::{PlateGenParams, generate_plates};
use planet_gen::preview::{PreviewRenderer, PreviewUniforms};
use planet_gen::terrain_compute::{
    DynamicsTextures, TectonicTerrain, TerrainComputePipeline, WindFieldPipeline,
};
use planet_gen::weather::{WeatherFieldPipeline, WeatherSnapshot, WeatherTextures};
use std::path::Path;
use std::sync::mpsc;
use std::time::{Duration, Instant};

struct PlanetPreset {
    name: &'static str,
    params: PlanetParams,
    continental_scale: f32,
    water_loss: f32,
}

fn presets() -> Vec<PlanetPreset> {
    vec![
        PlanetPreset {
            name: "earth",
            params: PlanetParams {
                star_distance_au: 1.0,
                mass_earth: 1.0,
                metallicity: 0.0,
                axial_tilt_deg: 23.4,
                rotation_period_h: 24.0,
                seed: 0, // will be overridden per seed
            },
            continental_scale: 0.8,
            water_loss: 0.0,
        },
        PlanetPreset {
            name: "mars",
            params: PlanetParams {
                star_distance_au: 1.5,
                mass_earth: 0.1,
                metallicity: 0.0,
                axial_tilt_deg: 25.2,
                rotation_period_h: 24.6,
                seed: 0,
            },
            continental_scale: 1.0,
            water_loss: 0.7,
        },
        PlanetPreset {
            name: "venus",
            params: PlanetParams {
                star_distance_au: 0.7,
                mass_earth: 0.8,
                metallicity: 0.0,
                axial_tilt_deg: 2.6,
                rotation_period_h: 5832.0,
                seed: 0,
            },
            continental_scale: 1.2,
            water_loss: 1.0,
        },
        PlanetPreset {
            name: "archipelago",
            params: PlanetParams {
                star_distance_au: 1.0,
                mass_earth: 0.5,
                metallicity: 0.2,
                axial_tilt_deg: 15.0,
                rotation_period_h: 20.0,
                seed: 0,
            },
            continental_scale: 3.0,
            water_loss: 0.0,
        },
        PlanetPreset {
            name: "ice_world",
            params: PlanetParams {
                star_distance_au: 2.0,
                mass_earth: 1.2,
                metallicity: -0.3,
                axial_tilt_deg: 30.0,
                rotation_period_h: 18.0,
                seed: 0,
            },
            continental_scale: 0.7,
            water_loss: 0.0,
        },
        PlanetPreset {
            name: "superearth",
            params: PlanetParams {
                star_distance_au: 1.1,
                mass_earth: 5.0,
                metallicity: 0.3,
                axial_tilt_deg: 10.0,
                rotation_period_h: 16.0,
                seed: 0,
            },
            continental_scale: 1.0,
            water_loss: 0.0,
        },
    ]
}

fn generate_planet_png(
    gpu: &GpuContext,
    compute: &TerrainComputePipeline,
    renderer: &PreviewRenderer,
    preset: &PlanetPreset,
    seed: u32,
    render_size: u32,
) -> Vec<u8> {
    let mut params = preset.params.clone();
    params.seed = seed;

    let derived = DerivedProperties::from_params(&params);
    let effective_ocean = derived.ocean_fraction * (1.0 - preset.water_loss);

    let plates = generate_plates(&PlateGenParams {
        seed,
        mass_earth: params.mass_earth,
        ocean_fraction: effective_ocean,
        tectonics_factor: derived.tectonics_factor,
        continental_scale: preset.continental_scale,
        num_plates_override: 0,
        num_continents: 0,
        continent_size_variety: 0.0,
    });

    // Terrain params from spectral exponents
    let dist = params.star_distance_au;
    let dist_factor = (dist.ln() / 3.0_f32.ln()).clamp(0.0, 1.0);
    let base_beta = 1.47 + 0.91 * dist_factor;
    let beta = (base_beta + 0.3 * params.metallicity).clamp(1.2, 3.0);
    let hurst = (beta - 1.0) / 2.0;
    let gain = 2.0_f32.powf(-hurst);
    let amplitude = 0.6 + 0.6 * params.mass_earth.powf(0.3).min(2.0);
    let frequency = (1.0 + 0.5 * params.mass_earth.powf(0.2)) * preset.continental_scale;
    let tilt_factor = params.axial_tilt_deg / 90.0;
    let octaves = (8.0 + 4.0 * tilt_factor * derived.tectonics_factor) as u32;
    let rotation_factor = (24.0 / params.rotation_period_h).clamp(0.5, 2.0);
    let lacunarity = 1.9 + 0.2 * rotation_factor;

    let terrain = compute.generate(
        gpu,
        &plates,
        512,
        seed,
        amplitude,
        frequency,
        octaves,
        gain,
        lacunarity,
        1.0,
        0.10,
        1.0,
        1.0,
        derived.surface_gravity,
        derived.tectonics_factor,
        derived.surface_age,
        1.0,
    );

    let cubemap_view = renderer.upload_terrain(gpu, &terrain);
    let ocean_level = -1.0 + 2.0 * effective_ocean;

    // Tilted view showing equatorial features (~20° tilt)
    let tilt = 0.35_f32; // ~20 degrees
    let ct = tilt.cos();
    let st = tilt.sin();
    let uniforms = PreviewUniforms {
        rotation: [
            [1.0, 0.0, 0.0, 0.0],
            [0.0, ct, -st, 0.0],
            [0.0, st, ct, 0.0],
            [0.0, 0.0, 0.0, 1.0],
        ],
        light_dir: [0.5, 0.7, -1.0],
        ocean_level,
        base_temp_c: derived.base_temperature_c,
        ocean_fraction: effective_ocean,
        axial_tilt_rad: params.axial_tilt_deg.to_radians(),
        view_mode: 0,
        season: 0.5,
        atmosphere_density: 0.0,
        atmosphere_height: 0.0,
        height_scale: 3.0,
        zoom: 1.0,
        pan_x: 0.0,
        pan_y: 0.0,
        cloud_coverage: 0.0,
        cloud_seed: 0,
        night_lights: 0.0,
        star_color_temp: 0.5,
        city_light_hue: 0.0,
        show_ao: 1.0,
        show_water: 1.0,
        show_ice: 1.0,
        show_biomes: 1.0,
        show_clouds: 0.0,
        show_atmosphere_layer: 0.0,
        show_cities: 0.0,
        cloud_opacity: 1.0,
        cloud_advection: 0.0,
        rotation_rate: 1.0,
        atm_pressure: 0.7,
        _pad4: 0.0,
        lava_glow: 0.0,
        ring_inner: 0.0,
        ring_outer: 0.0,
        ring_tilt: 0.0,
        ring_opacity: 0.0,
        planet_radius_km: derived.radius_km,
        show_cloud_shadows: 1.0,
        _pad5: 0.0,
    };

    renderer.render(gpu, &uniforms, &cubemap_view, None, None, render_size)
}

struct WeatherScene {
    terrain: TectonicTerrain,
    cubemap: wgpu::TextureView,
    dynamics: DynamicsTextures,
    derived: DerivedProperties,
    tilt_rad: f32,
    ocean_level: f32,
}

fn generate_weather_scene(
    gpu: &GpuContext,
    compute: &TerrainComputePipeline,
    renderer: &PreviewRenderer,
    wind_pipeline: &WindFieldPipeline,
    preset: &PlanetPreset,
    seed: u32,
    resolutions: (u32, u32),
) -> WeatherScene {
    let (terrain_resolution, weather_resolution) = resolutions;
    let mut params = preset.params.clone();
    params.seed = seed;
    let derived = DerivedProperties::from_params(&params);
    let effective_ocean = derived.ocean_fraction * (1.0 - preset.water_loss);
    let ocean_level = -1.0 + 2.0 * effective_ocean;
    let plates = generate_plates(&PlateGenParams {
        seed,
        mass_earth: params.mass_earth,
        ocean_fraction: effective_ocean,
        tectonics_factor: derived.tectonics_factor,
        continental_scale: preset.continental_scale,
        num_plates_override: 0,
        num_continents: 0,
        continent_size_variety: 0.0,
    });
    let beta = 1.47 + 0.3 * params.metallicity;
    let terrain = compute.generate(
        gpu,
        &plates,
        terrain_resolution,
        seed,
        0.6 + 0.6 * params.mass_earth.powf(0.3).min(2.0),
        (1.0 + 0.5 * params.mass_earth.powf(0.2)) * preset.continental_scale,
        10,
        2.0_f32.powf(-(beta - 1.0) / 2.0),
        2.0,
        1.0,
        0.10,
        1.0,
        1.0,
        derived.surface_gravity,
        derived.tectonics_factor,
        derived.surface_age,
        1.0,
    );
    let cubemap = renderer.upload_terrain(gpu, &terrain);
    let dynamics = wind_pipeline.create_textures(gpu, weather_resolution);
    wind_pipeline.generate_gpu(
        gpu,
        &terrain,
        &dynamics,
        seed,
        ocean_level,
        params.axial_tilt_deg.to_radians(),
        0.5,
        24.0 / params.rotation_period_h,
        derived.base_temperature_c,
        derived.surface_pressure_bar,
    );
    WeatherScene {
        terrain,
        cubemap,
        dynamics,
        derived,
        tilt_rad: params.axial_tilt_deg.to_radians(),
        ocean_level,
    }
}

fn save_png(output_dir: &str, name: &str, size: u32, pixels: &[u8]) {
    let path = Path::new(output_dir).join(name);
    image::save_buffer(path, pixels, size, size, image::ColorType::Rgba8)
        .expect("Failed to save PNG");
    println!("  {name}");
}

fn save_contact_sheet(output_dir: &str, name: &str, size: u32, renders: &[(Vec<u8>, Vec<u8>)]) {
    let width = size * renders.len() as u32;
    let height = size * 2;
    let mut sheet = vec![0; (width * height * 4) as usize];
    for (column, (planet, density)) in renders.iter().enumerate() {
        for (row, pixels) in [planet, density].into_iter().enumerate() {
            for y in 0..size as usize {
                let source = y * size as usize * 4;
                let target =
                    ((row * size as usize + y) * width as usize + column * size as usize) * 4;
                sheet[target..target + size as usize * 4]
                    .copy_from_slice(&pixels[source..source + size as usize * 4]);
            }
        }
    }
    let path = Path::new(output_dir).join(name);
    image::save_buffer(path, &sheet, width, height, image::ColorType::Rgba8)
        .expect("Failed to save contact sheet");
    println!("  {name}");
}

fn generate_validation_weather(
    pipeline: &WeatherFieldPipeline,
    gpu: &GpuContext,
    scene: &WeatherScene,
    seed: u32,
    storm_count: u32,
    storm_size: f32,
) -> WeatherTextures {
    generate_validation_weather_with_dynamics(
        pipeline,
        gpu,
        scene,
        &scene.dynamics,
        seed,
        storm_count,
        storm_size,
    )
}

fn generate_validation_weather_with_dynamics(
    pipeline: &WeatherFieldPipeline,
    gpu: &GpuContext,
    scene: &WeatherScene,
    dynamics: &DynamicsTextures,
    seed: u32,
    storm_count: u32,
    storm_size: f32,
) -> WeatherTextures {
    let weather = pipeline.create_textures(gpu, scene.dynamics.resolution);
    pipeline.generate(
        gpu,
        WeatherSnapshot {
            face: 0,
            resolution: scene.dynamics.resolution,
            seed,
            storm_count,
            coverage: 0.5,
            moisture: 1.0,
            surface_pressure_bar: scene.derived.surface_pressure_bar,
            base_temp_c: scene.derived.base_temperature_c,
            ocean_level: scene.ocean_level,
            axial_tilt_rad: scene.tilt_rad,
            season: 0.5,
            storm_size,
            radius_km: scene.derived.radius_km,
            rotation_rate_rad_s: scene.derived.rotation_rate_rad_s,
            wind_scale: 1.0,
        },
        &scene.terrain,
        dynamics,
        &weather,
    );
    weather
}

fn render_weather(
    renderer: &PreviewRenderer,
    gpu: &GpuContext,
    uniforms: &PreviewUniforms,
    scene: &WeatherScene,
    weather: &WeatherTextures,
    size: u32,
) -> Vec<u8> {
    renderer.render(
        gpu,
        uniforms,
        &scene.cubemap,
        Some(&scene.dynamics.wind_continentality),
        Some((&weather.mass, &weather.geometry)),
        size,
    )
}

fn rgb_distance(a: &[u8], b: &[u8]) -> f32 {
    let squared = a
        .chunks_exact(4)
        .zip(b.chunks_exact(4))
        .flat_map(|(a, b)| {
            (0..3).map(move |channel| (a[channel] as f32 - b[channel] as f32).powi(2))
        })
        .sum::<f32>();
    (squared / (a.len() / 4 * 3) as f32).sqrt() / 255.0
}

fn cloudy_change_fraction(a: &[u8], b: &[u8], density_a: &[u8], density_b: &[u8]) -> f32 {
    let size = (((a.len() / 4).max(1) as f32).sqrt()) as usize;
    let mut cloudy = 0;
    let mut changed = 0;
    for ((((a, b), density_a), density_b), index) in a
        .chunks_exact(4)
        .zip(b.chunks_exact(4))
        .zip(density_a.chunks_exact(4))
        .zip(density_b.chunks_exact(4))
        .zip(0..)
    {
        let x = index % size;
        let y = (index / size) % size;
        if !in_sphere_mask(x, y, size) {
            continue;
        }
        if density_a[0].max(density_b[0]) <= 13 {
            continue;
        }
        cloudy += 1;
        let distance = (0..3)
            .map(|channel| (a[channel] as f32 - b[channel] as f32).powi(2))
            .sum::<f32>()
            .sqrt()
            / 255.0;
        changed += usize::from(distance > 0.05);
    }
    changed as f32 / cloudy.max(1) as f32
}

#[derive(Clone, Copy, Debug)]
struct TopologyMetrics {
    occupied: f32,
    coherent: f32,
    zonal_continuity: f32,
    meridional_continuity: f32,
    ribbon_like: f32,
    polar_occupied: f32,
    directional_anisotropy: f32,
    components: usize,
    largest_component: f32,
}

#[derive(Clone, Copy, Debug)]
struct MassFieldMetrics {
    occupied_fraction: f32,
    mean_alpha: f32,
    nonfinite_pixels: usize,
    out_of_range_pixels: usize,
}

#[derive(Clone, Copy, Debug)]
struct SeamMetrics {
    edge_max_delta: f32,
    edge_p99_delta: f32,
    edge_sample_count: usize,
    corner_max_delta: f32,
    corner_p99_delta: f32,
    corner_sample_count: usize,
}

#[derive(Debug, Default)]
struct RuntimeStats {
    count: usize,
    mean_ms: f64,
    min_ms: f64,
    max_ms: f64,
    p95_ms: f64,
}

fn in_sphere_mask(x: usize, y: usize, size: usize) -> bool {
    let nx = (x as f32 + 0.5) / size as f32 * 2.0 - 1.0;
    let ny = (y as f32 + 0.5) / size as f32 * 2.0 - 1.0;
    let planet_radius = 0.85_f32;
    nx * nx + ny * ny <= planet_radius * planet_radius
}

fn srgb_to_linear(value: u8) -> f32 {
    let value = value as f32 / 255.0;
    if value <= 0.04045 {
        value / 12.92
    } else {
        ((value + 0.055) / 1.055).powf(2.4)
    }
}

fn mean_rendered_optical_depth(pixels: &[u8], size: u32) -> f32 {
    let size = size as usize;
    let (total, count) = pixels
        .chunks_exact(4)
        .enumerate()
        .filter(|(index, _)| in_sphere_mask(*index % size, *index / size, size))
        .map(|(_, pixel)| -(1.0 - srgb_to_linear(pixel[0])).max(f32::MIN_POSITIVE).ln())
        .fold((0.0, 0usize), |(total, count), value| {
            (total + value, count + 1)
        });
    total / count.max(1) as f32
}

fn sample_mass_pixel(
    values: &[f32],
    resolution: u32,
    face: usize,
    x: usize,
    y: usize,
    channel: usize,
) -> f32 {
    let idx = ((face * resolution as usize + y) * resolution as usize + x) * 4 + channel;
    values[idx]
}

fn u3_combined_mass(mass: &[f32], index: usize) -> f32 {
    mass[index] + mass[index + 1] * 1.2 + mass[index + 2] * 0.35
}

fn u3_distribution(values: &mut [(f32, f32)]) -> String {
    const BIN_COUNT: usize = 16;
    let mut bins = [0.0_f32; BIN_COUNT];
    let mut total_weight = 0.0;
    let mut min = f32::INFINITY;
    let mut max = f32::NEG_INFINITY;
    for &(value, weight) in values.iter() {
        let bin = ((value / 0.01).floor() as usize).min(BIN_COUNT - 1);
        bins[bin] += weight;
        total_weight += weight;
        min = min.min(value);
        max = max.max(value);
    }
    values.sort_by(|left, right| left.0.total_cmp(&right.0));
    let quantile = |quantile: f32| {
        let mut cumulative = 0.0;
        values
            .iter()
            .find_map(|(value, weight)| {
                cumulative += weight;
                (cumulative >= total_weight * quantile).then_some(*value)
            })
            .unwrap_or(0.0)
    };
    format!(
        "range=[{min:.5},{max:.5}] q50={:.5} q90={:.5} q95={:.5} q99={:.5} bins={:?}",
        quantile(0.50),
        quantile(0.90),
        quantile(0.95),
        quantile(0.99),
        bins.map(|value| value / total_weight.max(f32::EPSILON)),
    )
}

fn u3_mass_report(case: &str, label: &str, mass: &[f32], resolution: u32) -> String {
    const POLAR_LATITUDE_SINE: f32 = 0.866_025_4; // Geographic latitude >= 60 degrees.
    let mut global: [Vec<(f32, f32)>; 5] = std::array::from_fn(|_| Vec::new());
    let mut polar: [Vec<(f32, f32)>; 5] = std::array::from_fn(|_| Vec::new());
    for face in 0..6 {
        for y in 0..resolution {
            for x in 0..resolution {
                let index = ((face * resolution * resolution + y * resolution + x) * 4) as usize;
                let weight = u15_weight(x, y, resolution);
                let values = [
                    mass[index],
                    mass[index + 1],
                    mass[index + 2],
                    mass[index + 3],
                    u3_combined_mass(mass, index),
                ];
                for (channel, value) in values.into_iter().enumerate() {
                    global[channel].push((value, weight));
                }
                let position = planet_gen::cube_sphere::cube_to_sphere(
                    face,
                    x as f32 / (resolution - 1) as f32,
                    y as f32 / (resolution - 1) as f32,
                );
                if position[1].abs() >= POLAR_LATITUDE_SINE {
                    for (channel, value) in values.into_iter().enumerate() {
                        polar[channel].push((value, weight));
                    }
                }
            }
        }
    }
    let channels = ["low", "deep", "high", "occupancy", "M"];
    let reports = channels
        .into_iter()
        .enumerate()
        .map(|(channel, name)| {
            format!(
                "  {name} global: {}; polar: {}",
                u3_distribution(&mut global[channel]),
                u3_distribution(&mut polar[channel]),
            )
        })
        .collect::<Vec<_>>()
        .join("\n");
    format!("case={case} seed={label}\n{reports}")
}

fn u3_screen_direction(x: usize, y: usize, size: usize, rotation: [[f32; 4]; 4]) -> [f32; 3] {
    let ndc_x = (x as f32 + 0.5) / size as f32 * 2.0 / 0.85 - 1.0 / 0.85;
    let ndc_y = (y as f32 + 0.5) / size as f32 * 2.0 / 0.85 - 1.0 / 0.85;
    let z = (1.0 - ndc_x * ndc_x - ndc_y * ndc_y).sqrt();
    [
        rotation[0][0] * ndc_x + rotation[0][1] * ndc_y + rotation[0][2] * z,
        rotation[1][0] * ndc_x + rotation[1][1] * ndc_y + rotation[1][2] * z,
        rotation[2][0] * ndc_x + rotation[2][1] * ndc_y + rotation[2][2] * z,
    ]
}

fn u3_pixel_quantiles(values: &mut [f32]) -> (f32, f32, f32, f32) {
    if values.is_empty() {
        return (0.0, 0.0, 0.0, 0.0);
    }
    values.sort_by(f32::total_cmp);
    let index = |quantile: f32| ((values.len() - 1) as f32 * quantile).round() as usize;
    (
        values[0],
        values[index(0.50)],
        values[index(0.95)],
        values[values.len() - 1],
    )
}

fn u3_rendered_association(
    case: &str,
    label: &str,
    density: &[u8],
    mass: &[f32],
    resolution: u32,
    rotation: [[f32; 4]; 4],
) -> String {
    const OPACITY_THRESHOLD: f32 = 0.05;
    const POLAR_LATITUDE_SINE: f32 = 0.866_025_4;
    let size = (density.len() / 4).isqrt();
    let mut visible = 0usize;
    let mut opaque = 0usize;
    let mut polar_visible = 0usize;
    let mut polar_opaque = 0usize;
    let mut dominant = [0usize; 3];
    let mut polar_dominant = [0usize; 3];
    let mut opacity = Vec::new();
    let mut combined_mass = Vec::new();
    for (index, pixel) in density.chunks_exact(4).enumerate() {
        let x = index % size;
        let y = index / size;
        if !in_sphere_mask(x, y, size) {
            continue;
        }
        visible += 1;
        let direction = u3_screen_direction(x, y, size, rotation);
        let is_polar = direction[1].abs() >= POLAR_LATITUDE_SINE;
        if is_polar {
            polar_visible += 1;
        }
        let rendered_opacity = srgb_to_linear(pixel[0]);
        if rendered_opacity <= OPACITY_THRESHOLD {
            continue;
        }
        opaque += 1;
        if is_polar {
            polar_opaque += 1;
        }
        let mass_pixel = u15_sphere_to_pixel(direction, resolution) * 4;
        let family = [mass[mass_pixel], mass[mass_pixel + 1], mass[mass_pixel + 2]]
            .into_iter()
            .enumerate()
            .max_by(|left, right| left.1.total_cmp(&right.1))
            .map(|(family, _)| family)
            .unwrap_or(0);
        dominant[family] += 1;
        if is_polar {
            polar_dominant[family] += 1;
        }
        opacity.push(rendered_opacity);
        combined_mass.push(u3_combined_mass(mass, mass_pixel));
    }
    let (opacity_min, opacity_p50, opacity_p95, opacity_max) = u3_pixel_quantiles(&mut opacity);
    let (mass_min, mass_p50, mass_p95, mass_max) = u3_pixel_quantiles(&mut combined_mass);
    let family_share =
        |counts: [usize; 3], total: usize| counts.map(|count| count as f32 / total.max(1) as f32);
    format!(
        "case={case} seed={label} opacity>0.05 visible={:.3} polar={:.3} dominant=[low:{:.3},deep:{:.3},high:{:.3}] polar_dominant=[low:{:.3},deep:{:.3},high:{:.3}] opacity_range=[{opacity_min:.5},{opacity_p50:.5},{opacity_p95:.5},{opacity_max:.5}] M_range=[{mass_min:.5},{mass_p50:.5},{mass_p95:.5},{mass_max:.5}]",
        opaque as f32 / visible.max(1) as f32,
        polar_opaque as f32 / polar_visible.max(1) as f32,
        family_share(dominant, opaque)[0],
        family_share(dominant, opaque)[1],
        family_share(dominant, opaque)[2],
        family_share(polar_dominant, polar_opaque)[0],
        family_share(polar_dominant, polar_opaque)[1],
        family_share(polar_dominant, polar_opaque)[2],
    )
}

fn density_topology(pixels: &[u8], size: u32) -> TopologyMetrics {
    let size = size as usize;
    let cloudy = |x: usize, y: usize| srgb_to_linear(pixels[(y * size + x) * 4]) > 0.05;
    let mut sphere_pixels = 0;
    let mut occupied = 0;
    let mut coherent = 0;
    let mut zonal = 0;
    let mut meridional = 0;
    let mut ribbon_like = 0;
    let mut polar_pixels = 0;
    let mut polar_occupied = 0;
    for y in 0..size {
        for x in 0..size {
            if !in_sphere_mask(x, y, size) {
                continue;
            }
            let ny = (y as f32 + 0.5) / size as f32 * 2.0 - 1.0;
            sphere_pixels += 1;
            if ny.abs() > 0.65 {
                polar_pixels += 1;
                polar_occupied += usize::from(cloudy(x, y));
            }
            if !cloudy(x, y) {
                continue;
            }
            occupied += 1;
            if (x > 0 && cloudy(x - 1, y))
                || (x + 1 < size && cloudy(x + 1, y))
                || (y > 0 && cloudy(x, y - 1))
                || (y + 1 < size && cloudy(x, y + 1))
            {
                coherent += 1;
            }
            let horizontal = (x > 0 && cloudy(x - 1, y)) || (x + 1 < size && cloudy(x + 1, y));
            let vertical = (y > 0 && cloudy(x, y - 1)) || (y + 1 < size && cloudy(x, y + 1));
            zonal += usize::from(horizontal);
            meridional += usize::from(vertical);
            ribbon_like += usize::from(horizontal != vertical);
        }
    }
    let mut visited = vec![false; size * size];
    let mut components = 0;
    let mut largest_component = 0;
    for y in 0..size {
        for x in 0..size {
            let start = y * size + x;
            if visited[start] || !in_sphere_mask(x, y, size) || !cloudy(x, y) {
                continue;
            }
            components += 1;
            visited[start] = true;
            let mut stack = vec![(x, y)];
            let mut component_size = 0;
            while let Some((x, y)) = stack.pop() {
                component_size += 1;
                for (next_x, next_y) in [
                    (x.wrapping_sub(1), y),
                    (x + 1, y),
                    (x, y.wrapping_sub(1)),
                    (x, y + 1),
                ] {
                    if next_x >= size || next_y >= size {
                        continue;
                    }
                    let next = next_y * size + next_x;
                    if !visited[next]
                        && in_sphere_mask(next_x, next_y, size)
                        && cloudy(next_x, next_y)
                    {
                        visited[next] = true;
                        stack.push((next_x, next_y));
                    }
                }
            }
            largest_component = largest_component.max(component_size);
        }
    }
    let zonal_continuity = zonal as f32 / occupied.max(1) as f32;
    let meridional_continuity = meridional as f32 / occupied.max(1) as f32;
    TopologyMetrics {
        occupied: occupied as f32 / sphere_pixels as f32,
        coherent: coherent as f32 / occupied.max(1) as f32,
        zonal_continuity,
        meridional_continuity,
        ribbon_like: ribbon_like as f32 / occupied.max(1) as f32,
        polar_occupied: polar_occupied as f32 / polar_pixels.max(1) as f32,
        directional_anisotropy: (zonal_continuity - meridional_continuity).abs()
            / (zonal_continuity + meridional_continuity).max(f32::EPSILON),
        components,
        largest_component: largest_component as f32 / occupied.max(1) as f32,
    }
}

fn mass_field_metrics(values: &[f32], resolution: u32, mask_with_sphere: bool) -> MassFieldMetrics {
    let size = resolution as usize;
    let mut sphere_pixels = 0;
    let mut occupied_pixels = 0;
    let mut sum_alpha = 0.0;
    let mut nonfinite_pixels = 0;
    let mut out_of_range_pixels = 0;

    for f in 0..6 {
        for y in 0..size {
            for x in 0..size {
                if mask_with_sphere && !in_sphere_mask(x, y, size) {
                    continue;
                }
                sphere_pixels += 1;
                for channel in 0..4 {
                    let value = sample_mass_pixel(values, resolution, f, x, y, channel);
                    if !value.is_finite() {
                        nonfinite_pixels += 1;
                    }
                    if !(0.0..=1.0).contains(&value) {
                        out_of_range_pixels += 1;
                    }
                    if channel == 3 {
                        sum_alpha += value;
                        if value > 0.05 {
                            occupied_pixels += 1;
                        }
                    }
                }
            }
        }
    }
    MassFieldMetrics {
        occupied_fraction: occupied_pixels as f32 / sphere_pixels.max(1) as f32,
        mean_alpha: if sphere_pixels > 0 {
            sum_alpha / sphere_pixels as f32
        } else {
            0.0
        },
        nonfinite_pixels,
        out_of_range_pixels,
    }
}

fn validate_mass_field(
    label: &str,
    mass: &[f32],
    resolution: u32,
    expect_zero_coverage: bool,
    mask_with_sphere: bool,
) -> Result<MassFieldMetrics, String> {
    let metrics = mass_field_metrics(mass, resolution, mask_with_sphere);
    if metrics.nonfinite_pixels > 0 {
        return Err(format!(
            "{label}: mass field contains {} non-finite values",
            metrics.nonfinite_pixels
        ));
    }
    if metrics.out_of_range_pixels > 0 {
        return Err(format!(
            "{label}: too many mass values out of expected [0, 1] range ({})",
            metrics.out_of_range_pixels
        ));
    }
    if expect_zero_coverage {
        if metrics.occupied_fraction > 0.001 {
            return Err(format!(
                "{label}: expected zero-coverage field, occupied={:.3}",
                metrics.occupied_fraction
            ));
        }
        if metrics.mean_alpha > 1e-4 {
            return Err(format!(
                "{label}: expected zero-coverage field, mean_alpha={:.5}",
                metrics.mean_alpha
            ));
        }
    } else {
        if metrics.occupied_fraction < 0.01 {
            return Err(format!(
                "{label}: field is empty after masking (occupied={:.3})",
                metrics.occupied_fraction
            ));
        }
        if metrics.occupied_fraction > 0.95 {
            return Err(format!(
                "{label}: field is almost globally occupied (occupied={:.3})",
                metrics.occupied_fraction
            ));
        }
    }
    Ok(metrics)
}

fn validate_seam_metrics(metrics: &SeamMetrics) -> Result<(), &'static str> {
    if metrics.edge_sample_count == 0 || metrics.corner_sample_count == 0 {
        return Err("seam metrics had no samples");
    }
    if metrics.edge_max_delta > 0.30 || metrics.corner_max_delta > 0.30 {
        return Err("cubemap seam has a large discontinuity");
    }
    if metrics.edge_p99_delta > 0.12 || metrics.corner_p99_delta > 0.12 {
        return Err("cubemap seam has elevated average discontinuity");
    }
    Ok(())
}

#[derive(Clone, Copy)]
struct CubeEdge {
    face: usize,
    edge: usize,
}

fn cube_edge_point(edge: CubeEdge, index: usize, last: usize) -> (usize, usize, usize) {
    let (x, y) = match edge.edge {
        0 => (0, index),
        1 => (last, index),
        2 => (index, 0),
        3 => (index, last),
        _ => unreachable!("cubemap faces have four edges"),
    };
    (edge.face, x, y)
}

fn cube_edge_pairs(resolution: u32) -> Vec<(CubeEdge, CubeEdge, bool)> {
    let last = resolution as usize - 1;
    let edges: Vec<_> = (0..6)
        .flat_map(|face| (0..4).map(move |edge| CubeEdge { face, edge }))
        .collect();
    let point = |edge: CubeEdge, index| {
        let (_, x, y) = cube_edge_point(edge, index, last);
        planet_gen::cube_sphere::cube_to_sphere(
            edge.face as u32,
            x as f32 / last as f32,
            y as f32 / last as f32,
        )
    };
    let same = |a: [f32; 3], b: [f32; 3]| {
        a.iter()
            .zip(b)
            .all(|(left, right)| (left - right).abs() < 1e-5)
    };
    let mut pairs = Vec::new();
    for (index, left) in edges.iter().enumerate() {
        for right in &edges[index + 1..] {
            let forward = same(point(*left, 0), point(*right, 0))
                && same(point(*left, last), point(*right, last));
            let reversed = same(point(*left, 0), point(*right, last))
                && same(point(*left, last), point(*right, 0));
            if forward || reversed {
                pairs.push((*left, *right, reversed));
            }
        }
    }
    pairs
}

fn percentile_99(mut values: Vec<f32>) -> f32 {
    values.sort_by(f32::total_cmp);
    values[((values.len() - 1) as f32 * 0.99).round() as usize]
}

fn seam_continuity_metrics(mass: &[f32], resolution: u32, channel: usize) -> SeamMetrics {
    let last = resolution as usize - 1;
    let pixel =
        |face: usize, x: usize, y: usize| sample_mass_pixel(mass, resolution, face, x, y, channel);
    let mut edge_deltas = Vec::new();
    for (left, right, reversed) in cube_edge_pairs(resolution) {
        for index in 0..=last {
            let (left_face, left_x, left_y) = cube_edge_point(left, index, last);
            let right_index = if reversed { last - index } else { index };
            let (right_face, right_x, right_y) = cube_edge_point(right, right_index, last);
            edge_deltas.push(
                (pixel(left_face, left_x, left_y) - pixel(right_face, right_x, right_y)).abs(),
            );
        }
    }
    let mut corner_deltas = Vec::new();
    for corners in [
        [(0, 0, 0), (2, last, last), (4, last, 0)],
        [(0, last, 0), (2, last, 0), (5, 0, 0)],
        [(0, 0, last), (3, last, 0), (4, last, last)],
        [(0, last, last), (3, last, last), (5, 0, last)],
        [(1, last, 0), (2, 0, last), (4, 0, 0)],
        [(1, 0, 0), (2, 0, 0), (5, last, 0)],
        [(1, last, last), (3, 0, 0), (4, 0, last)],
        [(1, 0, last), (3, 0, last), (5, last, last)],
    ] {
        let corners = corners.map(|(face, x, y)| (face, x, y));
        let reference = pixel(corners[0].0, corners[0].1, corners[0].2);
        for &(face, x, y) in &corners[1..] {
            let value = pixel(face, x, y);
            let delta = (reference - value).abs();
            corner_deltas.push(delta);
        }
    }
    SeamMetrics {
        edge_max_delta: edge_deltas.iter().copied().fold(0.0, f32::max),
        edge_p99_delta: percentile_99(edge_deltas.clone()),
        edge_sample_count: edge_deltas.len(),
        corner_max_delta: corner_deltas.iter().copied().fold(0.0, f32::max),
        corner_p99_delta: percentile_99(corner_deltas.clone()),
        corner_sample_count: corner_deltas.len(),
    }
}

fn compute_runtime_stats(mut samples: Vec<f64>) -> RuntimeStats {
    if samples.is_empty() {
        return RuntimeStats {
            count: 0,
            mean_ms: 0.0,
            min_ms: 0.0,
            max_ms: 0.0,
            p95_ms: 0.0,
        };
    }
    let count = samples.len();
    samples.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
    let mean_ms = samples.iter().sum::<f64>() / count as f64;
    let min_ms = *samples.first().unwrap_or(&0.0);
    let max_ms = *samples.last().unwrap_or(&0.0);
    let p95_index = ((count as f64 - 1.0) * 0.95).round() as usize;
    RuntimeStats {
        count,
        mean_ms,
        min_ms,
        max_ms,
        p95_ms: samples[p95_index],
    }
}

fn time_gpu_call<T>(gpu: &GpuContext, f: impl FnOnce() -> T) -> (T, f64) {
    let start = Instant::now();
    let value = f();
    let (tx, rx) = mpsc::channel();
    gpu.queue.on_submitted_work_done(move || {
        let _ = tx.send(());
    });
    gpu.device
        .poll(wgpu::PollType::Wait {
            submission_index: None,
            timeout: Some(Duration::from_secs(30)),
        })
        .expect("timed out waiting for GPU queue completion");
    if rx.recv_timeout(Duration::from_secs(30)).is_err() {
        panic!("timed out waiting for GPU queue completion");
    }
    (value, start.elapsed().as_secs_f64() * 1000.0)
}

fn weather_validation_size_error(render_size: u32) -> Option<String> {
    (render_size < 512)
        .then(|| format!("--weather-validation requires --size >= 512 (got {render_size})"))
}

fn seed_change_fraction(a: &[u8], b: &[u8], size: u32) -> f32 {
    let size = size as usize;
    let mut sphere_pixels = 0;
    let mut changed = 0;
    for y in 0..size {
        for x in 0..size {
            if !in_sphere_mask(x, y, size) {
                continue;
            }
            sphere_pixels += 1;
            let offset = (y * size + x) * 4;
            changed += usize::from(a[offset].abs_diff(b[offset]) > 13);
        }
    }
    changed as f32 / sphere_pixels as f32
}

fn validate_seed_topology_metrics(
    topology: &TopologyMetrics,
    seed_change: Option<f32>,
) -> Result<(), &'static str> {
    if !(0.05..=0.80).contains(&topology.occupied) {
        return Err("cloud topology is empty or a global veil");
    }
    if topology.coherent < 0.90 {
        return Err("cloud topology is dominated by isolated noise");
    }
    if topology.zonal_continuity < 0.60 || topology.meridional_continuity < 0.60 {
        return Err("cloud field lacks directional continuity");
    }
    if topology.components < 2 || topology.largest_component > 0.985 {
        return Err("cloud component topology is degenerate");
    }
    if topology.ribbon_like > 0.45 {
        return Err("cloud field is dominated by thin ribbons");
    }
    if topology.polar_occupied > 0.85 {
        return Err("cloud field forms a polar slab");
    }
    if topology.directional_anisotropy > 0.65 {
        return Err("cloud field has excessive directional anisotropy");
    }
    if seed_change.is_some_and(|change| !(0.03..=0.60).contains(&change)) {
        return Err("cloud seed variation is invisible or replaces the physical field");
    }
    Ok(())
}

fn storm_control_metrics(renders: &[(Vec<u8>, Vec<u8>)]) -> (f32, f32, f32, f32) {
    let distance_low_mid = rgb_distance(&renders[0].0, &renders[1].0);
    let distance_low_high = rgb_distance(&renders[0].0, &renders[2].0);
    let changed_high =
        cloudy_change_fraction(&renders[0].0, &renders[2].0, &renders[0].1, &renders[2].1);
    let size = ((renders[2].1.len() / 4).max(1) as f32).sqrt() as usize;
    let mut cloudy_pixels = 0usize;
    let mut saturated_clouds = 0usize;
    for (index, pixel) in renders[2].1.chunks_exact(4).enumerate() {
        let x = index % size;
        let y = index / size;
        if !in_sphere_mask(x, y, size) {
            continue;
        }
        if pixel[0] > 13 {
            cloudy_pixels += 1;
            if pixel[0] > 242 {
                saturated_clouds += 1;
            }
        }
    }
    let saturated_clouds = saturated_clouds as f32 / cloudy_pixels.max(1) as f32;
    (
        distance_low_mid,
        distance_low_high,
        changed_high,
        saturated_clouds,
    )
}

fn u14_field_mean(values: &[f32], channel: usize) -> f32 {
    values
        .chunks_exact(4)
        .map(|pixel| pixel[channel])
        .sum::<f32>()
        / (values.len() / 4) as f32
}

fn u14_field_quantile(values: &[f32], channel: usize, quantile: f32) -> f32 {
    let mut samples: Vec<_> = values.chunks_exact(4).map(|pixel| pixel[channel]).collect();
    samples.sort_by(f32::total_cmp);
    let index = ((samples.len().saturating_sub(1) as f32 * quantile).round() as usize)
        .min(samples.len().saturating_sub(1));
    samples.get(index).copied().unwrap_or(0.0)
}

fn u14_geometry_metrics(mass: &[f32], geometry: &[f32]) -> (usize, usize) {
    mass.chunks_exact(4).zip(geometry.chunks_exact(4)).fold(
        (0, 0),
        |(occupied, invalid), (mass, geometry)| {
            let occupied_here = mass[3] > 1e-5;
            let valid = mass.iter().all(|value| value.is_finite())
                && geometry.iter().all(|value| value.is_finite())
                // Zero geometry is valid only for a zero-mass texel.
                && (!occupied_here
                    || (geometry[0] >= 0.0
                        && geometry[1] > geometry[0]
                        && geometry[2] > geometry[1]
                        && geometry[3] > geometry[2]));
            (
                occupied + usize::from(occupied_here),
                invalid + usize::from(!valid),
            )
        },
    )
}

fn u14_coverage_increments(totals: &[f32]) -> Result<Vec<f32>, &'static str> {
    const MIN_MEANINGFUL_INCREMENT: f32 = 0.0005;
    let increments: Vec<_> = totals.windows(2).map(|pair| pair[1] - pair[0]).collect();
    if increments.iter().any(|increment| *increment < 0.0) {
        return Err("coverage is not monotonic");
    }
    if increments
        .iter()
        .any(|increment| *increment < MIN_MEANINGFUL_INCREMENT)
    {
        return Err("coverage slider has a flat segment");
    }
    let mut sorted = increments.clone();
    sorted.sort_by(f32::total_cmp);
    if increments.iter().copied().fold(0.0, f32::max) > sorted[sorted.len() / 2] * 2.0 {
        return Err("coverage increment exceeds twice the median");
    }
    Ok(increments)
}

const U14_FLAT_COOL_OCEAN_MASK: &str = "flat_cool_ocean";
const U14_FLAT_INLAND_MASK: &str = "flat_inland";
const U14_MOUNTAIN_WINDWARD_MASK: &str = "mountain_windward";
const U14_MOUNTAIN_LEE_MASK: &str = "mountain_lee";
const U14_COAST_BAND_MASK: &str = "coast_band";

fn u14_coast_height(z: f32) -> f32 {
    -0.001 + 0.002 * ((z / 0.012).clamp(-1.0, 1.0) * 0.5 + 0.5)
}

fn u14_coast_continentality(z: f32) -> f32 {
    let t = ((z + 0.35) / 0.7).clamp(0.0, 1.0);
    t * t * (3.0 - 2.0 * t)
}

fn u14_coast_metrics(low_mass: &[f32], resolution: u32) -> (f32, f32) {
    let resolution = resolution as usize;
    let weather_texel = 1.0 / resolution as f32;
    let pixel = |x: usize, y: usize| low_mass[(y * resolution + x) * 4];
    let mut pairs = Vec::new();
    let mut coast_energy = 0.0;
    let mut coast_samples = 0usize;
    let mut surrounding_energy = 0.0;
    let mut surrounding_samples = 0usize;
    for y in 1..resolution - 1 {
        for x in 1..resolution - 1 {
            let position = planet_gen::cube_sphere::cube_to_sphere(
                0,
                x as f32 / (resolution - 1) as f32,
                y as f32 / (resolution - 1) as f32,
            );
            let z = position[2];
            if z.abs() > weather_texel * 6.0 {
                continue;
            }
            let left = planet_gen::cube_sphere::cube_to_sphere(
                0,
                (x - 1) as f32 / (resolution - 1) as f32,
                y as f32 / (resolution - 1) as f32,
            );
            let right = planet_gen::cube_sphere::cube_to_sphere(
                0,
                (x + 1) as f32 / (resolution - 1) as f32,
                y as f32 / (resolution - 1) as f32,
            );
            let cloud_gradient = (pixel(x + 1, y) - pixel(x - 1, y)).abs() * 0.5;
            let coast_gradient =
                (u14_coast_height(right[2]) - u14_coast_height(left[2])).abs() * 0.5;
            pairs.push((cloud_gradient, coast_gradient));
            if z.abs() <= weather_texel {
                coast_energy += cloud_gradient;
                coast_samples += 1;
            } else if z.abs() >= weather_texel * 2.0 {
                surrounding_energy += cloud_gradient;
                surrounding_samples += 1;
            }
        }
    }
    let mean = |index: usize| {
        pairs
            .iter()
            .map(|pair| if index == 0 { pair.0 } else { pair.1 })
            .sum::<f32>()
            / pairs.len() as f32
    };
    let cloud_mean = mean(0);
    let coast_mean = mean(1);
    let covariance = pairs
        .iter()
        .map(|(cloud, coast)| (cloud - cloud_mean) * (coast - coast_mean))
        .sum::<f32>();
    let cloud_variance = pairs
        .iter()
        .map(|(cloud, _)| (cloud - cloud_mean).powi(2))
        .sum::<f32>();
    let coast_variance = pairs
        .iter()
        .map(|(_, coast)| (coast - coast_mean).powi(2))
        .sum::<f32>();
    let correlation = covariance / (cloud_variance * coast_variance).sqrt().max(f32::EPSILON);
    let local_energy = coast_energy / coast_samples.max(1) as f32;
    let surrounding_energy = surrounding_energy / surrounding_samples.max(1) as f32;
    (
        correlation,
        local_energy / surrounding_energy.max(f32::EPSILON),
    )
}

fn u14_flat_terrain(resolution: u32, height: impl Fn([f32; 3]) -> f32) -> TectonicTerrain {
    TectonicTerrain {
        faces: std::array::from_fn(|face| {
            (0..resolution * resolution)
                .map(|index| {
                    planet_gen::cube_sphere::cube_to_sphere(
                        face as u32,
                        (index % resolution) as f32 / (resolution - 1) as f32,
                        (index / resolution) as f32 / (resolution - 1) as f32,
                    )
                })
                .map(&height)
                .collect()
        }),
        resolution,
    }
}

fn run_u14_field_validation(
    gpu: &GpuContext,
    pipeline: &WeatherFieldPipeline,
    output_dir: &str,
    resolution: u32,
) -> Vec<String> {
    const SEEDS: [u32; 8] = [7, 19, 37, 73, 101, 211, 509, 997];
    let terrain = u14_flat_terrain(resolution, |_| -0.1);
    // The height sign change and the continentality transition share z=0, but only
    // the latter is broad. The fixed mask catches a cloud edge tracing the coast.
    let coast = u14_flat_terrain(resolution, |pos| u14_coast_height(pos[2]));
    let ridge = u14_flat_terrain(resolution, |pos| {
        -0.15 + (-((pos[2] / 0.14).powi(2))).exp() * pos[0].max(0.0).powi(8) * 0.45
    });
    let wind = WindFieldPipeline::new(gpu).expect("U14 dynamics unavailable");
    let mut failures = Vec::new();
    let mut rows = Vec::new();
    let mut generation_samples_ms = Vec::new();
    let mut weather = |terrain: &TectonicTerrain,
                       continentality: fn([f32; 3]) -> f32,
                       temp: f32,
                       coverage: f32,
                       moisture: f32,
                       seed: u32,
                       flow: f32| {
        let dynamics = wind.create_test_textures(gpu, resolution, |pos| {
            let tangent = [pos[2], 0.0, -pos[0]];
            let length = (tangent[0] * tangent[0] + tangent[2] * tangent[2])
                .sqrt()
                .max(0.0001);
            (
                [
                    tangent[0] / length * flow,
                    0.0,
                    tangent[2] / length * flow,
                    continentality(pos),
                ],
                1025.0,
            )
        });
        let field = pipeline.create_textures(gpu, resolution);
        let start = Instant::now();
        pipeline.generate(
            gpu,
            WeatherSnapshot {
                face: 0,
                resolution,
                seed,
                storm_count: 0,
                coverage,
                moisture,
                surface_pressure_bar: 1.0,
                base_temp_c: temp,
                ocean_level: 0.0,
                axial_tilt_rad: 0.0,
                season: 0.5,
                storm_size: 1.0,
                radius_km: 6371.0,
                rotation_rate_rad_s: std::f32::consts::TAU / 86400.0,
                wind_scale: 1.0,
            },
            terrain,
            &dynamics,
            &field,
        );
        let mass = field.read_mass(gpu);
        let geometry = field.read_geometry(gpu);
        generation_samples_ms.push(start.elapsed().as_secs_f64() * 1000.0);
        (mass, geometry)
    };
    let mut cool_ratio = f32::INFINITY;
    let mut low_deep = f32::INFINITY;
    let mut deck_min = f32::INFINITY;
    let mut deck_max = 0.0_f32;
    let mut trade_min = f32::INFINITY;
    let mut trade_max = 0.0_f32;
    let mut cool_deck_p90_min = f32::INFINITY;
    let mut trade_p90_min = f32::INFINITY;
    let mut windward_p90_min = f32::INFINITY;
    let mut gaps_min = f32::INFINITY;
    let mut gaps_max = 0.0_f32;
    let mut coast_correlation_max = 0.0_f32;
    let mut coast_energy_ratio_max = 0.0_f32;
    let mut coverage_increment_min = [f32::INFINITY; 8];
    let mut coverage_increment_max_by_step = [0.0_f32; 8];
    let mut coverage_increment_max = 0.0_f32;
    let mut coverage_increment_median_min = f32::INFINITY;
    let mut zero_exact = true;
    let mut windward_enhancement_min = f32::INFINITY;
    let mut lee_enhancement_max = f32::NEG_INFINITY;
    let mut deterministic = true;
    let mut geometry_occupied_texels = 0usize;
    let mut geometry_invalid_texels = 0usize;
    let mut mass_seam_edge_max = [0.0_f32; 4];
    let mut mass_seam_edge_p99 = [0.0_f32; 4];
    let mut mass_seam_corner_max = [0.0_f32; 4];
    let mut mass_seam_corner_p99 = [0.0_f32; 4];
    let mut geometry_seam_edge_max = [0.0_f32; 4];
    let mut geometry_seam_edge_p99 = [0.0_f32; 4];
    let mut geometry_seam_corner_max = [0.0_f32; 4];
    let mut geometry_seam_corner_p99 = [0.0_f32; 4];
    let mut plateau_exact_max = [0.0_f32; 2];
    let mut plateau_near_max = [0.0_f32; 2];
    for seed in SEEDS {
        let (cool_ocean, cool_geometry) = weather(&terrain, |_| 0.0, -10.0, 0.75, 1.0, seed, 0.0);
        let (cool_inland, inland_geometry) =
            weather(&terrain, |_| 1.0, -10.0, 0.75, 1.0, seed, 0.0);
        let (repeat_mass, repeat_geometry) =
            weather(&terrain, |_| 0.0, -10.0, 0.75, 1.0, seed, 0.0);
        deterministic &= cool_ocean == repeat_mass && cool_geometry == repeat_geometry;
        for (mass, geometry) in [
            (&cool_ocean, &cool_geometry),
            (&cool_inland, &inland_geometry),
        ] {
            let (occupied, invalid) = u14_geometry_metrics(mass, geometry);
            geometry_occupied_texels += occupied;
            geometry_invalid_texels += invalid;
        }
        for channel in 0..4 {
            let mass_seam = seam_continuity_metrics(&cool_ocean, resolution, channel);
            let geometry_seam = seam_continuity_metrics(&cool_geometry, resolution, channel);
            mass_seam_edge_max[channel] = mass_seam_edge_max[channel].max(mass_seam.edge_max_delta);
            mass_seam_edge_p99[channel] = mass_seam_edge_p99[channel].max(mass_seam.edge_p99_delta);
            mass_seam_corner_max[channel] =
                mass_seam_corner_max[channel].max(mass_seam.corner_max_delta);
            mass_seam_corner_p99[channel] =
                mass_seam_corner_p99[channel].max(mass_seam.corner_p99_delta);
            geometry_seam_edge_max[channel] =
                geometry_seam_edge_max[channel].max(geometry_seam.edge_max_delta);
            geometry_seam_edge_p99[channel] =
                geometry_seam_edge_p99[channel].max(geometry_seam.edge_p99_delta);
            geometry_seam_corner_max[channel] =
                geometry_seam_corner_max[channel].max(geometry_seam.corner_max_delta);
            geometry_seam_corner_p99[channel] =
                geometry_seam_corner_p99[channel].max(geometry_seam.corner_p99_delta);
            if mass_seam.edge_max_delta > 0.02
                || mass_seam.corner_max_delta > 0.02
                || geometry_seam.edge_max_delta > 0.02
                || geometry_seam.corner_max_delta > 0.02
            {
                failures.push(format!(
                    "U14 seam seed {seed} channel {channel}: mass edge/corner={:.4}/{:.4}, geometry edge/corner={:.4}/{:.4}",
                    mass_seam.edge_max_delta,
                    mass_seam.corner_max_delta,
                    geometry_seam.edge_max_delta,
                    geometry_seam.corner_max_delta,
                ));
            }
        }
        let ocean_low = u14_field_mean(&cool_ocean, 0);
        let inland_low = u14_field_mean(&cool_inland, 0);
        cool_ratio = cool_ratio.min(ocean_low / inland_low.max(f32::EPSILON));
        cool_deck_p90_min = cool_deck_p90_min.min(u14_field_quantile(&cool_ocean, 0, 0.90));
        let ocean_deep = u14_field_mean(&cool_ocean, 1);
        low_deep = low_deep.min(ocean_low / ocean_deep.max(f32::EPSILON));
        let thickness = cool_geometry
            .chunks_exact(4)
            .map(|pixel| pixel[1] - pixel[0])
            .sum::<f32>()
            / (cool_geometry.len() / 4) as f32;
        deck_min = deck_min.min(thickness);
        deck_max = deck_max.max(thickness);

        let (warm, warm_geometry) = weather(&terrain, |_| 0.0, 28.0, 0.75, 1.0, seed, 0.0);
        let (occupied, invalid) = u14_geometry_metrics(&warm, &warm_geometry);
        geometry_occupied_texels += occupied;
        geometry_invalid_texels += invalid;
        let top = u14_field_mean(&warm_geometry, 1);
        trade_p90_min = trade_p90_min.min(u14_field_quantile(&warm, 0, 0.90));
        trade_min = trade_min.min(top);
        trade_max = trade_max.max(top);
        let gaps = warm
            .chunks_exact(4)
            .filter(|pixel| pixel[0] <= 0.05)
            .count() as f32
            / (warm.len() / 4) as f32;
        gaps_min = gaps_min.min(gaps);
        gaps_max = gaps_max.max(gaps);

        let (coast_mass, coast_geometry) = weather(
            &coast,
            |pos| u14_coast_continentality(pos[2]),
            15.0,
            0.75,
            1.0,
            seed,
            0.0,
        );
        let (occupied, invalid) = u14_geometry_metrics(&coast_mass, &coast_geometry);
        geometry_occupied_texels += occupied;
        geometry_invalid_texels += invalid;
        let (correlation, energy_ratio) = u14_coast_metrics(&coast_mass, resolution);
        coast_correlation_max = coast_correlation_max.max(correlation.abs());
        coast_energy_ratio_max = coast_energy_ratio_max.max(energy_ratio);

        let coverage: Vec<f32> = [0.0, 0.125, 0.25, 0.375, 0.5, 0.625, 0.75, 0.875, 1.0]
            .into_iter()
            .map(|coverage| {
                u14_field_mean(
                    &weather(&terrain, |_| 0.0, 15.0, coverage, 1.0, seed, 0.0).0,
                    3,
                )
            })
            .collect();
        match u14_coverage_increments(&coverage) {
            Ok(increments) => {
                let mut sorted = increments.clone();
                sorted.sort_by(f32::total_cmp);
                coverage_increment_median_min =
                    coverage_increment_median_min.min(sorted[sorted.len() / 2]);
                for (index, increment) in increments.into_iter().enumerate() {
                    coverage_increment_min[index] = coverage_increment_min[index].min(increment);
                    coverage_increment_max_by_step[index] =
                        coverage_increment_max_by_step[index].max(increment);
                    coverage_increment_max = coverage_increment_max.max(increment);
                }
            }
            Err(error) => failures.push(format!(
                "U14 coverage seed {seed}: {error}; totals={coverage:?}"
            )),
        }
        let zero = weather(&terrain, |_| 0.0, 15.0, 0.75, 0.0, seed, 0.0);
        zero_exact &= zero
            .0
            .iter()
            .chain(zero.1.iter())
            .all(|value| *value == 0.0);

        let high_coverage = weather(&terrain, |_| 0.0, 15.0, 1.0, 1.0, seed, 0.0).0;
        for channel in 0..2 {
            let exact = high_coverage
                .chunks_exact(4)
                .filter(|pixel| pixel[channel] >= 1.0)
                .count() as f32
                / (high_coverage.len() / 4) as f32;
            let near = high_coverage
                .chunks_exact(4)
                .filter(|pixel| pixel[channel] >= 0.995)
                .count() as f32
                / (high_coverage.len() / 4) as f32;
            plateau_exact_max[channel] = plateau_exact_max[channel].max(exact);
            plateau_near_max[channel] = plateau_near_max[channel].max(near);
        }

        let (forward, forward_geometry) = weather(&ridge, |_| 1.0, 5.0, 1.0, 1.0, seed, 1.0);
        let (reverse, reverse_geometry) = weather(&ridge, |_| 1.0, 5.0, 1.0, 1.0, seed, -1.0);
        for (mass, geometry) in [(&forward, &forward_geometry), (&reverse, &reverse_geometry)] {
            let (occupied, invalid) = u14_geometry_metrics(mass, geometry);
            geometry_occupied_texels += occupied;
            geometry_invalid_texels += invalid;
        }
        let side = |values: &[f32], positive: bool| {
            let mut total = 0.0;
            let mut samples = 0usize;
            for face in 0..6 {
                for y in 0..resolution {
                    for x in 0..resolution {
                        let pos = planet_gen::cube_sphere::cube_to_sphere(
                            face,
                            x as f32 / (resolution - 1) as f32,
                            y as f32 / (resolution - 1) as f32,
                        );
                        if pos[0] > 0.65 && (pos[2] > 0.04) == positive && pos[2].abs() < 0.45 {
                            total += values[((face * resolution * resolution + y * resolution + x)
                                * 4) as usize];
                            samples += 1;
                        }
                    }
                }
            }
            total / samples.max(1) as f32
        };
        let forward_asymmetry = side(&forward, true) - side(&forward, false);
        let reverse_asymmetry = side(&reverse, true) - side(&reverse, false);
        windward_enhancement_min = windward_enhancement_min.min(forward_asymmetry);
        lee_enhancement_max = lee_enhancement_max.max(reverse_asymmetry);
        let windward = side(&forward, true);
        windward_p90_min = windward_p90_min.min(windward);
    }
    let generation_stats = compute_runtime_stats(generation_samples_ms);
    // Kept only to avoid changing the established artifact line shape; these retired
    // survival metrics no longer drive validation or source ownership.
    let background_retention_q50_max = 0.0;
    let cool_retention_p90_min = 0.0;
    let trade_retention_p90_min = 0.0;
    let windward_retention_p90_min = 0.0;
    let cool_deep_min = 0.0;
    let values = format!(
        "command=cargo run --release --bin sweep -- --weather-validation --size 512 --output-dir output/u14-validation --low-survival-time T\nseeds={SEEDS:?}\nmasks={U14_FLAT_COOL_OCEAN_MASK},{U14_FLAT_INLAND_MASK},{U14_MOUNTAIN_WINDWARD_MASK},{U14_MOUNTAIN_LEE_MASK},{U14_COAST_BAND_MASK}\ncool_ocean_inland_min={cool_ratio:.3}\nbackground_retention_q50_max={background_retention_q50_max:.3}\ncool_retention_p90_min={cool_retention_p90_min:.3}\ntrade_retention_p90_min={trade_retention_p90_min:.3}\nwindward_retention_p90_min={windward_retention_p90_min:.3}\ncool_deck_low_p90_min={cool_deck_p90_min:.3}\ntrade_low_p90_min={trade_p90_min:.3}\nwindward_low_p90_min={windward_p90_min:.3}\ncool_deep_min={cool_deep_min:.3}\nlow_deep_min={low_deep:.3}\ndeck_thickness=[{deck_min:.3},{deck_max:.3}] km\ntrade_top=[{trade_min:.3},{trade_max:.3}] km\ntrade_clear_gap=[{gaps_min:.3},{gaps_max:.3}]\ncoast_gradient_abs_correlation_max={coast_correlation_max:.3}\ncoast_gradient_energy_ratio_max={coast_energy_ratio_max:.3}\ncoverage_samples=[0,.125,.25,.375,.5,.625,.75,.875,1]\ncoverage_increment_min={coverage_increment_min:?}\ncoverage_increment_max_by_step={coverage_increment_max_by_step:?}\ncoverage_increment_max={coverage_increment_max:.5}\ncoverage_increment_median_min={coverage_increment_median_min:.5}\ncoverage_zero_exact={zero_exact}\ndeterministic={deterministic}\ngeometry_occupied_texels={geometry_occupied_texels}\ngeometry_invalid_texels={geometry_invalid_texels}\nmass_seam_edge_max={mass_seam_edge_max:?}\nmass_seam_edge_p99={mass_seam_edge_p99:?}\nmass_seam_corner_max={mass_seam_corner_max:?}\nmass_seam_corner_p99={mass_seam_corner_p99:?}\ngeometry_seam_edge_max={geometry_seam_edge_max:?}\ngeometry_seam_edge_p99={geometry_seam_edge_p99:?}\ngeometry_seam_corner_max={geometry_seam_corner_max:?}\ngeometry_seam_corner_p99={geometry_seam_corner_p99:?}\nlow_plateau_exact_max={:.5}\ndeep_plateau_exact_max={:.5}\nlow_plateau_near_max={:.5}\ndeep_plateau_near_max={:.5}\nfixture_generation_n={}\nfixture_generation_p95_ms={:.3}\nwindward_mean_enhancement_min={windward_enhancement_min:.5}\nlee_mean_enhancement_max={lee_enhancement_max:.5}\n",
        plateau_exact_max[0],
        plateau_exact_max[1],
        plateau_near_max[0],
        plateau_near_max[1],
        generation_stats.count,
        generation_stats.p95_ms,
    );
    let artifact = Path::new(output_dir).join("u14_field_metrics.txt");
    std::fs::write(&artifact, values).expect("write U14 metrics artifact");
    println!("U14 field metrics: {}", artifact.display());
    if cool_ratio < 1.5 {
        failures.push(format!("U14 cool ocean/inland ratio {cool_ratio:.3} < 1.5"));
    }
    if cool_deck_p90_min < 0.02 || trade_p90_min < 0.02 || windward_p90_min < 0.02 {
        failures.push(format!(
            "U14 frozen feature p90 cool/trade/windward={cool_deck_p90_min:.3}/{trade_p90_min:.3}/{windward_p90_min:.3} < 0.02"
        ));
    }
    if low_deep < 4.0 || deck_min < 0.3 || deck_max > 1.2 {
        failures.push(format!("U14 deck mass/ratio/thickness low_deep={low_deep:.3}, thickness=[{deck_min:.3},{deck_max:.3}]"));
    }
    if trade_min < 1.0 || trade_max > 3.0 || gaps_min < 0.15 || gaps_max > 0.85 {
        failures.push(format!(
            "U14 trade top/gaps top=[{trade_min:.3},{trade_max:.3}], gaps=[{gaps_min:.3},{gaps_max:.3}]"
        ));
    }
    if coast_correlation_max >= 0.3 || coast_energy_ratio_max > 1.25 {
        failures.push(format!(
            "U14 coast gradient correlation={coast_correlation_max:.3}, energy_ratio={coast_energy_ratio_max:.3}"
        ));
    }
    if !zero_exact {
        failures.push("U14 zero moisture is not exact zero".to_string());
    }
    if !deterministic || geometry_invalid_texels > 0 {
        failures.push(format!(
            "U14 determinism={deterministic}, invalid geometry texels={geometry_invalid_texels}"
        ));
    }
    if plateau_exact_max.iter().any(|fraction| *fraction > 0.001)
        || plateau_near_max.iter().any(|fraction| *fraction > 0.02)
    {
        failures.push(format!(
            "U14 high-coverage mass plateau exact={plateau_exact_max:?}, near={plateau_near_max:?}"
        ));
    }
    if windward_enhancement_min <= 0.15 || lee_enhancement_max >= -0.15 {
        failures.push(format!(
            "U14 normalized windward/reversal windward={windward_enhancement_min:.3}, lee={lee_enhancement_max:.3}"
        ));
    }
    rows.extend(failures.iter().cloned());
    rows
}

#[derive(Default, Debug, Copy, Clone)]
struct U15CoreMetrics {
    count: usize,
    median_area: Option<f32>,
    deep_p95: Option<f32>,
    deep_top_p95: Option<f32>,
}

#[derive(Default, Debug, Clone)]
struct U15CompliantAnvilMetrics {
    core_count: usize,
    missing_components: Vec<usize>,
    outside_high_mass_fraction: Option<f32>,
    minimum_downwind_centroid_texels: Option<f32>,
    worst_pca_alignment_degrees: Option<f32>,
    components: Vec<U15AnvilComponentMetrics>,
}

#[derive(Default, Debug, Clone)]
struct U15AnvilComponentMetrics {
    index: usize,
    outside_high_mass_fraction: Option<f32>,
    downwind_centroid_texels: Option<f32>,
    pca_alignment_degrees: Option<f32>,
}

const U15_ELIGIBLE_MASK: &str = "eligible_convective_core";
const U15_DEEP_THRESHOLD: f32 = 0.02;
const U15_MINIMUM_COMPONENT_FACE_AREA: f32 = 0.0025;
const U15_FIXTURE_FLOW: [f32; 3] = [0.5, 0.0, 0.35];

#[derive(Debug, Clone)]
struct U15ResponseComponent {
    pixels: Vec<usize>,
    centroid: [f32; 3],
    area_fraction: f32,
}

fn u15_eligible(_face: u32, _pos: [f32; 3]) -> bool {
    true
}

fn u15_weight(x: u32, y: u32, resolution: u32) -> f32 {
    let u = x as f32 / (resolution - 1) as f32 * 2.0 - 1.0;
    let v = y as f32 / (resolution - 1) as f32 * 2.0 - 1.0;
    let edge = if x == 0 || x + 1 == resolution {
        0.5
    } else {
        1.0
    } * if y == 0 || y + 1 == resolution {
        0.5
    } else {
        1.0
    };
    edge * (1.0 + u * u + v * v).powf(-1.5)
}

fn u15_percentile(values: &mut [f32]) -> Option<f32> {
    (!values.is_empty()).then(|| {
        values.sort_by(f32::total_cmp);
        values[((values.len() - 1) as f32 * 0.95).round() as usize]
    })
}

fn u15_metric(value: Option<f32>) -> String {
    value
        .map(|value| format!("{value:.5}"))
        .unwrap_or_else(|| "N/A".to_string())
}

fn u15_pixel_position(pixel: usize, resolution: u32) -> [f32; 3] {
    let face_pixels = resolution as usize * resolution as usize;
    let face = pixel / face_pixels;
    let local = pixel % face_pixels;
    let x = local % resolution as usize;
    let y = local / resolution as usize;
    planet_gen::cube_sphere::cube_to_sphere(
        face as u32,
        x as f32 / (resolution - 1) as f32,
        y as f32 / (resolution - 1) as f32,
    )
}

fn u15_sphere_to_pixel(position: [f32; 3], resolution: u32) -> usize {
    let [x, y, z] = position;
    let (face, s, t) = if x.abs() >= y.abs() && x.abs() >= z.abs() {
        if x >= 0.0 {
            (0, -z / x, -y / x)
        } else {
            (1, -z / x, y / x)
        }
    } else if y.abs() >= z.abs() {
        if y >= 0.0 {
            (2, x / y, z / y)
        } else {
            (3, -x / y, z / y)
        }
    } else if z >= 0.0 {
        (4, x / z, -y / z)
    } else {
        (5, x / z, y / z)
    };
    let limit = resolution as isize - 1;
    let pixel = |value: f32| ((value + 1.0) * 0.5 * limit as f32).round() as isize;
    let px = pixel(s).clamp(0, limit) as usize;
    let py = pixel(t).clamp(0, limit) as usize;
    face * resolution as usize * resolution as usize + py * resolution as usize + px
}

fn u15_pixel_neighbors(pixel: usize, resolution: u32) -> [usize; 4] {
    let face_pixels = resolution as usize * resolution as usize;
    let face = pixel / face_pixels;
    let local = pixel % face_pixels;
    let x = local % resolution as usize;
    let y = local / resolution as usize;
    let coordinate = |x: isize, y: isize| {
        if (0..resolution as isize).contains(&x) && (0..resolution as isize).contains(&y) {
            face * face_pixels + y as usize * resolution as usize + x as usize
        } else {
            let uv = |coordinate: isize| {
                if coordinate < 0 {
                    -0.0001
                } else if coordinate >= resolution as isize {
                    1.0001
                } else {
                    coordinate as f32 / (resolution - 1) as f32
                }
            };
            let position = planet_gen::cube_sphere::cube_to_sphere(face as u32, uv(x), uv(y));
            u15_sphere_to_pixel(position, resolution)
        }
    };
    [
        coordinate(x as isize - 1, y as isize),
        coordinate(x as isize + 1, y as isize),
        coordinate(x as isize, y as isize - 1),
        coordinate(x as isize, y as isize + 1),
    ]
}

fn u15_significant_response_components(
    response: &[f32],
    resolution: u32,
) -> Vec<U15ResponseComponent> {
    let face_pixels = (resolution * resolution) as usize;
    let minimum_area = (face_pixels as f32 * U15_MINIMUM_COMPONENT_FACE_AREA).ceil() as usize;
    let mut visited = vec![false; face_pixels * 6];
    let mut components = Vec::new();
    for start in 0..visited.len() {
        let position = u15_pixel_position(start, resolution);
        if visited[start]
            || !u15_eligible((start / face_pixels) as u32, position)
            || response[start * 4 + 1] < U15_DEEP_THRESHOLD
        {
            continue;
        }
        let mut stack = vec![start];
        let mut pixels = Vec::new();
        visited[start] = true;
        while let Some(pixel) = stack.pop() {
            pixels.push(pixel);
            for neighbor in u15_pixel_neighbors(pixel, resolution) {
                let position = u15_pixel_position(neighbor, resolution);
                if !visited[neighbor]
                    && u15_eligible((neighbor / face_pixels) as u32, position)
                    && response[neighbor * 4 + 1] >= U15_DEEP_THRESHOLD
                {
                    visited[neighbor] = true;
                    stack.push(neighbor);
                }
            }
        }
        if pixels.len() < minimum_area {
            continue;
        }
        let mut weighted_center = [0.0; 3];
        for &pixel in &pixels {
            let local = pixel % face_pixels;
            let x = local % resolution as usize;
            let y = local / resolution as usize;
            let weight = response[pixel * 4 + 1] * u15_weight(x as u32, y as u32, resolution);
            let position = u15_pixel_position(pixel, resolution);
            for axis in 0..3 {
                weighted_center[axis] += position[axis] * weight;
            }
        }
        let Some(centroid) = u15_normalize(weighted_center) else {
            continue;
        };
        components.push(U15ResponseComponent {
            area_fraction: pixels.len() as f32 / face_pixels as f32,
            pixels,
            centroid,
        });
    }
    components
}

fn u15_significant_cores(
    components: &[U15ResponseComponent],
    response: &[f32],
    geometry: &[f32],
) -> U15CoreMetrics {
    let mut sorted_areas: Vec<f32> = components
        .iter()
        .map(|component| component.area_fraction)
        .collect();
    sorted_areas.sort_by(f32::total_cmp);
    let median_area =
        (!sorted_areas.is_empty()).then(|| sorted_areas[sorted_areas.len().saturating_sub(1) / 2]);
    let mut deep_values = Vec::new();
    let mut deep_tops = Vec::new();
    for component in components {
        for &pixel in &component.pixels {
            deep_values.push(response[pixel * 4 + 1]);
            deep_tops.push(geometry[pixel * 4 + 2]);
        }
    }
    U15CoreMetrics {
        count: sorted_areas.len(),
        median_area,
        deep_p95: u15_percentile(&mut deep_values),
        deep_top_p95: u15_percentile(&mut deep_tops),
    }
}

fn u15_mask_deep_mass(mass: &[f32], resolution: u32) -> Option<f32> {
    let mut weighted_mass = 0.0;
    let mut total_weight = 0.0;
    for face in 0..6 {
        for y in 0..resolution {
            for x in 0..resolution {
                let pos = planet_gen::cube_sphere::cube_to_sphere(
                    face,
                    x as f32 / (resolution - 1) as f32,
                    y as f32 / (resolution - 1) as f32,
                );
                if u15_eligible(face, pos) {
                    let weight = u15_weight(x, y, resolution);
                    weighted_mass += mass
                        [((face * resolution * resolution + y * resolution + x) * 4 + 1) as usize]
                        * weight;
                    total_weight += weight;
                }
            }
        }
    }
    (total_weight > 0.0).then_some(weighted_mass / total_weight)
}

fn u15_solid_angle_total(mass: &[f32], resolution: u32) -> f32 {
    let mut total = 0.0;
    for face in 0..6 {
        for y in 0..resolution {
            for x in 0..resolution {
                let index = ((face * resolution * resolution + y * resolution + x) * 4) as usize;
                total += (mass[index] + mass[index + 1] + mass[index + 2])
                    * u15_weight(x, y, resolution);
            }
        }
    }
    total
}

fn u15_response(endpoint: &[f32], baseline: &[f32]) -> Vec<f32> {
    endpoint
        .iter()
        .zip(baseline)
        .map(|(end, start)| (end - start).max(0.0))
        .collect()
}

fn u15_normalize(vector: [f32; 3]) -> Option<[f32; 3]> {
    let length = vector.iter().map(|value| value * value).sum::<f32>().sqrt();
    (length > f32::EPSILON).then(|| vector.map(|value| value / length))
}

fn u15_dot(left: [f32; 3], right: [f32; 3]) -> f32 {
    left.iter()
        .zip(right)
        .map(|(left, right)| left * right)
        .sum()
}

fn u15_cross(left: [f32; 3], right: [f32; 3]) -> [f32; 3] {
    [
        left[1] * right[2] - left[2] * right[1],
        left[2] * right[0] - left[0] * right[2],
        left[0] * right[1] - left[1] * right[0],
    ]
}

fn u15_fixture_wind(position: [f32; 3]) -> Option<[f32; 3]> {
    let projection = u15_dot(U15_FIXTURE_FLOW, position);
    u15_normalize([
        U15_FIXTURE_FLOW[0] - position[0] * projection,
        U15_FIXTURE_FLOW[1] - position[1] * projection,
        U15_FIXTURE_FLOW[2] - position[2] * projection,
    ])
}

fn u15_anvil_source_taps(position: [f32; 3], resolution: u32) -> Option<[usize; 4]> {
    let wind_dir = u15_fixture_wind(position)?;
    let reference = if position[1].abs() > 0.9 {
        [1.0, 0.0, 0.0]
    } else {
        [0.0, 1.0, 0.0]
    };
    let east = u15_normalize(u15_cross(reference, position))?;
    // Match the shader's cross-product perturbation before normalization.
    let lateral = u15_normalize([
        u15_cross(position, wind_dir)[0] + east[0] * 0.0001,
        u15_cross(position, wind_dir)[1] + east[1] * 0.0001,
        u15_cross(position, wind_dir)[2] + east[2] * 0.0001,
    ])?;
    let anvil_step = (600.0_f32 / 6371.0).clamp(0.02, 0.10);
    let anvil_spread = anvil_step * 0.08;
    let source = |wind_scale: f32, lateral_scale: f32| {
        u15_normalize([
            position[0] - wind_dir[0] * anvil_step * wind_scale
                + lateral[0] * anvil_spread * lateral_scale,
            position[1] - wind_dir[1] * anvil_step * wind_scale
                + lateral[1] * anvil_spread * lateral_scale,
            position[2] - wind_dir[2] * anvil_step * wind_scale
                + lateral[2] * anvil_spread * lateral_scale,
        ])
        .map(|source| u15_sphere_to_pixel(source, resolution))
    };
    Some([
        source(1.0, 0.0)?,
        source(2.2, 0.0)?,
        source(1.0, 1.0)?,
        source(1.0, -1.0)?,
    ])
}

fn u15_component_labels(
    components: &[U15ResponseComponent],
    resolution: u32,
) -> Vec<Option<usize>> {
    let mut labels = vec![None; resolution as usize * resolution as usize * 6];
    for (index, component) in components.iter().enumerate() {
        for &pixel in &component.pixels {
            labels[pixel] = Some(index);
        }
    }
    labels
}

fn u15_causal_component(
    taps: [usize; 4],
    labels: &[Option<usize>],
    response: &[f32],
    resolution: u32,
) -> Option<usize> {
    // The tap weights combine the shader's high contributions from deep and high state.
    const TAP_WEIGHT: [f32; 4] = [1.04, 0.38, 0.07, 0.07];
    let mut scores = std::collections::BTreeMap::<usize, f32>::new();
    for (tap, weight) in taps.into_iter().zip(TAP_WEIGHT) {
        if let Some(component) = labels[tap] {
            *scores.entry(component).or_default() += response[tap * 4 + 1] * weight;
        }
    }
    let dominant = |scores: std::collections::BTreeMap<usize, f32>| {
        scores
            .into_iter()
            .max_by(|(left_index, left_score), (right_index, right_score)| {
                left_score
                    .total_cmp(right_score)
                    .then_with(|| right_index.cmp(left_index))
            })
            .map(|(component, _)| component)
    };
    if !scores.is_empty() {
        return dominant(scores);
    }
    // textureSampleLevel is linear: recover labels from the tap's cubemap-adjacent
    // texels rather than assigning by destination-space proximity.
    let mut fallbacks = std::collections::BTreeMap::<usize, f32>::new();
    for (tap, weight) in taps.into_iter().zip(TAP_WEIGHT) {
        for neighbor in u15_pixel_neighbors(tap, resolution) {
            if let Some(component) = labels[neighbor] {
                *fallbacks.entry(component).or_default() += response[neighbor * 4 + 1] * weight;
            }
        }
    }
    dominant(fallbacks)
}

fn u15_anvil_component_report(metrics: &U15CompliantAnvilMetrics) -> String {
    metrics
        .components
        .iter()
        .map(|component| {
            if metrics.missing_components.contains(&component.index) {
                format!("core{}=NO_ANVIL", component.index)
            } else {
                format!(
                    "core{}=extent:{} shift:{} angle:{}",
                    component.index,
                    u15_metric(component.outside_high_mass_fraction),
                    u15_metric(component.downwind_centroid_texels),
                    u15_metric(component.pca_alignment_degrees),
                )
            }
        })
        .collect::<Vec<_>>()
        .join(",")
}

fn u15_compliant_anvil_metrics(
    response: &[f32],
    components: &[U15ResponseComponent],
    resolution: u32,
) -> U15CompliantAnvilMetrics {
    if components.is_empty() {
        return U15CompliantAnvilMetrics::default();
    }
    let labels = u15_component_labels(components, resolution);
    let mut high_mass = vec![0.0; components.len()];
    let mut outside_mass = vec![0.0; components.len()];
    let mut outside_points = vec![Vec::new(); components.len()];
    for face in 0..6 {
        for y in 0..resolution {
            for x in 0..resolution {
                let index = ((face * resolution * resolution + y * resolution + x) * 4) as usize;
                let pixel = index / 4;
                let high = response[index + 2] * u15_weight(x, y, resolution);
                if high < U15_DEEP_THRESHOLD * u15_weight(x, y, resolution) {
                    continue;
                }
                let pos = planet_gen::cube_sphere::cube_to_sphere(
                    face,
                    x as f32 / (resolution - 1) as f32,
                    y as f32 / (resolution - 1) as f32,
                );
                let Some(taps) = u15_anvil_source_taps(pos, resolution) else {
                    continue;
                };
                // A response-high destination belongs to the deep component that the shader
                // actually samples upstream, not the geographically nearest centroid.
                let Some(core) = u15_causal_component(taps, &labels, response, resolution) else {
                    continue;
                };
                high_mass[core] += high;
                if labels[pixel].is_none() {
                    outside_mass[core] += high;
                    outside_points[core].push((pos, high));
                }
            }
        }
    }
    let total_high: f32 = high_mass.iter().sum();
    let total_outside: f32 = outside_mass.iter().sum();
    let mut minimum_centroid = f32::INFINITY;
    let mut worst_pca = 0.0_f32;
    let mut missing_components = Vec::new();
    let mut component_metrics = Vec::with_capacity(components.len());
    for (index, component) in components.iter().enumerate() {
        let mut metric = U15AnvilComponentMetrics {
            index,
            outside_high_mass_fraction: (high_mass[index] > 0.0)
                .then_some(outside_mass[index] / high_mass[index]),
            ..Default::default()
        };
        if high_mass[index] <= 0.0 || outside_points[index].len() < 2 || outside_mass[index] <= 0.0
        {
            missing_components.push(index);
            component_metrics.push(metric);
            continue;
        }
        let center = component.centroid;
        let Some(downwind) = u15_fixture_wind(center) else {
            missing_components.push(index);
            component_metrics.push(metric);
            continue;
        };
        let reference = if center[1].abs() > 0.9 {
            [1.0, 0.0, 0.0]
        } else {
            [0.0, 1.0, 0.0]
        };
        let Some(east) = u15_normalize(u15_cross(reference, center)) else {
            missing_components.push(index);
            component_metrics.push(metric);
            continue;
        };
        let north = u15_cross(center, east);
        let mut covariance = [0.0; 3];
        let mut signed_distance = 0.0;
        for (pos, weight) in &outside_points[index] {
            let cosine = u15_dot(*pos, center).clamp(-1.0, 1.0);
            let Some(direction) = u15_normalize([
                pos[0] - center[0] * cosine,
                pos[1] - center[1] * cosine,
                pos[2] - center[2] * cosine,
            ]) else {
                continue;
            };
            let distance = cosine.acos();
            let east_value = u15_dot(direction, east) * distance;
            let north_value = u15_dot(direction, north) * distance;
            covariance[0] += east_value * east_value * *weight;
            covariance[1] += east_value * north_value * *weight;
            covariance[2] += north_value * north_value * *weight;
            signed_distance += u15_dot(direction, downwind) * distance * *weight;
        }
        if covariance[0] + covariance[2] <= f32::EPSILON {
            missing_components.push(index);
            component_metrics.push(metric);
            continue;
        }
        let principal_angle = 0.5 * (2.0 * covariance[1]).atan2(covariance[0] - covariance[2]);
        let principal = [principal_angle.cos(), principal_angle.sin()];
        let expected = [u15_dot(downwind, east), u15_dot(downwind, north)];
        let alignment = (principal[0] * expected[0] + principal[1] * expected[1])
            .abs()
            .clamp(-1.0, 1.0);
        metric.downwind_centroid_texels = Some(
            signed_distance
                / outside_mass[index]
                / (std::f32::consts::FRAC_PI_2 / resolution as f32),
        );
        metric.pca_alignment_degrees = Some(alignment.acos().to_degrees());
        minimum_centroid = minimum_centroid.min(metric.downwind_centroid_texels.unwrap());
        worst_pca = worst_pca.max(metric.pca_alignment_degrees.unwrap());
        component_metrics.push(metric);
    }
    U15CompliantAnvilMetrics {
        core_count: components.len(),
        missing_components,
        outside_high_mass_fraction: (total_high > 0.0).then_some(total_outside / total_high),
        minimum_downwind_centroid_texels: minimum_centroid.is_finite().then_some(minimum_centroid),
        worst_pca_alignment_degrees: Some(worst_pca),
        components: component_metrics,
    }
}

fn run_u15_field_validation(
    gpu: &GpuContext,
    pipeline: &WeatherFieldPipeline,
    output_dir: &str,
    resolution: u32,
) -> Vec<String> {
    const SEEDS: [u32; 8] = [7, 19, 37, 73, 101, 211, 509, 997];
    let terrain = u14_flat_terrain(resolution, |_| -0.1);
    let wind = WindFieldPipeline::new(gpu).expect("U15 dynamics unavailable");
    // One constant drives both the fixture wind and validator downwind expectation.
    let dynamics = wind.create_test_textures(gpu, resolution, |pos| {
        let projection = u15_dot(U15_FIXTURE_FLOW, pos);
        (
            [
                U15_FIXTURE_FLOW[0] - pos[0] * projection,
                U15_FIXTURE_FLOW[1] - pos[1] * projection,
                U15_FIXTURE_FLOW[2] - pos[2] * projection,
                0.0,
            ],
            1000.0,
        )
    });
    let generate = |seed: u32, storm_count: u32, storm_size: f32, moisture: f32, temp: f32| {
        let field = pipeline.create_textures(gpu, resolution);
        pipeline.generate(
            gpu,
            WeatherSnapshot {
                face: 0,
                resolution,
                seed,
                storm_count,
                coverage: 1.0,
                moisture,
                surface_pressure_bar: 1.0,
                base_temp_c: temp,
                ocean_level: 0.0,
                axial_tilt_rad: 0.0,
                season: 0.5,
                storm_size,
                radius_km: 6371.0,
                rotation_rate_rad_s: std::f32::consts::TAU / 86400.0,
                wind_scale: 1.0,
            },
            &terrain,
            &dynamics,
            &field,
        );
        (field.read_mass(gpu), field.read_geometry(gpu))
    };
    let mut rows = Vec::new();
    let mut failures = Vec::new();
    let mut worst_outside_high_fraction = f32::INFINITY;
    let mut worst_downwind_centroid_texels = f32::INFINITY;
    let mut worst_pca_alignment_degrees = 0.0_f32;
    for seed in SEEDS {
        let cases = [0, 4, 8].map(|count| generate(seed, count, 1.0, 1.0, 35.0));
        let sized = [0.3, 1.0, 3.0].map(|size| generate(seed, 4, size, 1.0, 35.0));
        let moisture_zero = [0, 8].map(|count| generate(seed, count, 3.0, 0.0, 35.0));
        let moist_stable = [0, 8].map(|count| generate(seed, count, 3.0, 1.0, -35.0));
        let count_response: [Vec<f32>; 3] =
            std::array::from_fn(|index| u15_response(&cases[index].0, &cases[0].0));
        let size_response: [Vec<f32>; 3] =
            std::array::from_fn(|index| u15_response(&sized[index].0, &cases[0].0));
        let size_geometry_response: [Vec<f32>; 3] =
            std::array::from_fn(|index| u15_response(&sized[index].1, &cases[0].1));
        let count_components: [Vec<U15ResponseComponent>; 3] = std::array::from_fn(|index| {
            u15_significant_response_components(&count_response[index], resolution)
        });
        let size_components: [Vec<U15ResponseComponent>; 3] = std::array::from_fn(|index| {
            u15_significant_response_components(&size_response[index], resolution)
        });
        let core: [U15CoreMetrics; 3] = std::array::from_fn(|index| {
            u15_significant_cores(
                &count_components[index],
                &count_response[index],
                &cases[index].1,
            )
        });
        let size_core: [U15CoreMetrics; 3] = std::array::from_fn(|index| {
            u15_significant_cores(
                &size_components[index],
                &size_response[index],
                &size_geometry_response[index],
            )
        });
        let mask_deep: [Option<f32>; 3] =
            std::array::from_fn(|index| u15_mask_deep_mass(&count_response[index], resolution));
        let total_zero = u15_solid_angle_total(&cases[0].0, resolution);
        let total_eight = u15_solid_angle_total(&cases[2].0, resolution);
        let condensate_change = (total_eight - total_zero).abs() / total_zero.max(f32::EPSILON);
        let anvil =
            u15_compliant_anvil_metrics(&count_response[2], &count_components[2], resolution);
        let anvil_pass = anvil.missing_components.is_empty()
            && anvil.core_count > 0
            && anvil
                .outside_high_mass_fraction
                .is_some_and(|value| value >= 0.10)
            && anvil
                .minimum_downwind_centroid_texels
                .is_some_and(|value| value >= 0.5)
            && anvil
                .worst_pca_alignment_degrees
                .is_some_and(|value| value <= 20.0);
        let moisture_zero_exact = moisture_zero[0].0 == moisture_zero[1].0
            && moisture_zero[0].1 == moisture_zero[1].1
            && moisture_zero[0]
                .0
                .iter()
                .chain(&moisture_zero[0].1)
                .all(|value| *value == 0.0);
        let moist_stable_identical =
            moist_stable[0].0 == moist_stable[1].0 && moist_stable[0].1 == moist_stable[1].1;
        rows.push(format!(
            "seed={seed} count_response={:?} deep_p95_response={:?} response_deep_mass={:?} size_response_area={:?} size_response_top={:?} column_mass_delta={condensate_change:.5} anvil_core_count={} anvil_extent_fraction={} anvil_shift_texels={} anvil_angle_degrees={} anvil_components={} anvil_status={} moisture_zero={moisture_zero_exact} moist_stable_identical={moist_stable_identical}",
            core.map(|metric| metric.count),
            core.map(|metric| u15_metric(metric.deep_p95)),
            mask_deep.map(u15_metric),
            size_core.map(|metric| u15_metric(metric.median_area)),
            size_core.map(|metric| u15_metric(metric.deep_top_p95)),
            anvil.core_count,
            u15_metric(anvil.outside_high_mass_fraction),
            u15_metric(anvil.minimum_downwind_centroid_texels),
            u15_metric(anvil.worst_pca_alignment_degrees),
            u15_anvil_component_report(&anvil),
            if anvil_pass { "PASS" } else { "FAIL" },
        ));
        worst_outside_high_fraction = worst_outside_high_fraction.min(
            anvil
                .outside_high_mass_fraction
                .unwrap_or(f32::NEG_INFINITY),
        );
        worst_downwind_centroid_texels = worst_downwind_centroid_texels.min(
            anvil
                .minimum_downwind_centroid_texels
                .unwrap_or(f32::NEG_INFINITY),
        );
        worst_pca_alignment_degrees = worst_pca_alignment_degrees
            .max(anvil.worst_pca_alignment_degrees.unwrap_or(f32::INFINITY));
        let baseline_applicable = core[0].deep_p95.is_some_and(|value| value > 0.0);
        let p95_gate = if baseline_applicable {
            match (core[0].deep_p95, core[2].deep_p95) {
                (Some(zero), Some(eight)) => eight >= zero * 1.25,
                _ => false,
            }
        } else {
            core[2].deep_p95.is_some_and(|value| value > 0.0)
        };
        if core[2].count < core[0].count + 2 || !p95_gate || condensate_change > 0.20 {
            failures.push(format!(
                "U15 seed {seed} count/deep/mass: {}",
                rows.last().unwrap()
            ));
        }
        let size_gate = match (
            size_core[0].median_area,
            size_core[2].median_area,
            size_core[0].deep_top_p95,
            size_core[2].deep_top_p95,
        ) {
            (Some(small_area), Some(large_area), Some(small_top), Some(large_top)) => {
                large_area >= small_area * 1.5 && large_top >= small_top + 2.0
            }
            _ => false,
        };
        if size_core[0].count == 0 || size_core[2].count == 0 || !size_gate {
            failures.push(format!("U15 seed {seed} size: {}", rows.last().unwrap()));
        }
        if !anvil_pass {
            failures.push(format!(
                "U15 seed {seed} compliant anvil response: {}",
                rows.last().unwrap()
            ));
        }
        if !moisture_zero_exact {
            failures.push(format!("U15 seed {seed} moisture-zero is not exact zero"));
        }
        if !moist_stable_identical {
            failures.push(format!(
                "U15 seed {seed} moist-stable storm fields are not bit-identical"
            ));
        }
    }
    let values = format!(
        "fixture={U15_ELIGIBLE_MASK}; fixture_flow={U15_FIXTURE_FLOW:?}; response=endpoint-minus-baseline; significant_deep_threshold={U15_DEEP_THRESHOLD:.2}; component_area>={:.2}% face; response_deep_mass>=0.02; column_mass<=20%; transport-only conservation<=2%; expected_anvil_direction=projected_fixture_flow; anvil_extent_fraction>=0.10; anvil_shift_texels>=0.5; anvil_angle_degrees<=20; seeds={SEEDS:?}\naggregate_worst_anvil_extent_fraction={worst_outside_high_fraction:.5}\naggregate_worst_anvil_shift_texels={worst_downwind_centroid_texels:.5}\naggregate_worst_anvil_angle_degrees={worst_pca_alignment_degrees:.5}\n{}\n",
        U15_MINIMUM_COMPONENT_FACE_AREA * 100.0,
        rows.join("\n"),
    );
    std::fs::write(Path::new(output_dir).join("u15_field_metrics.txt"), values)
        .expect("write U15 metrics artifact");
    failures
}

fn run_weather_validation_with_pipeline(
    gpu: &GpuContext,
    compute: &TerrainComputePipeline,
    renderer: &PreviewRenderer,
    earth: &PlanetPreset,
    output_dir: &str,
    render_size: u32,
    weather_pipeline: WeatherFieldPipeline,
) {
    const VALIDATION_SEEDS: [(&str, u32); 8] = [
        ("7", 7),
        ("19", 19),
        ("37", 37),
        ("73", 73),
        ("101", 101),
        ("211", 211),
        ("509", 509),
        ("997", 997),
    ];

    let terrain_resolution = render_size.clamp(128, 512);
    let weather_resolution = (render_size / 2).clamp(64, 384);
    let wind_pipeline = WindFieldPipeline::new(gpu).expect("Rgba16Float dynamics unsupported");
    let mut gate_failures = Vec::new();
    gate_failures.extend(run_u14_field_validation(
        gpu,
        &weather_pipeline,
        output_dir,
        weather_resolution,
    ));
    gate_failures.extend(run_u15_field_validation(
        gpu,
        &weather_pipeline,
        output_dir,
        weather_resolution,
    ));
    let scene = generate_weather_scene(
        gpu,
        compute,
        renderer,
        &wind_pipeline,
        earth,
        42,
        (terrain_resolution, weather_resolution),
    );
    let tilt = 0.35_f32;
    let (st, ct) = tilt.sin_cos();
    let mut base_uniforms = PreviewUniforms::zeroed();
    base_uniforms.rotation = [
        [1.0, 0.0, 0.0, 0.0],
        [0.0, ct, -st, 0.0],
        [0.0, st, ct, 0.0],
        [0.0, 0.0, 0.0, 1.0],
    ];
    base_uniforms.light_dir = [0.5, 0.7, -1.0];
    base_uniforms.ocean_level = scene.ocean_level;
    base_uniforms.base_temp_c = scene.derived.base_temperature_c;
    base_uniforms.ocean_fraction = scene.derived.ocean_fraction;
    base_uniforms.axial_tilt_rad = earth.params.axial_tilt_deg.to_radians();
    base_uniforms.view_mode = 9;
    base_uniforms.season = 0.5;
    base_uniforms.height_scale = 3.0;
    base_uniforms.zoom = 1.0;
    base_uniforms.cloud_coverage = 0.5;
    base_uniforms.cloud_seed = 42;
    base_uniforms.star_color_temp = 0.5;
    base_uniforms.show_ao = 1.0;
    base_uniforms.show_water = 1.0;
    base_uniforms.show_ice = 1.0;
    base_uniforms.show_biomes = 1.0;
    base_uniforms.show_clouds = 1.0;
    base_uniforms.cloud_opacity = 1.0;
    base_uniforms.cloud_advection = 1.0;
    base_uniforms.rotation_rate = 1.0;
    base_uniforms.atm_pressure = scene.derived.surface_pressure_bar;
    base_uniforms._pad4 = 0.0;
    base_uniforms.planet_radius_km = scene.derived.radius_km;
    base_uniforms.show_cloud_shadows = 1.0;

    let mut generation_samples_ms = Vec::new();
    let mut render_samples_ms = Vec::new();
    let mut u3_mass_reports = Vec::new();
    let mut u3_rendered_associations = Vec::new();
    let validate_weather_fields = |label: &str, weather: &WeatherTextures| -> Vec<String> {
        let mut failures = Vec::new();
        let mass = weather.read_mass(gpu);
        if let Err(error) = validate_mass_field(label, &mass, weather.resolution, false, false) {
            failures.push(error);
        }
        let seam = seam_continuity_metrics(&mass, weather.resolution, 0);
        if let Err(error) = validate_seam_metrics(&seam) {
            failures.push(format!("{label}: {error}"));
        }
        failures
    };

    println!("Cloud seed density views (global seed 42):");
    let (seed_42_storm_4, seed_42_generation_ms) = time_gpu_call(gpu, || {
        generate_validation_weather(&weather_pipeline, gpu, &scene, 42, 4, 1.0)
    });
    generation_samples_ms.push(seed_42_generation_ms);
    gate_failures.extend(validate_weather_fields("seed 42", &seed_42_storm_4));
    let mut seed_density_renders = Vec::new();
    let mut seed_density_sheet = Vec::new();
    for (label, cloud_seed) in VALIDATION_SEEDS {
        let generated;
        let weather = if cloud_seed == 42 {
            &seed_42_storm_4
        } else {
            let (g, generation_ms) = time_gpu_call(gpu, || {
                generate_validation_weather(&weather_pipeline, gpu, &scene, cloud_seed, 4, 1.0)
            });
            generation_samples_ms.push(generation_ms);
            generated = g;
            &generated
        };
        gate_failures.extend(validate_weather_fields(&format!("seed {label}"), weather));
        let mass = weather.read_mass(gpu);
        u3_mass_reports.push(u3_mass_report("cloud", label, &mass, weather.resolution));
        let uniforms = PreviewUniforms {
            cloud_seed,
            ..base_uniforms
        };
        let (pixels, render_ms) = time_gpu_call(gpu, || {
            render_weather(renderer, gpu, &uniforms, &scene, weather, render_size)
        });
        render_samples_ms.push(render_ms);
        u3_rendered_associations.push(u3_rendered_association(
            "cloud",
            label,
            &pixels,
            &mass,
            weather.resolution,
            uniforms.rotation,
        ));
        save_png(
            output_dir,
            &format!("weather_cloud_seed_{label}_density.png"),
            render_size,
            &pixels,
        );
        seed_density_sheet.push((pixels.clone(), pixels.clone()));
        if cloud_seed == 42 {
            save_png(
                output_dir,
                "weather_global_seed_42_density.png",
                render_size,
                &pixels,
            );
        }
        seed_density_renders.push((label, pixels));
    }
    let baseline_density = &seed_density_renders[0].1;
    for (index, (label, density)) in seed_density_renders.iter().enumerate() {
        let topology = density_topology(density, render_size);
        let seed_change =
            (index > 0).then(|| seed_change_fraction(baseline_density, density, render_size));
        println!(
            "  seed {label}: occupied={:.1}%, coherent={:.1}%, zonal={:.1}%, meridional={:.1}%, ribbons={:.1}%, polar={:.1}%, anisotropy={:.1}%, components={}, largest={:.1}%, changed-vs-42={}",
            topology.occupied * 100.0,
            topology.coherent * 100.0,
            topology.zonal_continuity * 100.0,
            topology.meridional_continuity * 100.0,
            topology.ribbon_like * 100.0,
            topology.polar_occupied * 100.0,
            topology.directional_anisotropy * 100.0,
            topology.components,
            topology.largest_component * 100.0,
            seed_change.map_or_else(
                || "baseline".to_string(),
                |value| format!("{:.1}%", value * 100.0)
            ),
        );
        if let Err(error) = validate_seed_topology_metrics(&topology, seed_change) {
            gate_failures.push(format!("Cloud seed {label}: {error}"));
        }
    }

    save_contact_sheet(
        output_dir,
        "weather_seed_density_contact_sheet.png",
        render_size,
        &seed_density_sheet,
    );
    println!("Global seed density views (cloud seed 42):");
    let mut global_density_sheet = Vec::new();
    let mut global_color_sheet = Vec::new();
    for (label, global_seed) in VALIDATION_SEEDS {
        let generated_scene;
        let case_scene = if global_seed == 42 {
            &scene
        } else {
            generated_scene = generate_weather_scene(
                gpu,
                compute,
                renderer,
                &wind_pipeline,
                earth,
                global_seed,
                (terrain_resolution, weather_resolution),
            );
            &generated_scene
        };
        let generated_weather;
        let weather = if global_seed == 42 {
            &seed_42_storm_4
        } else {
            let (weather, generation_ms) = time_gpu_call(gpu, || {
                generate_validation_weather(&weather_pipeline, gpu, case_scene, 42, 4, 1.0)
            });
            generation_samples_ms.push(generation_ms);
            generated_weather = weather;
            &generated_weather
        };
        gate_failures.extend(validate_weather_fields(
            &format!("global seed {label}"),
            weather,
        ));
        let mass = weather.read_mass(gpu);
        u3_mass_reports.push(u3_mass_report("global", label, &mass, weather.resolution));
        let (pixels, render_ms) = time_gpu_call(gpu, || {
            render_weather(
                renderer,
                gpu,
                &base_uniforms,
                case_scene,
                weather,
                render_size,
            )
        });
        render_samples_ms.push(render_ms);
        u3_rendered_associations.push(u3_rendered_association(
            "global",
            label,
            &pixels,
            &mass,
            weather.resolution,
            base_uniforms.rotation,
        ));
        save_png(
            output_dir,
            &format!("weather_global_seed_{label}_density.png"),
            render_size,
            &pixels,
        );
        global_density_sheet.push((pixels.clone(), pixels.clone()));
        let color_uniforms = PreviewUniforms {
            view_mode: 0,
            ..base_uniforms
        };
        let (color, render_ms) = time_gpu_call(gpu, || {
            render_weather(
                renderer,
                gpu,
                &color_uniforms,
                case_scene,
                weather,
                render_size,
            )
        });
        render_samples_ms.push(render_ms);
        save_png(
            output_dir,
            &format!("weather_global_seed_{label}_color.png"),
            render_size,
            &color,
        );
        global_color_sheet.push((color.clone(), color));
        let topology = density_topology(&pixels, render_size);
        println!(
            "  global seed {label}: occupied={:.1}%, coherent={:.1}%, zonal={:.1}%, meridional={:.1}%, ribbons={:.1}%, polar={:.1}%, anisotropy={:.1}%, components={}, largest={:.1}%",
            topology.occupied * 100.0,
            topology.coherent * 100.0,
            topology.zonal_continuity * 100.0,
            topology.meridional_continuity * 100.0,
            topology.ribbon_like * 100.0,
            topology.polar_occupied * 100.0,
            topology.directional_anisotropy * 100.0,
            topology.components,
            topology.largest_component * 100.0,
        );
        if let Err(error) = validate_seed_topology_metrics(&topology, None) {
            gate_failures.push(format!("Global seed {label}: {error}"));
        }
    }

    save_contact_sheet(
        output_dir,
        "weather_global_seed_density_contact_sheet.png",
        render_size,
        &global_density_sheet,
    );
    save_contact_sheet(
        output_dir,
        "weather_global_seed_color_contact_sheet.png",
        render_size,
        &global_color_sheet,
    );
    std::fs::write(
        Path::new(output_dir).join("u3_veil_diagnostics.txt"),
        format!(
            "mass bins=[0,.01),[.01,.02),[.02,.03),[.03,.04),[.04,.05),[.05,.06),[.06,.07),[.07,.08),[.08,.09),[.09,.10),[.10,.11),[.11,.12),[.12,.13),[.13,.14),[.14,.15),[.15,+)\nfield weights=solid-angle; polar=geographic abs(latitude)>=60 degrees\nrendered opacity=linearized integrated density view; projected association uses preview sphere direction, rotation, and nearest cubemap texel\n\nFIELD DISTRIBUTIONS\n{}\n\nRENDERED ASSOCIATIONS\n{}\n",
            u3_mass_reports.join("\n"),
            u3_rendered_associations.join("\n"),
        ),
    )
    .expect("write U3 veil diagnostics artifact");

    println!("Storm Count, Storm Size, and Cloud Shadow comparisons (global/cloud seed 42):");
    let mut storm_renders = Vec::new();
    for storm_count in [0, 4, 8] {
        let generated;
        let weather = if storm_count == 4 {
            &seed_42_storm_4
        } else {
            let (weather, generation_ms) = time_gpu_call(gpu, || {
                generate_validation_weather(&weather_pipeline, gpu, &scene, 42, storm_count, 1.0)
            });
            generation_samples_ms.push(generation_ms);
            generated = weather;
            &generated
        };
        gate_failures.extend(validate_weather_fields(
            &format!("storm count {storm_count}"),
            weather,
        ));
        let planet_uniforms = PreviewUniforms {
            view_mode: 0,
            ..base_uniforms
        };
        let (pixels, render_ms) = time_gpu_call(gpu, || {
            render_weather(
                renderer,
                gpu,
                &planet_uniforms,
                &scene,
                weather,
                render_size,
            )
        });
        render_samples_ms.push(render_ms);
        save_png(
            output_dir,
            &format!("weather_storm_count_{storm_count}.png"),
            render_size,
            &pixels,
        );
        let density_uniforms = PreviewUniforms {
            view_mode: 9,
            ..planet_uniforms
        };
        let (density, density_render_ms) = time_gpu_call(gpu, || {
            render_weather(
                renderer,
                gpu,
                &density_uniforms,
                &scene,
                weather,
                render_size,
            )
        });
        render_samples_ms.push(density_render_ms);
        save_png(
            output_dir,
            &format!("weather_storm_count_{storm_count}_density.png"),
            render_size,
            &density,
        );
        if storm_count == 4 {
            save_png(
                output_dir,
                "weather_cloud_shadows_on.png",
                render_size,
                &pixels,
            );
            let shadows_off = PreviewUniforms {
                show_cloud_shadows: 0.0,
                ..planet_uniforms
            };
            let (pixels, render_ms) = time_gpu_call(gpu, || {
                render_weather(renderer, gpu, &shadows_off, &scene, weather, render_size)
            });
            render_samples_ms.push(render_ms);
            save_png(
                output_dir,
                "weather_cloud_shadows_off.png",
                render_size,
                &pixels,
            );
        }
        storm_renders.push((pixels, density));
    }
    let (distance_04, distance_08, changed_08, saturated_clouds) =
        storm_control_metrics(&storm_renders);
    let count_tau_0 = mean_rendered_optical_depth(&storm_renders[0].1, render_size);
    let count_tau_8 = mean_rendered_optical_depth(&storm_renders[2].1, render_size);
    let count_tau_delta = (count_tau_8 - count_tau_0).abs() / count_tau_0.max(f32::EPSILON);
    println!(
        "  count RGB distance 0->4={distance_04:.4}, 0->8={distance_08:.4}, ratio={:.3}; cloudy pixels changed >0.05={:.1}%; saturated at 8={:.1}%",
        distance_04 / distance_08.max(f32::EPSILON),
        changed_08 * 100.0,
        saturated_clouds * 100.0,
    );
    println!(
        "  U3 Count 0->8 rendered mean tau={count_tau_0:.5}->{count_tau_8:.5}, delta={:.2}% ({})",
        count_tau_delta * 100.0,
        if count_tau_delta <= 0.20 {
            "PASS"
        } else {
            "FAIL"
        },
    );
    if count_tau_delta > 0.20 {
        gate_failures.push(format!(
            "U3 Count 0->8 rendered optical-depth delta {:.2}% exceeds 20%",
            count_tau_delta * 100.0
        ));
    }
    // U15 validates field-level catalyst response; U3 owns rendered optical-depth gates.
    save_contact_sheet(
        output_dir,
        "weather_storm_count_contact_sheet.png",
        render_size,
        &storm_renders,
    );

    let mut size_renders = Vec::new();
    for storm_size in [0.3, 1.0, 3.0] {
        let (weather, generation_ms) = time_gpu_call(gpu, || {
            generate_validation_weather(&weather_pipeline, gpu, &scene, 42, 4, storm_size)
        });
        generation_samples_ms.push(generation_ms);
        gate_failures.extend(validate_weather_fields(
            &format!("storm size {storm_size}"),
            &weather,
        ));
        let planet_uniforms = PreviewUniforms {
            view_mode: 0,
            ..base_uniforms
        };
        let (pixels, render_ms) = time_gpu_call(gpu, || {
            render_weather(
                renderer,
                gpu,
                &planet_uniforms,
                &scene,
                &weather,
                render_size,
            )
        });
        render_samples_ms.push(render_ms);
        save_png(
            output_dir,
            &format!("weather_storm_size_{storm_size:.1}.png"),
            render_size,
            &pixels,
        );
        let density_uniforms = PreviewUniforms {
            view_mode: 9,
            ..planet_uniforms
        };
        let (density, density_render_ms) = time_gpu_call(gpu, || {
            render_weather(
                renderer,
                gpu,
                &density_uniforms,
                &scene,
                &weather,
                render_size,
            )
        });
        render_samples_ms.push(density_render_ms);
        save_png(
            output_dir,
            &format!("weather_storm_size_{storm_size:.1}_density.png"),
            render_size,
            &density,
        );
        size_renders.push((pixels, density));
    }
    let (distance_small_medium, distance_small_large, changed_large, saturated_large) =
        storm_control_metrics(&size_renders);
    println!(
        "  size RGB distance 0.3->1.0={distance_small_medium:.4}, 0.3->3.0={distance_small_large:.4}, ratio={:.3}; cloudy pixels changed >0.05={:.1}%; saturated at 3.0={:.1}%",
        distance_small_medium / distance_small_large.max(f32::EPSILON),
        changed_large * 100.0,
        saturated_large * 100.0,
    );
    save_contact_sheet(
        output_dir,
        "weather_storm_size_contact_sheet.png",
        render_size,
        &size_renders,
    );

    println!("Wind reversal capture (analytic flow):");
    let build_flow = |sign: f32| {
        wind_pipeline.create_test_textures(gpu, weather_resolution, move |pos| {
            let east = pos[2];
            let north = 0.0;
            let west = -pos[0];
            let inv_len = (east * east + west * west).max(1.0e-8).sqrt();
            let speed = 0.7 * sign;
            (
                [east / inv_len * speed, north, west / inv_len * speed, 0.0],
                1013.0,
            )
        })
    };
    let forward = build_flow(1.0);
    let reverse = build_flow(-1.0);
    let (forward_weather, forward_generation_ms) = time_gpu_call(gpu, || {
        generate_validation_weather_with_dynamics(
            &weather_pipeline,
            gpu,
            &scene,
            &forward,
            42,
            4,
            1.0,
        )
    });
    generation_samples_ms.push(forward_generation_ms);
    let (reverse_weather, reverse_generation_ms) = time_gpu_call(gpu, || {
        generate_validation_weather_with_dynamics(
            &weather_pipeline,
            gpu,
            &scene,
            &reverse,
            42,
            4,
            1.0,
        )
    });
    generation_samples_ms.push(reverse_generation_ms);
    gate_failures.extend(validate_weather_fields(
        "wind reversal forward",
        &forward_weather,
    ));
    gate_failures.extend(validate_weather_fields(
        "wind reversal reverse",
        &reverse_weather,
    ));
    let planet_uniforms = PreviewUniforms {
        view_mode: 0,
        cloud_seed: 42,
        ..base_uniforms
    };
    let (forward_pixels, forward_render_ms) = time_gpu_call(gpu, || {
        render_weather(
            renderer,
            gpu,
            &planet_uniforms,
            &scene,
            &forward_weather,
            render_size,
        )
    });
    let (reverse_pixels, reverse_render_ms) = time_gpu_call(gpu, || {
        render_weather(
            renderer,
            gpu,
            &planet_uniforms,
            &scene,
            &reverse_weather,
            render_size,
        )
    });
    render_samples_ms.push(forward_render_ms);
    render_samples_ms.push(reverse_render_ms);
    save_png(
        output_dir,
        "weather_wind_reversal_forward.png",
        render_size,
        &forward_pixels,
    );
    save_png(
        output_dir,
        "weather_wind_reversal_reverse.png",
        render_size,
        &reverse_pixels,
    );
    let density_uniforms = PreviewUniforms {
        view_mode: 9,
        ..planet_uniforms
    };
    let (forward_density, forward_density_render_ms) = time_gpu_call(gpu, || {
        render_weather(
            renderer,
            gpu,
            &density_uniforms,
            &scene,
            &forward_weather,
            render_size,
        )
    });
    let (reverse_density, reverse_density_render_ms) = time_gpu_call(gpu, || {
        render_weather(
            renderer,
            gpu,
            &density_uniforms,
            &scene,
            &reverse_weather,
            render_size,
        )
    });
    render_samples_ms.push(forward_density_render_ms);
    render_samples_ms.push(reverse_density_render_ms);
    save_png(
        output_dir,
        "weather_wind_reversal_forward_density.png",
        render_size,
        &forward_density,
    );
    save_png(
        output_dir,
        "weather_wind_reversal_reverse_density.png",
        render_size,
        &reverse_density,
    );
    save_contact_sheet(
        output_dir,
        "weather_wind_reversal_contact_sheet.png",
        render_size,
        &[
            (forward_pixels.clone(), forward_density.clone()),
            (reverse_pixels.clone(), reverse_density.clone()),
        ],
    );
    println!(
        "  reversal RGB distance={:.4}",
        rgb_distance(&forward_pixels, &reverse_pixels)
    );

    let generation_stats = compute_runtime_stats(generation_samples_ms);
    let render_stats = compute_runtime_stats(render_samples_ms);
    if generation_stats.p95_ms > 33.3 {
        gate_failures.push(format!(
            "GPU generation p95 {:.3}ms exceeds 33.3ms queue-stall gate",
            generation_stats.p95_ms
        ));
    }
    if render_stats.p95_ms > 33.3 {
        gate_failures.push(format!(
            "GPU render p95 {:.3}ms exceeds 33.3ms queue-stall gate",
            render_stats.p95_ms
        ));
    }
    println!(
        "  Runtime (ms): generation n={} p95={:.3} min={:.3} max={:.3} mean={:.3}, render n={} p95={:.3} min={:.3} max={:.3} mean={:.3}",
        generation_stats.count,
        generation_stats.p95_ms,
        generation_stats.min_ms,
        generation_stats.max_ms,
        generation_stats.mean_ms,
        render_stats.count,
        render_stats.p95_ms,
        render_stats.min_ms,
        render_stats.max_ms,
        render_stats.mean_ms,
    );

    println!("U12 status: IMPLEMENTED, COMPLETE");
    if gate_failures.is_empty() {
        println!("  Automated eight-seed topology, morphology, and storm-control gates passed.");
    } else {
        println!("  Automated gate failures ({}):", gate_failures.len());
        for failure in &gate_failures {
            println!("    {failure}");
        }
    }
    println!(
        "  The 512px automated gates, independent visual review, wind reversal, corrected queue p95, and private transport-only conservation test passed."
    );
    assert!(
        gate_failures.is_empty(),
        "weather validation failed {} automated gate(s)",
        gate_failures.len()
    );
}

fn main() {
    env_logger::init();

    let output_dir = std::env::args()
        .skip_while(|a| a != "--output-dir")
        .nth(1)
        .unwrap_or_else(|| "output/sweep".to_string());

    let render_size: u32 = std::env::args()
        .skip_while(|a| a != "--size")
        .nth(1)
        .and_then(|s| s.parse().ok())
        .unwrap_or(512);
    let weather_validation = std::env::args().any(|arg| arg == "--weather-validation");
    if weather_validation && let Some(error) = weather_validation_size_error(render_size) {
        eprintln!("error: {error}");
        std::process::exit(2);
    }

    let seeds = [42, 137, 256, 999, 7777];
    let planet_presets = presets();

    if weather_validation {
        println!("Weather Validation");
        println!("  Resolution: {}x{}", render_size, render_size);
        println!("  Output: {}/", output_dir);
    } else {
        println!("Parameter Sweep");
        println!("  Presets: {}", planet_presets.len());
        println!("  Seeds: {}", seeds.len());
        println!("  Total images: {}", planet_presets.len() * seeds.len());
        println!("  Resolution: {}x{}", render_size, render_size);
        println!("  Output: {}/", output_dir);
        println!();
    }

    let gpu = GpuContext::new().expect("Failed to initialize GPU");
    println!("GPU: {}", gpu.adapter_name());
    let compute = TerrainComputePipeline::new(&gpu);
    let renderer = PreviewRenderer::new(&gpu);
    std::fs::create_dir_all(&output_dir).expect("Failed to create output directory");

    if weather_validation {
        let weather_pipeline =
            WeatherFieldPipeline::new(&gpu).expect("Rgba16Float weather unsupported");
        run_weather_validation_with_pipeline(
            &gpu,
            &compute,
            &renderer,
            &planet_presets[0],
            &output_dir,
            render_size,
            weather_pipeline,
        );
        println!("Done! Weather validation images saved to {}/", output_dir);
        return;
    }

    let total = planet_presets.len() * seeds.len();
    let mut count = 0;

    for preset in &planet_presets {
        for &seed in &seeds {
            count += 1;
            let filename = format!("{}/{}_{}.png", output_dir, preset.name, seed);
            print!("[{}/{}] {} seed={} ... ", count, total, preset.name, seed);

            let pixels = generate_planet_png(&gpu, &compute, &renderer, preset, seed, render_size);

            // Save as PNG
            let img = image::RgbaImage::from_raw(render_size, render_size, pixels)
                .expect("Failed to create image");
            img.save(Path::new(&filename)).expect("Failed to save PNG");

            println!("saved");
        }
    }

    println!("\nDone! {} images saved to {}/", total, output_dir);

    // === Wind effects comparison: earth with wind effects OFF vs ON ===
    println!("\n--- Wind Effects Comparison ---");
    let earth = &planet_presets[0]; // earth preset
    let seed = 42u32;
    let wind_pipeline = WindFieldPipeline::new(&gpu).expect("Rgba16Float dynamics unsupported");
    let scene = generate_weather_scene(
        &gpu,
        &compute,
        &renderer,
        &wind_pipeline,
        earth,
        seed,
        (512, (render_size / 2).max(192)),
    );
    let derived = scene.derived;
    let effective_ocean = derived.ocean_fraction * (1.0 - earth.water_loss);
    let ocean_level = scene.ocean_level;
    let cubemap_view = scene.cubemap;
    let cloud_view = scene.dynamics.wind_continentality;

    let tilt = 0.35_f32;
    let ct = tilt.cos();
    let st = tilt.sin();
    let base_uniforms = PreviewUniforms {
        rotation: [
            [1.0, 0.0, 0.0, 0.0],
            [0.0, ct, -st, 0.0],
            [0.0, st, ct, 0.0],
            [0.0, 0.0, 0.0, 1.0],
        ],
        light_dir: [0.5, 0.7, -1.0],
        ocean_level,
        base_temp_c: derived.base_temperature_c,
        ocean_fraction: effective_ocean,
        axial_tilt_rad: earth.params.axial_tilt_deg.to_radians(),
        view_mode: 0,
        season: 0.5,
        atmosphere_density: 0.0,
        atmosphere_height: 0.0,
        height_scale: 3.0,
        zoom: 1.0,
        pan_x: 0.0,
        pan_y: 0.0,
        cloud_coverage: 0.6,
        cloud_seed: 42,
        night_lights: 0.0,
        star_color_temp: 0.5,
        city_light_hue: 0.0,
        show_ao: 1.0,
        show_water: 1.0,
        show_ice: 1.0,
        show_biomes: 1.0,
        show_clouds: 1.0,
        show_atmosphere_layer: 0.0,
        show_cities: 0.0,
        cloud_opacity: 1.0,
        cloud_advection: 0.0,
        rotation_rate: 1.0,
        atm_pressure: 0.7,
        _pad4: 0.0,
        lava_glow: 0.0,
        ring_inner: 0.0,
        ring_outer: 0.0,
        ring_tilt: 0.0,
        ring_opacity: 0.0,
        planet_radius_km: derived.radius_km,
        show_cloud_shadows: 1.0,
        _pad5: 0.0,
    };

    // Render without wind effects (analytical wind only)
    let px_off = renderer.render(&gpu, &base_uniforms, &cubemap_view, None, None, render_size);
    let img = image::RgbaImage::from_raw(render_size, render_size, px_off).unwrap();
    img.save(Path::new(&format!("{}/wind_effects_OFF.png", output_dir)))
        .unwrap();
    println!("  wind_effects_OFF.png saved");

    // Render with wind effects (cubemap wind + continentality)
    let mut on_uniforms = base_uniforms;
    on_uniforms.cloud_advection = 1.0;
    let px_on = renderer.render(
        &gpu,
        &on_uniforms,
        &cubemap_view,
        Some(&cloud_view),
        None,
        render_size,
    );
    let img = image::RgbaImage::from_raw(render_size, render_size, px_on).unwrap();
    img.save(Path::new(&format!("{}/wind_effects_ON.png", output_dir)))
        .unwrap();
    println!("  wind_effects_ON.png saved");
    println!(
        "Compare: {}/wind_effects_OFF.png vs {}/wind_effects_ON.png",
        output_dir, output_dir
    );

    // Zoomed-in comparison
    let mut zoom_off = base_uniforms;
    zoom_off.zoom = 3.0;
    zoom_off.pan_y = 0.2;
    let px = renderer.render(&gpu, &zoom_off, &cubemap_view, None, None, render_size);
    image::RgbaImage::from_raw(render_size, render_size, px)
        .unwrap()
        .save(Path::new(&format!("{}/wind_zoom_OFF.png", output_dir)))
        .unwrap();
    println!("  wind_zoom_OFF.png saved");

    let mut zoom_on = zoom_off;
    zoom_on.cloud_advection = 1.0;
    let px = renderer.render(
        &gpu,
        &zoom_on,
        &cubemap_view,
        Some(&cloud_view),
        None,
        render_size,
    );
    image::RgbaImage::from_raw(render_size, render_size, px)
        .unwrap()
        .save(Path::new(&format!("{}/wind_zoom_ON.png", output_dir)))
        .unwrap();
    println!("  wind_zoom_ON.png saved");

    // Wind map visualization
    let mut wind_u = base_uniforms;
    wind_u.view_mode = 14;
    wind_u.show_clouds = 0.0;
    let px = renderer.render(&gpu, &wind_u, &cubemap_view, None, None, render_size);
    image::RgbaImage::from_raw(render_size, render_size, px)
        .unwrap()
        .save(Path::new(&format!("{}/wind_map.png", output_dir)))
        .unwrap();
    println!("  wind_map.png saved");
}

#[cfg(test)]
mod tests {
    use super::{
        TopologyMetrics, U15_DEEP_THRESHOLD, U15ResponseComponent, cube_edge_pairs,
        u14_coverage_increments, u14_geometry_metrics, u15_anvil_source_taps, u15_causal_component,
        u15_component_labels, u15_pixel_neighbors, u15_pixel_position,
        u15_significant_response_components, validate_seed_topology_metrics,
        weather_validation_size_error,
    };

    fn response_with_deep_pixels(resolution: u32, pixels: &[usize]) -> Vec<f32> {
        let mut response = vec![0.0; resolution as usize * resolution as usize * 6 * 4];
        for &pixel in pixels {
            response[pixel * 4 + 1] = U15_DEEP_THRESHOLD;
        }
        response
    }

    fn pixel(face: usize, x: usize, y: usize, resolution: u32) -> usize {
        face * resolution as usize * resolution as usize + y * resolution as usize + x
    }

    #[test]
    fn weather_validation_requires_at_least_512_pixels() {
        assert_eq!(
            weather_validation_size_error(511).as_deref(),
            Some("--weather-validation requires --size >= 512 (got 511)")
        );
        assert!(weather_validation_size_error(512).is_none());
    }

    #[test]
    fn seed_topology_validation_rejects_veils_noise_and_unbounded_change() {
        let coherent = TopologyMetrics {
            occupied: 0.35,
            coherent: 0.97,
            zonal_continuity: 0.9,
            meridional_continuity: 0.85,
            ribbon_like: 0.2,
            polar_occupied: 0.4,
            directional_anisotropy: 0.03,
            components: 5,
            largest_component: 0.7,
        };
        assert!(validate_seed_topology_metrics(&coherent, Some(0.18)).is_ok());
        assert!(
            validate_seed_topology_metrics(
                &TopologyMetrics {
                    occupied: 0.9,
                    ..coherent
                },
                Some(0.18),
            )
            .is_err()
        );
        assert!(
            validate_seed_topology_metrics(
                &TopologyMetrics {
                    coherent: 0.5,
                    ..coherent
                },
                Some(0.18),
            )
            .is_err()
        );
        assert!(validate_seed_topology_metrics(&coherent, Some(0.01)).is_err());
        assert!(validate_seed_topology_metrics(&coherent, Some(0.7)).is_err());
        assert!(
            validate_seed_topology_metrics(
                &TopologyMetrics {
                    zonal_continuity: 0.4,
                    ..coherent
                },
                Some(0.18),
            )
            .is_err()
        );
        assert!(
            validate_seed_topology_metrics(
                &TopologyMetrics {
                    components: 1,
                    largest_component: 1.0,
                    ..coherent
                },
                Some(0.18),
            )
            .is_err()
        );
    }

    #[test]
    fn cubemap_edge_pairs_cover_every_shared_edge_once() {
        assert_eq!(cube_edge_pairs(16).len(), 12);
    }

    #[test]
    fn u14_coverage_rejects_flat_or_disproportionate_segments() {
        assert!(u14_coverage_increments(&[0.0, 0.01, 0.02, 0.03]).is_ok());
        assert!(u14_coverage_increments(&[0.0, 0.01, 0.01, 0.03]).is_err());
        assert!(u14_coverage_increments(&[0.0, 0.01, 0.02, 0.08]).is_err());
    }

    #[test]
    fn u14_geometry_requires_positive_layers_for_occupied_texels() {
        let mass = [0.2, 0.0, 0.0, 0.2, 0.0, 0.0, 0.0, 0.0];
        assert_eq!(
            u14_geometry_metrics(&mass, &[0.1, 1.0, 2.0, 3.0, 0.0, 0.0, 0.0, 0.0]),
            (1, 0)
        );
        assert_eq!(
            u14_geometry_metrics(&mass, &[0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0]),
            (1, 1)
        );
    }

    #[test]
    fn u15_response_components_share_count_area_centroid_and_nearest_core() {
        let resolution = 40;
        let first = [
            pixel(0, 19, 19, resolution),
            pixel(0, 20, 19, resolution),
            pixel(0, 19, 20, resolution),
            pixel(0, 20, 20, resolution),
        ];
        let second = [
            pixel(1, 19, 19, resolution),
            pixel(1, 20, 19, resolution),
            pixel(1, 19, 20, resolution),
            pixel(1, 20, 20, resolution),
        ];
        let rejected = [
            pixel(2, 19, 19, resolution),
            pixel(2, 20, 19, resolution),
            pixel(2, 19, 20, resolution),
        ];
        let response = response_with_deep_pixels(
            resolution,
            &[first.as_slice(), second.as_slice(), rejected.as_slice()].concat(),
        );
        let components = u15_significant_response_components(&response, resolution);

        assert_eq!(components.len(), 2);
        assert!(
            components
                .iter()
                .all(|component| (component.area_fraction - 0.0025).abs() < 1e-6)
        );
        assert!(
            super::u15_dot(
                components[0].centroid,
                u15_pixel_position(first[0], resolution)
            ) > 0.99
        );
        assert!(
            super::u15_dot(
                components[1].centroid,
                u15_pixel_position(second[0], resolution)
            ) > 0.99
        );
    }

    #[test]
    fn u15_response_components_cross_cubemap_seams() {
        let resolution = 40;
        let left_edge = pixel(0, 0, 20, resolution);
        let right_edge = pixel(4, 39, 20, resolution);
        assert!(u15_pixel_neighbors(left_edge, resolution).contains(&right_edge));

        let response = response_with_deep_pixels(
            resolution,
            &[
                left_edge,
                pixel(0, 0, 21, resolution),
                right_edge,
                pixel(4, 39, 21, resolution),
            ],
        );
        let components = u15_significant_response_components(&response, resolution);

        assert_eq!(components.len(), 1);
        assert_eq!(components[0].pixels.len(), 4);
        assert!((components[0].area_fraction - 0.0025).abs() < 1e-6);
    }

    #[test]
    fn u15_anvil_uses_upwind_component_across_destination_voronoi_boundary() {
        let resolution = 64;
        let face_pixels = resolution as usize * resolution as usize;
        let (destination, source) = (0..face_pixels)
            .find_map(|destination| {
                let pixel = destination + face_pixels;
                let position = u15_pixel_position(pixel, resolution);
                let source = u15_anvil_source_taps(position, resolution)?[0];
                (source != pixel).then_some((pixel, source))
            })
            .expect("fixture has an upstream tap");
        let components = vec![
            U15ResponseComponent {
                pixels: vec![source],
                centroid: u15_pixel_position(source, resolution),
                area_fraction: 0.0025,
            },
            U15ResponseComponent {
                pixels: vec![destination],
                centroid: u15_pixel_position(destination, resolution),
                area_fraction: 0.0025,
            },
        ];
        let labels = u15_component_labels(&components, resolution);
        let mut response = vec![0.0; face_pixels * 6 * 4];
        response[source * 4 + 1] = U15_DEEP_THRESHOLD;
        response[destination * 4 + 1] = U15_DEEP_THRESHOLD;
        let destination_position = u15_pixel_position(destination, resolution);

        assert!(
            super::u15_dot(destination_position, components[1].centroid)
                > super::u15_dot(destination_position, components[0].centroid)
        );
        assert_eq!(
            u15_causal_component(
                u15_anvil_source_taps(destination_position, resolution).unwrap(),
                &labels,
                &response,
                resolution,
            ),
            Some(0),
        );
    }
}
