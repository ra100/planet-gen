---
date: 2026-04-02
topic: layer-toggle-system
---

# Layer Toggle System: Isolate and Build Up Rendering

## Problem Frame

Developing terrain and visual features is difficult because the preview renders everything at once — water, biomes, ice, atmosphere, clouds. When something looks wrong (e.g., noisy heightmap), it's unclear which layer is responsible. We need to isolate rendering effects and build them up one at a time, verifying each before adding the next. This also requires reorganizing the existing 14 view modes into a clearer structure.

## Requirements

**Render Layer Toggles**
- R1. Add per-layer checkboxes in the UI that independently enable/disable rendering effects in the Normal view mode
- R2. Layer list: Water/Ocean, Ice Caps, Biome Colors, Clouds, Atmosphere, City Lights
- R3. Base layer (Elevation Coloring) is always on in Normal view — land colored by height ramp (green lowlands → brown highlands → gray → white peaks), below-sea-level areas as flat dark color, diffuse lighting only
- R4. When Water is OFF, below-sea-level terrain shows as flat dark color (no depth shading, no specular, no transparency)
- R5. When Biome Colors is OFF, land uses only the height-based color ramp (no temperature/moisture-driven biome variation)
- R6. When Ice Caps is OFF, no polar or altitude ice rendering
- R7. When Clouds is OFF, no cloud layer rendering
- R8. When Atmosphere is OFF, no atmospheric scattering (already exists as `show_atmosphere` toggle — integrate into the new panel)
- R9. Layer toggles only affect the Normal view mode. Debug/export views render their specific data regardless of toggle state
- R10. Default state: only Elevation Coloring enabled (all other layers OFF). This supports the current development workflow of verifying terrain first.

**View Mode Reorganization**
- R11. Split existing view modes into two groups in the UI: "Export Views" and "Debug Views"
- R12. Export Views: Albedo, Height, Roughness, Normal Map, Clouds, Emission (City Lights) — these correspond to exportable texture layers
- R13. Debug Views: Plates, Temperature, Moisture, Biome, Ocean/Ice, Boundary Type, Snow/Ice, AO — diagnostic visualizations
- R14. Keep the existing ComboBox selector but group options with headers or separate selectors

## Success Criteria

- Can render the planet with only elevation coloring and verify terrain quality in isolation
- Can add water layer and verify ocean/coastline rendering without biome/ice/cloud interference
- Can incrementally enable each layer and confirm it looks correct before adding the next
- View mode selector is organized into logical groups (export vs debug)

## Scope Boundaries

- No new rendering features — this is purely toggle/organization work
- No changes to the actual rendering algorithms — just conditional execution of existing shader code
- No changes to the export pipeline — export always renders all layers
- The height-based color ramp for the base layer can reuse existing terrain coloring logic with biome/moisture factors zeroed out

## Key Decisions

- **Checkboxes per layer** (not presets): Maximum flexibility for isolating any combination of effects
- **Height-colored base** (not grayscale): Provides useful visual feedback while still being a clean baseline — green/brown/white ramp with diffuse lighting
- **Default all OFF except elevation**: Supports current development workflow where we need to verify terrain first

## Dependencies / Assumptions

- The preview shader (`preview_cubemap.wgsl`) already computes each effect in somewhat separable stages — toggles will gate these stages via uniform flags
- Existing `show_atmosphere` and `show_ao` toggles can be absorbed into the new layer panel

## Outstanding Questions

### Deferred to Planning
- [Affects R3][Technical] How to implement the height color ramp — extract from existing biome coloring or create a dedicated function?
- [Affects R1][Technical] How many uniform bool flags can be added before hitting buffer alignment issues? (Currently GenParams has 4 padding fields)
- [Affects R14][UI] Best egui pattern for grouped ComboBox or sectioned dropdown

## Next Steps

→ `/ce:plan` for structured implementation planning
