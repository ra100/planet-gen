---
title: "perf: Recover deterministic 8K terrain export performance"
type: feat
status: active
date: 2026-08-01
origin: docs/brainstorms/2026-07-28-procedural-terrain-realism-requirements.md
---

# perf: 8K Performance Recovery

## Summary

Recover a trustworthy, bounded 8192 texels-per-face export path without weakening terrain quality or making unmeasured performance claims. The work first makes partial performance evidence durable, then bounds dispatch/readback ownership, runs erosion at a fixed 2048 meso resolution with 8K delta reconstruction, removes avoidable staged reprojection/I/O cost, and publishes an artifact-backed benchmark ladder.

This is a recovery plan for the existing procedural-terrain direction. It does not introduce a simple erosion-pass cap, reduce the correctness halo, change the three-minute 8K budget, or relax seam, drainage, fine-band, spectral, parity, or repeatability requirements.

---

## Existing Evidence and Problem Frame

- `src/bin/perf_bench.rs` already runs a fixed 8K path with 25 erosion iterations, records inclusive generation/erosion/export timing, and rejects totals over 240 seconds. It currently reports streamed encode and I/O as the same overlapping interval and has no dispatch/readback batch ledger.
- `src/perf_evidence.rs` already publishes reports atomically through `<run-id>.incomplete/` and preserves `last-accepted.json`, but a report can be durable while its partial stage evidence is insufficient to attribute a timeout or batch-memory violation.
- `src/export.rs` has a 4 GiB `MAX_8K_OWNED_LIVE_BYTES` cap, correctness-derived stencil radius, staged face export, and tile-size selection against queried device limits. `src/terrain_compute.rs` still has per-tile dispatch/readback boundaries and erosion submission patterns that need explicit bounded batching evidence.
- `src/export_staging.rs` correctly avoids materializing a whole staged cubemap face in the sampler, but its region-at-a-time access and row generation must be profiled before changing cache or I/O behavior.
- The parent terrain-realism plan has a stricter 2 GiB planning target in one section. This recovery plan preserves the implemented and requested 4 GiB owned-live cap as the hard cap. It does not silently lower or raise it; any future cap change needs a separate approved plan and baseline.

### U2 Manual Product Waiver

`target/procedural-terrain-evidence/u2-8k-cold-seed42-rep1-1785691878476508601/manifest.json` records the U2 8K cold seed-42 run at `240335.084225 ms`, over the `240000 ms` gate; `cold_8k=FAIL` and completion is `FAIL`. The paired `stage-journal.json` remains the original failed-stage evidence. The earlier preflight failure at `target/procedural-terrain-evidence/u2-8k-cold-seed42-rep1-1785451466987823955/manifest.json` also remains retained evidence, including its before-allocation 4.5 GiB ownership estimate and `owned_bytes=FAIL` condition.

The user explicitly accepts this U2 8K result as slow for 8K and authorizes proceeding. This is a manual product acceptance waiver only: it unblocks U3 work, does not convert either U2 result into a measured benchmark `PASS`, does not update `last-accepted.json`, and does not weaken any recovery-plan timing, memory, quality, determinism, or final-release acceptance gate.

---

## Non-Negotiable Invariants

| Invariant | Required result | Enforcement |
|---|---|---|
| 8K time | Every cold end-to-end 8K run completes in `<= 240_000 ms`. Thirty seconds remains reported-only. | Maximum of five measured fresh-process runs for each canonical seed. |
| Owned live memory | Explicit owned CPU/GPU buffers, textures, staging, encode rows, and active batch resources peak at `<= 4 GiB`. | Runtime ledger plus preflight estimate; missing ledger is `NOT_RUN`/no-go. |
| Device limits | Every allocation and dispatch shape fits queried wgpu limits. | Record `max_buffer_size`, `max_storage_buffer_binding_size`, texture dimension, alignment, and chosen batch geometry. |
| Halo correctness | Halo is at least `max_map_stencil_radius(face_resolution)` and every consumer receives its required overlap. | Tile tests and adjacent-tile/face comparisons; no benchmark may select a smaller halo. |
| Seams and drainage | Normal seam p95 `<= 5 degrees`, max `<= 15 degrees`; artificial cube-edge drainage terminations `= 0`. | Canonical evaluator artifacts before and after every performance phase. |
| Detail and scale | Fine-band energy after erosion `>= 90%`; resolution-doubling spectral energy delta within `+/-10%`. | Spectral artifacts from canonical seeds. |
| Preview/export parity | Height correlation `>= 0.995`; water-mask IoU `>= 0.98`. | Shared parameters and canonical preview/export comparison. |
| Determinism | Same build/adapter repeats are byte-exact; cross-adapter deviation is measured and within its published tolerance. | Artifact hashes and adapter matrix; missing evidence is no-go. |
| Erosion quality | No simple global erosion-pass cap, no topology-changing progressive shortcut, and no halo reduction. | Phase 3 uses the existing full erosion contract at meso resolution and final topology checks. |

**Canonical controls:** Use `procedural-terrain-768-v1` and `procedural-terrain-8k-v1`, seeds `42` and `997`, current fixed planet/terrain controls, and the full supported 8K layer set. A preset/control change creates a new version and cannot replace the accepted v1 boundary.

---

## Scope Boundaries

### In Scope

- Durable performance evidence that preserves completed stages and marks unrun stages explicitly.
- Bounded GPU command submission, readback, staging, and encode ownership.
- A 2048 meso erosion domain plus deterministic 8K delta reconstruction that preserves drainage topology and residual detail.
- Staged sampling, reprojection, row encoding, and filesystem I/O profiling and optimization.
- Canonical performance and quality publication.

### Out of Scope

- Changing erosion quality by merely lowering the number of erosion passes.
- Reducing a halo below the maximum consumer stencil radius.
- Equal-area/global terrain rewrites, new terrain controls, ML/runtime imports, or a format redesign unrelated to measured export cost.
- Declaring the 30-second aspiration as an acceptance target.

---

## Phase 1. Durable Partial-Evidence Profiling

**Goal:** Make a failed or interrupted benchmark diagnostic rather than ambiguous. Each run must publish immutable identity, selected geometry, completed stage boundaries, partial metrics, and artifact hashes without allowing partial evidence to replace the accepted baseline.

**Dependencies:** None. This phase is required before all recovery changes.

**Files:**
- Modify: `src/perf_evidence.rs`, `src/bin/perf_bench.rs`, `tests/procedural_terrain_perf.rs`
- Modify only if required for measured ownership: `src/export.rs`, `src/export_staging.rs`, `src/terrain_compute.rs`
- Create: `docs/research/8k-performance-baseline.md`

**Implementation:**

1. Extend the canonical report/artifact contract with an ordered stage journal: device/setup, terrain dispatch, terrain readback, erosion dispatch, erosion readback, map/layer staging, reprojection, encode, atomic output publication, and total.
2. Record start/end timestamps, completion state, bytes acquired/released, submissions, map requests, poll waits, selected tile/batch geometry, and per-stage error/timeout reason. A stage not reached is `NOT_RUN`; a reached but failed stage is `FAIL`; neither is serialized as zero.
3. Preserve independent timing boundaries. Encode and I/O may overlap, but the report must state their wall intervals and the measured union; it must not present the same inclusive duration as independent additive work.
4. Publish the journal, configuration fingerprint, adapter/limit snapshot, allocation ledger, and quality-artifact digests through the existing `.incomplete` then atomic-rename protocol. Only a fully passing report updates `last-accepted.json`.
5. Capture an unchanged-code baseline for 768, 2048, 4096, and 8192. The baseline is evidence, not a required pass at every intermediate rung.

**Metrics and gates:**

- Every run has one immutable manifest digest, complete identity fields, queried limits, geometry, and a terminal status for every ordered stage.
- A timeout/error after any completed stage preserves that stage's measured data and marks subsequent stages `NOT_RUN`.
- The owned-live ledger reports peak and per-stage acquire/release totals; absent, underflowing, or nonzero-at-end ownership fails the report.
- Baseline runs retain all quality gates in the invariant table. A failed quality gate is evidence of an existing issue, but it blocks performance-optimization promotion.
- No partial, failed, or `NOT_RUN` run updates `last-accepted.json`.

**Rollback and no-go:**

- Rollback is the current `last-accepted.json` evidence directory and current benchmark schema.
- No-go for Phase 2 if stage ordering is incomplete, timings cannot distinguish dispatch/readback/reprojection/output, ownership cannot be reconciled to zero at run end, or durable publication is not atomic.

---

## Phase 2. Bounded GPU Dispatch and Readback Batching

**Goal:** Replace unbounded or per-item synchronization behavior with a measured batch contract that owns only a bounded number of tiles/readbacks at once and synchronizes only at intentional batch boundaries.

**Dependencies:** Phase 1 passing its durable-evidence gate.

**Files:**
- Modify: `src/terrain_compute.rs`, `src/export.rs`, `src/export_staging.rs`, `src/bin/perf_bench.rs`
- Modify tests: `tests/export_streaming.rs`, `tests/procedural_terrain_perf.rs`

**Implementation:**

1. Define one explicit export batch as a bounded set of same-layer tile dispatches, output buffers, staging buffers, map requests, and row/region consumers. The batch size is selected only from queried device limits and the 4 GiB live-owned ledger.
2. Encode commands for the batch before queue submission, issue readbacks after the copies, and perform `map_async`/`poll` at the batch completion boundary rather than after every intermediate compute dispatch. Keep cancellation/error handling at every boundary.
3. Release/unmap staging buffers and discard CPU tile data immediately after the corresponding crop is consumed by the staged writer. Do not retain six full faces, prior layer buffers, or completed batch mappings.
4. Keep tile selection benchmarkable, but retain `TileCoordinator::stencil_radius = max_map_stencil_radius(face_resolution)` exactly. A smaller halo is not a candidate configuration.
5. Emit batch evidence: dispatch count, tiles, bytes by resource class, map requests, poll count/wait time, readback latency, and zero retained completed-batch resources.

**Metrics and gates:**

- Runtime peak owned-live bytes `<= 4 GiB` for every canonical 8K run; preflight and runtime ledger peaks must agree within documented transient allocator accounting.
- Every buffer/texture fits queried limits; selected tile size divides face resolution; required halo is unchanged and all crop comparisons match the monolithic reference.
- Batching must not increase 8K total time or any stage's p95 by more than 5% versus Phase 1 baseline unless the phase simultaneously removes an observed ownership or synchronization failure. The next phase must recover that regression before promotion.
- Batch evidence shows no completed batch retains mapped/readback resources after writer consumption.
- Existing seam, drainage, fine-band, spectral, parity, and repeatability gates all pass unchanged.

**Rollback and no-go:**

- Roll back to the Phase 1 accepted evidence/configuration if batched output differs bytewise on the same adapter, the ledger exceeds 4 GiB, a device-limit failure occurs, or halo/tile seam validation fails.
- No-go for Phase 3 if batching is only a pass-count reduction, requires a reduced halo, leaves resources retained across batches, or cannot prove bounded runtime ownership.

---

## Phase 3. 2048 Meso Erosion and 8K Delta Reconstruction

**Goal:** Move the erosion working domain to 2048 texels per face while reconstructing the 8K result from the un-eroded deterministic high-frequency delta plus the eroded meso field. Preserve the existing erosion behavior at its chosen full pass contract; reduce spatial domain, not erosion fidelity by arbitrary pass capping.

**Dependencies:** Phase 2 accepted bounded batch contract and Phase 1 baseline artifacts.

**Files:**
- Modify: `src/terrain_compute.rs`, `src/shaders/erosion.wgsl`, `src/shaders/terrain_from_plates.wgsl`, `src/shaders/cube_sphere.wgsl`, `src/export.rs`, `src/bin/perf_bench.rs`
- Modify tests: `src/terrain_compute.rs`, `tests/export_streaming.rs`, `tests/procedural_terrain_eval_cli.rs`, `tests/procedural_terrain_perf.rs`

**Implementation:**

1. Generate a deterministic 2048 meso input from the same seed, terrain parameters, macro/meso fields, precipitation/runoff, and cross-face sampling contract used by 8K export.
2. Run the established erosion pass sequence and stable tie/order rules on this meso domain. Do not introduce a new simple pass cap as the recovery mechanism.
3. Construct the 8K eroded height as cross-face reconstruction of the 2048 eroded meso field plus the deterministic 8K residual delta defined against the matching un-eroded meso reconstruction.
4. Carry and revalidate the drainage topology/channel mask after reconstruction. Delta application is suppressed or constrained only where required to preserve protected channels; it must not create, remove, divert, or face-terminate a channel.
5. Use direction-based cross-face sampling for reconstruction. Tile halos remain consumer-derived and are not used to hide interpolation discontinuities.

**Metrics and gates:**

- Fine-band energy after erosion is `>= 90%` of the pre-erosion reference for both seeds.
- Resolution-doubling spectral energy delta stays within `+/-10%`.
- Artificial cube-edge drainage terminations remain `0`; protected channel topology is identical before/after delta reconstruction.
- Normal seams remain p95 `<= 5 degrees`, max `<= 15 degrees`.
- Preview/export height correlation remains `>= 0.995`; water-mask IoU remains `>= 0.98`.
- Same build/adapter repeated reconstruction artifacts are byte-exact. Cross-adapter comparison is recorded, not assumed exact.
- Phase 3 reduces the measured erosion-inclusive 8K critical-path time by at least 30% from the Phase 2 accepted median, or it is not promoted. This is a recovery threshold, not a substitute for the three-minute end-to-end gate.

**Rollback and no-go:**

- Roll back to the Phase 2 full-resolution erosion path and preserve Phase 3 artifacts as research-only if any topology, seam, fine-band, spectral, parity, or repeatability gate fails.
- No-go for Phase 4 if the 2048 path meets time only by reducing erosion passes, relaxing channel protection, changing the canonical controls, reducing halo, or weakening any quality threshold.

---

## Phase 4. Staged Reprojection and I/O Optimization

**Goal:** Remove measured reprojection and output stalls after GPU work is bounded, without changing output pixels, layer semantics, atomic publication, or the quality contract.

**Dependencies:** Phase 3 accepted reconstruction and Phase 2 batch ledger.

**Files:**
- Modify: `src/export.rs`, `src/export_staging.rs`, `src/openexr_writer.rs`, `src/png_writer.rs`, `src/bin/perf_bench.rs`
- Modify tests: `tests/export_streaming.rs`, `tests/procedural_terrain_perf.rs`, `tests/procedural_terrain_eval_cli.rs`

**Implementation:**

1. Profile staged sampler region reads, cache hits, row generation, encode writes, flushes, fsyncs, and atomic rename separately using Phase 1's stage journal.
2. Optimize only the largest measured reprojection/I/O contributor. Permitted changes include traversal order matching equirectangular rows, bounded region-cache sizing, batched row writes, and avoiding redundant format conversion/copies.
3. Keep the staged sampler below one full face and account cache, row, writer, and conversion buffers in the owned-live ledger.
4. Preserve atomic EXR/PNG publication semantics: incomplete files are never accepted outputs; only fully closed, synced, and atomically published output can enter evidence.
5. Confirm that every layer remains generated from the same reconstructed terrain and has the exact requested dimensions, crop, and orientation.

**Metrics and gates:**

- Reprojection plus output union-wall time improves by at least 20% from the Phase 3 accepted median, or the change is not retained.
- No individual write/flush/fsync error can report a completed output; failed output leaves no accepted artifact or pointer update.
- Runtime owned-live bytes remain `<= 4 GiB`; staged sampler cache remains smaller than one complete face.
- Pixel hashes for each canonical same-adapter layer match the Phase 3 accepted artifact; if a format path is intentionally nondeterministic, it is not eligible for promotion.
- All invariant quality, seam, drainage, spectral, fine-band, parity, and repeatability gates pass.

**Rollback and no-go:**

- Roll back to the previous staged traversal/writer configuration if output hashes differ, publication atomicity/durability regresses, cache ownership is unbounded, or improvement is below 20%.
- No-go for Phase 5 if the optimized path lacks a durable artifact per requested layer, changes orientation/crop semantics, or only moves time outside the measured workload.

---

## Phase 5. Benchmark Ladder and Acceptance Publication

**Goal:** Run and publish the full evidence ladder so the recovered path becomes default only after all hard performance and scientific-quality gates pass together.

**Dependencies:** Phases 1 through 4 accepted.

**Files:**
- Modify: `src/bin/perf_bench.rs`, `src/perf_evidence.rs`, `tests/procedural_terrain_perf.rs`, `tests/procedural_terrain_eval_cli.rs`
- Create: `docs/research/8k-performance-recovery-validation.md`
- Update after acceptance only: `Plans.md` and this plan frontmatter/status

**Benchmark ladder:**

1. CPU/unit contract: evidence serialization, partial-stage failure, owned-live acquire/release, batch-bound calculation, halo immutability, 2048-to-8K delta topology protection, staged output atomicity.
2. GPU smoke: 768 and 2048 canonical seed runs validate dispatch geometry, ledger, same-adapter repeat, and all scientific gates before expensive export.
3. Scaling characterization: 2048, 4096, and 8192 record each stage journal and owned-live peak. These runs identify nonlinear cost; they do not average away any failure.
4. 768 gate: for each seed, five fresh-process runs with the existing synchronized preconditioning/invalidation protocol. Every measured warm regeneration must be `<= 1_000 ms`.
5. 8K gate: for each seed, one ignored fresh-process warm-up followed by five measured fresh-process cold end-to-end exports. Every one includes generation, 2048 meso erosion, delta reconstruction, required halo/readback, reprojection, encode, atomic output publication, and I/O; every total must be `<= 240_000 ms`.
6. Adapter matrix: repeat the accepted protocol on each representative adapter. Same-adapter artifacts must be exact; cross-adapter deviation must be published against an observed, approved tolerance.

**Acceptance gates:**

- The hard time gate is the maximum, not median or p95, of the five 8K cold runs for each seed: `<= 240_000 ms`.
- The hard owned-live cap is `<= 4 GiB` for every 8K run. Missing runtime ledger, limit snapshot, RSS record where supported, stage record, layer artifact, or atomic manifest is `NOT_RUN` and no-go.
- 768 maximum is `<= 1_000 ms` for every canonical measured warm regeneration.
- All invariant quality gates pass for every canonical seed and accepted adapter: seams, zero drainage terminations, fine-band, spectral delta, parity, exact same-adapter repeats, and documented cross-adapter tolerance.
- Publish immutable evidence below `target/procedural-terrain-evidence/`, update `last-accepted.json` only after a complete passing report, and record evidence IDs, rejected configurations, geometry, and full stage timings in `docs/research/8k-performance-recovery-validation.md`.

**Rollback and no-go:**

- A failed run never replaces the last accepted pointer. Restore the last accepted Phase 4 configuration and evidence boundary; retain failure artifacts for diagnosis.
- No-go for default promotion if any hard run fails, any metric is `NOT_RUN`, the cap exceeds 4 GiB, 8K exceeds three minutes, or any scientific-quality requirement regresses.
- The 30-second result is published as an aspiration only. It cannot override a failed three-minute, quality, or determinism gate.

---

## Cross-Phase Verification Matrix

| Area | Required evidence | Stop condition |
|---|---|---|
| Evidence durability | Atomic incomplete-to-complete directory, synced manifest, preserved last accepted pointer, partial-stage journal | Partial or failed evidence overwrites last accepted state. |
| Memory and dispatch | Queried limits, batch geometry, acquire/release ledger, runtime peak `<= 4 GiB` | Unbounded completed-batch resources, invalid ledger, or cap breach. |
| Erosion reconstruction | 2048 meso artifact, 8K delta artifact, protected topology comparison | Simple pass cap, changed drainage topology, or quality regression. |
| Export correctness | Maximum consumer halo, tile and face seam crop comparisons, atomic layer outputs | Any halo reduction, seam, orientation, or crop mismatch. |
| Scientific quality | Seam, drainage, fine-band, spectral, parity, and determinism reports | Any threshold failure or missing report. |
| Release performance | Five-run per-seed 768/8K protocol and adapter matrix | Any run exceeds its hard maximum or fails output completion. |

---

## Documentation and Rollout

- `docs/research/8k-performance-baseline.md` records Phase 1's unchanged-code evidence and the selected rollback boundary.
- `docs/research/8k-performance-recovery-validation.md` records accepted/rejected configurations, phase deltas, per-stage timing, owned-live/RSS measurements, adapters, hashes, scientific gates, and final stop/go decision.
- Update `Plans.md` only after all Phase 5 acceptance gates pass, using the repository's `cc:完了 [commit_hash]` convention. Mark this plan completed only then.
- No code, configuration, or documentation outside this plan is changed by planning itself.

---

## Sources

- Requirements and hard quality budgets: `docs/brainstorms/2026-07-28-procedural-terrain-realism-requirements.md`
- Parent terrain plan and existing U2/U5/U6/U7 contracts: `docs/plans/2026-07-28-003-feat-procedural-terrain-realism-plan.md`
- Current benchmark/evidence path: `src/bin/perf_bench.rs`, `src/perf_evidence.rs`, `tests/procedural_terrain_perf.rs`
- Current tiled export/staging and halo contract: `src/export.rs`, `src/export_staging.rs`, `tests/export_streaming.rs`
- Terrain/erosion dispatch and readback path: `src/terrain_compute.rs`, `src/shaders/erosion.wgsl`, `src/shaders/terrain_from_plates.wgsl`
