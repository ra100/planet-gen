---
title: "feat: Add cyclone storms with count slider"
type: feat
status: active
date: 2026-03-31
---

# Cyclone Storms with Count Slider

## Overview

Add cyclone/large storm systems to the cloud layer, controlled by a storm count slider (0-8). Storms integrate into the existing Schneider remap pipeline as local coverage boosts with a soft spiral bias, rather than separate density calculations.

## Problem Frame

The cloud layer has good base coverage (Schneider remap, domain-warped fBm, self-shadowing) but lacks large-scale weather system structure. Real planets show visible cyclone systems at mid-latitudes. Previous attempts at cyclones failed because they used separate density calculations with geometric spiral patterns. The new approach integrates storms into the existing coverage threshold system.

## Requirements Trace

- R1. Storm count slider (0-8) in the Clouds UI section
- R2. Storm count = 0 produces no visible change (early-out)
- R3. Storms appear as concentrated weather systems, not geometric spirals
- R4. Storm placement is deterministic from cloud_seed
- R5. Storms prefer mid-latitudes (30-55°) with Coriolis-correct rotation direction

## Scope Boundaries

- NOT adding storm animation or dynamics
- NOT adding separate storm seed (reuses cloud_seed)
- NOT rendering eye walls or distinct storm structure — just denser cloud concentrations with spiral tendency

## Key Technical Decisions

- **Storms as coverage boosts, not separate density**: Add storm influence to `local_coverage` before the Schneider remap. The existing noise creates the cloud texture; storms just make coverage higher in their region. This avoids the geometric overlay problem from v1.
- **Soft spiral bias via coverage modulation**: The spiral pattern modulates the coverage boost (more along spiral arms), not the density itself. Since the Schneider remap converts coverage to density, the spiral shows through as natural-looking cloud concentrations.
- **Replace `_pad2` with `storm_count`**: Uses the remaining padding field (144 bytes total unchanged).

## Implementation Units

- [ ] **Unit 1: Add storm_count uniform + UI**

  **Goal:** Add `storm_count` field to PreviewUniforms and a slider in the Clouds UI section.

  **Requirements:** R1, R2

  **Dependencies:** None

  **Files:**
  - Modify: `src/preview.rs` (PreviewUniforms struct + test)
  - Modify: `src/app.rs` (field, default, build_uniforms, UI slider)
  - Modify: `src/shaders/preview_cubemap.wgsl` (Uniforms struct)
  - Modify: `src/bin/sweep.rs`, `src/bin/erosion_compare.rs` (defaults)

  **Approach:**
  - Replace `_pad2: f32` with `storm_count: f32` in PreviewUniforms
  - Add matching field to WGSL Uniforms struct (replace `_pad2: f32`)
  - Add `storm_count: u32` field to PlanetGenApp (default 0)
  - Add integer slider 0-8 in Clouds section, triggers `needs_render = true`
  - Pass as `storm_count: self.storm_count as f32` in build_uniforms

  **Patterns to follow:**
  - `cloud_type` field addition pattern (just done in this session)
  - `num_plates_override` for integer slider pattern

  **Test scenarios:**
  - Happy path: Build succeeds, all 42 tests pass
  - Edge case: storm_count=0 → no visual change

  **Verification:** UI shows Storm Count slider in Clouds section.

- [ ] **Unit 2: Implement storm coverage boost in shader**

  **Goal:** Add storm systems as local coverage boosts with soft spiral bias in `compute_cloud_density`.

  **Requirements:** R3, R4, R5

  **Dependencies:** Unit 1

  **Files:**
  - Modify: `src/shaders/preview_cubemap.wgsl`

  **Approach:**
  - In `compute_cloud_density`, after computing `local_coverage` and before the Schneider remap:
  - Loop `i = 0..storm_count` (cast to i32, max 8)
  - Place each storm center pseudo-randomly using cloud_seed (same hash pattern as before: `fract(sin(...))` for lat/lon)
  - Alternate hemispheres for visual balance
  - Compute great-circle distance `d` from current position to storm center
  - Compute spiral bias: tangent-plane angle with soft `cos()` modulation (0.7-1.0 range, NOT hard bands)
  - Storm coverage boost: `Gaussian falloff × spiral_bias × base_strength`
  - Add boost to `local_coverage` (clamped to 1.0)
  - The existing noise + Schneider remap then creates the actual cloud texture within the storm

  **Patterns to follow:**
  - The tangent-plane spiral computation from the earlier v1 cyclone code (fix degenerate `cross(up, center)` near poles)
  - Storm center placement hash from earlier implementation

  **Test scenarios:**
  - Happy path: storm_count=4, coverage=0.5 → visible concentrated cloud masses at mid-latitudes
  - Happy path: Different cloud_seed values place storms at different positions
  - Edge case: storm_count=0 → storm loop doesn't execute, no performance cost
  - Edge case: storm_count=8 → planet has many storm systems, no visual artifacts

  **Verification:** Storms appear as dense, organic cloud concentrations (not geometric spirals). Reducing storm_count to 0 removes them. The existing cloud texture is visible within storm systems.

## System-Wide Impact

- **Unchanged invariants:** All existing cloud behavior unchanged when storm_count=0. Uniform struct stays at 144 bytes.
- **Performance:** Storm loop adds up to 8 iterations with a few trig calls each + tangent plane computation. No additional noise calls — storms only modify `local_coverage`. Negligible cost.

## Risks & Dependencies

| Risk | Mitigation |
|------|------------|
| Storms may look geometric again | Use soft spiral bias (0.7-1.0 range) not hard bands. Cloud texture from fBm dominates. |
| Storm placement may cluster | Alternate hemispheres. Pseudo-random lat/lon spread using different hash constants per storm. |

## Sources & References

- Earlier cyclone implementation in this session (removed in v2 rewrite — use as anti-pattern for geometric spirals)
- `compute_cloud_density()` in `src/shaders/preview_cubemap.wgsl` (integration point)
- `docs/research/cloud-layer-rendering.md` (Schneider remap technique)
