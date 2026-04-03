---
date: 2026-04-03
topic: multipass-plate-terrain
---

# Multi-Pass GPU Plate Terrain

## Problem Frame

The current terrain uses pure noise (domain-warped fBm + ridged multifractal). It produces organic shapes but lacks geological structure — mountains are randomly placed, not along collision zones; there's no tectonic signature (linear chains, rift valleys, mid-ocean ridges). A reference implementation (planet_heightmap_generation/) achieves much more realistic results by combining Voronoi plates with BFS distance fields and stress-driven terrain. Our earlier plate attempts failed because raw `boundary_dist` (second_dist - nearest_dist) created sharp tube artifacts. The reference avoids this by computing smooth distance fields via BFS on a grid.

This rebuild replaces the terrain with a multi-pass GPU compute pipeline using the Jump Flooding Algorithm (JFA) for smooth distance fields — the GPU-native equivalent of BFS.

## Requirements

**Multi-Pass Compute Pipeline**

- R1. Pass 1 (Plate Assignment): Assign each cubemap pixel to its nearest tectonic plate via Voronoi. Store plate index per pixel. Reuse existing plate generation (CPU Fibonacci sphere + velocities from plates.rs).
- R2. Pass 2 (Distance Fields via JFA): Compute distance-to-nearest-boundary for each pixel using Jump Flooding Algorithm. Output: smooth distance field (no sharp Voronoi edges). O(log n) passes on the same texture.
- R3. Pass 3 (Terrain Generation): Use plate assignment + distance fields + plate velocities to compute stress, mountain elevation, rift depth, shelf profiles, and detail noise. Output: heightmap cubemap (same format as current).
- R4. The pipeline replaces the current single-pass noise terrain entirely. Noise (fBm, ridged multifractal, domain warping) is still used for detail within the plate framework, not as independent continent-shaping.

**Collision Mountains & Asymmetric Profiles**

- R5. Convergent boundaries produce mountain ridges. Height scales with collision stress (relative plate velocity dotted with boundary normal).
- R6. Asymmetric mountain profiles at subduction zones: steeper slope on the subducting (oceanic) side with a trench, gentler back-arc plateau on the overriding (continental) side.
- R7. Fold ridges parallel to plate motion direction (dot product with Euler pole), not perpendicular to boundary. These create linear valley/ridge patterns within mountain zones.
- R8. Divergent boundaries produce mid-ocean ridges (subtle elevation) and continental rift valleys.

**Continental Shelves**

- R9. Distance-based ocean floor profile using the JFA distance field from coast: shallow shelf (0-5 cells from coast), steep continental slope (5-12 cells), flat abyssal plain (12+).
- R10. Active margins (near convergent boundaries) have narrow shelves; passive margins have wide shelves.

**Stress-Driven Roughness**

- R11. Terrain roughness (fBm detail amplitude) scales with collision stress: rough near active boundaries (orogens), smooth in plate interiors (cratons).
- R12. Creates visual variety — mountainous collision zones are craggy, continental interiors are rolling plains, ocean floor is smooth.

**Plate Configuration**

- R13. 10-20 plates for Earth-like planets (reference uses 10-20). Plate count derived from planet mass, overridable.
- R14. Each plate has: continental/oceanic type, Euler pole velocity, density. Use existing plates.rs generation.
- R15. Continental/oceanic plate type sets base elevation bias (thick continental crust floats higher). The plate type combined with JFA distance field creates the land/ocean boundary naturally.

**Integration**

- R16. The fragment shader (preview_cubemap.wgsl) continues reading the cubemap — no changes needed. Climate, biomes, atmosphere, clouds all work on top of the new heightmap.
- R17. Water_loss and climate_moisture sliders continue to work as before.
- R18. Preview generation target: <2 seconds for full parameter change (all passes combined).

## Success Criteria

- Earth-like parameters produce recognizable tectonic features: linear mountain chains along convergent boundaries, rift valleys at divergent boundaries, smooth continental interiors, continental shelves
- Mountains concentrate at collision zones, not randomly across the surface
- Changing plate velocities visibly changes where mountains form
- No boundary tube artifacts (the JFA distance fields should be smooth)
- Terrain has visible roughness variation: craggy collision zones vs. smooth cratons
- Different seeds produce different but always geologically plausible plate configurations

## Scope Boundaries

- Not simulating plate motion over time (static snapshot)
- Not implementing mantle convection
- No hydraulic erosion in this phase (existing erosion pipeline continues to work on the output)
- Not changing the fragment shader biome/climate pipeline
- Not adding new UI parameters initially (plate count already exists, stress emerges from existing velocity vectors)
- Transform faults: minimal treatment (linear depression), not a priority

## Key Decisions

- **JFA for distance fields over BFS**: JFA runs in O(log n) passes on GPU textures — naturally parallel, no CPU involvement. BFS requires iterative CPU processing or complex GPU atomics. JFA produces equivalent smooth distance fields.
- **Replace pure noise entirely**: The plate system subsumes noise — fBm and ridged multifractal are used for detail within the plate framework. No dual code path, no mode switching.
- **Asymmetric profiles via subduction factor**: Following the reference approach, a subduction factor (0-1) determines which side of a convergent boundary is steeper. This single parameter creates realistic mountain asymmetry.

## Outstanding Questions

### Deferred to Planning

- [Affects R2][Technical] JFA implementation in WGSL: requires ping-pong textures (two textures alternating read/write per pass). The compute pipeline currently uses a single storage buffer. Need to assess whether to use textures or buffers for JFA.
- [Affects R6][Needs research] Exact subduction factor computation: the reference uses plate density contrast + velocity angle. Need to determine the specific formula.
- [Affects R9][Technical] Continental shelf cell-distance thresholds need calibration against our cubemap resolution (512px per face).
- [Affects R1][Technical] Whether to reuse the existing find_nearest_plates() Voronoi or switch to a texture-based assignment for JFA input.

## Next Steps

-> `/ce:plan` for structured implementation planning
