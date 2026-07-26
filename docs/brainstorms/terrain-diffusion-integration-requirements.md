---
date: 2026-07-12
topic: terrain-diffusion-integration
---

# Terrain Diffusion Artifact-Harness Requirements

## Problem Frame

Planet Gen's procedural terrain is spherical, deterministic, and the only product source. Terrain Diffusion remains a planar upstream workflow with no verified cubemap, pole, or native Rust-runtime path. The immediate task is one bounded local harness that rejects invalid canonical cubemap artifacts and compares them to a fixed procedural control without modifying the app.

Research basis: [Terrain Diffusion Integration Research](../research/terrain-diffusion-integration.md).

## Evidence Boundary

- Harness verification validates canonical artifact bytes, frozen CPU gates, fixed controls, and isolated preview capture.
- Terrain Diffusion evaluation is **NOT RUN** until a separate manifest freezes external output/projection/provenance and records resource and human-review evidence.
- Missing measurement is failure / `NOT RUN`, never inferred success.

## Requirements

- R1. Create only `src/bin/terrain_diffusion_eval.rs`; no app, export, weather, shader, or Cargo change.
- R2. Implement the full raw contract in research: six explicit canonical filenames/order, little-endian f32, row-major `index=y*N+x`, top-left first row/column convention, texel centers, 12 edges, 8 corners, and no implicit orientation repair.
- R3. Validate candidate and `earthlike-v1` procedural control artifacts using frozen finite, size, asymmetric orientation-fixture, edge, corner, normal, pole, exact byte-equality, and FNV-1a-64 identity gates.
- R4. Use the one frozen `earthlike-v1` control schema, including `PlanetParams`, derived values, plate source/parameters, every `TerrainComputePipeline::generate` argument, disabled erosion policy, and evaluator-only centered serialization: generate `2*N+1` source samples then extract odd `(2*x+1,2*y+1)` values into the canonical N×N artifact.
- R5. Run control generation, candidate validation, control validation, and each preview capture in separate fresh processes.
- R6. Use only current public preview APIs to upload and render an isolated fixed 512² PNG with frozen uniforms and disabled dependent effects.
- R7. Implement the documented deterministic line protocol, CLI grammar, exit policy, and `PASS|FAIL|NOT_RUN` results.

## Non-Goals

- No upstream execution, model/checkpoint download, candidate artifact, manifest, projection, PyTorch/CUDA/ONNX/cloud/native inference, or external SHA-256 computation.
- No app source selector, product/runtime integration, preview/export parity claim, export integration, shipping, redistribution, or rights conclusion.
- No human visual review in this phase; it remains `NOT RUN`.

## Acceptance Examples

- AE-1: `fixture-orientation` rejects face swaps, row reversal, column reversal, and transpose before upload.
- AE-2: A valid control reports all implemented local gates `PASS`, an explicit FNV-1a-64 identity, and external/model/reviewer/resource fields as `NOT_RUN`.
- AE-3: An invalid size, NaN/Inf, edge/corner, normal, pole, or exact-repeat result exits with its named `FAIL` gate.
- AE-4: Candidate/control PNGs are 512² isolated captures at the fixed artifact path and do not involve app or export state.

## Success Criteria

The team has a minimal deterministic gatekeeper for future canonical cubemap artifacts, not a claim that Terrain Diffusion has been evaluated. Any future productization needs a separately reviewed evidence record.
