---
title: "feat: Visual polish — AO, export clouds/lights, polar ice"
type: feat
status: active
date: 2026-03-31
---

# Visual Polish: AO, Export Layers, Polar Ice

## Overview

Three independent visual improvements: terrain ambient occlusion for depth, exporting cloud and city light layers as textures, and improved polar ice rendering.

## Requirements Trace

- R1. Ambient occlusion darkens terrain valleys and crevices for visual depth
- R2. Cloud density exported as 8K PNG alongside existing texture maps
- R3. City lights / night emission exported as 8K PNG
- R4. Polar ice has gradual transition, better visual quality, seasonal variation

## Scope Boundaries

- NOT changing the export pipeline architecture (just adding new map types)
- NOT adding volumetric AO (screen-space approximation from height neighbors)
- NOT modifying existing export map formats

## Key Technical Decisions

- **Height-difference AO over SSAO**: Sample 4-8 height neighbors, compare to current height. Lower neighbors = exposed ridge (bright), higher neighbors = occluded valley (dark). This is cheap and already used for terrain normals (`compute_terrain_normal` samples 4 neighbors). No new uniforms needed.
- **Cloud/light export as separate compute passes**: The existing export pipeline runs compute shaders per cubemap face. Add new shader entry points that output cloud density and city light intensity as R8 channels. Follow the existing albedo/roughness export pattern.
- **Polar ice improvement in fragment shader only**: Enhance the existing ice rendering code in `fs_main` with noise-based ice edge, subsurface blue tint, and seasonal sea ice extent.

## Implementation Units

- [ ] **Unit 1: Terrain ambient occlusion**

  **Goal:** Darken valleys and crevices by sampling height neighbors.

  **Requirements:** R1

  **Dependencies:** None

  **Files:**
  - Modify: `src/shaders/preview_cubemap.wgsl`

  **Approach:**
  - Add `compute_ao(sphere_pos)` function near `compute_terrain_normal`
  - Sample height at 8 positions around the current point (reuse the step size from `compute_terrain_normal`)
  - For each neighbor: if neighbor is higher, this pixel is occluded. Accumulate occlusion.
  - `ao = 1.0 - occlusion_count * weight` — valleys are darker
  - Apply to ambient term in PBR lighting: `ambient *= ao`
  - Add noise variation to break uniformity

  **Patterns to follow:**
  - `compute_terrain_normal` for height neighbor sampling pattern

  **Test scenarios:**
  - Happy path: Build succeeds, planet renders with darker valleys visible at zoom
  - Edge case: Flat ocean areas → AO = 1.0 (no darkening)
  - Edge case: Mountain peaks → AO near 1.0 (exposed, not occluded)

  **Verification:** Visible darkening in terrain valleys and crevices. Mountain ridges remain bright.

- [ ] **Unit 2: Export cloud density + city lights as textures**

  **Goal:** Add cloud and night light map export to the 8K pipeline.

  **Requirements:** R2, R3

  **Dependencies:** None (independent of Unit 1)

  **Files:**
  - Modify: `src/export.rs` (add cloud/light export passes)
  - Modify: `src/shaders/preview_cubemap.wgsl` or create new export shader entry points

  **Approach:**
  - Add `cloud_map` and `night_light_map` to the export output set
  - Cloud map: evaluate `compute_cloud_density` at each texel on the cubemap, output as R8
  - Night light map: evaluate `compute_urban_density` at each texel, output as R8
  - Follow the existing roughness/normal map export pattern (compute shader per face tile)
  - Write as PNG in the same output directory (`<planet_name>/cloud_map_*.png`, `<planet_name>/night_light_*.png`)

  **Patterns to follow:**
  - `src/export.rs` existing map export (roughness_map, normal_map patterns)
  - `src/shaders/roughness_map.wgsl` for compute shader export pattern

  **Test scenarios:**
  - Happy path: Export produces cloud_map and night_light_map PNGs for all 6 faces
  - Happy path: Cloud map values match preview visually (dense where clouds visible)
  - Edge case: cloud_coverage=0 → cloud map is all black
  - Edge case: night_lights=0 → night light map is all black

  **Verification:** Export directory contains cloud and night light PNGs. Files open correctly and show expected patterns.

- [ ] **Unit 3: Improved polar ice rendering**

  **Goal:** Better visual quality for polar ice caps with gradual transition and seasonal variation.

  **Requirements:** R4

  **Dependencies:** None (independent)

  **Files:**
  - Modify: `src/shaders/preview_cubemap.wgsl`

  **Approach:**
  - Find the existing ice rendering code in `fs_main` (ice overlay after surface color)
  - Add noise-based ice edge: `snoise(pos * 15) * 0.05` perturbs the ice boundary for organic coastlines
  - Add subsurface blue tint to thick ice: mix toward `vec3(0.75, 0.85, 0.95)` for deep ice
  - Seasonal sea ice: use `uniforms.season` to expand/contract ice extent (colder season = more ice)
  - Thin ice near edges: partially transparent, showing dark ocean underneath
  - Snow on ice: brighten with altitude/latitude

  **Patterns to follow:**
  - Existing ice rendering code in `fs_main` (ocean ice and land ice sections)
  - `compute_temperature` for seasonal temperature variation

  **Test scenarios:**
  - Happy path: Polar caps have organic, noisy edges instead of smooth latitude lines
  - Happy path: Thick ice has slight blue tint, thin ice shows ocean below
  - Happy path: Season slider changes ice extent visibly
  - Edge case: Hot planet (high base_temp) → minimal or no ice

  **Verification:** Polar ice looks more natural with irregular edges, blue-tinged thick ice, and visible seasonal variation.

## Risks & Dependencies

| Risk | Mitigation |
|------|------------|
| AO neighbor sampling adds GPU cost | 8 height samples is cheap (same as terrain normals). Only applies to land pixels. |
| Cloud export requires cloud uniforms in compute shader | Pass cloud_coverage and cloud_seed as compute shader params, same pattern as existing terrain params |

## Sources & References

- Existing code: `src/shaders/preview_cubemap.wgsl` (compute_terrain_normal, ice rendering, compute_cloud_density)
- Export pipeline: `src/export.rs` (existing map export pattern)
- Export shaders: `src/shaders/roughness_map.wgsl`, `src/shaders/normal_map.wgsl`
