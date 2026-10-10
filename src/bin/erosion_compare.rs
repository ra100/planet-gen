//! Erosion comparison: renders the same planet with different erosion levels.
//! Usage: erosion_compare [OUTPUT_DIR] [MOISTURE]

use planet_gen::gpu::GpuContext;
use planet_gen::planet::{DerivedProperties, PlanetParams};
use planet_gen::plates::{PlateGenParams, generate_plates};
use planet_gen::preview::{PreviewRenderer, PreviewUniforms};
use planet_gen::terrain_compute::{
    ErosionClimate, ErosionPipeline, TectonicTerrain, TerrainComputePipeline,
};
use std::path::Path;

fn report_changes(before: &TectonicTerrain, after: &TectonicTerrain, ocean_level: f32) {
    let mut total = [0.0_f64; 3];
    let mut counts = [0_u64; 3];
    let mut roughness = [0.0_f64; 2];
    let res = before.resolution as usize;
    for (original, eroded) in before.faces.iter().zip(&after.faces) {
        for y in 1..res - 1 {
            for x in 1..res - 1 {
                let i = y * res + x;
                let elevation = original[i] - ocean_level;
                let region = if elevation < -0.018 {
                    0
                } else if elevation.abs() < 0.018 {
                    1
                } else {
                    2
                };
                total[region] += f64::from((eroded[i] - original[i]).abs());
                counts[region] += 1;
                if region == 1 {
                    for (index, field) in [original, eroded].iter().enumerate() {
                        let mean =
                            (field[i - 1] + field[i + 1] + field[i - res] + field[i + res]) * 0.25;
                        roughness[index] += f64::from((field[i] - mean).abs());
                    }
                }
            }
        }
    }
    let mean = |index: usize| total[index] / counts[index].max(1) as f64;
    println!(
        "mean_height_change deep_ocean={:.7} shore={:.7} inland={:.7} shore_roughness_ratio={:.3}",
        mean(0),
        mean(1),
        mean(2),
        roughness[1] / roughness[0].max(1e-12)
    );
}

fn main() {
    env_logger::init();

    let output_dir = std::env::args()
        .nth(1)
        .unwrap_or_else(|| "output/erosion_compare".into());
    let moisture = std::env::args()
        .nth(2)
        .map(|s| s.parse::<f32>().expect("moisture in [0, 1]"))
        .unwrap_or(1.0)
        .clamp(0.0, 1.0);
    let render_size = 1024u32;

    let gpu = GpuContext::new().expect("Failed to initialize GPU");
    println!("GPU: {}", gpu.adapter_name());

    let compute = TerrainComputePipeline::new(&gpu);
    let erosion = ErosionPipeline::new(&gpu);
    let renderer = PreviewRenderer::new(&gpu);

    std::fs::create_dir_all(&output_dir).expect("Failed to create output directory");

    let params = PlanetParams::default();
    let derived = DerivedProperties::from_params(&params);
    let seed = 42u32;
    let effective_ocean = derived.ocean_fraction;
    let ocean_level = -1.0 + 2.0 * effective_ocean;
    let climate = ErosionClimate {
        seed,
        base_temp_c: derived.base_temperature_c,
        axial_tilt_rad: params.axial_tilt_deg.to_radians(),
        moisture,
        ocean_fraction: effective_ocean,
    };

    let plates = generate_plates(&PlateGenParams {
        seed,
        mass_earth: params.mass_earth,
        ocean_fraction: effective_ocean,
        tectonics_factor: derived.tectonics_factor,
        continental_scale: 1.0,
        num_plates_override: 0,
        num_continents: 0,
        continent_size_variety: 0.0,
    });

    // Terrain params
    let amplitude = 0.6 + 0.6 * params.mass_earth.powf(0.3).min(2.0);
    let frequency = 1.0 + 0.5 * params.mass_earth.powf(0.2);
    let octaves = 10u32;
    let gain = 0.707f32;
    let lacunarity = 2.1f32;

    // Slight tilt + rotation to show continent detail
    let rot_y = 0.5f32;
    let rot_x = 0.3f32;
    let cy = rot_y.cos();
    let sy = rot_y.sin();
    let cx = rot_x.cos();
    let sx = rot_x.sin();

    let erosion_levels = [(0, "no_erosion"), (25, "default_25"), (50, "heavy_50")];

    // Also render normal view for each
    let view_modes = [(0u32, "normal"), (1u32, "height")];

    for (iterations, erosion_name) in &erosion_levels {
        // Generate fresh terrain for each (erosion modifies in place)
        let mut terrain = compute.generate(
            &gpu, &plates, 512, seed, amplitude, frequency, octaves, gain, lacunarity, 1.0, 0.10,
            1.0, 1.0, 9.81, 0.85, 0.2, 1.0,
        );

        // Apply erosion
        let original = terrain.clone();
        let erosion_started = std::time::Instant::now();
        if let Err(error) = erosion.erode(&gpu, &mut terrain, *iterations, ocean_level, climate) {
            eprintln!("erosion failed: {error}");
            return;
        }
        println!(
            "erosion iterations={iterations} moisture={moisture:.2} elapsed_ms={:.1}",
            erosion_started.elapsed().as_secs_f64() * 1000.0
        );
        report_changes(&original, &terrain, ocean_level);

        let cubemap_view = renderer.upload_terrain(&gpu, &terrain);

        for (view_mode, view_name) in &view_modes {
            let uniforms = PreviewUniforms {
                rotation: [
                    [cy, sy * sx, sy * cx, 0.0],
                    [0.0, cx, -sx, 0.0],
                    [-sy, cy * sx, cy * cx, 0.0],
                    [0.0, 0.0, 0.0, 1.0],
                ],
                light_dir: [0.5, 0.7, 1.0],
                ocean_level,
                base_temp_c: derived.base_temperature_c,
                ocean_fraction: effective_ocean * moisture,
                axial_tilt_rad: params.axial_tilt_deg.to_radians(),
                view_mode: *view_mode,
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
                surface_seed: 42,
            };

            let pixels = renderer.render(&gpu, &uniforms, &cubemap_view, None, None, render_size);
            let filename = format!("{}/{}_{}.png", output_dir, erosion_name, view_name);
            let img = image::RgbaImage::from_raw(render_size, render_size, pixels)
                .expect("Failed to create image");
            img.save(Path::new(&filename)).expect("Failed to save PNG");
            println!("Saved: {}", filename);
        }
    }

    println!("\nDone! 6 images saved to {}/", output_dir);
}
