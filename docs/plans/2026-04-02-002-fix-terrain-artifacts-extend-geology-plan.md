---
title: "fix: Terrain artifact fixes + geological feature extensions"
type: fix
status: completed
date: 2026-04-02
origin: docs/brainstorms/2026-04-02-physics-terrain-rebuild-requirements.md
---

# Fix Terrain Artifacts + Extend Geological Features

## Overview

The tectonic terrain compute pipeline (Voronoi plates → crustal thickness → isostasy → cubemap) is architecturally sound but produces four visual artifacts: noodle ridges, puzzle-piece continents, boundary ghosting, and speckle. Root causes are identified in specific shader parameters and missing blending logic. This plan fixes those artifacts and adds three missing geological features: continental shelf profiles, stagnant lid terrain mode, and hotspot island chains.

## Problem Frame

The existing pipeline in `plates.wgsl` already implements Voronoi plate assignment, boundary classification, crustal thickness computation, and isostatic elevation conversion. The fragment shader `preview_cubemap.wgsl` reads the cubemap and runs the climate/biome pipeline on top. The architecture is correct — the problems are in specific algorithmic choices within the compute shader. (see origin: docs/brainstorms/2026-04-02-physics-terrain-rebuild-requirements.md)

## Requirements Trace

- R1. Widen mountain influence zones (noodle ridge fix)
- R2. Increase intra-plate thickness variation with per-plate seed offsets (puzzle-piece fix)
- R3. Blend between two nearest plates' properties at boundaries (ghosting fix)
- R4. Reduce boundary bias noise amplitude (speckle fix)
- R5. Suppress detail noise near boundaries (speckle fix)
- R6. Dual density ratios for continental vs oceanic crust in isostasy
- R7. Continental/oceanic thickness ranges matching Earth-like hypsometry
- R8. Continental shelf profiles at continent-ocean transitions
- R9. Active vs passive margin shelf width from boundary type
- R10. Stagnant lid terrain when tectonic regime < 0.2
- R11. Continuous blending between stagnant lid and plate tectonic terrain (0.2-0.5)
- R12. Cross-fade formula from plate-based to noise-only terrain at low regime
- R13. Hotspot trailing island chains in plate motion direction

## Scope Boundaries

- No hydraulic erosion, river networks, or fluid simulation
- No gas giant / ice giant terrain
- No plate motion animation
- Climate/biome pipeline (Hadley cells, rain shadows, Whittaker, ocean/ice, clouds) is unchanged — it reads elevation only
- UI framework and application architecture unchanged
- Continental shelf detail at 512x512 preview resolution will be approximate (~78 km per texel)

## Context & Research

### Relevant Code and Patterns

- `src/shaders/plates.wgsl` — compute shader: `find_nearest_plates()` at ~line 112, `classify_boundary()` at ~line 150, isostatic conversion at ~line 253-255, hotspot code at ~lines 275-286
- `src/plates.rs` — `PlateGpu` (32 bytes: center[3], plate_type, velocity[3], pad), `generate_plates()`, Fibonacci sphere distribution
- `src/terrain_compute.rs` — `TerrainGenParams` (80 bytes, `#[repr(C)] Pod Zeroable`), `TerrainComputePipeline::generate()` dispatches per face, reads back `Vec<f32>`
- `src/preview.rs` — `PreviewUniforms` with `_pad3: [f32; 3]` and `_pad4: [f32; 3]` spare slots, cubemap R16Float texture upload
- `src/app.rs` — `build_uniforms()` at ~line 141, `regenerate_terrain()` at ~line 216, `terrain_params()` at ~line 194, override pattern: `self.age_override.unwrap_or(self.derived.surface_age)`
- `src/planet.rs` — `DerivedProperties::from_params()`, continuous `tectonics_factor` [0,1] from Rayleigh number proxy with sigmoid

**Pattern: Adding compute shader parameters**
1. Add field to `TerrainGenParams` in `terrain_compute.rs` (16-byte alignment)
2. Add matching field to `GenParams` in `plates.wgsl`
3. Wire through `generate()` signature and `terrain_params()` in `app.rs`

**Pattern: Adding fragment shader parameters**
1. Add to `PreviewUniforms` in `preview.rs` (can use existing `_pad3`/`_pad4` slots)
2. Add matching field in `preview_cubemap.wgsl` `Uniforms`
3. Wire in `build_uniforms()` in `app.rs`
4. **CRITICAL**: Also update `src/bin/sweep.rs` and `src/bin/erosion_compare.rs`

### Institutional Learnings

- Past solution: plates define tectonic features (where mountains form); independent noise defines geography (coastlines, bays). Use plate type as elevation *bias*, not deterministic mask. (source: `docs/solutions/architecture/tectonic-terrain-architecture-2026-03-30.md`)
- Fibonacci sphere index-order creates south pole bias for plate type assignment — use seed-based hash scoring instead. (same source)
- Erosion is sub-pixel at 512x512/face — don't rely on it for terrain character at preview resolution. (source: `docs/solutions/architecture/planet-gen-session-2-learnings-2026-03-30.md`)

## Key Technical Decisions

- **Boundary blending via mix() between two plates**: At each texel, compute thickness assuming it belongs to plate A, then plate B. Blend with `mix(thickness_A, thickness_B, blend_factor)` where blend_factor transitions smoothly across the boundary zone. This eliminates the discrete index-change discontinuity that causes ghosting. Doubles per-texel work but at 512x512x6 faces this remains well under 2 seconds. (see origin R3)
- **Dual density isostasy**: Continental crust (ρ=2.7 g/cm³) and oceanic crust (ρ=3.0 g/cm³) use different density ratios against mantle (ρ=3.3 g/cm³). This produces correct bimodal peaks: continental at ~+0.8 km, oceanic at ~-4 km. The current single-ratio approach gets the qualitative shape right but wrong peak separation. (see origin R6)
- **Stagnant lid as linear boundary suppression + cross-fade below 0.2**: At tectonic_regime 0.2-1.0, boundary forces scale linearly. Below 0.2, cross-fade the entire plate structure to noise-only terrain. The Voronoi cells still run (they're cheap) but their influence fades to zero. This avoids a second code path for most of the regime range. (see origin R11, R12)
- **Shelf profiles in compute shader, not fragment shader**: Despite low resolution (~78 km/texel at 512²), the shelf profile should be computed in the thickness field. At higher export resolutions (2048² or 4096²), it will be detailed enough. The fragment shader just reads the cubemap — keeping all terrain generation in one place.

## Open Questions

### Resolved During Planning

- **Voronoi-on-sphere approach**: Already implemented using spherical distance (dot product) in `find_nearest_plates()`. No change needed.
- **Performance of boundary blending**: At 512x512x6 with ~24 plates, doubling per-texel work adds ~50-100ms. Well within the 2-second budget based on current pipeline timing.
- **ocean_fraction semantics**: Controls the ratio of continental to oceanic plate classification during CPU plate generation (`plates.rs`). Not related to sea level.

### Deferred to Implementation

- Exact smoothstep ranges for mountain width and boundary blend zone — will need visual tuning with the layer toggle debug view
- Continental shelf sigmoid parameters (width, steepness) — tune against real bathymetric profiles
- Stagnant lid volcanic province count and size distribution — start with Mars reference (1 giant + scattered smaller), tune visually
- Hotspot island chain spacing and decay rate along the trail

## High-Level Technical Design

> *This illustrates the intended approach and is directional guidance for review, not implementation specification. The implementing agent should treat it as context, not code to reproduce.*

```
Current per-texel flow in plates.wgsl:
  find_nearest_plates(pos) → nearest_idx, second_idx, boundary_dist
  thickness = base_thickness(plate_type[nearest_idx]) + noise
  thickness += boundary_modification(boundary_type, boundary_dist) * tect
  height = isostasy(thickness)

Proposed per-texel flow:
  find_nearest_plates(pos) → plate_a, plate_b, boundary_dist, blend_factor

  // Compute thickness for BOTH plates
  thickness_a = base_thickness(plate_a) + per_plate_noise(pos, plate_a.seed)
  thickness_b = base_thickness(plate_b) + per_plate_noise(pos, plate_b.seed)

  // Blend at boundary
  thickness = mix(thickness_a, thickness_b, blend_factor)

  // Boundary modifications (mountains, rifts) — applied to the blended field
  thickness += boundary_features(classify(plate_a, plate_b), boundary_dist) * tect

  // Continental shelf profile
  thickness = apply_shelf_profile(thickness, continent_dist, margin_type)

  // Stagnant lid cross-fade (when tect < 0.2)
  stagnant_thickness = volcanic_provinces(pos) + impact_basins(pos)
  thickness = mix(stagnant_thickness, thickness, smoothstep(0.1, 0.3, tect))

  // Isostasy with dual density (interpolate ratio using blend_factor, NOT hard threshold)
  rho = mix(rho_plate_a, rho_plate_b, blend_factor)  // e.g. continental=2.7, oceanic=3.0
  height = thickness * (1.0 - rho / RHO_MANTLE) - reference_depth

  // Detail noise (suppressed near boundaries)
  detail_amp = base_amp * smoothstep(0.0, 0.10, boundary_dist)
  height += fbm_detail(pos) * detail_amp
```

## Implementation Units

- [x] **Unit 1: Boundary blending between nearest two plates**

**Goal:** Eliminate boundary ghosting by computing thickness for both nearest plates and blending smoothly across the boundary zone.

**Requirements:** R3

**Dependencies:** None — this is the foundational fix

**Files:**
- Modify: `src/shaders/plates.wgsl` — restructure main compute function
- Test: `src/terrain_compute.rs` (existing test module)

**Approach:**
- Modify `find_nearest_plates()` to also return a blend factor: `smoothstep(0.0, margin_width, (second_dist - nearest_dist) / (second_dist + nearest_dist))`
- In the main function, compute `thickness_a` and `thickness_b` independently for the two nearest plates (each gets its own `is_continental` check, base thickness, etc.)
- Blend with `mix(thickness_a, thickness_b, blend_factor)`
- Apply boundary features (mountains, rifts) AFTER blending, operating on the blended thickness field
- This means the boundary zone smoothly transitions between plate properties instead of having a discrete jump

**Patterns to follow:**
- Existing `find_nearest_plates()` already tracks both nearest and second-nearest plate. Extend it to return the blend factor.
- Existing boundary_dist calculation: `second_dist - nearest_dist`

**Test scenarios:**
- Happy path: Generate terrain with Earth-like params (tect=0.8, ~10 plates). Sample heights along a line crossing a plate boundary — height should transition smoothly with no >50m jumps between adjacent texels.
- Edge case: Sample points exactly equidistant from two plates (boundary_dist ≈ 0). Blend factor should be ~0.5, not undefined.
- Integration: Full 6-face cubemap generation completes without NaN, all heights in valid range [-12, 12] km.

**Verification:**
- Elevation debug view shows no visible seams at plate boundaries
- Adjacent texels across boundaries differ by < max_detail_amplitude (no discrete jumps)

---

- [x] **Unit 2: Parameter tuning — wider mountains, stronger intra-plate variation, quieter boundaries**

**Goal:** Fix noodle ridges, puzzle-piece continents, and speckle through targeted parameter changes.

**Requirements:** R1, R2, R4, R5

**Dependencies:** Unit 1 (blending must be in place for tuning to be meaningful)

**Files:**
- Modify: `src/shaders/plates.wgsl` — boundary influence zone width, noise amplitudes, detail suppression

**Approach:**
- R1 (noodle ridges): Widen mountain influence smoothstep from `(0.35, 0.0)` to `(0.15, 0.0)` or wider. Soften convergence/divergence transition from 0.5 to ~1.0 span.
- R2 (puzzle-piece): Increase base thickness noise amplitude from `7.0` to `12-18`. Add per-plate seed offset to the noise: `snoise(pos * freq + vec3(f32(plate_id) * 127.1, f32(plate_id) * 311.7, f32(plate_id) * 74.7))` to decorrelate adjacent plates.
- R4 (speckle): Reduce per-plate boundary bias noise from `0.08 + 0.04 = 0.12` to `0.03 + 0.01 = 0.04`.
- R5 (speckle): Add `detail_amp *= smoothstep(0.0, 0.10, boundary_dist)` to suppress high-frequency detail near plate boundaries.

**Patterns to follow:**
- Existing noise patterns in plates.wgsl (`snoise()` calls with offset vectors)
- Existing smoothstep usage for boundary influence zones

**Test scenarios:**
- Happy path: Generate terrain, measure standard deviation of elevation within a single continental plate interior. Should be > 500m (not flat).
- Happy path: Generate terrain, measure mountain peak heights near convergent boundaries. Peaks should be broad (affecting area > 0.05 distance units from boundary, not < 0.02).
- Edge case: At `tect=0.0`, boundary modifications should be zero regardless of tuning.

**Verification:**
- Continental plate interiors show visible terrain variation in elevation debug view
- Mountain ranges appear as broad elevated zones, not thin lines
- No speckle dots at 1K preview resolution

---

- [x] **Unit 3: Dual density isostasy**

**Goal:** Use distinct density ratios for continental and oceanic crust to produce correct bimodal hypsometric peaks.

**Requirements:** R6, R7

**Dependencies:** Unit 1 (blending affects how plate type is determined at boundaries)

**Files:**
- Modify: `src/shaders/plates.wgsl` — isostatic conversion formula

**Approach:**
- Replace the single `(thickness - T_ref) * 0.025` conversion with density-aware isostasy
- Continental crust: ρ_c = 2.7, mantle: ρ_m = 3.3 → elevation = thickness × (1 - 2.7/3.3) - reference
- Oceanic crust: ρ_c = 3.0, mantle: ρ_m = 3.3 → elevation = thickness × (1 - 3.0/3.3) - reference
- At boundaries, use the blended plate type to interpolate density ratio (smooth transition between continental and oceanic isostasy)
- Continental thickness range: 25-45 km with noise → elevations ~+0.3 to +2.0 km
- Oceanic thickness range: 5-10 km with noise → elevations ~-3.0 to -5.0 km

**Patterns to follow:**
- Existing isostatic conversion at ~line 253-255 in plates.wgsl

**Test scenarios:**
- Happy path: Generate Earth-like terrain. Histogram of elevations should show two peaks: one near +0.5 to +1.0 km (continental) and one near -3.5 to -4.5 km (oceanic). Peaks separated by > 3 km.
- Edge case: All-continental planet (ocean_fraction=0.0). Should produce unimodal distribution with all positive elevations.
- Edge case: All-oceanic planet (ocean_fraction=1.0). Should produce unimodal distribution with all negative elevations.

**Verification:**
- Elevation histogram from generated cubemap is verifiably bimodal for Earth-like params
- Peak separation matches real Earth hypsometry within ~1 km

---

- [x] **Unit 4: Continental shelf profiles**

**Goal:** Add realistic continental shelf and slope transitions at continent-ocean boundaries.

**Requirements:** R8, R9

**Dependencies:** Units 1-3 (needs blended boundaries and correct isostasy)

**Files:**
- Modify: `src/shaders/plates.wgsl` — add shelf profile post-thickness computation
- Modify: `src/terrain_compute.rs` — add shelf_width parameter to `TerrainGenParams` if needed

**Approach:**
- After thickness blending, detect continental-oceanic transition zone: where blended plate type transitions from continental to oceanic (use the blend_factor from Unit 1)
- In this transition zone, apply a shelf profile: flat shelf at ~-100m for the first portion, then steep slope to abyssal depth
- Profile shape: `shelf_height = mix(-0.1, abyssal_depth, smoothstep(shelf_start, shelf_end, blend_factor))`
- Shelf width varies by margin type: detect nearest boundary classification. If convergent (active margin): narrow shelf (shelf_end - shelf_start ≈ 0.02). If passive margin: wide shelf (≈ 0.05-0.08).
- Note: at 512² preview, shelf is 1-3 texels wide. The profile will be visible but coarse. Higher export resolutions will show proper detail.

**Patterns to follow:**
- Existing smoothstep-based boundary zone calculations in plates.wgsl
- Existing boundary type classification from `classify_boundary()`

**Test scenarios:**
- Happy path: Sample elevation along a transect from continental interior to oceanic interior. Should see: high continental → gentle slope → flat shelf near -100m → steep continental slope → deep ocean floor.
- Edge case: At a convergent boundary (active margin), shelf should be narrower than at a passive margin location on the same planet.
- Edge case: Stagnant lid planet (tect < 0.2) — shelf profiles should fade out (no distinct continental/oceanic transition).

**Verification:**
- Debug elevation view shows visible shelf zone at coastlines (lighter band of blue between land and deep ocean)
- Transect sampling confirms shelf plateau exists between +0 and -200m

---

- [x] **Unit 5: Stagnant lid terrain mode**

**Goal:** Generate Mars/Venus-style terrain without plate structure at low tectonic regimes, with continuous blending to plate-tectonic terrain.

**Requirements:** R10, R11, R12

**Dependencies:** Units 1-3 (the plate-tectonic side of the blend must work first)

**Files:**
- Modify: `src/shaders/plates.wgsl` — add stagnant lid terrain generation function, cross-fade logic
- Modify: `src/terrain_compute.rs` — may need new params for volcanic province count/scale

**Approach:**
- Add a `stagnant_lid_terrain(pos, params)` function that generates:
  - Large shield volcanic provinces: 3-6 broad low-frequency noise peaks (think Tharsis Bulge). Use `max(0, snoise(pos * 1.5 + offset) - threshold) * scale` to create isolated elevated regions.
  - Impact basins: 2-4 circular depressions from noise-seeded positions. Similar to existing crater stamping but larger scale (hundreds of km radius).
  - Smooth rolling plains: low-amplitude fBm as baseline
- Cross-fade: `thickness = mix(stagnant_thickness, plate_thickness, smoothstep(0.1, 0.3, tect))`
  - At tect < 0.1: pure stagnant lid
  - At tect 0.1-0.3: blending zone
  - At tect > 0.3: pure plate tectonics (existing code)
- The existing hotspot scaling (`(1.0 - tect) * factor`) already gives stagnant lid planets more hotspots — this complements the volcanic provinces

**Patterns to follow:**
- Existing hotspot code in plates.wgsl (~lines 275-286) for volcanic feature placement
- Existing crater stamping logic for impact basin shapes

**Test scenarios:**
- Happy path: Generate terrain with tect=0.0. Should show volcanic peaks, impact depressions, smooth plains. No Voronoi plate boundaries visible.
- Happy path: Generate terrain with tect=0.15. Should show hints of plate structure fading in, blended with volcanic terrain.
- Happy path: Generate terrain with tect=0.5. Plate structure should dominate, stagnant features should be minimal.
- Edge case: tect=0.0 with varied seeds. Each seed should produce different volcanic province placement.
- Integration: Elevation histogram at tect=0.0 should be unimodal (no bimodal continental/oceanic split).

**Verification:**
- At tect=0.0, debug elevation view shows isolated volcanic peaks and basins, no plate boundary artifacts
- At tect=0.5, terrain looks like current plate-tectonic output (regression check)
- Continuous slider movement from 0→1 shows smooth visual transition

---

- [x] **Unit 6: Hotspot trailing island chains**

**Goal:** Extend existing hotspot volcanism to create Hawaii-style island chains trailing in the plate motion direction.

**Requirements:** R13

**Dependencies:** Unit 1 (needs plate velocity access), Units 1-3 working

**Files:**
- Modify: `src/shaders/plates.wgsl` — extend hotspot section

**Approach:**
- For each hotspot, determine which plate it sits on (use `find_nearest_plates()` result for the hotspot center position)
- Compute the plate's velocity direction at the hotspot (from the Euler pole rotation: `cross(euler_pole, hotspot_pos)` gives velocity direction)
- Place 2-4 trail volcanoes at positions along `hotspot_pos - velocity_dir * (i * spacing)`, each progressively smaller and more eroded (lower height, broader profile)
- Trail spacing: ~0.03-0.05 distance units (~300-500 km at planet scale)
- Trail volcano height decay: each ~60-70% of the previous
- Only apply on plate-tectonic planets (tect > 0.3). On stagnant lid planets, hotspots are stationary (no plate to move over them)

**Patterns to follow:**
- Existing hotspot volcano profile code in plates.wgsl
- PlateGpu struct has velocity[3] field accessible in the shader

**Test scenarios:**
- Happy path: Generate terrain with tect=0.8. Hotspot should show a main volcano + 2-3 smaller trailing features in a line.
- Edge case: Hotspot near a plate boundary — trail should follow the plate it's on, not jump to the other plate.
- Edge case: tect=0.1 (stagnant lid) — no island chains, just the main hotspot feature.

**Verification:**
- Debug elevation view shows visible chain of progressively smaller peaks near hotspot locations
- Chain direction is consistent with the plate's motion vector

## System-Wide Impact

- **Interaction graph:** Changes are isolated to `plates.wgsl` (compute shader) and its parameter structs. The fragment shader `preview_cubemap.wgsl` reads the cubemap output — its code doesn't change. Climate/biome pipeline is unaffected.
- **Error propagation:** If the compute shader produces NaN heights, the fragment shader will render black/corrupt pixels. Existing NaN guard tests in `terrain_compute.rs` catch this.
- **State lifecycle risks:** `regenerate_terrain()` runs the full compute pipeline on each parameter change. No partial state — each run produces a complete new cubemap. No cache invalidation concerns.
- **API surface parity:** `sweep.rs` and `erosion_compare.rs` construct `PreviewUniforms` directly. If Unit 4 or 5 add new fields to `TerrainGenParams`, those binaries don't use the compute pipeline so they're unaffected. Only fragment-shader uniform changes (PreviewUniforms) trigger the breakage pattern.
- **Unchanged invariants:** The fragment shader pipeline (biomes, atmosphere, clouds, ocean rendering, layer toggles) is completely untouched. All changes are in the compute shader that produces the heightmap cubemap.

## Risks & Dependencies

| Risk | Mitigation |
|------|------------|
| Boundary blending doubles per-texel compute work | At 512² × 6 faces with ~24 plates, estimated +50-100ms. Well within 2s budget. Profile after Unit 1 to confirm. |
| Stagnant lid cross-fade at intermediate tectonic regimes may look unnatural | The transition zone (0.1-0.3) is narrow. Use the layer toggle debug view to tune the smoothstep range visually. |
| Parameter tuning (Unit 2) is subjective — "wide enough" mountains are a judgment call | Use the elevation debug layer for objective assessment. Define "wide enough" as mountain influence zone > 200 km (> 0.05 distance units). |
| Shelf profiles at 512² resolution are only 1-3 texels wide | Accepted limitation for preview. Document that export at 2048²+ will show proper shelf detail. |

## Sources & References

- **Origin document:** [Physics terrain rebuild requirements](docs/brainstorms/2026-04-02-physics-terrain-rebuild-requirements.md)
- Past solution: [Tectonic terrain architecture](docs/solutions/architecture/tectonic-terrain-architecture-2026-03-30.md)
- Past solution: [Session 2 learnings](docs/solutions/architecture/planet-gen-session-2-learnings-2026-03-30.md)
- Research: [Tectonic algorithms](docs/research/tectonic-algorithms.md) — Voronoi plates, boundary classification, isostasy, WGSL code reference
- Research: [Planetary interior formation](docs/research/planetary-interior-formation.md) — convection regimes, tectonic decision framework
- Research: [Procedural planet generation](docs/research/procedural-planet-generation.md) — hypsometric curves, terrain pipeline, erosion rates
