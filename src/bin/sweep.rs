//! Parameter sweep: generates a grid of planet previews as PNG files.
//! Usage: cargo run --bin sweep [--output-dir <dir>] [--size <pixels>]

use planet_gen::gpu::GpuContext;
use planet_gen::planet::{DerivedProperties, PlanetParams};
use planet_gen::plates::{generate_plates, PlateGenParams};
use planet_gen::preview::{PreviewRenderer, PreviewUniforms};
use planet_gen::terrain_compute::TerrainComputePipeline;
use std::path::Path;

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
        gpu, &plates, 512, seed, amplitude, frequency, octaves, gain, lacunarity, 1.0, 0.10, 1.0, 1.0,
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
        cloud_seed: 0.0,
        cloud_altitude: 0.008,
        cloud_type: 0.5,
        storm_count: 0.0,
        storm_size: 1.0,
        night_lights: 0.0,
        _pad3: [0.0; 2],
    };

    renderer.render(gpu, &uniforms, &cubemap_view, render_size)
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

    let seeds = [42, 137, 256, 999, 7777];
    let planet_presets = presets();

    println!("Parameter Sweep");
    println!("  Presets: {}", planet_presets.len());
    println!("  Seeds: {}", seeds.len());
    println!("  Total images: {}", planet_presets.len() * seeds.len());
    println!("  Resolution: {}x{}", render_size, render_size);
    println!("  Output: {}/", output_dir);
    println!();

    let gpu = GpuContext::new().expect("Failed to initialize GPU");
    println!("GPU: {}", gpu.adapter_name());

    let compute = TerrainComputePipeline::new(&gpu);
    let renderer = PreviewRenderer::new(&gpu);

    std::fs::create_dir_all(&output_dir).expect("Failed to create output directory");

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
}
