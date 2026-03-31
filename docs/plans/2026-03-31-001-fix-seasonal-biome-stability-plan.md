---
title: "fix: Stabilize biome types across seasons — only colors should shift, not biome classification"
type: fix
status: active
date: 2026-03-31
---

# Fix Seasonal Biome Stability

## Overview

Biome classification currently uses seasonally-varying temperature and moisture, causing forests to visually become deserts when the season slider moves to winter. Fix so biome *type* is stable (determined by mean annual climate) while only the *appearance* shifts subtly with season.

## Problem Frame

`gradient_color()` in `preview_cubemap.wgsl` receives temperature and moisture that include full seasonal shift. The `t_cold`/`t_hot`/`moist_t` interpolation weights — which select between biome anchor colors (desert vs forest vs tundra) — swing wildly between summer and winter. A 20°C temperate forest pixel drops to 5°C in winter, shifting `t_hot` from ~0.6 to ~0.0, producing steppe/desert colors instead of forest.

On top of that, the winter color modulation (lines 260-270) adds `vec3(0.12, -0.04, -0.06) * (1-season) * green_amount * 3.0`, which is too aggressive — it pushes green forests toward tan/beige.

In reality, biome classification (Whittaker/Köppen) uses **mean annual** temperature and precipitation. Seasonal variation only changes the *appearance* — deciduous trees lose leaves (subtle brown shift), grasslands go dormant (slight yellowing) — but a temperate forest doesn't become a desert in winter.

## Requirements Trace

- R1. Biome anchor color selection must use mean annual temperature and moisture, not seasonal values
- R2. Seasonal color modulation must be subtle — slight autumn/winter browning for vegetated areas, not a full biome shift
- R3. Cold regions should still get whiter in winter (snow effect), but proportionally
- R4. Season slider at 0.5 (equinox) should produce identical results to current behavior
- R5. Other season-dependent features (ocean ice, snow/ice overlay) should continue using seasonal temperature

## Scope Boundaries

- NOT changing the temperature/moisture physics model — seasonal thermal shifts are physically correct
- NOT adding new biome types or changing the anchor color palette
- NOT modifying the export pipeline shaders
- NOT changing `preview.wgsl` (legacy shader)
- NOT adding seasonal variation to ocean colors

## Key Technical Decisions

- **Parameterize temperature/moisture functions with season**: Rather than duplicating the functions, refactor `compute_temperature` and `compute_moisture` to accept a `season` parameter. Call with `0.5` for mean annual values and `uniforms.season` for seasonal values. This avoids code duplication while keeping the physics model intact.

- **Pass both mean and seasonal values to gradient_color**: The function needs mean values for biome classification and seasonal values for modulation. Adding parameters is cleaner than computing the seasonal offset inside the function.

- **Reduce winter modulation intensity**: The current `green_amount * 3.0` multiplier is too aggressive. Scale it down to produce subtle autumn/winter tones rather than desert-like colors. Use the temperature *difference* (seasonal - mean) to drive modulation intensity rather than raw `(1 - season)`.

## Implementation Units

- [ ] **Unit 1: Parameterize temperature and moisture with season**

  **Goal:** Refactor `compute_temperature` and `compute_moisture` to accept an explicit season parameter instead of reading `uniforms.season` directly.

  **Requirements:** R1, R4

  **Dependencies:** None

  **Files:**
  - Modify: `src/shaders/preview_cubemap.wgsl`

  **Approach:**
  - Rename `compute_temperature` → internal helper that takes `season: f32` parameter
  - The `season_angle = (season - 0.5) * 2.0` line uses the parameter instead of `uniforms.season`
  - Same pattern for `compute_moisture` — the `season_angle` on line 175 uses the parameter
  - All existing call sites pass `uniforms.season` to preserve current behavior (no functional change in this unit)

  **Patterns to follow:**
  - Current function signatures at `preview_cubemap.wgsl:103` and `preview_cubemap.wgsl:169`

  **Test scenarios:**
  - Happy path: Build succeeds, visual output identical to before (pure refactor)
  - Edge case: season=0.5 passed to both parameterized functions produces same result as before

  **Verification:** `cargo build` succeeds. Visual output at all season values is unchanged.

- [ ] **Unit 2: Use mean annual values for biome classification in gradient_color**

  **Goal:** Separate biome anchor selection (mean annual climate) from seasonal color modulation.

  **Requirements:** R1, R2, R3, R4

  **Dependencies:** Unit 1

  **Files:**
  - Modify: `src/shaders/preview_cubemap.wgsl`

  **Approach:**
  - Change `gradient_color` signature to accept mean and seasonal temps/moisture: `gradient_color(mean_temp: f32, mean_moisture: f32, seasonal_temp: f32, variation: f32)`
  - Use `mean_temp` for `t_cold`/`t_hot` interpolation weights and `mean_moisture` for `moist_t` — these determine which biome anchor colors are blended
  - Replace the winter modulation block (lines 260-270):
    - Use `(seasonal_temp - mean_temp)` as the seasonal deviation signal instead of raw `(1 - season)`
    - When deviation is negative (winter): subtle golden-brown shift on vegetated areas, scaled down from current 3.0 multiplier
    - When deviation is positive (summer): optionally slightly more vibrant greens
    - Cold winter whitening (lines 268-272): use `seasonal_temp` (not mean) since snow cover IS seasonal
  - In `fs_main`, at the land rendering section (~line 407):
    - Compute `mean_temp = compute_temperature(rotated, height, 0.5)`
    - Compute `mean_moisture = compute_moisture(rotated, height, 0.5)`
    - Compute `seasonal_temp = compute_temperature(rotated, height, uniforms.season)`
    - Pass all to `gradient_color(mean_temp, mean_moisture, seasonal_temp, color_var)`
  - Other call sites that use seasonal temp for ice/snow/roughness: keep using `uniforms.season` (R5)

  **Patterns to follow:**
  - Current `gradient_color` at `preview_cubemap.wgsl:232`
  - Current biome anchor interpolation at lines 240-258
  - Current winter modulation at lines 260-272

  **Test scenarios:**
  - Happy path: Forest at lat 45° stays green in both summer and winter, with subtle browning in winter
  - Happy path: Desert at lat 25° stays desert-colored year-round (no greening in summer)
  - Happy path: season=0.5 (equinox) produces identical output to pre-fix behavior
  - Edge case: Tropical rainforest (near equator) shows minimal seasonal change
  - Edge case: High-latitude tundra gets whiter in winter (snow) but doesn't turn green in summer
  - Edge case: Biome boundary pixels — a pixel right at the forest/steppe boundary should NOT flip between biomes with season

  **Verification:** Rotating the season slider produces subtle color shifts (autumn tones, winter frost) without changing the underlying biome type. Forests stay forests, deserts stay deserts.

- [ ] **Unit 3: Update debug views that use seasonal temp/moisture**

  **Goal:** Ensure debug view modes (temperature, moisture, biome maps) still work correctly after the refactor.

  **Requirements:** R4

  **Dependencies:** Unit 2

  **Files:**
  - Modify: `src/shaders/preview_cubemap.wgsl`

  **Approach:**
  - Check debug view code (view_mode > 0, around line 448+) for calls to `compute_temperature`/`compute_moisture`
  - Debug views showing temperature/moisture should show SEASONAL values (what the planet experiences now), not mean annual
  - Ensure the updated function signatures are called correctly in debug paths

  **Patterns to follow:**
  - Debug view block at `preview_cubemap.wgsl:448+`

  **Test scenarios:**
  - Happy path: Temperature debug view still shows seasonal variation (hotter in summer hemisphere)
  - Happy path: Moisture debug view still shows seasonal ITCZ shift

  **Verification:** All debug view modes render without errors. Temperature/moisture views show seasonal variation as before.

## System-Wide Impact

- **Interaction graph:** Only `preview_cubemap.wgsl` is modified. No Rust code changes needed — the uniform struct is unchanged.
- **Error propagation:** None — shader-only change, worst case is visual artifact.
- **Unchanged invariants:** Ocean ice, snow overlay, altitude zonation, roughness computation all continue using seasonal temperature (R5). Export pipeline shaders are unaffected.

## Risks & Dependencies

| Risk | Mitigation |
|------|------------|
| Computing temp/moisture twice (mean + seasonal) doubles cost | These are arithmetic-only (no texture samples). The cost is negligible vs the texture-heavy parts of the shader |
| Mean annual values at equinox (season=0.5) don't match current output | By definition, season=0.5 IS the equinox — `season_angle = 0`, `sub_solar_lat = 0`. Output will be identical (R4) |
| Winter modulation too subtle after fix | Tune the multiplier during implementation. Start conservative (gentler shift), can increase if needed |

## Sources & References

- Biome classification: `gradient_color()` at `preview_cubemap.wgsl:232-278`
- Temperature model: `compute_temperature()` at `preview_cubemap.wgsl:103-139`
- Moisture model: `compute_moisture()` at `preview_cubemap.wgsl:169-230`
- Season slider: `app.rs:354`
- Debug views: `preview_cubemap.wgsl:448+`
