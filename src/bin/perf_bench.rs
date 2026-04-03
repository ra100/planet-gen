//! Performance benchmark: measures terrain generation time at multiple resolutions.
//! Usage: cargo run --release --bin perf_bench
//!
//! Outputs CSV to stdout with columns:
//!   resolution, plates_ms, compute_ms, erosion_ms, upload_ms, total_ms
//!
//! Also prints a Quick vs Classified comparison at 768px.

use planet_gen::gpu::GpuContext;
use planet_gen::planet::{DerivedProperties, PlanetParams};
use planet_gen::plates::{generate_plates, PlateGenParams};
use planet_gen::preview::PreviewRenderer;
use planet_gen::terrain_compute::{ErosionPipeline, TerrainComputePipeline};
use std::sync::Arc;
use std::time::Instant;

fn main() {
    let gpu = Arc::new(GpuContext::new().expect("GPU init failed"));
    let terrain_compute = TerrainComputePipeline::new(&gpu);
    let erosion_pipeline = ErosionPipeline::new(&gpu);
    let preview_renderer = PreviewRenderer::new(&gpu);

    let params = PlanetParams::default();
    let derived = DerivedProperties::from_params(&params);
    let effective_ocean = derived.ocean_fraction;
    let ocean_level = -1.0 + 2.0 * effective_ocean;

    let resolutions = [256, 512, 768, 1024, 2048];
    let erosion_iterations = 25u32;
    let seed = params.seed;
    let warmup_res = 256;

    // Warmup run (GPU shader compilation, pipeline creation)
    eprintln!("Warming up GPU...");
    {
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
        let mut terrain = terrain_compute.generate(
            &gpu, &plates, warmup_res, seed, 1.0, 1.2, 8, 0.5, 2.0, 1.0, 0.10, 1.0, 1.0, 9.81, 0.85, 0.2, 1.0,
        );
        erosion_pipeline.erode(&gpu, &mut terrain, 5, ocean_level);
        let _ = preview_renderer.upload_terrain(&gpu, &terrain);
    }

    // CSV header
    println!("resolution,plates_ms,compute_ms,erosion_ms,upload_ms,total_ms,erosion_per_iter_ms");

    for &res in &resolutions {
        eprintln!("Benchmarking {}x{}...", res, res);

        let t_total = Instant::now();

        // Plates
        let t0 = Instant::now();
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
        let plates_ms = t0.elapsed().as_secs_f64() * 1000.0;

        // Compute (terrain generation)
        let t1 = Instant::now();
        let mut terrain = terrain_compute.generate(
            &gpu, &plates, res, seed, 1.0, 1.2, 8, 0.5, 2.0, 1.0, 0.10, 1.0, 1.0, 9.81, 0.85, 0.2, 1.0,
        );
        let compute_ms = t1.elapsed().as_secs_f64() * 1000.0;

        // Erosion
        let t2 = Instant::now();
        erosion_pipeline.erode(&gpu, &mut terrain, erosion_iterations, ocean_level);
        let erosion_ms = t2.elapsed().as_secs_f64() * 1000.0;
        let erosion_per_iter = erosion_ms / erosion_iterations as f64;

        // Upload to cubemap texture
        let t3 = Instant::now();
        let _ = preview_renderer.upload_terrain(&gpu, &terrain);
        let upload_ms = t3.elapsed().as_secs_f64() * 1000.0;

        let total_ms = t_total.elapsed().as_secs_f64() * 1000.0;

        println!(
            "{},{:.1},{:.1},{:.1},{:.1},{:.1},{:.1}",
            res, plates_ms, compute_ms, erosion_ms, upload_ms, total_ms, erosion_per_iter
        );
    }

    // --- Classified mode timing at 768px ---
    eprintln!("\nBenchmarking Classified at 768x768...");
    let bench_res = 768u32;

    let plates_bench = generate_plates(&PlateGenParams {
        seed,
        mass_earth: params.mass_earth,
        ocean_fraction: effective_ocean,
        tectonics_factor: derived.tectonics_factor,
        continental_scale: 1.0,
        num_plates_override: 0,
        num_continents: 0,
        continent_size_variety: 0.0,
    });

    let t_classified = Instant::now();
    let _terrain_classified = terrain_compute.generate(
        &gpu, &plates_bench, bench_res, seed, 1.0, 1.2, 8, 0.5, 2.0, 1.0, 0.10, 1.0, 1.0, 9.81, 0.85, 0.2, 1.0,
    );
    let classified_ms = t_classified.elapsed().as_secs_f64() * 1000.0;

    println!();
    println!("mode,resolution,compute_ms");
    println!("Classified,{},{:.1}", bench_res, classified_ms);
    println!();
    eprintln!("Classified: {:.1}ms", classified_ms);

    eprintln!("Done.");
}
