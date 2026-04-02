---
title: "feat: Procedural cloud layer with climate-driven coverage"
type: feat
status: active
date: 2026-03-31
---

# Procedural Cloud Layer

## Overview

Add a procedural cloud layer rendered as a thin shell above the planet surface. Cloud density varies with moisture, latitude (Hadley cells), and topology (orographic lift). User controls: cloud coverage slider and independent cloud seed. Clouds are lit by the sun and blend with the atmosphere.

## Problem Frame

The planet preview has terrain, biomes, ocean, ice, and atmosphere but no clouds. Clouds are a major visual feature of habitable planets — they define weather patterns, break up uniform ocean/land surfaces, and add depth. The existing moisture model already provides the data needed to drive realistic cloud placement.

## Requirements Trace

- R1. Cloud shell rendered above planet surface, below atmosphere
- R2. Cloud density driven by moisture (ITCZ=heavy, subtropical=sparse, mid-lat=moderate)
- R3. Topology influence: orographic clouds on windward side of mountains
- R4. Cloud coverage slider (0.0 = clear sky, 1.0 = heavy overcast)
- R5. Independent cloud seed (separate from terrain seed)
- R6. Sun-lit clouds: bright on day side, dark on night side
- R7. Clouds partially transparent — surface visible through thin clouds
- R8. No performance regression beyond acceptable preview frame time

## Scope Boundaries

- NOT rendering volumetric 3D clouds — this is a 2D shell projection
- NOT adding cloud shadows on the surface (future enhancement)
- NOT exporting clouds as a separate texture map (future)
- NOT simulating cloud dynamics/weather — static snapshot per seed
- NOT modifying `preview.wgsl` (legacy shader)

## Key Technical Decisions

- **Cloud shell altitude**: Render clouds at radius `1.0 + cloud_altitude` where `cloud_altitude ≈ 0.005–0.01` (just above surface, below atmosphere shell at ~1.02–1.04). Clouds are ray-intersected as a thin sphere, separate from atmosphere.

- **Noise-based cloud shapes**: Use 3–4 octave fBm noise on the sphere position, seeded by `cloud_seed`. This produces natural-looking cloud patterns without precomputation. Different noise frequencies create cumulus-like (low freq) vs cirrus-like (high freq) patterns.

- **Climate modulation**: Multiply noise density by the moisture field from `compute_moisture()` with season=0.5 (mean annual). High moisture → thick clouds, low moisture → sparse/none. This naturally creates ITCZ cloud bands, dry subtropical clear zones, and mid-latitude weather patterns.

- **Orographic enhancement**: Sample terrain height at the cloud shell position. Where terrain is high and on the windward side, boost cloud density (orographic lift). Use the existing `wind_direction_vec()` function.

- **Rendering order**: Surface → clouds → atmosphere. Clouds blend over the surface using alpha from cloud density. Atmosphere ray-march then applies over both, creating realistic depth layering.

- **Uniform packing**: Replace `_pad1` with `cloud_coverage`, add `cloud_seed`, `cloud_altitude`, `_pad2` (128 → 144 bytes, 9×16 aligned).

## Implementation Units

- [ ] **Unit 1: Add cloud uniforms**

  **Goal:** Add `cloud_coverage`, `cloud_seed`, `cloud_altitude` to the uniform pipeline.

  **Requirements:** R4, R5

  **Dependencies:** None

  **Files:**
  - Modify: `src/preview.rs` (PreviewUniforms struct + test)
  - Modify: `src/app.rs` (fields, defaults, build_uniforms, UI)
  - Modify: `src/shaders/preview_cubemap.wgsl` (Uniforms struct)
  - Modify: `src/bin/sweep.rs`, `src/bin/erosion_compare.rs` (defaults)

  **Approach:**
  - Replace `_pad1: f32` with `cloud_coverage: f32` in PreviewUniforms
  - Add `cloud_seed: f32`, `cloud_altitude: f32`, `_pad2: f32` (total: 144 bytes)
  - Add matching fields to WGSL Uniforms struct
  - Add `cloud_coverage: f32` (default 0.5), `cloud_seed: u32` (default = params.seed + 1000) to PlanetGenApp
  - Add cloud_altitude = 0.008 (fixed, not exposed as slider yet)
  - Add "Clouds" section in UI: coverage slider (0.0–1.0) + seed DragValue with randomize button
  - Cloud coverage slider triggers `needs_render = true`
  - Cloud seed change triggers `needs_render = true`
  - Update all PreviewUniforms construction sites (bins, tests)

  **Patterns to follow:**
  - `atmosphere_density`/`atmosphere_height` pattern in PreviewUniforms
  - `show_atmosphere` checkbox + slider pattern in app.rs UI
  - Existing `_pad1` replacement precedent from zoom/pan addition

  **Test scenarios:**
  - Happy path: cargo build succeeds with new fields, all 42 tests pass
  - Edge case: cloud_coverage=0.0 produces no visible change (no clouds)

  **Verification:** Build passes, tests pass, UI shows Cloud section with coverage slider and seed control.

- [ ] **Unit 2: Cloud density function in shader**

  **Goal:** Implement the cloud density computation that combines noise, moisture, and topology.

  **Requirements:** R2, R3, R5

  **Dependencies:** Unit 1

  **Files:**
  - Modify: `src/shaders/preview_cubemap.wgsl`

  **Approach:**
  - Add `compute_cloud_density(sphere_pos, cloud_seed, coverage) -> f32` function
  - Base: 3-octave fBm noise using `cloud_seed` offset for pattern generation
  - Modulate by moisture: call `compute_moisture(sphere_pos, height, 0.5)` — use mean annual moisture for stable cloud placement
  - Moisture scaling: `moisture_factor = smoothstep(20.0, 150.0, moisture)` — no clouds over deserts, heavy over tropics
  - Coverage threshold: `density = smoothstep(1.0 - coverage, 1.0 - coverage + 0.3, raw_noise * moisture_factor)`
  - This gives controllable coverage: low coverage → only densest patches survive, high coverage → everything shows
  - Orographic boost: sample height cubemap at cloud position, boost density where terrain is high (mountains force air up → condensation)
  - Return density in 0.0–1.0 range

  **Patterns to follow:**
  - `compute_moisture()` for climate-driven spatial variation
  - `gradient_color()` for smoothstep-based thresholding
  - Noise sampling patterns using `snoise()` and `seed_offset()`

  **Test scenarios:**
  - Happy path: density > 0 in high-moisture areas (tropics, mid-latitudes)
  - Happy path: density ≈ 0 in subtropical desert zones
  - Edge case: cloud_coverage = 0.0 → density = 0.0 everywhere
  - Edge case: cloud_coverage = 1.0 → most of the planet has some cloud

  **Verification:** Cloud density function produces spatially varying values that correlate with the moisture model.

- [ ] **Unit 3: Cloud rendering in fragment shader**

  **Goal:** Render the cloud layer as a lit, semi-transparent shell between the surface and atmosphere.

  **Requirements:** R1, R6, R7

  **Dependencies:** Unit 2

  **Files:**
  - Modify: `src/shaders/preview_cubemap.wgsl`

  **Approach:**
  - In `fs_main`, after surface color computation but before atmosphere:
    - Intersect view ray with cloud shell sphere at radius `1.0 + cloud_altitude`
    - If hit, compute `sphere_pos` at cloud intersection, rotate to world space
    - Call `compute_cloud_density()` to get opacity
    - If density > 0.01: compute cloud lighting from sun direction (simple NdotL on the sphere normal)
    - Cloud color: white base, darkened by shadow (night side), slightly tinted by atmosphere
    - Blend: `surface_color = mix(surface_color, cloud_color, cloud_alpha)`
    - `cloud_alpha = density * 0.85` (clouds are semi-transparent)
  - For atmosphere-only pixels (ring beyond planet), clouds are not visible (shell is inside atmosphere ring)
  - Cloud lighting uses the same `sun_dir` as surface lighting

  **Patterns to follow:**
  - Atmosphere ray-sphere intersection in `fs_main` (two-sphere pattern)
  - Surface color blending patterns (ice overlay, altitude zonation)
  - Sun illumination pattern from atmosphere ray-marcher

  **Test scenarios:**
  - Happy path: Earth-like planet shows white cloud patches over oceans and continents
  - Happy path: Night side clouds are dark/invisible
  - Happy path: Clouds partially obscure surface features below
  - Edge case: cloud_coverage = 0.0 → no cloud blending, surface unchanged
  - Edge case: airless planet (atmosphere_density ≈ 0) → clouds still render if coverage > 0 (clouds don't require atmosphere)
  - Edge case: zoomed in → cloud layer clearly visible as separate from surface

  **Verification:** Planet shows realistic cloud patterns: bright on sun side, transparent enough to see surface, distributed according to moisture zones.

## System-Wide Impact

- **Interaction graph:** Only `preview_cubemap.wgsl` fragment shader + `app.rs` UI + `preview.rs` uniforms. No callbacks or middleware.
- **Error propagation:** None — shader-only visual feature. Worst case: visual artifact, no crash.
- **Unchanged invariants:** Export pipeline shaders unaffected. Atmosphere rendering unchanged. Surface coloring unchanged when cloud_coverage = 0.
- **Performance:** One additional ray-sphere intersection + noise evaluation per fragment. Cloud density is ~4 noise samples per pixel — comparable to the existing fBm in biome coloring. Should stay within frame budget.

## Risks & Dependencies

| Risk | Mitigation |
|------|------------|
| Clouds obscure too much surface detail | Coverage slider defaults to 0.5, user can reduce to 0. Cloud alpha capped at 0.85 |
| Cloud noise looks unrealistic (too uniform/too random) | Use 3-octave fBm with domain warping + moisture modulation for natural patterns |
| Cloud rendering adds noticeable GPU cost | Simple ray-sphere + noise, no marching. Early-out when coverage = 0 |
| Cloud/atmosphere depth sorting artifacts | Render clouds BEFORE atmosphere — atmosphere wraps over everything correctly |

## Sources & References

- PreviewUniforms: `src/preview.rs:8` (128 bytes, `_pad1` available for reuse)
- Moisture model: `compute_moisture()` in `preview_cubemap.wgsl`
- Wind direction: `wind_direction_vec()` in `preview_cubemap.wgsl`
- Atmosphere rendering: `ray_march_atmosphere()` in `preview_cubemap.wgsl`
- UI patterns: slider + checkbox in `app.rs` (atmosphere, advanced tweaks)
- Noise: `snoise()` from `noise.wgsl`
