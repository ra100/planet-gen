---
title: Terrain-Aware Wind — Reintroduce continent/height effects safely
type: feature
status: planned
date: 2026-04-07
origin: Session learnings — terrain effects caused coastline ghosting, need safer approach
---

# Terrain-Aware Wind

## Problem

The wind field is currently pure latitude + noise — no terrain influence at all. Continents and mountains don't affect wind patterns. We removed terrain effects because every approach we tried (pressure gradients, terrain deflection, continentality modulation, monsoon) created visible coastline-tracing artifacts.

## Key Lesson

**What causes ghosting:** Any effect that reads terrain height at full cubemap resolution creates a continent-shaped signal. Even "blurred" reads are too sharp at coastlines where height jumps from ocean to land in 1 texel.

**What doesn't cause ghosting:** The `lat_deg + wobble` mechanism. Noise shifts the effective latitude by ±8°, creating wavy cell boundaries. No terrain dependency, no artifacts.

## Strategy: Terrain Influence via Wobble

Instead of modifying wind VECTORS from terrain, modify the WOBBLE from terrain. This means terrain shifts WHERE cell boundaries fall (like the real monsoon shifting the ITCZ), not HOW wind blows at a given point.

The wobble already exists: `lat_deg = abs(shifted_lat) / DEG + noise_wobble`. Adding terrain terms:

```
lat_deg = abs(shifted_lat) / DEG + noise_wobble + continental_wobble + elevation_wobble
```

### Continental wobble (monsoon)
Over large continents in summer, the Hadley cell extends poleward (ITCZ shifts toward the heated landmass). This is the Asian monsoon mechanism.

Source: `sample_src()` reads smoothed continentality (already available, 80 iterations of diffusion). High continentality = deep interior = stronger pull.

```wgsl
let continental_wobble = continentality * 5.0 * season_factor;
```

This shifts cell boundaries ~5° poleward over continent interiors in summer. Since continentality is heavily smoothed, the effect is broad and gradual — no coastline tracing.

**Why this won't ghost:** The continentality cubemap is at half-resolution (384px) with 80 iterations of diffusion. The gradient at coastlines spans ~6 texels. The wobble adds this smooth signal to the latitude, creating a gentle undulation — not a hard edge.

### Elevation wobble (orographic)
High mountain ranges (Himalayas, Andes) block atmospheric flow and can shift cell boundaries locally. This is modeled as an elevation-dependent latitude shift.

Source: `height_data[]` — but NOT at full resolution. Use a SMOOTHED height, either:
- Option A: Read height via `sample_height()` with bilinear interpolation (already implemented), then clamp to mountain-only threshold (>0.10 height units ≈ 3km)
- Option B: Add a "smoothed elevation" pass to the pipeline (mode 4) that diffuses height similar to continentality, creating a very low-resolution elevation field

Option A is simpler but still reads per-pixel height. Option B gives a properly blurred field but adds pipeline complexity.

**Recommendation: Option A with high threshold.** At 3km+ threshold, only major mountain ranges trigger the wobble. Flat coasts and low-elevation continents contribute zero.

```wgsl
let elevation = max(height_data[idx] - params.ocean_level, 0.0);
let mountain_wobble = smooth_step(0.10, 0.25, elevation) * 3.0;
```

This shifts cell boundaries ~3° around major mountain ranges. The `smooth_step(0.10, 0.25)` only activates above 3km, so no coastline signal.

### Wind speed from elevation
Separate from direction: mountains can accelerate wind (gap winds, venturi effect). This modifies wind SPEED, not direction, so it doesn't shift cell boundaries.

```wgsl
let speed_boost = 1.0 + smooth_step(0.08, 0.20, elevation) * 0.3; // up to 30% faster near mountains
wind_e *= speed_boost;
wind_n *= speed_boost;
```

## What NOT to do

Based on session learnings:
- ❌ Don't read height at coastlines (height jumps create edges)
- ❌ Don't use continentality as a direct wind vector modifier
- ❌ Don't compute finite differences of any terrain-derived quantity
- ❌ Don't use `if (height > ocean_level)` binary switches
- ✅ DO use terrain data to shift lat_deg (wobble mechanism)
- ✅ DO use high elevation thresholds (>3km) to isolate mountains from coasts
- ✅ DO use already-smoothed data (continentality cubemap, bilinear height)

## Risks

1. Continental wobble might be too subtle at 5° shift — could need 8-10° for visible effect
2. Elevation wobble at per-pixel resolution might still create faint mountain outlines (mitigated by 3km threshold)
3. Wind cubemap is at half-resolution (384px) so terrain detail is naturally low-pass filtered
