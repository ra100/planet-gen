---
title: "Tectonic Plate Terrain: Separating Land/Ocean Mask from Plate Structure"
category: architecture
date: 2026-03-30
tags: [terrain, tectonic-plates, voronoi, procedural-generation, gpu-compute, wgsl]
module: terrain_compute
component: plates.wgsl
problem_type: logic_error
severity: high
---

# Tectonic Plate Terrain: Separating Land/Ocean Mask from Plate Structure

## Problem

Procedurally generated planets looked like "noise over noise" — no recognizable geological structure. After implementing a Voronoi-based tectonic plate system, continents looked like convex polygonal cells because **continent shape was identical to plate shape**. Real continents have concave coastlines (Gulf of Mexico, Mediterranean), peninsulas, inland seas, and irregular shorelines that don't follow tectonic plate boundaries.

## Symptoms

- Continents shaped like convex Voronoi polygons
- No bays, gulfs, peninsulas, or inland seas
- Domain warping on Voronoi edges improved boundary irregularity but couldn't create concavity within a cell
- Plates appeared "squarish" regardless of warping strength
- South pole was systematically higher (continental plates assigned by index order which correlates with Fibonacci sphere latitude)

## What Didn't Work

1. **Increasing domain warping strength** (0.07 → 0.15 + 0.06 multi-octave): Made edges more wavy but didn't change the fundamental convex cell topology
2. **Noise-biased Voronoi distance** (per-plate noise perturbation): Shifted boundaries irregularly but each plate remained a single connected convex-ish region
3. **More plate centers**: Smaller cells still produce convex shapes, just more of them
4. **Fragment shader noise-only terrain** (pre-tectonic approach): No geological structure at all — pure fBm noise produces uniform "noisy islands" with no continent-scale features

## Solution

**Separate the land/ocean elevation mask from the plate tectonic structure.**

**Before (plate = continent):**
```
if (is_continental_plate) {
    height = +0.25;  // All continental plate area is land
} else {
    height = -0.35;  // All oceanic plate area is ocean
}
```

**After (plate biases, noise determines coastline):**
```
// Plate type biases elevation but doesn't determine it
let plate_bias = select(-0.25, 0.25, is_continental);

// Independent noise creates actual coastline shapes
let coastal_noise = domain_warped_multi_octave_noise(position);

// Combination: continental plates are MOSTLY land, but noise creates
// bays, gulfs, peninsulas, and inland seas
height = plate_bias + coastal_noise * 0.35;
```

Key architectural principles:
- **Plates define tectonic features**: where mountains form (convergent boundaries), where trenches appear (subduction zones), where rifts develop (divergent boundaries)
- **Noise defines geography**: coastline shapes, highland/lowland distribution within continents, island placement within ocean
- **The bias ensures correlation**: continental plates are mostly land, oceanic mostly ocean, but the coastline doesn't follow plate boundaries

Additional fixes applied during this iteration:
- **South pole bias**: Continental/oceanic plate assignment changed from index-order (correlated with Fibonacci latitude) to seed-based hash scoring (randomized across latitudes)
- **Continental scale slider**: Now controls plate count (fewer plates = larger continents) instead of just noise frequency
- **Three terrain layers**: Plates (continental structure) → regional geology (highlands, basins, plateaus) → fBm detail (local texture)
- **Cubemap projection**: Fixed cube_to_sphere mapping to match wgpu's standard cubemap face convention, eliminating visible square face boundaries

## Why This Works

Real continents don't follow tectonic plate boundaries. The Indian plate extends underwater far south of the Indian subcontinent. The North American plate includes both land and ocean floor. Coastlines are shaped by erosion, sediment deposition, sea level changes, and local geology — processes independent of plate boundaries.

By using plates for tectonic features (where the forces are) and noise for geography (where the land surface is), the generator produces:
- Concave coastlines (bays, gulfs) where noise dips below sea level on continental plates
- Peninsulas where noise pushes land into oceanic plate territory
- Island chains where noise peaks on oceanic plates breach the surface
- Inland seas where large negative noise patches exist on continental plates

## Prevention

- When building Voronoi-based procedural systems, always ask: "Should the cell identity determine the visual output, or just influence it?" Voronoi cells are great for partitioning space into regions with distinct properties, but using cell boundaries directly as visual boundaries produces geometric-looking results.
- For any system where the output should look natural/organic, add an independent noise layer that's *correlated with* but *not determined by* the structural partitioning.
- Test for systematic biases in point distribution algorithms (like Fibonacci sphere) — any index-order assignment of properties will correlate with the distribution pattern.
