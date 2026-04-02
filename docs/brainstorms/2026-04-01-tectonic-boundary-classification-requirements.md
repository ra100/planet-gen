---
date: 2026-04-01
topic: tectonic-boundary-classification
---

# Tectonic Boundary Classification

## Problem Frame

Currently all tectonic plate boundaries produce the same terrain feature: mountains. In reality, boundary type determines terrain shape — subduction creates trenches + volcanic arcs, rifts create wide valleys, transform faults create offset ridges. This makes planet terrain look monotonous at boundaries regardless of geological context.

## Requirements

**Boundary Classification**

- R1. Each plate boundary segment is classified as convergent, divergent, or transform based on relative plate velocity vectors
- R2. Plate velocity vectors are physics-derived: magnitude from rotation period + mass (Coriolis/mantle drag approximation), direction pseudo-random from seed

**Terrain Shape by Boundary Type + Plate Type**

Full 6-combination matrix:

- R3. Ocean-ocean convergent → island arc chain + deep ocean trench on subducting side
- R4. Ocean-continent convergent → coastal mountain range (Andes-type) + ocean trench
- R5. Continent-continent convergent → broad highland/plateau (Himalayas-type), no trench
- R6. Divergent (any) → rift valley with slight volcanic ridge, widening gap
- R7. Transform (any) → lateral offset of existing features, no mountains, slight valley

**Integration**

- R8. UI dropdown "Tectonics Mode" in Advanced Tweaks: Quick (current) / Classified (new). Quick preserves current behavior exactly
- R9. Classified mode produces visually distinct terrain at different boundary types — not all boundaries look the same

## Success Criteria

- Planets generated in Classified mode show visible trenches at subduction zones, rifts at divergent boundaries, and no mountains at transform faults
- Quick mode is identical to current behavior (regression-free)
- Performance: Classified mode adds < 50ms to plate generation at 768px
- Different seeds produce geologically varied boundary configurations

## Scope Boundaries

- NOT implementing plate motion over time (Phase 8b)
- NOT implementing mantle convection (Phase 8c)
- NOT changing the Voronoi plate generation algorithm — boundaries are classified AFTER plates are generated
- NOT adding volcanic hotspot chains (future enhancement)

## Key Decisions

- **Classify after generation, not during**: The current Voronoi + noise warp pipeline stays unchanged. Boundary classification is a post-processing step on the existing plate data. This preserves Quick mode and minimizes risk.
- **Physics-derived velocity**: Plate speed scales with rotation period (faster rotation → more active tectonics via Coriolis) and planet mass (higher mass → stronger mantle convection). Direction is pseudo-random from seed per plate.
- **Full boundary matrix**: All 6 ocean/continent × convergent/divergent/transform combinations produce distinct terrain. This is the highest-impact visual improvement for the implementation cost.

## Dependencies / Assumptions

- Current `PlateGenParams` and `generate_plates()` in `src/plates.rs` provide plate positions, types (continental/oceanic), and boundary distances
- The `plates.wgsl` compute shader generates height from boundary proximity — this needs to branch by boundary type
- Plate velocity vectors need to be generated on CPU and passed to the GPU as additional plate data

## Outstanding Questions

### Deferred to Planning

- [Affects R2][Needs research] What simplified formula relates rotation period + mass to plate velocity magnitude? Lookup Rayleigh number → convection vigor → plate speed scaling.
- [Affects R3-R7][Technical] How to pass boundary type to the GPU compute shader? Options: encode in existing plate data buffer, or add a new boundary type buffer.
- [Affects R3][Technical] How to determine which side of a convergent boundary is subducting? The denser (oceanic) plate dives under the lighter (continental) one. For ocean-ocean, the older/denser one subducts.

## Next Steps

→ `/ce:plan` for structured implementation planning
