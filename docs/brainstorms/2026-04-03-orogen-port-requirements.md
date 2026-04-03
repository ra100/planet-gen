---
date: 2026-04-03
topic: orogen-port
---

# Port planet_heightmap_generation Orogen Algorithm to Rust/wgpu

## Problem Frame

The current terrain generation uses pure noise — it produces organic shapes but lacks geological structure. Three attempts to add plate-based terrain on the cubemap architecture all failed due to per-face distance field limitations. The reference implementation (planet_heightmap_generation/) produces far more realistic terrain using a fundamentally different approach: a single global sphere grid with BFS distance fields, multi-scale plate hierarchy, and stress-driven orogeny. This project ports that algorithm to Rust/wgpu for native GPU performance.

## Requirements

**Sphere Representation**

- R1. Replace per-face cubemap terrain generation with a single global sphere grid. The reference uses an icosphere or subdivided grid that avoids face-boundary discontinuities.
- R2. The sphere grid must be GPU-friendly — either a flat buffer indexed by vertex ID, or a texture representation that supports neighbor lookups without face-boundary issues.
- R3. Grid resolution: 512K-2M vertices for preview (equivalent to current 512x512x6 = 1.5M pixels), scalable to 8M+ for export.

**Plate Simulation (adapted from reference)**

- R4. Voronoi plate assignment on the sphere grid (10-20 plates). BFS flood-fill from plate seeds, with noise perturbation for organic boundaries.
- R5. Dual-layer plate hierarchy: small plates (10-20) for fine boundary detail + super-plate clusters (3-5) for continent-scale structure. Reference blends 5% small-plate + 95% super-plate influence.
- R6. Each plate: continental/oceanic type, Euler pole velocity, density. Collision stress from velocity × density contrast at boundaries.

**Distance Fields (global BFS)**

- R7. BFS distance-to-boundary computed globally on the sphere grid (not per-face). This is the key difference from our failed JFA approach.
- R8. Multiple distance fields: (a) distance to any plate boundary, (b) distance to coast (continental-oceanic boundary), (c) distance to convergent boundary. Each enables different terrain features.
- R9. Distance fields are smooth and continuous across the entire sphere — no face-boundary artifacts.

**Orogeny & Terrain (adapted from reference)**

- R10. Convergent boundaries: mountain ridges with stress-driven height. Asymmetric subduction profiles (steeper subducting side + trench, gentler overriding plateau).
- R11. Fold ridges: directional ridges parallel to plate motion within mountain zones.
- R12. Divergent boundaries: mid-ocean ridges (subtle) and continental rift valleys.
- R13. Continental shelves: distance-from-coast profile — shallow shelf, steep slope, abyssal plain.
- R14. Stress-driven roughness: terrain detail amplitude scales with collision stress. Craggy orogens, smooth cratons.
- R15. Noise detail layered on top: fBm + ridged multifractal for texture, domain warping for natural shapes.

**Integration**

- R16. Output: cubemap heightmap (same R16Float format) for the existing fragment shader pipeline. The sphere grid is an intermediate representation; final output is sampled back to cubemap.
- R17. All existing rendering (biomes, atmosphere, clouds, ocean, export) continues to work unchanged.
- R18. Preview performance target: <3 seconds for full generation (BFS is more expensive than single-pass noise).
- R19. Existing UI parameters (water_loss, climate_moisture, seed, mountain_scale, etc.) continue to work.

## Success Criteria

- Terrain shows recognizable tectonic features: linear mountain chains at convergent boundaries, rift valleys, mid-ocean ridges, smooth continental interiors, continental shelves
- No visible plate boundary artifacts (no Voronoi edges, no soccer ball, no noodle ridges)
- Quality approaches the reference implementation (planet_heightmap_generation)
- Different seeds produce geologically plausible, visually distinct planets
- Existing biome/climate/export pipeline works unchanged on the new terrain

## Scope Boundaries

- Not a 1:1 port — adapt the reference's algorithm principles, not its JS code structure
- Not implementing the reference's ocean current simulation or climate model (we have our own)
- Not changing the fragment shader biome/climate pipeline
- Not implementing plate motion animation (static snapshot)
- Preview uses single-resolution grid (not progressive LOD)

## Key Decisions

- **Global sphere grid over cubemap for terrain computation**: The cubemap per-face approach can't do cross-face BFS. A single icosphere or HEALPix grid enables global operations. The result is sampled back to cubemap for rendering.
- **CPU BFS + GPU noise detail**: BFS is inherently sequential and hard to parallelize on GPU. Run BFS on CPU (fast enough for 1-2M vertices), then use GPU compute for noise detail and the final heightmap. Hybrid CPU/GPU pipeline.
- **Adapt, don't clone**: Port the algorithmic principles (multi-scale plates, BFS distance, stress orogeny, asymmetric profiles) but implement in idiomatic Rust with our existing infrastructure.

## Outstanding Questions

### Resolve Before Planning

- [Affects R1][Architecture] Which sphere grid representation? Options: icosphere (easy subdivision, good neighbor access), HEALPix (equal-area, used in astronomy), Fibonacci sphere (easy to generate, hard neighbor lookup). The choice affects BFS implementation complexity.

### Deferred to Planning

- [Affects R5][Needs research] How exactly does the reference implement super-plate clustering? Need to study the coarse-plates.js and super-plates.js files in detail.
- [Affects R7][Technical] BFS performance on 1-2M vertices — profile to verify <1 second. May need optimization (priority queue, parallel BFS).
- [Affects R16][Technical] Sampling from sphere grid to cubemap — interpolation method (nearest, barycentric for icosphere triangles).
- [Affects R18][Technical] Memory budget for sphere grid + distance fields + plate data at 2M vertices.

## Next Steps

-> This is a multi-session project. Start with `/ce:plan` to break into phases:
1. Sphere grid infrastructure (representation, neighbor lookup, cubemap sampling)
2. Plate simulation (Voronoi, BFS, super-plates)
3. Orogen terrain (stress, mountains, shelves, roughness)
4. Integration + parameter tuning
