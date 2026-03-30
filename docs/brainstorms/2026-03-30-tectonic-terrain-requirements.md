---
date: 2026-03-30
topic: tectonic-terrain
---

# Tectonic Plate-Driven Terrain Generation

## Problem Frame

The current heightmap is built from layered noise (fBm + domain warping). While the Hadley cell climate system and biome pipeline are physics-driven, the terrain itself has no geological structure — continents are just noise blobs, mountains appear randomly, and there are no recognizable geological formations (mountain ranges along plate boundaries, volcanic island arcs, mid-ocean ridges, continental shelves). This makes the planet look procedurally generated rather than geologically plausible.

The research (section 7.3) explicitly recommends: Voronoi cells for continental boundaries → mountain ridges at plate boundaries → fBm detail → erosion. We're doing only the fBm detail step.

## Requirements

**Tectonic Plate Generation**

- R1. Generate N tectonic plates as Voronoi cells on the sphere surface (N derived from planet size: small planets ~4-6 plates, Earth-like ~8-14, large ~15-20)
- R2. Classify each plate as continental (thick, buoyant) or oceanic (thin, dense) based on the planet's ocean fraction
- R3. Each plate boundary is classified by relative motion: convergent (collision), divergent (spreading), or transform (sliding). Motion vectors assigned per plate from seed.

**Boundary-Driven Terrain**

- R4. Convergent continental-continental boundaries produce mountain ranges (Himalayas, Alps): elevated ridge along the boundary with foothills tapering away
- R5. Convergent oceanic-continental boundaries produce volcanic mountain chains (Andes) + ocean trench on the oceanic side
- R6. Divergent boundaries produce mid-ocean ridges (underwater) or rift valleys (on land, like East Africa)
- R7. Transform boundaries produce fault lines with offset terrain (San Andreas style)

**Continental Structure**

- R8. Continental plates have elevated interiors (continental shelf) with gradual slopes to the ocean basin
- R9. Continental margins have characteristic shapes: passive margins (smooth coastal shelves like US East Coast) vs active margins (steep, mountainous like US West Coast based on boundary type)
- R10. Island arcs form at oceanic-oceanic convergent boundaries (Japan, Philippines pattern)

**Geological Features**

- R11. Volcanic hotspots: 1-3 per planet, independent of plate boundaries, producing shield volcano chains (Hawaii pattern)
- R12. Continental interiors have lower, smoother terrain (plains, plateaus) distinct from boundary mountains
- R13. fBm detail noise is layered on top of the plate-driven structure, not replacing it — the plate structure provides the large-scale geography, noise provides the local texture

**Architecture**

- R14. Terrain generation moves to a GPU compute pipeline: plates are generated once per parameter change, heightmap stored in a cubemap texture
- R15. The preview fragment shader samples the pre-computed cubemap instead of computing noise per-pixel (reverting to cubemap architecture, but with plate-driven content instead of raw noise)
- R16. Biome pipeline (temperature, moisture, Whittaker lookup) continues to run in the fragment shader, sampling height from the cubemap
- R17. Preview resolution for the compute-generated cubemap: 512x512 per face (enough for visual quality, fast to generate)

## Success Criteria

- Earth-like parameters produce a planet with recognizable geological structure: continents with mountain ranges along edges, ocean basins, island arcs
- Different seeds produce different plate configurations but all look geologically plausible
- Mountains cluster along plate boundaries, not randomly across the surface
- Continental interiors are relatively flat compared to boundary zones
- The debug height view shows clear plate-boundary-driven structure, not noise blobs

## Scope Boundaries

- Not simulating plate motion over time (no continental drift animation) — just the "current snapshot" configuration
- Not simulating mantle convection — plate motion vectors are random-but-coherent from seed
- Hydraulic erosion remains deferred (v1.1) — this focuses on the large-scale structure
- Not changing the biome/climate pipeline — it continues to work on top of whatever heightmap is generated
- Not adding new UI parameters initially — plate count and types derive from existing params (mass, ocean fraction, tectonics_factor)

## Key Decisions

- **Voronoi on sphere for plates**: Standard approach used by SpaceEngine, Outerra. Voronoi cells naturally tile the sphere without poles or seams. Edge warping makes boundaries look natural.
- **Compute pipeline replaces fragment-shader terrain**: Fragment shader per-pixel noise was an interim approach. Compute pipeline generates a cubemap heightmap once, then the fragment shader samples it. This enables multi-pass operations (plate generation → boundary processing → detail addition) that per-pixel evaluation can't do.
- **Continental vs oceanic plate classification**: Use ocean_fraction to determine the ratio. ~70% ocean fraction → ~70% of plate area is oceanic. This naturally produces the right land/ocean balance.
- **Boundary classification from plate motion**: Each plate gets a random velocity vector. The relative velocity between adjacent plates at each boundary point determines boundary type (converging/diverging/transform). This is simple but produces realistic patterns.

## Outstanding Questions

### Deferred to Planning

- [Affects R1][Technical] Voronoi on a sphere: use spherical Voronoi directly, or project onto cube faces and compute Voronoi per face with stitching? Spherical Voronoi is more correct but harder to implement in WGSL.
- [Affects R14][Technical] Cubemap resolution 512x512 per face — is this enough detail? May need adaptive resolution or multi-level generation.
- [Affects R4-R7][Needs research] What height profiles should boundary types use? Need specific elevation functions for convergent/divergent/transform boundaries.
- [Affects R11][Technical] How to place hotspot volcanoes independent of Voronoi structure — separate noise-based placement?

## Next Steps

→ `/ce:plan` for structured implementation planning
