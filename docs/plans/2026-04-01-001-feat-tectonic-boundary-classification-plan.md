---
title: "feat: Tectonic boundary classification with distinct terrain shapes"
type: feat
status: active
date: 2026-04-01
origin: docs/brainstorms/2026-04-01-tectonic-boundary-classification-requirements.md
---

# Tectonic Boundary Classification

## Overview

Add velocity vectors to tectonic plates, classify boundaries as convergent/divergent/transform, and generate distinct terrain shapes for each type. UI dropdown switches between Quick (current) and Classified modes.

## Problem Frame

All plate boundaries currently produce mountains regardless of geological context. Real planets have trenches at subduction zones, rift valleys at divergent boundaries, and lateral offsets at transform faults. (see origin: `docs/brainstorms/2026-04-01-tectonic-boundary-classification-requirements.md`)

## Requirements Trace

- R1. Boundary classification: convergent/divergent/transform from relative velocity
- R2. Physics-derived plate velocities from rotation period + mass
- R3. Ocean-ocean convergent → island arc + trench
- R4. Ocean-continent convergent → Andes-type mountains + trench
- R5. Continent-continent convergent → Himalayas (broad highland)
- R6. Divergent → rift valley with volcanic ridge
- R7. Transform → lateral offset, no mountains
- R8. UI dropdown: Quick / Classified
- R9. Visually distinct terrain at different boundary types

## Scope Boundaries

- NOT plate motion over time (Phase 8b)
- NOT mantle convection (Phase 8c)
- NOT changing Voronoi plate algorithm — classification is post-processing
- NOT volcanic hotspot chains

## Context & Research

### Relevant Code and Patterns

- `src/plates.rs` — `PlateData` struct (center, type, boundary distances), `generate_plates()` returns `Vec<PlateData>` with per-plate info
- `src/terrain_compute.rs` — `TerrainComputePipeline`, creates GPU buffers for plate data, passes to compute shader
- `src/shaders/plates.wgsl` — `boundary_influence()` generates height from boundary proximity. Currently all boundaries get the same mountain treatment
- `src/app.rs` — `PlateGenParams` struct passed to `generate_plates()`, UI in Advanced Tweaks section

## Key Technical Decisions

- **Classify on CPU, encode for GPU**: Compute velocity vectors and boundary types on CPU in `plates.rs`. Encode boundary type into the plate data buffer passed to the GPU. The GPU shader reads the type and generates terrain accordingly.
- **Boundary type as per-pixel computation**: At each pixel, the shader already knows the two nearest plates. It can look up their velocity vectors and determine the boundary type on-the-fly. This avoids storing a separate boundary type buffer.
- **Velocity from physics**: `speed ∝ sqrt(Ra)` where Rayleigh number Ra ∝ mass × rotation_factor. Direction is Euler pole rotation derived from seed. This reuses the existing `tectonics_factor` from `DerivedProperties`.

## Open Questions

### Resolved During Planning

- **How to pass velocity to GPU?**: Add `velocity_x, velocity_y, velocity_z` to the per-plate data buffer. The existing buffer already stores `center_x, center_y, center_z, plate_type` per plate — extend with 3 more floats per plate.
- **Subduction direction?**: At ocean-continent convergence, the oceanic plate always subducts. At ocean-ocean, compare plate density (approximated by distance from plate center — older crust = denser). Trench forms on the subducting side.

### Deferred to Implementation

- Exact terrain height profiles for each boundary type — tune visually during implementation
- Whether the velocity encoding needs 16-byte alignment in the GPU buffer

## Implementation Units

- [ ] **Unit 1: Add plate velocities to PlateData**

  **Goal:** Generate physics-derived velocity vectors for each plate on CPU.

  **Requirements:** R2

  **Dependencies:** None

  **Files:**
  - Modify: `src/plates.rs` (PlateData struct, generate_plates fn)

  **Approach:**
  - Add `velocity: [f32; 3]` to `PlateData`
  - Velocity magnitude: `base_speed * tectonics_factor * (rotation_period_factor)`
  - Direction: Euler pole rotation — cross product of plate center with a seed-derived axis, giving tangential motion on sphere surface
  - Each plate gets a unique direction from `fract(sin(plate_index * hash) * large_prime)`

  **Patterns to follow:**
  - Existing `PlateData` struct fields and generation logic in `plates.rs`

  **Test scenarios:**
  - Happy path: `generate_plates()` returns plates with non-zero velocity vectors
  - Happy path: Velocity magnitudes scale with `tectonics_factor`
  - Edge case: All velocity vectors are tangent to the sphere (dot(velocity, center) ≈ 0)

  **Verification:** Plates have velocity vectors, tests pass.

- [ ] **Unit 2: UI dropdown for Tectonics Mode**

  **Goal:** Add Quick/Classified mode selector in Advanced Tweaks.

  **Requirements:** R8

  **Dependencies:** None (parallel with Unit 1)

  **Files:**
  - Modify: `src/app.rs` (add `tectonics_mode: u32` field, UI dropdown)

  **Approach:**
  - Add `tectonics_mode: u32` to `PlanetGenApp` (0=Quick, 1=Classified)
  - Add dropdown in Advanced Tweaks after the existing plate controls
  - When mode changes, trigger `needs_terrain = true`
  - Pass mode to `PlateGenParams` so `generate_plates()` knows whether to compute velocities

  **Patterns to follow:**
  - Existing `view_mode` label-based selector pattern
  - `num_plates_override` for passing config through PlateGenParams

  **Test scenarios:**
  - Happy path: Dropdown appears, defaults to Quick
  - Happy path: Switching mode triggers terrain regeneration

  **Verification:** UI dropdown works, mode change regenerates terrain.

- [ ] **Unit 3: Pass velocities to GPU + boundary classification in shader**

  **Goal:** Extend the GPU plate data buffer with velocities, classify boundaries in the compute shader.

  **Requirements:** R1, R3-R7

  **Dependencies:** Unit 1, Unit 2

  **Files:**
  - Modify: `src/terrain_compute.rs` (extend plate buffer layout)
  - Modify: `src/shaders/plates.wgsl` (read velocities, classify boundaries, generate terrain by type)

  **Approach:**
  - Extend the per-plate GPU buffer to include `velocity_x, velocity_y, velocity_z` after existing fields
  - In `plates.wgsl`, at each pixel that's near a boundary:
    1. Identify the two nearest plates
    2. Compute relative velocity: `v_rel = v1 - v2`
    3. Project onto boundary normal: `convergent = dot(v_rel, boundary_normal) < 0`
    4. If convergent: check plate types for the 6-combination matrix
    5. If nearly perpendicular to boundary: transform fault
    6. If divergent: rift valley
  - Replace the uniform mountain generation with type-specific terrain:
    - Convergent ocean-ocean: trench (negative height dip) + island arc (peaks offset from boundary)
    - Convergent ocean-continent: trench on ocean side + tall Andes-style ridge on land side
    - Convergent continent-continent: broad raised plateau, no trench
    - Divergent: V-shaped valley with slight central ridge
    - Transform: no height change, slight depression along fault
  - In Quick mode: skip velocity lookup, use existing mountain-only behavior

  **Patterns to follow:**
  - Existing `boundary_influence()` function structure in `plates.wgsl`
  - Existing plate data buffer layout in `terrain_compute.rs`

  **Test scenarios:**
  - Happy path: Quick mode produces identical terrain to current (regression test)
  - Happy path: Classified mode produces visually different terrain at different boundary types
  - Happy path: Trench visible at ocean-continent convergence
  - Edge case: Transform boundaries produce no mountains
  - Integration: Changing tectonics_mode in UI produces correct terrain for each mode

  **Verification:** Plates debug view shows distinct boundary types. Normal view shows trenches, rifts, and ridges in appropriate locations. Quick mode unchanged.

- [ ] **Unit 4: Performance comparison + visual validation**

  **Goal:** Benchmark Classified vs Quick, document visual differences.

  **Requirements:** R8, R9 (performance + visual success criteria)

  **Dependencies:** Unit 3

  **Files:**
  - Modify: `src/bin/perf_bench.rs` (add Classified mode benchmark)
  - Modify: `docs/research/performance-analysis.md` (add comparison)

  **Approach:**
  - Run perf_bench in both modes at 768px and 1024px
  - Classified should add < 50ms to plate generation (per requirements)
  - Document timing comparison
  - Take screenshots of same seed in both modes for visual comparison

  **Test scenarios:**
  - Happy path: Classified mode adds < 50ms at 768px
  - Happy path: Visual comparison shows distinct boundary features

  **Verification:** Performance within budget, visual comparison documented.

## System-Wide Impact

- **Unchanged invariants:** Quick mode is byte-identical to current behavior. All existing parameters, export pipeline, preview rendering unchanged.
- **Buffer layout change:** The plate data GPU buffer grows by 3 floats per plate. This affects `terrain_compute.rs` buffer creation and `plates.wgsl` data access. No other shaders use this buffer.
- **PlateGenParams extension:** Adding `tectonics_mode` to the params struct requires updating all construction sites (app.rs, tests, bins).

## Risks & Dependencies

| Risk | Mitigation |
|------|------------|
| Boundary type classification may be noisy near triple junctions (3 plates meet) | Use dominant boundary type from the two largest plates; ignore third |
| Trench depth may cause visual artifacts (below ocean floor) | Clamp trench depth relative to existing ocean level |
| GPU buffer alignment issues with 3 extra floats per plate | Pad to 4 floats if needed (add dummy field) |

## Sources & References

- **Origin document:** [docs/brainstorms/2026-04-01-tectonic-boundary-classification-requirements.md](docs/brainstorms/2026-04-01-tectonic-boundary-classification-requirements.md)
- Existing plate system: `src/plates.rs`, `src/shaders/plates.wgsl`, `src/terrain_compute.rs`
- Physics model: `src/planet.rs` (DerivedProperties, tectonics_factor, Rayleigh number)
