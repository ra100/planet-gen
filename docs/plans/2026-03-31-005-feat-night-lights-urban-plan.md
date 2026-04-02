---
title: "feat: Night side city lights + day side urban grey patches"
type: feat
status: active
date: 2026-03-31
---

# Night Side City Lights + Urban Areas

## Overview

Add procedural urban/city areas that are visible as grey patches on the day side and as bright light clusters on the night side. Controlled by a "Development" slider (0 = pristine, 1 = heavily urbanized).

## Problem Frame

The planet currently looks like a pristine wilderness. For sci-fi renders, habitable planets need signs of civilization — city lights visible on the dark hemisphere and urban grey sprawl visible from space on the day side.

## Requirements Trace

- R1. Procedural urban density function: cities on habitable land, clustered near coasts, temperate zones, lowlands
- R2. Day side: grey/dark patches on surface in urban areas (visible at zoom)
- R3. Night side: bright warm-yellow light clusters in urban areas (only on unlit hemisphere)
- R4. Development slider (0.0–1.0): controls urbanization level. 0 = no cities, 1 = heavily urbanized
- R5. Development = 0 produces no visual change

## Scope Boundaries

- NOT adding road networks or grid patterns
- NOT adding individual building geometry
- NOT exporting city lights as a separate texture map (future)

## Key Technical Decisions

- **Urban density from existing climate data**: Use temperature, moisture, height, and ocean proximity already computed in the shader. Cities prefer: temperate land (10-25°C), low elevation, near coasts. This requires no new noise channels — just a threshold on existing data plus high-frequency noise for clustering.
- **Replace one `_pad3` field with `night_lights`**: PreviewUniforms has `_pad3: [f32; 3]` — replace first with `night_lights: f32` (development level). Keeps 160 bytes.
- **Night detection via NdotL**: The sun-facing dot product already computed in `fs_main` determines day/night. City lights fade in on the dark side using `smooth_step(0.05, -0.1, n_dot_l)`.

## Implementation Units

- [ ] **Unit 1: Add night_lights uniform + UI slider**

  **Goal:** Add development level uniform and slider in the UI.

  **Requirements:** R4, R5

  **Dependencies:** None

  **Files:**
  - Modify: `src/preview.rs` (PreviewUniforms: replace first `_pad3` element)
  - Modify: `src/app.rs` (field, default=0, build_uniforms, slider)
  - Modify: `src/shaders/preview_cubemap.wgsl` (Uniforms struct)
  - Modify: `src/bin/sweep.rs`, `src/bin/erosion_compare.rs` (defaults)

  **Approach:**
  - Replace `_pad3: [f32; 3]` with `night_lights: f32, _pad3: [f32; 2]`
  - Add matching field to WGSL Uniforms struct
  - Add `night_lights: f32` to PlanetGenApp (default 0.0)
  - Slider 0.0–1.0 labeled "Development" in a new section after Clouds, with tooltip

  **Patterns to follow:**
  - `cloud_type` / `storm_count` field addition pattern

  **Test scenarios:**
  - Happy path: Build succeeds, 42 tests pass
  - Edge case: night_lights=0 → no visual change

  **Verification:** Slider appears in UI, builds clean.

- [ ] **Unit 2: Urban density function + day/night rendering**

  **Goal:** Compute procedural urban density and render as grey patches (day) and light clusters (night).

  **Requirements:** R1, R2, R3

  **Dependencies:** Unit 1

  **Files:**
  - Modify: `src/shaders/preview_cubemap.wgsl`

  **Approach:**
  - Add `compute_urban_density(sphere_pos, height)` function:
    - Early-out if `night_lights <= 0`
    - Only on land (`height > ocean_level`)
    - Prefer temperate zones: `smooth_step(5, 15, temp) * smooth_step(30, 20, temp)`
    - Prefer low elevation: `1.0 - smooth_step(0.0, 0.3, land_height)`
    - Prefer coasts: sample neighbors for ocean proximity (reuse existing pattern from `compute_moisture`)
    - High-frequency noise for city clustering: `snoise(pos * 40) * 0.5 + snoise(pos * 80) * 0.3 + snoise(pos * 160) * 0.2` — creates small bright clusters
    - Threshold with development level: higher development = more area urbanized
    - Return 0.0–1.0 density
  - Day side rendering (in `fs_main` after surface_color, before lighting):
    - `urban = compute_urban_density(rotated, height) * night_lights`
    - Grey the surface: `surface_color = mix(surface_color, vec3(0.45, 0.43, 0.42), urban * 0.5)`
  - Night side rendering (after PBR lighting, before clouds):
    - `night_factor = smooth_step(0.05, -0.1, n_dot_l)` — 1 on dark side, 0 on lit side
    - `light_intensity = urban * night_factor * night_lights`
    - Warm yellow-orange emission: `lit_color += vec3(1.0, 0.85, 0.4) * light_intensity * 0.3`
    - Fine-grain sparkle: additional very high frequency noise to create individual "light dots"

  **Patterns to follow:**
  - `compute_cloud_density` for the density function pattern
  - `compute_moisture` for coast proximity sampling
  - Cloud shadow rendering for the day-side surface modification pattern

  **Test scenarios:**
  - Happy path: night_lights=0.5, planet shows grey patches on visible land near coasts
  - Happy path: Rotate to night side → warm yellow glow clusters visible on dark land
  - Edge case: night_lights=0 → no grey patches, no lights
  - Edge case: night_lights=1.0 → dense urbanization on all habitable land
  - Edge case: Ocean/ice areas → no urban density (ocean and frozen land excluded)

  **Verification:** Day side shows subtle grey urban patches. Night side shows warm city light clusters. Development=0 produces pristine planet.

## Risks & Dependencies

| Risk | Mitigation |
|------|------------|
| Urban areas may look like noise blobs | Use very high frequency noise (40-160×) for fine-grain clustering + threshold for crisp city edges |
| Night lights too bright/dim | Use HDR emission that goes through existing tonemap; tune intensity empirically |
| Performance: urban density adds noise calls | Only compute when night_lights > 0; urban noise is cheap (3 snoise calls) |

## Sources & References

- Existing shader: `src/shaders/preview_cubemap.wgsl` (compute_cloud_density, compute_moisture patterns)
- PreviewUniforms: `src/preview.rs` (_pad3 has 3 spare f32 slots)
- PBR lighting in fs_main: n_dot_l already computed for day/night detection
