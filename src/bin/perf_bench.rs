//! Performance benchmark: measures terrain generation time at multiple resolutions.
//! Usage: cargo run --release --bin perf_bench
//!
//! Outputs CSV to stdout with columns:
//!   resolution, plates_ms, compute_ms, erosion_ms, upload_ms, total_ms
//!
//! Also prints a Quick vs Classified comparison at 768px.

use planet_gen::export::{
    ExportConfig, ExportLayers, ExportTimings, MAX_8K_OWNED_LIVE_BYTES,
    estimated_export_preflight_bytes_with_erosion, estimated_peak_streaming_bytes,
    run_export_with_timings_and_checkpoints,
};
use planet_gen::gpu::GpuContext;
use planet_gen::perf_evidence::{
    AcceptanceProfile, CanonicalReport, GateStatus, PRESET, StageJournal, publish, sha256,
};
use planet_gen::planet::{DerivedProperties, PlanetParams};
use planet_gen::plates::{PlateGenParams, generate_plates};
use planet_gen::preview::PreviewRenderer;
use planet_gen::terrain_compute::{
    ErosionPipeline, TerrainComputePipeline, TerrainGenerationParams,
};
use planet_gen::weather::WeatherSnapshot;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::{Duration, Instant};

const EVIDENCE_ROOT: &str = "target/procedural-terrain-evidence";
const U2_768_PREVIEW_EROSION_ITERATIONS: u32 = 15;
const U2_8K_TIMEOUT: Duration = Duration::from_secs(240);
const U2_8K_MAX_MS: f64 = 240_000.0;

fn cancel_after(timeout: Duration) -> Arc<AtomicBool> {
    let cancelled = Arc::new(AtomicBool::new(false));
    let timer_cancelled = Arc::clone(&cancelled);
    std::thread::spawn(move || {
        std::thread::sleep(timeout);
        timer_cancelled.store(true, Ordering::Relaxed);
    });
    cancelled
}

fn run_id(mode: &str) -> String {
    format!(
        "u2-{mode}-seed42-rep1-{}",
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .expect("system time before epoch")
            .as_nanos()
    )
}

fn rss_bytes() -> Option<u64> {
    #[cfg(target_os = "linux")]
    {
        std::fs::read_to_string("/proc/self/status")
            .ok()?
            .lines()
            .find_map(|line| line.strip_prefix("VmHWM:"))?
            .split_whitespace()
            .next()?
            .parse::<u64>()
            .ok()?
            .checked_mul(1024)
    }
    #[cfg(not(target_os = "linux"))]
    {
        None
    }
}

fn report(gpu: &GpuContext, mode: &str, resolution: u32, owned_live_bytes: u64) -> CanonicalReport {
    let limits = gpu.device.limits();
    let mut report = CanonicalReport::not_run(run_id(mode), 42, 1);
    report.acceptance_profile = if resolution == 768 {
        AcceptanceProfile::Warm768
    } else {
        AcceptanceProfile::Cold8k
    };
    report.adapter = format!("{} ({:?})", gpu.adapter_name(), gpu.adapter_info.backend);
    report.build_fingerprint = sha256(include_bytes!("../../Cargo.lock"));
    report.config_fingerprint = sha256(
        format!(
            "{PRESET}|resolution={resolution}|seed=42|erosion_iterations={}|export_path={}",
            if resolution == 768 {
                U2_768_PREVIEW_EROSION_ITERATIONS
            } else {
                25
            },
            if resolution == 8192 {
                "direct-exr-staged-png-no-emission"
            } else {
                "preview"
            },
        )
        .as_bytes(),
    );
    report.owned_live_bytes = Some(owned_live_bytes);
    report.rss_bytes = rss_bytes();
    report.max_buffer_size = Some(limits.max_buffer_size);
    report.max_storage_buffer_binding_size =
        Some(u64::from(limits.max_storage_buffer_binding_size));
    report.max_texture_dimension_2d = Some(limits.max_texture_dimension_2d);
    report.gate_owned_bytes = if report.owned_live_bytes.unwrap() <= MAX_8K_OWNED_LIVE_BYTES {
        GateStatus::Pass
    } else {
        GateStatus::Fail
    };
    report.gate_rss = match report.rss_bytes {
        Some(bytes) if bytes <= 4 * 1024 * 1024 * 1024 => GateStatus::Pass,
        Some(_) => GateStatus::Fail,
        None => GateStatus::NotRun,
    };
    report
}

fn u2_8k_layers() -> ExportLayers {
    ExportLayers {
        height: true,
        albedo: true,
        normals: true,
        roughness: true,
        water_mask: true,
        clouds: true,
        // U2-012: emission still requires the legacy full-equirect path.
        emission: false,
    }
}

fn u2_8k_bounded_ledger(limits: &wgpu::Limits, layers: &ExportLayers) -> Result<u64, String> {
    if layers.emission {
        return Err("U2 8K does not support legacy emission export".into());
    }
    estimated_export_preflight_bytes_with_erosion(8192, layers, 25, limits)
}

fn u2_8k_enabled_layers(layers: &ExportLayers) -> String {
    [
        ("height", layers.height),
        ("albedo", layers.albedo),
        ("normal", layers.normals),
        ("roughness", layers.roughness),
        ("water_mask", layers.water_mask),
        ("clouds", layers.clouds),
        ("emission", layers.emission),
    ]
    .into_iter()
    .filter_map(|(name, enabled)| enabled.then_some(name))
    .collect::<Vec<_>>()
    .join(",")
}

fn record_u2_8k_timings(evidence: &mut CanonicalReport, timings: ExportTimings, total_ms: f64) {
    evidence.generation_inclusive_ms = timings
        .generation_completed
        .then_some(timings.generation_inclusive_ms);
    evidence.erosion_inclusive_ms = timings
        .erosion_completed
        .then_some(timings.erosion_inclusive_ms);
    if timings.export_completed {
        // The bounded export has no preview upload stage; zero is its completed N/A value.
        evidence.upload_sync_ms = Some(0.0);
        // These inclusive wall-clock intervals overlap because encoding and file I/O are one streamed stage.
        evidence.encode_ms = Some(timings.export_inclusive_ms);
        evidence.io_ms = Some(timings.export_inclusive_ms);
    }
    evidence.total_ms = Some(total_ms);
}

fn stage_timing(completed: bool, milliseconds: f64) -> String {
    completed
        .then_some(milliseconds.to_string())
        .unwrap_or_else(|| "NOT_RUN".into())
}

fn u2_8k_artifact(
    layers: &ExportLayers,
    bounded_ledger: u64,
    timings: ExportTimings,
    total_ms: f64,
    outcome: &str,
) -> String {
    format!(
        "mode=8k-cold\nexport_path=direct-exr-staged-png\nenabled_layers={}\nunsupported_legacy=emission\nowned_live_bytes_estimate={bounded_ledger}\ngeneration_inclusive_ms={}\nerosion_inclusive_ms={}\nmeso_erosion_resolution={}\nmeso_erosion_ms={}\ndelta_reconstruction_ms={}\nupload_sync_ms={}\nencode_ms={} (inclusive streamed export)\nio_ms={} (inclusive streamed export; overlaps encode)\ntotal_ms={total_ms}\nresult={outcome}\n",
        u2_8k_enabled_layers(layers),
        stage_timing(
            timings.generation_completed,
            timings.generation_inclusive_ms
        ),
        stage_timing(timings.erosion_completed, timings.erosion_inclusive_ms),
        timings
            .meso_erosion_resolution
            .map(|resolution| resolution.to_string())
            .unwrap_or_else(|| "NOT_RUN".into()),
        stage_timing(timings.meso_erosion_completed, timings.meso_erosion_ms),
        stage_timing(
            timings.delta_reconstruction_completed,
            timings.delta_reconstruction_ms,
        ),
        if timings.export_completed {
            "0 (not applicable: no preview upload)"
        } else {
            "NOT_RUN"
        },
        stage_timing(timings.export_completed, timings.export_inclusive_ms),
        stage_timing(timings.export_completed, timings.export_inclusive_ms),
    )
}

fn plates(params: &PlanetParams, derived: &DerivedProperties) -> Vec<planet_gen::plates::PlateGpu> {
    generate_plates(&PlateGenParams {
        seed: params.seed,
        mass_earth: params.mass_earth,
        ocean_fraction: derived.ocean_fraction,
        tectonics_factor: derived.tectonics_factor,
        continental_scale: 1.0,
        num_plates_override: 0,
        num_continents: 0,
        continent_size_variety: 0.0,
    })
}

fn terrain_params(params: &PlanetParams, derived: &DerivedProperties) -> TerrainGenerationParams {
    TerrainGenerationParams {
        seed: params.seed,
        amplitude: 1.0,
        frequency: 1.2,
        octaves: 8,
        gain: 0.5,
        lacunarity: 2.0,
        mountain_scale: 1.0,
        boundary_width: 0.10,
        warp_strength: 1.0,
        detail_scale: 1.0,
        surface_gravity: 9.81,
        tectonics_factor: derived.tectonics_factor,
        surface_age: derived.surface_age,
        continental_scale: 1.0,
        num_plates_override: 0,
        num_continents: 0,
        continent_size_variety: 0.0,
    }
}

fn run_u2_768() {
    let gpu = Arc::new(GpuContext::new().expect("GPU init failed"));
    let compute = TerrainComputePipeline::new(&gpu);
    let erosion = ErosionPipeline::new(&gpu);
    let preview = PreviewRenderer::new(&gpu);
    let params = PlanetParams::default();
    let derived = DerivedProperties::from_params(&params);
    let plates = plates(&params, &derived);
    let ocean_level = -1.0 + 2.0 * derived.ocean_fraction;
    let terrain = terrain_params(&params, &derived);

    let mut warmup = compute.generate(
        &gpu,
        &plates,
        768,
        terrain.seed,
        terrain.amplitude,
        terrain.frequency,
        terrain.octaves,
        terrain.gain,
        terrain.lacunarity,
        terrain.mountain_scale,
        terrain.boundary_width,
        terrain.warp_strength,
        terrain.detail_scale,
        terrain.surface_gravity,
        terrain.tectonics_factor,
        terrain.surface_age,
        terrain.continental_scale,
    );
    if let Err(error) = erosion.erode(
        &gpu,
        &mut warmup,
        U2_768_PREVIEW_EROSION_ITERATIONS,
        ocean_level,
    ) {
        eprintln!("warmup erosion failed: {error}");
        return;
    }
    let _ = preview.upload_terrain(&gpu, &warmup);

    let total = Instant::now();
    let generation = Instant::now();
    let mut measured = compute.generate(
        &gpu,
        &plates,
        768,
        terrain.seed,
        terrain.amplitude,
        terrain.frequency,
        terrain.octaves,
        terrain.gain,
        terrain.lacunarity,
        terrain.mountain_scale,
        terrain.boundary_width,
        terrain.warp_strength,
        terrain.detail_scale,
        terrain.surface_gravity,
        terrain.tectonics_factor,
        terrain.surface_age,
        terrain.continental_scale,
    );
    let generation_ms = generation.elapsed().as_secs_f64() * 1000.0;
    let erosion_start = Instant::now();
    if let Err(error) = erosion.erode(
        &gpu,
        &mut measured,
        U2_768_PREVIEW_EROSION_ITERATIONS,
        ocean_level,
    ) {
        eprintln!("measured erosion failed: {error}");
        return;
    }
    let erosion_ms = erosion_start.elapsed().as_secs_f64() * 1000.0;
    let upload_start = Instant::now();
    let _ = preview.upload_terrain(&gpu, &measured);
    gpu.device
        .poll(wgpu::PollType::Wait {
            submission_index: None,
            timeout: None,
        })
        .expect("GPU upload synchronization failed");
    let upload_sync_ms = upload_start.elapsed().as_secs_f64() * 1000.0;
    let total_ms = total.elapsed().as_secs_f64() * 1000.0;

    let mut evidence = report(
        &gpu,
        "768-warm",
        768,
        estimated_peak_streaming_bytes(768, 16),
    );
    evidence.generation_inclusive_ms = Some(generation_ms);
    evidence.erosion_inclusive_ms = Some(erosion_ms);
    evidence.upload_sync_ms = Some(upload_sync_ms);
    evidence.total_ms = Some(total_ms);
    evidence.gate_768_warm = if total_ms <= 1000.0 {
        GateStatus::Pass
    } else {
        GateStatus::Fail
    };
    let artifact = format!(
        "mode=768-warm\nerosion_iterations={U2_768_PREVIEW_EROSION_ITERATIONS}\ngeneration_inclusive_ms={generation_ms}\nerosion_inclusive_ms={erosion_ms}\nupload_sync_ms={upload_sync_ms}\ntotal_ms={total_ms}\n"
    );
    let mut journal = StageJournal::new(&evidence.run_id);
    journal.start("terrain_inclusive");
    journal.complete("terrain_inclusive", generation_ms);
    journal.start("erosion_inclusive");
    journal.complete("erosion_inclusive", erosion_ms);
    journal.start("total");
    journal.complete("total", total_ms);
    let journal_artifact = journal.json();
    let publication = publish(
        std::path::Path::new(EVIDENCE_ROOT),
        &evidence,
        &[
            ("benchmark.txt", artifact.as_bytes()),
            ("stage-journal.json", journal_artifact.as_bytes()),
        ],
    )
    .expect("failed to publish U2 768 evidence");
    println!(
        "preset={PRESET}\nmode=768-warm\nadapter={}\ncompletion={}\nmanifest_sha256={}\npath={}",
        evidence.adapter,
        if publication.accepted { "PASS" } else { "FAIL" },
        publication.manifest_digest,
        publication.directory.display()
    );
}

fn run_u2_8k() {
    let gpu = Arc::new(GpuContext::new().expect("GPU init failed"));
    let params = PlanetParams::default();
    let derived = DerivedProperties::from_params(&params);
    let terrain = terrain_params(&params, &derived);
    let layers = u2_8k_layers();
    let bounded_ledger = u2_8k_bounded_ledger(&gpu.device.limits(), &layers)
        .expect("U2 8K bounded export configuration is invalid");
    let mut evidence = report(&gpu, "8k-cold", 8192, bounded_ledger);
    let config = ExportConfig {
        face_resolution: 8192,
        tile_size: 512,
        output_dir: "target/procedural-terrain-exports".into(),
        planet_name: evidence.run_id.clone(),
        erosion_iterations: 25,
        layers,
        weather: WeatherSnapshot {
            seed: params.seed.wrapping_add(1000),
            ..WeatherSnapshot::default()
        },
        night_lights: 0.0,
    };
    let (progress, _progress_rx) = std::sync::mpsc::channel();
    let started = Instant::now();
    let mut timings = ExportTimings::default();
    let mut journal = StageJournal::new(&evidence.run_id);
    journal.peak_owned_live_bytes = Some(bounded_ledger);
    journal.rss_bytes = rss_bytes();
    journal.start("terrain_inclusive");
    journal
        .checkpoint(std::path::Path::new(EVIDENCE_ROOT))
        .expect("failed to checkpoint U2 8K journal");
    let cancel = cancel_after(U2_8K_TIMEOUT);
    let result = run_export_with_timings_and_checkpoints(
        &gpu,
        &config,
        &params,
        &derived,
        terrain.continental_scale,
        0.0,
        terrain,
        &progress,
        &cancel,
        &mut timings,
        &mut |checkpoint| {
            journal.record_layer_checkpoint(checkpoint);
            journal
                .checkpoint(std::path::Path::new(EVIDENCE_ROOT))
                .map(|_| ())
                .map_err(|error| format!("failed to checkpoint U2 layer materialization: {error}"))
        },
    );
    let total_ms = started.elapsed().as_secs_f64() * 1000.0;
    if timings.generation_completed {
        journal.complete("terrain_inclusive", timings.generation_inclusive_ms);
        journal.start("erosion_inclusive");
    }
    if timings.erosion_completed {
        journal.complete("erosion_inclusive", timings.erosion_inclusive_ms);
    }
    if timings.meso_erosion_completed {
        journal.start("meso_erosion");
        journal.complete("meso_erosion", timings.meso_erosion_ms);
    }
    if timings.delta_reconstruction_completed {
        journal.start("delta_reconstruction");
        journal.complete("delta_reconstruction", timings.delta_reconstruction_ms);
    }
    if timings.map_batch_metrics.reached {
        let map_elapsed_ms = timings.map_batch_metrics.elapsed_ms.max(f64::MIN_POSITIVE);
        journal.start("map_gpu_readback");
        if let Some(record) = journal
            .records
            .iter_mut()
            .find(|record| record.name == "map_gpu_readback")
        {
            record.submissions = Some(timings.map_batch_metrics.submissions);
            record.polls = Some(timings.map_batch_metrics.polls);
            record.map_requests = Some(timings.map_batch_metrics.map_requests);
            record.owned_bytes = Some(timings.map_batch_metrics.owned_bytes);
            record.retained_bytes = Some(timings.map_batch_metrics.retained_bytes_after_consume);
        }
        if timings.map_batch_metrics.completed {
            journal.complete("map_gpu_readback", map_elapsed_ms);
        } else {
            journal.fail(
                "map_gpu_readback",
                map_elapsed_ms,
                result
                    .as_ref()
                    .err()
                    .cloned()
                    .unwrap_or_else(|| "map batch did not complete".into()),
            );
        }
    }
    if timings.staged_io_metrics.rows_generated > 0 {
        let staged_elapsed_ms = timings.staged_io_metrics.stage_wall_elapsed_ms();
        journal.start("layer_staging");
        if let Some(record) = journal
            .records
            .iter_mut()
            .find(|record| record.name == "layer_staging")
        {
            record.cache_hits = Some(timings.staged_io_metrics.cache_hits);
            record.cache_misses = Some(timings.staged_io_metrics.region_reads);
            record.cache_capacity_bytes = Some(timings.staged_io_metrics.cache_capacity_bytes());
            record.io_bytes = Some(timings.staged_io_metrics.published_file_bytes);
            record.retained_bytes = Some(timings.staged_io_metrics.peak_cached_bytes());
            record.row_generation_wall_ms = Some(timings.staged_io_metrics.row_generation_wall_ms);
            record.worker_row_generation_wall_sum_ms =
                Some(timings.staged_io_metrics.worker_row_generation_wall_sum_ms);
            record.output_write_ms = Some(timings.staged_io_metrics.output_write_ms);
            record.output_finish_ms = Some(timings.staged_io_metrics.output_finish_ms);
        }
        if timings.staged_io_completed {
            journal.complete("layer_staging", staged_elapsed_ms);
        } else {
            journal.fail(
                "layer_staging",
                staged_elapsed_ms,
                "staged output did not complete",
            );
        }
    }
    if cancel.load(Ordering::Relaxed) {
        journal
            .timeout(
                std::path::Path::new(EVIDENCE_ROOT),
                total_ms,
                format!(
                    "8K deadline exceeded after {} ms",
                    U2_8K_TIMEOUT.as_millis()
                ),
            )
            .expect("failed to checkpoint U2 8K timeout journal");
    } else {
        match &result {
            Ok(_) => journal.complete("total", total_ms),
            Err(error) => journal.fail("total", total_ms, error),
        }
    }
    journal
        .checkpoint(std::path::Path::new(EVIDENCE_ROOT))
        .expect("failed to checkpoint final U2 8K journal");
    record_u2_8k_timings(&mut evidence, timings, total_ms);
    evidence.gate_8k_cold = if result.is_ok() && total_ms <= U2_8K_MAX_MS {
        GateStatus::Pass
    } else {
        GateStatus::Fail
    };
    let outcome = result.map_or_else(|error| error, |path| path.display().to_string());
    let artifact = u2_8k_artifact(&config.layers, bounded_ledger, timings, total_ms, &outcome);
    let journal_artifact = journal.json();
    let publication = publish(
        std::path::Path::new(EVIDENCE_ROOT),
        &evidence,
        &[
            ("benchmark.txt", artifact.as_bytes()),
            ("stage-journal.json", journal_artifact.as_bytes()),
        ],
    )
    .expect("failed to publish U2 8K evidence");
    println!(
        "preset={PRESET}\nmode=8k-cold\nadapter={}\ncompletion={}\nmanifest_sha256={}\npath={}",
        evidence.adapter,
        if publication.accepted { "PASS" } else { "FAIL" },
        publication.manifest_digest,
        publication.directory.display()
    );
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn u2_8k_selects_only_bounded_layers_and_ledger() {
        let layers = u2_8k_layers();
        assert!(layers.height);
        assert!(layers.albedo);
        assert!(layers.normals);
        assert!(layers.roughness);
        assert!(layers.water_mask);
        assert!(layers.clouds);
        assert!(!layers.emission);
        assert_eq!(
            u2_8k_enabled_layers(&layers),
            "height,albedo,normal,roughness,water_mask,clouds"
        );

        let limits = wgpu::Limits::default();
        let bounded = u2_8k_bounded_ledger(&limits, &layers).unwrap();
        assert_eq!(
            bounded,
            estimated_export_preflight_bytes_with_erosion(8192, &layers, 25, &limits).unwrap()
        );
        assert!(bounded <= MAX_8K_OWNED_LIVE_BYTES);
        assert!(bounded < estimated_peak_streaming_bytes(8192, 16));

        let legacy_emission = ExportLayers {
            emission: true,
            ..layers
        };
        assert!(u2_8k_bounded_ledger(&limits, &legacy_emission).is_err());
    }

    #[test]
    fn cold_8k_deadline_is_four_minutes() {
        assert_eq!(U2_8K_TIMEOUT, Duration::from_secs(240));
        assert_eq!(U2_8K_MAX_MS, 240_000.0);
    }

    #[test]
    fn u2_8k_records_complete_truthful_stage_timings() {
        let mut evidence = CanonicalReport::not_run("u2-timing".into(), 42, 1);
        record_u2_8k_timings(
            &mut evidence,
            ExportTimings {
                generation_inclusive_ms: 1.0,
                erosion_inclusive_ms: 2.0,
                export_inclusive_ms: 3.0,
                generation_completed: true,
                erosion_completed: true,
                export_completed: true,
                meso_erosion_resolution: Some(2048),
                meso_erosion_ms: 1.5,
                meso_erosion_completed: true,
                delta_reconstruction_ms: 0.5,
                delta_reconstruction_completed: true,
                ..ExportTimings::default()
            },
            6.0,
        );
        for timing in [
            evidence.generation_inclusive_ms,
            evidence.erosion_inclusive_ms,
            evidence.upload_sync_ms,
            evidence.encode_ms,
            evidence.io_ms,
            evidence.total_ms,
        ] {
            assert!(timing.is_some_and(f64::is_finite));
        }
        assert_eq!(evidence.upload_sync_ms, Some(0.0));
        assert_eq!(evidence.encode_ms, Some(3.0));
        assert_eq!(evidence.io_ms, Some(3.0));
    }

    #[test]
    fn u2_8k_artifact_labels_layers_and_ledger_as_an_estimate() {
        let artifact = u2_8k_artifact(
            &u2_8k_layers(),
            123,
            ExportTimings {
                generation_inclusive_ms: 1.0,
                erosion_inclusive_ms: 2.0,
                export_inclusive_ms: 3.0,
                generation_completed: true,
                erosion_completed: true,
                export_completed: true,
                meso_erosion_resolution: Some(2048),
                meso_erosion_ms: 1.5,
                meso_erosion_completed: true,
                delta_reconstruction_ms: 0.5,
                delta_reconstruction_completed: true,
                ..ExportTimings::default()
            },
            6.0,
            "target/procedural-terrain-exports/u2-test",
        );
        for field in [
            "export_path=direct-exr-staged-png",
            "enabled_layers=height,albedo,normal,roughness,water_mask,clouds",
            "unsupported_legacy=emission",
            "owned_live_bytes_estimate=123",
            "generation_inclusive_ms=1",
            "erosion_inclusive_ms=2",
            "meso_erosion_resolution=2048",
            "meso_erosion_ms=1.5",
            "delta_reconstruction_ms=0.5",
            "upload_sync_ms=0 (not applicable: no preview upload)",
            "encode_ms=3 (inclusive streamed export)",
            "io_ms=3 (inclusive streamed export; overlaps encode)",
        ] {
            assert!(artifact.contains(field), "missing {field}");
        }
    }

    #[test]
    fn u2_8k_early_failure_marks_unexecuted_stages_not_run() {
        let mut evidence = CanonicalReport::not_run("u2-early-failure".into(), 42, 1);
        let timings = ExportTimings::default();
        record_u2_8k_timings(&mut evidence, timings, 0.01);
        assert_eq!(evidence.generation_inclusive_ms, None);
        assert_eq!(evidence.erosion_inclusive_ms, None);
        assert_eq!(evidence.upload_sync_ms, None);
        assert_eq!(evidence.encode_ms, None);
        assert_eq!(evidence.io_ms, None);
        assert_eq!(evidence.total_ms, Some(0.01));

        let artifact = u2_8k_artifact(&u2_8k_layers(), 123, timings, 0.01, "preflight failed");
        for field in [
            "generation_inclusive_ms=NOT_RUN",
            "erosion_inclusive_ms=NOT_RUN",
            "upload_sync_ms=NOT_RUN",
            "encode_ms=NOT_RUN",
            "io_ms=NOT_RUN",
        ] {
            assert!(artifact.contains(field), "missing {field}");
        }
    }

    #[test]
    fn u2_8k_partial_failure_keeps_only_completed_stage_timing() {
        let mut evidence = CanonicalReport::not_run("u2-partial-failure".into(), 42, 1);
        record_u2_8k_timings(
            &mut evidence,
            ExportTimings {
                generation_inclusive_ms: 1.0,
                generation_completed: true,
                ..ExportTimings::default()
            },
            1.5,
        );
        assert_eq!(evidence.generation_inclusive_ms, Some(1.0));
        assert_eq!(evidence.erosion_inclusive_ms, None);
        assert_eq!(evidence.upload_sync_ms, None);
        assert_eq!(evidence.encode_ms, None);
        assert_eq!(evidence.io_ms, None);
    }
}

fn main() {
    match std::env::args().nth(1).as_deref() {
        Some("--u2-768") => {
            run_u2_768();
            return;
        }
        Some("--u2-8k") => {
            run_u2_8k();
            return;
        }
        _ => {}
    }
    if std::env::args().nth(1).as_deref() == Some("--u2-not-run-report") {
        let run_id = format!(
            "u2-{}-seed42-rep0-not-run",
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .expect("system time before epoch")
                .as_secs()
        );
        let publication = publish(
            std::path::Path::new("target/procedural-terrain-evidence"),
            &CanonicalReport::not_run(run_id, 42, 0),
            &[],
        )
        .expect("failed to publish NOT_RUN U2 evidence");
        println!(
            "preset={PRESET}\ncompletion=NOT_RUN\nmanifest_sha256={}\npath={}",
            publication.manifest_digest,
            publication.directory.display()
        );
        return;
    }
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
            &gpu, &plates, warmup_res, seed, 1.0, 1.2, 8, 0.5, 2.0, 1.0, 0.10, 1.0, 1.0, 9.81,
            0.85, 0.2, 1.0,
        );
        if let Err(error) = erosion_pipeline.erode(&gpu, &mut terrain, 5, ocean_level) {
            eprintln!("warmup erosion failed: {error}");
            return;
        }
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
            &gpu, &plates, res, seed, 1.0, 1.2, 8, 0.5, 2.0, 1.0, 0.10, 1.0, 1.0, 9.81, 0.85, 0.2,
            1.0,
        );
        let compute_ms = t1.elapsed().as_secs_f64() * 1000.0;

        // Erosion
        let t2 = Instant::now();
        if let Err(error) =
            erosion_pipeline.erode(&gpu, &mut terrain, erosion_iterations, ocean_level)
        {
            eprintln!("erosion failed at {res}px: {error}");
            return;
        }
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
        &gpu,
        &plates_bench,
        bench_res,
        seed,
        1.0,
        1.2,
        8,
        0.5,
        2.0,
        1.0,
        0.10,
        1.0,
        1.0,
        9.81,
        0.85,
        0.2,
        1.0,
    );
    let classified_ms = t_classified.elapsed().as_secs_f64() * 1000.0;

    println!();
    println!("mode,resolution,compute_ms");
    println!("Classified,{},{:.1}", bench_res, classified_ms);
    println!();
    eprintln!("Classified: {:.1}ms", classified_ms);

    eprintln!("Done.");
}
