use planet_gen::export::LayerMaterializationCheckpoint;
use planet_gen::perf_evidence::{
    publish, publish_with_failpoint, sha256, AcceptanceProfile, CanonicalReport, GateStatus,
    PublishFailpoint, StageJournal, PRESET,
};

fn root(name: &str) -> std::path::PathBuf {
    std::env::temp_dir().join(format!("planet-gen-{name}-{}", std::process::id()))
}

#[test]
fn cancellation_after_submit_preserves_map_batch_metrics_in_journal() {
    let root = root("perf-journal");
    let _ = std::fs::remove_dir_all(&root);
    let mut journal = StageJournal::new("u2-partial");
    journal.peak_owned_live_bytes = Some(123);
    journal.start("terrain_inclusive");
    journal.complete("terrain_inclusive", 12.5);
    journal.start("map_gpu_readback");
    {
        let map = journal
            .records
            .iter_mut()
            .find(|record| record.name == "map_gpu_readback")
            .unwrap();
        map.submissions = Some(2);
        map.polls = Some(3);
        map.map_requests = Some(8);
        map.owned_bytes = Some(4096);
        map.retained_bytes = Some(0);
    }
    journal.fail(
        "map_gpu_readback",
        13.0,
        "Cancelled while waiting for map batch readback",
    );
    let path = journal.timeout(&root, 13.0, "benchmark timeout").unwrap();
    let saved = std::fs::read_to_string(path).unwrap();
    assert!(saved.contains("\"name\":\"terrain_inclusive\",\"status\":\"PASS\""));
    assert!(saved.contains("\"name\":\"terrain_readback\",\"status\":\"NOT_RUN\""));
    assert!(saved.contains("\"submissions\":\"NOT_RUN\""));
    assert!(saved.contains("\"submissions\":2,\"polls\":3,\"map_requests\":8"));
    assert!(saved.contains("\"owned_bytes\":4096,\"retained_bytes\":0"));
    assert!(saved.contains("\"name\":\"map_gpu_readback\",\"status\":\"FAIL\""));
    assert!(saved.contains("\"name\":\"map_gpu_readback\",\"status\":\"FAIL\",\"elapsed_ms\":13"));
    assert!(saved.contains("\"reason\":\"benchmark timeout\""));
    assert!(!root.join("last-accepted.json").exists());
    let _ = std::fs::remove_dir_all(root);
}

#[test]
fn cancellation_before_map_leaves_map_readback_unmeasured() {
    let journal = StageJournal::new("u2-before-map");
    let json = journal.json();
    assert!(json.contains("\"name\":\"map_gpu_readback\",\"status\":\"NOT_RUN\""));
    assert!(json.contains("\"submissions\":\"NOT_RUN\""));
}

#[test]
fn layer_staging_journal_preserves_cache_and_row_io_metrics() {
    let mut journal = StageJournal::new("u2-staged-io");
    journal.start("layer_staging");
    {
        let stage = journal
            .records
            .iter_mut()
            .find(|record| record.name == "layer_staging")
            .unwrap();
        stage.cache_hits = Some(12);
        stage.cache_misses = Some(3);
        stage.io_bytes = Some(4096);
        stage.retained_bytes = Some(1024);
        stage.row_generation_ms = Some(1.5);
        stage.worker_row_generation_ms = Some(6.0);
        stage.output_write_ms = Some(2.0);
        stage.output_finish_ms = Some(0.5);
    }
    journal.complete("layer_staging", 4.0);
    let json = journal.json();
    assert!(json.contains("\"name\":\"layer_staging\",\"status\":\"PASS\""));
    assert!(json.contains("\"cache_hits\":12,\"cache_misses\":3,\"cache_capacity_bytes\":\"NOT_RUN\",\"io_bytes\":4096"));
    assert!(
        json.contains("\"row_generation_ms\":1.5,\"worker_row_generation_ms\":6,\"output_write_ms\":2,\"output_finish_ms\":0.5")
    );
}

#[test]
fn complete_layer_materialization_checkpoint_is_durable() {
    let root = root("layer-checkpoint-complete");
    let _ = std::fs::remove_dir_all(&root);
    let mut journal = StageJournal::new("u2-layer-complete");
    journal.record_layer_checkpoint(&LayerMaterializationCheckpoint {
        layer: "albedo",
        rows_completed: 8192,
        rows_total: 8192,
        cache_hits: 12,
        cache_misses: 3,
        region_reads: 3,
        peak_cached_bytes: 128,
        cache_capacity_bytes: 256,
        intermediate_write_ms: 1.5,
        intermediate_write_bytes: 4096,
        completed: true,
        reason: None,
    });
    let saved = std::fs::read_to_string(journal.checkpoint(&root).unwrap()).unwrap();
    assert!(saved.contains("\"layer\":\"albedo\",\"status\":\"PASS\""));
    assert!(saved.contains("\"rows_completed\":8192,\"rows_total\":8192"));
    assert!(saved.contains(
        "\"cache_hits\":12,\"cache_misses\":3,\"region_reads\":3,\"peak_cached_bytes\":128,\"cache_capacity_bytes\":256"
    ));
    assert!(saved.contains("\"intermediate_write_ms\":1.5,\"intermediate_write_bytes\":4096"));
    let _ = std::fs::remove_dir_all(root);
}

#[test]
fn cancelled_layer_materialization_checkpoint_preserves_partial_values() {
    let mut journal = StageJournal::new("u2-layer-cancelled");
    journal.record_layer_checkpoint(&LayerMaterializationCheckpoint {
        layer: "normal",
        rows_completed: 256,
        rows_total: 8192,
        cache_hits: 7,
        cache_misses: 2,
        region_reads: 2,
        peak_cached_bytes: 64,
        cache_capacity_bytes: 128,
        intermediate_write_ms: 0.5,
        intermediate_write_bytes: 1024,
        completed: false,
        reason: Some("Cancelled".into()),
    });
    let json = journal.json();
    assert!(json.contains("\"layer\":\"normal\",\"status\":\"FAIL\""));
    assert!(json.contains("\"rows_completed\":256,\"rows_total\":8192"));
    assert!(json.contains("\"reason\":\"Cancelled\""));
}

#[test]
fn pre_row_cancellation_reports_zero_actual_cache_and_explicit_capacity() {
    let mut journal = StageJournal::new("u2-layer-pre-row-cancelled");
    journal.record_layer_checkpoint(&LayerMaterializationCheckpoint {
        layer: "height",
        rows_completed: 0,
        rows_total: 8192,
        cache_hits: 0,
        cache_misses: 0,
        region_reads: 0,
        peak_cached_bytes: 0,
        cache_capacity_bytes: 1024,
        intermediate_write_ms: 0.0,
        intermediate_write_bytes: 0,
        completed: false,
        reason: Some("Cancelled".into()),
    });
    let json = journal.json();
    assert!(json.contains("\"rows_completed\":0,\"rows_total\":8192"));
    assert!(json.contains("\"peak_cached_bytes\":0,\"cache_capacity_bytes\":1024"));
    assert!(json.contains("\"reason\":\"Cancelled\""));
}

fn passing_report(run_id: &str) -> CanonicalReport {
    let mut report = CanonicalReport::not_run(run_id.into(), 42, 1);
    report.adapter = "adapter\"name".into();
    report.build_fingerprint = "build\\fingerprint".into();
    report.config_fingerprint = "config\nvalue".into();
    report.generation_inclusive_ms = Some(1.0);
    report.erosion_inclusive_ms = Some(2.0);
    report.upload_sync_ms = Some(3.0);
    report.encode_ms = Some(5.0);
    report.io_ms = Some(6.0);
    report.total_ms = Some(21.0);
    report.owned_live_bytes = Some(1);
    report.rss_bytes = Some(2);
    report.max_buffer_size = Some(3);
    report.max_storage_buffer_binding_size = Some(4);
    report.max_texture_dimension_2d = Some(5);
    report.gate_768_warm = GateStatus::Pass;
    report.gate_8k_cold = GateStatus::Pass;
    report.gate_owned_bytes = GateStatus::Pass;
    report.gate_rss = GateStatus::Pass;
    report
}

#[test]
fn canonical_v1_report_schema() {
    let root = root("perf-schema");
    let _ = std::fs::remove_dir_all(&root);
    let publication = publish(
        &root,
        &CanonicalReport::not_run("u2-schema".into(), 42, 1),
        &[("control.bin", b"control")],
    )
    .unwrap();
    let manifest = std::fs::read_to_string(publication.directory.join("manifest.json")).unwrap();

    for field in [
        "\"preset\":\"procedural-terrain-u2-v1\"",
        "\"seed\":42",
        "\"adapter\":\"NOT_RUN\"",
        "\"generation_inclusive\":\"NOT_RUN\"",
        "\"upload_sync\":\"NOT_RUN\"",
        "\"owned_live\":\"NOT_RUN\"",
        "\"rss\":\"NOT_RUN\"",
        "\"max_buffer_size\":\"NOT_RUN\"",
        "\"completion\":\"NOT_RUN\"",
    ] {
        assert!(manifest.contains(field), "missing {field}");
    }
    assert_eq!(
        sha256(b"abc"),
        "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad"
    );
    assert!(manifest.contains(&sha256(b"control")));
    assert!(!manifest.contains("readback"));
    assert_eq!(publication.manifest_digest, sha256(manifest.as_bytes()));
    assert!(!publication.accepted);
    assert_eq!(PRESET, "procedural-terrain-u2-v1");
    let _ = std::fs::remove_dir_all(root);
}

#[test]
fn p2_manifest_is_durable_atomic_and_preserves_last_accepted() {
    let root = root("p2-publication");
    let _ = std::fs::remove_dir_all(&root);
    std::fs::create_dir_all(&root).unwrap();
    std::fs::write(root.join("last-accepted.json"), "previous").unwrap();

    let incomplete = publish(
        &root,
        &CanonicalReport::not_run("u2-incomplete".into(), 42, 1),
        &[("artifact.bin", b"artifact")],
    )
    .unwrap();
    assert!(incomplete.directory.join("manifest.json").is_file());
    assert!(!root.join("u2-incomplete.incomplete").exists());
    assert_eq!(
        std::fs::read_to_string(root.join("last-accepted.json")).unwrap(),
        "previous"
    );

    let accepted = passing_report("u2-accepted");
    let complete = publish(&root, &accepted, &[("artifact.bin", b"artifact")]).unwrap();
    let pointer = std::fs::read_to_string(root.join("last-accepted.json")).unwrap();
    let manifest = std::fs::read_to_string(complete.directory.join("manifest.json")).unwrap();
    assert!(complete.accepted);
    assert!(pointer.contains("u2-accepted"));
    assert!(pointer.contains(&complete.manifest_digest));
    assert!(manifest.contains("adapter\\\"name"));
    assert!(manifest.contains("build\\\\fingerprint"));
    assert!(manifest.contains("config\\nvalue"));
    let _ = std::fs::remove_dir_all(root);
}

#[test]
fn warm_768_acceptance_requires_only_warm_measurements_and_publishes_journal() {
    let root = root("warm-768-publication");
    let _ = std::fs::remove_dir_all(&root);
    std::fs::create_dir_all(&root).unwrap();
    std::fs::write(root.join("last-accepted.json"), "cold-8k-baseline").unwrap();
    let mut report = passing_report("u2-768-success");
    report.acceptance_profile = AcceptanceProfile::Warm768;
    report.gate_8k_cold = GateStatus::NotRun;
    report.encode_ms = None;
    report.io_ms = None;
    let mut journal = StageJournal::new(&report.run_id);
    journal.start("terrain_inclusive");
    journal.complete("terrain_inclusive", 1.0);
    journal.start("total");
    journal.complete("total", 6.0);
    let publication = publish(
        &root,
        &report,
        &[("stage-journal.json", journal.json().as_bytes())],
    )
    .unwrap();
    let manifest = std::fs::read_to_string(publication.directory.join("manifest.json")).unwrap();
    assert!(publication.accepted);
    assert!(manifest.contains("\"completion\":\"PASS\""));
    assert!(publication.directory.join("stage-journal.json").is_file());
    assert_eq!(
        std::fs::read_to_string(root.join("last-accepted.json")).unwrap(),
        "cold-8k-baseline"
    );

    let mut cold = passing_report("u2-8k-success");
    cold.acceptance_profile = AcceptanceProfile::Cold8k;
    cold.gate_768_warm = GateStatus::NotRun;
    let cold_publication = publish(&root, &cold, &[("stage-journal.json", b"{}")]).unwrap();
    assert!(cold_publication.accepted);
    assert!(std::fs::read_to_string(root.join("last-accepted.json"))
        .unwrap()
        .contains("u2-8k-success"));
    let _ = std::fs::remove_dir_all(root);
}

#[test]
fn incomplete_or_failed_reports_never_advance_last_accepted() {
    let root = root("perf-eligibility");
    let _ = std::fs::remove_dir_all(&root);
    std::fs::create_dir_all(&root).unwrap();
    std::fs::write(root.join("last-accepted.json"), "previous").unwrap();

    let mut missing_identity = passing_report("missing-identity");
    missing_identity.adapter = "NOT_RUN".into();
    let missing = publish(&root, &missing_identity, &[]).unwrap();
    assert!(!missing.accepted);
    assert!(
        std::fs::read_to_string(missing.directory.join("manifest.json"))
            .unwrap()
            .contains("\"completion\":\"FAIL\"")
    );
    assert_eq!(
        std::fs::read_to_string(root.join("last-accepted.json")).unwrap(),
        "previous"
    );

    let mut missing_measurement = passing_report("missing-measurement");
    missing_measurement.upload_sync_ms = None;
    let missing = publish(&root, &missing_measurement, &[]).unwrap();
    assert!(!missing.accepted);
    assert!(
        std::fs::read_to_string(missing.directory.join("manifest.json"))
            .unwrap()
            .contains("\"completion\":\"FAIL\"")
    );
    assert_eq!(
        std::fs::read_to_string(root.join("last-accepted.json")).unwrap(),
        "previous"
    );

    let mut failed = passing_report("failed-gate");
    failed.gate_rss = GateStatus::Fail;
    let failure = publish(&root, &failed, &[]).unwrap();
    assert!(!failure.accepted);
    assert!(
        std::fs::read_to_string(failure.directory.join("manifest.json"))
            .unwrap()
            .contains("\"completion\":\"FAIL\"")
    );
    assert_eq!(
        std::fs::read_to_string(root.join("last-accepted.json")).unwrap(),
        "previous"
    );
    let _ = std::fs::remove_dir_all(root);
}

#[test]
fn failed_completion_keeps_recoverable_staging_and_pointer() {
    let root = root("perf-recovery");
    let _ = std::fs::remove_dir_all(&root);
    std::fs::create_dir_all(&root).unwrap();
    std::fs::write(root.join("last-accepted.json"), "previous").unwrap();

    let error = match publish_with_failpoint(
        &root,
        &passing_report("rename-failure"),
        &[("artifact.bin", b"artifact")],
        Some(PublishFailpoint::BeforeRename),
    ) {
        Ok(_) => panic!("injected rename failure must fail"),
        Err(error) => error,
    };
    assert_eq!(error.kind(), std::io::ErrorKind::Other);
    let staging = root.join("rename-failure.incomplete");
    assert!(staging.join("artifact.bin").is_file());
    assert!(staging.join("manifest.json").is_file());
    assert_eq!(
        std::fs::read_to_string(root.join("last-accepted.json")).unwrap(),
        "previous"
    );
    let _ = std::fs::remove_dir_all(root);
}
