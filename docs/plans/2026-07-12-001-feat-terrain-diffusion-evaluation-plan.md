---
title: "feat: Terrain Diffusion offline artifact evaluation"
type: feat
status: active
date: 2026-07-12
origin: docs/brainstorms/terrain-diffusion-integration-requirements.md
---

# Terrain Diffusion Offline Artifact Evaluation

## Overview

This time-boxed research spike determines whether Terrain Diffusion can safely augment Planet Gen through offline, validated terrain artifacts. It does not add a runtime provider, inference engine, cloud route, or product source selector.

## Requirements Trace

- R1-R6: evaluation boundary, reproduction, cubemap validation, preview comparison, resource measurement, decision.
- Deferred R7-R10 are explicitly outside this plan.

## Decision Flow

1. Confirm rights and reproducibility boundaries.
2. Reproduce upstream-supported planar output from a pinned revision.
3. Evaluate conversion to Planet Gen's canonical scalar-height cubemap contract.
4. Compare preview-only output and measure costs.
5. Decide whether imported-artifact productization warrants a new plan.

## Fixed Spike Evaluation Protocol

These **spike acceptance thresholds are not product SLAs**. Freeze the protocol, the selected NVIDIA target GPU, and the procedural baseline before U2 begins; do not relax a failed threshold during U2 or U3. Record raw samples, scripts or commands, and the resulting evidence with the manifest so a reviewer can recompute every statistic.

| Area | Measurement and spike acceptance threshold |
|---|---|
| Artifact validity | Canonical output has exactly six faces at the requested dimensions and documented face orientation; every sample is finite. Any NaN, Inf, dimension mismatch, or orientation mismatch fails. |
| Height continuity | Across all 12 cubemap edges, sample paired border heights after the declared orientation transform and compare their absolute jumps with interior neighbor-gradient jumps from the same faces and the procedural baseline at the same resolution. The candidate p95 edge jump must be <= `max(1.5 x p95 interior neighbor jump, procedural baseline p95 edge jump x 1.10)`. At all 8 corners, compare every incident-face height after the same transform; the p95 corner mismatch must satisfy the same normalized rule. |
| Normal continuity | Derive normals from the declared height-to-surface mapping on both sides of every shared edge. The candidate p95 cross-edge angular error must be <= `max(5 degrees, procedural baseline p95 + 2 degrees)` and the maximum must be <= 15 degrees. |
| Pole artifacts | Derive latitude from the existing cube-face direction mapping, using the mapped unit direction's latitude so classification is deterministic and face-independent. For each hemisphere separately, define polar-cap samples as `|lat| >= 75 degrees` and the adjacent comparison band as `60 degrees <= |lat| < 75 degrees`. Require finite normalized-elevation and slope-magnitude values everywhere. For each metric, compare the predeclared p10, p50, and p90 cap quantiles against its same-hemisphere adjacent-band quantile. Each quantile passes when relative deviation is <= 20%; when the comparison-band magnitude is below 0.05 of the normalized full-range scale, use absolute deviation <= 0.02 instead. A recorded visual inspection must also reject any radial or ring seam. |
| Determinism | Re-run identical inputs with identical provenance, including source revision, checkpoint revision/hash, environment, hardware/driver/software revisions, seed, sampler, conditioning, projection, and command. Canonical output hashes must be byte-identical. If upstream floating-point behavior prevents this, record a determinism failure; do not silently weaken this gate. |
| Runtime and resources | On the frozen selected NVIDIA target GPU, capture wall-clock time to first tile (TTFT), elapsed generation/conversion/upload time, peak VRAM, peak RAM, cache size, GPU model, driver, CUDA/PyTorch versions, OS, and source/checkpoint revisions. At 512x512 T=2: TTFT <= 5 s and peak VRAM <= 4 GiB. At 1024x1024 T=2: elapsed time <= 10 s and peak VRAM <= 6 GiB. These are gates for continued evaluation, not end-user runtime promises. |
| Blind visual comparison | At least 3 reviewers inspect randomized, source-blinded procedural and candidate outputs using 1-5 scores for global-structure preservation, local-detail quality, seams/poles, controllability, and obvious model artifacts. Candidate median overall must be >= 4, median local-detail improvement over procedural must be >= 1 point, and no seams/poles/artifact-category median may be below 4. If three reviewers cannot be obtained, productization is blocked rather than reducing the review count. |

The decision record must include the procedural baseline inputs and measurements, per-edge and per-corner samples, north and south pole quantiles, normal-angle distribution, hashes, reviewer score sheets, and resource captures. A gate is pass/fail; an absent measurement is a fail.

## Implementation Units

- [ ] **U1: Define evaluation boundary and evidence manifest**

**Requirements:** R1
**Dependencies:** None
**Artifacts:** `docs/research/terrain-diffusion-integration.md`, evaluation manifest template/location documented in the spike record.

**Approach:** Record the pinned upstream revision, checkpoint/model identity, intended local-only asset handling, seed, sampler, conditioning, projection candidate, and content hashing fields. Record unresolved WorldClim/MERIT licensing questions as blockers, not conclusions.

**Evaluation scenarios:** MIT code/model-card review; downstream asset-term review; reproducibility record completeness.

**Verification outcome:** A reviewer can identify exactly what was evaluated and why no weights, cloud service, or generated assets are being shipped.

- [ ] **U2: Reproduce a bounded upstream planar evaluation**

**Requirements:** R2
**Dependencies:** U1
**Artifacts:** Local evaluation record with command-independent environment summary, inputs, output hashes, elapsed time, peak VRAM/RAM, and failures if any.

**Approach:** Use an upstream-supported workflow at a pinned revision. Preserve the Python pipeline as an offline authoring tool; do not attempt ONNX completion or Rust embedding.

**Evaluation scenarios:** Deterministic repeated input; public 30 m or 90 m checkpoint availability; failure capture when required hardware or dependencies are unavailable.

**Verification outcome:** Either a reproducible planar result that passes the fixed protocol below or an explicit reproducibility stop condition.

- [ ] **U3: Evaluate cubemap projection, seams, and poles**

**Requirements:** R3
**Dependencies:** U2
**Artifacts:** Projection specification and seam/pole result table.

**Approach:** Define face order and orientation for conversion into `TectonicTerrain { faces: [Vec<f32>; 6], resolution }`. Validate finite values, units, normalization, sea level, shared-edge deltas, corner agreement, and pole behavior before any consumer comparison.

**Evaluation scenarios:** All 12 shared edges; all 8 corners; both polar regions; deliberately invalid/non-finite sample rejection; planar baseline comparison.

**Verification outcome:** Pass only if continuity and pole quality meet the fixed protocol below. Otherwise stop productization.

- [ ] **U4: Run a preview-only imported-artifact comparison**

**Requirements:** R4
**Dependencies:** U3
**Artifacts:** Side-by-side preview capture and comparison notes.

**Approach:** Use the existing cubemap upload path in `src/preview.rs` or an equivalent evaluation-only adapter. Keep procedural terrain untouched and make the active source visible in the comparison record.

**Evaluation scenarios:** Earth-like procedural baseline; imported artifact at representative preview resolutions; edge/corner/pole inspection; repeated display from the cached artifact.

**Verification outcome:** Imported preview is visibly continuous and does not require inference on each preview refresh. This is not preview/export parity proof.

- [ ] **U5: Measure resource envelope and produce go/no-go**

**Requirements:** R5, R6
**Dependencies:** U4
**Artifacts:** `docs/research/terrain-diffusion-evaluation-decision.md`.

**Approach:** Record generation, conversion, upload, cache storage, RAM, and VRAM measurements at preview resolutions. State extrapolation uncertainty for 2K/4K/8K, including the approximately 1.5 GiB six-face 8K f32-height baseline. Score the gates below.

| Gate | Go condition | No-go/defer condition |
|---|---|---|
| Rights | Intended use and artifact handling are cleared | Upstream terms remain incompatible or unclear |
| Reproducibility | Fixed byte-identical repeat gate passes | Environment or output cannot be reproduced |
| Spherical fit | Fixed height, normal, and pole gates pass | Any continuity or pole threshold fails |
| Preview value | Blind review meets the fixed visual-comparison gate | Any visual threshold fails or reviewers are unavailable |
| Resource envelope | Reproduction meets the fixed runtime/resource gate | Any runtime/resource threshold fails or capture is incomplete |

**Verification outcome:** The decision artifact selects `go`, `no-go`, or `defer` and names the evidence. Only `go` authorizes a separate productization plan.

## Stop Conditions

- Any fixed artifact-validity, reproducibility, height/normal continuity, pole, blind-review, or runtime/resource gate fails or is not measured.
- Rights or provenance remain unclear for the intended use.
- The evaluation requires runtime inference, cloud inference, redistributed weights, or an unvalidated silent fallback.
- Imported artifacts do not demonstrate the fixed local-detail benefit over procedural terrain.

## Productization Boundary

No production code follows from this plan. A passing U5 may justify a separate plan for a narrow `Procedural` versus `Imported artifact` source identity before the existing canonical terrain boundary. That future plan must define validation, caching, provenance, explicit preview/export parity, and an explicit procedural fallback. A hybrid learned residual remains a later experiment after import and spherical validation pass.

## Sources

- Requirements: [terrain-diffusion-integration-requirements.md](../brainstorms/terrain-diffusion-integration-requirements.md)
- Research: [terrain-diffusion-integration.md](../research/terrain-diffusion-integration.md)
- Canonical terrain boundary: `src/terrain_compute.rs`
