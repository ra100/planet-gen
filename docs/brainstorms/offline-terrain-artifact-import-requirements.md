# Approved Offline Terrain Artifact Import Requirements

**Date:** 2026-07-26  
**Status:** Approved for dormant infrastructure only  
**Depends on:** Phase 5.24 canonical-artifact evidence  
**Architecture:** GEN-005

## Problem Frame

Planet Gen may accept a previously evaluated canonical terrain artifact only when a strict, local approval record binds that artifact to the Phase 5.24 evidence. The import path is not an inference, projection, conversion, or evaluation path. It must fail closed: an incomplete, unapproved, altered, or unsupported artifact cannot start the application in imported mode.

No candidate artifact or approval record exists in this repository. Therefore the product remains procedural by default and real activation remains blocked.

## Scope

- Add a local canonical-artifact loader and approval parser.
- Add an explicit, all-or-nothing application CLI import mode.
- Feed one imported terrain identity through the existing preview, wind, weather, and cloud lifecycle.
- Refuse export from imported mode before any export resource is acquired.
- Cover the loader, approval, CLI parsing, and app seams with CPU-only synthetic tests.

## Non-goals

- Model/checkpoint acquisition, inference, planar projection, conversion, or upstream validation.
- A candidate artifact, approval file, evidence capture, or readiness claim.
- Changing `src/export.rs`, Cargo dependencies, shaders, World Orogen, or normal no-argument procedural behavior.

## Frozen CLI

The complete grammar is:

```text
planet-gen
planet-gen --terrain-artifact DIR --terrain-approval FILE
planet-gen --help
```

`std::env::args_os()` is required so paths remain `OsString` values until filesystem use. `--help` writes usage and exits 0 without logger, GPU, or eframe initialization. No arguments select unchanged procedural mode. Import mode requires exactly one `--terrain-artifact DIR` and one `--terrain-approval FILE`, in either order.

Duplicate flags, unknown flags, a missing value, a partial pair, a positional argument, or an explicit import validation failure write a diagnostic and exit 2 before `env_logger::init`, GPU construction, or `eframe::run_native`. Imported mode never falls back to procedural terrain.

## Canonical Artifact Contract

The artifact directory contains exactly these six regular files and no `.incomplete` marker:

```text
posx.f32le
negx.f32le
posy.f32le
negy.f32le
posz.f32le
negz.f32le
```

They are the Phase 5.24 raw-face contract: `+X,-X,+Y,-Y,+Z,-Z`, little-endian IEEE-754 binary32, row-major, with exactly `4*N*N` bytes per face. Values must be finite. All six files infer the same supported square resolution; no resampling, orientation correction, normalization, conversion, or erosion is permitted.

Supported resolution is a closed enum: `512`, `1024`, `2048`, `4096`, or `8192`. The loader rejects every other resolution and enforces `8192` as the maximum before allocation. It accepts only the exact six expected names, rejects extra directory entries, symlinks, non-regular files, and `.incomplete`.

## Path and Resource Boundary

The artifact directory and approval file are local offline inputs, not a privileged-security boundary. Resolve relative paths under canonical current working directory and reject absolute paths, `..` components, symlink components, symlink leafs, and paths resolving outside it. The artifact path must resolve to a non-symlink directory; approval and evidence paths must resolve to non-symlink regular files. The approval binds the canonical normalized relative artifact path, not a basename. The threat model excludes hostile concurrent local filesystem mutation.

After validating the entries, the loader opens each expected regular face file, captures metadata, reads exactly its bounded expected byte count while computing FNV-1a-64, then verifies length and modified metadata again. It applies the resolution cap before allocation and rejects a changed size or modified metadata. FNV-1a-64 is only a fixed-width lowercase 16-hex local identity; it is not a cryptographic integrity claim.

## Approval Record

The approval file is strict UTF-8 `key=value`, LF-delimited, at most 16 KiB including newlines. It has no blank lines, comments, duplicate keys, unknown keys, whitespace around keys/values, escaping, quoting, continuation, or optional fields. Required keys appear exactly once and in this exact order:

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

Status values are exactly `APPROVED`, `REJECTED`, or `NOT_RUN`; only `APPROVED` is accepted. Every gate value is exactly `PASS`, `FAIL`, or `NOT_RUN`; all listed gates must be `PASS`. The immutable constants are `KNOWN_NO_GO_CONTROL_FNV=243e1887675e77a8` and `KNOWN_NO_GO_VALIDATOR_COMMIT=0cba9579e68f0fc72a75d21db2d655496ef76d09`. `control_fnv1a64` and `validator_commit` MUST NOT equal those NO-GO identities; they must identify a separately corrected approved control and validator. The computed FNV-1a-64 over the six canonical face files in fixed filename order MUST equal `candidate_fnv1a64`, MUST differ from `KNOWN_NO_GO_CONTROL_FNV`, and MUST differ from `control_fnv1a64`. `evidence.path` and `evidence.fnv1a64` must bind a non-symlink regular evidence file under canonical CWD. The declared face FNVs must match the six loaded files.

The canonical units are normalized height against the approved control range and unit-sphere horizontal coordinates. `control_ocean_level` is the exact canonical `f32::to_string()` value in `-0.5..=1.2`; imported preview, wind, and weather use it directly. An imported artifact must not reinterpret either. `export_policy=REFUSE_IMPORTED` is mandatory and cannot be overridden by UI or CLI.

## Lifecycle and Export Safety

`PlanetGenApp::new(cc: &eframe::CreationContext<'_>, source: TerrainSource)` stores the source. `main` hands it off exactly as `Box::new(move |cc| Ok(Box::new(PlanetGenApp::new(cc, source)?)))` in `eframe::run_native`. Procedural mode retains the existing owned `weather_terrain: Option<TectonicTerrain>` and erosion state. Imported `TerrainSource` retains exactly one `Arc<TectonicTerrain>`; preview, wind, weather, and clouds borrow it through deref without signature changes. `dispatch_weather` explicitly selects the imported Arc borrow or owned procedural terrain.

`erosion_pipeline: Option<ErosionPipeline>` is `Some` only for `TerrainSource::Procedural` and `None` for `TerrainSource::Imported`; every use is guarded. Imported mode never constructs or calls erosion, clones face vectors, or reloads artifact files. `regenerate_terrain` may run, but its imported match only reinstalls/reuses that Arc and never enters plate or procedural generation.

The existing egui `add_enabled_ui(false, ...)` (or equivalent existing pattern) groups and disables all provenance-defining inputs in imported mode: Planet Parameters `Distance (AU)`, `Mass (M⊕)`, `[Fe/H]`, `Tilt (°)`, `Day (hours)`, random seed, and `Seed:`; Visual Overrides `Continent Scale`, `Continents`, `Size Variety`, `Water Loss`, and `Erosion`; Advanced Tweaks `Plates`, `Mountain Height`, `Range Width`, `Shape Warp`, `Detail`, and Surface Age override. Only `Season` plus weather, cloud, render, and camera controls remain active. Season reuses the same Arc and recomputes downstream state only; weather controls regenerate weather; render/camera controls rerender.

No-argument execution remains the current procedural lifecycle. A pure CPU-testable `export_refusal(&TerrainSource) -> Option<&'static str>` helper is called first by `PlanetGenApp::start_export`; imported mode sets that clear status and returns before creating an export config, output directory, handle, or worker thread. Procedural export remains unchanged. `src/export.rs` is untouched.

## Evidence File

The evidence file is UTF-8, at most 1 MiB, and LF-line-ended. It contains deterministic section headers followed by verbatim Phase 5.24 evaluator stdout transcripts for fresh-process `capture-control` A, fresh-process `capture-control` B, third-process `compare-bytes`, `validate-control`, and `validate-candidate`, then external, resource, and human decision sections. The loader treats it as opaque bytes and checks only its canonical relative path and FNV-1a-64. It cannot independently derive administrative approval statuses from the evidence; those remain the approval authority. The evaluator protocol remains byte-for-byte unchanged.

## Acceptance Criteria

### Code complete

- The strict parser and loader reject malformed CLI, path, schema, approval, identity, size, finite-value, resolution, and six-file inputs before UI/GPU startup.
- Procedural no-argument startup and procedural export preserve existing behavior.
- Imported startup shares one `Arc<TectonicTerrain>` across preview, wind, weather, and clouds, and refuses export.
- CPU-only synthetic tests cover accepted and rejected boundaries without a GPU window, model, or candidate artifact.

### Product activation evidence

Code completion is not activation. Activation remains blocked until Phase 5.24 produces approved, reproducible evidence and a separately reviewed real candidate plus approval record binds all schema fields. The required current status is:

`Import infrastructure implemented but dormant; approved candidate NOT AVAILABLE; product activation and readiness NOT RUN.`
