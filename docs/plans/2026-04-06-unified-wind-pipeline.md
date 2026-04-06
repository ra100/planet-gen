---
title: Unified Wind Pipeline — Replace analytical wind with cubemap wind
type: feature
status: planned
date: 2026-04-06
origin: Cloud simplification session — identified dual wind model inconsistency
---

# Unified Wind Pipeline

## Problem

The preview shader currently has **two incompatible wind models**:

1. **Analytical wind** (`wind_direction_vec` / `wind_direction_at`): Pure latitude-based Hadley/Ferrel/Polar cells with terrain deflection. Used for moisture rain shadows, orographic cloud lift/föhn, and ocean currents.

2. **Pressure-derived cubemap wind** (`sample_wind_field`): GPU-computed from pressure gradient + Coriolis + continentality + terrain + turbulence. Stored in RGBA16Float cubemap RGB channels. Currently only visible in debug view 18 — **not used for anything in the main render**.

This means:
- Wind effects on moisture/clouds use a simplified model that ignores pressure, continentality, and monsoon patterns
- The GPU spends ~160ms computing a pressure-based wind field that's only used for debug visualization
- Continentality is used (alpha channel), but the actual wind vectors (RGB) are wasted

## Goal

Replace all analytical wind usage in the preview shader with cubemap wind sampling. One wind model, one source of truth, physically consistent across all effects.

## Architecture

### Current data flow
```
WindFieldPipeline → RGBA16Float cubemap
  .rgb = pressure-derived wind vectors (UNUSED in render)
  .a   = continentality (used for cloud/moisture modulation)

Preview shader:
  compute_moisture() → wind_direction_vec() [analytical]
  compute_cloud_density() → wind_direction_vec() [analytical]
  compute_temperature() → wind_direction_vec() [analytical, for currents]
  debug view 14 → wind_direction_at() [analytical]
  debug view 18 → sample_wind_field() [cubemap]
```

### Target data flow
```
WindFieldPipeline → RGBA16Float cubemap
  .rgb = pressure-derived wind vectors (used EVERYWHERE)
  .a   = continentality (unchanged)

Preview shader:
  sample_wind_tangent(pos) → .rgb projected to tangent plane
  compute_moisture() → sample_wind_tangent() [cubemap]
  compute_cloud_density() → sample_wind_tangent() [cubemap]
  compute_temperature() → sample_wind_tangent() [cubemap, for currents]
  debug view 14 → sample_wind_tangent() [cubemap, replaces analytical]
  debug view 18 → removed (merged into view 14)
```

### Fallback

When `cloud_advection = 0` (Wind Effects toggle OFF), no cubemap is available. The analytical wind functions remain as the fallback path, gated by `if (uniforms.cloud_advection < 0.5) { use analytical } else { use cubemap }`.

## Risks

1. **Cubemap resolution**: Wind cubemap is half-resolution (384px for 768px preview). Analytical wind is per-pixel. Cloud detail may look blockier. Mitigation: the cubemap has hardware bilinear filtering which smooths well.

2. **Face seam artifacts**: Despite bilinear sampling in the compute shader, small discontinuities may persist at cubemap face boundaries. Mitigation: the turbulence perturbation already added in the compute shader helps mask seams. Can add an explicit seam-smoothing pass if needed.

3. **Performance**: `sample_wind_field()` is 1 cubemap sample vs `wind_direction_vec()` which is pure math (no texture reads). Over ocean (where currents sample wind), this adds cubemap pressure. But the analytical wind also calls `textureSample` for terrain deflection, so the cost difference is small.

4. **Ocean currents quality**: The current ocean current model uses east/west land detection, not wind direction directly. The cubemap wind would give actual pressure-derived flow direction, but the current system doesn't model gyres — it just infers warm/cold currents from nearby land position. This task doesn't change the current model, only the wind source for rain shadows and cloud shaping.

## Non-goals

- Changing the ocean current model to use wind-driven gyre simulation
- Adding new wind-related features (jet streams, local wind systems)
- Changing the wind field compute shader itself
