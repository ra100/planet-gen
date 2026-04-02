---
date: 2026-03-31
topic: cloud-layer
---

# Procedural Cloud Layer

## Problem Frame

The planet preview needs a convincing cloud layer rendered as a 2D shell. The current implementation suffers from four critical visual defects:

1. **Slider cliff** — Coverage slider has non-linear response; small changes flip the planet between nearly clear and heavily overcast
2. **Flat appearance** — Clouds render as uniform white/grey with no depth, self-shadowing, or thickness variation
3. **Latitude banding** — Multiplying noise by Hadley cell moisture creates obvious horizontal climate bands that dominate cloud placement
4. **No texture** — Cloud formations lack visible internal structure at planet scale

The target aesthetic is Space Engine / procedural planet generators: plausible cloud patterns that work across planet types, with climate accuracy as the foundation but visual quality as the measure of success.

## Requirements

**Density Generation**

- R1. Cloud density uses domain-warped fBm noise (not plain fBm) for organic, non-uniform shapes
- R2. 5 octaves minimum for visible detail at planet scale (wispy edges, cellular structure)
- R3. Base frequency ~5.0 to produce 6-10 major cloud systems visible from space

**Coverage Slider**

- R4. Coverage slider (0.0–1.0) produces approximately linear visual response — each 0.1 increment adds roughly equal visible cloud area
- R5. Use Schneider remap technique: `remap(noise, 1-coverage, 1, 0, 1) * coverage` for natural density distribution (lighter thin clouds, denser large clouds)
- R6. No cliff effects — smooth transition from clear to overcast across the full slider range

**Climate Modulation**

- R7. Climate data (moisture) controls the local coverage threshold, NOT multiplied with density — this is the key technique to avoid latitude bands
- R8. Domain-warp the moisture lookup position so climate zones become wavy, not straight latitude lines
- R9. Climate influence blended at ~30-35% with global coverage — noise drives shape, climate nudges placement

**Rendering**

- R10. Beer-Lambert exponential opacity: `alpha = 1 - exp(-density * thickness_param)` instead of linear alpha — thin clouds translucent, thick clouds opaque
- R11. Self-shadowing: sample cloud density offset toward sun direction to approximate light depth — creates bright sun-facing tops and darker shadow sides
- R12. Cloud color varies: bright warm-white (lit) to blue-grey (shadow), not flat uniform white
- R13. Optional: Henyey-Greenstein forward scattering for bright edges when backlit (silver lining)

**Scope Control**

- R14. Independent cloud seed for pattern variation
- R15. Cloud coverage = 0 produces zero visual change (early-out for performance)
- R16. Drop cyclone/spiral features — focus on getting base cloud quality right first

## Success Criteria

- Moving the coverage slider from 0.0 to 1.0 shows a smooth, continuous increase in cloud area with no cliff effects
- At coverage ~0.5, the planet shows a plausible mix of clear sky and cloud formations with visible internal texture
- At coverage 1.0, most of the planet is covered but with density variation (not flat white)
- No visible latitude banding — cloud patterns look organic and spatially varied
- Clouds have visible depth: bright sun-lit tops, darker shadowed undersides, thin translucent edges
- Different seeds produce visually distinct cloud patterns

## Scope Boundaries

- NOT adding cyclone/spiral storm patterns (future enhancement)
- NOT rendering volumetric 3D clouds — 2D shell only
- NOT adding cloud shadows on the surface (future enhancement)
- NOT exporting clouds as a separate texture map (future)
- NOT simulating cloud dynamics/weather — static snapshot per seed

## Key Decisions

- **Schneider remap over naive thresholding**: The `remap(noise, 1-cov, 1, 0, 1) * cov` technique naturally produces lighter small clouds and denser large clouds, solving the slider cliff problem. This is the industry standard (Horizon Zero Dawn, Skybolt).
- **Domain warping for both noise and climate**: Warping the noise input creates organic shapes; warping the climate lookup breaks latitude bands. Two separate warp applications.
- **Beer-Lambert over linear alpha**: Exponential opacity creates the critical visual distinction between thin translucent clouds and thick opaque clouds. Linear alpha makes everything look like the same semi-transparent overlay.
- **Self-shadowing as primary depth cue**: Even a single density sample offset toward the sun transforms flat white into dimensional cloud masses. This is the #1 technique for visual quality.
- **Drop cyclones**: The spiral storm code was causing most visual artifacts (geometric lollipop patterns). Clean removal lets us focus on getting the foundation right.

## Dependencies / Assumptions

- Existing `snoise()` simplex noise function available (from noise.wgsl)
- Existing `compute_moisture()` and `compute_temperature()` provide climate data
- PreviewUniforms already has cloud_coverage, cloud_seed, cloud_altitude fields
- Cloud UI (coverage slider, seed control) already exists in app.rs

## Outstanding Questions

### Deferred to Planning

- [Affects R11][Needs research] How many shadow samples give good quality without noticeable perf cost? Research suggests 1-3; test empirically.
- [Affects R1][Technical] What domain warp strength looks best? Research suggests 0.5-0.8; tune during implementation.
- [Affects R3][Technical] Exact base frequency — 5.0 is the starting point but may need adjustment for this planet's scale.

## Next Steps

→ `/ce:plan` for structured implementation planning
