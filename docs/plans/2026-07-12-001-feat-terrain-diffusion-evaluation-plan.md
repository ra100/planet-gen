---
title: "feat: Terrain Diffusion artifact-harness evaluation"
type: feat
status: dropped
date: 2026-07-12
origin: docs/brainstorms/terrain-diffusion-integration-requirements.md
---

# Terrain Diffusion Artifact-Harness Evaluation

> **ARCHIVED / NOT FEASIBLE FOR WHOLE-PLANET PRODUCT USE.** This retained evaluation plan records the rejected artifact-harness direction. No implementation unit below is active or planned; the procedural control remains `NO-GO`, while external model, projection, resource, review, and product evidence remain `NOT_RUN`. See [the archived roadmap entry](../../Plans.md#archived-terrain-diffusion-and-imported-terrain) and [the procedural replacement](2026-07-28-003-feat-procedural-terrain-realism-plan.md).

## Summary

Implement one evaluation-only binary, `src/bin/terrain_diffusion_eval.rs`. It validates already-canonical cubemap artifacts against fixed procedural control `earthlike-v1` and makes isolated preview captures. It does not run Terrain Diffusion, convert planar output, add product integration, or change `app.rs`, `export.rs`, `weather.rs`, shaders, or `Cargo.toml`.

The complete raw contract, cube adjacency, orientation fixture, CPU algorithms, control preset, hashes, and frozen preview path are normative in [terrain-diffusion-integration.md](../research/terrain-diffusion-integration.md). This plan does not duplicate or weaken them.

## Evidence Rule

Every gate emits `PASS`, `FAIL`, or `NOT_RUN`. Missing evidence is `FAIL` / `NOT_RUN`; it is never success by inference. Local harness `PASS` is not Terrain Diffusion evaluation. Upstream model/projection/provenance/resource/reviewer evidence remains `NOT_RUN` until a later manifest records it.

## Fixed CLI and Result Protocol

All artifact commands use `--dir DIR`; no command accepts face paths. Each artifact directory contains exactly the fixed canonical filenames `posx.f32le negx.f32le posy.f32le negy.f32le posz.f32le negz.f32le` in that order:

```text
terrain_diffusion_eval fixture-orientation
terrain_diffusion_eval capture-control --resolution N --dir DIR
terrain_diffusion_eval validate-control --resolution N --dir DIR
terrain_diffusion_eval validate-candidate --resolution N --dir DIR --control DIR
terrain_diffusion_eval compare-bytes --first DIR --second DIR
terrain_diffusion_eval preview-control --resolution N --dir DIR
terrain_diffusion_eval preview-candidate --resolution N --dir DIR --control DIR
```

`N` must be an integer `>=4`. `capture-control` atomically reserves `DIR` itself with directory creation, immediately places `.incomplete` inside it, and removes that marker only after all six files validate. On ordinary errors the reserved directory is removed; after a hard crash any remaining `.incomplete` directory is invalid and collides until explicitly deleted. This is final-directory reservation, not an atomic directory swap. Every other command rejects `.incomplete` artifact directories. `capture-control` fails if `DIR` already exists and never overwrites or merges a destination. Every other command requires an existing complete artifact directory. `compare-bytes` is the sole two-directory command and never generates files. No optional source/projection/config/transform flags exist. Absolute and `..` paths and pre-existing symlink escapes are rejected during preflight; component-wise checks prevent ordinary configuration escapes. The evaluator assumes its working tree is not concurrently mutated by an untrusted local process: it is a local offline research tool, not a privileged security boundary.

`--dir`, `--first`, `--second`, and every future `--out` value must be a relative path under the current working directory. Reject an absolute path or any component equal to `..` with usage exit 2. Normalize accepted `path.*` values with `/`; they are therefore always current-working-directory-relative.

Stdout is deterministic UTF-8, one `key=value` per line, and emits only the following keys in this exact order. Fixed decimals use Rust `{:0.9}`; FNV is exactly 16 lowercase hex digits. Stderr contains diagnostics only.

`capture-control`:

```text
result_version=1
command=capture-control
artifact=control
resolution=<N>
gate.capture=PASS|FAIL
gate.finite=PASS|FAIL
metric.fnv1a64=<16-lowercase-hex>
path.dir=<relative-dir>
path.posx=<relative-dir>/posx.f32le
path.negx=<relative-dir>/negx.f32le
path.posy=<relative-dir>/posy.f32le
path.negy=<relative-dir>/negy.f32le
path.posz=<relative-dir>/posz.f32le
path.negz=<relative-dir>/negz.f32le
```

`compare-bytes`:

```text
result_version=1
command=compare-bytes
artifact=control-pair
resolution=<N>
gate.byte_equality=PASS|FAIL
metric.first_fnv1a64=<16-lowercase-hex>
metric.second_fnv1a64=<16-lowercase-hex>
path.first=<relative-dir>
path.second=<relative-dir>
```

`validate-control` emits the validation keys below in this exact order; `path.dir` and `path.control` are the same value:

```text
result_version=1
command=validate-control
artifact=control
resolution=<N>
gate.size=PASS|FAIL
gate.finite=PASS|FAIL
gate.orientation_fixture=PASS|FAIL
gate.control_range=PASS|FAIL
gate.height_edge=PASS|FAIL
gate.height_corner=PASS|FAIL
gate.normal=PASS|FAIL
gate.pole_elevation=PASS|FAIL
gate.pole_slope=PASS|FAIL
gate.external_model=NOT_RUN
gate.projection=NOT_RUN
gate.resource_capture=NOT_RUN
gate.human_review=NOT_RUN
gate.product_integration=NOT_RUN
metric.fnv1a64=<16-lowercase-hex>
metric.height_edge_p95=<fixed-decimal>
metric.height_corner_p95=<fixed-decimal>
metric.normal_p95_deg=<fixed-decimal>
metric.normal_max_deg=<fixed-decimal>
metric.pole_elevation_north_p50=<fixed-decimal>
metric.pole_elevation_south_p50=<fixed-decimal>
metric.pole_slope_north_p50=<fixed-decimal>
metric.pole_slope_south_p50=<fixed-decimal>
path.dir=<relative-dir>
path.control=<relative-dir>
```

`validate-candidate` emits the identical key order, with only these fixed value differences: `command=validate-candidate`, `artifact=candidate`, `path.dir=<candidate-relative-dir>`, and `path.control=<control-relative-dir>`.

`preview-control` emits the render keys below in this exact order; `path.dir` and `path.control` are the same value:

```text
result_version=1
command=preview-control
artifact=control
resolution=<N>
gate.validation=PASS|FAIL
gate.render=PASS|FAIL
gate.png=PASS|FAIL
gate.external_model=NOT_RUN
gate.projection=NOT_RUN
gate.resource_capture=NOT_RUN
gate.human_review=NOT_RUN
gate.product_integration=NOT_RUN
metric.fnv1a64=<16-lowercase-hex>
metric.png_bytes=<unsigned-decimal>
path.dir=<relative-dir>
path.control=<relative-dir>
path.png=artifacts/terrain-diffusion-eval/control-512.png
```

`preview-candidate` emits the identical key order, with only these fixed value differences: `command=preview-candidate`, `artifact=candidate`, `path.dir=<candidate-relative-dir>`, `path.control=<control-relative-dir>`, and `path.png=artifacts/terrain-diffusion-eval/candidate-512.png`.

Exit `0` means every implemented required local gate passed. Exit `2` means CLI/IO/byte-layout input error, including an existing `capture-control --dir` destination. Exit `3` means a named local validation gate or `compare-bytes` exact-byte comparison failed. Exit `4` means GPU initialization/render/PNG failure. `NOT_RUN` fields alone do not change a passing local harness exit, but prohibit any productization decision.

## Frozen Gates

- Artifact bytes, canonical filenames, orientation fixture, 12 edges, 8 corners, height normalization/range guard, normal stencil, weighted polar quantiles, exact byte equality, and FNV behavior follow the research contract exactly.
- Height p95, corner p95, normal p95/max, and pole p10/p50/p90 use its frozen control-relative equations. Do not introduce tolerance/config flags.
- Exact byte equality is determinism authority. FNV-1a-64 is non-cryptographic display identity only. External source/checkpoint SHA-256 is a future manifest input and `NOT_RUN`; do not add a hash dependency.

## Historical Implementation Units (Not Planned)

- **U1: Canonical loader, fixture, and result protocol**

  **Requirements:** R1, R2, R7  
  **Files:** create only `src/bin/terrain_diffusion_eval.rs`  
  **Approach:** Parse only the frozen subcommands. Load six explicit files with the frozen row-major LE-f32 contract. Implement `fixture-orientation` and reject swaps, row flips, column flips, transposes, malformed length, or non-finite values before constructing `TectonicTerrain`. Emit the line protocol and exit codes above.

  ```bash
  rtk cargo run --bin terrain_diffusion_eval -- fixture-orientation
  ```

  Expected: exit 0; fixture reports canonical order `PASS` and every deliberate face/order/row/column/transpose mutation as named `FAIL` internally.

- **U2: Built-in procedural control and frozen CPU validation**

  **Requirements:** R3, R4  
  **Dependencies:** U1  
  **Approach:** Implement only `earthlike-v1` exactly as research specifies: `PlanetParams::default`, derived properties, `generate_plates` parameters, every terrain generate argument, and no erosion. The evaluator generates checked `2*N+1` source faces and extracts odd `(2*x+1,2*y+1)` values so serialized artifacts use canonical texel centers without changing product generation. Generate/serialize the control raw faces; validate finite/range, edge/corner, normal, pole, FNV, and exact repeat rules using the frozen algorithms. Do not make a generic config/preset system.

  ```bash
  rtk cargo run --bin terrain_diffusion_eval -- capture-control --resolution 512 --dir artifacts/terrain-diffusion-eval/control-a
  rtk cargo run --bin terrain_diffusion_eval -- validate-control --resolution 512 --dir artifacts/terrain-diffusion-eval/control-a
  ```

  Expected: both exit 0; all implemented local gates `PASS`; every external/model/projection/resource/reviewer/product field `NOT_RUN`. A pre-existing `control-a` directory exits 2 without mutation.

- **U3: Candidate comparison and fresh-process determinism**

  **Requirements:** R3, R5  
  **Dependencies:** U2  
  **Approach:** Compare candidate against the explicit control directory. Fresh-process determinism is mechanically decided by two separate `capture-control` invocations to distinct empty directories, followed by a third-process `compare-bytes`; no command generates both controls. No child-process orchestration, shared GPU context, or cache is retained. Candidate validation does not transform/correct any face.

  ```bash
  rtk cargo run --bin terrain_diffusion_eval -- validate-candidate --resolution 512 --dir artifacts/terrain-diffusion-eval/candidate --control artifacts/terrain-diffusion-eval/control-a
  rtk cargo run --bin terrain_diffusion_eval -- capture-control --resolution 512 --dir artifacts/terrain-diffusion-eval/control-b
  rtk cargo run --bin terrain_diffusion_eval -- compare-bytes --first artifacts/terrain-diffusion-eval/control-a --second artifacts/terrain-diffusion-eval/control-b
  ```

  Expected: candidate exits 0 only when every local frozen gate passes; each capture is a fresh process; comparison exits 0 only for exact byte equality and exits 3 for any mismatch. No external-evaluation field becomes `PASS`.

- **U4: Public isolated preview capture**

  **Requirements:** R5, R6  
  **Dependencies:** U3  
  **Approach:** Use existing public APIs only: `GpuContext::new`, `PreviewRenderer::new`, `upload_terrain`, and `render`. The research contract freezes size, uniforms, null cloud/weather inputs, RGBA readback, existing `image` PNG encoding, and paths. Current APIs are sufficient; no minimal scoped API exposure/change is required. A preview command receives only validated terrain and exits after one capture.

  ```bash
  rtk cargo run --bin terrain_diffusion_eval -- preview-control --resolution 512 --dir artifacts/terrain-diffusion-eval/control
  rtk cargo run --bin terrain_diffusion_eval -- preview-candidate --resolution 512 --dir artifacts/terrain-diffusion-eval/candidate --control artifacts/terrain-diffusion-eval/control
  ```

  Expected: each command runs in its own fresh process, exits 0 only after validation/upload/readback/PNG write, and creates respectively `artifacts/terrain-diffusion-eval/control-512.png` or `candidate-512.png`. It makes no app/export/parity claim.

## Stop Conditions

- Any local artifact, orientation, height, corner, normal, pole, or exact-repeat gate fails or lacks its required local metric.
- Work requires projection, upstream inference/model/checkpoint, cloud service, an app source toggle, product export integration, or an edit outside `src/bin/terrain_diffusion_eval.rs`.
- Rights, provenance, external resources, or human review are absent: emit `NOT_RUN`; do not productize.

## Productization Boundary

No product code follows from this plan. A later separately reviewed evidence record may justify a plan for source identity, cached artifact reuse, validation, explicit preview/export parity, and procedural fallback. Projection, external inference, runtime integration, and human review remain deferred.

## Sources

- Requirements: [terrain-diffusion-integration-requirements.md](../brainstorms/terrain-diffusion-integration-requirements.md)
- Research/contract: [terrain-diffusion-integration.md](../research/terrain-diffusion-integration.md)
- Terrain: `src/terrain_compute.rs`
- Cube mapping: `src/cube_sphere.rs`, `src/shaders/cube_sphere.wgsl`
- Preview: `src/preview.rs`
