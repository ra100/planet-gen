//! Erosion comparison: renders the same planet with different erosion levels.
//! Generates 3 images: no erosion, default (25), heavy (50).
//! Uses height debug view for clearest comparison.

use planet_gen::gpu::GpuContext;
use planet_gen::planet::{DerivedProperties, PlanetParams};
use planet_gen::plates::{generate_plates, PlateGenParams};
use planet_gen::preview::{PreviewRenderer, PreviewUniforms};
use planet_gen::terrain_compute::{ErosionPipeline, TerrainComputePipeline};
use std::path::Path;

fn main() {
    env_logger::init();

    let output_dir = "output/erosion_compare";
    let render_size = 1024u32;

    let gpu = GpuContext::new().expect("Failed to initialize GPU");
    println!("GPU: {}", gpu.adapter_name());

    let compute = TerrainComputePipeline::new(&gpu);
    let erosion = ErosionPipeline::new(&gpu);
    let renderer = PreviewRenderer::new(&gpu);

    std::fs::create_dir_all(output_dir).expect("Failed to create output directory");

    let params = PlanetParams::default();
    let derived = DerivedProperties::from_params(&params);
    let seed = 42u32;
    let effective_ocean = derived.ocean_fraction;
    let ocean_level = -1.0 + 2.0 * effective_ocean;

    let plates = generate_plates(&PlateGenParams {
        seed,
        mass_earth: params.mass_earth,
        ocean_fraction: effective_ocean,
        tectonics_factor: derived.tectonics_factor,
        continental_scale: 1.0,
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

    let erosion_levels = [
        (0, "no_erosion"),
        (25, "default_25"),
        (50, "heavy_50"),
    ];

    // Also render normal view for each
    let view_modes = [
        (0u32, "normal"),
        (1u32, "height"),
    ];

    for (iterations, erosion_name) in &erosion_levels {
        // Generate fresh terrain for each (erosion modifies in place)
        let mut terrain = compute.generate(
            &gpu, &plates, 512, seed, amplitude, frequency, octaves, gain, lacunarity,
        );

        // Apply erosion
        erosion.erode(&gpu, &mut terrain, *iterations, ocean_level);

        let cubemap_view = renderer.upload_terrain(&gpu, &terrain);

        for (view_mode, view_name) in &view_modes {
            let uniforms = PreviewUniforms {
                rotation: [
                    [cy, sy * sx, sy * cx, 0.0],
                    [0.0, cx, -sx, 0.0],
                    [-sy, cy * sx, cy * cx, 0.0],
                    [0.0, 0.0, 0.0, 1.0],
                ],
                light_dir: [0.5, 0.7, -1.0],
                ocean_level,
                base_temp_c: derived.base_temperature_c,
                ocean_fraction: effective_ocean,
                axial_tilt_rad: params.axial_tilt_deg.to_radians(),
                view_mode: *view_mode,
                season: 0.5,
                atmosphere_density: 0.0,
                atmosphere_height: 0.0,
                height_scale: 3.0,
                zoom: 1.0,
                pan_x: 0.0,
                pan_y: 0.0,
                _pad1: 0.0,
            };

            let pixels = renderer.render(&gpu, &uniforms, &cubemap_view, render_size);
            let filename = format!("{}/{}_{}.png", output_dir, erosion_name, view_name);
            let img = image::RgbaImage::from_raw(render_size, render_size, pixels)
                .expect("Failed to create image");
            img.save(Path::new(&filename)).expect("Failed to save PNG");
            println!("Saved: {}", filename);
        }
    }

    println!("\nDone! 6 images saved to {}/", output_dir);
}
