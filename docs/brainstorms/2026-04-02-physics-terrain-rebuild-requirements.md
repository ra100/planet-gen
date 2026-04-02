---
date: 2026-04-02
topic: physics-terrain-rebuild
---

# Physics-Based Terrain Generation: Fix Artifacts + Add Missing Features

## Problem Frame

The tectonic terrain pipeline (Voronoi plates -> crustal thickness -> isostasy -> cubemap -> fragment shader) is already built and running. The compute shader (`plates.wgsl`), CPU plate generation (`plates.rs`), compute pipeline (`terrain_compute.rs`), and cubemap-reading fragment shader (`preview_cubemap.wgsl`) all exist. The physics chain from `PlanetParams` -> `DerivedProperties` -> `PlateGenParams` -> GPU is wired up.

However, the output has four persistent visual artifacts:
1. **Noodle ridges** — thin line mountains following plate boundaries instead of broad mountain ranges
2. **Puzzle-piece continents** — uniform elevation per plate with sharp edges, no interior variation
3. **Boundary ghosting** — visible seams at plate transitions
4. **Speckle** — high-frequency noise dots, especially near convergent boundaries

Additionally, several geological features are missing: continental shelf profiles, true stagnant-lid mode (Mars/Venus terrain without plates), and hotspot island chains.

This is a fix + extend task, not a greenfield rebuild. The architecture is correct; the algorithms need targeted fixes and the feature set needs completion.

## Root Cause Analysis

### Noodle Ridges
- **Cause**: Mountain influence zone is too narrow (`smoothstep(0.35, 0.0, boundary_dist)` in plates.wgsl ~line 320). At planet scale this is <50km — real orogens are 200-800km wide.
- **Cause**: Convergence/divergence transitions are binary (`smoothstep(0.0, -0.5, boundary_type)` at ~line 345-348), creating sharp on/off mountain activation.

### Puzzle-Piece Continents
- **Cause**: Base thickness noise amplitude too small (`7.0 * gravity_factor` at ~line 288). Continental plates need 10-50km of intrinsic variation, currently getting ~7-18km.
- **Cause**: No per-plate seed offset for intra-plate noise — adjacent plates share correlated noise patterns.
- **Cause**: Continental-oceanic margin transition too narrow (~0.1-0.15 units from smoothstep edges at ~line 312-314).

### Boundary Ghosting
- **Cause**: `find_nearest_plates()` returns hard `nearest_idx`/`second_idx`. A 1-pixel shift at a boundary changes the plate index, causing discrete jumps in `is_continental` and all downstream values.
- **Cause**: No blending between nearest two plates' properties — the boundary is a discontinuity in the thickness field.

### Speckle
- **Cause**: Per-plate boundary bias noise is too strong (`0.08 + 0.04 = 0.12` amplitude, ~line 3-4 bias computation). High-frequency (4.5x) noise pushes boundaries back and forth creating dotted patterns.
- **Cause**: Detail noise applies at full amplitude near boundaries with no falloff (~line 335-340). High octaves create speckle where boundary distance is small.

## Requirements

**Artifact Fixes (plates.wgsl compute shader)**

- R1. Widen mountain influence zone from 0.35 to 0.15-0.08 distance units (broader mountains). Convergence/divergence smoothstep transition should span ~0.8-1.0 units, not 0.5.
- R2. Increase base thickness noise amplitude from `7.0` to `12-18 * gravity_factor`. Add per-plate seed offset: `fbm(pos, seed + plate_id * large_prime)` so adjacent plates have uncorrelated interior terrain.
- R3. Blend between nearest two plates' properties at boundaries using `mix(prop_plate1, prop_plate2, boundary_blend)` where `boundary_blend = smoothstep(0.0, margin_width, normalized_dist)`. This eliminates the discrete index-change discontinuity.
- R4. Reduce per-plate boundary bias noise amplitude from 0.12 total to ~0.04 total. Remove or attenuate the 4.5x frequency component.
- R5. Suppress detail noise near boundaries: `detail_amplitude *= smoothstep(0.0, 0.10, boundary_dist)`. Detail texture fades to zero at plate boundaries where the large-scale structure dominates.

**Crustal Thickness & Isostasy Improvements**

- R6. Use distinct density ratios for continental crust (~2.7 g/cm3) and oceanic crust (~3.0 g/cm3) in the Airy isostasy formula, rather than a single constant. This produces correct bimodal peak separation: continental at ~+0.8 km, oceanic at ~-4 km.
- R7. Continental thickness range: 25-45 km with noise variation. Oceanic thickness range: 5-10 km. These ranges should produce Earth-like hypsometry when converted through isostasy.

**Continental Shelves & Margins (New Feature)**

- R8. Add continental shelf profile at continental-oceanic transitions: a shallow shelf (0 to -200m) extending ~0.02-0.05 distance units from the continental edge, then a steep continental slope to the abyssal plain.
- R9. Active margins (near convergent boundaries) have narrow shelves. Passive margins (away from boundaries) have wide shelves with gradual slopes. Margin type derived from nearest boundary classification.

**Stagnant Lid Mode (New Feature)**

- R10. When tectonic regime < ~0.2, suppress Voronoi plate structure entirely. Generate terrain from: large shield volcanic provinces (low-frequency noise peaks), impact basins (depressions from crater-like stamps), and smooth rolling plains.
- R11. Intermediate tectonic regime (0.2-0.5): plates exist but boundaries are weaker. Approach: linearly scale boundary thickness modification amplitude by `tectonic_regime`. At 0.2, boundaries barely affect thickness. At 0.5, half-strength. At 1.0, full strength. The Voronoi plate structure itself (continental vs oceanic classification) always applies above 0.2 — the question is only how much boundaries modify thickness.
- R12. Below tectonic regime 0.2, blend from plate-based to noise-only terrain: `thickness = mix(stagnant_lid_thickness, plate_thickness, smoothstep(0.1, 0.3, tectonic_regime))`.

**Hotspot Island Chains (New Feature)**

- R13. Existing hotspot volcanism code (plates.wgsl ~line 275-286) is preserved. Add trailing seamount chain: place 2-4 progressively older/smaller volcanic features trailing in the direction opposite to the plate's velocity vector at the hotspot location.

**Artistic Overrides**

- R14. All physics-derived intermediate values remain overridable in the UI: tectonic regime, ocean fraction, plate count, mountain intensity. The physics chain provides defaults; the user has final say.
- R15. `ocean_fraction` controls the ratio of oceanic to continental plate area. It determines how many plates are classified as continental vs oceanic during CPU plate generation, not sea level directly.

## Success Criteria

- Bimodal hypsometric histogram for plate-tectonic planets (two distinct peaks: continental ~+0.8 km, oceanic ~-4 km). Verifiable by sampling elevation distribution from the cubemap.
- Unimodal hypsometric histogram for stagnant-lid planets (single broad peak).
- Mountain elevation variance is >3x higher within 0.15 distance units of convergent boundaries compared to plate interiors (boundary concentration test).
- No visible seams at plate boundaries in elevation debug view.
- No speckle dots visible at 1K preview resolution.
- Continental interiors show terrain variation (hills, plains, plateaus) — not flat uniform elevation.
- Preview updates in <2 seconds after parameter change (compute + render, excluding shader compilation).
- Existing climate rendering (rain shadows, desert bands at ~30 lat, altitude zonation) still works correctly on the new heightmap.

## Scope Boundaries

- No plate motion animation (static snapshot)
- No hydraulic erosion (separate feature)
- No river networks
- No gas giant or ice giant terrain (rocky/terrestrial only)
- Not changing the climate/biome pipeline (Hadley cells, Whittaker biomes, rain shadows — all preserved as-is, they read elevation only)
- Not changing the UI framework or application architecture
- Continental shelf detail (R8-R9) at preview resolution (512x512 per face) will be approximate — each texel covers ~78 km, and shelves are 50-200 km wide. Fine detail deferred to higher-resolution export.

## Key Decisions

- **Fix, don't rebuild**: The compute pipeline architecture is correct. The artifacts come from specific parameter choices and missing blending logic in the shader, not architectural flaws.
- **Crustal thickness blending at boundaries**: The single most important fix. Currently there's a discrete jump when `nearest_idx` changes. Blending between the two nearest plates' thickness values eliminates ghosting and creates natural transitions.
- **Continuous tectonic regime, not binary**: Scale boundary influence linearly by regime. Below 0.2, cross-fade to noise-only terrain. No hard mode switch.
- **Dual density ratios**: Continental and oceanic crust have different densities. Using a single ratio produces wrong bimodal peak separation. Two ratios produce correct Earth-like hypsometry.

## Dependencies / Assumptions

- The existing compute shader pipeline (`terrain_compute.rs`) handles re-dispatching on parameter change — no infrastructure changes needed.
- PreviewUniforms may need new fields for shelf width, stagnant-lid parameters. Remember to update `sweep.rs` and `erosion_compare.rs` binaries when adding fields.
- The fragment shader `preview_cubemap.wgsl` reads only elevation from the cubemap — it does not depend on continental identity, so R3's blending won't break climate calculations.

## Outstanding Questions

### Deferred to Planning

- [Affects R3][Technical] Blending between two plates requires computing thickness for BOTH nearest plates at each texel, doubling the per-texel work. Profile to confirm this stays under 2s budget at 512x512x6.
- [Affects R8][Technical] Continental shelf profile function: sigmoid, polynomial, or piecewise linear? Need to match real bathymetric profiles. Likely a tuned smoothstep with distance from continental edge.
- [Affects R10][Needs research] Stagnant-lid volcanic province placement: how many, what size distribution? Reference Mars (Tharsis Bulge = 1 giant province + scattered smaller ones) or Venus (thousands of small volcanoes spread broadly).
- [Affects R13][Technical] Hotspot island chain trailing direction requires knowing plate velocity at arbitrary surface points — currently velocity is per-plate in the `PlateGpu` buffer. The compute shader can read this, but needs to compute the trail direction from the Euler pole rotation.

## Next Steps

-> `/ce:plan` for structured implementation planning
