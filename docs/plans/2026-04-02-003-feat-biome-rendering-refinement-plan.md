---
title: "feat: Biome rendering refinement — color variance, moisture balance, realistic snow"
type: feat
status: active
date: 2026-04-02
---

# Biome Rendering Refinement

## Overview

The biome rendering uses a 6-anchor color palette with minimal variance, an aggressive moisture model that creates excessive deserts, and snow rules that blanket all high terrain regardless of slope or altitude. This plan adds within-biome color diversity, rebalances moisture, and implements physically motivated snow rules.

## Problem Frame

Three visual issues with current biome rendering:
1. **Monotone biomes**: All deserts are the same orange, all forests the same green. Real planets have red/yellow/black sand, olive/emerald/lime grasslands, dark/light forests.
2. **Excessive deserts**: The Hadley moisture model multiplies by `(0.05 + 0.95 * ocean_fraction)` — on a 40% ocean world, global moisture is halved. Everything becomes desert.
3. **Unrealistic snow**: Snow covers all high terrain equally. In reality, steep slopes shed snow, and extremely high peaks above the cloud layer receive little precipitation — they're cold but dry.

## Requirements Trace

- R1. Within-biome color variance: each biome anchor should produce a range of plausible colors driven by position-based noise (e.g., deserts: tan, orange, red, dark volcanic sand)
- R2. The color variance should be seed-dependent and spatially coherent (large regions of similar sand color, not per-pixel speckle)
- R3. Rebalance moisture model so Earth-like planets (~50-70% ocean) have recognizable climate zones, not majority desert
- R4. Snow reduces on steep slopes (sharp ridges shed snow, gentle plateaus accumulate it)
- R5. Snow reduces at extreme altitudes above the cloud layer (too high for precipitation)
- R6. All changes are in the fragment shader only — no compute shader or Rust code changes

## Scope Boundaries

- Not adding new biome types or changing the Whittaker lookup structure
- Not changing the temperature model
- Not adding new UI sliders (variance comes from noise, not user control)
- Not changing ocean rendering or ice cap logic

## Key Technical Decisions

- **Low-frequency noise for color regions**: Use 2-3 octaves of snoise at low frequency (0.5-2.0) to create spatially coherent color variation zones. This produces "red sand deserts in one region, tan sand in another" rather than per-pixel random color.
- **Per-biome color palettes instead of single anchors**: Each of the 6 biome anchors becomes a blend between 2-3 sub-variants, selected by the regional noise. Desert: tan/orange/red/volcanic-dark. Forest: emerald/olive/deep-green. Tundra: grey-brown/purple-grey/beige.
- **Moisture floor adjustment rather than formula rewrite**: Increase the base moisture offset and soften the ocean_fraction scaling. This is the minimal change that fixes the desert problem without rewriting the Hadley model.
- **Slope from heightmap gradient for snow**: The fragment shader already samples 4 neighboring height texels for normals/AO. Reuse these samples to compute slope steepness. Steep slopes → less snow accumulation.

## Open Questions

### Deferred to Implementation

- Exact sub-variant colors per biome — will need visual tuning
- The slope threshold for snow reduction — tune against visual results
- Cloud layer altitude for the "above clouds = less snow" rule — probably reuse the existing cloud altitude from the cloud rendering system or approximate as a fixed height

## Implementation Units

- [ ] **Unit 1: Regional color variance noise**

**Goal:** Add a low-frequency noise field that creates spatially coherent "color regions" — large areas with consistent color character.

**Requirements:** R1, R2

**Dependencies:** None

**Files:**
- Modify: `src/shaders/preview_cubemap.wgsl` — `gradient_color()` function

**Approach:**
- Add a 2-octave low-freq noise sampled at the sphere position (freq ~0.8 and ~1.6) with a unique seed. This produces a "color region ID" value in [0, 1].
- Use this value to shift the hue/saturation of each biome anchor. For each anchor point in gradient_color, instead of a single RGB, interpolate between 2-3 sub-variant RGBs based on the region noise.
- Desert sub-variants: tan (0.85, 0.75, 0.55), red-sand (0.75, 0.40, 0.25), dark volcanic (0.35, 0.30, 0.25)
- Forest sub-variants: emerald (0.10, 0.45, 0.12), olive (0.30, 0.38, 0.15), deep jungle (0.06, 0.25, 0.08)
- Tundra sub-variants: grey-brown (0.55, 0.50, 0.42), purple-grey (0.50, 0.45, 0.50), beige (0.65, 0.60, 0.50)
- Increase the existing color_var noise influence from 0.10 to 0.15 for more per-pixel texture within each region

**Patterns to follow:**
- Existing `color_var = snoise(rotated * 8.0)` in the main function — same pattern but at lower frequency for regional coherence

**Test scenarios:**
- Happy path: Generate planet with Earth-like params, different seeds. Verify visually that desert regions show variety (not all same orange). Different continents should have different sand/soil tones.
- Happy path: Forest regions show variety — some areas darker green, some lighter/olive.
- Edge case: At biome boundaries (temp/moisture transition), colors should still blend smoothly without hard edges between sub-variants.
- Integration: Seasonal tinting still works on top of variant colors (winter browning of green biomes).

**Verification:**
- Zooming into different desert regions on the same planet shows visibly different sand colors
- Different forest regions have distinct green tones
- No new hard color edges visible at biome transitions

---

- [ ] **Unit 2: Moisture model rebalance**

**Goal:** Reduce excessive desert coverage by adjusting the moisture model's ocean scaling and base values.

**Requirements:** R3

**Dependencies:** None (independent of Unit 1)

**Files:**
- Modify: `src/shaders/preview_cubemap.wgsl` — `compute_moisture()` and `hadley_cell_moisture()`

**Approach:**
- Soften the ocean_fraction scaling: change `0.05 + 0.95 * ocean_fraction` to something like `0.25 + 0.75 * ocean_fraction`. This ensures even 30% ocean worlds get 47.5% of base moisture instead of 33.5%.
- Increase the Hadley cell base offset (currently +90.0) to ensure mid-latitude regions stay above the desert threshold (10cm) even on drier planets.
- Optionally widen the subtropical dry zone slightly (currently centered at 28° with σ=60) to be more concentrated — currently the -80 dip is very broad and affects too much of the mid-latitudes.
- Keep the final moisture clamp range [0, 400] unchanged.

**Patterns to follow:**
- Existing `hadley_cell_moisture()` function structure

**Test scenarios:**
- Happy path: Earth-like planet (ocean_fraction ~0.5, water_loss ~0.4) shows recognizable zones: equatorial rainforest, subtropical desert bands, temperate green, polar tundra — not majority desert.
- Happy path: Low-water planet (ocean_fraction ~0.3) still has some green/wet zones, not all desert.
- Edge case: Full ocean world (ocean_fraction ~1.0) doesn't produce unreasonably wet everywhere (moisture should still have latitude variation).
- Edge case: desert planet (water_loss ~0.9) should still be mostly desert — the fix shouldn't eliminate deserts entirely.

**Verification:**
- Earth-like parameters produce a planet where desert area is roughly 20-30% of land, not 50%+
- Clear latitude-based climate bands visible

---

- [ ] **Unit 3: Slope-aware and altitude-aware snow**

**Goal:** Snow accumulation considers terrain steepness and extreme altitude, creating more realistic mountain snow patterns.

**Requirements:** R4, R5

**Dependencies:** None (independent of Units 1-2)

**Files:**
- Modify: `src/shaders/preview_cubemap.wgsl` — land ice/snow blending section (~lines 930-941)

**Approach:**
- **Slope factor**: Reuse the 4-neighbor height samples already done for continentality/AO to compute terrain slope: `slope = max(abs(h_east - h_west), abs(h_north - h_south)) / (2.0 * step)`. Normalize to [0, 1] where 1 = near-vertical. Apply `snow_amount *= smoothstep(0.7, 0.3, slope)` — vertical cliffs get no snow, gentle slopes get full snow.
- **Above-clouds factor**: At very high elevations (land_height > ~0.6-0.7 of max), reduce snow because the air is too thin and dry for precipitation. Apply `snow_amount *= smoothstep(0.8, 0.5, land_height)` — creates a "too high for snow" zone on the tallest peaks.
- Both factors multiply the existing snow blend amount, so they work alongside the existing temperature + altitude logic.

**Patterns to follow:**
- Existing height sampling for continentality at lines 250-260 (h_east, h_west, h_north, h_south)
- Existing snow blend logic at lines 930-941

**Test scenarios:**
- Happy path: Mountain range shows snow on plateaus and gentle slopes but exposed rock on steep ridges/cliffs.
- Happy path: The highest peak on the planet has less snow than slightly lower mountains (above-cloud dryness).
- Edge case: Polar regions (flat, cold) still get full ice coverage — the slope factor shouldn't affect flat terrain.
- Edge case: Low mountains just above the snow line still get snow (the altitude cap only affects extreme peaks).

**Verification:**
- Debug elevation view shows mountains with patchy snow — snow on gentle areas, rock on steep faces
- The very highest peaks show exposed rock (too dry for snow accumulation)
- Low-altitude polar ice unaffected

## System-Wide Impact

- **Unchanged invariants**: Compute shader terrain generation, ocean rendering, cloud layer, atmosphere, temperature model — all unchanged. Only biome coloring and snow blending in the fragment shader.
- **Interaction graph**: The moisture rebalance (Unit 2) will change which pixels the biome color function sees as "wet" vs "dry" — this affects biome colors, seasonal tinting, and potentially cloud patterns (clouds use moisture). Visual tuning may be needed.
- **Export parity**: Exported albedo textures will reflect the new biome colors. No structural changes to export pipeline.

## Risks & Dependencies

| Risk | Mitigation |
|------|------------|
| Color sub-variants look jarring at biome transitions | Use the same low-freq noise for smooth spatial coherence; existing bilinear interpolation in gradient_color handles transitions |
| Moisture rebalance makes deserts disappear entirely | Keep the subtropical -80 dip and test at water_loss=0.9 to verify deserts still form |
| Slope-based snow creates noisy speckle on rough terrain | Use the smoothstep with wide transition range (0.3 to 0.7) and possibly smooth the slope value |

## Sources & References

- Related code: `src/shaders/preview_cubemap.wgsl` — `gradient_color()` (line ~583), `compute_moisture()` (line ~228), `hadley_cell_moisture()` (line ~201), snow blending (line ~930)
- Research: `docs/research/procedural-planet-generation.md` — biome color palettes, Whittaker diagram
