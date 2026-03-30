---
date: 2026-03-30
topic: physics-driven-terrain
---

# Physics-Driven Terrain & Climate

## Problem Frame

The planet generator currently produces terrain and climate from independent noise layers. Height is fBm noise, moisture is separate noise, biomes are a lookup table on (temp, moisture). Nothing causes anything else — deserts can appear next to rainforests, mountains have no rain shadows, and there's no atmospheric circulation pattern. The result looks procedurally generated rather than physically plausible, which undermines the tool's value for VFX.

## Requirements

**Atmospheric Circulation**

- R1. Replace noise-based moisture with Hadley cell circulation model. Three cells per hemisphere: tropical (0-30°), mid-latitude (30-60°), polar (60-90°). Rising air = wet, sinking air = dry.
- R2. Deserts should appear preferentially at ~30° latitude (subtropical high) where Hadley and Ferrel cells meet, not randomly.
- R3. Equatorial regions should be consistently wet (ITCZ — Intertropical Convergence Zone). Polar regions should be dry (cold air holds less moisture).
- R4. Axial tilt should shift the ITCZ and Hadley cell boundaries, creating asymmetric wet/dry zones.

**Terrain-Climate Interaction**

- R5. Mountains create rain shadows: moist wind hits a mountain range → precipitation on windward side → dry leeward side. Wind direction derived from Hadley cell model.
- R6. Coastal areas receive more moisture than continental interiors (continentality effect). Distance from ocean should reduce moisture gradually, not just as a sharp proximity bonus.
- R7. Altitude zonation on mountains: biome should transition with elevation (tropical forest → cloud forest → alpine → bare rock → snow) creating visible horizontal banding on mountain slopes.

**Domain-Warped Terrain**

- R8. Continental terrain should use domain warping (use noise to displace noise sampling coordinates) to create geological-looking features: curved mountain ridges, winding valleys, irregular coastlines with peninsulas and bays.
- R9. Different terrain character by elevation: mountain areas should be craggy (high-frequency detail), lowlands should be smoother (erosion fills valleys), ocean floor should be smooth with occasional ridges.

## Success Criteria

- Earth-like parameters produce a planet with recognizable geographic patterns: desert bands at ~30° latitude, wet equatorial zone, rain shadows behind mountains
- Changing axial tilt visibly shifts the wet/dry zones
- Mountains show altitude zonation (visible color banding from green to brown to white)
- Terrain has geological character (ridges, valleys) not blobby noise

## Scope Boundaries

- Not simulating actual wind flow or fluid dynamics — approximating the visual result
- Not adding rivers, erosion, or sediment transport (future feature)
- Not changing the preview shader architecture (still per-pixel in fragment shader)
- Not adding new UI parameters (these improvements use existing parameters)
- Performance: preview must still update in <1 second

## Key Decisions

- **Visual approximation over simulation**: Use mathematical models of Hadley cells, not fluid simulation. The goal is the visual result, not scientific accuracy. Per-pixel evaluation in the fragment shader.
- **Wind direction from Hadley cells**: Surface wind direction is deterministic from latitude (trade winds from east at 0-30°, westerlies from west at 30-60°, polar easterlies at 60-90°). No need for a separate wind parameter.
- **Domain warping in continental_base**: Apply warping to the existing continental noise layer, not as a separate pass. This adds geological character without architectural changes.

## Outstanding Questions

### Deferred to Planning

- [Affects R5][Technical] Rain shadow requires knowing "which direction is upwind" at each point. Hadley model gives wind direction from latitude, but the mountain orientation relative to wind matters. May need to sample height at an offset point in the wind direction.
- [Affects R8][Technical] Domain warping adds noise evaluations per pixel (2-3 additional snoise calls). Need to verify preview stays <1s on target GPU.

## Next Steps

→ `/ce:plan` for structured implementation planning
