---
title: "fix: Height-aware climate and biome zone fixes"
type: fix
status: completed
date: 2026-03-30
---

# Height-Aware Climate & Biome Zone Fixes

## Overview

Fix straight biome bands, Mars coloring, and sweep viewing angle. The core problem: the climate system (Hadley cells) runs in the fragment shader independently of terrain, producing latitude-only stripes. Biomes need to respond to the actual heightmap — mountains should create rain shadows on specific sides, valleys should be wetter, continental interiors drier.

## Problem Frame

From the parameter sweep review:
1. Biome zones are straight horizontal stripes that don't follow terrain
2. Mars preset is uniform gray — needs desert colors and visible terrain relief
3. Sweep viewing angle shows south pole prominently, hiding equatorial features

The underlying issue: the preview fragment shader computes moisture from Hadley cells + fragment-shader noise, but it can't see the actual heightmap topography because it only samples a single height value at the current pixel. Rain shadows require sampling height at neighboring positions, which means the climate system needs access to the cubemap.

## Requirements Trace

- R1. Biome zones break along terrain features (mountains, valleys, coastlines), not just latitude
- R2. Rain shadows create dry zones behind specific mountain ranges visible in the heightmap
- R3. Continental interiors are drier than coasts based on actual land/ocean distribution in the cubemap
- R4. Mars preset shows visible desert terrain with color variety (tan, rust, ochre)
- R5. Sweep tool generates slightly tilted views showing equatorial features

## Scope Boundaries

- Not adding rivers, erosion, or hydraulic simulation (next phase)
- Not changing the compute pipeline (plates.wgsl) — only the preview shader and sweep tool
- Not adding new parameters — fixing how existing systems interact

## Key Technical Decisions

- **Sample cubemap for rain shadow in fragment shader**: The preview shader already has access to the height cubemap. For rain shadow, sample height at an offset position in the upwind direction. This is 1-2 extra texture samples per pixel — cheap.
- **Continentality from cubemap sampling**: Sample height at 4-6 offsets around the pixel. Count how many are land vs ocean. More land neighbors = more continental = drier. This replaces the current fragment-shader noise-based approximation.
- **Mars fix via biome color tuning**: Mars appears gray because the Whittaker lookup returns desert biome but the desert color is a muted tan. When temperature is low AND moisture is low (cold desert), use rust/ochre colors specific to cold arid worlds.

## Implementation Units

- [ ] **Unit 1: Cubemap-based rain shadow in fragment shader**

**Goal:** Replace the noise-based rain shadow approximation with actual terrain-aware rain shadows using the height cubemap.

**Requirements:** R1, R2

**Dependencies:** None

**Files:**
- Modify: `src/shaders/preview_cubemap.wgsl` — rewrite `compute_moisture()` to sample height cubemap at upwind offsets

**Approach:**
- Wind direction from Hadley cells already computed (trade winds, westerlies, polar easterlies)
- At each pixel, sample the height cubemap at 2-3 points offset in the upwind direction
- If upwind terrain is significantly higher than current position → rain shadow (reduce moisture by 40-60%)
- Use `textureSample(height_tex, height_sampler, rotated + upwind_offset)` — the cubemap is already bound

**Test scenarios:**
- Happy path: mountain range with westerly wind shows green western slopes, brown/desert eastern slopes
- Edge case: ocean-only area shows no rain shadow effect
- Integration: changing seed changes which mountains create which shadows

**Verification:** Visible wet/dry asymmetry on mountain ranges in the Normal and Moisture debug views

---

- [ ] **Unit 2: Cubemap-based continentality**

**Goal:** Replace the noise-based ocean proximity with actual land/ocean distribution from the heightmap cubemap.

**Requirements:** R1, R3

**Dependencies:** None (parallel with Unit 1)

**Files:**
- Modify: `src/shaders/preview_cubemap.wgsl` — update `compute_moisture()` continentality section

**Approach:**
- Sample height cubemap at 4-6 offset positions around the current pixel (compass directions)
- Count how many samples are above sea level (land) vs below (ocean)
- More land neighbors → more continental → multiply moisture by 0.5-0.7
- More ocean neighbors → coastal → multiply moisture by 1.2-1.4
- This creates moisture gradients that follow actual coastlines, not noise patterns

**Test scenarios:**
- Happy path: coastal areas greener than deep continental interiors at same latitude
- Edge case: island surrounded by ocean should be wet on all sides

**Verification:** Moisture debug view shows gradients following actual coastlines, not random noise patterns

---

- [ ] **Unit 3: Cold desert colors for Mars-like planets**

**Goal:** Mars preset shows visible terrain with appropriate arid colors instead of uniform gray.

**Requirements:** R4

**Dependencies:** None (parallel with Units 1-2)

**Files:**
- Modify: `src/shaders/preview_cubemap.wgsl` — add cold desert color variant in `biome_color()`

**Approach:**
- Currently desert biome (ID 4) always returns tan `(0.82, 0.71, 0.55)`
- Add temperature-dependent desert coloring:
  - Hot desert (>20°C): tan/yellow — Sahara-like
  - Warm desert (10-20°C): brown/ochre
  - Cold desert (<10°C): rust/red-brown — Mars-like `(0.65, 0.35, 0.2)`
- Also add slight altitude-based color variation for deserts (lighter at low elevation, darker at high)

**Test scenarios:**
- Happy path: Mars preset shows rust/red-brown desert, not gray
- Happy path: Earth desert areas still show tan/yellow (hot desert)
- Edge case: tundra-to-desert transition at high altitude on cold planet

**Verification:** Mars sweep images show visible terrain with Mars-like rust coloring

---

- [ ] **Unit 4: Tilted sweep view**

**Goal:** Sweep tool generates views showing equatorial features instead of south pole.

**Requirements:** R5

**Dependencies:** None (parallel with all units)

**Files:**
- Modify: `src/bin/sweep.rs` — add slight rotation to the view matrix

**Approach:**
- Current sweep uses identity rotation → views directly at +Z face, south pole prominent
- Apply a ~20° tilt to show the equator and mid-latitudes
- Also add a second view per preset rotated 180° to show the other hemisphere

**Test scenarios:**
- Sweep images show equatorial continent/ocean features prominently
- Both hemispheres visible across the two views

**Verification:** Re-run sweep, images show green/brown equatorial zones not just ice/ocean

## Risks & Dependencies

| Risk | Mitigation |
|------|------------|
| Extra cubemap samples may slow fragment shader | Only 2-3 samples for rain shadow + 4-6 for continentality = ~8 extra texture fetches. Texture sampling is cheap on modern GPUs. Profile if needed. |
| Rain shadow direction may look wrong for some planet orientations | Use the Hadley cell wind direction which is latitude-dependent, not a fixed global wind. This naturally varies across the planet. |

## Sources & References

- Research section 13.2: temperature and moisture generation
- `docs/solutions/architecture/tectonic-terrain-architecture-2026-03-30.md`: separated land/ocean mask learning
- Current climate implementation: `src/shaders/preview_cubemap.wgsl`
