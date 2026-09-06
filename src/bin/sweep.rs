//! Parameter sweep: generates a grid of planet previews as PNG files.
//! Usage: cargo run --features validation --bin sweep [--output-dir <dir>] [--size <pixels>]

use bytemuck::Zeroable;
use planet_gen::gpu::GpuContext;
use planet_gen::planet::{DerivedProperties, PlanetParams};
use planet_gen::plates::{PlateGenParams, generate_plates};
use planet_gen::preview::{PreviewRenderer, PreviewUniforms};
use planet_gen::terrain_compute::{
    DynamicsTextures, TectonicTerrain, TerrainComputePipeline, WindField, WindFieldPipeline,
};
use planet_gen::weather::{
    WEATHER_DIAGNOSTIC_NO_SOURCE, WeatherFieldPipeline, WeatherSnapshot, WeatherTextures,
};
use std::path::Path;
use std::sync::mpsc;
use std::time::{Duration, Instant};

const WEATHER_GENERATION_QUEUE_STALL_P95_MS: f64 = 36.0;
const U15_VALIDATION_SEEDS: [u32; 8] = [7, 19, 37, 73, 101, 211, 509, 997];

#[derive(Debug, Copy, Clone, PartialEq, Eq)]
enum U15ValidationMode {
    Legacy,
    Batch,
    Seed(u32),
}

fn u15_validation_mode(args: &[String]) -> Result<Option<U15ValidationMode>, String> {
    let legacy = args.iter().any(|arg| arg == "--u15-validation");
    let batch = args.iter().any(|arg| arg == "--u15-validation-batch");
    let seed_flag_count = args
        .iter()
        .filter(|arg| arg.as_str() == "--u15-validation-seed")
        .count();
    if seed_flag_count > 1 {
        return Err("--u15-validation-seed may be supplied only once".to_string());
    }
    let seed = args
        .iter()
        .position(|arg| arg == "--u15-validation-seed")
        .map(|index| {
            args.get(index + 1)
                .ok_or_else(|| "--u15-validation-seed requires a frozen seed".to_string())?
                .parse::<u32>()
                .map_err(|_| "--u15-validation-seed requires an unsigned frozen seed".to_string())
        })
        .transpose()?;
    if usize::from(legacy) + usize::from(batch) + usize::from(seed.is_some()) > 1 {
        return Err(
            "choose one of --u15-validation, --u15-validation-batch, or --u15-validation-seed"
                .to_string(),
        );
    }
    if let Some(seed) = seed {
        if !U15_VALIDATION_SEEDS.contains(&seed) {
            return Err(format!(
                "--u15-validation-seed must be one of {U15_VALIDATION_SEEDS:?} (got {seed})"
            ));
        }
        return Ok(Some(U15ValidationMode::Seed(seed)));
    }
    Ok(legacy
        .then_some(U15ValidationMode::Legacy)
        .or(batch.then_some(U15ValidationMode::Batch)))
}

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
    let dynamics = wind_pipeline.create_textures(gpu, weather_resolution, &terrain, ocean_level);
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
        1.0,
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

fn generate_native_default_scene(
    gpu: &GpuContext,
    compute: &TerrainComputePipeline,
    renderer: &PreviewRenderer,
    wind_pipeline: &WindFieldPipeline,
    render_size: u32,
) -> WeatherScene {
    let params = PlanetParams::default();
    let derived = DerivedProperties::from_params(&params);
    let effective_ocean = derived.ocean_fraction * 0.5;
    let ocean_level = -0.5 + 1.7 * effective_ocean;
    let plates = generate_plates(&PlateGenParams {
        seed: params.seed,
        mass_earth: params.mass_earth,
        ocean_fraction: effective_ocean,
        tectonics_factor: derived.tectonics_factor,
        continental_scale: 1.0,
        num_plates_override: 0,
        num_continents: 4,
        continent_size_variety: 0.35,
    });
    let dist_factor = (params.star_distance_au.ln() / 3.0_f32.ln()).clamp(0.0, 1.0);
    let beta = (1.47 + 0.91 * dist_factor + 0.3 * params.metallicity).clamp(1.2, 3.0);
    let gain = 2.0_f32.powf(-(beta - 1.0) / 2.0);
    let terrain = compute.generate(
        gpu,
        &plates,
        render_size,
        params.seed,
        0.6 + 0.6 * params.mass_earth.powf(0.3).min(2.0),
        1.0 + 0.5 * params.mass_earth.powf(0.2),
        (8.0 + 4.0 * (params.axial_tilt_deg / 90.0) * derived.tectonics_factor) as u32,
        gain,
        1.9 + 0.2 * (24.0 / params.rotation_period_h).clamp(0.5, 2.0),
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
    let dynamics =
        wind_pipeline.create_textures(gpu, (render_size / 2).max(192), &terrain, ocean_level);
    wind_pipeline.generate_gpu(
        gpu,
        &terrain,
        &dynamics,
        params.seed.wrapping_add(1000),
        ocean_level,
        params.axial_tilt_deg.to_radians(),
        0.5,
        24.0 / params.rotation_period_h,
        derived.base_temperature_c,
        derived.surface_pressure_bar,
        1.0,
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

fn field_view_pixels(mass: &[f32], geometry: &[f32], resolution: u32) -> [Vec<u8>; 3] {
    let width = resolution as usize * 3;
    let height = resolution as usize * 2;
    let mut formation = vec![0; width * height * 4];
    let mut mass_view = vec![0; width * height * 4];
    let mut density = vec![0; width * height * 4];
    let byte = |value: f32| (value.clamp(0.0, 1.0) * 255.0).round() as u8;

    for face in 0..6 {
        for y in 0..resolution as usize {
            for x in 0..resolution as usize {
                let source = ((face * resolution as usize * resolution as usize
                    + y * resolution as usize
                    + x)
                    * 4) as usize;
                let target = (((face / 3 * resolution as usize + y) * width
                    + face % 3 * resolution as usize
                    + x)
                    * 4) as usize;
                let layer = [
                    geometry[source + 1] - geometry[source],
                    geometry[source + 2] - geometry[source + 1],
                    geometry[source + 3] - geometry[source + 2],
                ];
                formation[target..target + 4].copy_from_slice(&[
                    byte(layer[0] * 32.0),
                    byte(layer[1] * 32.0),
                    byte(layer[2] * 32.0),
                    255,
                ]);
                mass_view[target..target + 4].copy_from_slice(&[
                    byte(mass[source]),
                    byte(mass[source + 1]),
                    byte(mass[source + 2]),
                    255,
                ]);
                let tau = u14_column_tau(&mass[source..source + 4]);
                density[target..target + 4].copy_from_slice(&[
                    byte(tau),
                    byte(tau),
                    byte(tau),
                    255,
                ]);
            }
        }
    }

    [formation, mass_view, density]
}

fn save_field_views(
    output_dir: &str,
    label: &str,
    resolution: u32,
    mass: &[f32],
    geometry: &[f32],
) {
    let [formation, mass_view, density] = field_view_pixels(mass, geometry, resolution);
    let width = resolution * 3;
    let height = resolution * 2;
    for (view, pixels) in [
        ("formation", formation),
        ("mass", mass_view),
        ("density", density),
    ] {
        let name = format!("{label}_{view}.png");
        image::save_buffer(
            Path::new(output_dir).join(&name),
            &pixels,
            width,
            height,
            image::ColorType::Rgba8,
        )
        .expect("save weather field view");
        println!("  {name}");
    }
}

fn save_u14_total_density_view(
    output_dir: &str,
    label: &str,
    resolution: u32,
    mass: &[f32],
    max_density: f32,
) {
    let width = resolution as usize * 3;
    let height = resolution as usize * 2;
    let mut pixels = vec![0; width * height * 4];
    for face in 0..6 {
        for y in 0..resolution as usize {
            for x in 0..resolution as usize {
                let source = (face * resolution as usize * resolution as usize
                    + y * resolution as usize
                    + x)
                    * 4;
                let target = ((face / 3 * resolution as usize + y) * width
                    + face % 3 * resolution as usize
                    + x)
                    * 4;
                let value = ((mass[source] + mass[source + 1] + mass[source + 2]) / max_density)
                    .clamp(0.0, 1.0);
                let byte = (value * 255.0).round() as u8;
                pixels[target..target + 4].copy_from_slice(&[byte, byte, byte, 255]);
            }
        }
    }
    let name = format!("{label}_density.png");
    image::save_buffer(
        Path::new(output_dir).join(&name),
        &pixels,
        width as u32,
        height as u32,
        image::ColorType::Rgba8,
    )
    .expect("save U14 validation density view");
    println!("  {name}");
}

// ponytail: env-gated U15 triage diagnostics (PLANET_GEN_U15_DUMP_TRAIL=1).
// Dumps the shear-plume counterfactual response as a scalar cube view so the
// plume morphology can be inspected without touching gate behavior.
fn save_u15_trail_scalar_view(output_dir: &str, label: &str, resolution: u32, values: &[f32]) {
    let width = resolution as usize * 3;
    let height = resolution as usize * 2;
    let max_value = values
        .iter()
        .fold(0.0f32, |maximum, &value| maximum.max(value));
    let mut pixels = vec![0u8; width * height * 4];
    for face in 0..6 {
        for y in 0..resolution as usize {
            for x in 0..resolution as usize {
                let source =
                    face * resolution as usize * resolution as usize + y * resolution as usize + x;
                let target = (face / 3 * resolution as usize + y) * width
                    + face % 3 * resolution as usize
                    + x;
                let value = (values[source] / max_value.max(f32::EPSILON)).clamp(0.0, 1.0);
                let byte = (value * 255.0).round() as u8;
                pixels[target * 4..target * 4 + 4].copy_from_slice(&[byte, byte, byte, 255]);
            }
        }
    }
    let name = format!("{label}.png");
    image::save_buffer(
        Path::new(output_dir).join(&name),
        &pixels,
        width as u32,
        height as u32,
        image::ColorType::Rgba8,
    )
    .expect("save U15 trail scalar view");
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

fn render_weather_with_dynamics(
    renderer: &PreviewRenderer,
    gpu: &GpuContext,
    uniforms: &PreviewUniforms,
    scene: &WeatherScene,
    dynamics: &DynamicsTextures,
    weather: &WeatherTextures,
    size: u32,
) -> Vec<u8> {
    renderer.render(
        gpu,
        uniforms,
        &scene.cubemap,
        Some(&dynamics.wind_continentality),
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

fn native_default_land_masks(
    scene: &WeatherScene,
    uniforms: &PreviewUniforms,
    size: u32,
) -> (Vec<bool>, Vec<bool>) {
    let size_usize = size as usize;
    let mut land = vec![false; size_usize * size_usize];
    let mut lowland = vec![false; size_usize * size_usize];
    for y in 0..size_usize {
        for x in 0..size_usize {
            if !in_sphere_mask(x, y, size_usize) {
                continue;
            }
            let direction = u3_screen_direction(x, y, size_usize, uniforms.rotation);
            let (face, sample_x, sample_y) =
                u3_cube_coordinates(direction, scene.terrain.resolution);
            let resolution = scene.terrain.resolution as usize;
            let height =
                scene.terrain.faces[face][sample_y.round().clamp(0.0, (resolution - 1) as f32)
                    as usize
                    * resolution
                    + sample_x.round().clamp(0.0, (resolution - 1) as f32) as usize];
            let land_height = height - uniforms.ocean_level;
            let index = y * size_usize + x;
            land[index] = land_height > 0.0;
            lowland[index] = land_height > 0.01 && land_height <= 0.36;
        }
    }
    (land, lowland)
}

fn native_default_mask_png(mask: &[bool]) -> Vec<u8> {
    mask.iter()
        .flat_map(|&land| {
            let value = u8::from(land) * 255;
            [value, value, value, 255]
        })
        .collect()
}

fn native_default_delta_png(on: &[u8], off: &[u8], land: &[bool]) -> Vec<u8> {
    on.chunks_exact(4)
        .zip(off.chunks_exact(4))
        .zip(land)
        .flat_map(|((on, off), &land)| {
            let delta = if land {
                ((0..3)
                    .map(|channel| (on[channel] as f32 - off[channel] as f32).powi(2))
                    .sum::<f32>()
                    .sqrt()
                    / 3.0_f32.sqrt()
                    * 255.0)
                    .round()
                    .clamp(0.0, 255.0) as u8
            } else {
                0
            };
            [delta, delta, delta, 255]
        })
        .collect()
}

fn native_default_quantile(values: &mut [f32], quantile: f32) -> f32 {
    if values.is_empty() {
        return 0.0;
    }
    values.sort_by(f32::total_cmp);
    values[((values.len() - 1) as f32 * quantile).round() as usize]
}

fn native_default_metrics(
    density: &[u8],
    on: &[u8],
    off: &[u8],
    lowland: &[bool],
    land: &[bool],
    size: u32,
) -> String {
    let size = size as usize;
    let mut opacity = Vec::new();
    let mut land_delta = Vec::new();
    let mut ocean_delta = Vec::new();
    let mut land_occupied = 0usize;
    let mut ocean_occupied = 0usize;
    let mut land_count = 0usize;
    let mut ocean_count = 0usize;
    for index in 0..land.len() {
        let delta = (0..3)
            .map(|channel| {
                (on[index * 4 + channel] as f32 - off[index * 4 + channel] as f32).powi(2)
            })
            .sum::<f32>()
            .sqrt()
            / (255.0 * 3.0_f32.sqrt());
        if lowland[index] {
            let alpha = srgb_to_linear(density[index * 4]);
            opacity.push(alpha);
            land_occupied += usize::from(alpha > 0.05);
            land_count += 1;
        }
        if land[index] {
            land_delta.push(delta);
        } else if in_sphere_mask(index % size, index / size, size) {
            ocean_delta.push(delta);
            ocean_occupied += usize::from(delta > 0.05);
            ocean_count += 1;
        }
    }
    let mut visited = vec![false; size * size];
    let mut components = 0usize;
    let mut largest = 0usize;
    for y in 0..size {
        for x in 0..size {
            let start = y * size + x;
            if visited[start] || !lowland[start] || srgb_to_linear(density[start * 4]) <= 0.05 {
                continue;
            }
            components += 1;
            visited[start] = true;
            let mut stack = vec![(x, y)];
            let mut area = 0usize;
            while let Some((x, y)) = stack.pop() {
                area += 1;
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
                    if !visited[next] && lowland[next] && srgb_to_linear(density[next * 4]) > 0.05 {
                        visited[next] = true;
                        stack.push((next_x, next_y));
                    }
                }
            }
            largest = largest.max(area);
        }
    }
    let land_delta_mean = land_delta.iter().sum::<f32>() / land_delta.len().max(1) as f32;
    let ocean_delta_mean = ocean_delta.iter().sum::<f32>() / ocean_delta.len().max(1) as f32;
    format!(
        "lowland_pixels={land_count}\nintegrated_opacity_p75={:.5}\noccupancy_gt_0_05={:.3}\ncomponents={components}\nlargest_component_share={:.3}\nland_rgb_delta_p75={:.5}\nland_rgb_delta_mean={land_delta_mean:.5}\nocean_rgb_delta_mean={ocean_delta_mean:.5}\nocean_rgb_delta_occupancy_gt_0_05={:.3}\n",
        native_default_quantile(&mut opacity, 0.75),
        land_occupied as f32 / land_count.max(1) as f32,
        largest as f32 / land_occupied.max(1) as f32,
        native_default_quantile(&mut land_delta, 0.75),
        ocean_occupied as f32 / ocean_count.max(1) as f32,
    )
}

fn native_default_source_report(
    gpu: &GpuContext,
    wind_pipeline: &WindFieldPipeline,
    scene: &WeatherScene,
    uniforms: &PreviewUniforms,
    size: u32,
) -> String {
    let params = PlanetParams::default();
    let wind = wind_pipeline.generate(
        gpu,
        &scene.terrain,
        scene.dynamics.resolution,
        params.seed,
        scene.ocean_level,
        scene.tilt_rad,
        0.5,
        24.0 / params.rotation_period_h,
        scene.derived.base_temperature_c,
        scene.derived.atmosphere_strength,
        1.0,
    );
    let wind_data = native_default_wind_data(&wind);
    let height_data = native_default_height_data(&scene.terrain);
    let size = size as usize;
    let mut visible = 0usize;
    let mut land = vec![false; size * size];
    let mut stages = vec![[0.0; 5]; size * size];
    let mut elevation_km = vec![0.0; size * size];
    let mut rain_shadow = vec![0.0; size * size];
    let mut temperature_c = vec![0.0; size * size];
    let mut broad_noise = vec![0.0; size * size];
    let mut lowland_factor = vec![0.0; size * size];
    let broad_offset = u3_noise_seed_offset(1042, 403);

    for y in 0..size {
        for x in 0..size {
            if !in_sphere_mask(x, y, size) {
                continue;
            }
            visible += 1;
            let pos = u3_screen_direction(x, y, size, uniforms.rotation);
            let height = u3_linear_cube_sample(&height_data, scene.terrain.resolution, pos, 0);
            let land_height = height - scene.ocean_level;
            let index = y * size + x;
            if land_height <= 0.0 {
                continue;
            }
            land[index] = true;
            let wind_vector = [
                u3_linear_cube_sample(&wind_data, wind.resolution, pos, 0),
                u3_linear_cube_sample(&wind_data, wind.resolution, pos, 1),
                u3_linear_cube_sample(&wind_data, wind.resolution, pos, 2),
            ];
            let continentality = u3_linear_cube_sample(&wind_data, wind.resolution, pos, 3);
            let tangent_wind = [
                wind_vector[0] - pos[0] * native_default_dot(wind_vector, pos),
                wind_vector[1] - pos[1] * native_default_dot(wind_vector, pos),
                wind_vector[2] - pos[2] * native_default_dot(wind_vector, pos),
            ];
            let normalized_speed = native_default_dot(tangent_wind, tangent_wind).sqrt();
            let wind_scale: f32 = 1.0;
            // DS-046 #3 mirror of weather_spinup.wgsl forcing_scale.
            let forcing_scale = wind_scale.max(0.0).powf(0.85);
            let effective_speed = normalized_speed * forcing_scale;
            let effective_speed = if effective_speed >= 0.01 {
                effective_speed
            } else {
                0.0
            };
            let wind_dir = [
                tangent_wind[0] / normalized_speed.max(0.0001),
                tangent_wind[1] / normalized_speed.max(0.0001),
                tangent_wind[2] / normalized_speed.max(0.0001),
            ];
            let terrain_lookahead =
                1.5 * (300.0 / scene.derived.radius_km.max(1.0)).clamp(0.02, 0.08);
            let upwind = native_default_normalize([
                pos[0] - wind_dir[0] * terrain_lookahead,
                pos[1] - wind_dir[1] * terrain_lookahead,
                pos[2] - wind_dir[2] * terrain_lookahead,
            ]);
            let downwind = native_default_normalize([
                pos[0] + wind_dir[0] * terrain_lookahead,
                pos[1] + wind_dir[1] * terrain_lookahead,
                pos[2] + wind_dir[2] * terrain_lookahead,
            ]);
            let terrain_response =
                (u3_linear_cube_sample(&height_data, scene.terrain.resolution, downwind, 0)
                    - u3_linear_cube_sample(&height_data, scene.terrain.resolution, upwind, 0))
                    * native_default_smooth_step(0.03, 0.2, effective_speed);
            let local_rain_shadow = native_default_smooth_step(0.005, 0.08, -terrain_response);
            let tilted_y = pos[1] * scene.tilt_rad.cos() + pos[2] * scene.tilt_rad.sin();
            let latitude = tilted_y.clamp(-1.0, 1.0).asin().abs() / std::f32::consts::FRAC_PI_2;
            let season = 0.5;
            let season_shift = (season - 0.5) * 2.0 * scene.tilt_rad.sin();
            let local_elevation_km = land_height.max(0.0) * 5.0;
            let local_temperature_c = scene.derived.base_temperature_c - latitude * 35.0
                + season_shift * tilted_y * 16.0
                - local_elevation_km * 6.5
                + continentality * season_shift * 5.0;
            let thermal = native_default_smooth_step(-25.0, 30.0, local_temperature_c);
            let seeded_broad = u3_snoise([
                pos[0] * 1.9 + broad_offset[0],
                pos[1] * 1.9 + broad_offset[1],
                pos[2] * 1.9 + broad_offset[2],
            ]) * 0.5
                + 0.5;
            let broad = if effective_speed > 0.0 {
                seeded_broad
            } else {
                0.5
            };
            let exposed_land = native_default_smooth_step(0.0, 0.01, land_height);
            let production_continentality = native_default_smooth_step(0.15, 0.85, continentality);
            let lowland = 1.0 - native_default_smooth_step(0.35, 1.8, local_elevation_km);
            let climate = native_default_smooth_step(
                0.46,
                0.70,
                broad * 0.45 + thermal * 0.28 + lowland * 0.17 + production_continentality * 0.15
                    - local_rain_shadow * 0.35,
            );
            let stratiform_wind = 1.0 - native_default_smooth_step(0.35, 0.70, effective_speed);
            stages[index][0] = exposed_land;
            stages[index][1] = stages[index][0] * production_continentality;
            stages[index][2] = stages[index][1] * lowland;
            stages[index][3] = stages[index][2] * climate;
            stages[index][4] = stages[index][3] * stratiform_wind;
            elevation_km[index] = local_elevation_km;
            rain_shadow[index] = local_rain_shadow;
            temperature_c[index] = local_temperature_c;
            broad_noise[index] = broad;
            lowland_factor[index] = lowland;
        }
    }

    let mut component = vec![false; size * size];
    let center = (size / 2) * size + size / 2;
    if land[center] {
        component[center] = true;
        let mut stack = vec![(size / 2, size / 2)];
        while let Some((x, y)) = stack.pop() {
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
                if land[next] && !component[next] {
                    component[next] = true;
                    stack.push((next_x, next_y));
                }
            }
        }
    }

    let summarize = |mask: &[bool]| {
        let indices: Vec<_> = mask
            .iter()
            .enumerate()
            .filter_map(|(index, &included)| included.then_some(index))
            .collect::<Vec<_>>();
        let count = indices.len();
        let shares: [f32; 5] = std::array::from_fn(|stage| {
            indices
                .iter()
                .filter(|&&index| stages[index][stage] > 0.25)
                .count() as f32
                / count.max(1) as f32
        });
        let dry_or_high = indices
            .iter()
            .filter(|&&index| elevation_km[index] >= 1.8 || rain_shadow[index] >= 0.5)
            .count() as f32
            / count.max(1) as f32;
        let median = |values: &[f32]| {
            native_default_quantile(
                &mut indices
                    .iter()
                    .map(|&index| values[index])
                    .collect::<Vec<_>>(),
                0.5,
            )
        };
        (
            count,
            shares,
            dry_or_high,
            median(&elevation_km),
            median(&rain_shadow),
            median(&temperature_c),
            median(&broad_noise),
        )
    };
    let visible_summary = summarize(&land);
    let center_summary = summarize(&component);
    let center_component_status = if land[center] {
        "APPLICABLE"
    } else {
        "NOT_APPLICABLE_CENTER_IS_OCEAN"
    };

    format!(
        "source_visible_land_pixels={}\nsource_visible_land_fraction_disk={:.5}\nsource_visible_land_s0_exposed_share_gt_025={:.5}\nsource_visible_land_s1_continentality_share_gt_025={:.5}\nsource_visible_land_s2_lowland_share_gt_025={:.5}\nsource_visible_land_s3_climate_rain_shadow_share_gt_025={:.5}\nsource_visible_land_s4_stratiform_wind_share_gt_025={:.5}\nsource_visible_land_dry_or_high_share={:.5}\nsource_visible_land_elevation_km_median={:.5}\nsource_visible_land_rain_shadow_median={:.5}\nsource_visible_land_temperature_c_median={:.5}\nsource_visible_land_broad_noise_median={:.5}\nscreen_center_land_component_pixels={}\nscreen_center_land_component_fraction_disk={:.5}\nscreen_center_land_component={}\n",
        visible_summary.0,
        visible_summary.0 as f32 / visible.max(1) as f32,
        visible_summary.1[0],
        visible_summary.1[1],
        visible_summary.1[2],
        visible_summary.1[3],
        visible_summary.1[4],
        visible_summary.2,
        visible_summary.3,
        visible_summary.4,
        visible_summary.5,
        visible_summary.6,
        center_summary.0,
        center_summary.0 as f32 / visible.max(1) as f32,
        center_component_status,
    )
}

fn native_default_wind_data(wind: &WindField) -> Vec<f32> {
    (0..6)
        .flat_map(|face| {
            wind.continentality[face].iter().enumerate().flat_map(
                move |(index, &continentality)| {
                    let base = (face * wind.continentality[face].len() + index) * 3;
                    [
                        wind.wind[base],
                        wind.wind[base + 1],
                        wind.wind[base + 2],
                        continentality,
                    ]
                },
            )
        })
        .collect()
}

fn native_default_height_data(terrain: &TectonicTerrain) -> Vec<f32> {
    terrain
        .faces
        .iter()
        .flat_map(|face| face.iter().flat_map(|&height| [height, 0.0, 0.0, 0.0]))
        .collect()
}

fn native_default_dot(a: [f32; 3], b: [f32; 3]) -> f32 {
    a[0] * b[0] + a[1] * b[1] + a[2] * b[2]
}

fn native_default_normalize(vector: [f32; 3]) -> [f32; 3] {
    let length = native_default_dot(vector, vector).sqrt().max(f32::EPSILON);
    [vector[0] / length, vector[1] / length, vector[2] / length]
}

fn native_default_smooth_step(edge0: f32, edge1: f32, value: f32) -> f32 {
    let t = ((value - edge0) / (edge1 - edge0)).clamp(0.0, 1.0);
    t * t * (3.0 - 2.0 * t)
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

fn u3_screen_direction_from_ndc(ndc_x: f32, ndc_y: f32, rotation: [[f32; 4]; 4]) -> [f32; 3] {
    let z = (1.0 - ndc_x * ndc_x - ndc_y * ndc_y).sqrt();
    [
        rotation[0][0] * ndc_x + rotation[1][0] * ndc_y + rotation[2][0] * z,
        rotation[0][1] * ndc_x + rotation[1][1] * ndc_y + rotation[2][1] * z,
        rotation[0][2] * ndc_x + rotation[1][2] * ndc_y + rotation[2][2] * z,
    ]
}

fn u3_screen_direction(x: usize, y: usize, size: usize, rotation: [[f32; 4]; 4]) -> [f32; 3] {
    let ndc_x = (x as f32 + 0.5) / size as f32 * 2.0 - 1.0;
    let ndc_y = (y as f32 + 0.5) / size as f32 * 2.0 - 1.0;
    u3_screen_direction_from_ndc(ndc_x, ndc_y, rotation)
}

fn u3_cube_coordinates(direction: [f32; 3], resolution: u32) -> (usize, f32, f32) {
    let [x, y, z] = direction;
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
    let coordinate = |value: f32| (value + 1.0) * 0.5 * resolution as f32 - 0.5;
    (face, coordinate(s), coordinate(t))
}

fn u3_cube_texel(
    values: &[f32],
    resolution: u32,
    face: usize,
    x: isize,
    y: isize,
    channel: usize,
) -> f32 {
    let width = resolution as usize;
    let (face, x, y) = if (0..width as isize).contains(&x) && (0..width as isize).contains(&y) {
        (face, x as usize, y as usize)
    } else {
        let direction = planet_gen::cube_sphere::cube_to_sphere(
            face as u32,
            (x as f32 + 0.5) / width as f32,
            (y as f32 + 0.5) / width as f32,
        );
        let (face, x, y) = u3_cube_coordinates(direction, resolution);
        (
            face,
            x.round().clamp(0.0, (width - 1) as f32) as usize,
            y.round().clamp(0.0, (width - 1) as f32) as usize,
        )
    };
    values[(face * width * width + y * width + x) * 4 + channel]
}

fn u3_linear_cube_sample(
    values: &[f32],
    resolution: u32,
    direction: [f32; 3],
    channel: usize,
) -> f32 {
    let (face, x, y) = u3_cube_coordinates(direction, resolution);
    let x0 = x.floor() as isize;
    let y0 = y.floor() as isize;
    let x1 = x0 + 1;
    let y1 = y0 + 1;
    let tx = x - x0 as f32;
    let ty = y - y0 as f32;
    let top = u3_cube_texel(values, resolution, face, x0, y0, channel) * (1.0 - tx)
        + u3_cube_texel(values, resolution, face, x1, y0, channel) * tx;
    let bottom = u3_cube_texel(values, resolution, face, x0, y1, channel) * (1.0 - tx)
        + u3_cube_texel(values, resolution, face, x1, y1, channel) * tx;
    top * (1.0 - ty) + bottom * ty
}

struct U3RayContext<'a> {
    mass: &'a [f32],
    geometry: &'a [f32],
    resolution: u32,
    rotation: [[f32; 4]; 4],
    radius_km: f32,
    pan: [f32; 2],
    zoom: f32,
    cloud_seed: u32,
}

fn u3_normalize3(vector: [f32; 3]) -> [f32; 3] {
    let length = (vector[0] * vector[0] + vector[1] * vector[1] + vector[2] * vector[2])
        .sqrt()
        .max(f32::EPSILON);
    [vector[0] / length, vector[1] / length, vector[2] / length]
}

fn u3_noise_seed_offset(seed: u32, stream: u32) -> [f32; 3] {
    let mixed = seed ^ stream.wrapping_mul(2_654_435_769);
    let hash = |value: u32| {
        let mut value = value.wrapping_mul(747_796_405).wrapping_add(2_891_336_453);
        value = ((value >> ((value >> 28) + 4)) ^ value).wrapping_mul(277_803_737);
        (value >> 22) ^ value
    };
    [
        (hash(mixed) & 0xffff) as f32 / 655.35,
        (hash(mixed ^ 0x68bc_21eb) & 0xffff) as f32 / 655.35,
        (hash(mixed ^ 0x02e5_be93) & 0xffff) as f32 / 655.35,
    ]
}

fn u3_mod289(value: f32) -> f32 {
    value - (value * (1.0 / 289.0)).floor() * 289.0
}

fn u3_permute(value: f32) -> f32 {
    u3_mod289(((value * 34.0) + 1.0) * value)
}

fn u3_snoise(v: [f32; 3]) -> f32 {
    let c_x = 1.0 / 6.0;
    let c_y = 1.0 / 3.0;
    let skew = (v[0] + v[1] + v[2]) * c_y;
    let i = [
        (v[0] + skew).floor(),
        (v[1] + skew).floor(),
        (v[2] + skew).floor(),
    ];
    let unskew = (i[0] + i[1] + i[2]) * c_x;
    let x0 = [
        v[0] - i[0] + unskew,
        v[1] - i[1] + unskew,
        v[2] - i[2] + unskew,
    ];
    let g = [
        (x0[0] >= x0[1]) as u8 as f32,
        (x0[1] >= x0[2]) as u8 as f32,
        (x0[2] >= x0[0]) as u8 as f32,
    ];
    let l = [1.0 - g[0], 1.0 - g[1], 1.0 - g[2]];
    let i1 = [g[0].min(l[2]), g[1].min(l[0]), g[2].min(l[1])];
    let i2 = [g[0].max(l[2]), g[1].max(l[0]), g[2].max(l[1])];
    let x1 = [
        x0[0] - i1[0] + c_x,
        x0[1] - i1[1] + c_x,
        x0[2] - i1[2] + c_x,
    ];
    let x2 = [
        x0[0] - i2[0] + c_y,
        x0[1] - i2[1] + c_y,
        x0[2] - i2[2] + c_y,
    ];
    let x3 = [x0[0] - 0.5, x0[1] - 0.5, x0[2] - 0.5];
    let i = [u3_mod289(i[0]), u3_mod289(i[1]), u3_mod289(i[2])];
    let p = [
        u3_permute(u3_permute(u3_permute(i[2]) + i[1]) + i[0]),
        u3_permute(u3_permute(u3_permute(i[2] + i1[2]) + i[1] + i1[1]) + i[0] + i1[0]),
        u3_permute(u3_permute(u3_permute(i[2] + i2[2]) + i[1] + i2[1]) + i[0] + i2[0]),
        u3_permute(u3_permute(u3_permute(i[2] + 1.0) + i[1] + 1.0) + i[0] + 1.0),
    ];
    let n = 0.142_857_15;
    let ns = [n * 2.0, n * 0.5 - 1.0, n];
    let j = p.map(|value| value - 49.0 * (value * ns[2] * ns[2]).floor());
    let x_ = j.map(|value| (value * ns[2]).floor());
    let y_: [f32; 4] = std::array::from_fn(|index| (j[index] - 7.0 * x_[index]).floor());
    let x = x_.map(|value| value * ns[0] + ns[1]);
    let y = y_.map(|value| value * ns[0] + ns[1]);
    let h: [f32; 4] = std::array::from_fn(|index| 1.0 - x[index].abs() - y[index].abs());
    let b0 = [x[0], x[1], y[0], y[1]];
    let b1 = [x[2], x[3], y[2], y[3]];
    let s0 = b0.map(|value| value.floor() * 2.0 + 1.0);
    let s1 = b1.map(|value| value.floor() * 2.0 + 1.0);
    let sh = h.map(|value| if 0.0 >= value { -1.0 } else { 0.0 });
    let a0 = [
        b0[0] + s0[0] * sh[0],
        b0[2] + s0[2] * sh[0],
        b0[1] + s0[1] * sh[1],
        b0[3] + s0[3] * sh[1],
    ];
    let a1 = [
        b1[0] + s1[0] * sh[2],
        b1[2] + s1[2] * sh[2],
        b1[1] + s1[1] * sh[3],
        b1[3] + s1[3] * sh[3],
    ];
    let mut gradients = [
        [a0[0], a0[1], h[0]],
        [a0[2], a0[3], h[1]],
        [a1[0], a1[1], h[2]],
        [a1[2], a1[3], h[3]],
    ];
    for gradient in &mut gradients {
        let dot = gradient[0] * gradient[0] + gradient[1] * gradient[1] + gradient[2] * gradient[2];
        let inverse_sqrt = 1.792_842_9 - 0.853_734_73 * dot;
        *gradient = [
            gradient[0] * inverse_sqrt,
            gradient[1] * inverse_sqrt,
            gradient[2] * inverse_sqrt,
        ];
    }
    let corners = [x0, x1, x2, x3];
    let mut result = 0.0;
    for index in 0..4 {
        let dot = gradients[index][0] * corners[index][0]
            + gradients[index][1] * corners[index][1]
            + gradients[index][2] * corners[index][2];
        let attenuation = (0.6
            - (corners[index][0] * corners[index][0]
                + corners[index][1] * corners[index][1]
                + corners[index][2] * corners[index][2]))
            .max(0.0);
        result += attenuation * attenuation * attenuation * attenuation * dot;
    }
    42.0 * result
}

fn u3_ray_jitter_taps(
    ndc_x: f32,
    ndc_y: f32,
    rotation: [[f32; 4]; 4],
    cloud_seed: u32,
) -> [f32; 2] {
    let ray = u3_normalize3([ndc_x, ndc_y, 0.5]);
    let anchor = u3_normalize3([
        rotation[0][0] * ray[0] + rotation[1][0] * ray[1] + rotation[2][0] * ray[2],
        rotation[0][1] * ray[0] + rotation[1][1] * ray[1] + rotation[2][1] * ray[2],
        rotation[0][2] * ray[0] + rotation[1][2] * ray[1] + rotation[2][2] * ray[2],
    ]);
    let offset = u3_noise_seed_offset(cloud_seed, 45);
    let jitter = 0.5
        + u3_snoise([
            anchor[0] * 47.0 + offset[0],
            anchor[1] * 47.0 + offset[1],
            anchor[2] * 47.0 + offset[2],
        ]) * 0.005;
    [jitter, jitter]
}

fn u3_ray_ndc(x: usize, y: usize, size: usize, context: &U3RayContext<'_>) -> [f32; 2] {
    [
        (((x as f32 + 0.5) / size as f32 * 2.0 - 1.0) / 0.85 - context.pan[0]) / context.zoom,
        (((y as f32 + 0.5) / size as f32 * 2.0 - 1.0) / 0.85 - context.pan[1]) / context.zoom,
    ]
}

#[derive(Clone, Copy)]
struct U3RaySample {
    mass: f32,
    face: usize,
    uv: [f32; 2],
}

fn u3_ray_samples(
    x: usize,
    y: usize,
    size: usize,
    context: &U3RayContext<'_>,
) -> Option<[U3RaySample; 8]> {
    let [ndc_x, ndc_y] = u3_ray_ndc(x, y, size, context);
    let r2 = ndc_x * ndc_x + ndc_y * ndc_y;
    if r2 > 1.0 {
        return None;
    }
    let surface = u3_screen_direction_from_ndc(ndc_x, ndc_y, context.rotation);
    let top_radius = 1.0
        + u3_linear_cube_sample(context.geometry, context.resolution, surface, 3)
            / context.radius_km.max(1.0);
    let z_cloud = (top_radius * top_radius - r2).max(0.0).sqrt();
    let z_surface = (1.0 - r2).sqrt();
    let step = (z_cloud - z_surface) / 8.0;
    let sample_world = |sample: usize, jitter: f32| {
        let z = z_cloud - (sample as f32 + jitter) * step;
        let length = (r2 + z * z).sqrt();
        let local = [ndc_x / length, ndc_y / length, z / length];
        [
            context.rotation[0][0] * local[0]
                + context.rotation[1][0] * local[1]
                + context.rotation[2][0] * local[2],
            context.rotation[0][1] * local[0]
                + context.rotation[1][1] * local[1]
                + context.rotation[2][1] * local[2],
            context.rotation[0][2] * local[0]
                + context.rotation[1][2] * local[1]
                + context.rotation[2][2] * local[2],
        ]
    };
    let jitter_taps = u3_ray_jitter_taps(ndc_x, ndc_y, context.rotation, context.cloud_seed);
    Some(std::array::from_fn(|sample| {
        let world = sample_world(sample, 0.5);
        let jittered_mass = jitter_taps
            .into_iter()
            .map(|jitter| {
                let world = sample_world(sample, jitter);
                u3_linear_cube_sample(context.mass, context.resolution, world, 0)
                    + u3_linear_cube_sample(context.mass, context.resolution, world, 1) * 1.2
                    + u3_linear_cube_sample(context.mass, context.resolution, world, 2) * 0.35
            })
            .sum::<f32>()
            * 0.5;
        let (face, sample_x, sample_y) = u3_cube_coordinates(world, context.resolution);
        U3RaySample {
            mass: jittered_mass,
            face,
            uv: [
                (sample_x + 0.5) / context.resolution as f32,
                (sample_y + 0.5) / context.resolution as f32,
            ],
        }
    }))
}

fn u3_ray_mass(x: usize, y: usize, size: usize, context: &U3RayContext<'_>) -> f32 {
    u3_ray_samples(x, y, size, context)
        .map(|samples| {
            samples
                .into_iter()
                .map(|sample| sample.mass)
                .fold(0.0, f32::max)
        })
        .unwrap_or(0.0)
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
    context: &U3RayContext<'_>,
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
        let [ndc_x, ndc_y] = u3_ray_ndc(x, y, size, context);
        if ndc_x * ndc_x + ndc_y * ndc_y > 1.0 {
            continue;
        }
        visible += 1;
        let direction = u3_screen_direction_from_ndc(ndc_x, ndc_y, context.rotation);
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
        let mass_pixel = u15_sphere_to_pixel(direction, context.resolution) * 4;
        let family = [
            context.mass[mass_pixel],
            context.mass[mass_pixel + 1],
            context.mass[mass_pixel + 2],
        ]
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
        combined_mass.push(u3_ray_mass(x, y, size, context));
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

#[derive(Debug, Clone)]
struct U3RenderedMassAssociation {
    support: f32,
    zero_mass_opaque: usize,
    first_failure: Option<String>,
}

fn u3_mass_association(masses: impl IntoIterator<Item = f32>) -> U3RenderedMassAssociation {
    let (supported, zero_mass_opaque, opaque) = masses.into_iter().fold(
        (0usize, 0usize, 0usize),
        |(supported, zero_mass_opaque, opaque), mass| {
            (
                supported + usize::from(mass.is_finite() && mass > 0.0),
                zero_mass_opaque + usize::from(mass == 0.0),
                opaque + 1,
            )
        },
    );
    U3RenderedMassAssociation {
        support: supported as f32 / opaque.max(1) as f32,
        zero_mass_opaque,
        first_failure: None,
    }
}

fn u3_rendered_mass_association(
    density: &[u8],
    context: &U3RayContext<'_>,
) -> U3RenderedMassAssociation {
    let size = (density.len() / 4).isqrt();
    let mut masses = Vec::new();
    let mut first_failure = None;
    for (index, pixel) in density.chunks_exact(4).enumerate() {
        let x = index % size;
        let y = index / size;
        if srgb_to_linear(pixel[0]) <= 0.05 {
            continue;
        }
        let Some(samples) = u3_ray_samples(x, y, size, context) else {
            continue;
        };
        let mass = samples
            .into_iter()
            .map(|sample| sample.mass)
            .fold(0.0, f32::max);
        if mass == 0.0 && first_failure.is_none() {
            let ndc = u3_ray_ndc(x, y, size, context);
            let details = samples
                .into_iter()
                .map(|sample| {
                    format!(
                        "f{} uv=({:.4},{:.4}) m={:.7}",
                        sample.face, sample.uv[0], sample.uv[1], sample.mass
                    )
                })
                .collect::<Vec<_>>()
                .join("; ");
            first_failure = Some(format!(
                "pixel=({x},{y}) ndc=({:.6},{:.6}) {details}",
                ndc[0], ndc[1]
            ));
        }
        masses.push(mass);
    }
    let mut result = u3_mass_association(masses);
    result.first_failure = first_failure;
    result
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

fn requires_weather_validation_size(
    weather_validation: bool,
    u15_validation: bool,
    u3_validation: bool,
    native_default_cloud_validation: bool,
) -> bool {
    weather_validation || u15_validation || u3_validation || native_default_cloud_validation
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
) -> Vec<&'static str> {
    let mut failures = Vec::new();
    if !(0.05..=0.80).contains(&topology.occupied) {
        failures.push("cloud topology is empty or a global veil");
    }
    if topology.coherent < 0.90 {
        failures.push("cloud topology is dominated by isolated noise");
    }
    if topology.zonal_continuity < 0.60 || topology.meridional_continuity < 0.60 {
        failures.push("cloud field lacks directional continuity");
    }
    if topology.components < 2 || topology.largest_component > 0.990 {
        failures.push("cloud component topology is degenerate");
    }
    if topology.ribbon_like > 0.45 {
        failures.push("cloud field is dominated by thin ribbons");
    }
    if topology.polar_occupied > 0.85 {
        failures.push("cloud field forms a polar slab");
    }
    if topology.directional_anisotropy > 0.65 {
        failures.push("cloud field has excessive directional anisotropy");
    }
    if seed_change.is_some_and(|change| !(0.03..=0.60).contains(&change)) {
        failures.push("cloud seed variation is invisible or replaces the physical field");
    }
    failures
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

fn u14_masked_mean(values: &[f32], channel: usize, mask: &[bool]) -> f32 {
    let (total, count) = values
        .chunks_exact(4)
        .zip(mask)
        .filter(|(_, selected)| **selected)
        .fold((0.0, 0usize), |(total, count), (pixel, _)| {
            (total + pixel[channel], count + 1)
        });
    total / count.max(1) as f32
}

fn u14_masked_quantile(values: &[f32], channel: usize, mask: &[bool], quantile: f32) -> f32 {
    let mut samples: Vec<_> = values
        .chunks_exact(4)
        .zip(mask)
        .filter(|(_, selected)| **selected)
        .map(|(pixel, _)| pixel[channel])
        .collect();
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

#[derive(Debug, Clone, Copy)]
struct U14CoverageSupportMetrics {
    occupied: f32,
    clear: f32,
    component_p75: Option<f32>,
    clear_gap_radius: f32,
}

#[derive(Debug, Clone)]
struct U14CoverageComponent {
    pixels: Vec<usize>,
    area_fraction: f32,
}

#[derive(Debug, Default)]
struct U14CoverageGrowth {
    ratios: Vec<f32>,
    missing_low_components: Vec<usize>,
    merged_low_components: usize,
}

const U14_OCCUPIED_THRESHOLD: f32 = 0.01;
const U14_MINIMUM_COMPONENT_FACE_AREA: f32 = 0.0025;

fn u14_column_tau(pixel: &[f32]) -> f32 {
    1.2 * (0.5 * pixel[0] + 1.2 * pixel[1] + 0.35 * pixel[2])
}

#[derive(Debug, Clone, Copy)]
struct PolarBandMetrics {
    low_mean: f32,
    occupied: f32,
}

#[derive(Debug, Clone, Copy)]
struct PolarMetrics {
    polar: PolarBandMetrics,
    adjacent: PolarBandMetrics,
}

fn polar_metrics(mass: &[f32], resolution: u32, axial_tilt_rad: f32, north: bool) -> PolarMetrics {
    let (sin, cos) = axial_tilt_rad.sin_cos();
    let mut polar_low = 0.0;
    let mut polar_occupied = 0.0;
    let mut polar_weight = 0.0;
    let mut adjacent_low = 0.0;
    let mut adjacent_occupied = 0.0;
    let mut adjacent_weight = 0.0;
    for face in 0..6 {
        for y in 0..resolution {
            for x in 0..resolution {
                let position = planet_gen::cube_sphere::cube_to_sphere(
                    face,
                    x as f32 / (resolution - 1) as f32,
                    y as f32 / (resolution - 1) as f32,
                );
                let latitude = (position[1] * cos + position[2] * sin)
                    .clamp(-1.0, 1.0)
                    .asin()
                    .to_degrees();
                let latitude = if north { latitude } else { -latitude };
                let band = if latitude >= 70.0 {
                    Some(true)
                } else if latitude >= 55.0 {
                    Some(false)
                } else {
                    None
                };
                let Some(is_polar) = band else { continue };
                let index = ((face * resolution * resolution + y * resolution + x) * 4) as usize;
                let pixel = &mass[index..index + 4];
                let weight = u15_weight(x, y, resolution);
                if is_polar {
                    polar_low += pixel[0] * weight;
                    polar_occupied +=
                        f32::from(u14_column_tau(pixel) >= U14_OCCUPIED_THRESHOLD) * weight;
                    polar_weight += weight;
                } else {
                    adjacent_low += pixel[0] * weight;
                    adjacent_occupied +=
                        f32::from(u14_column_tau(pixel) >= U14_OCCUPIED_THRESHOLD) * weight;
                    adjacent_weight += weight;
                }
            }
        }
    }
    PolarMetrics {
        polar: PolarBandMetrics {
            low_mean: polar_low / polar_weight.max(f32::EPSILON),
            occupied: polar_occupied / polar_weight.max(f32::EPSILON),
        },
        adjacent: PolarBandMetrics {
            low_mean: adjacent_low / adjacent_weight.max(f32::EPSILON),
            occupied: adjacent_occupied / adjacent_weight.max(f32::EPSILON),
        },
    }
}

fn run_earth_polar_validation(
    mass: &[f32],
    resolution: u32,
    axial_tilt_rad: f32,
    output_dir: &str,
) -> Vec<String> {
    let mut failures = Vec::new();
    let mut rows = Vec::new();
    for (name, north) in [("north", true), ("south", false)] {
        let metrics = polar_metrics(mass, resolution, axial_tilt_rad, north);
        rows.push(format!(
            "{name}: polar_low_mean={:.5} adjacent_low_mean={:.5} polar_occupied={:.5} adjacent_occupied={:.5}",
            metrics.polar.low_mean,
            metrics.adjacent.low_mean,
            metrics.polar.occupied,
            metrics.adjacent.occupied,
        ));
        if metrics.polar.low_mean > metrics.adjacent.low_mean * 0.85 + 0.005
            || metrics.polar.occupied > metrics.adjacent.occupied + 0.10
            || metrics.polar.low_mean < metrics.adjacent.low_mean * 0.10
        {
            failures.push(format!(
                "polar {name}: low={:.5}/{:.5}, occupied={:.5}/{:.5}",
                metrics.polar.low_mean,
                metrics.adjacent.low_mean,
                metrics.polar.occupied,
                metrics.adjacent.occupied,
            ));
        }
    }
    std::fs::write(
        Path::new(output_dir).join("earth_polar_metrics.txt"),
        format!(
            "latitude_axis=tilted_y=position.y*cos(axial_tilt)+position.z*sin(axial_tilt)\nweights=solid_angle\noccupancy=tau>=.01\ngates=polar_low_mean<=.85*adjacent+.005,polar_occupied<=adjacent+.10,polar_low_mean>=.10*adjacent\n{}\n",
            rows.join("\n"),
        ),
    )
    .expect("write Earth polar metrics artifact");
    println!("Earth polar metrics:\n  {}", rows.join("\n  "));
    failures
}

fn u14_coverage_support_metrics(values: &[f32], resolution: u32) -> U14CoverageSupportMetrics {
    let resolution = resolution as usize;
    let face_pixels = resolution * resolution;
    let occupied: Vec<_> = values
        .chunks_exact(4)
        .map(|pixel| u14_column_tau(pixel) >= U14_OCCUPIED_THRESHOLD)
        .collect();
    let mut component_areas: Vec<_> =
        u14_significant_occupied_components(values, resolution as u32)
            .into_iter()
            .map(|component| component.area_fraction)
            .collect();
    let mut visited = vec![false; occupied.len()];
    let mut largest_clear = 0usize;
    for start in 0..occupied.len() {
        if visited[start] || occupied[start] {
            continue;
        }
        let mut stack = vec![start];
        let mut area = 0usize;
        visited[start] = true;
        while let Some(index) = stack.pop() {
            area += 1;
            for neighbor in u15_pixel_neighbors(index, resolution as u32) {
                if !visited[neighbor] && !occupied[neighbor] {
                    visited[neighbor] = true;
                    stack.push(neighbor);
                }
            }
        }
        largest_clear = largest_clear.max(area);
    }
    component_areas.sort_by(f32::total_cmp);
    let component_p75 = component_areas
        .get((component_areas.len().saturating_sub(1) * 3) / 4)
        .copied();
    U14CoverageSupportMetrics {
        occupied: occupied.iter().filter(|&&value| value).count() as f32 / occupied.len() as f32,
        clear: occupied.iter().filter(|&&value| !value).count() as f32 / occupied.len() as f32,
        component_p75,
        clear_gap_radius: (largest_clear as f32 / face_pixels as f32 / std::f32::consts::PI).sqrt(),
    }
}

fn u14_significant_occupied_components(
    values: &[f32],
    resolution: u32,
) -> Vec<U14CoverageComponent> {
    let resolution = resolution as usize;
    let face_pixels = resolution * resolution;
    let occupied: Vec<_> = values
        .chunks_exact(4)
        .map(|pixel| u14_column_tau(pixel) >= U14_OCCUPIED_THRESHOLD)
        .collect();
    let mut visited = vec![false; occupied.len()];
    let mut components = Vec::new();
    for start in 0..occupied.len() {
        if visited[start] || !occupied[start] {
            continue;
        }
        let mut stack = vec![start];
        let mut pixels = Vec::new();
        visited[start] = true;
        while let Some(index) = stack.pop() {
            pixels.push(index);
            for neighbor in u15_pixel_neighbors(index, resolution as u32) {
                if !visited[neighbor] && occupied[neighbor] {
                    visited[neighbor] = true;
                    stack.push(neighbor);
                }
            }
        }
        let area_fraction = pixels.len() as f32 / face_pixels as f32;
        if area_fraction >= U14_MINIMUM_COMPONENT_FACE_AREA {
            components.push(U14CoverageComponent {
                pixels,
                area_fraction,
            });
        }
    }
    components
}

fn u14_coverage_growth(
    low: &[U14CoverageComponent],
    high: &[U14CoverageComponent],
    resolution: u32,
) -> U14CoverageGrowth {
    let mut high_labels = vec![None; resolution as usize * resolution as usize * 6];
    for (label, component) in high.iter().enumerate() {
        for &pixel in &component.pixels {
            high_labels[pixel] = Some(label);
        }
    }
    let mut matches = Vec::with_capacity(low.len());
    let mut growth = U14CoverageGrowth::default();
    for (low_label, component) in low.iter().enumerate() {
        let mut overlap = vec![0usize; high.len()];
        for &pixel in &component.pixels {
            if let Some(high_label) = high_labels[pixel] {
                overlap[high_label] += 1;
            }
        }
        let Some((high_label, _)) = overlap
            .into_iter()
            .enumerate()
            .filter(|(_, overlap)| *overlap > 0)
            .max_by(|(left_label, left), (right_label, right)| {
                left.cmp(right).then_with(|| right_label.cmp(left_label))
            })
        else {
            growth.missing_low_components.push(low_label);
            continue;
        };
        matches.push(high_label);
        growth
            .ratios
            .push(high[high_label].area_fraction / component.area_fraction);
    }
    matches.sort_unstable();
    growth.merged_low_components = matches.windows(2).filter(|pair| pair[0] == pair[1]).count();
    growth
}

fn u14_fixed_core_p90(values: &[f32], mask: &[bool]) -> f32 {
    let mut core: Vec<_> = values
        .chunks_exact(4)
        .zip(mask)
        .filter(|(_, fixed)| **fixed)
        .map(|(pixel, _)| u14_column_tau(pixel))
        .collect();
    core.sort_by(f32::total_cmp);
    core.get(((core.len().saturating_sub(1) as f32) * 0.9).round() as usize)
        .copied()
        .unwrap_or(0.0)
}

const U14_FLAT_COOL_OCEAN_MASK: &str = "flat_cool_ocean";
const U14_FLAT_INLAND_MASK: &str = "flat_inland";
const U14_MOUNTAIN_WINDWARD_MASK: &str = "mountain_windward";
const U14_MOUNTAIN_LEE_MASK: &str = "mountain_lee";
const U14_COAST_BAND_MASK: &str = "coast_band";
const U14_LOW_COVERAGE_CORE: f32 = 0.25;

#[derive(Debug, Clone, Copy)]
struct U14SourceFlowMetrics {
    maritime_low_p90: f32,
    downwind_land_low_p90: f32,
    coastline_crossing_component: bool,
}

#[derive(Debug, Clone, Copy)]
struct U14LeeContinuationMetrics {
    advected_total_p90: f32,
    full_total_p90: f32,
    retention_ratio: f32,
    new_phase_total_p90: f32,
    new_phase_fraction: f32,
    channel_delta_p90: [f32; 3],
}

fn u14_total_field(values: &[f32]) -> Vec<f32> {
    values
        .chunks_exact(4)
        .flat_map(|pixel| [pixel[0] + pixel[1] + pixel[2], 0.0, 0.0, 0.0])
        .collect()
}

fn u14_lee_continuation_metrics(
    advected: &[f32],
    full: &[f32],
    lee_mask: &[bool],
) -> U14LeeContinuationMetrics {
    let advected_total = u14_total_field(advected);
    let full_total = u14_total_field(full);
    let advected_total_p90 = u14_masked_quantile(&advected_total, 0, lee_mask, 0.9);
    let full_total_p90 = u14_masked_quantile(&full_total, 0, lee_mask, 0.9);
    let increment: Vec<_> = advected
        .chunks_exact(4)
        .zip(full.chunks_exact(4))
        .map(|(advected, full)| {
            [
                ((full[0] + full[1] + full[2] - advected[0] - advected[1] - advected[2]).max(0.0)),
                0.0,
                0.0,
                0.0,
            ]
        })
        .flatten()
        .collect();
    let new_phase_total_p90 = u14_masked_quantile(&increment, 0, lee_mask, 0.9);
    U14LeeContinuationMetrics {
        advected_total_p90,
        full_total_p90,
        retention_ratio: full_total_p90 / advected_total_p90.max(0.0001),
        new_phase_total_p90,
        new_phase_fraction: new_phase_total_p90 / full_total_p90.max(0.0001),
        channel_delta_p90: std::array::from_fn(|channel| {
            let delta: Vec<_> = advected
                .chunks_exact(4)
                .zip(full.chunks_exact(4))
                .flat_map(|(advected, full)| {
                    [(full[channel] - advected[channel]).max(0.0), 0.0, 0.0, 0.0]
                })
                .collect();
            u14_masked_quantile(&delta, 0, lee_mask, 0.9)
        }),
    }
}

fn u14_total_increment(full: &[f32], advected: &[f32]) -> Vec<f32> {
    full.chunks_exact(4)
        .zip(advected.chunks_exact(4))
        .flat_map(|(full, advected)| {
            [
                ((full[0] + full[1] + full[2]) - (advected[0] + advected[1] + advected[2]))
                    .max(0.0),
                0.0,
                0.0,
                0.0,
            ]
        })
        .collect()
}

struct U14OrographicMasks {
    windward: Vec<bool>,
    lee: Vec<bool>,
}

fn u14_orographic_masks(resolution: u32) -> U14OrographicMasks {
    let mask = |windward| {
        (0..6)
            .flat_map(|face| {
                (0..resolution).flat_map(move |y| {
                    (0..resolution).map(move |x| {
                        let pos = planet_gen::cube_sphere::cube_to_sphere(
                            face,
                            x as f32 / (resolution - 1) as f32,
                            y as f32 / (resolution - 1) as f32,
                        );
                        pos[0] > 0.8
                            && if windward {
                                pos[2] > 0.04 && pos[2] < 0.12
                            } else {
                                pos[2] < -0.04 && pos[2] > -0.12
                            }
                    })
                })
            })
            .collect()
    };
    U14OrographicMasks {
        windward: mask(true),
        lee: mask(false),
    }
}

fn u14_coast_height(z: f32) -> f32 {
    -0.001 + 0.002 * ((z / 0.012).clamp(-1.0, 1.0) * 0.5 + 0.5)
}

fn u14_coast_continentality(z: f32) -> f32 {
    let t = ((z + 0.35) / 0.7).clamp(0.0, 1.0);
    t * t * (3.0 - 2.0 * t)
}

fn u14_marine_to_land_continentality(pos: [f32; 3]) -> f32 {
    if pos[2] > 0.0 { 0.0 } else { 1.0 }
}

fn u14_coast_terrain_localization_ratio(
    coast_mass: &[f32],
    flat_control_mass: &[f32],
    resolution: u32,
) -> f32 {
    let resolution = resolution as usize;
    let weather_texel = 1.0 / resolution as f32;
    let residual = |x: usize, y: usize| {
        coast_mass[(y * resolution + x) * 4] - flat_control_mass[(y * resolution + x) * 4]
    };
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
            let terrain_attributable_gradient =
                (residual(x + 1, y) - residual(x - 1, y)).abs() * 0.5;
            if z.abs() <= weather_texel {
                coast_energy += terrain_attributable_gradient;
                coast_samples += 1;
            } else if z.abs() >= weather_texel * 2.0 {
                surrounding_energy += terrain_attributable_gradient;
                surrounding_samples += 1;
            }
        }
    }
    let local_energy = coast_energy / coast_samples.max(1) as f32;
    let surrounding_energy = surrounding_energy / surrounding_samples.max(1) as f32;
    local_energy / surrounding_energy.max(f32::EPSILON)
}

fn u14_mixed_coast_land_support(values: &[f32], resolution: u32) -> (f32, f32) {
    let mut low_mass = Vec::new();
    let mut occupied = 0usize;
    for face in 0..6 {
        for y in 0..resolution {
            for x in 0..resolution {
                let position = planet_gen::cube_sphere::cube_to_sphere(
                    face,
                    x as f32 / (resolution - 1) as f32,
                    y as f32 / (resolution - 1) as f32,
                );
                if u14_marine_to_land_continentality(position) < 1.0
                    || position[0] <= 0.8
                    || !(position[2] < -0.04 && position[2] > -0.12)
                {
                    continue;
                }
                let pixel =
                    &values[((face * resolution * resolution + y * resolution + x) * 4) as usize..];
                low_mass.push(pixel[0]);
                occupied += usize::from(pixel[3] >= 0.01);
            }
        }
    }
    low_mass.sort_by(f32::total_cmp);
    let p90 = low_mass[(low_mass.len() * 9 / 10).min(low_mass.len() - 1)];
    (occupied as f32 / low_mass.len() as f32, p90)
}

fn u14_source_flow_metrics(values: &[f32], resolution: u32) -> U14SourceFlowMetrics {
    let mut maritime_low = Vec::new();
    let mut downwind_land_low = Vec::new();
    for (index, pixel) in values.chunks_exact(4).enumerate() {
        let position = u15_pixel_position(index, resolution);
        if position[0] <= 0.8 {
            continue;
        }
        if position[2] > 0.04 && position[2] < 0.12 {
            maritime_low.push(pixel[0]);
        } else if position[2] < -0.04 && position[2] > -0.12 {
            downwind_land_low.push(pixel[0]);
        }
    }
    let p90 = |samples: &mut Vec<f32>| {
        samples.sort_by(f32::total_cmp);
        samples
            .get((samples.len() * 9 / 10).min(samples.len().saturating_sub(1)))
            .copied()
            .unwrap_or(0.0)
    };
    let coastline_crossing_component = u14_significant_occupied_components(values, resolution)
        .into_iter()
        .any(|component| {
            let mut maritime = false;
            let mut land = false;
            for pixel in component.pixels {
                let position = u15_pixel_position(pixel, resolution);
                maritime |= position[0] > 0.8 && position[2] > 0.01;
                land |= position[0] > 0.8 && position[2] < -0.01;
            }
            maritime && land
        });
    U14SourceFlowMetrics {
        maritime_low_p90: p90(&mut maritime_low),
        downwind_land_low_p90: p90(&mut downwind_land_low),
        coastline_crossing_component,
    }
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
    // Terrain owns the vapor source, so the no-source inland fixture must present
    // real land (height above ocean_level) or it sources vapor like ocean.
    let inland_terrain = u14_flat_terrain(resolution, |_| 0.1);
    // The height sign change and the continentality transition share z=0, but only
    // the latter is broad. The fixed mask catches a cloud edge tracing the coast.
    let coast = u14_flat_terrain(resolution, |pos| u14_coast_height(pos[2]));
    let coast_control = u14_flat_terrain(resolution, |_| -0.001);
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
    let coverage_weather = |coverage: f32, seed: u32| {
        let dynamics = wind.create_test_textures(gpu, resolution, |pos| {
            let target = [1.0, 0.0, 0.0];
            let projection = target[0] * pos[0] + target[1] * pos[1] + target[2] * pos[2];
            (
                [
                    (target[0] - pos[0] * projection) * 0.8,
                    (target[1] - pos[1] * projection) * 0.8,
                    (target[2] - pos[2] * projection) * 0.8,
                    0.0,
                ],
                1013.0,
            )
        });
        let field = pipeline.create_textures(gpu, resolution);
        pipeline.generate(
            gpu,
            WeatherSnapshot {
                face: 0,
                resolution,
                seed,
                storm_count: 0,
                coverage,
                moisture: 1.0,
                surface_pressure_bar: 1.0,
                base_temp_c: 15.0,
                ocean_level: 0.0,
                axial_tilt_rad: 0.0,
                season: 0.5,
                storm_size: 1.0,
                radius_km: 6371.0,
                rotation_rate_rad_s: std::f32::consts::TAU / 86400.0,
                wind_scale: 1.0,
            },
            &terrain,
            &dynamics,
            &field,
        );
        field.read_mass(gpu)
    };
    let mut cool_ratio = f32::INFINITY;
    let mut low_deep = f32::INFINITY;
    let mut deck_min = f32::INFINITY;
    let mut deck_max = 0.0_f32;
    let mut trade_min = f32::INFINITY;
    let mut trade_max = 0.0_f32;
    let mut cool_deck_p90_min = f32::INFINITY;
    let mut trade_p90_min = f32::INFINITY;
    let mut windward_low_p90_min = f32::INFINITY;
    let mut windward_lee_low_p90_delta_min = f32::INFINITY;
    let mut lee_advected_low_p90_min = f32::INFINITY;
    let mut lee_full_low_p90_min = f32::INFINITY;
    let mut lee_retention_ratio_min = f32::INFINITY;
    let mut lee_new_phase_low_p90_max = 0.0_f32;
    let mut lee_new_phase_fraction_max = 0.0_f32;
    let mut gaps_min = f32::INFINITY;
    let mut gaps_max = 0.0_f32;
    let mut coast_terrain_localization_ratio_max = 0.0_f32;
    let mut coverage_increment_min = [f32::INFINITY; 8];
    let mut coverage_increment_max_by_step = [0.0_f32; 8];
    let mut coverage_increment_max = 0.0_f32;
    let mut coverage_increment_median_min = f32::INFINITY;
    let mut zero_exact = true;
    let orographic_masks = u14_orographic_masks(resolution);
    let mut orography_rows = Vec::new();
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
    let mut coverage_seed_rows = Vec::new();
    let mut inland_occupied_min = f32::INFINITY;
    let mut inland_low_p90_min = f32::INFINITY;
    let mut maritime_low_p90_min = f32::INFINITY;
    let mut downwind_land_maritime_p90_ratio_min = f32::INFINITY;
    let mut coastline_crossing_component = true;
    let mut inland_no_source_exact = true;
    let mut paired_lee_rows = Vec::new();
    // DS-045a: per-seed source-flow land support, logged so the U14 land_p90
    // gate rebaseline (median − k·σ across the frozen seeds) stays auditable.
    let mut source_flow_land_rows = Vec::new();
    for seed in SEEDS {
        let (cool_ocean, cool_geometry) = weather(&terrain, |_| 0.0, 5.0, 0.75, 1.0, seed, 0.0);
        let (cool_inland, inland_geometry) =
            weather(&inland_terrain, |_| 1.0, 5.0, 0.75, 1.0, seed, 0.0);
        let (repeat_mass, repeat_geometry) = weather(&terrain, |_| 0.0, 5.0, 0.75, 1.0, seed, 0.0);
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
        inland_no_source_exact &= cool_inland.iter().all(|value| *value == 0.0);
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

        let (mixed_coast_mass, mixed_coast_geometry) = weather(
            &terrain,
            u14_marine_to_land_continentality,
            15.0,
            1.0,
            1.0,
            seed,
            0.8,
        );
        let (mixed_coast_repeat, mixed_coast_repeat_geometry) = weather(
            &terrain,
            u14_marine_to_land_continentality,
            15.0,
            1.0,
            1.0,
            seed,
            0.8,
        );
        let (occupied, invalid) = u14_geometry_metrics(&mixed_coast_mass, &mixed_coast_geometry);
        geometry_occupied_texels += occupied;
        geometry_invalid_texels += invalid;
        let (coast_land_occupied, coast_land_low_p90) =
            u14_mixed_coast_land_support(&mixed_coast_mass, resolution);
        source_flow_land_rows.push(format!(
            "seed={seed} occupied={coast_land_occupied:.5} land_p90={coast_land_low_p90:.5}"
        ));
        inland_occupied_min = inland_occupied_min.min(coast_land_occupied);
        inland_low_p90_min = inland_low_p90_min.min(coast_land_low_p90);
        let source_flow = u14_source_flow_metrics(&mixed_coast_mass, resolution);
        let source_flow_repeat = u14_source_flow_metrics(&mixed_coast_repeat, resolution);
        let mixed_coast_deterministic = mixed_coast_mass == mixed_coast_repeat
            && mixed_coast_geometry == mixed_coast_repeat_geometry
            && source_flow.maritime_low_p90 == source_flow_repeat.maritime_low_p90
            && source_flow.downwind_land_low_p90 == source_flow_repeat.downwind_land_low_p90
            && source_flow.coastline_crossing_component
                == source_flow_repeat.coastline_crossing_component;
        deterministic &= mixed_coast_deterministic;
        if !mixed_coast_deterministic {
            failures.push(format!(
                "U14 mixed-coast source-flow was non-deterministic for seed {seed}"
            ));
        }
        maritime_low_p90_min = maritime_low_p90_min.min(source_flow.maritime_low_p90);
        downwind_land_maritime_p90_ratio_min = downwind_land_maritime_p90_ratio_min.min(
            source_flow.downwind_land_low_p90 / source_flow.maritime_low_p90.max(f32::EPSILON),
        );
        coastline_crossing_component &= source_flow.coastline_crossing_component;

        let (coast_mass, coast_geometry) = weather(
            &coast,
            |pos| u14_coast_continentality(pos[2]),
            15.0,
            0.75,
            1.0,
            seed,
            0.0,
        );
        let (coast_control_mass, coast_control_geometry) = weather(
            &coast_control,
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
        let (occupied, invalid) =
            u14_geometry_metrics(&coast_control_mass, &coast_control_geometry);
        geometry_occupied_texels += occupied;
        geometry_invalid_texels += invalid;
        coast_terrain_localization_ratio_max = coast_terrain_localization_ratio_max.max(
            u14_coast_terrain_localization_ratio(&coast_mass, &coast_control_mass, resolution),
        );

        let coverage: Vec<f32> = [0.0, 0.125, 0.25, 0.375, 0.5, 0.625, 0.75, 0.875, 1.0]
            .into_iter()
            .map(|coverage| {
                u14_field_mean(
                    &weather(&terrain, |_| 0.0, 15.0, coverage, 1.0, seed, 0.0).0,
                    3,
                )
            })
            .collect();
        let increments: Vec<_> = coverage.windows(2).map(|pair| pair[1] - pair[0]).collect();
        let mut sorted = increments.clone();
        sorted.sort_by(f32::total_cmp);
        coverage_increment_median_min = coverage_increment_median_min.min(sorted[sorted.len() / 2]);
        for (index, increment) in increments.into_iter().enumerate() {
            coverage_increment_min[index] = coverage_increment_min[index].min(increment);
            coverage_increment_max_by_step[index] =
                coverage_increment_max_by_step[index].max(increment);
            coverage_increment_max = coverage_increment_max.max(increment);
        }
        let support_mass =
            [U14_LOW_COVERAGE_CORE, 0.5, 0.75].map(|coverage| coverage_weather(coverage, seed));
        let support: [U14CoverageSupportMetrics; 3] = std::array::from_fn(|index| {
            u14_coverage_support_metrics(&support_mass[index], resolution)
        });
        let fixed_core: Vec<_> = support_mass[0]
            .chunks_exact(4)
            .map(|pixel| u14_column_tau(pixel) >= U14_OCCUPIED_THRESHOLD)
            .collect();
        let fixed_core_tau = support_mass
            .each_ref()
            .map(|mass| u14_fixed_core_p90(mass, &fixed_core));
        let mut coverage_pass = true;
        for index in 0..2 {
            let retained = support_mass[index]
                .chunks_exact(4)
                .zip(support_mass[index + 1].chunks_exact(4))
                .filter(|(before, after)| {
                    u14_column_tau(before) >= U14_OCCUPIED_THRESHOLD
                        && u14_column_tau(after) >= U14_OCCUPIED_THRESHOLD
                })
                .count() as f32
                / (support[index].occupied * (support_mass[index].len() / 4) as f32).max(1.0);
            let core_growth = fixed_core_tau[index + 1] / fixed_core_tau[index].max(f32::EPSILON);
            if support[index + 1].occupied - support[index].occupied < 0.08
                || retained < 0.90
                || support[index].clear - support[index + 1].clear < 0.08
                || core_growth > 1.35
            {
                coverage_pass = false;
                failures.push(format!(
                    "U14 causal coverage seed {seed} step {index}: area={:.3}, retained={retained:.3}, clear_drop={:.3}, core_growth={core_growth:.3}",
                    support[index + 1].occupied - support[index].occupied,
                    support[index].clear - support[index + 1].clear,
                ));
            }
        }
        let frozen_components = u14_significant_occupied_components(&support_mass[0], resolution);
        let high_components = u14_significant_occupied_components(&support_mass[2], resolution);
        let patch_growth = u14_coverage_growth(&frozen_components, &high_components, resolution);
        let mut growth_ratios = patch_growth.ratios.clone();
        growth_ratios.sort_by(f32::total_cmp);
        let conservative_median = growth_ratios
            .get(growth_ratios.len().saturating_sub(1) / 2)
            .copied();
        let gap_ratio = support[2].clear_gap_radius / support[0].clear_gap_radius.max(f32::EPSILON);
        match conservative_median {
            None => {
                coverage_pass = false;
                failures.push(format!(
                    "U14 causal coverage seed {seed}: no matched occupied components (>= {:.2}% face)",
                    U14_MINIMUM_COMPONENT_FACE_AREA * 100.0,
                ));
            }
            Some(conservative_median)
                if !patch_growth.missing_low_components.is_empty()
                    || conservative_median < 1.3
                    || gap_ratio > 0.75 =>
            {
                coverage_pass = false;
                failures.push(format!(
                    "U14 causal coverage seed {seed} component growth median={conservative_median:.3}, gap_ratio={gap_ratio:.3}, missing_low_components={:?}",
                    patch_growth.missing_low_components,
                ));
            }
            Some(_) => {}
        }
        coverage_seed_rows.push(format!(
            "seed={seed} coverage=[{}] fixed_mask=.25 frozen_components={} high_components={} component_growth_ratios={growth_ratios:?} component_growth_conservative_median={} missing_low_components={:?} merged_low_components={} fixed_p90_tau={fixed_core_tau:?} status={}",
            support
                .map(|metrics| format!(
                    "(occupied={:.5},clear={:.5},component_p75={},clear_gap_radius={:.5})",
                    metrics.occupied,
                    metrics.clear,
                    metrics
                        .component_p75
                        .map_or_else(|| "N/A".to_string(), |value| format!("{value:.5}")),
                    metrics.clear_gap_radius,
                ))
                .join(","),
            frozen_components.len(),
            high_components.len(),
            conservative_median.map_or_else(|| "N/A".to_string(), |value| format!("{value:.5}")),
            patch_growth.missing_low_components,
            patch_growth.merged_low_components,
            if coverage_pass { "PASS" } else { "FAIL" },
        ));
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

        // This ridge crosses the declared ocean/land boundary: vapor enters only
        // on the marine side, then the windward lowland can convert transported mass.
        let (calm, calm_geometry) = weather(
            &ridge,
            u14_marine_to_land_continentality,
            5.0,
            1.0,
            1.0,
            seed,
            0.0,
        );
        let (whisper, whisper_geometry) = weather(
            &ridge,
            u14_marine_to_land_continentality,
            5.0,
            1.0,
            1.0,
            seed,
            0.01,
        );
        let (forward, forward_geometry) = weather(
            &ridge,
            u14_marine_to_land_continentality,
            5.0,
            1.0,
            1.0,
            seed,
            1.0,
        );
        let (reverse, reverse_geometry) = weather(
            &ridge,
            u14_marine_to_land_continentality,
            5.0,
            1.0,
            1.0,
            seed,
            -1.0,
        );
        let ridge_dynamics = wind.create_test_textures(gpu, resolution, |pos| {
            let tangent = [pos[2], 0.0, -pos[0]];
            let length = (tangent[0] * tangent[0] + tangent[2] * tangent[2])
                .sqrt()
                .max(0.0001);
            (
                [
                    tangent[0] / length,
                    0.0,
                    tangent[2] / length,
                    u14_marine_to_land_continentality(pos),
                ],
                1025.0,
            )
        });
        let ridge_snapshot = WeatherSnapshot {
            face: 0,
            resolution,
            seed,
            storm_count: 0,
            coverage: 1.0,
            moisture: 1.0,
            surface_pressure_bar: 1.0,
            base_temp_c: 5.0,
            ocean_level: 0.0,
            axial_tilt_rad: 0.0,
            season: 0.5,
            storm_size: 1.0,
            radius_km: 6371.0,
            rotation_rate_rad_s: std::f32::consts::TAU / 86400.0,
            wind_scale: 1.0,
        };
        let paired = pipeline.validation_paired_continuation_for_sweep(
            gpu,
            ridge_snapshot,
            &ridge,
            &ridge_dynamics,
            ridge_snapshot,
            &ridge,
            &ridge_dynamics,
        );
        let paired_repeat = pipeline.validation_paired_continuation_for_sweep(
            gpu,
            ridge_snapshot,
            &ridge,
            &ridge_dynamics,
            ridge_snapshot,
            &ridge,
            &ridge_dynamics,
        );
        let lee_pair = u14_lee_continuation_metrics(
            &paired.advected_mass,
            &paired.full_mass,
            &orographic_masks.lee,
        );
        let lee_pair_repeat = u14_lee_continuation_metrics(
            &paired_repeat.advected_mass,
            &paired_repeat.full_mass,
            &orographic_masks.lee,
        );
        let paired_deterministic = paired.advected_mass == paired_repeat.advected_mass
            && paired.full_mass == paired_repeat.full_mass
            && lee_pair.advected_total_p90 == lee_pair_repeat.advected_total_p90
            && lee_pair.full_total_p90 == lee_pair_repeat.full_total_p90
            && lee_pair.new_phase_total_p90 == lee_pair_repeat.new_phase_total_p90;
        deterministic &= paired_deterministic;
        if !paired_deterministic {
            failures.push(format!(
                "U14 paired source-flow continuation was non-deterministic for seed {seed}"
            ));
        }
        let paired_windward_total = u14_total_field(&paired.full_mass);
        let paired_windward_total_p90 =
            u14_masked_quantile(&paired_windward_total, 0, &orographic_masks.windward, 0.9);
        let paired_reverse_windward =
            u14_masked_quantile(&reverse, 0, &orographic_masks.windward, 0.9);
        let paired_reverse_lee = u14_masked_quantile(&reverse, 0, &orographic_masks.lee, 0.9);
        lee_advected_low_p90_min = lee_advected_low_p90_min.min(lee_pair.advected_total_p90);
        lee_full_low_p90_min = lee_full_low_p90_min.min(lee_pair.full_total_p90);
        lee_retention_ratio_min = lee_retention_ratio_min.min(lee_pair.retention_ratio);
        lee_new_phase_low_p90_max = lee_new_phase_low_p90_max.max(lee_pair.new_phase_total_p90);
        lee_new_phase_fraction_max = lee_new_phase_fraction_max.max(lee_pair.new_phase_fraction);
        windward_low_p90_min = windward_low_p90_min.min(paired_windward_total_p90);
        windward_lee_low_p90_delta_min =
            windward_lee_low_p90_delta_min.min(paired_windward_total_p90 - lee_pair.full_total_p90);
        let paired_increment = u14_total_increment(&paired.full_mass, &paired.advected_mass);
        save_u14_total_density_view(
            output_dir,
            &format!("u14_seed_{seed}_coast_to_interior"),
            resolution,
            &mixed_coast_mass,
            0.16,
        );
        save_u14_total_density_view(
            output_dir,
            &format!("u14_seed_{seed}_mountain_forward_windward_lee"),
            resolution,
            &paired.full_mass,
            0.16,
        );
        save_u14_total_density_view(
            output_dir,
            &format!("u14_seed_{seed}_mountain_forward_lee_advected_reference"),
            resolution,
            &paired.advected_mass,
            0.16,
        );
        save_u14_total_density_view(
            output_dir,
            &format!("u14_seed_{seed}_mountain_forward_lee_new_phase_increment"),
            resolution,
            &paired_increment,
            0.01,
        );
        save_u14_total_density_view(
            output_dir,
            &format!("u14_seed_{seed}_mountain_reverse_windward_lee"),
            resolution,
            &reverse,
            0.16,
        );
        paired_lee_rows.push(format!(
            "seed={seed} lee_advected_total_p90={:.5} lee_full_total_p90={:.5} lee_retention_ratio={:.5} lee_new_phase_total_p90={:.5} lee_new_phase_fraction={:.5} channel_delta_p90={:?} windward_total_p90={paired_windward_total_p90:.5} reverse_windward_low_p90={paired_reverse_windward:.5} reverse_lee_low_p90={paired_reverse_lee:.5}",
            lee_pair.advected_total_p90, lee_pair.full_total_p90, lee_pair.retention_ratio, lee_pair.new_phase_total_p90, lee_pair.new_phase_fraction, lee_pair.channel_delta_p90,
        ));
        if lee_pair.advected_total_p90 < 0.03
            || lee_pair.retention_ratio < 0.75
            || lee_pair.full_total_p90 >= paired_windward_total_p90
            || lee_pair.new_phase_total_p90 > 0.003
            || lee_pair.new_phase_fraction > 0.05
            || paired_reverse_lee <= paired_reverse_windward
        {
            failures.push(format!("U14 paired lee continuation seed {seed}: {lee_pair:?}, windward_total={paired_windward_total_p90:.3}, reverse windward/lee={paired_reverse_windward:.3}/{paired_reverse_lee:.3}"));
        }
        if seed == SEEDS[0] {
            save_field_views(
                output_dir,
                "u14_seed_7_flat_cool_ocean",
                resolution,
                &cool_ocean,
                &cool_geometry,
            );
            save_field_views(
                output_dir,
                "u14_seed_7_flat_inland",
                resolution,
                &cool_inland,
                &inland_geometry,
            );
            save_field_views(
                output_dir,
                "u14_seed_7_mixed_coast_source_flow",
                resolution,
                &mixed_coast_mass,
                &mixed_coast_geometry,
            );
        }
        for (mass, geometry) in [
            (&calm, &calm_geometry),
            (&whisper, &whisper_geometry),
            (&forward, &forward_geometry),
            (&reverse, &reverse_geometry),
        ] {
            let (occupied, invalid) = u14_geometry_metrics(mass, geometry);
            geometry_occupied_texels += occupied;
            geometry_invalid_texels += invalid;
        }
        let asymmetry = |values: &[f32]| {
            u14_masked_mean(values, 0, &orographic_masks.windward)
                - u14_masked_mean(values, 0, &orographic_masks.lee)
        };
        let calm_asymmetry = asymmetry(&calm);
        let whisper_asymmetry = asymmetry(&whisper);
        let forward_asymmetry = asymmetry(&forward);
        let reverse_asymmetry = asymmetry(&reverse);
        let forward_delta = forward_asymmetry - calm_asymmetry;
        let reverse_delta = reverse_asymmetry - calm_asymmetry;
        let span = forward_asymmetry - reverse_asymmetry;
        let whisper_delta = (whisper_asymmetry - calm_asymmetry).abs();
        let windward_low_p90 = u14_masked_quantile(&forward, 0, &orographic_masks.windward, 0.9);
        let lee_low_p90 = u14_masked_quantile(&forward, 0, &orographic_masks.lee, 0.9);
        orography_rows.push(format!(
            "seed={seed} A_calm={calm_asymmetry:.5} A_whisper={whisper_asymmetry:.5} A_forward={forward_asymmetry:.5} A_reverse={reverse_asymmetry:.5} Df={forward_delta:.5} Dr={reverse_delta:.5} span={span:.5} whisper_delta={whisper_delta:.5} low_windward_p90={windward_low_p90:.5} low_lee_p90={lee_low_p90:.5}",
        ));
        // FE-081 de-classification removed marine_climate conversion inflation;
        // baseline moved 3011d61→FE-081. Gate rebaselined from the multi-seed
        // distribution, not a single-seed floor: median − 2σ over the 8 frozen
        // seeds (median 0.1171, σ_pop 0.0115 → 0.0941). Do not loosen further.
        if calm_asymmetry.abs() > 0.02 || whisper_delta > 0.01 || windward_low_p90 < 0.094 {
            failures.push(format!(
                "U14 source-flow orography seed {seed}: windward_p90={windward_low_p90:.3}, lee_p90={lee_low_p90:.3}, calm={calm_asymmetry:.3}, whisper={whisper_delta:.3}",
            ));
        }
    }
    let generation_stats = compute_runtime_stats(generation_samples_ms);
    // Kept only to avoid changing the established artifact line shape; these retired
    // survival metrics no longer drive validation or source ownership.
    let background_retention_q50_max = 0.0;
    let cool_retention_p90_min = 0.0;
    let trade_retention_p90_min = 0.0;
    let windward_retention_p90_min = 0.0;
    let cool_deep_min = 0.0;
    let feedback = format!(
        "\nfeedback_u14_coverage_threshold=tau>=.01; tau=1.2*(.5*low+1.2*deep+.35*high)\nfeedback_u14_significant_component_area>={:.2}% face\nfeedback_u14_component_growth=.25_frozen_to_.75_significant,max_positive_pixel_overlap,deterministic_lowest_label_tie,all_frozen_overlap,conservative_lower_median_ratio>=1.30,merges_reported\nfeedback_u14_coverage_seed_tuples=\n{}\nsource_flow_scenarios=inland_no_source_exact,marine_to_land_lowland,humid_marine_ridge_windward,lee_drying\ncoast_inland_low_p90={inland_low_p90_min:.3}\ncoast_inland_to_marine_ratio={downwind_land_maritime_p90_ratio_min:.3}\ncoast_inland_component_crosses_shore={coastline_crossing_component}\nwindward_low_p90={windward_low_p90_min:.3}\nlee_full_low_p90={lee_full_low_p90_min:.3}\nlee_advected_low_p90={lee_advected_low_p90_min:.3}\nlee_retention_ratio={lee_retention_ratio_min:.3}\nlee_new_phase_low_p90={lee_new_phase_low_p90_max:.3}\nlee_new_phase_fraction={lee_new_phase_fraction_max:.3}\ninland_no_source_exact={inland_no_source_exact}\npaired_lee_continuations=\n{}\n",
        U14_MINIMUM_COMPONENT_FACE_AREA * 100.0,
        coverage_seed_rows.join("\n"),
        paired_lee_rows.join("\n"),
    );
    let mut values = format!(
        "command=cargo run --release --features validation --bin sweep -- --weather-validation --size 512 --output-dir {output_dir}\nseeds={SEEDS:?}\nmasks={U14_FLAT_COOL_OCEAN_MASK},{U14_FLAT_INLAND_MASK},{U14_MOUNTAIN_WINDWARD_MASK},{U14_MOUNTAIN_LEE_MASK},{U14_COAST_BAND_MASK}\ncool_ocean_inland_min={cool_ratio:.3}\nbackground_retention_q50_max={background_retention_q50_max:.3}\ncool_retention_p90_min={cool_retention_p90_min:.3}\ntrade_retention_p90_min={trade_retention_p90_min:.3}\nwindward_retention_p90_min={windward_retention_p90_min:.3}\ncool_deck_low_p90_min={cool_deck_p90_min:.3}\ntrade_low_p90_min={trade_p90_min:.3}\nwindward_lee_low_p90_delta_min={windward_lee_low_p90_delta_min:.3}\ncool_deep_min={cool_deep_min:.3}\nlow_deep_min={low_deep:.3}\ndeck_thickness=[{deck_min:.3},{deck_max:.3}] km\ntrade_top=[{trade_min:.3},{trade_max:.3}] km\ntrade_clear_gap=[{gaps_min:.3},{gaps_max:.3}]\ncoast_terrain_localization_ratio_max={coast_terrain_localization_ratio_max:.3}\ncoverage_samples=[0,.125,.25,.375,.5,.625,.75,.875,1]\ncoverage_increment_min={coverage_increment_min:?}\ncoverage_increment_max_by_step={coverage_increment_max_by_step:?}\ncoverage_increment_max={coverage_increment_max:.5}\ncoverage_increment_median_min={coverage_increment_median_min:.5}\ncoverage_zero_exact={zero_exact}\ndeterministic={deterministic}\ngeometry_occupied_texels={geometry_occupied_texels}\ngeometry_invalid_texels={geometry_invalid_texels}\nmass_seam_edge_max={mass_seam_edge_max:?}\nmass_seam_edge_p99={mass_seam_edge_p99:?}\nmass_seam_corner_max={mass_seam_corner_max:?}\nmass_seam_corner_p99={mass_seam_corner_p99:?}\ngeometry_seam_edge_max={geometry_seam_edge_max:?}\ngeometry_seam_edge_p99={geometry_seam_edge_p99:?}\ngeometry_seam_corner_max={geometry_seam_corner_max:?}\ngeometry_seam_corner_p99={geometry_seam_corner_p99:?}\nlow_plateau_exact_max={:.5}\ndeep_plateau_exact_max={:.5}\nlow_plateau_near_max={:.5}\ndeep_plateau_near_max={:.5}\nfixture_generation_n={}\nfixture_generation_p95_ms={:.3}\nfixture_generation_p95_gate_ms={WEATHER_GENERATION_QUEUE_STALL_P95_MS:.1}\nfixture_generation_p95_rebaseline_current_samples_ms=[33.185,32.463,32.087]\nfixture_generation_p95_rebaseline_baseline_samples_ms=[30.985,31.837,28.906]\nfixture_generation_p95_rebaseline=FE-062 current_min=32.087ms>baseline_max=31.837ms; gate=36.0ms\n",
        plateau_exact_max[0],
        plateau_exact_max[1],
        plateau_near_max[0],
        plateau_near_max[1],
        generation_stats.count,
        generation_stats.p95_ms,
    );
    values.push_str(&feedback);
    values.push_str(&format!(
        "lee_advected_total_p90={lee_advected_low_p90_min:.3}\nlee_full_total_p90={lee_full_low_p90_min:.3}\nlee_new_phase_total_p90={lee_new_phase_low_p90_max:.3}\n"
    ));
    values.push_str("orography_masks=frozen_projected_world_space\norography_metrics=\n");
    values.push_str(&orography_rows.join("\n"));
    values.push('\n');
    values.push_str("source_flow_land_metrics=\n");
    values.push_str(&source_flow_land_rows.join("\n"));
    values.push('\n');
    let artifact = Path::new(output_dir).join("u14_field_metrics.txt");
    std::fs::write(&artifact, values).expect("write U14 metrics artifact");
    println!("U14 field metrics: {}", artifact.display());
    if cool_ratio < 1.5 {
        failures.push(format!("U14 cool ocean/inland ratio {cool_ratio:.3} < 1.5"));
    }
    // FE-081 de-classification removed marine_climate conversion inflation;
    // baseline moved 3011d61→FE-081. Gate rebaselined from the multi-seed
    // distribution: median − 2σ over the 8 frozen seeds (median 0.0676,
    // σ_pop 0.0058 → 0.0560). Do not loosen further.
    if inland_occupied_min < 0.05
        || inland_low_p90_min < 0.056
        || downwind_land_maritime_p90_ratio_min < 0.35
        || !coastline_crossing_component
    {
        failures.push(format!(
            "U14 source-flow land occupied={inland_occupied_min:.3}, land_p90={inland_low_p90_min:.3}, maritime_p90={maritime_low_p90_min:.3}, ratio={downwind_land_maritime_p90_ratio_min:.3}, coastline_component={coastline_crossing_component}"
        ));
    }
    if !inland_no_source_exact {
        failures.push("U14 inland no-source field was not exact zero".to_string());
    }
    // FE-081 de-classification removed marine_climate conversion inflation;
    // baseline moved 3011d61→FE-081. Gate rebaselined from the multi-seed
    // distribution: median − 2σ over the 8 frozen seeds (median 0.1171,
    // σ_pop 0.0115 → 0.0941). Do not loosen further.
    if cool_deck_p90_min < 0.02 || trade_p90_min < 0.02 || windward_low_p90_min < 0.094 {
        failures.push(format!(
            "U14 frozen feature p90 cool/trade/windward={cool_deck_p90_min:.3}/{trade_p90_min:.3}/{windward_low_p90_min:.3} < 0.02"
        ));
    }
    if windward_lee_low_p90_delta_min < 0.01 {
        failures.push(format!(
            "U14 paired terrain response windward/lee_p90_delta={windward_lee_low_p90_delta_min:.3}"
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
    if coast_terrain_localization_ratio_max > 1.25 {
        failures.push(format!(
            "U14 coast terrain-attributable localization ratio={coast_terrain_localization_ratio_max:.3}"
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
    rows.extend(failures.iter().cloned());
    rows
}

#[derive(Default, Debug, Copy, Clone)]
struct U15CoreMetrics {
    count: usize,
    deep_p95: Option<f32>,
}

#[derive(Debug, Clone)]
struct U15PairedTopMetrics {
    small_top_p95: f32,
    large_top_p95: f32,
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
    // Telemetry only: major/minor eigenvalue ratio of the captured anvil cloud.
    // Near 1 means a blob whose PCA axis is noise; large values mean a real
    // elongated structure whose alignment against the wind is meaningful.
    elongation: Option<f32>,
}

const U15_ELIGIBLE_MASK: &str = "eligible_convective_core";
const U15_DEEP_THRESHOLD: f32 = 0.02;
// Anvil capture zone around each deep core for the per-core organization metric:
// three core radii ≈ twice the fixture convergence influence radius — an anvil-scale
// neighborhood; unlabeled high cloud is assigned to its nearest core.
const U15_ANVIL_CAPTURE_RADIUS: f32 = 3.0 * U15_FIXTURE_CORE_RADIUS;
const U15_MINIMUM_COMPONENT_FACE_AREA: f32 = 0.0025;
const U15_FIXTURE_FLOW: [f32; 3] = [0.5, 0.0, 0.35];
const U15_FIXTURE_CORE_RADIUS: f32 = 0.055;
const U15_SIZE_STORM_COUNT: u32 = 4;
const U15_SIZE_SUPPORT_RADIUS: f32 = 0.34;
const U15_SIZE_FULL_SUPPORT_RADIUS: f32 = 0.290;
const U15_MAX_SATELLITES_PER_OWNER: usize = 2;
const U15_MAX_SATELLITE_AREA_RATIO: f32 = 0.50;
const U15_SIZE_RING_RADIUS: f32 = 0.10;
const U15_SIZE_MIN_PHYSICAL_ELIGIBILITY: f32 = 0.035;
// Frozen from the documented production-center input-only selection.
const U15_SIZE_SEEDS: [u32; 8] = [4, 40, 44, 59, 62, 78, 80, 84];

#[derive(Debug, Clone)]
struct U15ResponseComponent {
    pixels: Vec<usize>,
    centroid: [f32; 3],
    area_fraction: f32,
}

#[derive(Debug, Clone, Copy)]
struct U15OwnerSizeMetrics {
    primary_area: f32,
    primary_top_p95: f32,
    satellite_count: usize,
    satellite_area_ratio: f32,
}

fn u15_smooth_step(edge0: f32, edge1: f32, value: f32) -> f32 {
    let t = ((value - edge0) / (edge1 - edge0)).clamp(0.0, 1.0);
    t * t * (3.0 - 2.0 * t)
}

fn u15_normalize_or_zero(vector: [f32; 3]) -> [f32; 3] {
    u15_normalize(vector).unwrap_or([0.0; 3])
}

fn u15_seed_offset(seed: u32, stream: u32) -> [f32; 3] {
    let mixed = seed ^ stream.wrapping_mul(2_654_435_769);
    let hash = |value: u32| {
        let mut value = value.wrapping_mul(747_796_405).wrapping_add(2_891_336_453);
        value = ((value >> ((value >> 28) + 4)) ^ value).wrapping_mul(277_803_737);
        (value >> 22) ^ value
    };
    [
        (hash(mixed) & 0xffff) as f32 / 655.35,
        (hash(mixed ^ 0x68bc_21eb) & 0xffff) as f32 / 655.35,
        (hash(mixed ^ 0x02e5_be93) & 0xffff) as f32 / 655.35,
    ]
}

fn u15_fixture_center(seed: u32, index: u32) -> [f32; 3] {
    let rank = (index.reverse_bits() >> 29) as f32;
    let z = 1.0 - 2.0 * (rank + 0.5) / 8.0;
    let phase = (seed & 0xffff) as f32 / 65_536.0 * std::f32::consts::TAU;
    let angle = rank * 2.399_963_2 + phase;
    let base = [
        (1.0 - z * z).max(0.0).sqrt() * angle.cos(),
        z,
        (1.0 - z * z).max(0.0).sqrt() * angle.sin(),
    ];
    let reference = if base[1].abs() > 0.9 {
        [1.0, 0.0, 0.0]
    } else {
        [0.0, 1.0, 0.0]
    };
    let east = u15_normalize_or_zero(u15_cross(reference, base));
    let north = u15_cross(base, east);
    let jitter = u15_seed_offset(seed, 201 + index);
    u15_normalize_or_zero([
        base[0]
            + east[0] * (jitter[0] * 2.0 - 1.0) * 0.12
            + north[0] * (jitter[1] * 2.0 - 1.0) * 0.12,
        base[1]
            + east[1] * (jitter[0] * 2.0 - 1.0) * 0.12
            + north[1] * (jitter[1] * 2.0 - 1.0) * 0.12,
        base[2]
            + east[2] * (jitter[0] * 2.0 - 1.0) * 0.12
            + north[2] * (jitter[1] * 2.0 - 1.0) * 0.12,
    ])
}

fn u15_fixture_centers(seed: u32, count: u32) -> Vec<[f32; 3]> {
    (0..count)
        .map(|index| u15_fixture_center(seed, index))
        .collect()
}

fn u15_minimum_center_separation(centers: &[[f32; 3]]) -> f32 {
    let mut minimum = f32::INFINITY;
    for (index, center) in centers.iter().enumerate() {
        for other in &centers[..index] {
            minimum = minimum.min(u15_dot(*center, *other).clamp(-1.0, 1.0).acos());
        }
    }
    minimum
}

fn u15_size_seed_criterion(seed: u32) -> bool {
    let centers = u15_fixture_centers(seed, U15_SIZE_STORM_COUNT);
    u15_minimum_center_separation(&centers) > U15_SIZE_SUPPORT_RADIUS * 2.0
        && centers.iter().all(|center| {
            center[1].clamp(-1.0, 1.0).asin().abs() + U15_SIZE_SUPPORT_RADIUS
                <= std::f32::consts::FRAC_PI_3
        })
}

fn u15_size_frozen_candidates() -> Vec<u32> {
    (1..=10_000)
        .filter(|seed| u15_size_seed_criterion(*seed))
        .take(U15_SIZE_SEEDS.len())
        .collect()
}

fn u15_eligible(seed: u32, pos: [f32; 3]) -> bool {
    (0..8)
        .any(|index| u15_dot(pos, u15_fixture_center(seed, index)) >= U15_FIXTURE_CORE_RADIUS.cos())
}

fn u15_fixture_seed_wind(seed: u32, count: u32, pos: [f32; 3]) -> [f32; 3] {
    let projection = u15_dot(U15_FIXTURE_FLOW, pos);
    let mut wind = [
        U15_FIXTURE_FLOW[0] - pos[0] * projection,
        U15_FIXTURE_FLOW[1] - pos[1] * projection,
        U15_FIXTURE_FLOW[2] - pos[2] * projection,
    ];
    for index in 0..count {
        let center = u15_fixture_center(seed, index);
        let cosine = u15_dot(pos, center).clamp(-1.0, 1.0);
        let distance = cosine.acos();
        let influence = u15_smooth_step(
            U15_FIXTURE_CORE_RADIUS * 2.0,
            U15_FIXTURE_CORE_RADIUS,
            distance,
        );
        let inward = [
            center[0] - pos[0] * cosine,
            center[1] - pos[1] * cosine,
            center[2] - pos[2] * cosine,
        ];
        for axis in 0..3 {
            wind[axis] += inward[axis] * influence * 8.0;
        }
    }
    wind
}

// Environmental steering flow: the base fixture flow plus convergence from every
// seeded center EXCEPT the one nearest `pos` (the core's own inflow). Real anvils are
// steered by the environmental flow around the storm, not by the storm's own low-level
// circulation; sampling wind at the component centroid otherwise folds in the core's
// own ×8 convergence and rotates the "downwind" reference toward the centroid offset
// direction (measured: this deflection alone accounts for every anvil angle outlier).
fn u15_fixture_environmental_wind(seed: u32, count: u32, pos: [f32; 3]) -> [f32; 3] {
    let projection = u15_dot(U15_FIXTURE_FLOW, pos);
    let mut wind = [
        U15_FIXTURE_FLOW[0] - pos[0] * projection,
        U15_FIXTURE_FLOW[1] - pos[1] * projection,
        U15_FIXTURE_FLOW[2] - pos[2] * projection,
    ];
    let mut own_center = usize::MAX;
    let mut own_distance = f32::INFINITY;
    for index in 0..count {
        let center = u15_fixture_center(seed, index);
        let distance = u15_dot(pos, center).clamp(-1.0, 1.0).acos();
        if distance < own_distance {
            own_distance = distance;
            own_center = index as usize;
        }
    }
    for index in 0..count {
        if index as usize == own_center {
            continue;
        }
        let center = u15_fixture_center(seed, index);
        let cosine = u15_dot(pos, center).clamp(-1.0, 1.0);
        let distance = cosine.acos();
        let influence = u15_smooth_step(
            U15_FIXTURE_CORE_RADIUS * 2.0,
            U15_FIXTURE_CORE_RADIUS,
            distance,
        );
        let inward = [
            center[0] - pos[0] * cosine,
            center[1] - pos[1] * cosine,
            center[2] - pos[2] * cosine,
        ];
        for axis in 0..3 {
            wind[axis] += inward[axis] * influence * 8.0;
        }
    }
    wind
}

fn u15_size_fixture_support(distance: f32) -> f32 {
    u15_smooth_step(
        U15_SIZE_SUPPORT_RADIUS,
        U15_SIZE_FULL_SUPPORT_RADIUS,
        distance,
    )
}

fn u15_size_fixture_seed_wind(seed: u32, count: u32, pos: [f32; 3]) -> [f32; 3] {
    let projection = u15_dot(U15_FIXTURE_FLOW, pos);
    let mut wind = [
        U15_FIXTURE_FLOW[0] - pos[0] * projection,
        U15_FIXTURE_FLOW[1] - pos[1] * projection,
        U15_FIXTURE_FLOW[2] - pos[2] * projection,
    ];
    for index in 0..count {
        let center = u15_fixture_center(seed, index);
        let cosine = u15_dot(pos, center).clamp(-1.0, 1.0);
        let inward = [
            center[0] - pos[0] * cosine,
            center[1] - pos[1] * cosine,
            center[2] - pos[2] * cosine,
        ];
        let support = u15_size_fixture_support(cosine.acos());
        for axis in 0..3 {
            wind[axis] += inward[axis] * support * 8.0;
        }
    }
    wind
}

fn u15_fixture_height(seed: u32, count: u32, pos: [f32; 3]) -> f32 {
    let mut height = -0.1;
    for index in 0..count {
        let center = u15_fixture_center(seed, index);
        let cosine = u15_dot(pos, center).clamp(-1.0, 1.0);
        let distance = cosine.acos();
        let influence = u15_smooth_step(U15_FIXTURE_CORE_RADIUS * 2.0, 0.0, distance);
        let flow = u15_normalize_or_zero([
            U15_FIXTURE_FLOW[0] - center[0] * u15_dot(U15_FIXTURE_FLOW, center),
            U15_FIXTURE_FLOW[1] - center[1] * u15_dot(U15_FIXTURE_FLOW, center),
            U15_FIXTURE_FLOW[2] - center[2] * u15_dot(U15_FIXTURE_FLOW, center),
        ]);
        let delta = [
            pos[0] - center[0] * cosine,
            pos[1] - center[1] * cosine,
            pos[2] - center[2] * cosine,
        ];
        height += u15_dot(delta, flow) * influence * 0.5;
    }
    height
}

fn u15_size_fixture_height(seed: u32, count: u32, pos: [f32; 3]) -> f32 {
    let mut height = -0.1;
    for index in 0..count {
        let center = u15_fixture_center(seed, index);
        let cosine = u15_dot(pos, center).clamp(-1.0, 1.0);
        let flow = u15_normalize_or_zero([
            U15_FIXTURE_FLOW[0] - center[0] * u15_dot(U15_FIXTURE_FLOW, center),
            U15_FIXTURE_FLOW[1] - center[1] * u15_dot(U15_FIXTURE_FLOW, center),
            U15_FIXTURE_FLOW[2] - center[2] * u15_dot(U15_FIXTURE_FLOW, center),
        ]);
        let delta = [
            pos[0] - center[0] * cosine,
            pos[1] - center[1] * cosine,
            pos[2] - center[2] * cosine,
        ];
        height += u15_dot(delta, flow) * u15_size_fixture_support(cosine.acos()) * 0.5;
    }
    height
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
    seed: Option<u32>,
) -> Vec<U15ResponseComponent> {
    let face_pixels = (resolution * resolution) as usize;
    let minimum_area = (face_pixels as f32 * U15_MINIMUM_COMPONENT_FACE_AREA).ceil() as usize;
    let mut visited = vec![false; face_pixels * 6];
    let mut components = Vec::new();
    for start in 0..visited.len() {
        let position = u15_pixel_position(start, resolution);
        if visited[start]
            || seed.is_some_and(|seed| !u15_eligible(seed, position))
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
                    && seed.is_none_or(|seed| u15_eligible(seed, position))
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

fn u15_size_association(resolution: u32, centers: &[[f32; 3]]) -> Vec<Option<u32>> {
    (0..resolution as usize * resolution as usize * 6)
        .map(|pixel| {
            let position = u15_pixel_position(pixel, resolution);
            let center = centers
                .iter()
                .enumerate()
                .max_by(|(_, left), (_, right)| {
                    u15_dot(position, **left).total_cmp(&u15_dot(position, **right))
                })
                .map(|(center, _)| center)?;
            (u15_dot(position, centers[center]) >= U15_SIZE_SUPPORT_RADIUS.cos())
                .then_some(center as u32)
        })
        .collect()
}

fn u15_significant_size_components(
    response: &[f32],
    resolution: u32,
    association: &[Option<u32>],
) -> Vec<U15ResponseComponent> {
    let face_pixels = (resolution * resolution) as usize;
    let minimum_area = (face_pixels as f32 * U15_MINIMUM_COMPONENT_FACE_AREA).ceil() as usize;
    let mut visited = vec![false; face_pixels * 6];
    let mut components = Vec::new();
    for start in 0..visited.len() {
        if visited[start]
            || association[start].is_none()
            || response[start * 4 + 1] < U15_DEEP_THRESHOLD
        {
            continue;
        }
        let center = association[start];
        let mut stack = vec![start];
        let mut pixels = Vec::new();
        visited[start] = true;
        while let Some(pixel) = stack.pop() {
            pixels.push(pixel);
            for neighbor in u15_pixel_neighbors(pixel, resolution) {
                if !visited[neighbor]
                    && association[neighbor] == center
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

fn u15_significant_cores(components: &[U15ResponseComponent], response: &[f32]) -> U15CoreMetrics {
    let mut deep_values = Vec::new();
    for component in components {
        for &pixel in &component.pixels {
            deep_values.push(response[pixel * 4 + 1]);
        }
    }
    U15CoreMetrics {
        count: components.len(),
        deep_p95: u15_percentile(&mut deep_values),
    }
}

fn u15_component_top_p95(geometry: &[f32], component: &U15ResponseComponent) -> Option<f32> {
    let mut tops: Vec<_> = component
        .pixels
        .iter()
        .map(|&pixel| geometry[pixel * 4 + 2])
        .collect();
    u15_percentile(&mut tops)
}

fn u15_size_component_owner(
    component: &U15ResponseComponent,
    association: &[Option<u32>],
) -> Option<u32> {
    component
        .pixels
        .iter()
        .try_fold(None, |owner, &pixel| {
            let pixel_owner = association.get(pixel).copied().flatten()?;
            match owner {
                Some(owner) if owner != pixel_owner => None,
                _ => Some(Some(pixel_owner)),
            }
        })
        .flatten()
}

fn u15_owner_size_metrics(
    components: &[U15ResponseComponent],
    geometry: &[f32],
    association: &[Option<u32>],
) -> Vec<Option<U15OwnerSizeMetrics>> {
    (0..U15_SIZE_STORM_COUNT)
        .map(|owner| {
            let owned: Vec<_> = components
                .iter()
                .filter(|component| u15_size_component_owner(component, association) == Some(owner))
                .collect();
            let primary_index = owned
                .iter()
                .enumerate()
                .max_by(|(_, left), (_, right)| left.area_fraction.total_cmp(&right.area_fraction))?
                .0;
            let primary = owned[primary_index];
            let satellite_area = owned
                .iter()
                .enumerate()
                .filter(|(index, _)| *index != primary_index)
                .map(|(_, component)| component.area_fraction)
                .sum::<f32>();
            Some(U15OwnerSizeMetrics {
                primary_area: primary.area_fraction,
                primary_top_p95: u15_component_top_p95(geometry, primary)?,
                satellite_count: owned.len() - 1,
                satellite_area_ratio: satellite_area / primary.area_fraction.max(f32::EPSILON),
            })
        })
        .collect()
}

fn u15_paired_size_tops(
    small_owners: &[Option<U15OwnerSizeMetrics>],
    large_owners: &[Option<U15OwnerSizeMetrics>],
) -> Option<Vec<U15PairedTopMetrics>> {
    let mut pairs = Vec::with_capacity(U15_SIZE_STORM_COUNT as usize);
    for center in 0..U15_SIZE_STORM_COUNT {
        let small = small_owners.get(center as usize).copied().flatten()?;
        let large = large_owners.get(center as usize).copied().flatten()?;
        pairs.push(U15PairedTopMetrics {
            small_top_p95: small.primary_top_p95,
            large_top_p95: large.primary_top_p95,
        });
    }
    Some(pairs)
}

fn u15_primary_area(owners: &[Option<U15OwnerSizeMetrics>]) -> Option<f32> {
    let mut total = 0.0;
    for owner in owners {
        total += owner.as_ref()?.primary_area;
    }
    Some(total)
}

fn u15_fragmentation_within_bound(owners: &[Option<U15OwnerSizeMetrics>]) -> bool {
    owners.iter().flatten().all(|owner| {
        owner.satellite_count <= U15_MAX_SATELLITES_PER_OWNER
            && owner.satellite_area_ratio <= U15_MAX_SATELLITE_AREA_RATIO
    })
}

fn u15_size_owner_report(owners: &[Option<U15OwnerSizeMetrics>]) -> String {
    owners
        .iter()
        .enumerate()
        .map(|(owner, metrics)| match metrics {
            Some(metrics) => format!(
                "owner={owner}:area={:.5},top={:.5},satellites={},satellite_area_ratio={:.5}",
                metrics.primary_area,
                metrics.primary_top_p95,
                metrics.satellite_count,
                metrics.satellite_area_ratio,
            ),
            None => format!("owner={owner}:missing"),
        })
        .collect::<Vec<_>>()
        .join(";")
}

fn u15_missing_size_owners(owners: &[Option<U15OwnerSizeMetrics>]) -> Vec<usize> {
    owners
        .iter()
        .enumerate()
        .filter_map(|(owner, metrics)| metrics.is_none().then_some(owner))
        .collect()
}

fn u15_size_top_deltas(
    baseline: &[Option<U15OwnerSizeMetrics>],
    endpoint: &[Option<U15OwnerSizeMetrics>],
) -> Vec<Option<f32>> {
    baseline
        .iter()
        .zip(endpoint)
        .map(|(baseline, endpoint)| {
            Some(endpoint.as_ref()?.primary_top_p95 - baseline.as_ref()?.primary_top_p95)
        })
        .collect()
}

fn u15_size_weather_snapshot(resolution: u32, seed: u32, storm_size: f32) -> WeatherSnapshot {
    WeatherSnapshot {
        face: 0,
        resolution,
        seed,
        storm_count: U15_SIZE_STORM_COUNT,
        coverage: 1.0,
        moisture: 1.0,
        surface_pressure_bar: 0.8,
        base_temp_c: 12.0,
        ocean_level: 0.0,
        axial_tilt_rad: 0.0,
        season: 0.5,
        storm_size,
        radius_km: 6371.0,
        rotation_rate_rad_s: std::f32::consts::TAU / 86400.0,
        wind_scale: 1.0,
    }
}

#[derive(Debug)]
struct U15SizePrecondition {
    marine_fraction: f32,
    fixture_support: f32,
    initial_vapor_ratio: f32,
    warm_gate: f32,
    physical_eligibility: f32,
}

fn u15_size_tangent_basis(pos: [f32; 3]) -> ([f32; 3], [f32; 3]) {
    let reference = if pos[1].abs() > 0.9 {
        [1.0, 0.0, 0.0]
    } else {
        [0.0, 1.0, 0.0]
    };
    let east = u15_normalize_or_zero(u15_cross(reference, pos));
    (east, u15_normalize_or_zero(u15_cross(pos, east)))
}

fn u15_size_ring_position(center: [f32; 3], angle: f32) -> [f32; 3] {
    let (east, north) = u15_size_tangent_basis(center);
    let (sin, cos) = U15_SIZE_RING_RADIUS.sin_cos();
    u15_normalize_or_zero([
        center[0] * cos + (east[0] * angle.cos() + north[0] * angle.sin()) * sin,
        center[1] * cos + (east[1] * angle.cos() + north[1] * angle.sin()) * sin,
        center[2] * cos + (east[2] * angle.cos() + north[2] * angle.sin()) * sin,
    ])
}

fn u15_size_precondition(seed: u32, center: [f32; 3], pos: [f32; 3]) -> U15SizePrecondition {
    let snapshot = u15_size_weather_snapshot(128, seed, 0.3);
    let pressure_factor = u15_smooth_step(0.05, 0.3, snapshot.surface_pressure_bar);
    let local_pressure = (810.0_f32 / 1013.0).clamp(0.8, 1.2);
    let fixture_support = u15_size_fixture_support(u15_dot(pos, center).clamp(-1.0, 1.0).acos());
    let marine_fraction = 1.0 - u15_smooth_step(0.15, 0.85, 0.0);
    let temperature = |point: [f32; 3]| {
        let latitude = point[1].clamp(-1.0, 1.0).asin().abs() / std::f32::consts::FRAC_PI_2;
        let elevation = (u15_size_fixture_height(seed, U15_SIZE_STORM_COUNT, point)
            - snapshot.ocean_level)
            .max(0.0)
            * 5.0;
        snapshot.base_temp_c - latitude * 35.0 - elevation * 6.5
    };
    let thermal = u15_smooth_step(-25.0, 30.0, temperature(pos));
    let surface_supply = 0.60;
    let supply = (surface_supply + thermal * 0.04).clamp(0.0, 1.0);
    let initial_vapor = supply * pressure_factor * 0.36;
    let q_sat = (0.16 + (0.68 - 0.16) * thermal) * pressure_factor * local_pressure;
    let (east, north) = u15_size_tangent_basis(pos);
    let diagnostic_step = (std::f32::consts::FRAC_PI_2 / 128.0 * 1.5).max(0.01);
    let sample_wind = |point| u15_size_fixture_seed_wind(seed, U15_SIZE_STORM_COUNT, point);
    let divergence = (u15_dot(
        u15_sub(
            sample_wind(u15_normalize_or_zero(u15_add(
                pos,
                u15_scale(east, diagnostic_step),
            ))),
            sample_wind(u15_normalize_or_zero(u15_sub(
                pos,
                u15_scale(east, diagnostic_step),
            ))),
        ),
        east,
    ) + u15_dot(
        u15_sub(
            sample_wind(u15_normalize_or_zero(u15_add(
                pos,
                u15_scale(north, diagnostic_step),
            ))),
            sample_wind(u15_normalize_or_zero(u15_sub(
                pos,
                u15_scale(north, diagnostic_step),
            ))),
        ),
        north,
    )) / (2.0 * diagnostic_step);
    let convergence = u15_smooth_step(0.01, 0.3, -divergence * 0.2);
    let wind = sample_wind(pos);
    let speed = u15_dot(wind, wind).sqrt();
    let wind_dir = u15_normalize_or_zero(wind);
    let terrain_lookahead = 1.5 * (300.0_f32 / snapshot.radius_km).clamp(0.02, 0.08);
    let terrain_response = (u15_size_fixture_height(
        seed,
        U15_SIZE_STORM_COUNT,
        u15_normalize_or_zero(u15_add(pos, u15_scale(wind_dir, terrain_lookahead))),
    ) - u15_size_fixture_height(
        seed,
        U15_SIZE_STORM_COUNT,
        u15_normalize_or_zero(u15_sub(pos, u15_scale(wind_dir, terrain_lookahead))),
    )) * u15_smooth_step(0.03, 0.20, speed);
    let terrain_lift = u15_smooth_step(0.005, 0.08, terrain_response);
    let rain_shadow = u15_smooth_step(0.005, 0.08, -terrain_response);
    let lcl_lift = (convergence * 0.70 + terrain_lift * 1.75 + 0.04 + (1.0 - thermal) * 0.16
        - rain_shadow * 0.14)
        .clamp(0.0, 1.0);
    let warm_gate = thermal * u15_smooth_step(0.10, 0.20, lcl_lift);
    let humidity = u15_smooth_step(0.45, 0.95, initial_vapor / q_sat.max(0.0001));
    U15SizePrecondition {
        marine_fraction,
        fixture_support,
        initial_vapor_ratio: initial_vapor / q_sat.max(0.0001),
        warm_gate,
        physical_eligibility: warm_gate * u15_smooth_step(0.12, 0.75, lcl_lift) * humidity,
    }
}

fn u15_size_precondition_diagnostics() -> Vec<String> {
    let mut failures = Vec::new();
    for &seed in &U15_SIZE_SEEDS {
        for (owner, center) in u15_fixture_centers(seed, U15_SIZE_STORM_COUNT)
            .into_iter()
            .enumerate()
        {
            let positions = std::iter::once(("center", center)).chain((0..4).map(|ring| {
                (
                    "ring",
                    u15_size_ring_position(center, ring as f32 * std::f32::consts::FRAC_PI_2),
                )
            }));
            for (location, pos) in positions {
                let check = u15_size_precondition(seed, center, pos);
                if check.marine_fraction < 1.0
                    || check.fixture_support < 1.0
                    || check.initial_vapor_ratio < 0.50
                    || check.warm_gate < 0.25
                    || check.physical_eligibility < U15_SIZE_MIN_PHYSICAL_ELIGIBILITY
                {
                    failures.push(format!(
                        "seed={seed} owner={owner} {location}: marine={:.5} support={:.5} vapor/q_sat={:.5} warm={:.5} physical={:.5}",
                        check.marine_fraction,
                        check.fixture_support,
                        check.initial_vapor_ratio,
                        check.warm_gate,
                        check.physical_eligibility,
                    ));
                }
            }
        }
    }
    failures
}

fn u15_size_minimum_physical_eligibility(seed: u32) -> f32 {
    u15_fixture_centers(seed, U15_SIZE_STORM_COUNT)
        .into_iter()
        .flat_map(|center| {
            std::iter::once((center, center)).chain((0..4).map(move |ring| {
                (
                    center,
                    u15_size_ring_position(center, ring as f32 * std::f32::consts::FRAC_PI_2),
                )
            }))
        })
        .map(|(center, position)| {
            u15_size_precondition(seed, center, position).physical_eligibility
        })
        .fold(f32::INFINITY, f32::min)
}

fn u15_assert_size_preconditions() {
    assert_eq!(
        u15_size_frozen_candidates().as_slice(),
        U15_SIZE_SEEDS.as_slice(),
        "U15 Size frozen seeds are not the first eight ascending input-only candidates"
    );
    let diagnostics = u15_size_precondition_diagnostics();
    if !diagnostics.is_empty() {
        eprintln!(
            "U15 Size fixture precondition diagnostics:\n{}",
            diagnostics.join("\n")
        );
    }
    assert!(
        diagnostics.is_empty(),
        "U15 Size fixture climate preconditions failed for {} sample(s)",
        diagnostics.len(),
    );
    eprintln!(
        "U15 Size frozen seeds/min eligibility: {}",
        U15_SIZE_SEEDS
            .iter()
            .map(|seed| format!("{seed}={:.5}", u15_size_minimum_physical_eligibility(*seed)))
            .collect::<Vec<_>>()
            .join(", ")
    );
}

fn u15_mask_deep_mass(mass: &[f32], resolution: u32, seed: u32) -> Option<f32> {
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
                if u15_eligible(seed, pos) {
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

fn u15_provenance_total(provenance: &[f32], resolution: u32) -> f32 {
    provenance
        .iter()
        .enumerate()
        .map(|(pixel, value)| {
            let local = pixel % (resolution as usize * resolution as usize);
            value
                * u15_weight(
                    (local % resolution as usize) as u32,
                    (local / resolution as usize) as u32,
                    resolution,
                )
        })
        .sum()
}

/// Area-weighted source-owned share ΣP/ΣT over an owner's primary storm
/// component. Ocean sources mint at 1.0 and land ET at 0.3, so this is
/// ocean-dominant provenance rather than marine-origin evidence.
/// The U15 owner-growth gate only needs nonzero tracked source ownership, not
/// a full per-texel source decomposition.
const U15_MIN_OWNER_PROVENANCE_SHARE: f32 = 0.01;

fn u15_owner_primary_provenance_share(
    provenance: &[f32],
    total_water: &[f32],
    resolution: u32,
    components: &[U15ResponseComponent],
    association: &[Option<u32>],
    owner: u32,
) -> Option<f32> {
    let primary = components
        .iter()
        .filter(|component| u15_size_component_owner(component, association) == Some(owner))
        .max_by(|left, right| left.area_fraction.total_cmp(&right.area_fraction))?;
    // Provenance is written on the spin-up grid (min(resolution, 128) per face)
    // while components live on the validation grid (`resolution`). Nearest-neighbor
    // map each component pixel onto the provenance grid before indexing;
    // ΣP/ΣT keeps area-weighted semantics because both sides use the same
    // validation-resolution solid-angle weights.
    let spin_face_pixels = provenance.len() / 6;
    let spin_resolution_f = (spin_face_pixels as f64).sqrt();
    if resolution == 0
        || provenance.len() % 6 != 0
        || spin_face_pixels == 0
        || spin_resolution_f.fract() != 0.0
        || total_water.len() < resolution as usize * resolution as usize * 6 * 4
    {
        return None;
    }
    let face_pixels = resolution as usize * resolution as usize;
    let spin_resolution = spin_resolution_f as usize;
    let mut owned = 0.0f32;
    let mut total = 0.0f32;
    for &pixel in &primary.pixels {
        let face = pixel / face_pixels;
        let local = pixel % face_pixels;
        let vx = local % resolution as usize;
        let vy = local / resolution as usize;
        let sx = vx * spin_resolution / resolution as usize;
        let sy = vy * spin_resolution / resolution as usize;
        let weight = u15_weight(vx as u32, vy as u32, resolution);
        owned += provenance
            .get(face * spin_face_pixels + sy * spin_resolution + sx)
            .copied()
            .unwrap_or(0.0)
            * weight;
        total += total_water
            .get(pixel * 4..pixel * 4 + 4)
            .map(|channels| channels.iter().sum::<f32>())
            .unwrap_or(0.0)
            * weight;
    }
    Some(if total > 0.0 { owned / total } else { 0.0 })
}

fn u15_deep_mass_metrics(mass: &[f32]) -> (f32, f32) {
    mass.chunks_exact(4)
        .map(|channels| channels[1])
        .fold((0.0, 0.0), |(total, maximum), deep| {
            (total + deep, maximum.max(deep))
        })
}

fn u15_mass_edge_energy(mass: &[f32], resolution: u32) -> f32 {
    (0..mass.len() / 4)
        .flat_map(|pixel| {
            u15_pixel_neighbors(pixel, resolution)
                .into_iter()
                .filter(move |neighbor| pixel < *neighbor)
                .map(move |neighbor| {
                    let value = mass[pixel * 4] + mass[pixel * 4 + 1] + mass[pixel * 4 + 2];
                    let adjacent =
                        mass[neighbor * 4] + mass[neighbor * 4 + 1] + mass[neighbor * 4 + 2];
                    (value - adjacent).abs()
                })
        })
        .sum()
}

const U15_TRAIL_SOURCE: [f32; 3] = [0.522_687_26, 0.0, 0.852_524_5];
const U15_TRAIL_EAST: [f32; 3] = [0.852_524_5, 0.0, -0.522_687_26];
const U15_TRAIL_NORTH: [f32; 3] = [0.0, 1.0, 0.0];
const U15_TRAIL_Q: f32 = 0.02;
const U15_TRAIL_SOURCE_RADIUS: f32 = 0.10;
const U15_TRAIL_SHORE_RADIUS: f32 = 0.14;
const U15_TRAIL_MAX_TRANSPORT_WIND2: f32 = 0.401_820_75;
const U15_TRAIL_SHAPE_ROI_RADIUS: f32 =
    (U15_TRAIL_SHORE_RADIUS + U15_TRAIL_MAX_TRANSPORT_WIND2) / 0.80;
const U15_TRAIL_OUTER_SUPPORT_RADIUS: f32 = U15_TRAIL_SHORE_RADIUS + U15_TRAIL_MAX_TRANSPORT_WIND2;
const U15_TRAIL_BOUNDARY_SHELL_INNER_RADIUS: f32 = 0.652_732;
#[cfg(test)]
const U15_TRAIL_SHEAR_CANDIDATES: [f32; 4] = [0.02, 0.03, 0.04, 0.10];
const U15_TRAIL_SHEAR: f32 = 0.10;

fn u15_geodesic_offset(position: [f32; 3], tangent: [f32; 3], angle: f32) -> [f32; 3] {
    u15_normalize_or_zero(u15_add(
        u15_scale(position, angle.cos()),
        u15_scale(tangent, angle.sin()),
    ))
}

fn u15_trail_wind(pos: [f32; 3], shear_scale: f32) -> [f32; 3] {
    let shear = (1.0 + shear_scale * (pos[1] / 0.08).tanh()) / (1.0 + shear_scale);
    u15_scale(u15_cross(U15_TRAIL_NORTH, pos), shear)
}

fn u15_trail_coordinates(pos: [f32; 3]) -> (f32, f32, f32) {
    let cosine = u15_dot(U15_TRAIL_SOURCE, pos).clamp(-1.0, 1.0);
    let distance = cosine.acos();
    let tangent = u15_normalize_or_zero(u15_sub(pos, u15_scale(U15_TRAIL_SOURCE, cosine)));
    (
        distance,
        u15_dot(tangent, U15_TRAIL_EAST) * distance,
        u15_dot(tangent, U15_TRAIL_NORTH) * distance,
    )
}

fn u15_trail_source_core(pos: [f32; 3]) -> bool {
    u15_trail_height(pos) < 0.0
}

fn u15_source_component(response: &[f32], resolution: u32) -> (Vec<bool>, usize, bool) {
    let mut component = vec![false; response.len()];
    let mut stack = Vec::new();
    for (pixel, occupied) in component.iter_mut().enumerate() {
        if u15_trail_source_core(u15_pixel_position(pixel, resolution))
            && response[pixel] >= U15_TRAIL_Q
        {
            *occupied = true;
            stack.push(pixel);
        }
    }
    let source_pixels = stack.len();
    while let Some(pixel) = stack.pop() {
        for neighbor in u15_pixel_neighbors(pixel, resolution) {
            if !component[neighbor] && response[neighbor] >= U15_TRAIL_Q {
                component[neighbor] = true;
                stack.push(neighbor);
            }
        }
    }
    (component, source_pixels, source_pixels > 0)
}

fn u15_trail_fragmentation(
    response: &[f32],
    source_component: &[bool],
    resolution: u32,
) -> (usize, usize) {
    let mut visited = vec![false; response.len()];
    let mut components = 0;
    let mut detached = 0;
    for start in 0..response.len() {
        if visited[start]
            || response[start] < U15_TRAIL_Q
            || !u15_trail_shape_corridor(u15_pixel_position(start, resolution))
        {
            continue;
        }
        components += 1;
        let mut touches_source = false;
        let mut stack = vec![start];
        visited[start] = true;
        while let Some(pixel) = stack.pop() {
            touches_source |= source_component[pixel];
            for neighbor in u15_pixel_neighbors(pixel, resolution) {
                if !visited[neighbor]
                    && response[neighbor] >= U15_TRAIL_Q
                    && u15_trail_shape_corridor(u15_pixel_position(neighbor, resolution))
                {
                    visited[neighbor] = true;
                    stack.push(neighbor);
                }
            }
        }
        if !touches_source {
            detached += 1;
        }
    }
    (components, detached)
}

#[derive(Debug, Clone, Default)]
struct U15PlumeMetrics {
    source_connected: bool,
    response_p95: f32,
    effective_n: f32,
    alongwind_span: f32,
    crosswind_span: f32,
    axis_wind_degrees: f32,
    sharpness: f32,
    outer_support_response_fraction: f32,
    boundary_shell_response_fraction: f32,
    outside_corridor_response_fraction: f32,
    area_telemetry: f32,
    isotropic_edge_telemetry: f32,
    centroid_texels: Option<f32>,
    // U15 triage 2026-09-05 (user-approved gate surgery): signed alongwind
    // offset of the shape-ROI centroid relative to the source (positive =
    // downwind), and the share of shape-ROI mass upwind of the source. Used by
    // the weak-wind downwind-organization check that replaces the PCA axis
    // gate for the diffusion-limited ws=1 plume regime.
    downwind_centroid_texels: Option<f32>,
    upwind_response_fraction: f32,
    response_mass: f32,
    source_response_mass: f32,
    component_response_mass: f32,
    detached_response_mass: f32,
    detached_component_response_mass: f32,
    component_count: usize,
    detached_component_count: usize,
}

#[derive(Debug, Clone, Default)]
struct U15SeedDiagnostics {
    normalized_high_extent: f32,
    high_deep_centroid_displacement_texels: Option<f32>,
    high_deep_centroid_angle_degrees: Option<f32>,
    downwind_near_decay: f32,
    downwind_mid_decay: f32,
    downwind_far_decay: f32,
    cross_along_ratio: f32,
    edge_variation: f32,
    face_local_column_mass_delta: f32,
}

fn u15_weighted_channel_centroid(
    mass: &[f32],
    resolution: u32,
    channel: usize,
) -> Option<[f32; 3]> {
    let sum = mass
        .chunks_exact(4)
        .enumerate()
        .fold([0.0; 3], |sum, (pixel, values)| {
            let local = pixel % (resolution as usize * resolution as usize);
            let weight = u15_weight(
                (local % resolution as usize) as u32,
                (local / resolution as usize) as u32,
                resolution,
            );
            u15_add(
                sum,
                u15_scale(
                    u15_pixel_position(pixel, resolution),
                    values[channel] * weight,
                ),
            )
        });
    u15_normalize(sum)
}

fn u15_downwind_decay(response: &[f32], resolution: u32) -> [f32; 3] {
    let mut sums = [0.0; 3];
    let mut weights = [0.0; 3];
    for (pixel, value) in response.iter().enumerate() {
        let (distance, zonal, _) = u15_trail_coordinates(u15_pixel_position(pixel, resolution));
        if zonal <= 0.0
            || distance < U15_TRAIL_SHORE_RADIUS
            || distance > U15_TRAIL_SHAPE_ROI_RADIUS
        {
            continue;
        }
        let fraction = (distance - U15_TRAIL_SHORE_RADIUS)
            / (U15_TRAIL_SHAPE_ROI_RADIUS - U15_TRAIL_SHORE_RADIUS);
        let bin = (fraction * 3.0).floor().min(2.0) as usize;
        let local = pixel % (resolution as usize * resolution as usize);
        let weight = u15_weight(
            (local % resolution as usize) as u32,
            (local / resolution as usize) as u32,
            resolution,
        );
        sums[bin] += value * weight;
        weights[bin] += weight;
    }
    std::array::from_fn(|index| sums[index] / weights[index].max(f32::EPSILON))
}

fn u15_face_local_column_mass_delta(baseline: &[f32], post: &[f32], resolution: u32) -> f32 {
    let face_pixels = resolution as usize * resolution as usize;
    (0..6)
        .map(|face| {
            let range = face * face_pixels..(face + 1) * face_pixels;
            let (baseline_total, post_total) = range.fold((0.0, 0.0), |totals, pixel| {
                let local = pixel % face_pixels;
                let weight = u15_weight(
                    (local % resolution as usize) as u32,
                    (local / resolution as usize) as u32,
                    resolution,
                );
                (
                    totals.0 + baseline[pixel * 4..pixel * 4 + 3].iter().sum::<f32>() * weight,
                    totals.1 + post[pixel * 4..pixel * 4 + 3].iter().sum::<f32>() * weight,
                )
            });
            (post_total - baseline_total).abs() / baseline_total.abs().max(f32::EPSILON)
        })
        .fold(0.0, f32::max)
}

fn u15_seed_diagnostics(
    baseline_mass: &[f32],
    post_mass: &[f32],
    plume_response: &[f32],
    resolution: u32,
    plume: &U15PlumeMetrics,
) -> U15SeedDiagnostics {
    let high_centroid = u15_weighted_channel_centroid(post_mass, resolution, 2);
    let deep_centroid = u15_weighted_channel_centroid(post_mass, resolution, 1);
    let centroid_angle = high_centroid
        .zip(deep_centroid)
        .map(|(high, deep)| u15_dot(high, deep).clamp(-1.0, 1.0).acos());
    let decay = u15_downwind_decay(plume_response, resolution);
    U15SeedDiagnostics {
        normalized_high_extent: post_mass
            .chunks_exact(4)
            .filter(|channels| channels[2] >= U15_DEEP_THRESHOLD)
            .count() as f32
            / (post_mass.len() / 4) as f32,
        high_deep_centroid_displacement_texels: centroid_angle
            .map(|angle| angle / (std::f32::consts::FRAC_PI_2 / resolution as f32)),
        high_deep_centroid_angle_degrees: centroid_angle.map(f32::to_degrees),
        downwind_near_decay: decay[0],
        downwind_mid_decay: decay[1],
        downwind_far_decay: decay[2],
        cross_along_ratio: plume.crosswind_span / plume.alongwind_span.max(f32::EPSILON),
        edge_variation: plume.isotropic_edge_telemetry / plume.response_mass.max(f32::EPSILON),
        face_local_column_mass_delta: u15_face_local_column_mass_delta(
            baseline_mass,
            post_mass,
            resolution,
        ),
    }
}

fn u15_seed_diagnostics_report(seed: u32, diagnostics: &U15SeedDiagnostics) -> String {
    format!(
        "seed={seed}\nnormalized_high_extent={:.8}\nhigh_deep_centroid_displacement_texels={:?}\nhigh_deep_centroid_angle_degrees={:?}\ndownwind_decay_near={:.8}\ndownwind_decay_mid={:.8}\ndownwind_decay_far={:.8}\ncross_along_ratio={:.8}\nedge_variation={:.8}\nface_local_column_mass_delta={:.8}\n",
        diagnostics.normalized_high_extent,
        diagnostics.high_deep_centroid_displacement_texels,
        diagnostics.high_deep_centroid_angle_degrees,
        diagnostics.downwind_near_decay,
        diagnostics.downwind_mid_decay,
        diagnostics.downwind_far_decay,
        diagnostics.cross_along_ratio,
        diagnostics.edge_variation,
        diagnostics.face_local_column_mass_delta,
    )
}

fn u15_weighted_quantile(values: &mut [(f32, f32)], quantile: f32) -> Option<f32> {
    values.sort_by(|left, right| left.0.total_cmp(&right.0));
    let total = values.iter().map(|(_, weight)| weight).sum::<f32>();
    let target = total * quantile;
    let mut cumulative = 0.0;
    values.iter().find_map(|(value, weight)| {
        cumulative += weight;
        (cumulative >= target).then_some(*value)
    })
}

fn u15_trail_shape_corridor(pos: [f32; 3]) -> bool {
    u15_trail_coordinates(pos).0 <= U15_TRAIL_SHAPE_ROI_RADIUS
}

fn u15_plume_metrics(response: &[f32], resolution: u32) -> U15PlumeMetrics {
    let mut metrics = U15PlumeMetrics::default();
    let mut shape_roi = Vec::new();
    let (component, _, source_connected) = u15_source_component(response, resolution);
    metrics.source_connected = source_connected;
    (metrics.component_count, metrics.detached_component_count) =
        u15_trail_fragmentation(response, &component, resolution);
    for pixel in 0..response.len() {
        let pos = u15_pixel_position(pixel, resolution);
        let (distance, zonal, meridional) = u15_trail_coordinates(pos);
        let local = pixel % (resolution as usize * resolution as usize);
        let weight = u15_weight(
            (local % resolution as usize) as u32,
            (local / resolution as usize) as u32,
            resolution,
        );
        let value = response[pixel];
        metrics.response_mass += value * weight;
        if !component[pixel] {
            metrics.detached_response_mass += value * weight;
            if value >= U15_TRAIL_Q {
                metrics.detached_component_response_mass += value * weight;
            }
        }
        if component[pixel] {
            metrics.component_response_mass += value * weight;
        }
        if distance <= U15_TRAIL_SOURCE_RADIUS {
            metrics.source_response_mass += value * weight;
        }
        if value >= U15_TRAIL_Q && u15_trail_shape_corridor(pos) {
            shape_roi.push((pos, zonal, meridional, value, weight, distance));
        }
    }
    let shape_response = shape_roi;
    let mut p95: Vec<_> = shape_response
        .iter()
        .map(|(_, _, _, response, weight, _)| (*response, *weight))
        .collect();
    metrics.response_p95 = u15_weighted_quantile(&mut p95, 0.95).unwrap_or(0.0);
    let response_weight = shape_response
        .iter()
        .map(|(_, _, _, response, weight, _)| response * weight)
        .sum::<f32>();
    let response_weight_sq = shape_response
        .iter()
        .map(|(_, _, _, response, weight, _)| (response * weight).powi(2))
        .sum::<f32>();
    metrics.effective_n = response_weight.powi(2) / response_weight_sq.max(f32::EPSILON);
    if response_weight <= f32::EPSILON {
        return metrics;
    }
    let high_response_weight = response
        .iter()
        .enumerate()
        .filter(|(_, value)| **value >= U15_TRAIL_Q)
        .map(|(pixel, value)| {
            let local = pixel % (resolution as usize * resolution as usize);
            *value
                * u15_weight(
                    (local % resolution as usize) as u32,
                    (local / resolution as usize) as u32,
                    resolution,
                )
        })
        .sum::<f32>();
    metrics.outside_corridor_response_fraction = if high_response_weight > 0.0 {
        (high_response_weight - response_weight) / high_response_weight
    } else {
        0.0
    };
    let centroid_sum = shape_response
        .iter()
        .fold([0.0; 3], |sum, (pos, _, _, response, weight, _)| {
            u15_add(sum, u15_scale(*pos, response * weight))
        });
    let Some(centroid) = u15_normalize(centroid_sum) else {
        return metrics;
    };
    metrics.centroid_texels = Some(
        u15_dot(centroid, U15_TRAIL_SOURCE).clamp(-1.0, 1.0).acos()
            / (std::f32::consts::FRAC_PI_2 / resolution as f32),
    );
    let downwind_centroid = shape_response
        .iter()
        .fold(0.0, |sum, (_, zonal, _, response, weight, _)| {
            sum + zonal * response * weight
        })
        / response_weight;
    metrics.downwind_centroid_texels =
        Some(downwind_centroid / (std::f32::consts::FRAC_PI_2 / resolution as f32));
    metrics.upwind_response_fraction = shape_response
        .iter()
        .filter(|(_, zonal, _, response, _, _)| *zonal < 0.0 && *response > 0.0)
        .map(|(_, _, _, response, weight, _)| response * weight)
        .sum::<f32>()
        / response_weight;
    let (east, north) = u15_size_tangent_basis(centroid);
    let wind = u15_normalize_or_zero(u15_trail_wind(centroid, U15_TRAIL_SHEAR));
    let alongwind = [u15_dot(wind, east), u15_dot(wind, north)];
    let crosswind = [-alongwind[1], alongwind[0]];
    let points: Vec<_> = shape_response
        .iter()
        .filter_map(|(pos, _, _, response, weight, _)| {
            if *response <= 0.0 {
                return None;
            }
            let cosine = u15_dot(*pos, centroid).clamp(-1.0, 1.0);
            let direction = u15_normalize(u15_sub(*pos, u15_scale(centroid, cosine)))?;
            let distance = cosine.acos();
            Some((
                u15_dot(direction, east) * distance,
                u15_dot(direction, north) * distance,
                response * weight,
            ))
        })
        .collect::<Vec<_>>();
    if points.len() < 2 {
        return metrics;
    }
    let mean = points.iter().fold([0.0; 2], |sum, (x, y, weight)| {
        [sum[0] + x * weight, sum[1] + y * weight]
    });
    let mean = [mean[0] / response_weight, mean[1] / response_weight];
    let covariance = points.iter().fold([0.0; 3], |sum, (x, y, weight)| {
        let dx = x - mean[0];
        let dy = y - mean[1];
        [
            sum[0] + dx * dx * weight,
            sum[1] + dx * dy * weight,
            sum[2] + dy * dy * weight,
        ]
    });
    let angle = 0.5 * (2.0 * covariance[1]).atan2(covariance[0] - covariance[2]);
    let major = [angle.cos(), angle.sin()];
    let mut alongwind_values: Vec<_> = points
        .iter()
        .map(|(x, y, weight)| {
            (
                (x - mean[0]) * alongwind[0] + (y - mean[1]) * alongwind[1],
                *weight,
            )
        })
        .collect();
    let mut crosswind_values: Vec<_> = points
        .iter()
        .map(|(x, y, weight)| {
            (
                (x - mean[0]) * crosswind[0] + (y - mean[1]) * crosswind[1],
                *weight,
            )
        })
        .collect();
    metrics.alongwind_span = u15_weighted_quantile(&mut alongwind_values, 0.9).unwrap_or(0.0)
        - u15_weighted_quantile(&mut alongwind_values, 0.1).unwrap_or(0.0);
    metrics.crosswind_span = u15_weighted_quantile(&mut crosswind_values, 0.9).unwrap_or(0.0)
        - u15_weighted_quantile(&mut crosswind_values, 0.1).unwrap_or(0.0);
    metrics.axis_wind_degrees = (major[0] * alongwind[0] + major[1] * alongwind[1])
        .abs()
        .clamp(-1.0, 1.0)
        .acos()
        .to_degrees();
    metrics.area_telemetry = response_weight;
    metrics.outer_support_response_fraction = shape_response
        .iter()
        .filter(|(_, _, _, _, _, distance)| *distance >= U15_TRAIL_OUTER_SUPPORT_RADIUS)
        .map(|(_, _, _, response, weight, _)| response * weight)
        .sum::<f32>()
        / response_weight;
    metrics.boundary_shell_response_fraction = shape_response
        .iter()
        .filter(|(_, _, _, _, _, distance)| *distance >= U15_TRAIL_BOUNDARY_SHELL_INNER_RADIUS)
        .map(|(_, _, _, response, weight, _)| response * weight)
        .sum::<f32>()
        / response_weight;
    metrics.isotropic_edge_telemetry = shape_response
        .iter()
        .flat_map(|(pos, _, _, _, _, _)| {
            let pixel = u15_sphere_to_pixel(*pos, resolution);
            let local = pixel % (resolution as usize * resolution as usize);
            let weight = u15_weight(
                (local % resolution as usize) as u32,
                (local / resolution as usize) as u32,
                resolution,
            );
            u15_pixel_neighbors(pixel, resolution)
                .into_iter()
                .map(move |neighbor| (response[pixel] - response[neighbor]).abs() * weight)
        })
        .sum();
    let step = std::f32::consts::FRAC_PI_2 / resolution as f32;
    let crosswind_gradient = shape_response
        .iter()
        .map(|(pos, _, _, value, weight, _)| {
            let local_crosswind =
                u15_normalize_or_zero(u15_cross(*pos, u15_trail_wind(*pos, U15_TRAIL_SHEAR)));
            let plus = response
                [u15_sphere_to_pixel(u15_geodesic_offset(*pos, local_crosswind, step), resolution)];
            let minus = response[u15_sphere_to_pixel(
                u15_geodesic_offset(*pos, local_crosswind, -step),
                resolution,
            )];
            (
                (plus - minus).abs() / (step * (plus + minus).max(f32::EPSILON)),
                value * weight,
            )
        })
        .fold((0.0, 0.0), |(sum, total), (gradient, weight)| {
            (sum + gradient * weight, total + weight)
        });
    metrics.sharpness =
        metrics.crosswind_span * crosswind_gradient.0 / crosswind_gradient.1.max(f32::EPSILON);
    metrics
}

fn u15_thermal_equator_delta(mass: &[f32], resolution: u32, tilt: f32) -> f32 {
    let bands = [-0.08_f32, 0.0, 0.08].map(|center| {
        let (sum, count) = mass.chunks_exact(4).enumerate().fold(
            (0.0, 0usize),
            |(sum, count), (pixel, channels)| {
                let position = u15_pixel_position(pixel, resolution);
                let latitude = position[1] * tilt.cos() + position[2] * tilt.sin();
                if (latitude - center).abs() <= 0.02 {
                    (sum + channels[0] + channels[1] + channels[2], count + 1)
                } else {
                    (sum, count)
                }
            },
        );
        sum / count.max(1) as f32
    });
    (bands[1] - bands[0]).abs().max((bands[2] - bands[1]).abs())
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

fn u15_add(left: [f32; 3], right: [f32; 3]) -> [f32; 3] {
    std::array::from_fn(|axis| left[axis] + right[axis])
}

fn u15_sub(left: [f32; 3], right: [f32; 3]) -> [f32; 3] {
    std::array::from_fn(|axis| left[axis] - right[axis])
}

fn u15_scale(vector: [f32; 3], scale: f32) -> [f32; 3] {
    vector.map(|value| value * scale)
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

fn u15_anvil_component_report(metrics: &U15CompliantAnvilMetrics) -> String {
    metrics
        .components
        .iter()
        .map(|component| {
            if metrics.missing_components.contains(&component.index) {
                format!("core{}=NO_ANVIL", component.index)
            } else {
                format!(
                    "core{}=extent:{} shift:{} angle:{} elong:{}",
                    component.index,
                    u15_metric(component.outside_high_mass_fraction),
                    u15_metric(component.downwind_centroid_texels),
                    u15_metric(component.pca_alignment_degrees),
                    u15_metric(component.elongation),
                )
            }
        })
        .collect::<Vec<_>>()
        .join(",")
}

fn u15_compliant_anvil_metrics(
    seed: u32,
    response: &[f32],
    components: &[U15ResponseComponent],
    resolution: u32,
) -> U15CompliantAnvilMetrics {
    if components.is_empty() {
        return U15CompliantAnvilMetrics::default();
    }

    // U15 triage P3 (2026-09-05): organization is a per-core property. The previous
    // implementation aggregated every seeded core into one global centroid and measured
    // displacement/PCA there — with cores scattered across the sphere that phantom point
    // recorded geometry, not anvil physics (aggregate shifts read -68..-1 texels while
    // every individual core drifted +10..+17 texels downwind). Per core: own deep
    // centroid, own local fixture wind for the downwind reference, and unlabeled high
    // cloud assigned to its nearest core within the anvil capture zone. Thresholds are
    // unchanged from the aggregate gate; only the geometry is repaired.
    let labels = u15_component_labels(components, resolution);
    let mut frames: Vec<([f32; 3], [f32; 3], [f32; 3], Option<[f32; 3]>)> = Vec::new();
    for component in components {
        let mut center = [0.0; 3];
        for &pixel in &component.pixels {
            let weight = response[pixel * 4 + 1];
            let position = u15_pixel_position(pixel, resolution);
            for axis in 0..3 {
                center[axis] += position[axis] * weight;
            }
        }
        let Some(core) = u15_normalize(center) else {
            continue;
        };
        // C-5 (pending ruling): steering reference = environmental flow with the
        // core's own convergence excluded. Thresholds are unchanged from C-4.
        let wind = u15_fixture_environmental_wind(seed, 8, core);
        let downwind = u15_normalize([
            wind[0] - core[0] * u15_dot(wind, core),
            wind[1] - core[1] * u15_dot(wind, core),
            wind[2] - core[2] * u15_dot(wind, core),
        ]);
        let reference = if core[1].abs() > 0.9 {
            [1.0, 0.0, 0.0]
        } else {
            [0.0, 1.0, 0.0]
        };
        let Some(east) = u15_normalize(u15_cross(reference, core)) else {
            continue;
        };
        let north = u15_cross(core, east);
        frames.push((core, east, north, downwind));
    }
    if frames.is_empty() {
        return U15CompliantAnvilMetrics::default();
    }

    #[derive(Clone, Default)]
    struct Bucket {
        captured: f32,
        centroid: [f32; 3],
        covariance: [f32; 3],
    }
    let mut buckets = vec![Bucket::default(); frames.len()];
    let mut total_high = 0.0_f32;
    let mut outside_high = 0.0_f32;
    for pixel in 0..response.len() / 4 {
        let high = response[pixel * 4 + 2];
        if high < U15_DEEP_THRESHOLD {
            continue;
        }
        total_high += high;
        if labels[pixel].is_some() {
            continue;
        }
        outside_high += high;
        let position = u15_pixel_position(pixel, resolution);
        let mut nearest = usize::MAX;
        let mut nearest_distance = f32::INFINITY;
        for (index, (core, ..)) in frames.iter().enumerate() {
            let distance = u15_dot(position, *core).clamp(-1.0, 1.0).acos();
            if distance < nearest_distance {
                nearest_distance = distance;
                nearest = index;
            }
        }
        if nearest == usize::MAX || nearest_distance > U15_ANVIL_CAPTURE_RADIUS {
            continue;
        }
        let bucket = &mut buckets[nearest];
        bucket.captured += high;
        for axis in 0..3 {
            bucket.centroid[axis] += position[axis] * high;
        }
        let (core, east, north, _) = frames[nearest];
        let cosine = u15_dot(position, core).clamp(-1.0, 1.0);
        let Some(direction) = u15_normalize([
            position[0] - core[0] * cosine,
            position[1] - core[1] * cosine,
            position[2] - core[2] * cosine,
        ]) else {
            continue;
        };
        let distance = cosine.acos();
        let east_value = u15_dot(direction, east) * distance;
        let north_value = u15_dot(direction, north) * distance;
        bucket.covariance[0] += east_value * east_value * high;
        bucket.covariance[1] += east_value * north_value * high;
        bucket.covariance[2] += north_value * north_value * high;
    }

    let outside_fraction = (total_high > 0.0).then_some(outside_high / total_high);
    let mut per_core = Vec::with_capacity(frames.len());
    let mut missing_components = Vec::new();
    let mut minimum_shift: Option<f32> = None;
    let mut worst_angle: Option<f32> = None;
    for index in 0..frames.len() {
        let (core, east, north, downwind) = frames[index];
        let bucket = &buckets[index];
        let shift = match (downwind, u15_normalize(bucket.centroid)) {
            (Some(downwind), Some(centroid)) if bucket.captured > 0.0 => {
                let cosine = u15_dot(centroid, core).clamp(-1.0, 1.0);
                let delta = [
                    centroid[0] - core[0] * cosine,
                    centroid[1] - core[1] * cosine,
                    centroid[2] - core[2] * cosine,
                ];
                u15_normalize(delta).map(|direction| {
                    u15_dot(direction, downwind) * cosine.acos()
                        / (std::f32::consts::FRAC_PI_2 / resolution as f32)
                })
            }
            _ => None,
        };
        let angle = match (downwind, bucket.captured > 0.0) {
            (Some(downwind), true)
                if bucket.covariance[0] + bucket.covariance[2] > f32::EPSILON =>
            {
                let principal_angle = 0.5
                    * (2.0 * bucket.covariance[1])
                        .atan2(bucket.covariance[0] - bucket.covariance[2]);
                let expected = [u15_dot(downwind, east), u15_dot(downwind, north)];
                Some(
                    (principal_angle.cos() * expected[0] + principal_angle.sin() * expected[1])
                        .abs()
                        .clamp(-1.0, 1.0)
                        .acos()
                        .to_degrees(),
                )
            }
            _ => None,
        };
        if bucket.captured <= 0.0 || shift.is_none() || angle.is_none() {
            missing_components.push(index);
        }
        let elongation = if bucket.covariance[0] + bucket.covariance[2] > f32::EPSILON {
            let mean = 0.5 * (bucket.covariance[0] + bucket.covariance[2]);
            let disc = ((0.5 * (bucket.covariance[0] - bucket.covariance[2])).powi(2)
                + bucket.covariance[1] * bucket.covariance[1])
                .sqrt();
            let minor = mean - disc;
            (minor > f32::EPSILON).then(|| ((mean + disc) / minor).sqrt())
        } else {
            None
        };
        if let Some(shift) = shift {
            minimum_shift = Some(minimum_shift.map_or(shift, |value: f32| value.min(shift)));
        }
        if let Some(angle) = angle {
            worst_angle = Some(worst_angle.map_or(angle, |value: f32| value.max(angle)));
        }
        per_core.push(U15AnvilComponentMetrics {
            index,
            outside_high_mass_fraction: (total_high > 0.0).then_some(bucket.captured / total_high),
            downwind_centroid_texels: shift,
            pca_alignment_degrees: angle,
            elongation,
        });
    }
    U15CompliantAnvilMetrics {
        core_count: components.len(),
        missing_components,
        outside_high_mass_fraction: outside_fraction,
        minimum_downwind_centroid_texels: minimum_shift,
        worst_pca_alignment_degrees: worst_angle,
        components: per_core,
    }
}

// TEMP tilt investigation diagnostic (PLANET_GEN_U15_DUMP_TILT=1): per-core PCA
// angle vs downwind decomposed by capture annulus, plus inter-core bearings.
fn u15_anvil_tilt_diagnostics(
    seed: u32,
    response: &[f32],
    components: &[U15ResponseComponent],
    resolution: u32,
) {
    let labels = u15_component_labels(components, resolution);
    let mut frames: Vec<([f32; 3], [f32; 3], [f32; 3], [f32; 3])> = Vec::new();
    for component in components {
        let mut center = [0.0; 3];
        for &pixel in &component.pixels {
            let weight = response[pixel * 4 + 1];
            let position = u15_pixel_position(pixel, resolution);
            for axis in 0..3 {
                center[axis] += position[axis] * weight;
            }
        }
        let Some(core) = u15_normalize(center) else {
            continue;
        };
        let wind = u15_fixture_seed_wind(seed, 8, core);
        let Some(downwind) = u15_normalize([
            wind[0] - core[0] * u15_dot(wind, core),
            wind[1] - core[1] * u15_dot(wind, core),
            wind[2] - core[2] * u15_dot(wind, core),
        ]) else {
            continue;
        };
        let reference = if core[1].abs() > 0.9 {
            [1.0, 0.0, 0.0]
        } else {
            [0.0, 1.0, 0.0]
        };
        let Some(east) = u15_normalize(u15_cross(reference, core)) else {
            continue;
        };
        let north = u15_cross(core, east);
        frames.push((core, east, north, downwind));
    }
    #[derive(Clone, Copy, Default)]
    struct Annulus {
        mass: f32,
        centroid: [f32; 3],
        covariance: [f32; 3],
    }
    let mut buckets = vec![[Annulus::default(); 3]; frames.len()];
    for pixel in 0..response.len() / 4 {
        let high = response[pixel * 4 + 2];
        if high < U15_DEEP_THRESHOLD || labels[pixel].is_some() {
            continue;
        }
        let position = u15_pixel_position(pixel, resolution);
        let mut nearest = usize::MAX;
        let mut nearest_distance = f32::INFINITY;
        for (index, (core, ..)) in frames.iter().enumerate() {
            let distance = u15_dot(position, *core).clamp(-1.0, 1.0).acos();
            if distance < nearest_distance {
                nearest_distance = distance;
                nearest = index;
            }
        }
        let annulus = (nearest_distance / U15_FIXTURE_CORE_RADIUS) as usize;
        if nearest == usize::MAX || annulus >= 3 {
            continue;
        }
        let (core, east, north, _) = frames[nearest];
        let cosine = u15_dot(position, core).clamp(-1.0, 1.0);
        let Some(direction) = u15_normalize([
            position[0] - core[0] * cosine,
            position[1] - core[1] * cosine,
            position[2] - core[2] * cosine,
        ]) else {
            continue;
        };
        let distance = cosine.acos();
        let east_value = u15_dot(direction, east) * distance;
        let north_value = u15_dot(direction, north) * distance;
        let slot = &mut buckets[nearest][annulus];
        slot.mass += high;
        for axis in 0..3 {
            slot.centroid[axis] += position[axis] * high;
        }
        slot.covariance[0] += east_value * east_value * high;
        slot.covariance[1] += east_value * north_value * high;
        slot.covariance[2] += north_value * north_value * high;
    }
    for index in 0..frames.len() {
        let (core, east, north, downwind) = frames[index];
        let wind_bearing = u15_dot(downwind, north).atan2(u15_dot(downwind, east));
        // Reference-deflection probe: nearest seeded center, centroid offset, and
        // the angle between the fixture wind at the component centroid (includes
        // own-core inflow) and the tangent base flow at the same point.
        let mut best_center = usize::MAX;
        let mut best_distance = f32::INFINITY;
        for candidate in 0..8u32 {
            let seeded = u15_fixture_center(seed, candidate);
            let distance = u15_dot(core, seeded).clamp(-1.0, 1.0).acos();
            if distance < best_distance {
                best_distance = distance;
                best_center = candidate as usize;
            }
        }
        let texel = std::f32::consts::FRAC_PI_2 / resolution as f32;
        let projection = u15_dot(U15_FIXTURE_FLOW, core);
        let base = [
            U15_FIXTURE_FLOW[0] - core[0] * projection,
            U15_FIXTURE_FLOW[1] - core[1] * projection,
            U15_FIXTURE_FLOW[2] - core[2] * projection,
        ];
        let base_bearing = u15_dot(base, north).atan2(u15_dot(base, east));
        let mut deflection = (wind_bearing - base_bearing).to_degrees();
        while deflection >= 90.0 {
            deflection -= 180.0;
        }
        while deflection < -90.0 {
            deflection += 180.0;
        }
        println!(
            "u15_deflect seed={seed} core={index} center={best_center} offset_texels={:.1} base_mag={:.3} reference_deflection_deg={deflection:.1}",
            best_distance / texel,
            (base[0] * base[0] + base[1] * base[1] + base[2] * base[2]).sqrt(),
        );
        let mut parts = Vec::new();
        let mut full_covariance = [0.0f32; 3];
        let mut full_centroid = [0.0f32; 3];
        for annulus in 0..3 {
            let slot = &buckets[index][annulus];
            for axis in 0..3 {
                full_covariance[axis] += slot.covariance[axis];
                full_centroid[axis] += slot.centroid[axis];
            }
            if slot.covariance[0] + slot.covariance[2] <= f32::EPSILON {
                parts.push(format!("a{annulus}:mass=0"));
                continue;
            }
            let principal =
                0.5 * (2.0 * slot.covariance[1]).atan2(slot.covariance[0] - slot.covariance[2]);
            let mut tilt = (principal - wind_bearing).to_degrees();
            while tilt >= 90.0 {
                tilt -= 180.0;
            }
            while tilt < -90.0 {
                tilt += 180.0;
            }
            parts.push(format!("a{annulus}:mass={:.2},tilt={tilt:.1}", slot.mass));
        }
        let full_principal =
            0.5 * (2.0 * full_covariance[1]).atan2(full_covariance[0] - full_covariance[2]);
        let fold = |delta: f32| {
            let mut value = delta;
            while value >= 90.0 {
                value -= 180.0;
            }
            while value < -90.0 {
                value += 180.0;
            }
            value
        };
        let shift_pair = match (u15_normalize(full_centroid), u15_normalize(base)) {
            (Some(centroid), Some(base_unit)) => {
                let cosine = u15_dot(centroid, core).clamp(-1.0, 1.0);
                let delta = [
                    centroid[0] - core[0] * cosine,
                    centroid[1] - core[1] * cosine,
                    centroid[2] - core[2] * cosine,
                ];
                u15_normalize(delta).map(|direction| {
                    let angular = cosine.acos();
                    (
                        u15_dot(direction, downwind) * angular / texel,
                        u15_dot(direction, base_unit) * angular / texel,
                    )
                })
            }
            _ => None,
        };
        match shift_pair {
            Some((shift_wind, shift_base)) => println!(
                "u15_full seed={seed} core={index} tilt_vs_local_wind={:.1} tilt_vs_base_flow={:.1} shift_vs_local_wind={:+.1} shift_vs_base_flow={:+.1}",
                fold((full_principal - wind_bearing).to_degrees()),
                fold((full_principal - base_bearing).to_degrees()),
                shift_wind,
                shift_base,
            ),
            None => println!(
                "u15_full seed={seed} core={index} tilt_vs_local_wind={:.1} tilt_vs_base_flow={:.1} shift=none",
                fold((full_principal - wind_bearing).to_degrees()),
                fold((full_principal - base_bearing).to_degrees()),
            ),
        }
        let mut neighbors = Vec::new();
        for other in 0..frames.len() {
            if other == index {
                continue;
            }
            let (core_b, ..) = frames[other];
            let cosine = u15_dot(core_b, core).clamp(-1.0, 1.0);
            let distance = cosine.acos();
            if distance > 4.0 * U15_FIXTURE_CORE_RADIUS {
                continue;
            }
            let Some(direction) = u15_normalize([
                core_b[0] - core[0] * cosine,
                core_b[1] - core[1] * cosine,
                core_b[2] - core[2] * cosine,
            ]) else {
                continue;
            };
            let bearing = (u15_dot(direction, north).atan2(u15_dot(direction, east))
                - wind_bearing)
                .to_degrees();
            neighbors.push(format!(
                "{other}@{}R/relbearing{bearing:.0}",
                (distance / U15_FIXTURE_CORE_RADIUS * 10.0).round() / 10.0
            ));
        }
        println!(
            "u15_tilt seed={seed} core={index} [{}] neighbors=[{}]",
            parts.join(","),
            neighbors.join(" ")
        );
    }
}

fn u15_trail_height(pos: [f32; 3]) -> f32 {
    let distance = u15_dot(U15_TRAIL_SOURCE, pos).clamp(-1.0, 1.0).acos();
    if distance <= U15_TRAIL_SOURCE_RADIUS {
        -0.10
    } else if distance >= U15_TRAIL_SHORE_RADIUS {
        0.02
    } else {
        -0.10
            + (distance - U15_TRAIL_SOURCE_RADIUS)
                / (U15_TRAIL_SHORE_RADIUS - U15_TRAIL_SOURCE_RADIUS)
                * 0.12
    }
}

fn u15_trail_control_height(_: [f32; 3]) -> f32 {
    0.02
}

fn u15_trail_continentality(pos: [f32; 3]) -> f32 {
    let distance = u15_dot(U15_TRAIL_SOURCE, pos).clamp(-1.0, 1.0).acos();
    ((distance - U15_TRAIL_SOURCE_RADIUS) / (U15_TRAIL_SHORE_RADIUS - U15_TRAIL_SOURCE_RADIUS))
        .clamp(0.0, 1.0)
}

fn u15_trail_response(source: &[f32], control: &[f32]) -> Vec<f32> {
    source
        .chunks_exact(4)
        .zip(control.chunks_exact(4))
        .map(|(source, control)| {
            ((source[0] + source[1] + source[2]) - (control[0] + control[1] + control[2])).max(0.0)
        })
        .collect()
}

fn u15_trail_weather_snapshot(resolution: u32, seed: u32, wind_scale: f32) -> WeatherSnapshot {
    WeatherSnapshot {
        face: 0,
        resolution,
        seed,
        storm_count: 0,
        coverage: 1.0,
        moisture: 1.0,
        surface_pressure_bar: 1.0,
        base_temp_c: 15.0,
        ocean_level: 0.0,
        axial_tilt_rad: 0.0,
        season: 0.5,
        storm_size: 1.0,
        radius_km: 6371.0,
        rotation_rate_rad_s: std::f32::consts::TAU / 86400.0,
        wind_scale,
    }
}

fn run_u15_field_validation(
    gpu: &GpuContext,
    pipeline: &WeatherFieldPipeline,
    output_dir: &str,
    resolution: u32,
    validation_flag: &str,
    seeds: &[u32],
    persist_per_seed: bool,
) -> Vec<String> {
    let wind = WindFieldPipeline::new(gpu).expect("U15 dynamics unavailable");
    u15_assert_size_preconditions();
    let generate = |seed: u32,
                    storm_count: u32,
                    storm_size: f32,
                    moisture: f32,
                    temp: f32,
                    wind_scale: f32,
                    axial_tilt_rad: f32| {
        // The named storm cores receive convergent wind over an upwind slope.
        let terrain = u14_flat_terrain(resolution, |pos| u15_fixture_height(seed, 8, pos));
        let dynamics = wind.create_test_textures(gpu, resolution, |pos| {
            let velocity = u15_fixture_seed_wind(seed, 8, pos);
            ([velocity[0], velocity[1], velocity[2], 0.0], 1000.0)
        });
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
                axial_tilt_rad,
                season: 0.5,
                storm_size,
                radius_km: 6371.0,
                rotation_rate_rad_s: std::f32::consts::TAU / 86400.0,
                wind_scale,
            },
            &terrain,
            &dynamics,
            &field,
        );
        (field.read_mass(gpu), field.read_geometry(gpu))
    };
    let generate_size = |seed: u32, storm_count: u32, storm_size: f32| {
        let terrain = u14_flat_terrain(resolution, |pos| {
            u15_size_fixture_height(seed, U15_SIZE_STORM_COUNT, pos)
        });
        let dynamics = wind.create_test_textures(gpu, resolution, |pos| {
            let velocity = u15_size_fixture_seed_wind(seed, U15_SIZE_STORM_COUNT, pos);
            ([velocity[0], velocity[1], velocity[2], 0.0], 810.0)
        });
        let field = pipeline.create_textures(gpu, resolution);
        let snapshot = WeatherSnapshot {
            storm_count,
            ..u15_size_weather_snapshot(resolution, seed, storm_size)
        };
        let provenance = pipeline
            .generate_with_provenance_for_validation(gpu, snapshot, &terrain, &dynamics, &field);
        (field.read_mass(gpu), field.read_geometry(gpu), provenance)
    };
    let generate_without_source = |seed: u32, storm_count: u32| {
        let terrain = u14_flat_terrain(resolution, |pos| u15_fixture_height(seed, 8, pos));
        let dynamics = wind.create_test_textures(gpu, resolution, |pos| {
            let velocity = u15_fixture_seed_wind(seed, 8, pos);
            ([velocity[0], velocity[1], velocity[2], 0.0], 1000.0)
        });
        let field = pipeline.create_textures(gpu, resolution);
        pipeline.generate_with_diagnostic_flags(
            gpu,
            WeatherSnapshot {
                face: 0,
                resolution,
                seed,
                storm_count,
                coverage: 1.0,
                moisture: 1.0,
                surface_pressure_bar: 1.0,
                base_temp_c: 35.0,
                ocean_level: 0.0,
                axial_tilt_rad: 0.0,
                season: 0.5,
                storm_size: 3.0,
                radius_km: 6371.0,
                rotation_rate_rad_s: std::f32::consts::TAU / 86400.0,
                wind_scale: 1.0,
            },
            &terrain,
            &dynamics,
            &field,
            WEATHER_DIAGNOSTIC_NO_SOURCE,
        );
        (field.read_mass(gpu), field.read_geometry(gpu))
    };
    let generate_trail = |seed: u32, wind_scale: f32, control: bool, shear: f32| {
        let terrain = u14_flat_terrain(
            resolution,
            if control {
                u15_trail_control_height
            } else {
                u15_trail_height
            },
        );
        let dynamics = wind.create_test_textures(gpu, resolution, |pos| {
            let wind = u15_trail_wind(pos, shear);
            (
                [
                    wind[0],
                    wind[1],
                    wind[2],
                    if control {
                        1.0
                    } else {
                        u15_trail_continentality(pos)
                    },
                ],
                1013.0,
            )
        });
        let field = pipeline.create_textures(gpu, resolution);
        pipeline.generate(
            gpu,
            u15_trail_weather_snapshot(resolution, seed, wind_scale),
            &terrain,
            &dynamics,
            &field,
        );
        field.read_mass(gpu)
    };
    let mut rows = Vec::new();
    let mut failures = Vec::new();
    let mut worst_outside_high_fraction = f32::INFINITY;
    let mut worst_downwind_centroid_texels = f32::INFINITY;
    let mut worst_pca_alignment_degrees = 0.0_f32;
    let mut qualified_organization_seed_count = 0usize;
    let mut size_deterministic = true;
    for &seed in seeds {
        let cases = [0, 4, 8].map(|count| generate(seed, count, 1.0, 1.0, 35.0, 1.0, 0.0));
        let size_seed = U15_SIZE_SEEDS[U15_VALIDATION_SEEDS
            .iter()
            .position(|candidate| *candidate == seed)
            .expect("U15 validation seed must be frozen")];
        assert!(
            u15_size_seed_criterion(size_seed),
            "U15 Size seed {size_seed} violates its frozen input criterion"
        );
        let size_centers = u15_fixture_centers(size_seed, U15_SIZE_STORM_COUNT);
        let size_association = u15_size_association(resolution, &size_centers);
        let sized = [0.3, 1.0, 3.0]
            .map(|storm_size| generate_size(size_seed, U15_SIZE_STORM_COUNT, storm_size));
        let sized_repeat = [0.3, 1.0, 3.0]
            .map(|storm_size| generate_size(size_seed, U15_SIZE_STORM_COUNT, storm_size));
        let size_control =
            [0.3, 1.0, 3.0].map(|storm_size| generate_size(size_seed, 0, storm_size));
        let size_repeat_matches = sized == sized_repeat;
        size_deterministic &= size_repeat_matches;
        let moisture_zero = [0, 8].map(|count| generate(seed, count, 3.0, 0.0, 35.0, 1.0, 0.0));
        let moist_stable = [0, 8].map(|count| generate(seed, count, 3.0, 1.0, -35.0, 1.0, 0.0));
        let source_disabled = [0, 8].map(|count| generate_without_source(seed, count));
        if !persist_per_seed && seed == U15_VALIDATION_SEEDS[0] {
            save_field_views(
                output_dir,
                "u15_seed_7_eligible_storms_8",
                resolution,
                &cases[2].0,
                &cases[2].1,
            );
            save_field_views(
                output_dir,
                "u15_seed_7_ineligible_dry_stable_storms_8",
                resolution,
                &moisture_zero[1].0,
                &moisture_zero[1].1,
            );
            save_field_views(
                output_dir,
                "u15_seed_7_ineligible_no_source_storms_8",
                resolution,
                &source_disabled[1].0,
                &source_disabled[1].1,
            );
        }
        let thermal_deltas = [0.0, 0.35, 0.70].map(|tilt| {
            u15_thermal_equator_delta(
                &generate(seed, 0, 1.0, 1.0, 15.0, 1.0, tilt).0,
                resolution,
                tilt,
            )
        });
        let thermal_continuous = thermal_deltas.into_iter().all(|delta| delta <= 0.10);
        let trail_started = Instant::now();
        let trail_source =
            [1.0, 2.0].map(|wind_scale| generate_trail(seed, wind_scale, false, U15_TRAIL_SHEAR));
        let trail_control =
            [1.0, 2.0].map(|wind_scale| generate_trail(seed, wind_scale, true, U15_TRAIL_SHEAR));
        let trail_source_repeat =
            [1.0, 2.0].map(|wind_scale| generate_trail(seed, wind_scale, false, U15_TRAIL_SHEAR));
        let trail_control_repeat =
            [1.0, 2.0].map(|wind_scale| generate_trail(seed, wind_scale, true, U15_TRAIL_SHEAR));
        let trail_response: [Vec<f32>; 2] = std::array::from_fn(|index| {
            u15_trail_response(&trail_source[index], &trail_control[index])
        });
        if std::env::var_os("PLANET_GEN_U15_DUMP_TRAIL").is_some() {
            save_u15_trail_scalar_view(
                output_dir,
                &format!("u15tr_seed_{seed}_ws1_response"),
                resolution,
                &trail_response[0],
            );
            save_u15_trail_scalar_view(
                output_dir,
                &format!("u15tr_seed_{seed}_ws2_response"),
                resolution,
                &trail_response[1],
            );
        }
        let plume = trail_response
            .each_ref()
            .map(|response| u15_plume_metrics(response, resolution));
        let plume_length_ratio =
            plume[1].alongwind_span / plume[0].alongwind_span.max(f32::EPSILON);
        let plume_breadth_ratio =
            plume[1].crosswind_span / plume[0].crosswind_span.max(f32::EPSILON);
        let plume_sharpness_ratio = plume[1].sharpness / plume[0].sharpness.max(f32::EPSILON);
        let plume_area_ratio = plume[1].area_telemetry / plume[0].area_telemetry.max(f32::EPSILON);
        let plume_edge_ratio =
            plume[1].isotropic_edge_telemetry / plume[0].isotropic_edge_telemetry.max(f32::EPSILON);
        let plume_deterministic =
            trail_source == trail_source_repeat && trail_control == trail_control_repeat;
        let diagnostics = u15_seed_diagnostics(
            &cases[0].0,
            &cases[2].0,
            &trail_response[1],
            resolution,
            &plume[1],
        );
        let trail_runtime_ms = trail_started.elapsed().as_secs_f64() * 1000.0;
        // U15 triage 2026-09-05 (user-approved gate surgery, see
        // docs/research/2026-09-05-u15-triage-findings.md): in the current
        // physics regime the ws=1 plume is diffusion-limited (crosswind >=
        // alongwind on every seed), so its PCA major axis carries no
        // wind-ownership information; the weak-wind plume must instead
        // organize downwind of the source. The axis gate applies to the ws=2
        // plume, where the response is an elongated streak. Ratio bands were
        // re-specified for the dilution-limited reach scaling that replaced
        // the pre-0e2b044 transport-limited regime.
        let plume_pass = plume.iter().enumerate().all(|(scale, metric)| {
            let organized = if scale == 0 {
                metric
                    .downwind_centroid_texels
                    .is_some_and(|texels| texels >= 8.0)
                    && metric.upwind_response_fraction <= 0.05
            } else {
                metric.axis_wind_degrees <= 30.0
            };
            metric.response_p95 >= 0.04
                && metric.effective_n >= 32.0
                && organized
                && metric.outer_support_response_fraction <= 0.05
                && metric.boundary_shell_response_fraction <= 0.01
                && metric.outside_corridor_response_fraction <= 0.05
        }) && (1.10..=2.75).contains(&plume_length_ratio)
            && (0.80..=1.25).contains(&plume_breadth_ratio)
            && (0.70..=1.25).contains(&plume_sharpness_ratio)
            && plume_deterministic;
        let count_response: [Vec<f32>; 3] =
            std::array::from_fn(|index| u15_response(&cases[index].0, &cases[0].0));
        let endpoint_components: [Vec<U15ResponseComponent>; 3] = std::array::from_fn(|index| {
            u15_significant_response_components(&cases[index].0, resolution, Some(seed))
        });
        let count_components: [Vec<U15ResponseComponent>; 3] = std::array::from_fn(|index| {
            u15_significant_response_components(&count_response[index], resolution, Some(seed))
        });
        let size_response: [Vec<f32>; 3] =
            std::array::from_fn(|index| u15_response(&sized[index].0, &size_control[index].0));
        let size_components: [Vec<U15ResponseComponent>; 3] = std::array::from_fn(|index| {
            u15_significant_size_components(&size_response[index], resolution, &size_association)
        });
        let core: [U15CoreMetrics; 3] = std::array::from_fn(|index| {
            u15_significant_cores(&count_components[index], &count_response[index])
        });
        let endpoint_core: [U15CoreMetrics; 3] = std::array::from_fn(|index| {
            u15_significant_cores(&endpoint_components[index], &cases[index].0)
        });
        let size_owners: [Vec<Option<U15OwnerSizeMetrics>>; 3] = std::array::from_fn(|index| {
            u15_owner_size_metrics(&size_components[index], &sized[index].1, &size_association)
        });
        let size_area = size_owners
            .each_ref()
            .map(|owners| u15_primary_area(owners));
        let size_top_pairs = u15_paired_size_tops(&size_owners[0], &size_owners[2]);
        let size_fragmentation_pass = size_owners
            .iter()
            .all(|owners| u15_fragmentation_within_bound(owners));
        let size_area_ratios = [
            size_area[1]
                .zip(size_area[0])
                .map(|(medium, small)| medium / small.max(f32::EPSILON)),
            size_area[2]
                .zip(size_area[0])
                .map(|(large, small)| large / small.max(f32::EPSILON)),
        ];
        let size_top_deltas = [
            u15_size_top_deltas(&size_owners[0], &size_owners[1]),
            u15_size_top_deltas(&size_owners[0], &size_owners[2]),
        ];
        let legacy_size_area_growth_pass = size_area[0]
            .zip(size_area[2])
            .is_some_and(|(small, large)| large >= small * 1.5);
        let physical_eligible =
            u15_size_minimum_physical_eligibility(size_seed) >= U15_SIZE_MIN_PHYSICAL_ELIGIBILITY;
        // ponytail: only the U15 owner-growth gate consumes transient P; preview/export stay ABI-identical.
        let size_area_growth_pass = sized[0]
            .2
            .as_ref()
            .zip(sized[2].2.as_ref())
            .map(|(small_provenance, large_provenance)| {
                physical_eligible
                    && u15_provenance_total(small_provenance, resolution).is_finite()
                    && u15_provenance_total(large_provenance, resolution).is_finite()
                    && (0..U15_SIZE_STORM_COUNT).all(|owner| {
                        [
                            u15_owner_primary_provenance_share(
                                small_provenance,
                                &sized[0].0,
                                resolution,
                                &size_components[0],
                                &size_association,
                                owner,
                            ),
                            u15_owner_primary_provenance_share(
                                large_provenance,
                                &sized[2].0,
                                resolution,
                                &size_components[2],
                                &size_association,
                                owner,
                            ),
                        ]
                        .into_iter()
                        .all(|share| {
                            share.is_some_and(|value| value >= U15_MIN_OWNER_PROVENANCE_SHARE)
                        })
                    })
                    && legacy_size_area_growth_pass
            })
            .unwrap_or(legacy_size_area_growth_pass);
        let size_top_growth_pass = size_top_pairs.as_ref().is_some_and(|pairs| {
            pairs
                .iter()
                .all(|pair| pair.large_top_p95 >= pair.small_top_p95 + 2.0)
        });
        let size_gate = size_area_growth_pass && size_top_growth_pass;
        let size_predicate = format!(
            "fragmentation(satellite_count<={U15_MAX_SATELLITES_PER_OWNER},satellite_area_ratio<={U15_MAX_SATELLITE_AREA_RATIO:.2})={size_fragmentation_pass} && large_primary_area>=small_primary_area*1.5&&owner_provenance_share>={U15_MIN_OWNER_PROVENANCE_SHARE:.2}={size_area_growth_pass} && each_paired_large_top_p95>=small_top_p95+2.0={size_top_growth_pass} => {}",
            size_fragmentation_pass && size_gate,
        );
        let mask_deep: [Option<f32>; 3] = std::array::from_fn(|index| {
            u15_mask_deep_mass(&count_response[index], resolution, seed)
        });
        let total_zero = u15_solid_angle_total(&cases[0].0, resolution);
        let total_eight = u15_solid_angle_total(&cases[2].0, resolution);
        let condensate_change = (total_eight - total_zero).abs() / total_zero.max(f32::EPSILON);
        let anvil =
            u15_compliant_anvil_metrics(seed, &count_response[2], &count_components[2], resolution);
        if std::env::var_os("PLANET_GEN_U15_DUMP_TILT").is_some() {
            u15_anvil_tilt_diagnostics(seed, &count_response[2], &count_components[2], resolution);
        }
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
        qualified_organization_seed_count += usize::from(anvil_pass);
        let moisture_zero_exact = moisture_zero[0].0 == moisture_zero[1].0
            && moisture_zero[0].1 == moisture_zero[1].1
            && moisture_zero[0]
                .0
                .iter()
                .chain(&moisture_zero[0].1)
                .all(|value| *value == 0.0);
        let source_disabled_deep = source_disabled
            .each_ref()
            .map(|(mass, _)| u15_deep_mass_metrics(mass));
        let source_disabled_deep_exact = source_disabled_deep == [(0.0, 0.0); 2];
        let moist_stable_identical =
            moist_stable[0].0 == moist_stable[1].0 && moist_stable[0].1 == moist_stable[1].1;
        rows.push(format!(
            "seed={seed} plume1={:?} plume2={:?} L2_L1={plume_length_ratio:.5} B2_B1={plume_breadth_ratio:.5} S2_S1={plume_sharpness_ratio:.5} area_telemetry_ratio={plume_area_ratio:.5} isotropic_edge_telemetry_ratio={plume_edge_ratio:.5} outside_corridor={:.5}/{:.5} components={}/{} deterministic={plume_deterministic} runtime_ms={trail_runtime_ms:.3} plume_status={} count_telemetry=edge:{:.5},occupancy:{:.5},mass:{:.5},deterministic:{} size_seed={size_seed} size_repeat_matches={size_repeat_matches} size_small=[{}] size_medium=[{}] size_large=[{}] size_missing=small:{:?},medium:{:?},large:{:?} size_aggregate_primary_area=small:{:?},medium:{:?},large:{:?} size_aggregate_area_ratio=medium_small:{:?},large_small:{:?} size_aggregate_top_delta=medium_small:{:?},large_small:{:?} size_predicate={size_predicate} thermal_equator_continuous={thermal_continuous} column_mass_delta={condensate_change:.5} anvil_status={} anvil_components={} source_disabled_gpu_deep_exact={source_disabled_deep_exact} dry_gpu_exact={moisture_zero_exact} moist_stable_identical={moist_stable_identical}",
            plume[0],
            plume[1],
            plume[0].outside_corridor_response_fraction,
            plume[1].outside_corridor_response_fraction,
            plume[1].component_count,
            plume[1].detached_component_count,
            if plume_pass { "PASS" } else { "FAIL" },
            u15_mass_edge_energy(&cases[2].0, resolution),
            cases[2].0.chunks_exact(4).filter(|channels| channels[0] + channels[1] + channels[2] >= U15_TRAIL_Q).count() as f32 / (cases[2].0.len() / 4) as f32,
            u15_solid_angle_total(&cases[2].0, resolution),
            cases[2] == generate(seed, 8, 1.0, 1.0, 35.0, 1.0, 0.0),
            u15_size_owner_report(&size_owners[0]),
            u15_size_owner_report(&size_owners[1]),
            u15_size_owner_report(&size_owners[2]),
            u15_missing_size_owners(&size_owners[0]),
            u15_missing_size_owners(&size_owners[1]),
            u15_missing_size_owners(&size_owners[2]),
            size_area[0],
            size_area[1],
            size_area[2],
            size_area_ratios[0],
            size_area_ratios[1],
            size_top_deltas[0],
            size_top_deltas[1],
            if anvil_pass { "PASS" } else { "FAIL" },
            u15_anvil_component_report(&anvil),
        ));
        if persist_per_seed {
            let prefix = format!("u15_seed_{seed}");
            save_field_views(
                output_dir,
                &format!("{prefix}_eligible_storms_8"),
                resolution,
                &cases[2].0,
                &cases[2].1,
            );
            save_field_views(
                output_dir,
                &format!("{prefix}_ineligible_dry_stable_storms_8"),
                resolution,
                &moisture_zero[1].0,
                &moisture_zero[1].1,
            );
            save_field_views(
                output_dir,
                &format!("{prefix}_ineligible_no_source_storms_8"),
                resolution,
                &source_disabled[1].0,
                &source_disabled[1].1,
            );
            std::fs::write(
                Path::new(output_dir).join(format!("{prefix}_metrics.txt")),
                u15_seed_diagnostics_report(seed, &diagnostics),
            )
            .expect("write incremental U15 seed metrics artifact");
            std::fs::write(
                Path::new(output_dir).join("u15_field_metrics.partial.txt"),
                rows.join("\n"),
            )
            .expect("write incremental U15 metrics artifact");
        }
        let row = rows.last().map(String::as_str).unwrap_or("missing metrics");
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
        if endpoint_core[2].count < endpoint_core[0].count + 2
            || !p95_gate
            || !core[2].deep_p95.is_some_and(|value| value > 0.15)
            || !mask_deep[2].is_some_and(|value| value >= U15_DEEP_THRESHOLD)
            || condensate_change > 0.20
        {
            failures.push(format!("U15 seed {seed} count/deep/mass: {row}"));
        }
        if !size_fragmentation_pass || !size_gate {
            failures.push(format!("U15 seed {seed} size: {row}"));
        }
        if !anvil_pass {
            failures.push(format!("U15 seed {seed} compliant anvil response: {row}"));
        }
        if !moisture_zero_exact {
            failures.push(format!("U15 seed {seed} moisture-zero is not exact zero"));
        }
        if !source_disabled_deep_exact {
            failures.push(format!(
                "U15 seed {seed} source-disabled GPU storm guard created mass: {row}"
            ));
        }
        if !moist_stable_identical {
            failures.push(format!(
                "U15 seed {seed} moist-stable storm fields are not bit-identical"
            ));
        }
        if !thermal_continuous {
            failures.push(format!(
                "U15 seed {seed} thermal-equator continuity failed: adjacent deltas={thermal_deltas:?}"
            ));
        }
        if !plume_pass {
            failures.push(format!(
                "U15 seed {seed} shear-driven plume fixture: scale1={:?}, scale2={:?}, L2/L1={plume_length_ratio:.3}, B2/B1={plume_breadth_ratio:.3}, S2/S1={plume_sharpness_ratio:.3}, area_telemetry={plume_area_ratio:.3}, isotropic_edge_telemetry={plume_edge_ratio:.3}, outside_corridor={:.3}/{:.3}, components={}/{}, deterministic={plume_deterministic}",
                plume[0], plume[1],
                plume[0].outside_corridor_response_fraction,
                plume[1].outside_corridor_response_fraction,
                plume[1].component_count,
                plume[1].detached_component_count,
            ));
        }
    }
    let values = format!(
        "command=cargo run --release --features validation --bin sweep -- --{validation_flag} --size 512 --output-dir {output_dir}\nshear_plume_fixture=source:{U15_TRAIL_SOURCE:?},zonal_axis:{U15_TRAIL_EAST:?},Y:{U15_TRAIL_NORTH:?};wind=((1+{U15_TRAIL_SHEAR:.2}*tanh(y/.08))/(1+{U15_TRAIL_SHEAR:.2}))*cross(Y,p); tangent_divergence_free; speed_bound=[{:.5},1]; snapshot.wind_scale=1/2; source=ocean_patch; control=matched_exterior_land_and_continentality; coverage:1,moisture:1,temp_c:15,pressure_hpa:1013,tilt:0,season:.5,earth_radius_rotation,storms:0,diagnostics:0; MUSCL/Hancock active; CFL substeps use active `transport_substeps`; shape_corridor={U15_TRAIL_SHAPE_ROI_RADIUS:.8},physical_support={U15_TRAIL_OUTER_SUPPORT_RADIUS:.8},boundary_shell={U15_TRAIL_BOUNDARY_SHELL_INNER_RADIUS:.6}..{U15_TRAIL_SHAPE_ROI_RADIUS:.6}; response=max(total_mass_source-total_mass_control,0); morphology=all_counterfactual_response>=.02_within_frozen_corridor_own_centroid_log_map_wind_frame_weighted_Q90_Q10_L_alongwind_B_crosswind; Gperp=weighted_normalized_crosswind_geodesic_derivative; S=B*Gperp; gates=each:p95>=.04,Neff>=32,outside_corridor<=5%,beyond_physical_support<=5%,boundary_shell<=1%; ws1:downwind_centroid>=+8texels,upwind_mass<=5% (diffusion-limited blob regime — PCA axis is not a wind-ownership diagnostic); ws2:axis<=30deg; ratios:L2/L1=1.10..2.75,B2/B1=.80..1.25,S2/S1=.70..1.25,deterministic; mass/area/isotropic_edge/components/centroid=telemetry_only; seeds={seeds:?}\nfixture={U15_ELIGIBLE_MASK}; fixture_flow={U15_FIXTURE_FLOW:?}; source_guard=actual_GPU_weather_pipeline; size_deterministic={size_deterministic}; qualified_organization_seed_count={qualified_organization_seed_count}/{}; qualified_organization=outside_high>=.10,each_core_downwind_centroid>=.5texels,each_core_pca<=20deg(per-core:own centroid+environmental steering flow with own-core convergence excluded,capture=3xcore_radius nearest-core assignment)\n{}\n",
        (1.0 - U15_TRAIL_SHEAR) / (1.0 + U15_TRAIL_SHEAR),
        seeds.len(),
        rows.join("\n"),
    );
    std::fs::write(Path::new(output_dir).join("u15_field_metrics.txt"), values)
        .expect("write U15 metrics artifact");
    if !size_deterministic {
        failures.push("U15 Size same-seed generation was not bitwise deterministic".to_string());
    }
    if qualified_organization_seed_count != seeds.len() {
        failures.push(format!(
            "U15 qualified storm organization {qualified_organization_seed_count}/{}",
            seeds.len()
        ));
    }
    failures
}

#[derive(Default)]
struct U3LocalMetrics {
    windows: usize,
    axis_samples: usize,
    axis_median: f32,
    axis_p90: f32,
    anisotropy_median: f32,
    anisotropy_p90: f32,
    curvature_p95: f32,
}

fn u3_quantile(values: &mut [f32], quantile: f32) -> f32 {
    values.sort_by(f32::total_cmp);
    values[((values.len() - 1) as f32 * quantile).round() as usize]
}

fn u3_density(pixels: &[u8], pixel: usize) -> f32 {
    pixels[pixel * 4] as f32 / 255.0
}

fn u3_inside(size: usize, x: isize, y: isize, halo: isize) -> bool {
    if x < halo || y < halo || x + halo >= size as isize || y + halo >= size as isize {
        return false;
    }
    [-halo, halo].into_iter().all(|dy| {
        [-halo, halo].into_iter().all(|dx| {
            let nx = (x + dx) as f32 / size as f32 * 2.0 + 1.0 / size as f32 - 1.0;
            let ny = (y + dy) as f32 / size as f32 * 2.0 + 1.0 / size as f32 - 1.0;
            nx * nx + ny * ny <= 0.85_f32.powi(2)
        })
    })
}

fn u3_dot(a: [f32; 2], b: [f32; 2]) -> f32 {
    a[0] * b[0] + a[1] * b[1]
}

fn u3_normalize(v: [f32; 2]) -> Option<[f32; 2]> {
    let length = u3_dot(v, v).sqrt();
    (length > 1.0e-6).then(|| [v[0] / length, v[1] / length])
}

fn u3_feature_axis(xx: f32, xy: f32, yy: f32) -> [f32; 2] {
    let discriminant = ((xx - yy).powi(2) + 4.0 * xy * xy).sqrt();
    let normal = u3_normalize([2.0 * xy, yy - xx + discriminant]).unwrap_or([1.0, 0.0]);
    [normal[1], -normal[0]]
}

fn u3_projected_test_wind(
    x: usize,
    y: usize,
    size: usize,
    rotation: [[f32; 4]; 4],
) -> Option<[f32; 2]> {
    let direction = u3_screen_direction(x, y, size, rotation);
    let wind = [direction[2], 0.0, -direction[0]];
    let right = u3_screen_direction(x + 1, y, size, rotation);
    let down = u3_screen_direction(x, y + 1, size, rotation);
    let jx = [
        right[0] - direction[0],
        right[1] - direction[1],
        right[2] - direction[2],
    ];
    let jy = [
        down[0] - direction[0],
        down[1] - direction[1],
        down[2] - direction[2],
    ];
    let dot3 = |a: [f32; 3], b: [f32; 3]| a[0] * b[0] + a[1] * b[1] + a[2] * b[2];
    let gxx = dot3(jx, jx);
    let gxy = dot3(jx, jy);
    let gyy = dot3(jy, jy);
    let determinant = gxx * gyy - gxy * gxy;
    if determinant <= 1.0e-12 {
        return None;
    }
    let bx = dot3(jx, wind);
    let by = dot3(jy, wind);
    u3_normalize([
        (gyy * bx - gxy * by) / determinant,
        (gxx * by - gxy * bx) / determinant,
    ])
}

fn u3_blur_radius4(values: &[f32], size: usize) -> Vec<f32> {
    let mut blurred = vec![0.0; values.len()];
    for y in 0..size {
        for x in 0..size {
            let mut sum = 0.0;
            let mut count = 0.0;
            for dy in -4isize..=4 {
                for dx in -4isize..=4 {
                    let nx = x as isize + dx;
                    let ny = y as isize + dy;
                    if u3_inside(size, nx, ny, 0) {
                        sum += values[ny as usize * size + nx as usize];
                        count += 1.0;
                    }
                }
            }
            blurred[y * size + x] = sum / count;
        }
    }
    blurred
}

fn u3_macro_metrics(left: &[u8], right: &[u8], size: usize) -> (f32, f32, f32, f32) {
    let left_values: Vec<_> = (0..size * size)
        .map(|pixel| u3_density(left, pixel))
        .collect();
    let right_values: Vec<_> = (0..size * size)
        .map(|pixel| u3_density(right, pixel))
        .collect();
    let left_blurred = u3_blur_radius4(&left_values, size);
    let right_blurred = u3_blur_radius4(&right_values, size);
    let valid: Vec<_> = (0..size * size)
        .filter(|pixel| u3_inside(size, (pixel % size) as isize, (pixel / size) as isize, 4))
        .collect();
    let left_mean =
        valid.iter().map(|pixel| left_blurred[*pixel]).sum::<f32>() / valid.len() as f32;
    let right_mean =
        valid.iter().map(|pixel| right_blurred[*pixel]).sum::<f32>() / valid.len() as f32;
    let (mut covariance, mut left_variance, mut right_variance, mut mae) = (0.0, 0.0, 0.0, 0.0);
    let (mut left_count, mut right_count, mut left_x, mut left_y, mut right_x, mut right_y) =
        (0usize, 0usize, 0.0, 0.0, 0.0, 0.0);
    for &pixel in &valid {
        let left_delta = left_blurred[pixel] - left_mean;
        let right_delta = right_blurred[pixel] - right_mean;
        covariance += left_delta * right_delta;
        left_variance += left_delta * left_delta;
        right_variance += right_delta * right_delta;
        mae += (left_blurred[pixel] - right_blurred[pixel]).abs();
        let x = (pixel % size) as f32 / size as f32;
        let y = (pixel / size) as f32 / size as f32;
        if left_values[pixel] > 0.01 {
            left_count += 1;
            left_x += x;
            left_y += y;
        }
        if right_values[pixel] > 0.01 {
            right_count += 1;
            right_x += x;
            right_y += y;
        }
    }
    let centroid = if left_count > 0 && right_count > 0 {
        let dx = left_x / left_count as f32 - right_x / right_count as f32;
        let dy = left_y / left_count as f32 - right_y / right_count as f32;
        (dx * dx + dy * dy).sqrt()
    } else {
        0.0
    };
    (
        covariance / (left_variance * right_variance).sqrt().max(f32::EPSILON),
        mae / valid.len() as f32,
        (left_count as f32 - right_count as f32).abs() / valid.len() as f32,
        centroid,
    )
}

fn u3_local_metrics(
    detail: &[u8],
    baseline: &[u8],
    size: usize,
    rotation: [[f32; 4]; 4],
    mass: &[f32],
    resolution: u32,
    family: usize,
) -> U3LocalMetrics {
    let detail_values: Vec<_> = (0..size * size)
        .map(|pixel| u3_density(detail, pixel))
        .collect();
    let baseline_values: Vec<_> = (0..size * size)
        .map(|pixel| u3_density(baseline, pixel))
        .collect();
    let residual: Vec<_> = detail_values
        .iter()
        .zip(&baseline_values)
        .map(|(on, off)| on - off)
        .collect();
    let mut metrics = U3LocalMetrics::default();
    let (mut angles, mut anisotropies, mut axes) = (Vec::new(), Vec::new(), Vec::new());
    for y in (14..size - 14).step_by(16) {
        for x in (14..size - 14).step_by(16) {
            if !u3_inside(size, x as isize, y as isize, 13) {
                continue;
            }
            let center_distance = ((x as f32 - size as f32 * 0.5).powi(2)
                + (y as f32 - size as f32 * 0.5).powi(2))
            .sqrt()
                / size as f32;
            if center_distance > 0.15 {
                continue;
            }
            if !(y - 12..=y + 12).all(|py| {
                (x - 12..=x + 12).all(|px| {
                    u3_linear_cube_sample(
                        mass,
                        resolution,
                        u3_screen_direction(px, py, size, rotation),
                        family,
                    ) >= 0.001
                })
            }) {
                continue;
            }
            let mut rms = 0.0;
            for py in y - 12..=y + 12 {
                for px in x - 12..=x + 12 {
                    rms += residual[py * size + px].powi(2);
                }
            }
            rms = (rms / 625.0).sqrt();
            let Some(wind) = u3_projected_test_wind(x, y, size, rotation) else {
                continue;
            };
            if rms < 0.001 {
                continue;
            }
            metrics.windows += 1;
            let (mut xx, mut xy, mut yy) = (0.0, 0.0, 0.0);
            for py in y - 11..=y + 11 {
                for px in x - 11..=x + 11 {
                    let gx = (residual[py * size + px + 1] - residual[py * size + px - 1]) * 0.5;
                    let gy =
                        (residual[(py + 1) * size + px] - residual[(py - 1) * size + px]) * 0.5;
                    xx += gx * gx;
                    xy += gx * gy;
                    yy += gy * gy;
                }
            }
            let trace = xx + yy;
            let anisotropy = ((xx - yy).powi(2) + 4.0 * xy * xy).sqrt() / trace.max(f32::EPSILON);
            anisotropies.push(anisotropy);
            if anisotropy < 0.10 {
                continue;
            }
            let axis = u3_feature_axis(xx, xy, yy);
            angles.push(
                u3_dot(axis, wind)
                    .abs()
                    .clamp(-1.0, 1.0)
                    .acos()
                    .to_degrees(),
            );
            axes.push((x, y, wind, axis));
        }
    }
    metrics.axis_samples = axes.len();
    if !angles.is_empty() {
        metrics.axis_median = u3_quantile(&mut angles, 0.5);
        metrics.axis_p90 = u3_quantile(&mut angles, 0.9);
    }
    if !anisotropies.is_empty() {
        metrics.anisotropy_median = u3_quantile(&mut anisotropies, 0.5);
        metrics.anisotropy_p90 = u3_quantile(&mut anisotropies, 0.9);
    }
    let mut curvatures = Vec::new();
    for &(x, y, wind, axis) in &axes {
        let mut adjacent = None;
        for &(other_x, other_y, _, other_axis) in &axes {
            let dx = other_x as isize - x as isize;
            let dy = other_y as isize - y as isize;
            if dx.abs() + dy.abs() != 16 {
                continue;
            }
            let distance = (dx * dx + dy * dy) as f32;
            let alignment = (dx as f32 * wind[0] + dy as f32 * wind[1]) / distance.sqrt();
            if alignment > 0.7 && adjacent.is_none_or(|(best, _): (f32, [f32; 2])| alignment > best)
            {
                adjacent = Some((alignment, other_axis));
            }
        }
        if let Some((_, other_axis)) = adjacent {
            curvatures.push(
                u3_dot(axis, other_axis)
                    .abs()
                    .clamp(-1.0, 1.0)
                    .acos()
                    .to_degrees(),
            );
        }
    }
    if !curvatures.is_empty() {
        metrics.curvature_p95 = u3_quantile(&mut curvatures, 0.95);
    }
    metrics
}

fn u3_residual_edge_energy(detail: &[u8], baseline: &[u8], size: usize) -> f32 {
    let residual: Vec<_> = (0..size * size)
        .map(|pixel| u3_density(detail, pixel) - u3_density(baseline, pixel))
        .collect();
    let mut energy = 0.0;
    let mut count = 0usize;
    for y in 1..size - 1 {
        for x in 1..size - 1 {
            if u3_inside(size, x as isize, y as isize, 0) {
                energy += (residual[y * size + x + 1] - residual[y * size + x]).powi(2)
                    + (residual[(y + 1) * size + x] - residual[y * size + x]).powi(2);
                count += 2;
            }
        }
    }
    energy / count as f32
}

fn u3_residual_axis(detail: &[u8], baseline: &[u8], size: usize) -> Option<[f32; 2]> {
    let mut tensor = [0.0; 3];
    for y in 1..size - 1 {
        for x in 1..size - 1 {
            if !u3_inside(size, x as isize, y as isize, 1) {
                continue;
            }
            let residual = |x: usize, y: usize| {
                u3_density(detail, y * size + x) - u3_density(baseline, y * size + x)
            };
            let gx = (residual(x + 1, y) - residual(x - 1, y)) * 0.5;
            let gy = (residual(x, y + 1) - residual(x, y - 1)) * 0.5;
            tensor[0] += gx * gx;
            tensor[1] += gx * gy;
            tensor[2] += gy * gy;
        }
    }
    (tensor[0] + tensor[2] > f32::EPSILON).then(|| u3_feature_axis(tensor[0], tensor[1], tensor[2]))
}

fn u3_family_only_fixture(
    pipeline: &WeatherFieldPipeline,
    gpu: &GpuContext,
    resolution: u32,
    family: usize,
) -> WeatherTextures {
    let weather = pipeline.create_textures(gpu, resolution);
    let mut mass = vec![0.0; resolution as usize * resolution as usize * 6 * 4];
    let mut geometry = vec![0.0; mass.len()];
    for (mass, geometry) in mass.chunks_exact_mut(4).zip(geometry.chunks_exact_mut(4)) {
        mass[family] = 0.35;
        mass[3] = if family < 2 { 0.35 } else { 0.0 };
        geometry.copy_from_slice(&[1.0, 2.0, 8.0, 11.0]);
    }
    weather.overwrite_for_sweep(gpu, &mass, &geometry);
    weather
}

fn run_u3_local_validation(
    gpu: &GpuContext,
    weather_pipeline: &WeatherFieldPipeline,
    wind_pipeline: &WindFieldPipeline,
    scene: &WeatherScene,
    base_uniforms: PreviewUniforms,
    output_dir: &str,
    render_size: u32,
) -> Vec<String> {
    let baseline_renderer = PreviewRenderer::new_with_cloud_detail_layers(gpu, [0.0; 3]);
    let full_renderer = PreviewRenderer::new_with_cloud_detail_layers(gpu, [1.0; 3]);
    let layer_renderers = [
        PreviewRenderer::new_with_cloud_detail_layers(gpu, [1.0, 0.0, 0.0]),
        PreviewRenderer::new_with_cloud_detail_layers(gpu, [0.0, 1.0, 0.0]),
        PreviewRenderer::new_with_cloud_detail_layers(gpu, [0.0, 0.0, 1.0]),
    ];
    let mut failures = Vec::new();
    let mut report = Vec::new();
    let mut render_samples_ms = Vec::new();
    let mut contact_sheet = Vec::new();
    let weather_resolution = (render_size / 2).clamp(64, 384);
    let weather_dynamics = wind_pipeline.create_test_textures(gpu, weather_resolution, |pos| {
        let tangent = [pos[2], 0.0, -pos[0]];
        let length = (tangent[0] * tangent[0] + tangent[2] * tangent[2])
            .sqrt()
            .max(1.0e-8);
        ([tangent[0] / length, 0.0, tangent[2] / length, 0.0], 1013.0)
    });
    let weather = generate_validation_weather_with_dynamics(
        weather_pipeline,
        gpu,
        scene,
        &weather_dynamics,
        42,
        4,
        1.0,
    );
    let family_weather: [WeatherTextures; 3] = std::array::from_fn(|family| {
        u3_family_only_fixture(weather_pipeline, gpu, weather.resolution, family)
    });
    let family_mass = family_weather
        .each_ref()
        .map(|weather| weather.read_mass(gpu));
    let mut magnitude_reference: Option<Vec<u8>> = None;
    for (speed_label, speed) in [("Wind.2", 0.2), ("Wind1", 1.0), ("Wind2", 2.0)] {
        let dynamics = wind_pipeline.create_test_textures(gpu, weather_resolution, move |pos| {
            let tangent = [pos[2], 0.0, -pos[0]];
            let length = (tangent[0] * tangent[0] + tangent[2] * tangent[2])
                .sqrt()
                .max(1.0e-8);
            (
                [
                    tangent[0] * speed / length,
                    0.0,
                    tangent[2] * speed / length,
                    0.0,
                ],
                1013.0,
            )
        });
        let dynamics_y = wind_pipeline.create_test_textures(gpu, weather_resolution, move |pos| {
            let tangent = [-pos[0] * pos[1], 1.0 - pos[1] * pos[1], -pos[2] * pos[1]];
            let length =
                (tangent[0] * tangent[0] + tangent[1] * tangent[1] + tangent[2] * tangent[2])
                    .sqrt()
                    .max(1.0e-8);
            (
                [
                    tangent[0] * speed / length,
                    tangent[1] * speed / length,
                    tangent[2] * speed / length,
                    0.0,
                ],
                1013.0,
            )
        });
        let uniforms = PreviewUniforms {
            view_mode: 9,
            cloud_advection: 1.0,
            ..base_uniforms
        };
        let mut layer_metrics = Vec::new();
        for (family, (name, renderer)) in [
            ("low", &layer_renderers[0]),
            ("deep", &layer_renderers[1]),
            ("high", &layer_renderers[2]),
        ]
        .into_iter()
        .enumerate()
        {
            let metric_weather = &family_weather[family];
            let metric_mass = &family_mass[family];
            let (metric_baseline, baseline_ms) = time_gpu_call(gpu, || {
                render_weather_with_dynamics(
                    &baseline_renderer,
                    gpu,
                    &uniforms,
                    scene,
                    &dynamics,
                    metric_weather,
                    render_size,
                )
            });
            render_samples_ms.push(baseline_ms);
            let (detail, detail_ms) = time_gpu_call(gpu, || {
                render_weather_with_dynamics(
                    renderer,
                    gpu,
                    &uniforms,
                    scene,
                    &dynamics,
                    metric_weather,
                    render_size,
                )
            });
            render_samples_ms.push(detail_ms);
            let (detail_y, detail_y_ms) = time_gpu_call(gpu, || {
                render_weather_with_dynamics(
                    renderer,
                    gpu,
                    &uniforms,
                    scene,
                    &dynamics_y,
                    metric_weather,
                    render_size,
                )
            });
            render_samples_ms.push(detail_y_ms);
            let metrics = u3_local_metrics(
                &detail,
                &metric_baseline,
                render_size as usize,
                uniforms.rotation,
                metric_mass,
                metric_weather.resolution,
                family,
            );
            let (macro_corr, macro_mae, occupied_delta, centroid_delta) =
                u3_macro_metrics(&detail, &detail_y, render_size as usize);
            let xy_mae = detail
                .iter()
                .zip(&detail_y)
                .map(|(x, y)| (*x as f32 - *y as f32).abs())
                .sum::<f32>()
                / (detail.len() as f32 * 255.0);
            if family < 2 {
                if detail != detail_y {
                    failures.push(format!(
                        "U3 {speed_label} {name}: Wind-X/Y changed an isotropic layer"
                    ));
                }
            } else if speed == 1.0 {
                let x_axis = u3_residual_axis(&detail, &metric_baseline, render_size as usize);
                let y_axis = u3_residual_axis(&detail_y, &metric_baseline, render_size as usize);
                let axis_rotation = x_axis
                    .zip(y_axis)
                    .map(|(x, y)| u3_dot(x, y).abs().clamp(-1.0, 1.0).acos().to_degrees());
                let edge_retention =
                    u3_residual_edge_energy(&detail_y, &metric_baseline, render_size as usize)
                        / u3_residual_edge_energy(&detail, &metric_baseline, render_size as usize)
                            .max(f32::EPSILON);
                if metrics.windows < 8
                    || metrics.axis_samples < 8
                    || !(0.001..=0.010).contains(&xy_mae)
                    || axis_rotation.is_none_or(|value| value < 60.0)
                    || metrics.axis_median > 30.0
                    || macro_corr < 0.995
                    || occupied_delta > 0.02
                    || centroid_delta >= 0.005
                    || edge_retention < 0.90
                {
                    failures.push(format!("U3 high Wind-X/Y: mae={xy_mae:.5} axis_rotation={} axis_to_wind={:.2} macro_corr={macro_corr:.5} occupied_delta={:.2}% centroid={centroid_delta:.5} edge_retention={edge_retention:.3}", u15_metric(axis_rotation), metrics.axis_median, occupied_delta * 100.0));
                }
            }
            if family == 2 {
                if let Some(reference) = &magnitude_reference {
                    let magnitude_mae = detail
                        .iter()
                        .zip(reference)
                        .map(|(x, y)| (*x as f32 - *y as f32).abs())
                        .sum::<f32>()
                        / (detail.len() as f32 * 255.0);
                    if magnitude_mae > 1.0e-4 {
                        failures.push(format!("U3 high {speed_label}: same-direction magnitude MAE={magnitude_mae:.6} > 1e-4"));
                    }
                } else {
                    magnitude_reference = Some(detail.clone());
                }
            }
            layer_metrics.push(format!("{name}: windows={} axis_samples={} axis={:.2}/{:.2} anisotropy={:.3}/{:.3} curvature={:.2} wind_xy_mae={xy_mae:.5} macro={macro_corr:.5}/{macro_mae:.5}/{:.2}%/{centroid_delta:.5}", metrics.windows, metrics.axis_samples, metrics.axis_median, metrics.axis_p90, metrics.anisotropy_median, metrics.anisotropy_p90, metrics.curvature_p95, occupied_delta * 100.0));
        }
        let (full, full_ms) = time_gpu_call(gpu, || {
            render_weather_with_dynamics(
                &full_renderer,
                gpu,
                &uniforms,
                scene,
                &dynamics,
                &weather,
                render_size,
            )
        });
        let (baseline, baseline_ms) = time_gpu_call(gpu, || {
            render_weather_with_dynamics(
                &baseline_renderer,
                gpu,
                &uniforms,
                scene,
                &dynamics,
                &weather,
                render_size,
            )
        });
        render_samples_ms.extend([full_ms, baseline_ms]);
        save_png(
            output_dir,
            &format!("u3_{speed_label}_detail.png"),
            render_size,
            &full,
        );
        save_png(
            output_dir,
            &format!("u3_{speed_label}_detail_off.png"),
            render_size,
            &baseline,
        );
        contact_sheet.push((full, baseline));
        report.push(format!("{speed_label}: {}", layer_metrics.join("; ")));
    }
    let render_stats = compute_runtime_stats(render_samples_ms);
    report.push(format!(
        "render_ms n={} p95={:.3} min={:.3} max={:.3} mean={:.3}",
        render_stats.count,
        render_stats.p95_ms,
        render_stats.min_ms,
        render_stats.max_ms,
        render_stats.mean_ms
    ));
    save_contact_sheet(
        output_dir,
        "u3_local_contact_sheet.png",
        render_size,
        &contact_sheet,
    );
    std::fs::write(
        Path::new(output_dir).join("u3_local_metrics.txt"),
        report.join("\n"),
    )
    .expect("write U3 local metrics artifact");
    failures
}

// DS-046 A1–A3: interior-spread, micro-texture, and streak-anisotropy gates on
// the real validation scene.
//   A1 (re-specified per the DS-046 ruling, 2026-08-24):
//     A1a: coastline-correlation score of the largest connected cloud-free
//          region < T_COAST at wind_scale {0.25, 4.0} (detector promoted from
//          advisory to gating; threshold calibrated once on the FE-081-era
//          build 882b7cb; RV-003 #3: max-wind eval point moved 2.0 → 4.0 when
//          the validated range extended to [0.25, 4.0]).
//     A1b: land cloud fraction ≥ target (0.15 floor at ws < 0.75, else 0.25)
//          at every tested wind_scale in [0.25, 4.0].
//     A1c: raw largest connected cloud-free region ≤35% of sphere at
//          wind_scale 4 (catastrophe backstop, zero exclusions; RV-003 #3:
//          moved from ws=2 to the extended max).
//     The legacy habitable-band raw-area metric is telemetry only.
//   A2: high-pass (<~1500 km removed) condensate RMS ≥15% of the total, with
//       the next octave carrying ≥25% of that energy (≥2 octaves of texture).
//   A3: along-wind/cross-wind condensate autocorrelation ratio ≤3:1 at
//       wind_scale 2 (lags ≈300 km and ≈800 km).
// DS-046 ruling (2026-08-24) T_coast calibration, measured once on the
// FE-081-era build (commit 882b7cb), same eval points as the gate (seed-42
// validation scene, equirect grid, wind_scale 0.25 and 2.0): baseline
// coastline-correlation scores were −0.068 (ws=min) / +0.135 (ws=max);
// T_coast = 50% × max(−0.068, +0.135) = 0.068.
// RV-003 #3: wind_scale 4.0 added as the max-wind eval point — measured 0.017
// on the FE-090-era build (target/val-fe090-run.log), far below T_coast, so no
// recalibration was needed.
const DS046_A1A_T_COAST: f32 = 0.068;
// RV-002 A1b re-specification (measured, validation run 6): the warm-land
// occupied fraction at ws=0.25 is FLAT with distance from coast (≤2 texels
// 20.7% → all land 17.5%) — low-wind land cloud is local physics (stratiform
// decks + orographic condensation), not fetch-delivered, so no coastal
// restriction can raise it to the full target within the pinned horizon
// (FE-088 falsified horizon extension; RV-002 #3 falsified ET strength).
// Re-spec: full 0.25 target at ws ≥ 1.0, where transport reach within the
// horizon is sufficient; at ws=0.25 a 0.15 FLOOR — "land not structurally
// cloud-free" — catches collapse of the local physics without pretending the
// pinned horizon delivers marine vapor at low wind.
const DS046_A1B_T_LOW_WIND: f32 = 0.15;
fn ds046_equirect_direction(width: usize, height: usize, x: usize, y: usize) -> [f32; 3] {
    let lon = std::f32::consts::TAU * (x as f32 + 0.5) / width as f32;
    let lat = std::f32::consts::PI * ((y as f32 + 0.5) / height as f32 - 0.5);
    let cos_lat = lat.cos();
    [cos_lat * lon.cos(), lat.sin(), cos_lat * lon.sin()]
}

struct Ds046Grid {
    width: usize,
    height: usize,
    occupied: Vec<bool>,
    condensate: Vec<f32>,
    land: Vec<bool>,
    // Habitable-band mask: the A1 defect is dry WARM interiors
    // (geography-through-absence); polar/cold aridity is correct physics and
    // must not inflate the cloud-free region.
    warm: Vec<bool>,
    // RV-002 A1a re-specification: subtropical subsidence belt (|lat| in
    // [20°, 35°], same tilt-corrected latitude as `warm`). Hadley dry belts
    // are modeled physics, not the coastline-tracing defect A1a detects; they
    // are excluded from A1a free-region growth only. A1c keeps zero exclusions.
    subsidence: Vec<bool>,
}

impl Ds046Grid {
    fn weight(&self, index: usize) -> f64 {
        let lat =
            std::f32::consts::PI * (((index / self.width) as f32 + 0.5) / self.height as f32 - 0.5);
        lat.cos().max(0.0) as f64
    }
}

// RV-003: equirect cloud-map dump (condensate low+deep, land warm-tinted,
// ocean blue) so wind_scale excursions beyond the gated range can be compared
// visually. Diagnostic output only; not consumed by any gate.
fn ds046_dump_cloud_map_ppm(grid: &Ds046Grid, path: &str) {
    let max_c = grid
        .condensate
        .iter()
        .cloned()
        .fold(0.0_f32, f32::max)
        .max(1e-6);
    let v = |x: f32| (x.clamp(0.0, 1.0) * 255.0) as u8;
    let mut buf = Vec::with_capacity(grid.width * grid.height * 3 + 32);
    buf.extend_from_slice(format!("P6\n{} {}\n255\n", grid.width, grid.height).as_bytes());
    for i in 0..grid.width * grid.height {
        let c = (grid.condensate[i] / max_c).clamp(0.0, 1.0);
        if grid.land[i] {
            buf.extend_from_slice(&[v(0.30 + 0.70 * c), v(0.22 + 0.60 * c), v(0.12 + 0.30 * c)]);
        } else {
            buf.extend_from_slice(&[v(0.75 + 0.25 * c), v(0.85 + 0.15 * c), v(0.95 + 0.05 * c)]);
        }
    }
    if let Ok(mut f) = std::fs::File::create(path) {
        use std::io::Write;
        let _ = f.write_all(&buf);
    }
}

fn ds046_sample_grid(scene: &WeatherScene, mass: &[f32], resolution: u32) -> Ds046Grid {
    // Flat terrain at weather resolution for land classification (rgba stride,
    // matching the u3_linear_cube_sample layout).
    let source_resolution = scene.terrain.resolution;
    let mut heights = vec![0.0_f32; 4 * (resolution * resolution * 6) as usize];
    for face in 0..6usize {
        for y in 0..resolution as usize {
            for x in 0..resolution as usize {
                let sx = x * source_resolution as usize / resolution as usize;
                let sy = y * source_resolution as usize / resolution as usize;
                let index =
                    (face * (resolution * resolution) as usize + y * resolution as usize + x) * 4;
                heights[index] = scene.terrain.faces[face][sy * source_resolution as usize + sx];
            }
        }
    }
    let width = (resolution.max(128) as usize).next_power_of_two();
    let height = width / 2;
    let mut grid = Ds046Grid {
        width,
        height,
        occupied: Vec::with_capacity(width * height),
        condensate: Vec::with_capacity(width * height),
        land: vec![false; width * height],
        warm: vec![false; width * height],
        subsidence: vec![false; width * height],
    };
    for y in 0..height {
        for x in 0..width {
            let pos = ds046_equirect_direction(width, height, x, y);
            let occupancy = u3_linear_cube_sample(mass, resolution, pos, 3);
            grid.occupied.push(occupancy > 0.05);
            grid.condensate.push(
                u3_linear_cube_sample(mass, resolution, pos, 0)
                    + u3_linear_cube_sample(mass, resolution, pos, 1),
            );
            let height_value =
                u3_linear_cube_sample(&heights, resolution, pos, 0) - scene.ocean_level;
            grid.land[y * width + x] = height_value > 0.0;
            // Mirror weather_spinup temperature_at (season = 0.5 → no seasonal
            // term): base − latitude·35 − elevation·6.5.
            let tilted_y = pos[1] * scene.tilt_rad.cos() + pos[2] * scene.tilt_rad.sin();
            let latitude = tilted_y.clamp(-1.0, 1.0).asin().abs() / std::f32::consts::FRAC_PI_2;
            let elevation_km = height_value.max(0.0) * 5.0;
            grid.warm[y * width + x] =
                scene.derived.base_temperature_c - latitude * 35.0 - elevation_km * 6.5 >= 0.0;
            // RV-002: subsidence belt from the same tilt-corrected latitude
            // (latitude is normalized 0..1 → ×90 for degrees).
            grid.subsidence[y * width + x] = (20.0_f32..=35.0_f32).contains(&(latitude * 90.0));
        }
    }
    grid
}

// Weighted largest connected cloud-free component as a sphere fraction, plus
// its member mask for downstream boundary analysis. With `restrict_to_warm`
// this keeps the legacy habitable-band semantics (the A1 defect is dry WARM
// interiors): non-warm free cells act as bridges but carry no weight. With
// `restrict_to_warm = false` it measures the raw sphere with zero exclusions
// (A1c backstop semantics). With `exclude_subsidence` the subtropical
// subsidence belt is treated as occupied for region growth (RV-002 A1a
// re-specification: Hadley dry belts are modeled physics, not the
// coastline-tracing defect; A1c must never set this). Longitude wraps;
// latitude clamps at the poles.
fn ds046_largest_free_region(
    grid: &Ds046Grid,
    restrict_to_warm: bool,
    exclude_subsidence: bool,
) -> (f32, Vec<bool>) {
    let width = grid.width;
    let height = grid.height;
    let total_weight: f64 = (0..width * height)
        .filter(|&i| !restrict_to_warm || grid.warm[i])
        .map(|i| grid.weight(i))
        .sum();
    let mut visited = vec![false; width * height];
    let mut largest = 0.0_f64;
    let mut largest_members: Vec<bool> = vec![false; width * height];
    for start in 0..width * height {
        if visited[start]
            || grid.occupied[start]
            || (restrict_to_warm && !grid.warm[start])
            || (exclude_subsidence && grid.subsidence[start])
        {
            continue;
        }
        let mut stack = vec![start];
        visited[start] = true;
        let mut members = vec![start];
        let mut weight_sum = 0.0_f64;
        while let Some(index) = stack.pop() {
            if !restrict_to_warm || grid.warm[index] {
                weight_sum += grid.weight(index);
            }
            let x = index % width;
            let y = index / width;
            for (nx, ny) in [
                ((x + width - 1) % width, y),
                ((x + 1) % width, y),
                (x, y.saturating_sub(1)),
                (x, (y + 1).min(height - 1)),
            ] {
                let next = ny * width + nx;
                if !visited[next]
                    && !grid.occupied[next]
                    && !(exclude_subsidence && grid.subsidence[next])
                {
                    visited[next] = true;
                    stack.push(next);
                    members.push(next);
                }
            }
        }
        if weight_sum > largest {
            largest = weight_sum;
            largest_members = vec![false; width * height];
            for &member in &members {
                largest_members[member] = true;
            }
        }
    }
    (
        (largest / total_weight.max(f64::EPSILON)) as f32,
        largest_members,
    )
}

// FE-088 telemetry only: the reported max-wind void was concentrated in the
// lower hemisphere. Keep this independent of the A1 gates so it catches a
// regression without changing their calibrated thresholds.
fn ds046_lower_hemisphere_telemetry(grid: &Ds046Grid) -> (f32, f32) {
    let mut total_weight = 0.0_f64;
    let mut occupied_weight = 0.0_f64;
    let mut condensate_sum = 0.0_f64;
    for y in grid.height / 2..grid.height {
        for x in 0..grid.width {
            let index = y * grid.width + x;
            let weight = grid.weight(index);
            total_weight += weight;
            if grid.occupied[index] {
                occupied_weight += weight;
            }
            condensate_sum += grid.condensate[index] as f64 * weight;
        }
    }
    (
        (occupied_weight / total_weight.max(f64::EPSILON)) as f32,
        (condensate_sum / total_weight.max(f64::EPSILON)) as f32,
    )
}

// Multi-source texel BFS distance to the nearest coastline (land/ocean
// 4-neighbor transition), in equirect grid texels.
fn ds046_coast_distances(grid: &Ds046Grid) -> Vec<u32> {
    let width = grid.width;
    let height = grid.height;
    let neighbors = |index: usize| -> [usize; 4] {
        let x = index % width;
        let y = index / width;
        [
            y * width + (x + width - 1) % width,
            y * width + (x + 1) % width,
            y.saturating_sub(1) * width + x,
            (y + 1).min(height - 1) * width + x,
        ]
    };
    let mut dist = vec![u32::MAX; width * height];
    let mut queue = std::collections::VecDeque::new();
    for index in 0..width * height {
        if neighbors(index)
            .iter()
            .any(|&next| grid.land[next] != grid.land[index])
        {
            dist[index] = 0;
            queue.push_back(index);
        }
    }
    while let Some(index) = queue.pop_front() {
        for next in neighbors(index) {
            if dist[next] > dist[index] + 1 {
                dist[next] = dist[index] + 1;
                queue.push_back(next);
            }
        }
    }
    dist
}

// DS-046 A1a coastline-correlation score (DS-046 ruling, 2026-08-24): Pearson
// r between the cloudy-facing boundary indicator of the largest connected
// cloud-free region and coast proximity 1/(1+dist_texels), over the full
// equirect grid. Promotes the DS-045 synthetic-fixture detector
// (`ds045_no_coastline_correlated_boundary_across_wind_sweep`) from advisory
// unit-test scope to this gating real-scene metric. A cloud-free region whose
// edge traces the coastline scores near 1; flow-decoupled boundaries score
// near 0. Degenerate fields (no boundary or no coast contrast) score 0.
fn ds046_coastline_correlation(grid: &Ds046Grid, members: &[bool]) -> f32 {
    let width = grid.width;
    let height = grid.height;
    let dist = ds046_coast_distances(grid);
    let neighbor_indices = |index: usize| -> [usize; 4] {
        let x = index % width;
        let y = index / width;
        [
            y * width + (x + width - 1) % width,
            y * width + (x + 1) % width,
            y.saturating_sub(1) * width + x,
            (y + 1).min(height - 1) * width + x,
        ]
    };
    let mut sum_x = 0.0_f64;
    let mut sum_y = 0.0_f64;
    let mut xs = vec![0.0_f64; width * height];
    let mut ys = vec![0.0_f64; width * height];
    for index in 0..width * height {
        let boundary = members[index]
            && neighbor_indices(index)
                .iter()
                .any(|&next| grid.occupied[next]);
        xs[index] = if boundary { 1.0 } else { 0.0 };
        ys[index] = 1.0 / (1.0 + dist[index] as f64);
        sum_x += xs[index];
        sum_y += ys[index];
    }
    let count = (width * height) as f64;
    let mean_x = sum_x / count;
    let mean_y = sum_y / count;
    let mut covariance = 0.0_f64;
    let mut var_x = 0.0_f64;
    let mut var_y = 0.0_f64;
    for index in 0..width * height {
        covariance += (xs[index] - mean_x) * (ys[index] - mean_y);
        var_x += (xs[index] - mean_x) * (xs[index] - mean_x);
        var_y += (ys[index] - mean_y) * (ys[index] - mean_y);
    }
    let denom = (var_x * var_y).sqrt();
    if denom < 1e-9 {
        return 0.0;
    }
    (covariance / denom) as f32
}

fn ds046_rms(values: &[f32]) -> f32 {
    (values.iter().map(|v| v * v).sum::<f32>() / values.len().max(1) as f32).sqrt()
}

// Block-average low-pass on the equirect grid; block size k in grid texels.
fn ds046_low_pass(grid_condensate: &[f32], width: usize, height: usize, k: usize) -> Vec<f32> {
    if k <= 1 {
        return grid_condensate.to_vec();
    }
    let mut result = vec![0.0; width * height];
    for y in 0..height {
        for x in 0..width {
            let mut sum = 0.0;
            let mut count = 0u32;
            for dy in 0..k {
                let sy = (y + dy).min(height - 1);
                for dx in 0..k {
                    let sx = (x + dx) % width;
                    sum += grid_condensate[sy * width + sx];
                    count += 1;
                }
            }
            result[y * width + x] = sum / count as f32;
        }
    }
    result
}

#[allow(clippy::too_many_arguments)]
fn run_ds046_a_metrics(
    gpu: &GpuContext,
    weather_pipeline: &WeatherFieldPipeline,
    wind_pipeline: &WindFieldPipeline,
    scene: &WeatherScene,
    seed: u32,
    generation_samples_ms: &mut Vec<f64>,
    output_dir: &str,
) -> Vec<String> {
    let mut failures = Vec::new();
    let resolution = scene.dynamics.resolution;
    let radius_km = scene.derived.radius_km.max(1.0);

    // === DS-046 A1, re-specified per the DS-046 ruling (2026-08-24). The
    // pre-re-specification raw-area gate (largest habitable-band cloud-free
    // region ≤10% of sphere) is demoted to telemetry; it failed for
    // physics-correct reasons (Hadley subtropical dry belts are modeled
    // physics, not the continent-mask defect). The new PASS set:
    //   A1a coastline decoupling: coastline-correlation score of the largest
    //       connected cloud-free region < T_COAST at wind_scale min (0.25) and
    //       max (4.0, RV-003 #3). Promotes the existing DS-045 detector
    //       (`ds045_no_coastline_correlated_boundary_across_wind_sweep`) from
    //       advisory unit-test scope to this gating real-scene metric.
    //       RV-002 re-specification: the free region is grown with the
    //       subtropical subsidence belt (|lat| 20..=35°) excluded — Hadley dry
    //       belts are modeled physics, not the coastline-tracing defect.
    //       T_COAST calibrated once as 50% of the measured score on the
    //       FE-081-era build (commit 882b7cb) at these same eval points
    //       (see the constant's calibration note below).
    //   A1b land-cell cloud fraction ≥0.25 at ws ≥ 1.0 and ≥ DS046_A1B_T_LOW_WIND
    //       (0.15 floor) at ws=0.25 — RV-002 re-specification: low-wind land
    //       cloud is local physics, flat with coastal distance (measured run 6),
    //       so the full target is only physical where transport reach within the
    //       pinned horizon suffices. See DS046_A1B_T_LOW_WIND comment.
    //   A1c catastrophe backstop: largest connected cloud-free region over the
    //       RAW sphere (no warm-band or any exclusions) ≤0.35 of surface at
    //       wind_scale max.
    println!(
        "DS-046 A1 structure gates (A1a coast decoupling / A1b land cloud / A1c raw backstop):"
    );
    let mut ws2_mass = None;
    let mut max_wind_void_telemetry = None;
    // wind_field.wgsl does not consume wind_scale. Reuse the scene dynamics;
    // WeatherSnapshot.wind_scale changes transport and forcing only.
    // RV-003 #3: the validated gate range is [0.25, 4.0]; ws=4.0 is a full
    // gated eval point (A1a max-wind, A1b target, A1c backstop), no longer a
    // telemetry-only excursion.
    for wind_scale in [0.25_f32, 1.0, 2.0, 4.0] {
        let (weather, generation_ms) = time_gpu_call(gpu, || {
            generate_validation_weather_at_wind_scale(
                weather_pipeline,
                gpu,
                scene,
                &scene.dynamics,
                seed,
                wind_scale,
            )
        });
        generation_samples_ms.push(generation_ms);
        let mass = weather.read_mass(gpu);
        if wind_scale == 2.0 {
            ws2_mass = Some(mass.clone());
        }
        let grid = ds046_sample_grid(scene, &mass, resolution);
        // RV-002: A1a excludes the subtropical subsidence belt from free-region
        // growth (Hadley dry belts are modeled physics, not the defect).
        let (free_fraction, free_members) = ds046_largest_free_region(&grid, true, true);
        let coast_score = ds046_coastline_correlation(&grid, &free_members);
        // RV-003: dump equirect cloud maps at the high-wind points so the
        // extended range can be compared visually.
        if wind_scale >= 2.0 {
            ds046_dump_cloud_map_ppm(
                &grid,
                &format!("{output_dir}/ds046_cloud_map_ws{wind_scale:.0}.ppm"),
            );
        }
        let land_total: f64 = (0..grid.width * grid.height)
            .filter(|&i| grid.land[i] && grid.warm[i])
            .map(|i| grid.weight(i))
            .sum();
        let land_cloud: f64 = (0..grid.width * grid.height)
            .filter(|&i| grid.land[i] && grid.warm[i] && grid.occupied[i])
            .map(|i| grid.weight(i))
            .sum();
        let land_cloud_fraction = (land_cloud / land_total.max(f64::EPSILON)) as f32;
        println!(
            "  wind_scale={wind_scale:.2}: largest_free(habitable)={:.1}% land_cloud(warm)={:.1}% coast_corr={coast_score:.3} (telemetry / A1b ≥ target / A1a <{DS046_A1A_T_COAST:.3} at ws=min,max)",
            free_fraction * 100.0,
            land_cloud_fraction * 100.0,
        );
        if (wind_scale == 0.25 || wind_scale == 4.0) && coast_score >= DS046_A1A_T_COAST {
            failures.push(format!(
                "DS-046 A1a wind_scale={wind_scale}: coastline-correlation score {coast_score:.3} >= T_coast {DS046_A1A_T_COAST:.3}"
            ));
        }
        // RV-002 A1b re-specification: wind-dependent target (see
        // DS046_A1B_T_LOW_WIND for the measured rationale). RV-003 #3: gated
        // across the full validated range [0.25, 4.0] — ws=4 measured 51.9%
        // land_cloud(warm), well above the 0.25 target (target/val-fe090-run.log).
        let a1b_target = if wind_scale < 0.75 {
            DS046_A1B_T_LOW_WIND
        } else {
            0.25
        };
        if land_cloud_fraction < a1b_target {
            failures.push(format!(
                "DS-046 A1b wind_scale={wind_scale}: land cloud fraction {land_cloud_fraction:.3} < target {a1b_target:.2}"
            ));
        }
        // RV-003 #3: A1c backstop evaluated at the extended max wind (4.0).
        if wind_scale == 4.0 {
            let (raw_free_fraction, _) = ds046_largest_free_region(&grid, false, false);
            let (lower_hemisphere_occupancy, lower_hemisphere_condensate_mean) =
                ds046_lower_hemisphere_telemetry(&grid);
            max_wind_void_telemetry = Some(format!(
                "wind_scale=4 lower_hemisphere_occupancy={lower_hemisphere_occupancy:.5}\nlower_hemisphere_condensate_mean={lower_hemisphere_condensate_mean:.5}"
            ));
            println!(
                "  DS-046 A1c raw cloud-free region at wind_scale=4: {:.1}% of sphere (backstop ≤35%)",
                raw_free_fraction * 100.0
            );
            if raw_free_fraction > 0.35 {
                failures.push(format!(
                    "DS-046 A1c: raw largest cloud-free region {raw_free_fraction:.3} > 0.35 of sphere at wind_scale=4"
                ));
            }
        }
    }

    if let Some(metrics) = max_wind_void_telemetry {
        std::fs::write(
            Path::new(output_dir).join("ds046_max_wind_void_metrics.txt"),
            metrics,
        )
        .expect("write DS-046 max-wind void telemetry");
    }

    // A2 micro-texture on the wind_scale=2 field.
    if let Some(mass) = &ws2_mass {
        let grid = ds046_sample_grid(scene, mass, resolution);
        let mean = grid.condensate.iter().sum::<f32>() / grid.condensate.len() as f32;
        let centered: Vec<f32> = grid.condensate.iter().map(|v| v - mean).collect();
        let rms_total = ds046_rms(&centered);
        let texel_km = radius_km * std::f32::consts::PI / grid.width as f32;
        let k = ((1500.0 / texel_km).round() as usize).max(1);
        let low = ds046_low_pass(&grid.condensate, grid.width, grid.height, k);
        let high_pass: Vec<f32> = grid
            .condensate
            .iter()
            .zip(&low)
            .map(|(c, l)| c - l)
            .collect();
        let rms_high = ds046_rms(&high_pass);
        let k2 = (k / 2).max(1);
        let octave_low = ds046_low_pass(&high_pass, grid.width, grid.height, k2);
        let octave_band: Vec<f32> = high_pass
            .iter()
            .zip(&octave_low)
            .map(|(c, l)| c - l)
            .collect();
        let rms_octave = ds046_rms(&octave_band);
        println!(
            "DS-046 A2 micro-texture (wind_scale=2): high-pass(<{:.0}km) RMS={rms_high:.4} vs total {rms_total:.4} ({:.1}%); sub-octave share {:.1}%",
            k as f32 * texel_km,
            100.0 * rms_high / rms_total.max(f32::EPSILON),
            100.0 * rms_octave / rms_high.max(f32::EPSILON),
        );
        if rms_high < 0.15 * rms_total {
            failures.push(format!(
                "DS-046 A2: high-pass condensate RMS {rms_high:.4} < 15% of total {rms_total:.4}"
            ));
        }
        if rms_octave < 0.25 * rms_high {
            failures.push(format!(
                "DS-046 A2: sub-octave energy {rms_octave:.4} < 25% of high-pass {rms_high:.4} (single-scale texture)"
            ));
        }

        // A3 anisotropy at wind_scale=2 using the CPU dynamics readback.
        let wind_cpu = wind_pipeline.generate(
            gpu,
            &scene.terrain,
            resolution,
            seed,
            scene.ocean_level,
            scene.tilt_rad,
            0.5,
            planet_gen::terrain_compute::earth_relative_rotation_rate(
                scene.derived.rotation_rate_rad_s,
            ),
            scene.derived.base_temperature_c,
            scene.derived.surface_pressure_bar,
            2.0,
        );
        let wind_data = native_default_wind_data(&wind_cpu);
        let cube_texel_km = radius_km * std::f32::consts::FRAC_PI_2 / resolution as f32;
        let mut along_pairs: Vec<(f32, f32)> = Vec::new();
        let mut cross_pairs: Vec<(f32, f32)> = Vec::new();
        let res_i = resolution as usize;
        let max_lag_texels = ((800.0 / cube_texel_km).ceil() as usize + 2).min(res_i / 4);
        let condensate_at = |face: usize, x: isize, y: isize| -> f32 {
            let (face, x, y) =
                if (0..res_i as isize).contains(&x) && (0..res_i as isize).contains(&y) {
                    (face, x, y)
                } else {
                    let direction = planet_gen::cube_sphere::cube_to_sphere(
                        face as u32,
                        (x as f32 + 0.5) / res_i as f32,
                        (y as f32 + 0.5) / res_i as f32,
                    );
                    let (face, fx, fy) = u3_cube_coordinates(direction, resolution);
                    (
                        face,
                        (fx.round() as isize).clamp(0, res_i as isize - 1),
                        (fy.round() as isize).clamp(0, res_i as isize - 1),
                    )
                };
            let index = (face * res_i * res_i + y as usize * res_i + x as usize) * 4;
            mass[index] + mass[index + 1]
        };
        for face in 0..6usize {
            let margin = max_lag_texels as isize + 2;
            let mut y = margin;
            while y < res_i as isize - margin {
                let mut x = margin;
                while x < res_i as isize - margin {
                    let direction = planet_gen::cube_sphere::cube_to_sphere(
                        face as u32,
                        x as f32 / (res_i - 1) as f32,
                        y as f32 / (res_i - 1) as f32,
                    );
                    let wind = [
                        u3_linear_cube_sample(&wind_data, resolution, direction, 0),
                        u3_linear_cube_sample(&wind_data, resolution, direction, 1),
                        u3_linear_cube_sample(&wind_data, resolution, direction, 2),
                    ];
                    let radial =
                        wind[0] * direction[0] + wind[1] * direction[1] + wind[2] * direction[2];
                    let tangent = [
                        wind[0] - radial * direction[0],
                        wind[1] - radial * direction[1],
                        wind[2] - radial * direction[2],
                    ];
                    let speed = (tangent[0] * tangent[0]
                        + tangent[1] * tangent[1]
                        + tangent[2] * tangent[2])
                        .sqrt();
                    if speed > 0.01 {
                        let along = [tangent[0] / speed, tangent[1] / speed, tangent[2] / speed];
                        let cross_raw = [
                            direction[1] * along[2] - direction[2] * along[1],
                            direction[2] * along[0] - direction[0] * along[2],
                            direction[0] * along[1] - direction[1] * along[0],
                        ];
                        let cross_len = (cross_raw[0] * cross_raw[0]
                            + cross_raw[1] * cross_raw[1]
                            + cross_raw[2] * cross_raw[2])
                            .sqrt()
                            .max(1e-6);
                        // Map tangent directions to face-space offsets via the
                        // local cube-face Jacobian: finite-difference the face
                        // parameterization once per sample point.
                        let step_dir = |dir: [f32; 3], n: f32| -> (isize, isize) {
                            let eps = 1.0 / res_i as f32;
                            let center_uv = crate_face_uv(direction);
                            let plus = crate_face_uv(normalize3([
                                direction[0] + dir[0] * eps,
                                direction[1] + dir[1] * eps,
                                direction[2] + dir[2] * eps,
                            ]));
                            let du = (plus.0 - center_uv.0) / eps;
                            let dv = (plus.1 - center_uv.1) / eps;
                            let len = (du * du + dv * dv).sqrt().max(1e-6);
                            (
                                (du / len * n).round() as isize,
                                (dv / len * n).round() as isize,
                            )
                        };
                        let center_value = condensate_at(face, x, y);
                        for lag_km in [300.0_f32, 800.0] {
                            let n = ((lag_km / cube_texel_km).round() as isize).max(2);
                            let (ox, oy) = step_dir(along, n as f32);
                            along_pairs.push((center_value, condensate_at(face, x + ox, y + oy)));
                            let (cx, cy) = step_dir(
                                [
                                    cross_raw[0] / cross_len,
                                    cross_raw[1] / cross_len,
                                    cross_raw[2] / cross_len,
                                ],
                                n as f32,
                            );
                            cross_pairs.push((center_value, condensate_at(face, x + cx, y + cy)));
                        }
                    }
                    x += 7;
                }
                y += 7;
            }
        }
        let correlation = |pairs: &[(f32, f32)]| -> Option<f32> {
            if pairs.len() < 64 {
                return None;
            }
            let mean_a = pairs.iter().map(|p| p.0).sum::<f32>() / pairs.len() as f32;
            let mean_b = pairs.iter().map(|p| p.1).sum::<f32>() / pairs.len() as f32;
            let cov: f32 = pairs.iter().map(|p| (p.0 - mean_a) * (p.1 - mean_b)).sum();
            let var_a: f32 = pairs.iter().map(|p| (p.0 - mean_a).powi(2)).sum();
            let var_b: f32 = pairs.iter().map(|p| (p.1 - mean_b).powi(2)).sum();
            let denom = (var_a * var_b).sqrt();
            (denom > 1e-9).then(|| cov / denom)
        };
        let rho_along = correlation(&along_pairs);
        let rho_cross = correlation(&cross_pairs);
        match (rho_along, rho_cross) {
            (Some(a), Some(c)) => {
                let ratio = a.abs() / c.abs().max(1e-6);
                println!(
                    "DS-046 A3 streak anisotropy (wind_scale=2): ρ_along={a:.3} ρ_cross={c:.3} ratio={ratio:.2} (gate ≤3.0, n={})",
                    along_pairs.len()
                );
                if ratio > 3.0 {
                    failures.push(format!(
                        "DS-046 A3: along/cross autocorrelation ratio {ratio:.2} > 3.0"
                    ));
                }
            }
            _ => failures
                .push("DS-046 A3: insufficient pair samples for autocorrelation".to_string()),
        }
    }
    failures
}

fn normalize3(v: [f32; 3]) -> [f32; 3] {
    let len = (v[0] * v[0] + v[1] * v[1] + v[2] * v[2]).sqrt().max(1e-9);
    [v[0] / len, v[1] / len, v[2] / len]
}

// Face uv (unit-scale; only differences matter) for a sphere direction.
fn crate_face_uv(direction: [f32; 3]) -> (f32, f32) {
    let (_, x, y) = u3_cube_coordinates(direction, 1);
    (x, y)
}

fn generate_validation_weather_at_wind_scale(
    pipeline: &WeatherFieldPipeline,
    gpu: &GpuContext,
    scene: &WeatherScene,
    dynamics: &DynamicsTextures,
    seed: u32,
    wind_scale: f32,
) -> WeatherTextures {
    let weather = pipeline.create_textures(gpu, scene.dynamics.resolution);
    pipeline.generate(
        gpu,
        WeatherSnapshot {
            face: 0,
            resolution: scene.dynamics.resolution,
            seed,
            storm_count: 4,
            coverage: 0.5,
            moisture: 1.0,
            surface_pressure_bar: scene.derived.surface_pressure_bar,
            base_temp_c: scene.derived.base_temperature_c,
            ocean_level: scene.ocean_level,
            axial_tilt_rad: scene.tilt_rad,
            season: 0.5,
            storm_size: 1.0,
            radius_km: scene.derived.radius_km,
            rotation_rate_rad_s: scene.derived.rotation_rate_rad_s,
            wind_scale,
        },
        &scene.terrain,
        dynamics,
        &weather,
    );
    weather
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
    // Perf (queue p95) failures are collected separately so
    // PLANET_GEN_IGNORE_PERF_GATES=1 can downgrade them to reported-only
    // (documented environmental flakiness); correctness gates stay fatal.
    let mut perf_gate_failures = Vec::new();
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
        "weather-validation",
        &U15_VALIDATION_SEEDS,
        false,
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
    gate_failures.extend(run_u3_local_validation(
        gpu,
        &weather_pipeline,
        &wind_pipeline,
        &scene,
        base_uniforms,
        output_dir,
        render_size,
    ));

    let mut generation_samples_ms = Vec::new();
    let mut render_samples_ms = Vec::new();
    let mut u3_cloud_render_samples_ms = Vec::new();
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
    gate_failures.extend(run_earth_polar_validation(
        &seed_42_storm_4.read_mass(gpu),
        seed_42_storm_4.resolution,
        scene.tilt_rad,
        output_dir,
    ));
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
        let geometry = weather.read_geometry(gpu);
        u3_mass_reports.push(u3_mass_report("cloud", label, &mass, weather.resolution));
        let uniforms = PreviewUniforms {
            cloud_seed,
            ..base_uniforms
        };
        let ray_context = U3RayContext {
            mass: &mass,
            geometry: &geometry,
            resolution: weather.resolution,
            rotation: uniforms.rotation,
            radius_km: uniforms.planet_radius_km,
            pan: [uniforms.pan_x, uniforms.pan_y],
            zoom: uniforms.zoom,
            cloud_seed: uniforms.cloud_seed,
        };
        let (pixels, render_ms) = time_gpu_call(gpu, || {
            render_weather(renderer, gpu, &uniforms, &scene, weather, render_size)
        });
        render_samples_ms.push(render_ms);
        u3_cloud_render_samples_ms.push(render_ms);
        let association = u3_rendered_association("cloud", label, &pixels, &ray_context);
        let support = u3_rendered_mass_association(&pixels, &ray_context);
        if support.support < 0.90 {
            gate_failures.push(format!(
                "U3 cloud seed {label} rendered mass association={:.3} < 0.900",
                support.support,
            ));
        }
        if support.zero_mass_opaque != 0 {
            gate_failures.push(format!(
                "U3 cloud seed {label} zero_mass_opaque={} != 0",
                support.zero_mass_opaque,
            ));
            if let Some(diagnostic) = &support.first_failure {
                gate_failures.push(format!(
                    "U3 cloud seed {label} first zero-mass opaque: {diagnostic}"
                ));
            }
        }
        u3_rendered_associations.push(format!(
            "{association} mass_association={:.3} zero_mass_opaque={} first_failure={}",
            support.support,
            support.zero_mass_opaque,
            support.first_failure.as_deref().unwrap_or("none"),
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
        let topology_failures = validate_seed_topology_metrics(&topology, seed_change);
        if !topology_failures.is_empty() {
            gate_failures.push(format!(
                "Cloud seed {label}: {}",
                topology_failures.join("; ")
            ));
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
        let geometry = weather.read_geometry(gpu);
        u3_mass_reports.push(u3_mass_report("global", label, &mass, weather.resolution));
        let ray_context = U3RayContext {
            mass: &mass,
            geometry: &geometry,
            resolution: weather.resolution,
            rotation: base_uniforms.rotation,
            radius_km: base_uniforms.planet_radius_km,
            pan: [base_uniforms.pan_x, base_uniforms.pan_y],
            zoom: base_uniforms.zoom,
            cloud_seed: base_uniforms.cloud_seed,
        };
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
        let association = u3_rendered_association("global", label, &pixels, &ray_context);
        let support = u3_rendered_mass_association(&pixels, &ray_context);
        if support.support < 0.90 {
            gate_failures.push(format!(
                "U3 global seed {label} rendered mass association={:.3} < 0.900",
                support.support,
            ));
        }
        if support.zero_mass_opaque != 0 {
            gate_failures.push(format!(
                "U3 global seed {label} zero_mass_opaque={} != 0",
                support.zero_mass_opaque,
            ));
            if let Some(diagnostic) = &support.first_failure {
                gate_failures.push(format!(
                    "U3 global seed {label} first zero-mass opaque: {diagnostic}"
                ));
            }
        }
        u3_rendered_associations.push(format!(
            "{association} mass_association={:.3} zero_mass_opaque={} first_failure={}",
            support.support,
            support.zero_mass_opaque,
            support.first_failure.as_deref().unwrap_or("none"),
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
        let topology_failures = validate_seed_topology_metrics(&topology, None);
        if !topology_failures.is_empty() {
            gate_failures.push(format!(
                "Global seed {label}: {}",
                topology_failures.join("; ")
            ));
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
            "mass bins=[0,.01),[.01,.02),[.02,.03),[.03,.04),[.04,.05),[.05,.06),[.06,.07),[.07,.08),[.08,.09),[.09,.10),[.10,.11),[.11,.12),[.12,.13),[.13,.14),[.14,.15),[.15,+)\nfield weights=solid-angle; polar=geographic abs(latitude)>=60 degrees\nrendered opacity=linearized integrated density view; projected association mirrors preview NDC pan/zoom, eight centered rays with 0.5±.005 jitter averaging, and seam-aware linear cubemap taps; mass_association>=0.900\n\nFIELD DISTRIBUTIONS\n{}\n\nRENDERED ASSOCIATIONS\n{}\n",
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
        "  U3 Count 0->8 rendered optical-depth response={count_tau_0:.5}->{count_tau_8:.5}, delta={:.2}% ({})",
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

    println!("DS-046 A1–A3 weather structure gates:");
    // Timed separately: the queue-stall gate measures production-path
    // generations, and the wind_scale sweep adds extra substep-heavy passes.
    let mut ds046_generation_samples_ms = Vec::new();
    gate_failures.extend(run_ds046_a_metrics(
        gpu,
        &weather_pipeline,
        &wind_pipeline,
        &scene,
        42,
        &mut ds046_generation_samples_ms,
        output_dir,
    ));
    let ds046_stats = compute_runtime_stats(ds046_generation_samples_ms);
    if ds046_stats.count > 0 {
        println!(
            "  DS-046 fixture generation (ms): n={} p95={:.3} min={:.3} max={:.3} mean={:.3}",
            ds046_stats.count,
            ds046_stats.p95_ms,
            ds046_stats.min_ms,
            ds046_stats.max_ms,
            ds046_stats.mean_ms
        );
    }

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
    let u3_cloud_render_stats = compute_runtime_stats(u3_cloud_render_samples_ms);
    // Queue p95 checks are performance, not correctness: they go to
    // perf_gate_failures so PLANET_GEN_IGNORE_PERF_GATES=1 can skip them
    // (see the ignore handling before the status report below).
    if generation_stats.p95_ms > WEATHER_GENERATION_QUEUE_STALL_P95_MS {
        perf_gate_failures.push(format!(
            "GPU generation p95 {:.3}ms exceeds {:.1}ms queue-stall gate",
            generation_stats.p95_ms, WEATHER_GENERATION_QUEUE_STALL_P95_MS
        ));
    }
    if render_stats.p95_ms > 33.3 {
        perf_gate_failures.push(format!(
            "GPU render p95 {:.3}ms exceeds 33.3ms queue-stall gate",
            render_stats.p95_ms
        ));
    }
    if u3_cloud_render_stats.p95_ms > 33.3 {
        perf_gate_failures.push(format!(
            "U3 cloud-enabled render fixture p95 {:.3}ms exceeds 33.3ms",
            u3_cloud_render_stats.p95_ms
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
    println!(
        "  U3 cloud-enabled render fixture (ms): n={} p95={:.3} min={:.3} max={:.3} mean={:.3}",
        u3_cloud_render_stats.count,
        u3_cloud_render_stats.p95_ms,
        u3_cloud_render_stats.min_ms,
        u3_cloud_render_stats.max_ms,
        u3_cloud_render_stats.mean_ms,
    );

    // Opt-in perf-gate ignore: PLANET_GEN_IGNORE_PERF_GATES=1 downgrades
    // queue p95 failures to reported-only. Intended for known environmental
    // slowdowns (measured 410-545 ms generation floor vs 32.9 ms baseline on
    // the same box with bit-identical code); perf remains fatal by default.
    // Correctness gates (including parked U15) are never ignored.
    let ignore_perf_gates = std::env::var("PLANET_GEN_IGNORE_PERF_GATES").as_deref() == Ok("1");
    if ignore_perf_gates {
        if perf_gate_failures.is_empty() {
            println!("  Perf gates (queue p95): no failures recorded.");
        } else {
            println!(
                "  Perf gate failures ({}): ignored via PLANET_GEN_IGNORE_PERF_GATES=1",
                perf_gate_failures.len()
            );
            for failure in &perf_gate_failures {
                println!("    {failure}");
            }
            println!(
                "  Perf gates are still failing; re-run without the env var before declaring perf green."
            );
        }
    } else {
        gate_failures.extend(perf_gate_failures);
    }

    if gate_failures.is_empty() {
        println!("U12 status: IMPLEMENTED, COMPLETE");
        println!("  Automated eight-seed topology, morphology, and storm-control gates passed.");
        if ignore_perf_gates {
            println!(
                "  The 512px automated gates and wind reversal passed; queue p95 gates were ignored via PLANET_GEN_IGNORE_PERF_GATES=1."
            );
        } else {
            println!("  The 512px automated gates, wind reversal, and queue p95 gates passed.");
        }
        println!("  Visual review is separate and was not performed by this command.");
    } else {
        println!("U12 status: IMPLEMENTED");
        println!("  Automated gate failures ({}):", gate_failures.len());
        for failure in &gate_failures {
            println!("    {failure}");
        }
    }
    assert!(
        gate_failures.is_empty(),
        "weather validation failed {} automated gate(s)",
        gate_failures.len()
    );
}

fn run_native_default_cloud_validation(
    gpu: &GpuContext,
    compute: &TerrainComputePipeline,
    renderer: &PreviewRenderer,
    output_dir: &str,
    render_size: u32,
) {
    let wind_pipeline = WindFieldPipeline::new(gpu).expect("Rgba16Float dynamics unsupported");
    let weather_pipeline = WeatherFieldPipeline::new(gpu).expect("Rgba16Float weather unsupported");
    let scene = generate_native_default_scene(gpu, compute, renderer, &wind_pipeline, render_size);
    let weather = generate_validation_weather(&weather_pipeline, gpu, &scene, 1042, 2, 1.0);
    let params = PlanetParams::default();
    let mut uniforms = PreviewUniforms::zeroed();
    uniforms.rotation = [
        [1.0, 0.0, 0.0, 0.0],
        [0.0, 1.0, 0.0, 0.0],
        [0.0, 0.0, 1.0, 0.0],
        [0.0, 0.0, 0.0, 1.0],
    ];
    uniforms.light_dir = [
        (-0.5_f32).cos() * 0.3_f32.cos(),
        0.3_f32.sin(),
        (-0.5_f32).sin() * 0.3_f32.cos(),
    ];
    uniforms.ocean_level = scene.ocean_level;
    uniforms.base_temp_c = scene.derived.base_temperature_c;
    uniforms.ocean_fraction = scene.derived.ocean_fraction;
    uniforms.axial_tilt_rad = params.axial_tilt_deg.to_radians();
    uniforms.season = 0.5;
    uniforms.atmosphere_density = scene.derived.surface_pressure_bar;
    uniforms.atmosphere_height = scene.derived.atmosphere_shell_height();
    uniforms.height_scale = 3.0;
    uniforms.zoom = 1.0;
    uniforms.cloud_coverage = 0.5;
    uniforms.cloud_seed = 1042;
    uniforms.star_color_temp = 0.5;
    uniforms.show_ao = 1.0;
    uniforms.show_water = 1.0;
    uniforms.show_ice = 1.0;
    uniforms.show_biomes = 1.0;
    uniforms.show_clouds = 1.0;
    uniforms.show_atmosphere_layer = 1.0;
    uniforms.show_cities = 1.0;
    uniforms.cloud_opacity = 1.0;
    uniforms.cloud_advection = 1.0;
    uniforms.rotation_rate = scene.derived.rotation_rate_rad_s / (std::f32::consts::TAU / 86400.0);
    uniforms.atm_pressure = scene.derived.surface_pressure_bar;
    uniforms.planet_radius_km = scene.derived.radius_km;
    uniforms.show_cloud_shadows = 1.0;

    let clouds_off = PreviewUniforms {
        show_clouds: 0.0,
        ..uniforms
    };
    let color_off = render_weather(renderer, gpu, &clouds_off, &scene, &weather, render_size);
    let color_on = render_weather(renderer, gpu, &uniforms, &scene, &weather, render_size);
    let density = render_weather(
        renderer,
        gpu,
        &PreviewUniforms {
            view_mode: 9,
            ..uniforms
        },
        &scene,
        &weather,
        render_size,
    );
    let (land, lowland) = native_default_land_masks(&scene, &uniforms, render_size);
    save_png(
        output_dir,
        "native_default_clouds_off.png",
        render_size,
        &color_off,
    );
    save_png(
        output_dir,
        "native_default_clouds_on.png",
        render_size,
        &color_on,
    );
    save_png(
        output_dir,
        "native_default_density.png",
        render_size,
        &density,
    );
    save_png(
        output_dir,
        "native_default_land_mask.png",
        render_size,
        &native_default_mask_png(&land),
    );
    save_png(
        output_dir,
        "native_default_land_delta.png",
        render_size,
        &native_default_delta_png(&color_on, &color_off, &land),
    );
    std::fs::write(
        Path::new(output_dir).join("native_default_metrics.txt"),
        format!(
            "command=cargo run --release --features validation --bin sweep -- --native-default-cloud-validation --size {render_size} --output-dir {output_dir}\nseed=42\ncloud_seed=1042\ncoverage=0.5\nmoisture=1\nstorm_count=2\nstorm_size=1\nwind_scale=1\nwater_loss=0.5\ncontinental_scale=1\nnum_continents=4\ncontinent_size_variety=0.35\nocean_level_formula=-0.5+1.7*(derived.ocean_fraction*(1-water_loss))\nocean_level={:.5}\n\n{}\n{}",
            scene.ocean_level,
            native_default_metrics(&density, &color_on, &color_off, &lowland, &land, render_size),
            native_default_source_report(
                gpu,
                &wind_pipeline,
                &scene,
                &uniforms,
                render_size,
            ),
        ),
    )
    .expect("write native default metrics artifact");
}

fn main() {
    env_logger::init();
    let args: Vec<String> = std::env::args().collect();
    let u15_mode = match u15_validation_mode(&args) {
        Ok(mode) => mode,
        Err(error) => {
            eprintln!("error: {error}");
            std::process::exit(2);
        }
    };

    let output_dir = args
        .iter()
        .skip_while(|a| a.as_str() != "--output-dir")
        .nth(1)
        .cloned()
        .unwrap_or_else(|| "output/sweep".to_string());

    let render_size: u32 = args
        .iter()
        .skip_while(|a| a.as_str() != "--size")
        .nth(1)
        .and_then(|s| s.parse().ok())
        .unwrap_or(512);
    let weather_validation = args.iter().any(|arg| arg == "--weather-validation");
    let u15_validation = u15_mode.is_some();
    let u3_validation = args.iter().any(|arg| arg == "--u3-validation");
    let native_default_cloud_validation = args
        .iter()
        .any(|arg| arg == "--native-default-cloud-validation");
    if requires_weather_validation_size(
        weather_validation,
        u15_validation,
        u3_validation,
        native_default_cloud_validation,
    ) && let Some(error) = weather_validation_size_error(render_size)
    {
        eprintln!("error: {error}");
        std::process::exit(2);
    }

    let seeds = [42, 137, 256, 999, 7777];
    let planet_presets = presets();

    if weather_validation || u15_validation || native_default_cloud_validation {
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

    if native_default_cloud_validation {
        run_native_default_cloud_validation(&gpu, &compute, &renderer, &output_dir, render_size);
        println!("Native-default cloud evidence saved to {output_dir}/");
        return;
    }

    if let Some(mode) = u15_mode {
        let weather_pipeline =
            WeatherFieldPipeline::new(&gpu).expect("Rgba16Float weather unsupported");
        let (validation_flag, selected_seeds, persist_per_seed) = match mode {
            U15ValidationMode::Legacy => ("u15-validation", U15_VALIDATION_SEEDS.to_vec(), false),
            U15ValidationMode::Batch => {
                ("u15-validation-batch", U15_VALIDATION_SEEDS.to_vec(), true)
            }
            U15ValidationMode::Seed(seed) => ("u15-validation-seed", vec![seed], true),
        };
        let failures = run_u15_field_validation(
            &gpu,
            &weather_pipeline,
            &output_dir,
            render_size.clamp(64, 512),
            validation_flag,
            &selected_seeds,
            persist_per_seed,
        );
        assert!(failures.is_empty(), "U15 validation failed: {failures:?}");
        println!("U15 focused fixture passed.");
        return;
    }

    if u3_validation {
        let wind_pipeline = WindFieldPipeline::new(&gpu).expect("Rgba16Float dynamics unsupported");
        let weather_pipeline =
            WeatherFieldPipeline::new(&gpu).expect("Rgba16Float weather unsupported");
        let scene = generate_weather_scene(
            &gpu,
            &compute,
            &renderer,
            &wind_pipeline,
            &planet_presets[0],
            42,
            (
                render_size.clamp(128, 512),
                (render_size / 2).clamp(64, 384),
            ),
        );
        let tilt = 0.35_f32;
        let (st, ct) = tilt.sin_cos();
        let mut uniforms = PreviewUniforms::zeroed();
        uniforms.rotation = [
            [1.0, 0.0, 0.0, 0.0],
            [0.0, ct, -st, 0.0],
            [0.0, st, ct, 0.0],
            [0.0, 0.0, 0.0, 1.0],
        ];
        uniforms.light_dir = [0.5, 0.7, -1.0];
        uniforms.ocean_level = scene.ocean_level;
        uniforms.base_temp_c = scene.derived.base_temperature_c;
        uniforms.ocean_fraction = scene.derived.ocean_fraction;
        uniforms.axial_tilt_rad = planet_presets[0].params.axial_tilt_deg.to_radians();
        uniforms.season = 0.5;
        uniforms.height_scale = 3.0;
        uniforms.zoom = 1.0;
        uniforms.cloud_coverage = 0.5;
        uniforms.cloud_seed = 42;
        uniforms.star_color_temp = 0.5;
        uniforms.show_ao = 1.0;
        uniforms.show_water = 1.0;
        uniforms.show_ice = 1.0;
        uniforms.show_biomes = 1.0;
        uniforms.show_clouds = 1.0;
        uniforms.cloud_opacity = 1.0;
        uniforms.cloud_advection = 1.0;
        uniforms.rotation_rate = 1.0;
        uniforms.atm_pressure = scene.derived.surface_pressure_bar;
        uniforms.planet_radius_km = scene.derived.radius_km;
        uniforms.show_cloud_shadows = 1.0;
        let failures = run_u3_local_validation(
            &gpu,
            &weather_pipeline,
            &wind_pipeline,
            &scene,
            uniforms,
            &output_dir,
            render_size,
        );
        assert!(failures.is_empty(), "U3 validation failed: {failures:?}");
        println!("U3 focused local validation passed.");
        return;
    }

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
        TopologyMetrics, U14CoverageComponent, U15_DEEP_THRESHOLD, U15_SIZE_FULL_SUPPORT_RADIUS,
        U15_SIZE_SEEDS, U15_SIZE_SUPPORT_RADIUS, U15PlumeMetrics, U15ResponseComponent,
        U15ValidationMode, WEATHER_DIAGNOSTIC_NO_SOURCE, WEATHER_GENERATION_QUEUE_STALL_P95_MS,
        cube_edge_pairs, field_view_pixels, polar_metrics, requires_weather_validation_size,
        u3_cube_coordinates, u3_feature_axis, u3_linear_cube_sample, u3_mass_association,
        u3_ray_ndc, u3_screen_direction, u14_coverage_growth, u14_coverage_support_metrics,
        u14_fixed_core_p90, u14_geometry_metrics, u14_lee_continuation_metrics,
        u14_marine_to_land_continentality, u14_mixed_coast_land_support,
        u14_significant_occupied_components, u15_fixture_centers,
        u15_owner_primary_provenance_share, u15_owner_size_metrics, u15_paired_size_tops,
        u15_pixel_neighbors, u15_pixel_position, u15_seed_diagnostics, u15_seed_diagnostics_report,
        u15_significant_response_components, u15_significant_size_components, u15_size_association,
        u15_size_fixture_support, u15_size_frozen_candidates,
        u15_size_minimum_physical_eligibility, u15_size_precondition_diagnostics,
        u15_size_seed_criterion, u15_size_weather_snapshot, u15_validation_mode, u15_weight,
        validate_seed_topology_metrics, weather_validation_size_error,
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
    fn u15_harness_modes_accept_frozen_seed_and_reject_ambiguous_or_unknown_modes() {
        let args = |values: &[&str]| {
            values
                .iter()
                .map(|value| (*value).to_string())
                .collect::<Vec<_>>()
        };
        assert_eq!(
            u15_validation_mode(&args(&["sweep", "--u15-validation-batch"])),
            Ok(Some(U15ValidationMode::Batch))
        );
        assert_eq!(
            u15_validation_mode(&args(&["sweep", "--u15-validation-seed", "73"])),
            Ok(Some(U15ValidationMode::Seed(73)))
        );
        assert!(u15_validation_mode(&args(&["sweep", "--u15-validation-seed", "8"])).is_err());
        assert!(
            u15_validation_mode(&args(&[
                "sweep",
                "--u15-validation-seed",
                "7",
                "--u15-validation-seed",
                "19",
            ]))
            .is_err()
        );
        assert!(
            u15_validation_mode(&args(&[
                "sweep",
                "--u15-validation",
                "--u15-validation-batch"
            ]))
            .is_err()
        );
    }

    #[test]
    fn u15_seed_diagnostics_are_normalized_and_report_all_ds_039_metrics() {
        let resolution = 4;
        let pixels = resolution as usize * resolution as usize * 6;
        let mut mass = vec![0.0; pixels * 4];
        mass[4 + 1] = 0.04;
        mass[8 + 2] = 0.04;
        let response = vec![0.01; pixels];
        let baseline = vec![0.02; pixels * 4];
        let plume = U15PlumeMetrics {
            alongwind_span: 2.0,
            crosswind_span: 1.0,
            isotropic_edge_telemetry: 0.5,
            response_mass: 2.0,
            ..U15PlumeMetrics::default()
        };

        let diagnostics = u15_seed_diagnostics(&baseline, &mass, &response, resolution, &plume);
        let report = u15_seed_diagnostics_report(7, &diagnostics);

        assert!((diagnostics.normalized_high_extent - 1.0 / pixels as f32).abs() < 1.0e-6);
        assert!(diagnostics.high_deep_centroid_displacement_texels.is_some());
        assert!((diagnostics.cross_along_ratio - 0.5).abs() < f32::EPSILON);
        assert!((diagnostics.edge_variation - 0.25).abs() < f32::EPSILON);
        assert!(diagnostics.face_local_column_mass_delta > 0.0);
        for metric in [
            "normalized_high_extent",
            "high_deep_centroid_displacement_texels",
            "high_deep_centroid_angle_degrees",
            "downwind_decay_near",
            "downwind_decay_mid",
            "downwind_decay_far",
            "cross_along_ratio",
            "edge_variation",
            "face_local_column_mass_delta",
        ] {
            assert!(report.contains(metric), "missing {metric}");
        }
    }

    #[test]
    fn validation_artifact_commands_enable_the_validation_feature() {
        let source = include_str!("sweep.rs");
        let legacy_release = ["cargo run --release", "--bin sweep"].join(" ");
        let legacy_debug = ["cargo run", "--bin sweep"].join(" ");
        let validation_release = [
            "cargo run --release",
            "--features validation",
            "--bin sweep",
        ]
        .join(" ");
        assert!(!source.contains(&legacy_release));
        assert!(!source.contains(&legacy_debug));
        assert!(source.contains(&validation_release));
    }

    #[test]
    fn generation_queue_stall_gate_is_rebaselined_without_relaxing_render_gates() {
        let source = include_str!("sweep.rs");

        assert_eq!(WEATHER_GENERATION_QUEUE_STALL_P95_MS, 36.0);
        assert!(
            source.contains("if generation_stats.p95_ms > WEATHER_GENERATION_QUEUE_STALL_P95_MS")
        );
        assert!(source.contains("if render_stats.p95_ms > 33.3"));
        assert!(source.contains("if u3_cloud_render_stats.p95_ms > 33.3"));
        assert!(source.contains("fixture_generation_p95_rebaseline=FE-062"));
    }

    #[test]
    fn native_default_cloud_validation_requires_weather_validation_size() {
        assert!(requires_weather_validation_size(false, false, false, true));
    }

    #[test]
    fn u14_mixed_land_support_excludes_ocean_and_all_inland_diagnostic() {
        let resolution = 8;
        let mut mass = vec![0.0; resolution as usize * resolution as usize * 6 * 4];
        let mut ocean_samples = 0;
        let mut land_samples = 0;
        for face in 0..6 {
            for y in 0..resolution {
                for x in 0..resolution {
                    let position = planet_gen::cube_sphere::cube_to_sphere(
                        face,
                        x as f32 / (resolution - 1) as f32,
                        y as f32 / (resolution - 1) as f32,
                    );
                    let pixel =
                        ((face * resolution * resolution + y * resolution + x) * 4) as usize;
                    if u14_marine_to_land_continentality(position) >= 1.0
                        && position[0] > 0.8
                        && position[2] < -0.04
                        && position[2] > -0.12
                    {
                        mass[pixel] = 0.03;
                        mass[pixel + 3] = 1.0;
                        land_samples += 1;
                    } else {
                        mass[pixel] = 0.9;
                        mass[pixel + 3] = 1.0;
                        ocean_samples += 1;
                    }
                }
            }
        }

        let (occupied, p90) = u14_mixed_coast_land_support(&mass, resolution);
        assert!(ocean_samples > 0 && land_samples > 0);
        assert_eq!(occupied, 1.0);
        assert_eq!(p90, 0.03);
    }

    #[test]
    fn u14_lee_continuation_metrics_measure_only_new_phase_creation() {
        let mask = vec![true, true, false, false];
        let advected = vec![
            0.04, 0.01, 0.0, 0.0, 0.05, 0.0, 0.0, 0.0, 0.90, 0.0, 0.0, 0.0, 0.90, 0.0, 0.0, 0.0,
        ];
        let full = vec![
            0.041, 0.012, 0.001, 0.0, 0.052, 0.0, 0.0, 0.0, 0.90, 0.0, 0.0, 0.0, 0.90, 0.0, 0.0,
            0.0,
        ];

        let metrics = u14_lee_continuation_metrics(&advected, &full, &mask);

        assert!((metrics.advected_total_p90 - 0.05).abs() < 1.0e-6);
        assert!((metrics.full_total_p90 - 0.054).abs() < 1.0e-6);
        assert!((metrics.new_phase_total_p90 - 0.004).abs() < 1.0e-6);
        assert!(
            metrics
                .channel_delta_p90
                .iter()
                .zip([0.002, 0.002, 0.001])
                .all(|(actual, expected)| (actual - expected).abs() < 1.0e-6)
        );
        assert!((metrics.new_phase_fraction - 0.004 / 0.054).abs() < 1.0e-6);
    }

    #[test]
    fn field_views_keep_all_six_cubemap_faces() {
        let resolution = 2;
        let mut mass = vec![0.0; 6 * resolution as usize * resolution as usize * 4];
        let mut geometry = mass.clone();
        mass[0..4].copy_from_slice(&[0.25, 0.5, 0.75, 1.0]);
        geometry[0..4].copy_from_slice(&[0.0, 0.01, 0.02, 0.03]);

        let [formation, mass_view, density] = field_view_pixels(&mass, &geometry, resolution);

        assert_eq!(
            formation.len(),
            3 * resolution as usize * 2 * resolution as usize * 4
        );
        assert_eq!(mass_view.len(), formation.len());
        assert_eq!(density.len(), formation.len());
        assert_eq!(&mass_view[0..4], &[64, 128, 191, 255]);
        assert_eq!(density[3], 255);
    }

    #[test]
    fn u15_shear_wind_is_analytic_tangent_divergence_free_and_bounded() {
        for shear in super::U15_TRAIL_SHEAR_CANDIDATES {
            for y in [-0.95, -0.5, 0.0, 0.5, 0.95] {
                let position = [
                    (1.0_f32 - y * y).sqrt() * 0.6,
                    y,
                    (1.0_f32 - y * y).sqrt() * 0.8,
                ];
                let wind = super::u15_trail_wind(position, shear);
                assert!(super::u15_dot(wind, position).abs() < 1.0e-6);
                assert!(wind[1].abs() < 1.0e-6);
                let speed = super::u15_dot(wind, wind).sqrt();
                assert!(speed <= 1.0 + 1.0e-6);
                assert!(speed / (1.0_f32 - y * y).sqrt() >= (1.0 - shear) / (1.0 + shear) - 1.0e-6);
            }
        }
    }

    #[test]
    fn u15_shear_wind_is_discretely_bounded_and_divergence_free() {
        let step = 1.0e-3;
        for face in 0..6 {
            for y in 1..32 {
                for x in 1..32 {
                    let position = u15_pixel_position(pixel(face, x, y, 33), 33);
                    for shear in super::U15_TRAIL_SHEAR_CANDIDATES {
                        let wind = super::u15_trail_wind(position, shear);
                        assert!(super::u15_dot(wind, wind).sqrt() <= 1.0 + 1.0e-6);
                        let (east, north) = super::u15_size_tangent_basis(position);
                        let derivative = |basis| {
                            let plus = super::u15_geodesic_offset(position, basis, step);
                            let minus = super::u15_geodesic_offset(position, basis, -step);
                            (super::u15_dot(super::u15_trail_wind(plus, shear), basis)
                                - super::u15_dot(super::u15_trail_wind(minus, shear), basis))
                                / (2.0 * step)
                        };
                        assert!((derivative(east) + derivative(north)).abs() < 2.0e-3);
                    }
                }
            }
        }
    }

    #[test]
    fn u15_trail_response_clamps_controlled_mass() {
        let response = super::u15_trail_response(
            &[0.02, 0.03, 0.04, 0.0, 0.01, 0.01, 0.01, 0.0],
            &[0.01, 0.02, 0.03, 0.0, 0.02, 0.01, 0.01, 0.0],
        );
        assert!((response[0] - 0.03).abs() < 1.0e-6);
        assert_eq!(response[1], 0.0);
    }

    #[test]
    fn u3_cirrus_filter_is_symmetric_local_and_geodesic() {
        let shader = include_str!("../shaders/cloud_density.wgsl");
        assert!(
            shader.contains("let forward = normalize(direction * cos(step) + along * sin(step));")
        );
        assert!(
            shader.contains("let backward = normalize(direction * cos(step) - along * sin(step));")
        );
        assert!(shader.contains("let amount = clamp(stretch - 1.0, 0.0, 1.0);"));
        assert!(shader.contains("let step = clamp(0.60 / frequency, 1.0e-4, 0.10);"));
        assert!(shader.contains("return mix("));
        assert!(shader.contains("fn isotropic_noise("));
        assert!(shader.contains("1.20, angular_pixel_footprint, 70u"));
        assert!(shader.contains("snoise(direction * frequencies.z"));
        assert!(!shader.contains("global_warp"));
        assert!(!shader.contains("wind_warp"));
    }

    #[test]
    fn u3_structure_tensor_feature_axis_follows_synthetic_stripes_in_common_frame() {
        let stripe_tensor = |vertical: bool| {
            let mut tensor = [0.0; 3];
            for y in 1..31 {
                for x in 1..31 {
                    let value = |x: usize, y: usize| if vertical { x as f32 } else { y as f32 };
                    let gx = (value(x + 1, y) - value(x - 1, y)) * 0.5;
                    let gy = (value(x, y + 1) - value(x, y - 1)) * 0.5;
                    tensor[0] += gx * gx;
                    tensor[1] += gx * gy;
                    tensor[2] += gy * gy;
                }
            }
            u3_feature_axis(tensor[0], tensor[1], tensor[2])
        };
        let vertical = stripe_tensor(true);
        let horizontal = stripe_tensor(false);
        assert!(vertical[0].abs() < 1.0e-4 && vertical[1].abs() > 0.99);
        assert!(horizontal[0].abs() > 0.99 && horizontal[1].abs() < 1.0e-4);
    }

    #[test]
    fn u3_screen_direction_uses_wgsl_column_major_rotation() {
        let direction = u3_screen_direction(
            1,
            1,
            4,
            [
                [1.0, 0.0, 0.0, 0.0],
                [0.0, 0.0, 1.0, 0.0],
                [0.0, -1.0, 0.0, 0.0],
                [0.0, 0.0, 0.0, 1.0],
            ],
        );
        assert!(direction[1] < 0.0);
        assert!(direction[2] < 0.0);
    }

    #[test]
    fn u3_rendered_mass_support_accepts_sub_milli_positive_mass_and_rejects_zero_mass() {
        let sub_milli = u3_mass_association([0.000_1]);
        assert_eq!(sub_milli.support, 1.0);
        assert_eq!(sub_milli.zero_mass_opaque, 0);

        let zero = u3_mass_association([0.0]);
        assert_eq!(zero.support, 0.0);
        assert_eq!(zero.zero_mass_opaque, 1);
    }

    #[test]
    fn u3_cube_coordinates_round_trip_all_faces() {
        let resolution = 32;
        for face in 0..6 {
            for (u, v) in [(0.25, 0.25), (0.5, 0.5), (0.75, 0.75)] {
                let direction = planet_gen::cube_sphere::cube_to_sphere(face, u, v);
                let (actual_face, x, y) = u3_cube_coordinates(direction, resolution);
                assert_eq!(actual_face, face as usize, "face={face} uv=({u},{v})");
                assert!((x - (u * resolution as f32 - 0.5)).abs() < 1.0e-4);
                assert!((y - (v * resolution as f32 - 0.5)).abs() < 1.0e-4);
            }
        }
    }

    #[test]
    fn u3_linear_cube_sample_interpolates_across_face_seams() {
        let resolution = 4;
        let mut values = vec![0.0; resolution as usize * resolution as usize * 6 * 4];
        for y in 0..resolution as usize {
            values[(4 * 16 + y * 4 + 3) * 4] = 1.0;
        }
        let direction = planet_gen::cube_sphere::cube_to_sphere(0, 0.0625, 0.5);
        let sample = u3_linear_cube_sample(&values, resolution, direction, 0);
        assert!((sample - 0.25).abs() < 1.0e-6, "sample={sample}");
    }

    #[test]
    fn u3_ray_ndc_matches_preview_pan_zoom_transform() {
        let context = super::U3RayContext {
            mass: &[],
            geometry: &[],
            resolution: 1,
            rotation: [[0.0; 4]; 4],
            radius_km: 1.0,
            pan: [0.1, -0.2],
            zoom: 2.0,
            cloud_seed: 0,
        };
        let ndc = u3_ray_ndc(1, 2, 4, &context);
        assert!((ndc[0] - ((-0.25 / 0.85 - 0.1) / 2.0)).abs() < 1.0e-6);
        assert!((ndc[1] - ((0.25 / 0.85 + 0.2) / 2.0)).abs() < 1.0e-6);
    }

    #[test]
    fn u3_ray_jitter_matches_pinned_shader_inputs() {
        let resolution = 16;
        let mut mass = vec![0.0; resolution * resolution * 6 * 4];
        let mut geometry = vec![0.0; mass.len()];
        for face in 0..6 {
            for y in 0..resolution {
                for x in 0..resolution {
                    let index = (face * resolution * resolution + y * resolution + x) * 4;
                    mass[index] = (face * 256 + y * 16 + x) as f32;
                    geometry[index + 3] = 0.2;
                }
            }
        }
        let rotation = [
            [1.0, 0.0, 0.0, 0.0],
            [0.0, 1.0, 0.0, 0.0],
            [0.0, 0.0, 1.0, 0.0],
            [0.0, 0.0, 0.0, 1.0],
        ];
        let context = super::U3RayContext {
            mass: &mass,
            geometry: &geometry,
            resolution: resolution as u32,
            rotation,
            radius_km: 1.0,
            pan: [0.0, 0.0],
            zoom: 1.0,
            cloud_seed: 42,
        };
        let samples = super::u3_ray_samples(36, 27, 64, &context).unwrap();
        assert_eq!(
            samples.map(|sample| (sample.mass, sample.face, sample.uv)),
            [
                (1170.828, 4, [0.571_055_1, 0.571_055_1]),
                (1_171.262_5, 4, [0.572_652_16, 0.572_652_16]),
                (1_171.716_8, 4, [0.574_322_64, 0.574_322_64]),
                (1_172.192_7, 4, [0.576_071_74, 0.576_071_74]),
                (1_172.691_4, 4, [0.577_905_1, 0.577_905_1]),
                (1_173.214_8, 4, [0.579_829_1, 0.579_829_1]),
                (1_173.764_6, 4, [0.581_850_5, 0.581_850_5]),
                (1_174.343_3, 4, [0.583_976_9, 0.583_976_9]),
            ]
        );
    }

    #[test]
    fn rainout_uses_the_relative_humidity_threshold() {
        let shader = include_str!("../shaders/weather_spinup.wgsl");
        assert!(
            shader.contains(
                "max(condensate - q_sat * effective_relative_humidity_target, 0.0) * 0.22"
            )
        );
        assert!(shader.contains("max(marine_climate, source_owned_share)"));
        // DS-045 de-classification: low-cloud dissipation follows wind-driven
        // entrainment instead of the static marine class.
        assert!(shader.contains("state.y * low_dissipation"));
        assert!(!shader.contains("max(condensate - q_target, 0.0) * 0.22"));
    }

    #[test]
    fn polar_metrics_use_the_temperature_axis_for_each_hemisphere() {
        let resolution = 32;
        let tilt = 0.35_f32;
        let (sin, cos) = tilt.sin_cos();
        let mut mass = vec![0.0; resolution as usize * resolution as usize * 6 * 4];
        for face in 0..6 {
            for y in 0..resolution {
                for x in 0..resolution {
                    let position = planet_gen::cube_sphere::cube_to_sphere(
                        face,
                        x as f32 / (resolution - 1) as f32,
                        y as f32 / (resolution - 1) as f32,
                    );
                    let latitude = (position[1] * cos + position[2] * sin)
                        .clamp(-1.0, 1.0)
                        .asin()
                        .to_degrees()
                        .abs();
                    let low = if latitude >= 70.0 {
                        0.10
                    } else if latitude >= 55.0 {
                        0.30
                    } else {
                        0.0
                    };
                    let index =
                        ((face * resolution * resolution + y * resolution + x) * 4) as usize;
                    mass[index] = low;
                }
            }
        }
        for north in [true, false] {
            let metrics = polar_metrics(&mass, resolution, tilt, north);
            assert!((metrics.polar.low_mean - 0.10).abs() < 1e-5);
            assert!((metrics.adjacent.low_mean - 0.30).abs() < 1e-5);
            assert_eq!(metrics.polar.occupied, 1.0);
            assert_eq!(metrics.adjacent.occupied, 1.0);
        }
    }

    #[test]
    fn u15_size_fixture_climate_preconditions_cover_centers_and_ring() {
        assert!(u15_size_precondition_diagnostics().is_empty());
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
        assert!(validate_seed_topology_metrics(&coherent, Some(0.18)).is_empty());
        assert!(
            validate_seed_topology_metrics(
                &TopologyMetrics {
                    largest_component: 0.990,
                    ..coherent
                },
                Some(0.18),
            )
            .is_empty()
        );
        assert!(
            !validate_seed_topology_metrics(
                &TopologyMetrics {
                    largest_component: 0.991,
                    ..coherent
                },
                Some(0.18),
            )
            .is_empty()
        );
        assert!(
            !validate_seed_topology_metrics(
                &TopologyMetrics {
                    occupied: 0.9,
                    ..coherent
                },
                Some(0.18),
            )
            .is_empty()
        );
        assert!(
            !validate_seed_topology_metrics(
                &TopologyMetrics {
                    coherent: 0.5,
                    ..coherent
                },
                Some(0.18),
            )
            .is_empty()
        );
        assert!(!validate_seed_topology_metrics(&coherent, Some(0.01)).is_empty());
        assert!(!validate_seed_topology_metrics(&coherent, Some(0.7)).is_empty());
        assert!(
            !validate_seed_topology_metrics(
                &TopologyMetrics {
                    zonal_continuity: 0.4,
                    ..coherent
                },
                Some(0.18),
            )
            .is_empty()
        );
        assert!(
            !validate_seed_topology_metrics(
                &TopologyMetrics {
                    components: 1,
                    largest_component: 1.0,
                    ..coherent
                },
                Some(0.18),
            )
            .is_empty()
        );
        let failures = validate_seed_topology_metrics(
            &TopologyMetrics {
                occupied: 0.9,
                coherent: 0.5,
                components: 1,
                largest_component: 1.0,
                ..coherent
            },
            Some(0.7),
        );
        assert_eq!(failures.len(), 4);
    }

    #[test]
    fn cubemap_edge_pairs_cover_every_shared_edge_once() {
        assert_eq!(cube_edge_pairs(16).len(), 12);
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
    fn u14_coverage_uses_column_tau_and_crosses_cubemap_seams() {
        let resolution = 40;
        let left = pixel(0, 0, 20, resolution);
        let right = pixel(4, 39, 20, resolution);
        let mut mass = vec![0.0; resolution as usize * resolution as usize * 6 * 4];
        for pixel in [
            left,
            pixel(0, 0, 21, resolution),
            right,
            pixel(4, 39, 21, resolution),
        ] {
            mass[pixel * 4] = 0.02;
        }

        let metrics = u14_coverage_support_metrics(&mass, resolution);

        assert!((metrics.occupied - 4.0 / (resolution * resolution * 6) as f32).abs() < 1e-6);
        assert_eq!(metrics.component_p75, Some(0.0025));
        assert!(metrics.clear_gap_radius > 0.0);
    }

    #[test]
    fn u14_coverage_excludes_insignificant_components_and_uses_fixed_tau_mask() {
        let resolution = 40;
        let mut mass = vec![0.0; resolution as usize * resolution as usize * 6 * 4];
        let pixels = [
            pixel(0, 10, 10, resolution),
            pixel(0, 11, 10, resolution),
            pixel(0, 10, 11, resolution),
        ];
        for pixel in pixels {
            mass[pixel * 4 + 1] = 0.01;
        }

        assert_eq!(
            u14_coverage_support_metrics(&mass, resolution).component_p75,
            None
        );
        let mut mask = vec![false; resolution as usize * resolution as usize * 6];
        for pixel in pixels {
            mask[pixel] = true;
        }
        assert!((u14_fixed_core_p90(&mass, &mask) - 0.0144).abs() < 1e-6);
    }

    #[test]
    fn u14_coverage_growth_requires_overlap_and_tie_breaks_split_components() {
        let low = vec![U14CoverageComponent {
            pixels: vec![0, 1, 2, 3],
            area_fraction: 0.01,
        }];
        let no_overlap = u14_coverage_growth(
            &low,
            &[U14CoverageComponent {
                pixels: vec![4, 5],
                area_fraction: 0.01,
            }],
            1,
        );
        assert_eq!(no_overlap.missing_low_components, vec![0]);

        let split = u14_coverage_growth(
            &low,
            &[
                U14CoverageComponent {
                    pixels: vec![2, 3],
                    area_fraction: 0.005,
                },
                U14CoverageComponent {
                    pixels: vec![0, 1],
                    area_fraction: 0.01,
                },
            ],
            2,
        );
        assert_eq!(split.missing_low_components, Vec::<usize>::new());
        assert_eq!(split.ratios, vec![0.5]);
    }

    #[test]
    fn u14_coverage_growth_allows_multiple_low_components_to_merge() {
        let low = vec![
            U14CoverageComponent {
                pixels: vec![0, 1],
                area_fraction: 0.01,
            },
            U14CoverageComponent {
                pixels: vec![2, 3],
                area_fraction: 0.01,
            },
        ];
        let growth = u14_coverage_growth(
            &low,
            &[U14CoverageComponent {
                pixels: vec![0, 1, 2, 3],
                area_fraction: 0.04,
            }],
            2,
        );
        assert_eq!(growth.ratios, vec![4.0, 4.0]);
        assert_eq!(growth.merged_low_components, 1);
    }

    #[test]
    fn u14_coverage_growth_matches_components_across_cubemap_seams() {
        let resolution = 40;
        let left = pixel(0, 0, 20, resolution);
        let right = pixel(4, 39, 20, resolution);
        let mut mass = vec![0.0; resolution as usize * resolution as usize * 6 * 4];
        for pixel in [
            left,
            pixel(0, 0, 21, resolution),
            right,
            pixel(4, 39, 21, resolution),
        ] {
            mass[pixel * 4] = 0.02;
        }
        let components = u14_significant_occupied_components(&mass, resolution);
        let growth = u14_coverage_growth(&components, &components, resolution);
        assert_eq!(components.len(), 1);
        assert_eq!(growth.ratios, vec![1.0]);
        assert!(growth.missing_low_components.is_empty());
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
        let components = u15_significant_response_components(&response, resolution, None);

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
        let components = u15_significant_response_components(&response, resolution, None);

        assert_eq!(components.len(), 1);
        assert_eq!(components[0].pixels.len(), 4);
        assert!((components[0].area_fraction - 0.0025).abs() < 1e-6);
    }

    #[test]
    fn u15_trail_source_component_seeds_all_occupied_ocean_core_pixels() {
        let resolution = 64;
        let center = super::u15_sphere_to_pixel(super::U15_TRAIL_SOURCE, resolution);
        let source = (0..resolution as usize * resolution as usize * 6)
            .find(|&pixel| {
                pixel != center
                    && super::u15_trail_source_core(u15_pixel_position(pixel, resolution))
            })
            .expect("ocean source core pixel");
        let mut response = vec![0.0; resolution as usize * resolution as usize * 6];
        response[source] = super::U15_TRAIL_Q;

        let (component, source_pixels, source_connected) =
            super::u15_source_component(&response, resolution);

        assert_eq!(source_pixels, 1);
        assert!(source_connected);
        assert!(component[source]);
    }

    #[test]
    fn u15_plume_metrics_keep_detached_response_as_telemetry() {
        let resolution = 64;
        let source = super::u15_sphere_to_pixel(super::U15_TRAIL_SOURCE, resolution);
        let connected = super::u15_pixel_neighbors(source, resolution)[0];
        let detached = (0..resolution as usize * resolution as usize * 6)
            .find(|pixel| {
                *pixel != source
                    && *pixel != connected
                    && super::u15_trail_coordinates(u15_pixel_position(*pixel, resolution)).0
                        > super::U15_TRAIL_SHAPE_ROI_RADIUS
            })
            .expect("detached pixel");
        let mut response = vec![0.0; resolution as usize * resolution as usize * 6];
        response[source] = 0.04;
        response[connected] = 0.04;
        response[detached] = 0.04;

        let metrics = super::u15_plume_metrics(&response, resolution);

        assert!(metrics.source_connected);
        assert!(metrics.detached_response_mass > 0.0);
        assert!(metrics.detached_component_response_mass > 0.0);
        assert!(metrics.response_mass > metrics.component_response_mass);
    }

    #[test]
    fn u15_size_tops_require_one_pair_for_each_production_center() {
        let resolution = 64;
        let centers = u15_fixture_centers(U15_SIZE_SEEDS[0], 4);
        let association = u15_size_association(resolution, &centers);
        let pixels = resolution as usize * resolution as usize * 6;
        let mut small_geometry = vec![0.0; pixels * 4];
        let mut large_geometry = vec![0.0; pixels * 4];
        let components = |tops: &mut [f32], offset: f32| {
            centers
                .iter()
                .enumerate()
                .map(|(owner, center)| {
                    let pixel = super::u15_sphere_to_pixel(*center, resolution);
                    tops[pixel * 4 + 2] = owner as f32 + offset;
                    U15ResponseComponent {
                        pixels: vec![pixel],
                        centroid: *center,
                        area_fraction: 0.0025,
                    }
                })
                .collect::<Vec<_>>()
        };
        let small = components(&mut small_geometry, 4.0);
        let large = components(&mut large_geometry, 7.0);

        let small_owners = u15_owner_size_metrics(&small, &small_geometry, &association);
        let large_owners = u15_owner_size_metrics(&large, &large_geometry, &association);
        let pairs = u15_paired_size_tops(&small_owners, &large_owners).expect("paired components");

        assert_eq!(pairs.len(), 4);
        for (owner, pair) in pairs.iter().enumerate() {
            assert_eq!(pair.small_top_p95, owner as f32 + 4.0);
            assert_eq!(pair.large_top_p95, owner as f32 + 7.0);
        }
        let mut missing_small = small_owners.clone();
        missing_small[0] = None;
        assert!(u15_paired_size_tops(&missing_small, &large_owners).is_none());
        let mut duplicate_small = small.clone();
        duplicate_small.push(small[0].clone());
        let duplicate_owners =
            u15_owner_size_metrics(&duplicate_small, &small_geometry, &association);
        assert_eq!(duplicate_owners[0].unwrap().satellite_count, 1);
    }

    #[test]
    fn u15_owner_provenance_share_maps_validation_pixels_onto_spin_snapshot() {
        const RESOLUTION: u32 = 512;
        const SPIN: usize = 128;
        let face_pixels = RESOLUTION as usize * RESOLUTION as usize;
        // Face-symmetric validation patch (mirror pairs x ↔ 511-x straddle the
        // two halves) so solid-angle weights cancel in the half-ownership case.
        let mut component_pixels = Vec::new();
        for y in 128..192usize {
            for x in 192..320usize {
                component_pixels.push(y * RESOLUTION as usize + x);
            }
        }
        let mut association: Vec<Option<u32>> = vec![None; face_pixels * 6];
        for &pixel in &component_pixels {
            association[pixel] = Some(0);
        }
        let component = U15ResponseComponent {
            pixels: component_pixels.clone(),
            centroid: [0.0, 0.0, 1.0],
            area_fraction: 0.0025,
        };
        let mut total_water = vec![0.0; face_pixels * 6 * 4];
        for chunk in total_water.chunks_exact_mut(4) {
            chunk[0] = 1.0;
        }
        let components = [component];
        // Validation patch 192..320 maps by /4 onto spin texels 48..80.
        let mut provenance = vec![0.0; SPIN * SPIN * 6];
        let stamp = |provenance: &mut Vec<f32>, x_range: std::ops::Range<usize>| {
            for y in 32..48usize {
                for x in x_range.clone() {
                    provenance[y * SPIN + x] = 1.0;
                }
            }
        };
        stamp(&mut provenance, 48..80);
        let full = u15_owner_primary_provenance_share(
            &provenance,
            &total_water,
            RESOLUTION,
            &components,
            &association,
            0,
        );
        assert!((full.unwrap() - 1.0).abs() < 1e-5);

        let mut half_provenance = vec![0.0; SPIN * SPIN * 6];
        stamp(&mut half_provenance, 48..64);
        let half = u15_owner_primary_provenance_share(
            &half_provenance,
            &total_water,
            RESOLUTION,
            &components,
            &association,
            0,
        );
        assert!((half.unwrap() - 0.5).abs() < 1e-5);

        let empty = u15_owner_primary_provenance_share(
            &vec![0.0; SPIN * SPIN * 6],
            &total_water,
            RESOLUTION,
            &components,
            &association,
            0,
        );
        assert_eq!(empty.unwrap(), 0.0);

        // Malformed provenance buffer (not 6·k²) must not panic.
        assert!(
            u15_owner_primary_provenance_share(
                &vec![0.0; 100],
                &total_water,
                RESOLUTION,
                &components,
                &association,
                0,
            )
            .is_none()
        );
    }

    #[test]
    fn u15_size_seeds_are_the_first_input_only_production_candidates() {
        let selected = u15_size_frozen_candidates();
        assert_eq!(selected, U15_SIZE_SEEDS);
        for seed in U15_SIZE_SEEDS {
            assert!(u15_size_seed_criterion(seed));
            assert!(
                u15_size_minimum_physical_eligibility(seed)
                    >= super::U15_SIZE_MIN_PHYSICAL_ELIGIBILITY
            );
        }
    }

    #[test]
    fn u15_size_fixture_support_is_full_through_major_radius_and_zero_at_association_cap() {
        assert_eq!(u15_size_fixture_support(U15_SIZE_FULL_SUPPORT_RADIUS), 1.0);
        assert_eq!(u15_size_fixture_support(U15_SIZE_SUPPORT_RADIUS), 0.0);
        assert!(u15_size_fixture_support(0.315) > 0.0);
        assert!(u15_size_fixture_support(0.315) < 1.0);
    }

    #[test]
    fn u15_size_association_is_fixed_and_unique() {
        let centers = u15_fixture_centers(U15_SIZE_SEEDS[0], 4);
        let association = u15_size_association(64, &centers);
        let masks = [0.3, 1.0, 3.0].map(|_| u15_size_association(64, &centers));
        assert!(masks.windows(2).all(|pair| pair[0] == pair[1]));
        for (index, center) in centers.iter().enumerate() {
            let pixel = super::u15_sphere_to_pixel(*center, 64);
            assert_eq!(association[pixel], Some(index as u32));
        }
        for (pixel, center) in association
            .iter()
            .enumerate()
            .filter_map(|(pixel, center)| center.map(|center| (pixel, center)))
        {
            let position = u15_pixel_position(pixel, 64);
            assert!(
                super::u15_dot(position, centers[center as usize]) >= U15_SIZE_SUPPORT_RADIUS.cos()
            );
            assert_eq!(
                center,
                centers
                    .iter()
                    .enumerate()
                    .max_by(|(_, left), (_, right)| {
                        super::u15_dot(position, **left)
                            .total_cmp(&super::u15_dot(position, **right))
                    })
                    .map(|(nearest, _)| nearest as u32)
                    .unwrap()
            );
        }
    }

    #[test]
    fn u15_size_weather_uses_each_frozen_seed() {
        for seed in U15_SIZE_SEEDS {
            let snapshot = u15_size_weather_snapshot(512, seed, 3.0);
            assert_eq!(snapshot.seed, seed);
            assert_eq!(snapshot.storm_count, 4);
            assert_eq!(snapshot.storm_size, 3.0);
        }
    }

    #[test]
    fn u15_size_association_measures_larger_synthetic_footprints_as_larger() {
        let resolution = 64;
        let centers = u15_fixture_centers(U15_SIZE_SEEDS[0], 4);
        let association = u15_size_association(resolution, &centers);
        let response = |radius: f32| {
            let mut values = vec![0.0; association.len() * 4];
            for (pixel, center) in association.iter().enumerate() {
                if *center == Some(0)
                    && super::u15_dot(u15_pixel_position(pixel, resolution), centers[0])
                        >= radius.cos()
                {
                    values[pixel * 4 + 1] = U15_DEEP_THRESHOLD;
                }
            }
            values
        };

        let small = u15_significant_size_components(&response(0.12), resolution, &association);
        let large = u15_significant_size_components(&response(0.24), resolution, &association);

        assert_eq!(small.len(), 1);
        assert_eq!(large.len(), 1);
        assert!(large[0].area_fraction > small[0].area_fraction);
    }

    #[test]
    fn u15_size_response_uses_the_small_size_at_the_same_storm_count_as_baseline() {
        let small = vec![0.1, 0.2, 0.3, 0.4];
        let large = vec![0.4, 0.5, 0.6, 0.7];
        let count_zero = vec![0.0; 4];

        assert!(
            super::u15_response(&large, &small)
                .iter()
                .all(|value| (*value - 0.3).abs() < 1e-6)
        );
        assert_ne!(
            super::u15_response(&large, &small),
            super::u15_response(&large, &count_zero)
        );
    }

    // Executable replacement for the former source-text gate: the following
    // tests run the real spin-up on the GPU with the owned-vapor tracer and
    // assert its contract directly.
    #[cfg(feature = "validation")]
    use super::{
        U15_SIZE_MIN_PHYSICAL_ELIGIBILITY, WeatherFieldPipeline, WeatherSnapshot, WeatherTextures,
        cube_edge_point, u14_flat_terrain,
    };
    #[cfg(feature = "validation")]
    use planet_gen::gpu::GpuContext;
    #[cfg(feature = "validation")]
    use planet_gen::terrain_compute::WindFieldPipeline;
    #[cfg(feature = "validation")]
    use planet_gen::weather::{TracerValidationTrace, WEATHER_DIAGNOSTIC_NO_SINK};

    #[cfg(feature = "validation")]
    fn u15_tracer_run(
        resolution: u32,
        height: impl Fn([f32; 3]) -> f32,
        diagnostic_flags: u32,
    ) -> (
        GpuContext,
        WeatherFieldPipeline,
        WeatherTextures,
        TracerValidationTrace,
    ) {
        u15_tracer_run_with_provenance(resolution, height, diagnostic_flags, true, 0.0)
    }

    /// `provenance_enabled = false` forces the no-R16 fallback path so capable
    /// adapters exercise the branch incapable adapters always take.
    /// `continentality` sets the dynamics `.a` channel uniformly; 0.0 keeps
    /// marine_fraction ≡ 1 (all fixtures' default), 1.0 drives it to 0.
    #[cfg(feature = "validation")]
    fn u15_tracer_run_with_provenance(
        resolution: u32,
        height: impl Fn([f32; 3]) -> f32,
        diagnostic_flags: u32,
        provenance_enabled: bool,
        continentality: f32,
    ) -> (
        GpuContext,
        WeatherFieldPipeline,
        WeatherTextures,
        TracerValidationTrace,
    ) {
        u15_tracer_run_with_snapshot(
            resolution,
            height,
            diagnostic_flags,
            provenance_enabled,
            continentality,
            WeatherSnapshot {
                base_temp_c: 30.0,
                ..u15_size_weather_snapshot(resolution, U15_SIZE_SEEDS[0], 1.0)
            },
        )
    }

    #[cfg(feature = "validation")]
    fn u15_tracer_run_with_snapshot(
        resolution: u32,
        height: impl Fn([f32; 3]) -> f32,
        diagnostic_flags: u32,
        provenance_enabled: bool,
        continentality: f32,
        snapshot: WeatherSnapshot,
    ) -> (
        GpuContext,
        WeatherFieldPipeline,
        WeatherTextures,
        TracerValidationTrace,
    ) {
        let gpu = GpuContext::new().expect("GPU init failed");
        let wind = WindFieldPipeline::new(&gpu).expect("U15 dynamics unavailable");
        let pipeline = WeatherFieldPipeline::new(&gpu).expect("U15 weather unavailable");
        let terrain = u14_flat_terrain(resolution, height);
        let dynamics = wind.create_test_textures(&gpu, resolution, |_| {
            ([0.2, 0.0, -0.1, continentality], 1013.0)
        });
        let field = pipeline.create_textures(&gpu, resolution);
        let trace = pipeline.validation_tracer_trace_for_sweep(
            &gpu,
            snapshot,
            &terrain,
            &dynamics,
            &field,
            diagnostic_flags,
            provenance_enabled,
        );
        (gpu, pipeline, field, trace)
    }

    #[cfg(feature = "validation")]
    fn u15_total_water(state: &[f32], cell: usize) -> f32 {
        state[cell * 4..cell * 4 + 4].iter().sum()
    }

    #[cfg(feature = "validation")]
    fn fe089_weighted_water_budget(trace: &TracerValidationTrace) -> f32 {
        let resolution = trace.spin_resolution;
        let face_pixels = resolution as usize * resolution as usize;
        trace
            .state
            .chunks_exact(4)
            .zip(trace.aux.chunks_exact(4))
            .enumerate()
            .map(|(cell, (state, aux))| {
                let local = cell % face_pixels;
                let weight = u15_weight(
                    (local % resolution as usize) as u32,
                    (local / resolution as usize) as u32,
                    resolution,
                );
                (state.iter().sum::<f32>() + aux[0]) * weight
            })
            .sum()
    }

    #[cfg(feature = "validation")]
    #[test]
    fn fe089_internal_budget_is_weighted_deterministic_and_void_exact() {
        let resolution = 16;
        let (_, _, _, first) = u15_tracer_run(resolution, |_| -0.1, 0);
        let (_, _, _, second) = u15_tracer_run(resolution, |_| -0.1, 0);
        assert_eq!(first.state, second.state, "state must be deterministic");
        assert_eq!(first.aux, second.aux, "aux must be deterministic");
        assert_eq!(
            first.provenance_tail, second.provenance_tail,
            "provenance tail must be deterministic"
        );
        let budget = fe089_weighted_water_budget(&first);
        assert!(
            budget.is_finite() && budget > 0.0,
            "weighted water budget={budget}"
        );

        let (_, _, _, void) = u15_tracer_run(resolution, |_| 0.1, WEATHER_DIAGNOSTIC_NO_SOURCE);
        assert!(
            void.state.iter().all(|value| *value == 0.0),
            "void state is nonzero"
        );
        assert!(
            void.aux.iter().all(|value| *value == 0.0),
            "void aux is nonzero"
        );
        assert!(
            void.provenance_tail.iter().all(|value| *value == 0.0),
            "void provenance tail is nonzero"
        );
        assert_eq!(fe089_weighted_water_budget(&void), 0.0, "void water budget");
    }

    #[cfg(feature = "validation")]
    fn u15_skip_without_r16(provenance: &Option<Vec<f32>>) -> Option<&Vec<f32>> {
        match provenance {
            Some(provenance) => Some(provenance),
            None => {
                eprintln!("adapter lacks R16 storage support; tracer coverage skipped");
                None
            }
        }
    }

    #[cfg(feature = "validation")]
    #[test]
    fn u15_source_owned_tracer_tracks_ocean_and_weighted_land_sources() {
        assert!(
            u15_size_minimum_physical_eligibility(U15_SIZE_SEEDS[0])
                >= U15_SIZE_MIN_PHYSICAL_ELIGIBILITY
        );
        let resolution = 16;
        // Pure ocean with the sink off: P must equal total water T per cell even
        // after phase transfers repartition vapor into condensate reservoirs.
        let (_, _, _, ocean) = u15_tracer_run(resolution, |_| -0.1, WEATHER_DIAGNOSTIC_NO_SINK);
        if let Some(p) = u15_skip_without_r16(&ocean.provenance) {
            assert_eq!(ocean.state.len(), p.len() * 4);
            // P advects through its own forward-Euler flux pass while T advects
            // through Hancock's predictor-corrector, so P == T holds only to the
            // schemes' combined truncation, not to f16 ULPs. Budgets are
            // measured on the reference adapter (worst per-cell drift 6.3%;
            // u15_weight-area-weighted global drift 1.17%, small-T cells
            // exact) with ~1.7x margin. The global conservation gate is what
            // catches genuine ownership loss.
            const CELL_DRIFT: f32 = 0.08;
            const CELL_FLOOR: f32 = 2e-4;
            const GLOBAL_DRIFT: f32 = 0.02;
            // Cubemap texels cover unequal solid angles; weight every texel by
            // its u15_weight area factor so ΣP/ΣT measure surface-integral
            // conservation, matching u15_provenance_total.
            let weight = |cell: usize| {
                let face_pixels = resolution as usize * resolution as usize;
                let local = cell % face_pixels;
                u15_weight(
                    (local % resolution as usize) as u32,
                    (local / resolution as usize) as u32,
                    resolution,
                )
            };
            let mut sum_p = 0.0f32;
            let mut sum_t = 0.0f32;
            for cell in 0..p.len() {
                let total = u15_total_water(&ocean.state, cell);
                assert!(total.is_finite() && total >= 0.0, "cell {cell}: T={total}");
                let w = weight(cell);
                sum_p += p[cell] * w;
                sum_t += total * w;
                let tolerance = CELL_DRIFT * total + CELL_FLOOR;
                assert!(
                    (p[cell] - total).abs() <= tolerance,
                    "ocean cell {}: P={} T={} exceeds drift budget {tolerance}",
                    cell,
                    p[cell],
                    total
                );
            }
            let global_gap = (sum_p - sum_t).abs();
            assert!(
                global_gap <= GLOBAL_DRIFT * sum_t,
                "ocean ownership lost {global_gap} of {sum_t} ({:.2}% > {}% budget)",
                100.0 * global_gap / sum_t,
                100.0 * GLOBAL_DRIFT
            );
            // Focused negative check: a distributed 3%-of-T ownership leak
            // slips past every per-cell budget but must trip the global gate.
            let leaked_sum: f32 = (0..p.len())
                .map(|cell| (p[cell] - 0.03 * u15_total_water(&ocean.state, cell)) * weight(cell))
                .sum();
            assert!(
                (leaked_sum - sum_t).abs() > GLOBAL_DRIFT * sum_t,
                "global gate must catch a 3% distributed provenance leak"
            );
        }
        // All-land ET is source-owned at the declared 0.3 share. The finite
        // unowned land bootstrap makes the final scalar ratio lower than 0.3,
        // but it must never exceed the source-mixing cap.
        let (_, _, _, land) = u15_tracer_run(resolution, |_| 0.1, 0);
        if let Some(p) = u15_skip_without_r16(&land.provenance) {
            let provenance_sum: f32 = p.iter().sum();
            assert!(
                provenance_sum > 0.0,
                "land evapotranspiration minted no provenance"
            );
            let total_budget: f32 = (0..p.len())
                .map(|cell| u15_total_water(&land.state, cell))
                .sum();
            assert!(
                provenance_sum <= 0.31 * total_budget,
                "land source ownership exceeds the 0.3 ET mixing cap: {provenance_sum} vs {total_budget}"
            );
        }
    }

    /// FE-079: transitional water (open_ocean=1, marine_fraction=0) must mint
    /// full-evaporation provenance — provenance_source_evaporation is the
    /// unmodified source-step evaporation, with water extent encoded exactly
    /// once via supply_budget's water.local * fetch factors. With maximal
    /// continentality on an all-ocean warm tilt-0 planet every other
    /// continentality coupling neutralizes (marine_climate stays 1, ice terms
    /// vanish, land_seed is 0), so any extra marine_fraction scaling would
    /// freeze provenance at its init value while total water grows unowned each
    /// source step.
    #[cfg(feature = "validation")]
    #[test]
    fn u15_tracer_transitional_water_mints_full_evaporation_provenance() {
        let resolution = 16;
        let (_, _, _, trace) = u15_tracer_run_with_provenance(
            resolution,
            |_| -0.1,
            WEATHER_DIAGNOSTIC_NO_SINK,
            true,
            1.0,
        );
        if let Some(p) = u15_skip_without_r16(&trace.provenance) {
            assert_eq!(trace.state.len(), p.len() * 4);
            // Same per-cell budget as the marine-fraction≡1 ocean tracer test:
            // with water.local ≡ 1 here the unmodified evaporation mint makes
            // this cell class behave exactly like pure ocean, P ≈ T within the
            // schemes' combined truncation.
            const CELL_DRIFT: f32 = 0.08;
            const CELL_FLOOR: f32 = 2e-4;
            for cell in 0..p.len() {
                let total = u15_total_water(&trace.state, cell);
                assert!(total.is_finite() && total >= 0.0, "cell {cell}: T={total}");
                let tolerance = CELL_DRIFT * total + CELL_FLOOR;
                assert!(
                    (p[cell] - total).abs() <= tolerance,
                    "transitional water cell {}: P={} T={} exceeds drift budget {tolerance}",
                    cell,
                    p[cell],
                    total
                );
            }
        }
    }

    #[cfg(feature = "validation")]
    #[test]
    fn u15_land_et_source_ownership_respects_the_share_and_zero_gates() {
        let resolution = 16;
        let snapshot = |coverage, moisture, base_temp_c| WeatherSnapshot {
            coverage,
            moisture,
            base_temp_c,
            ..u15_size_weather_snapshot(resolution, U15_SIZE_SEEDS[0], 1.0)
        };
        let run = |coverage, moisture, base_temp_c| {
            u15_tracer_run_with_snapshot(
                resolution,
                |_| 0.1,
                WEATHER_DIAGNOSTIC_NO_SINK,
                true,
                0.0,
                snapshot(coverage, moisture, base_temp_c),
            )
            .3
        };

        let active = run(1.0, 1.0, 30.0);
        if let Some(p) = u15_skip_without_r16(&active.provenance) {
            let source_owned: f32 = p.iter().sum();
            let total: f32 = (0..p.len())
                .map(|cell| u15_total_water(&active.state, cell))
                .sum();
            assert!(
                source_owned > 0.0,
                "active land ET minted no source ownership"
            );
            assert!(
                source_owned <= 0.31 * total,
                "source-owned scalar exceeds the land ET 0.3 mixing cap: {source_owned} vs {total}"
            );
        }

        for (name, coverage, moisture, temp) in [
            ("coverage", 0.0, 1.0, 30.0),
            ("moisture", 1.0, 0.0, 30.0),
            ("temperature", 1.0, 1.0, -50.0),
        ] {
            let gated = run(coverage, moisture, temp);
            if let Some(p) = u15_skip_without_r16(&gated.provenance) {
                assert!(
                    p.iter().all(|value| *value == 0.0),
                    "{name} ET gate must leave source ownership exactly zero"
                );
            }
        }
    }

    #[cfg(feature = "validation")]
    #[test]
    fn u15_tracer_forced_disable_exercises_the_no_r16_fallback_on_capable_adapters() {
        // Incapable adapters always take the provenance=None fallback; force
        // the same branch here so every adapter covers it.
        let (_, _, _, enabled) = u15_tracer_run(16, |_| -0.1, WEATHER_DIAGNOSTIC_NO_SINK);
        if u15_skip_without_r16(&enabled.provenance).is_none() {
            return;
        }
        let (gpu, pipeline, field, trace) =
            u15_tracer_run_with_provenance(16, |_| -0.1, WEATHER_DIAGNOSTIC_NO_SINK, false, 0.0);
        assert!(
            trace.provenance.is_none(),
            "forced disable must yield no tracer readback"
        );
        // FE-081 couples source ownership into phase partitioning, so the
        // enabled and disabled runs are intentionally different fields (see
        // the weather.rs fingerprint pins). Pin the forced disable to plain
        // generation instead: the branch incapable adapters always take must
        // stay bit-identical to the Mode-Off production path.
        let wind = WindFieldPipeline::new(&gpu).expect("U15 dynamics unavailable");
        let terrain = u14_flat_terrain(16, |_| -0.1);
        let dynamics = wind.create_test_textures(&gpu, 16, |_| ([0.2, 0.0, -0.1, 0.0], 1013.0));
        let snapshot = WeatherSnapshot {
            base_temp_c: 30.0,
            ..u15_size_weather_snapshot(16, U15_SIZE_SEEDS[0], 1.0)
        };
        let reference = pipeline.create_textures(&gpu, 16);
        pipeline.generate_with_diagnostic_flags(
            &gpu,
            snapshot,
            &terrain,
            &dynamics,
            &reference,
            WEATHER_DIAGNOSTIC_NO_SINK,
        );
        assert_eq!(
            field.read_mass(&gpu),
            reference.read_mass(&gpu),
            "forced disable must stay bit-identical to plain generation"
        );
    }

    #[cfg(feature = "validation")]
    #[test]
    fn u15_tracer_mixed_ownership_is_deterministic_fractional_and_bounded_by_total_water() {
        let resolution = 16;
        let mixed = |pos: [f32; 3]| if pos[2] > 0.0 { -0.1 } else { 0.1 };
        let (_, _, _, first) = u15_tracer_run(resolution, mixed, 0);
        let Some(p) = u15_skip_without_r16(&first.provenance) else {
            return;
        };
        let p = p.clone();
        let (_, _, _, second) = u15_tracer_run(resolution, mixed, 0);
        assert_eq!(
            Some(&p),
            second.provenance.as_ref(),
            "tracer ownership must be deterministic"
        );
        let mut fractional = false;
        for cell in 0..p.len() {
            let total = u15_total_water(&first.state, cell);
            // The shader clamps P against its pre-storage T; both textures then
            // round to half precision independently, so allow a couple of
            // relative f16 ULPs of drift when re-summing T on the host.
            let headroom = total.max(p[cell]) * 0.001953125 + 1e-5;
            assert!(
                p[cell].is_finite() && p[cell] >= 0.0 && p[cell] <= total + headroom,
                "cell {}: P={} outside [0, T={}+{}]",
                cell,
                p[cell],
                total,
                headroom
            );
            if p[cell] > 0.001 && total - p[cell] > 0.001 {
                fractional = true;
            }
        }
        assert!(
            fractional,
            "mixed planet must contain partially-owned cells"
        );
    }

    #[cfg(feature = "validation")]
    #[test]
    fn u15_tracer_rainout_only_removes_owned_mass_and_never_adds_it() {
        let resolution = 16;
        let (_, _, _, kept) = u15_tracer_run(resolution, |_| -0.1, WEATHER_DIAGNOSTIC_NO_SINK);
        let (_, _, _, rained) = u15_tracer_run(resolution, |_| -0.1, 0);
        let Some(kept_p) = u15_skip_without_r16(&kept.provenance) else {
            return;
        };
        let Some(rained_p) = u15_skip_without_r16(&rained.provenance) else {
            return;
        };
        // ponytail: proportionality is asserted as monotone non-increase plus a
        // strict global removal; per-cell ratios are unobservable from final
        // states alone and would need an extra readback pass to verify exactly.
        let mut removed_somewhere = false;
        for cell in 0..kept_p.len() {
            assert!(
                rained_p[cell] <= kept_p[cell] + 0.01,
                "cell {}: rainout grew P from {} to {}",
                cell,
                kept_p[cell],
                rained_p[cell]
            );
            if kept_p[cell] - rained_p[cell] > 0.005 {
                removed_somewhere = true;
            }
        }
        let kept_total: f32 = kept_p.iter().sum();
        let rained_total: f32 = rained_p.iter().sum();
        assert!(
            removed_somewhere && rained_total < kept_total,
            "rainout must remove owned provenance somewhere: {kept_total} -> {rained_total}"
        );
    }

    #[cfg(feature = "validation")]
    #[test]
    fn u15_tracer_provenance_is_continuous_across_cubemap_seams() {
        let resolution = 16;
        let (_, _, _, trace) = u15_tracer_run(resolution, |_| -0.1, 0);
        let Some(p) = u15_skip_without_r16(&trace.provenance) else {
            return;
        };
        let face_len = (resolution * resolution) as usize;
        let width = resolution as usize;
        let last = width - 1;
        let sample = |face: usize, x: usize, y: usize| p[face * face_len + y * width + x];
        let seam_deltas: Vec<f32> = cube_edge_pairs(resolution)
            .iter()
            .flat_map(|(left, right, reversed)| {
                (0..=last).map(move |index| {
                    let (left_face, left_x, left_y) = cube_edge_point(*left, index, last);
                    let right_index = if *reversed { last - index } else { index };
                    let (right_face, right_x, right_y) = cube_edge_point(*right, right_index, last);
                    (sample(left_face, left_x, left_y) - sample(right_face, right_x, right_y)).abs()
                })
            })
            .collect();
        let interior_deltas: Vec<f32> = (0..6usize)
            .flat_map(|face| {
                (0..last).flat_map(move |y| {
                    (0..last).flat_map(move |x| {
                        [
                            (sample(face, x, y) - sample(face, x + 1, y)).abs(),
                            (sample(face, x, y) - sample(face, x, y + 1)).abs(),
                        ]
                    })
                })
            })
            .collect();
        let seam_mean = seam_deltas.iter().sum::<f32>() / seam_deltas.len() as f32;
        let interior_mean = interior_deltas.iter().sum::<f32>() / interior_deltas.len() as f32;
        assert!(
            seam_mean <= interior_mean * 8.0 + 1e-4,
            "seam discontinuity {seam_mean} exceeds interior gradient {interior_mean}"
        );
    }

    #[cfg(feature = "validation")]
    #[test]
    fn u15_tracer_mode_off_preserves_the_production_fingerprint_and_respects_budget() {
        // Policy: the A/B provenance cubemap pair stays within 384 KiB — exactly
        // 2 textures x R16Float x SPINUP_RESOLUTION^2 x 6 faces x 2 bytes today.
        // weather.rs asserts this at texture creation; restate the arithmetic so
        // the policy survives refactors of either site.
        const PROVENANCE_PAIR_BUDGET_BYTES: usize = 384 * 1024;
        const SPINUP_RESOLUTION_CAP: u32 = 128;
        let spin_resolution = 16u32.clamp(2, SPINUP_RESOLUTION_CAP);
        let pair_bytes = 2 * (spin_resolution * spin_resolution * 6 * 2) as usize;
        assert!(pair_bytes <= PROVENANCE_PAIR_BUDGET_BYTES);

        let resolution = 16;
        let gpu = GpuContext::new().expect("GPU init failed");
        let wind = WindFieldPipeline::new(&gpu).expect("U15 dynamics unavailable");
        let pipeline = WeatherFieldPipeline::new(&gpu).expect("U15 weather unavailable");
        let terrain = u14_flat_terrain(resolution, |_| -0.1);
        let dynamics =
            wind.create_test_textures(&gpu, resolution, |_| ([0.2, 0.0, -0.1, 0.0], 1013.0));
        let snapshot = WeatherSnapshot {
            base_temp_c: 30.0,
            ..u15_size_weather_snapshot(resolution, U15_SIZE_SEEDS[0], 1.0)
        };

        let field = pipeline.create_textures(&gpu, resolution);
        pipeline.generate(&gpu, snapshot, &terrain, &dynamics, &field);
        let baseline_mass = field.read_mass(&gpu);
        let baseline_geometry = field.read_geometry(&gpu);

        // FE-081 couples source ownership into phase partitioning, so a
        // tracer-enabled rerun is intentionally a different field. Pin the
        // infrastructure-neutrality policy through the Mode-Off branch
        // instead: a forced-disable rerun must reproduce production output
        // bit-exactly (this is also the branch incapable adapters take).
        let disabled = pipeline.validation_tracer_trace_for_sweep(
            &gpu, snapshot, &terrain, &dynamics, &field, 0, false,
        );
        assert!(disabled.provenance.is_none());
        assert_eq!(
            field.read_mass(&gpu),
            baseline_mass,
            "tracer perturbed the Mode-Off mass fingerprint"
        );
        assert_eq!(
            field.read_geometry(&gpu),
            baseline_geometry,
            "tracer perturbed the Mode-Off geometry fingerprint"
        );

        // Both tracer entry points must agree on R16 capability.
        let enabled = pipeline.validation_tracer_trace_for_sweep(
            &gpu, snapshot, &terrain, &dynamics, &field, 0, true,
        );
        let direct = pipeline
            .generate_with_provenance_for_validation(&gpu, snapshot, &terrain, &dynamics, &field);
        assert_eq!(
            direct.is_some(),
            enabled.provenance.is_some(),
            "both tracer entry points must agree on R16 capability"
        );
        if enabled.provenance.is_none() {
            // Unsupported-R16 adapter fallback: production path still runs and
            // the state readback stays available without any P channel.
            assert!(!enabled.state.is_empty());
            assert_eq!(field.read_mass(&gpu), baseline_mass);
        }
    }
}
