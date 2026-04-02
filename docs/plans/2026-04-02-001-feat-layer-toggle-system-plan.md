---
title: "feat: Layer toggle system for isolated rendering"
type: feat
status: active
date: 2026-04-02
origin: docs/brainstorms/2026-04-02-layer-toggle-system-requirements.md
---

# feat: Layer toggle system for isolated rendering

## Overview

Add per-layer checkboxes that independently enable/disable rendering effects (water, ice, biomes, clouds, atmosphere, city lights) in the Normal view mode. Reorganize the 14 existing view modes into Export and Debug groups. This enables verifying each rendering layer in isolation before compositing.

## Problem Frame

Developing terrain and visual features is difficult because the preview renders everything at once. When something looks wrong (e.g., noisy heightmap), it's unclear which layer is responsible. We need to isolate effects and build them up one at a time. (see origin: docs/brainstorms/2026-04-02-layer-toggle-system-requirements.md)

## Requirements Trace

- R1. Per-layer checkboxes in UI for Normal view mode
- R2. Layers: Water/Ocean, Ice Caps, Biome Colors, Clouds, Atmosphere, City Lights
- R3. Base layer: height-colored terrain (green→brown→gray→white), flat dark below sea level, diffuse only
- R4. Water OFF: below-sea-level as flat dark color
- R5. Biomes OFF: height-only color ramp
- R6. Ice OFF: no polar or altitude ice
- R7. Clouds OFF: no cloud layer
- R8. Atmosphere OFF: absorb existing `show_atmosphere` toggle
- R9. Toggles only affect Normal view mode
- R10. Default: all layers OFF except elevation
- R11-R14. View mode reorganization into Export/Debug groups

## Scope Boundaries

- No new rendering features — conditional execution of existing shader code only
- No changes to export pipeline
- No changes to rendering algorithms

## Context & Research

### Relevant Code and Patterns

- **Shader**: `src/shaders/preview_cubemap.wgsl` — rendering stages are somewhat separable:
  - Water/ocean: lines ~860-895 (shallow→deep gradient, specular)
  - Ice caps: lines ~878-891 (ocean ice), ~947-957 (land ice)
  - Biome coloring: `gradient_color()` fn at ~571-617, applied at ~901-902
  - Clouds: computed at ~300, applied at ~1045, ~1179, ~1201, ~1210
  - Atmosphere: `ray_march()` at ~92, applied at ~1121-1145
  - City lights: `compute_urban_density()` at ~708, day ~958-968, night ~1160-1170
- **Uniforms**: `PreviewUniforms` in `src/preview.rs:8-37` — bools passed as f32 (0.0/1.0). Currently has `show_ao: f32` and 3 padding floats (`_pad4: [f32; 3]`)
- **Toggle wiring**: `show_atmosphere` uses `atmosphere_density` (set to 0.0 when off). `show_ao` uses dedicated f32 flag. Both in `src/app.rs`
- **View modes**: u32 selector (0-13), displayed as `selectable_label` grid in `src/app.rs:677-681`

### Existing Toggle Pattern

```
app.rs: checkbox → bool field → preview.rs: uniforms.flag = if bool { 1.0 } else { 0.0 } → shader: if (flag > 0.5) { ... }
```

This pattern (f32 flags in uniform buffer) is already established and should be reused for all new layer toggles.

## Key Technical Decisions

- **f32 flags in uniform buffer** (not u32 bitfield): Matches existing `show_ao` pattern. 6 new flags = 24 bytes. Current padding has 12 bytes (3 × f32), so we need 3 additional f32s beyond the padding — add them and adjust padding to maintain 16-byte alignment.
- **Height color ramp**: Extract a dedicated `height_color()` function in the shader rather than zeroing biome inputs. The existing `gradient_color()` blends temperature/moisture which would require dummy values. A clean height ramp (green→brown→gray→white) is simpler and more useful as a base.
- **View mode grouping**: Use two separate `CollapsingHeader` sections ("Export Views" / "Debug Views") with `selectable_label` grids inside each. Simpler than trying to group within a single ComboBox.

## Open Questions

### Resolved During Planning

- **Uniform buffer space**: 3 padding f32s available. Need 6 new flags (water, ice, biomes, clouds, atmosphere, cities). Replace 3 padding slots + add 3 new fields + add 1 new padding for 16-byte alignment = net +4 f32s. This works within wgpu alignment requirements.
- **Height color ramp approach**: Create a `height_color(h: f32) -> vec3<f32>` function with smooth gradient: green(0.0) → brown(0.3) → gray(0.6) → white(0.9+). Below sea level: flat dark blue-gray.
- **egui grouping**: Two `CollapsingHeader` sections replace the single flat grid. Each contains `selectable_label` items.

### Deferred to Implementation

- Exact color values for the height ramp — tune visually during implementation
- Whether cloud application points (4 separate locations in shader) can be cleanly gated with a single flag check or need per-site guards

## Implementation Units

- [ ] **Unit 1: Add layer flag fields to uniform buffer**

  **Goal:** Add 6 f32 flag fields to PreviewUniforms and wire them from app state to shader

  **Requirements:** R1, R2

  **Dependencies:** None

  **Files:**
  - Modify: `src/preview.rs` (PreviewUniforms struct)
  - Modify: `src/app.rs` (add bool fields to PlanetGenApp, wire to uniforms)
  - Modify: `src/shaders/preview_cubemap.wgsl` (add fields to Uniforms struct)
  - Modify: `src/bin/sweep.rs` (update PreviewUniforms construction)
  - Modify: `src/bin/erosion_compare.rs` (update PreviewUniforms construction)
  - Test: `cargo test --lib`

  **Approach:**
  - Add to PreviewUniforms: `show_water: f32`, `show_ice: f32`, `show_biomes: f32`, `show_clouds: f32`, `show_atmosphere: f32`, `show_cities: f32`
  - Replace `_pad4: [f32; 3]` with the first 3 flags, then add 3 more flags + 1 padding f32 for alignment
  - Add corresponding bool fields to PlanetGenApp with defaults matching R10 (all false except implicit elevation)
  - Absorb existing `show_atmosphere` bool into the new `show_atmosphere` layer flag (R8)
  - Wire in `build_uniforms()` using the `if bool { 1.0 } else { 0.0 }` pattern
  - Update WGSL Uniforms struct to match new layout
  - Update sweep.rs and erosion_compare.rs PreviewUniforms constructions (known breakage point per MEMORY.md)

  **Patterns to follow:**
  - Existing `show_ao` wiring in app.rs:168 and preview.rs:35

  **Test scenarios:**
  - Happy path: project compiles and all existing tests pass with new fields at default values
  - Happy path: uniform buffer size matches between Rust struct and WGSL struct (existing alignment test)

  **Verification:**
  - `cargo test --lib` passes
  - App launches without GPU validation errors

- [ ] **Unit 2: Add layer toggle UI checkboxes**

  **Goal:** Add a "Render Layers" panel with 6 checkboxes in the UI

  **Requirements:** R1, R2, R8, R10

  **Dependencies:** Unit 1

  **Files:**
  - Modify: `src/app.rs` (UI code in the side panel)

  **Approach:**
  - Add a new `CollapsingHeader("Render Layers")` section in the side panel, before the current view mode selector
  - Add checkboxes for: Water/Ocean, Ice Caps, Biome Colors, Clouds, Atmosphere, City Lights
  - Remove the standalone `show_atmosphere` and `show_ao` checkboxes — absorb atmosphere into the new panel, keep AO as a separate utility toggle or add it to the panel
  - Each checkbox triggers `self.needs_render = true` on change
  - Default all to false (R10)

  **Patterns to follow:**
  - Existing checkbox pattern at app.rs:484-497

  **Test scenarios:**
  - Happy path: all checkboxes render and are clickable
  - Happy path: toggling a checkbox triggers re-render

  **Verification:**
  - App shows "Render Layers" panel with 6 checkboxes
  - Toggling any checkbox causes the planet to re-render

- [ ] **Unit 3: Implement height color ramp base layer in shader**

  **Goal:** Create the height-only base rendering path (R3) — the view when all layer toggles are OFF

  **Requirements:** R3, R4, R5

  **Dependencies:** Unit 1

  **Files:**
  - Modify: `src/shaders/preview_cubemap.wgsl`

  **Approach:**
  - Add a `height_color(h: f32) -> vec3<f32>` function with smooth gradient:
    - Below 0.0: flat dark blue-gray (e.g., `vec3(0.15, 0.18, 0.22)`)
    - 0.0-0.15: green (lowlands/plains)
    - 0.15-0.4: brown (highlands)
    - 0.4-0.7: gray (alpine/rock)
    - 0.7+: white (snow peaks)
  - In the main rendering path, when `show_biomes < 0.5`, use `height_color()` instead of `gradient_color()` for land coloring
  - When `show_water < 0.5` AND height < 0 (below sea level), use the flat dark color from `height_color()` instead of the ocean rendering path
  - Diffuse-only lighting when in base mode (skip specular for water-off areas)

  **Test scenarios:**
  - Happy path: with all toggles OFF, planet shows green/brown/gray/white terrain with dark ocean areas
  - Edge case: at sea level boundary, no visual artifacts between land and below-sea-level areas
  - Edge case: very high peaks show white, very low ocean areas show uniform dark

  **Verification:**
  - Planet renders with clear elevation-based coloring when all layers are OFF
  - No biome variation visible — color depends only on height
  - Below-sea-level areas are uniformly dark

- [ ] **Unit 4: Gate water, ice, biomes, clouds, cities in shader**

  **Goal:** Make each rendering layer conditional on its uniform flag

  **Requirements:** R4, R5, R6, R7, R8, R9

  **Dependencies:** Unit 1, Unit 3

  **Files:**
  - Modify: `src/shaders/preview_cubemap.wgsl`

  **Approach:**
  - **Water** (`show_water`): Gate the ocean color computation (~860-895). When OFF, below-sea-level pixels use `height_color()` result.
  - **Ice** (`show_ice`): Gate ocean ice blend (~878-891) and land ice overlay (~947-957). When OFF, skip both.
  - **Biomes** (`show_biomes`): Gate `gradient_color()` call (~901-902). When OFF, use `height_color()` instead.
  - **Clouds** (`show_clouds`): Gate all cloud application points (~1045, ~1179, ~1201, ~1210). When OFF, skip cloud density computation and overlay.
  - **Atmosphere** (`show_atmosphere`): Gate atmospheric scattering integration (~1121-1145). Replace current `atmosphere_density > 0.001` check.
  - **Cities** (`show_cities`): Gate urban density computation and application (~958-968 day, ~1160-1170 night). When OFF, skip both.
  - All gates use `if (flag > 0.5) { ... }` pattern
  - Debug view modes (R9): these already branch early in the shader and return before the normal rendering path, so no changes needed

  **Test scenarios:**
  - Happy path: each layer can be toggled independently without affecting others
  - Happy path: all layers ON produces identical output to current rendering (regression)
  - Edge case: water OFF + ice ON — ice should not render on non-existent water surfaces
  - Edge case: biomes OFF + cities ON — city darkening should still work on height-colored terrain
  - Integration: toggling layers in various combinations produces no shader compilation errors

  **Verification:**
  - Each layer independently controllable via its checkbox
  - All-ON matches current rendering output
  - No visual artifacts at layer boundaries when toggling

- [ ] **Unit 5: Reorganize view mode selector into Export/Debug groups**

  **Goal:** Split the 14 view modes into two labeled groups in the UI

  **Requirements:** R11, R12, R13, R14

  **Dependencies:** None (independent of Units 1-4)

  **Files:**
  - Modify: `src/app.rs` (view mode selector UI section)

  **Approach:**
  - Replace the single `horizontal_wrapped` grid of 14 selectable labels with two `CollapsingHeader` sections:
    - "Export Views": Normal (0), Height (1), Roughness (7), AO (8), Clouds (9), Cities (10), Normals (13)
    - "Debug Views": Plates (6), Temperature (2), Moisture (3), Biome (4), Ocean/Ice (5), Boundary Type (11), Snow/Ice (12)
  - Keep Normal in Export Views as it represents the final composited output (≈ Albedo)
  - Each section uses `horizontal_wrapped` + `selectable_label` internally
  - The u32 debug_mode value stays the same — only the UI grouping changes

  **Patterns to follow:**
  - Existing `CollapsingHeader` usage throughout the side panel
  - Existing `selectable_label` grid pattern at app.rs:677-681

  **Test scenarios:**
  - Happy path: all 14 view modes still accessible and functional
  - Happy path: selecting a mode in either group works correctly
  - Edge case: switching between groups doesn't cause mode conflicts

  **Verification:**
  - View modes organized into two clearly labeled groups
  - All existing view modes still work identically

## System-Wide Impact

- **Uniform buffer change**: Adding fields to PreviewUniforms changes the GPU buffer layout. Both Rust struct and WGSL struct must match exactly. Known impact on `sweep.rs` and `erosion_compare.rs` (documented in MEMORY.md).
- **Shader branching**: Adding conditional branches may slightly affect GPU performance, but the branches are uniform (all pixels take the same path for a given toggle state), so impact should be negligible.
- **Export pipeline**: Unaffected — export renders to textures via compute shaders, not the preview fragment shader.
- **Unchanged invariants**: All existing rendering algorithms remain identical. Layer toggles only gate execution, they don't modify the algorithms.

## Risks & Dependencies

| Risk | Mitigation |
|------|------------|
| Uniform buffer misalignment between Rust and WGSL | Verify with existing alignment tests; add fields carefully with padding |
| Cloud gating complexity (4 application sites) | May need a single early `cloud_density = 0.0` override rather than per-site checks |
| Ice+Water interaction when independently toggled | Define clear precedence: water OFF means no ocean surface, so ocean ice also hidden |

## Sources & References

- **Origin document:** [docs/brainstorms/2026-04-02-layer-toggle-system-requirements.md](docs/brainstorms/2026-04-02-layer-toggle-system-requirements.md)
- PreviewUniforms: `src/preview.rs:8-37`
- Toggle wiring pattern: `src/app.rs:484-497`
- View mode selector: `src/app.rs:677-681`
- Known bin breakage: MEMORY.md → `feedback_previewuniforms_bins.md`
