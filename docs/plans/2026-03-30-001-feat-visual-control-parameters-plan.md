---
title: "feat: Add visual control parameters for continents, ice, and water"
type: feat
status: active
date: 2026-03-30
origin: docs/brainstorms/2026-03-29-planet-gen-requirements.md
---

# Add Visual Control Parameters

## Overview

Add three new user-facing parameters that give direct control over planet visual characteristics that the physics model alone cannot produce: continent size, polar ice on ocean, and water loss. These bridge the gap between physics-first generation and artistic needs for VFX.

## Problem Frame

The current physics model derives ocean fraction and terrain structure from distance/mass/metallicity, but the user cannot:
- Control how large or small continents appear (always noise-determined)
- Get polar ice caps that extend over ocean (ice only appears on cold land biomes)
- Create a dry/desert planet that lost its water (ocean fraction is physics-locked)

These are common VFX scenarios: Earth-like with large continents, frozen ocean world, post-water-loss Mars-like arid planet.

## Requirements Trace

- R1. Continental scale slider controls the frequency of the low-frequency terrain base layer
- R2. Polar ocean ice renders frozen ocean at temperatures below a threshold
- R3. Water loss slider overrides derived ocean fraction downward, representing atmospheric escape
- R4. All three parameters integrate into existing preview pipeline with <1s update

## Scope Boundaries

- Not adding new biome types (e.g., pack ice biome) — just coloring ocean as ice at cold temperatures
- Not adding atmospheric escape physics — water loss is a simple override slider
- Not changing the physics model — these are artistic overrides on top of derived values

## Key Technical Decisions

- **Continental scale as a separate slider rather than tied to mass**: Mass already affects detail frequency. Continental scale independently controls the base-layer noise frequency (research section 7.3: freq 2-4 for continents). Separating them gives the user direct control over "how many continents" without changing terrain roughness.
- **Water loss as a multiplicative override**: The slider reduces the physics-derived ocean_fraction by a factor (1.0 = no loss, 0.0 = complete loss). This preserves the physics baseline while allowing artistic override. The derived properties panel shows both the physics value and the effective value.
- **Polar ice threshold in the shader, not a new parameter**: Ice on ocean is determined by the existing temperature model. If ocean surface temperature < -2°C (seawater freezing point), render as ice. This is automatic — no slider needed, just a shader fix.

## Implementation Units

- [ ] **Unit 1: Add continental_scale parameter to UI and uniforms**

**Goal:** Add a "Continental Scale" slider (0.5-4.0, default 1.0) that controls the frequency of the `continental_base()` noise layer.

**Requirements:** R1

**Dependencies:** None

**Files:**
- Modify: `src/app.rs` — add `continental_scale: f32` field to `PlanetGenApp`, add slider, pass to uniforms
- Modify: `src/preview.rs` — add `continental_scale: f32` to `PreviewUniforms`
- Modify: `src/shaders/preview.wgsl` — add `continental_scale` to Uniforms struct, multiply continental noise frequencies by it

**Approach:**
- Lower continental_scale → lower frequencies → fewer, larger continents (Earth-like at ~0.7-0.8)
- Higher continental_scale → more, smaller continents/islands (archipelago world at ~2.0-3.0)
- Default 1.0 = current behavior
- The slider multiplies the hardcoded frequencies in `continental_base()`: `snoise(p * 2.0 * scale)`, etc.

**Patterns to follow:** Existing slider pattern in `app.rs` (see star_distance slider)

**Test scenarios:**
- Happy path: changing continental_scale from 0.5 to 3.0 produces visibly different continent sizes
- Edge case: scale=0.5 produces 2-4 large landmasses; scale=3.0 produces many small islands
- Integration: preview updates within 1 second when slider changes

**Verification:** Slider visible in UI, preview shows different continent sizes at different values

---

- [ ] **Unit 2: Add polar ocean ice rendering**

**Goal:** Render ocean as ice (white/light blue) when temperature is below seawater freezing point (-2°C).

**Requirements:** R2

**Dependencies:** None (can be done in parallel with Unit 1)

**Files:**
- Modify: `src/shaders/preview.wgsl` — in the ocean coloring section, check temperature and render ice color when < -2°C

**Approach:**
- In `fs_main`, after the `is_ocean` check, compute temperature at the ocean surface point
- If temperature < -2°C (seawater freezing), use ice color instead of ocean color
- Blend between ice and ocean at the transition zone (-2°C to +2°C) for smooth edges
- This makes polar ice caps extend over ocean naturally from the existing temperature model

**Patterns to follow:** Existing ice cap logic in the land section of `fs_main`

**Test scenarios:**
- Happy path: Earth-like planet shows white ice extending over ocean at poles
- Edge case: fully frozen planet (distance ~2+ AU) shows ice everywhere including all ocean
- Edge case: hot planet (distance 0.5 AU) shows no ice at all
- Integration: tilt slider shifts where ocean ice appears (asymmetric ice caps)

**Verification:** Polar regions show ice on ocean, not just on land biomes

---

- [ ] **Unit 3: Add water_loss parameter to UI and uniforms**

**Goal:** Add a "Water Loss" slider (0.0-1.0, default 0.0) that reduces effective ocean fraction below the physics-derived value.

**Requirements:** R3

**Dependencies:** None (can be done in parallel with Units 1-2)

**Files:**
- Modify: `src/app.rs` — add `water_loss: f32` field, add slider, compute effective ocean fraction as `derived.ocean_fraction * (1.0 - water_loss)`, pass to uniforms
- Modify: `src/app.rs` — show both physics and effective ocean % in derived properties panel

**Approach:**
- `effective_ocean = derived.ocean_fraction * (1.0 - water_loss)`
- At water_loss=0.0: normal physics-derived ocean (default)
- At water_loss=0.5: half the ocean → exposed ocean floor becomes desert/barren land
- At water_loss=1.0: completely dry planet — all ocean becomes land
- The ocean_level uniform uses the effective value, not the raw derived value
- Show in UI: "Ocean: 70% (effective: 35%)" when water_loss > 0

**Patterns to follow:** Existing ocean_fraction usage in `build_uniforms()`

**Test scenarios:**
- Happy path: water_loss=0.5 on Earth-like planet shows roughly half the ocean area exposed as desert
- Happy path: water_loss=1.0 shows no ocean at all
- Edge case: water_loss on an already-dry planet (distance 0.3 AU) has no visible effect
- Integration: exposed ocean floor uses desert/barren biome colors, not ocean blue

**Verification:** Slider reduces ocean coverage smoothly; exposed areas show appropriate land biomes

---

- [ ] **Unit 4: Update uniform struct alignment**

**Goal:** Ensure the PreviewUniforms struct stays 16-byte aligned after adding new fields.

**Requirements:** R4

**Dependencies:** Units 1, 2, 3

**Files:**
- Modify: `src/preview.rs` — verify struct size matches WGSL expectations
- Modify: `src/shaders/preview.wgsl` — verify WGSL struct matches Rust layout

**Approach:**
- Adding `continental_scale` (f32) replaces `tectonics_factor`'s current position or adds after it
- Current struct is 128 bytes (8 × 16). Adding 1 float needs padding to reach 144 (9 × 16) or restructure
- Count total fields and add padding as needed
- Add a compile-time size assertion: `const _: () = assert!(size_of::<PreviewUniforms>() % 16 == 0);`

**Test scenarios:**
- Build succeeds with no wgpu validation errors
- All existing tests pass (27+ tests)

**Verification:** `cargo test --lib` passes, `cargo run` shows no GPU validation errors

## Risks & Dependencies

| Risk | Mitigation |
|------|------------|
| Uniform buffer size mismatch (WGSL vs Rust) | Compile-time size assertion, test with `cargo test` |
| Water loss slider interaction with temperature model | Exposed ocean floor inherits temperature/biome from terrain below sea level — this may produce odd biomes. Accept for v1, can refine later. |

## Sources & References

- **Origin document:** [docs/brainstorms/2026-03-29-planet-gen-requirements.md](docs/brainstorms/2026-03-29-planet-gen-requirements.md)
- Research section 7.3: Continental placement frequency ≈ 2-4
- Seawater freezing point: -1.8°C to -2°C
