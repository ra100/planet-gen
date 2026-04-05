---
title: Pressure-Based Wind Model for Cloud Advection
type: plan
status: draft
date: 2026-04-05
origin: Analysis of raguilar011095/planet_heightmap_generation wind.js
---

# Pressure-Based Wind Model

## Problem

Current wind model is a pure function of latitude (trade/westerly/polar smooth_steps). This means:
- Wind has NO longitude variation — clouds just slide along latitude lines
- No terrain-induced pressure effects (monsoons, thermal lows/highs)
- No land/ocean differential heating
- Cloud advection produces no visible "movement" — only slight density modulation

## Reference

The `planet_heightmap_generation` project ([wind.js](https://github.com/raguilar011095/planet_heightmap_generation/blob/main/js/wind.js)) implements a much more physical wind model:

1. **ITCZ varies by longitude** — samples land fraction, pulls ITCZ poleward over continents
2. **Pressure field** from 7 terms: ITCZ low, subtropical highs (weakened over land), subpolar lows, polar highs, continental thermal modifier, elevation, noise
3. **Wind from pressure gradient** — gradient computation on mesh + Coriolis deflection (70° geostrophic) + surface friction (20° toward low)
4. **Continentality via BFS** — true coast distance through land, maps to [0,1] over 2000km

Their wind varies in both latitude AND longitude, creating realistic patterns: monsoons, sea breezes, continental wind shadows.

## Architecture: Cubemap Adaptation

Their approach uses a mesh with adjacency (Voronoi). We use cubemaps. The adaptation:

### Compute Shader Pipeline (3 passes)

All shaders operate per-cubemap-face at the cloud advection resolution (256-512px).

#### Pass 1: Continentality (BFS on GPU)
- **Input**: height cubemap (6 faces)
- **Output**: continentality cubemap (R16Float, 6 faces)
- Each texel: 0.0 = ocean/coast, 1.0 = deep continental interior
- GPU BFS: iterative wavefront expansion from ocean cells through land
- Cross-face: sample adjacent faces for border texels
- Smooth: 2-3 passes of box blur to remove BFS stepping artifacts

#### Pass 2: Pressure Field
- **Input**: height cubemap, continentality cubemap, uniforms (season, tilt, seed)
- **Output**: pressure cubemap (R16Float, 6 faces)
- Per-texel pressure from 7 physical terms:
  1. ITCZ low (longitude-varying via noise-perturbed thermal equator)
  2. Subtropical highs at ±30° (weakened by continentality)
  3. Subpolar lows at ±60°
  4. Polar highs at ±85°
  5. Continental thermal modifier (summer low / winter high, scaled by continentality)
  6. Elevation reduction (-3 hPa per km)
  7. Noise perturbation (±2 hPa from low-freq noise)
- Smooth: 2 passes box blur (~75km equivalent)

#### Pass 3: Pressure Gradient → Wind
- **Input**: pressure cubemap
- **Output**: wind cubemap (RG16Float, 6 faces — east/north components)
- Finite differences for pressure gradient in tangent-plane east/north directions
- Coriolis deflection: 70° × smoothstep(0, 5°, |latitude|)
- Surface friction: rotate 20° back toward low pressure, 0.6× speed reduction
- Cross-face: sample adjacent faces for border gradient computation

### Integration with Existing Advection

The wind cubemap replaces the inline `wind_at()` function in `cloud_advect.wgsl`:
- Instead of computing wind from latitude smooth_steps, sample the precomputed wind cubemap
- Wind field is computed once per terrain generation, reused across all advection steps
- The existing semi-Lagrangian advection, condensation, and rain shadow logic remain unchanged

### ITCZ Longitude Variation (Simplified)

Instead of the full periodic spline approach, use noise-modulated thermal equator:
- Base ITCZ at latitude offset from season/tilt
- Add longitude-dependent perturbation from `snoise(sphere_pos * 1.5)` scaled by land fraction
- Land pulls ITCZ poleward (hotter surface → stronger thermal low)
- This naturally creates monsoon-like effects without explicit BFS

## Tasks

### Phase 5.18: Pressure-Based Wind Model

| # | Task | DoD | Depends |
|---|------|-----|---------|
| 5.18.1 | GPU continentality: compute shader BFS from ocean cells on cubemap | Continentality cubemap 0.0=coast, 1.0=interior; debug view shows smooth gradient | - |
| 5.18.2 | Pressure field shader: 7-term pressure per texel | Pressure cubemap shows ITCZ low, subtropical highs, continental thermal modifier; debug view shows blue=low, red=high | 5.18.1 |
| 5.18.3 | Pressure gradient → wind shader: finite differences + Coriolis + friction | Wind cubemap shows realistic flow; trade winds, westerlies, monsoon deflection visible | 5.18.2 |
| 5.18.4 | Wire wind cubemap into cloud advection | Advection uses precomputed wind instead of inline latitude bands | 5.18.3 |
| 5.18.5 | Make advected clouds the PRIMARY density source | Advected cubemap drives cloud shape, per-pixel noise adds only fine detail; toggle shows clear difference | 5.18.4 |
| 5.18.6 | Add pressure + continentality debug views | View mode dropdown: Pressure, Continentality alongside existing Wind | 5.18.2 |
| 5.18.7 | Tune and validate: compare with Earth-like patterns | Monsoon-like poleward shift over continents, maritime westerlies, polar vortex visible | 5.18.5 |

## Performance Budget

At 256px per face:
- Continentality BFS: ~50 iterations × 6 faces × 256² = ~10ms (GPU)
- Pressure field: 6 faces × 256² = ~2ms
- Gradient → wind: 6 faces × 256² = ~2ms
- Total: ~15ms added to terrain generation (currently ~120ms for advection)

## Risks

1. **Cross-face sampling**: Pressure gradient at cubemap face edges needs to sample adjacent faces. Either pack all 6 faces into one buffer (like existing advection) or use cubemap texture sampling.
2. **BFS on GPU**: Standard BFS is sequential. GPU adaptation: iterative wavefront expansion where each dispatch processes all cells at distance d, incrementing d each iteration. ~50 iterations for 256px.
3. **ITCZ complexity**: Full longitude-varying ITCZ requires sampling land fraction at many points. Simplified approach: use noise + continentality as proxy.
