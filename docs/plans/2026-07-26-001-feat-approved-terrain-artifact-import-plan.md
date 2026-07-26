---
title: "feat: Approved terrain artifact import"
type: feat
status: active
date: 2026-07-26
origin: docs/brainstorms/offline-terrain-artifact-import-requirements.md
---

# Approved Terrain Artifact Import

## Summary

Implement dormant product-side import of a Phase 5.24-approved canonical cubemap terrain. This is a fail-closed local loader, strict approval gate, application lifecycle seam, and export refusal. It does not perform inference, conversion, scientific evaluation, or candidate activation.

Phase 5.24 evidence is a hard dependency. This phase does not change its evaluator: canonical face names, loading, finite/size checks, resolution inference, and FNV identity may be reused by the product loader, but its scientific gates remain binary-local and evaluator output stays byte-for-byte stable.

Required U1–U4 code-complete status wording (not a current completion marker): `Import infrastructure implemented but dormant; approved candidate NOT AVAILABLE; product activation and readiness NOT RUN.`

## Frozen Contract

The requirements document is normative for the exact CLI, six-file canonical raw-face contract, 16 KiB approval record schema, status/gate values, FNV fields, validator-commit rejection, evidence binding, units, sea level, and export policy. Do not add flags, config, format compatibility, transformation, fallback, or dependencies.

The 16 KiB UTF-8 LF-only approval file is fixed-order `key=value` with no blank, comment, duplicate, unknown, escaped, quoted, or optional field:

```text
approval_version=1
artifact_schema_version=1
artifact_path=<normalized-relative-directory>
resolution=512|1024|2048|4096|8192
face.posx.fnv1a64=<16-lowercase-hex>
face.negx.fnv1a64=<16-lowercase-hex>
face.posy.fnv1a64=<16-lowercase-hex>
face.negy.fnv1a64=<16-lowercase-hex>
face.posz.fnv1a64=<16-lowercase-hex>
face.negz.fnv1a64=<16-lowercase-hex>
candidate_fnv1a64=<16-lowercase-hex>
control_fnv1a64=<16-lowercase-hex>
validator_commit=<40-lowercase-hex-git-commit>
evidence.path=<normalized-relative-file>
evidence.fnv1a64=<16-lowercase-hex>
gate.artifact=PASS
gate.control=PASS
gate.byte_equality=PASS
gate.orientation=PASS
gate.seams=PASS
gate.normals=PASS
gate.poles=PASS
gate.provenance=PASS
gate.rights=PASS
gate.resource_capture=PASS
gate.human_review=PASS
units.height=normalized_control_range
units.horizontal=unit_sphere
sea_level=control_ocean_level
control_ocean_level=<canonical-finite-f32-in--0.5..=1.2>
export_policy=REFUSE_IMPORTED
status=APPROVED
```

Only `status=APPROVED` is accepted. Status values are `APPROVED|REJECTED|NOT_RUN`; gate values are `PASS|FAIL|NOT_RUN`, and every listed gate must be `PASS`. Freeze `KNOWN_NO_GO_CONTROL_FNV=243e1887675e77a8` and `KNOWN_NO_GO_VALIDATOR_COMMIT=0cba9579e68f0fc72a75d21db2d655496ef76d09`. `control_fnv1a64` and `validator_commit` MUST NOT equal those NO-GO identities: they name a separately corrected approved control and validator. Computed FNV-1a-64 over the six canonical faces in fixed filename order MUST equal `candidate_fnv1a64`, MUST differ from `KNOWN_NO_GO_CONTROL_FNV`, and MUST differ from `control_fnv1a64`. Evidence path/hash and all face hashes bind to the loaded canonical files; a mismatch rejects import.

`main` parses `args_os` before logger/GPU/eframe. The only modes are no-argument procedural, paired `--terrain-artifact DIR --terrain-approval FILE`, and `--help`. Every explicit import error exits 2 before application startup; it never selects procedural terrain as a fallback.

Resolve every input relative to canonical CWD. The artifact must resolve to a non-symlink directory; approval and evidence must resolve to non-symlink regular files. Reject absolute paths, `..`, symlink components/leafs, extra artifact entries, and `.incomplete`. After entry validation, open each canonical face, capture metadata, read exactly the bounded expected bytes while hashing, then verify length and modified metadata after read. This is a bounded local-offline read, not a guarantee against hostile concurrent local mutation; that adversary is outside the threat model.

The evidence file is UTF-8, at most 1 MiB, LF-line-ended, and has deterministic section headers followed by verbatim evaluator stdout transcripts for fresh-process `capture-control` A, fresh-process `capture-control` B, third-process `compare-bytes`, `validate-control`, and `validate-candidate`, plus external, resource, and human decision sections. The loader treats it as opaque bytes and checks only canonical relative path plus FNV. It cannot independently derive administrative approval statuses; the approval record is authoritative. Do not change the evaluator protocol byte-for-byte.

## Exact Code Scope

| File | Change |
|---|---|
| `src/terrain_artifact.rs` | New canonical loader, strict approval parser, source/resolution/identity types, and CPU-only tests. |
| `src/lib.rs` | Export `terrain_artifact`. |
| `src/main.rs` | Parse frozen CLI before logger/GPU/eframe and pass a validated imported source to the app. |
| `src/app.rs` | Store one imported `Arc<TectonicTerrain>`, route it to preview/wind/weather/clouds, and refuse imported export in `start_export`. |

No other production source changes are allowed. In particular, do not touch `src/export.rs`, Cargo files, shaders, World Orogen, or the Phase 5.24 evaluator binary.

## `src/terrain_artifact.rs` API

Use only the types needed by `main` and `app`:

```rust
pub enum TerrainResolution { R512, R1024, R2048, R4096, R8192 }
pub enum TerrainSource {
    Procedural,
    Imported(std::sync::Arc<crate::terrain_compute::TectonicTerrain>),
}
pub struct TerrainArtifactApproval { /* parsed frozen approval fields */ }
pub struct ApprovedTerrainArtifact {
    pub terrain: std::sync::Arc<crate::terrain_compute::TectonicTerrain>,
    pub approval: TerrainArtifactApproval,
}
pub fn load_approved_terrain(
    artifact_dir: &std::path::Path,
    approval_file: &std::path::Path,
) -> Result<ApprovedTerrainArtifact, TerrainArtifactError>;
```

`TerrainArtifactError` has named variants/messages for usage, path, schema, approval, size, resolution, non-finite, identity, and evidence rejection. It is not a generic abstraction. Keep path validation, exact-six-name enumeration, regular-file checks, file-size checks, resolution inference, finite decoding, and FNV-1a-64 here. Keep seam/normal/pole/control/provenance science in `terrain_diffusion_eval`; do not call or refactor it.

## Application Source Lifecycle

`PlanetGenApp::new(cc: &eframe::CreationContext<'_>, source: TerrainSource)` stores the source, with `main` passing it exactly as `Box::new(move |cc| Ok(Box::new(PlanetGenApp::new(cc, source)?)))` to `eframe::run_native`. Keep owned procedural `weather_terrain: Option<TectonicTerrain>` and erosion state. Imported source retains one `Arc<TectonicTerrain>`; preview/wind/weather/clouds borrow it through deref, without changing their signatures. `dispatch_weather` explicitly selects the imported Arc borrow or the owned procedural terrain. `erosion_pipeline: Option<ErosionPipeline>` is `Some` only for Procedural and `None` for Imported; guard every use. Imported mode never constructs/calls erosion, clones `faces`, rereads the artifact, or creates a second terrain identity. `regenerate_terrain` may be called, but its imported match only reinstalls/reuses the Arc and recomputes downstream state; it never enters plate/procedural generation.

Use the existing egui `add_enabled_ui(false, ...)` (or equivalent existing pattern) to group-disable all provenance-defining imported-inapplicable controls: Planet Parameters `Distance (AU)`, `Mass (M⊕)`, `[Fe/H]`, `Tilt (°)`, `Day (hours)`, random seed, `Seed:`; Visual Overrides `Continent Scale`, `Continents`, `Size Variety`, `Water Loss`, `Erosion`; Advanced Tweaks `Plates`, `Mountain Height`, `Range Width`, `Shape Warp`, `Detail`, Surface Age override. Only `Season` plus weather, cloud, render, and camera controls remain active. Season reuses the Arc and recomputes downstream state only; weather controls regenerate weather; render/camera controls rerender.

Add pure `export_refusal(&TerrainSource) -> Option<&'static str>` and call it first in `start_export`. Imported mode uses its clear status `Export is unavailable for imported terrain artifacts.` and returns before `ExportConfig`, output-directory lookup, export handle, or thread creation; tests call the helper without a GPU window. Procedural export remains unchanged.

## Implementation Units

- [ ] **U1: Loader and approval gate**

  **Requirements:** exact six names, path/resource boundary, resolution cap, finite/size/FNV checks, strict 16 KiB schema.  
  **Files:** `src/terrain_artifact.rs`, `src/lib.rs`  
  **Approach:** Parse all required fields in fixed order. Resolve artifact to a non-symlink directory and approval/evidence to non-symlink regular files under canonical CWD. Validate entries, open each face, capture metadata, read exactly bounded bytes while hashing, then verify length/modified metadata. Reject symlinks, non-regular files, extra entries, `.incomplete`, malformed UTF-8/schema, NO-GO validator/control identities, candidate/control identity equality, evidence mismatch, and all non-`APPROVED` records. Build one `Arc<TectonicTerrain>` only after all checks pass.

  ```bash
  rtk cargo test --lib terrain_artifact
  ```

  Expected: synthetic valid faces/approval load; every malformed approval, sixth-file violation, size/resolution/non-finite/FNV mismatch, unsafe path, evidence mismatch, NO-GO control/validator identity, candidate/control equality, and metadata-change fixture rejects on CPU.

- [ ] **U2: Fail-closed CLI**

  **Requirements:** frozen `args_os` grammar and pre-start exit 2.  
  **Files:** `src/main.rs`  
  **Dependencies:** U1  
  **Approach:** Use a small local parser for exactly the three documented forms. Test parser behavior through a pure helper accepting `IntoIterator<Item = OsString>` so invalid paths/arguments do not open a GPU window. Help exits 0; no arguments produce procedural selection; duplicates, unknowns, positional values, omitted values, and partial pairs are usage errors; failed import returns 2 before logger/GPU/eframe.

  ```bash
  rtk cargo test --bin planet-gen cli
  ```

  Expected: all invalid forms are deterministic exit-2 candidates; import has no procedural fallback.

- [ ] **U3: Imported application lifecycle**

  **Requirements:** one shared terrain identity for preview/wind/weather/clouds; no imported erosion/reload/vector clone.  
  **Files:** `src/app.rs`  
  **Dependencies:** U1, U2  
  **Approach:** Change construction to `PlanetGenApp::new(cc: &eframe::CreationContext<'_>, source: TerrainSource)` and use the frozen `run_native` closure handoff. Preserve owned procedural `weather_terrain` and use the imported Arc by deref in preview/wind/weather/cloud paths without signature changes. Make erosion optional and guarded; imported regeneration only reuses the Arc and recomputes downstream state. Group-disable every listed provenance-defining control; only season plus weather/cloud/render/camera controls remain active. Keep source selection observable through CPU-only helpers; do not create a GPU window or model fixture.

  ```bash
  rtk cargo test --lib app
  ```

  Expected: unit-level source selection preserves the same `Arc` identity and procedural selection stays procedural.

- [ ] **U4: Imported export refusal and regressions**

  **Requirements:** refuse before any export resource, preserve procedural export, keep `src/export.rs` untouched.  
  **Files:** `src/app.rs`  
  **Dependencies:** U3  
  **Approach:** Call pure `export_refusal(&TerrainSource) -> Option<&'static str>` as the first operation in `start_export`. Test that Imported returns the refusal before config/directory/handle/thread work and Procedural returns `None`; do not alter export implementation.

  ```bash
  rtk cargo test --lib app
  rtk git diff -- src/export.rs
  ```

  Expected: refusal message is clear; the second command has no output.

- [ ] **U5: Real candidate activation evidence** — **BLOCKED / NOT RUN**

  **Depends on:** complete Phase 5.24 local and external evidence, a real approved candidate, an approval record conforming to the frozen schema, independent review, and a product-readiness decision.  
  **Approach:** Do not create, commit, synthesize, or infer any candidate/approval record. When evidence exists, validate it with the unchanged Phase 5.24 evaluator, independently review the binding and status fields, then explicitly update this unit. Code completion for U1–U4 does not complete this phase while U5 is blocked.

## Code-Complete vs Activation

U1–U4 may be marked complete after their CPU-only tests and standard checks pass. That means only dormant infrastructure is complete. The plan remains `active` and U5 remains `BLOCKED / NOT RUN` until real Phase 5.24 evidence and a reviewed approval are available. Do not add a completion marker or candidate hash before then.

## Verification

```bash
rtk cargo fmt --check
rtk cargo test --lib terrain_artifact
rtk cargo test --bin planet-gen cli
rtk cargo test --lib app
rtk cargo test --lib
rtk cargo clippy --all-targets -- -D warnings
rtk cargo build
rtk git diff --check
rtk git diff -- src/export.rs
```

All import tests must use synthetic CPU-only raw faces, approvals, and evidence files. They must not invoke a GPU window, model, checkpoint, converter, projection, real candidate, or real approval record.

## Sources

- Requirements: [offline-terrain-artifact-import-requirements.md](../brainstorms/offline-terrain-artifact-import-requirements.md)
- Phase 5.24 research/contract: [terrain-diffusion-integration.md](../research/terrain-diffusion-integration.md)
- Phase 5.24 plan: [2026-07-12-001-feat-terrain-diffusion-evaluation-plan.md](2026-07-12-001-feat-terrain-diffusion-evaluation-plan.md)
