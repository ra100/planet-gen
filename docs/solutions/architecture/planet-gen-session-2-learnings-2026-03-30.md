---
title: "Planet Gen Session 2: Tectonic Plates, Climate, Erosion"
category: architecture
date: 2026-03-30
tags: [planet-gen, tectonic-plates, climate, erosion, biomes, gpu-compute, wgsl, procedural-generation]
module: terrain_compute, preview
component: plates.wgsl, erosion.wgsl, preview_cubemap.wgsl
problem_type: feature_development
severity: medium
---

# Planet Gen Session 2: Tectonic Plates, Climate Systems, Erosion

## Problem

The planet generator produced terrain from layered noise that looked procedural — no geological structure, straight biome bands, no erosion signatures. Needed to evolve from "noise ball" to "geologically plausible planet" suitable for VFX.

## What Was Built

### Tectonic Plate System
- **Voronoi plates on sphere**: Fibonacci sphere distribution for plate centers, seed-based perturbation, continental/oceanic classification
- **Compute pipeline**: GPU compute shader (plates.wgsl) generates heightmap from plate structure — convergent boundaries → mountains, divergent → rifts, oceanic-oceanic → island arcs
- **Separated land/ocean mask from plates**: Key architectural insight — plates drive tectonics (where mountains go), noise drives coastlines (bays, peninsulas, inland seas). This broke the "convex Voronoi cell = continent" problem
- **Regional geology layer**: Mid-scale highlands, basins, plateaus within continents; seamounts in ocean
- **Noise-biased Voronoi**: Per-plate noise field perturbs distance metric for concave coastline shapes

### Climate System
- **Hadley cell atmospheric circulation**: Three-cell model (ITCZ, subtropical high, polar front) for latitude-based moisture patterns — deserts at ~30°
- **Cubemap-based rain shadow**: Preview shader samples height cubemap at upwind positions to detect mountains blocking moisture
- **Cubemap-based continentality**: Sample 4 neighbors to determine coast vs interior moisture gradient
- **Non-linear temperature gradient**: 50°C range with ocean heat transport effect (flatter mid-latitudes)
- **Ocean-scaled moisture**: Moisture scales by ocean_fraction — dry worlds (water_loss=1.0) have ~5% base moisture
- **Proper tilt model**: Axis rotation (y*cos + z*sin) instead of sin(longitude)*tilt — eliminates V-shaped artifacts

### Biome Improvements
- **Cold desert biome**: Mars-like rust/red-brown for cold arid worlds (temp < 5°C, low moisture)
- **Moisture-gated ice caps**: Ice only where moisture > threshold — dry cold worlds show desert, not ice
- **Seasonal color shifts**: Season slider (0=winter, 1=summer) — deciduous brown in winter, grasslands golden, tundra white
- **Altitude zonation**: forest → alpine → rock → snow with latitude-dependent snow line

### Hydraulic Erosion
- **Stream-power erosion**: E = K × sqrt(drainage) × slope — drainage accumulation creates differential erosion
- **Flow accumulation pass**: 8 sub-iterations propagate water downhill before each erosion step
- **Thermal erosion**: Talus slope collapse prevents impossible vertical cliffs
- **Erosion slider**: 0-50 iterations in Visual Overrides
- **Resolution limitation**: At 512x512/face (~65km/pixel), river channels are sub-pixel. Visible at 2K+ resolution

### Tools & UI
- **7 debug view modes**: Normal, Height, Temperature, Moisture, Biome, Ocean/Ice, Plates
- **Parameter sweep tool**: `cargo run --bin sweep` generates 30 PNGs across 6 presets × 5 seeds
- **Erosion comparison tool**: `cargo run --bin erosion_compare` renders 0/25/50 iteration comparison
- **Continental scale slider**: Controls plate count (fewer = bigger continents)
- **Water loss slider**: Simulates atmospheric water escape (Mars-like dry worlds)
- **Configurable preview resolution**: 256-2K

## Key Learnings

1. **Separate structure from appearance**: Voronoi cells are great for partitioning space (plates) but terrible as visual boundaries (coastlines). Use one system for structure, noise for appearance.

2. **South pole bias in Fibonacci sphere**: Assigning properties by index order creates latitude correlation. Use seed-based hash scoring instead.

3. **Moisture needs water source**: Hadley cell circulation patterns are meaningless without ocean evaporation. Scale all moisture by ocean_fraction at the source.

4. **Temperature tilt model matters**: `sin(longitude) * tilt` creates converging V-artifacts. Proper axis rotation `y*cos(tilt) + z*sin(tilt)` is correct.

5. **Erosion resolution dependency**: Stream-power erosion with drainage accumulation is correct physics, but river channels are sub-pixel at planetary preview resolution. The infrastructure is in place for when higher-resolution tiled generation is implemented.

6. **Cold desert is not ice**: Whittaker table maps cold+dry to ice biome, but real cold deserts (Mars) are arid, not frozen. Added cold+dry → desert override before the Whittaker lookup.

## Prevention / Best Practices

- When building Voronoi-based systems: always ask if cell boundaries should be visual boundaries or just structural influence zones
- For any latitude-dependent calculation with tilt: use axis rotation, not sinusoidal offset
- Scale physical quantities (moisture, erosion) by their source availability (ocean, atmosphere)
- Test with parameter sweeps across presets before declaring a feature "done"
- Debug view modes are invaluable — add them early, they pay for themselves in iteration speed
