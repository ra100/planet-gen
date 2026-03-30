---
title: "feat: Physics-driven terrain and climate system"
type: feat
status: active
date: 2026-03-30
origin: docs/brainstorms/2026-03-30-physics-driven-terrain-requirements.md
---

# Physics-Driven Terrain & Climate

## Overview

Replace independent noise layers with interconnected physical systems: Hadley cell atmospheric circulation for moisture, domain warping for geological terrain character, rain shadows from wind-terrain interaction, and altitude zonation. All implemented in the existing preview fragment shader.

## Problem Frame

Current terrain and climate are independent noise. Deserts appear randomly, not at 30° latitude. Mountains have no rain shadows. Terrain is blobby noise, not geological. This makes the result look procedurally generated rather than physically plausible.

## Requirements Trace

- R1-R4: Hadley cell moisture model (atmospheric circulation)
- R5-R6: Rain shadow and continentality (terrain-climate interaction)
- R7: Altitude zonation on mountains
- R8-R9: Domain warping for geological terrain

## Scope Boundaries

- No fluid simulation — visual approximation only
- No rivers, erosion, or sediment transport
- Preview must stay <1s on modern GPU
- All changes in preview.wgsl fragment shader

## Key Technical Decisions

- **Hadley cells as latitude-based functions**: Surface pressure, wind direction, and moisture tendency are deterministic from latitude. No simulation needed — use mathematical model of the three-cell circulation.
- **Wind direction from cells**: Trade winds (0-30°, easterly), westerlies (30-60°), polar easterlies (60-90°). Wind direction determines rain shadow orientation.
- **Domain warping in continental_base only**: Apply warping to the low-frequency continent layer. Detail noise stays unwrapped — this gives geological character to coastlines and mountain ranges without over-warping fine detail.

## Implementation Units

- [ ] **Unit 1: Hadley cell moisture model**

**Goal:** Replace noise-based moisture with physically-motivated atmospheric circulation.

**Requirements:** R1, R2, R3, R4

**Dependencies:** None

**Files:**
- Modify: `src/shaders/preview.wgsl` — rewrite `compute_moisture()` function

**Approach:**
- Three-cell model per hemisphere:
  - 0-30° (Hadley): rising air at equator (wet ITCZ), sinking at ~30° (dry subtropics)
  - 30-60° (Ferrel): rising at ~60° (wet), sinking at ~30° (dry — reinforces desert)
  - 60-90° (Polar): sinking cold air (dry)
- Moisture base = `sin(3 * latitude)` shaped curve with wet equator, dry 30°, wet 60°, dry poles
- Shift pattern by `axial_tilt_rad * sin(longitude)` for asymmetric seasons
- Still modulate with noise for local variation (±20% of base, not ±100%)
- Ocean proximity bonus remains but is secondary to circulation pattern

**Test scenarios:**
- Earth defaults: deserts visibly concentrated at ~30° N/S, green equatorial band
- Tilt effect: changing tilt shifts the wet/dry bands asymmetrically
- Still has local variation (not perfectly banded)

**Verification:** Visible latitude-banded climate zones; desert bands at ~30°

---

- [ ] **Unit 2: Wind direction and rain shadow**

**Goal:** Mountains create dry zones on their leeward side based on wind direction from Hadley model.

**Requirements:** R5, R6

**Dependencies:** Unit 1 (need wind direction from Hadley model)

**Files:**
- Modify: `src/shaders/preview.wgsl` — add wind direction function, modify moisture computation

**Approach:**
- Wind direction from latitude: `wind_dir` = easterly at 0-30°, westerly at 30-60°, easterly at 60-90°
- At each pixel, sample terrain height at a point offset in the upwind direction
- If upwind terrain is significantly higher → reduce moisture (rain shadow)
- Continentality: compute approximate distance from ocean by checking height at several offsets — more "land neighbors" = more continental = drier
- These replace the current simple `rain_shadow` and `ocean_bonus` calculations

**Test scenarios:**
- Mountain range with wind from west: green western slopes, brown eastern slopes
- Continental interior drier than coastal areas at same latitude

**Verification:** Visible wet/dry asymmetry on mountain ranges

---

- [ ] **Unit 3: Domain warping for geological terrain**

**Goal:** Make terrain look geological (ridges, winding valleys, irregular coastlines) instead of blobby noise.

**Requirements:** R8, R9

**Dependencies:** None (independent of Units 1-2)

**Files:**
- Modify: `src/shaders/preview.wgsl` — modify `continental_base()` to use domain warping

**Approach:**
- Domain warping: before sampling the main continental noise, offset the sample position by another noise field
  - `warped_pos = pos + warp_noise(pos) * warp_strength`
  - `warp_noise` = low-frequency snoise at different offset than main noise
  - `warp_strength` ≈ 0.3-0.5 (enough to create curves without destroying structure)
- Elevation-dependent detail: multiply fine-detail amplitude by a factor based on height
  - High terrain (mountains): detail × 1.5 (craggy)
  - Low terrain (plains): detail × 0.5 (smooth)
  - Ocean floor: detail × 0.3 (mostly smooth with occasional ridges)

**Test scenarios:**
- Coastlines have irregular shapes (bays, peninsulas) not smooth blobs
- Mountain areas visibly more detailed/rough than lowlands
- Different seeds produce different geological character

**Verification:** Terrain has visible geological character; coastlines are irregular

---

- [ ] **Unit 4: Altitude zonation**

**Goal:** Mountains show visible altitude bands: forest → alpine → rock → snow.

**Requirements:** R7

**Dependencies:** Unit 1 (need realistic moisture for base biome)

**Files:**
- Modify: `src/shaders/preview.wgsl` — modify the land coloring section in `fs_main`

**Approach:**
- Current code already blends mountain rock and snow at high elevation
- Extend to show full altitude progression:
  - 0-30% land height: base biome from Whittaker (forest, grassland, desert)
  - 30-55%: transition to alpine (short grass, shrubs — lighter green/brown)
  - 55-75%: bare rock (gray)
  - 75%+: permanent snow (white)
- Thresholds shift with latitude (snow line lower at poles, higher at equator)
- Temperature already drops with altitude — use this for smooth transitions

**Test scenarios:**
- Mountain on equator: green base → brown mid → gray rock → white peak
- Mountain at high latitude: entire mountain may be snow-covered
- Flat lowland areas unaffected (no rock/snow unless very cold)

**Verification:** Visible horizontal color banding on mountain slopes

## Risks & Dependencies

| Risk | Mitigation |
|------|------------|
| Performance: extra noise calls for domain warping + rain shadow sampling | Profile the shader. Domain warping adds ~3 noise calls, rain shadow adds ~1-2. Total should still be <1s at 768² resolution |
| Rain shadow quality: sampling upwind terrain from a single offset may look coarse | Use 2-3 offset samples and average for smoother result |

## Sources & References

- **Origin document:** [docs/brainstorms/2026-03-30-physics-driven-terrain-requirements.md](../brainstorms/2026-03-30-physics-driven-terrain-requirements.md)
- Hadley cell model: research section 3.1 (atmosphere reference)
- Domain warping technique: Inigo Quilez, "Domain Warping" (standard procedural technique)
- Altitude zonation: research section 13.2 (biome mapping with lapse rate)
