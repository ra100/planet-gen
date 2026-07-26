---
date: 2026-07-12
topic: terrain-diffusion-integration
---

# Terrain Diffusion Integration Evaluation

## Problem Frame

Planet Gen's procedural terrain is spherical, deterministic, fast, and its default source. Terrain Diffusion may offer useful offline local-terrain detail, but it produces planar heightfields and has no verified cubemap, pole, or native Rust runtime path. The immediate need is evidence for or against a safe offline artifact workflow, not product integration.

Research basis: [Terrain Diffusion Integration Research](../research/terrain-diffusion-integration.md).

## Product Posture

Optional offline authoring and imported artifacts may augment the procedural pipeline. They must not replace it, introduce a required runtime dependency, or make preview/export behavior diverge.

## Research-Spike Requirements

- R1. Establish the evaluation's legal, reproducibility, and asset-handling boundary without shipping weights or generated assets.
- R2. Reproduce an upstream-supported local evaluation from a pinned source revision and record the exact environment, model identity, inputs, outputs, and observed resource use.
- R3. Evaluate a documented planar-to-cubemap candidate projection at edges, corners, and poles; reject non-finite heights and record normalization and sea-level semantics.
- R4. Compare a preview-only imported scalar height cubemap with the existing procedural preview using the canonical `TectonicTerrain` shape or existing cubemap upload path.
- R5. Measure generation, conversion, upload, and artifact storage/memory costs at representative preview resolutions; document extrapolation limits for 2K/4K/8K export.
- R6. Produce a go/no-go decision that states whether a separate productization plan is justified.

## Deferred Product Requirements

- R7. A future product path, if approved, selects `Procedural` or `Imported artifact` before existing terrain consumers.
- R8. Imported artifacts define source identity, units, normalization, sea level, non-finite rejection, face order/orientation, edge/corner continuity, and deterministic provenance.
- R9. Preview and export use the same cached artifact and prove parity; export never silently falls back to procedural terrain.
- R10. Procedural terrain remains available as an explicit fallback.
- R11. A learned residual experiment is considered only after imported-artifact validation passes.

## Non-Goals

- No embedded PyTorch, CUDA, ONNX, or native Rust inference runtime.
- No cloud inference, redistributed model weights, or commercial asset release.
- No change to procedural generation, climate, erosion, wind, rendering, or export behavior.
- No generic plugin/provider system.
- No claim that reproduction, benchmark execution, or spherical suitability has already succeeded.

## Acceptance Examples

- AE-1: A result manifest identifies the upstream revision, model hash, seed, sampler, conditioning, projection, and output content hash.
- AE-2: A cubemap candidate has documented face order/orientation and recorded edge, corner, and polar continuity results.
- AE-3: The preview comparison uses an imported scalar height artifact without changing the procedural default path.
- AE-4: The decision artifact states pass/fail results for rights, reproducibility, seam/pole quality, preview quality, and resource limits.

## Scope Boundaries

The spike may create evaluation records and preview-only artifacts outside the runtime product path. It must keep artifacts local or otherwise handled according to confirmed rights. Product code is deferred to a separate approved plan.

## Key Decisions

- Procedural-first is the default and remains dependency-free.
- Offline validated import is the first and smallest possible learned-terrain experiment.
- AI-only runtime and cloud routes are rejected for this phase.
- Silent fallback is unacceptable for export; source identity must be explicit.

## Assumptions and Blockers

- Assumption: upstream evaluation can be run locally from a pinned revision on suitable hardware.
- Blocker: no verified spherical/cubemap/polar validation exists upstream.
- Blocker: commercial and redistribution implications of WorldClim/MERIT-derived assets require clarification before shipping.
- Blocker: no verified complete ONNX or native Rust runtime path exists.

## Success Criteria

- The team has a reproducible evidence record, seam/pole findings, preview comparison, and resource measurements.
- The final gate either supports a narrowly scoped imported-artifact product plan or rejects/defer the route with recorded reasons.
