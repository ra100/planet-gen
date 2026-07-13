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
        wind_strength: 0.5,
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
            _pad0: 0.0,
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
    max_delta: f32,
    mean_delta: f32,
    sample_count: usize,
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

fn density_topology(pixels: &[u8], size: u32) -> TopologyMetrics {
    let size = size as usize;
    let cloudy = |x: usize, y: usize| pixels[(y * size + x) * 4] > 13;
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
    if metrics.sample_count == 0 {
        return Err("seam metrics had no samples");
    }
    if metrics.max_delta > 0.30 {
        return Err("cubemap seam has a large discontinuity");
    }
    if metrics.mean_delta > 0.12 {
        return Err("cubemap seam has elevated average discontinuity");
    }
    Ok(())
}

fn seam_continuity_metrics(mass: &[f32], resolution: u32, channel: usize) -> SeamMetrics {
    let last = resolution as usize - 1;
    let pixel =
        |face: usize, x: usize, y: usize| sample_mass_pixel(mass, resolution, face, x, y, channel);
    let mut max_delta = 0.0_f32;
    let mut sum = 0.0_f32;
    let mut sample_count = 0;
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
            max_delta = max_delta.max(delta);
            sum += delta;
            sample_count += 1;
        }
    }
    SeamMetrics {
        max_delta,
        mean_delta: if sample_count > 0 {
            sum / sample_count as f32
        } else {
            0.0
        },
        sample_count,
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

fn validate_storm_control_metrics(
    distance_04: f32,
    distance_08: f32,
    changed_08: f32,
    saturated_clouds: f32,
) -> Result<(), &'static str> {
    let ratio = distance_04 / distance_08.max(f32::EPSILON);
    if distance_04 < 0.005 || distance_08 < 0.008 || changed_08 < 0.02 {
        return Err("storm control is effectively invisible");
    }
    if !(0.15..=0.90).contains(&ratio) {
        return Err("storm control response is not progressive");
    }
    if saturated_clouds > 0.02 {
        return Err("storm control saturates too many cloudy pixels");
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

fn run_weather_validation(
    gpu: &GpuContext,
    compute: &TerrainComputePipeline,
    renderer: &PreviewRenderer,
    earth: &PlanetPreset,
    output_dir: &str,
    render_size: u32,
) {
    const VALIDATION_SEEDS: [(&str, u32); 8] = [
        ("42", 42),
        ("137", 137),
        ("999", 999),
        ("7777", 7777),
        ("2p24_minus_1", (1 << 24) - 1),
        ("2p24_plus_1", (1 << 24) + 1),
        ("714003000", 714_003_000),
        ("u32_max", u32::MAX),
    ];

    let terrain_resolution = render_size.clamp(128, 512);
    let weather_resolution = (render_size / 2).clamp(64, 384);
    let wind_pipeline = WindFieldPipeline::new(gpu).expect("Rgba16Float dynamics unsupported");
    let weather_pipeline = WeatherFieldPipeline::new(gpu).expect("Rgba16Float weather unsupported");
    let mut gate_failures = Vec::new();
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
    base_uniforms.wind_strength = 0.5;
    base_uniforms.planet_radius_km = scene.derived.radius_km;
    base_uniforms.show_cloud_shadows = 1.0;

    let mut generation_samples_ms = Vec::new();
    let mut render_samples_ms = Vec::new();
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
        let uniforms = PreviewUniforms {
            cloud_seed,
            ..base_uniforms
        };
        let (pixels, render_ms) = time_gpu_call(gpu, || {
            render_weather(renderer, gpu, &uniforms, &scene, weather, render_size)
        });
        render_samples_ms.push(render_ms);
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
        save_png(
            output_dir,
            &format!("weather_global_seed_{label}_density.png"),
            render_size,
            &pixels,
        );
        global_density_sheet.push((pixels.clone(), pixels.clone()));
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
    println!(
        "  count RGB distance 0->4={distance_04:.4}, 0->8={distance_08:.4}, ratio={:.3}; cloudy pixels changed >0.05={:.1}%; saturated at 8={:.1}%",
        distance_04 / distance_08.max(f32::EPSILON),
        changed_08 * 100.0,
        saturated_clouds * 100.0,
    );
    if let Err(error) =
        validate_storm_control_metrics(distance_04, distance_08, changed_08, saturated_clouds)
    {
        gate_failures.push(format!("Storm Count: {error}"));
    }
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
    if let Err(error) = validate_storm_control_metrics(
        distance_small_medium,
        distance_small_large,
        changed_large,
        saturated_large,
    ) {
        gate_failures.push(format!("Storm Size: {error}"));
    }
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
        run_weather_validation(
            &gpu,
            &compute,
            &renderer,
            &planet_presets[0],
            &output_dir,
            render_size,
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
        wind_strength: 0.5,
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
        TopologyMetrics, validate_seed_topology_metrics, validate_storm_control_metrics,
        weather_validation_size_error,
    };

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
    fn storm_control_validation_rejects_invisible_and_saturated_renders() {
        assert!(validate_storm_control_metrics(0.0123, 0.0202, 0.077, 0.0).is_ok());
        assert!(validate_storm_control_metrics(0.005, 0.008, 0.01, 0.0).is_err());
        assert!(validate_storm_control_metrics(0.001, 0.02, 0.08, 0.0).is_err());
        assert!(validate_storm_control_metrics(0.0123, 0.0202, 0.077, 0.03).is_err());
    }
}
