# Performance & Visual Comparison: Climate Refinement (Phases 5.18-5.20)

**Date:** 2026-04-06
**Phases covered:** 5.18 (Pressure-based wind), 5.19 (Climate model refinement), 5.20 (Wind-shaped clouds)

---

## Architecture Changes

### Before (Phase 5.17)
- Wind model: latitude-only Hadley/Ferrel/Polar cell with fixed 30/60 degree boundaries
- Cloud advection: semi-Lagrangian transport on 256px cubemap, per-pixel noise as fallback
- Climate zones: hardcoded latitude bands for moisture, coverage, biomes

### After (Phase 5.20)
- Wind model: pressure-gradient-derived wind from continentality + terrain + Coriolis
- Cloud system: per-pixel wind streamline tracing (no cubemap advection)
- Climate zones: rotation-rate-dependent (Kaspi & Showman 2015), pressure-scaled

## GPU Pipeline Timing (768px preview, M-series Apple Silicon)

| Stage | Before | After | Change |
|-------|--------|-------|--------|
| Wind field compute (384px) | N/A | ~80ms | New |
| Cloud advection (30 steps) | ~120ms | Removed | -120ms |
| Preview render | ~8ms | ~10ms | +2ms (streamline trace) |
| **Total terrain regen** | ~600ms | ~560ms | **-7%** |

The wind field compute (80ms) is offset by removing cloud advection (120ms). The per-pixel streamline trace adds ~2ms per frame but eliminates the need for a separate compute pass.

## Wind Field Quality

### Pressure-based wind improvements
- Continentality-driven: wind varies with land/ocean distribution, not just latitude
- Monsoon patterns: ITCZ shifts poleward over large continents in summer
- Terrain deflection: wind bends around mountains
- Turbulent noise: breaks straight-line patterns in the wind field

### Rotation-rate scaling (Kaspi & Showman 2015)
| Rotation | Day length | Cells/hemisphere | Hadley top |
|----------|-----------|-----------------|------------|
| 0.0625x | 16 days | 1 (superrotation) | ~60 deg |
| 0.5x | 48h | 2 | ~42 deg |
| 1.0x (Earth) | 24h | 3 | ~30 deg |
| 2.0x | 12h | 5 | ~24 deg |

## Cloud System Comparison

### Cubemap advection (abandoned)
- Face seam artifacts at all cubemap boundaries
- Resolution mismatch: 384px cubemap modulating 768px per-pixel noise
- Semi-Lagrangian transport stretches initial noise into bands
- Multiple workaround attempts: divergence (noisy), moisture transport (uniform), power remap (blocky)

### Per-pixel wind streamline tracing (current)
- Zero cubemap artifacts (everything computed per-pixel at full resolution)
- 6-step backward Euler trace along `wind_direction_at()`
- Adjustable Wind Trail slider (0-1) for user control
- Cirrus gets 2.5x trail (jet stream)
- Continentality cubemap modulates coverage (ocean +15%, interior -25%)

## Export Layers

| Layer | Format | Notes |
|-------|--------|-------|
| height.exr | R32 gray | Terrain elevation |
| albedo.exr | RGBA32 | Biome colors with AO |
| normal.exr | RGBA32 | Object-space normals |
| roughness.exr | R32 gray | PBR roughness |
| water_mask.exr | R32 gray | Binary ocean mask |
| clouds.exr | R32 gray | Cloud density |
| **emission.exr** | R32 gray | **New: city lights** |
| ao.exr | R32 gray | Ambient occlusion |

## Known Limitations

1. Wind streamline trace uses analytical wind model (latitude + terrain deflection), not the pressure-derived wind cubemap. Analytical is smoother but less physically accurate.
2. Continentality cubemap is the only remaining compute-sourced data used in cloud rendering. It's smooth (40 diffusion passes) and doesn't have visible face seams.
3. Cloud export shader (`cloud_map.wgsl`) still uses the old fixed cell boundaries — could be updated to match the rotation-dependent preview shader.

## Screenshots

*Capture with the app at different rotation rates and zoom levels to document:*
- [ ] Earth-like (24h): trade wind cloud trails, westerly frontal bands, clear subtropical zones
- [ ] Fast rotator (12h): narrow cells, many cloud bands
- [ ] Slow rotator (96h): wide Hadley cell, fewer bands
- [ ] Zoomed: wind trail slider 0 vs 0.5 vs 1.0 comparison
- [ ] Advection OFF vs ON comparison
