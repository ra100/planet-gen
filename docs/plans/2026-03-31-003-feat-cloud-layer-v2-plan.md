---
title: "feat: Rewrite cloud layer with Schneider remap, domain warping, and self-shadowing"
type: feat
status: active
date: 2026-03-31
origin: docs/brainstorms/2026-03-31-cloud-layer-requirements.md
---

# Cloud Layer v2: Research-Based Rewrite

## Overview

Complete rewrite of the cloud density function and cloud rendering in `preview_cubemap.wgsl`. The current implementation suffers from slider cliff effects, flat grey appearance, latitude banding, and lack of texture. This plan applies proven techniques from Schneider (HZD), Quilez, and Skybolt research to fix all four issues.

## Problem Frame

The cloud layer was implemented iteratively without a solid technical foundation. Four critical defects persist despite multiple fix attempts (see origin: `docs/brainstorms/2026-03-31-cloud-layer-requirements.md`):
1. Slider cliff — coverage threshold operates on a narrow density distribution
2. Flat appearance — linear alpha and uniform color
3. Latitude banding — noise × moisture = horizontal bands
4. No texture — insufficient detail and no depth cues

## Requirements Trace

- R1. Domain-warped fBm noise for organic shapes
- R2. 5 octaves minimum for wispy detail
- R3. Base frequency ~5.0 (6-10 visible cloud systems)
- R4. Linear visual response on coverage slider
- R5. Schneider remap: `remap(noise, 1-cov, 1, 0, 1) * cov`
- R6. No cliff effects
- R7. Climate controls threshold, NOT multiplied with density
- R8. Domain-warp moisture lookup to break latitude bands
- R9. Climate influence ~30-35%, noise drives shape
- R10. Beer-Lambert exponential opacity
- R11. Self-shadowing via light-direction density offset
- R12. Cloud color: warm-white (lit) to blue-grey (shadow)
- R13. Optional HG forward scattering (silver lining)
- R14. Independent cloud seed
- R15. Coverage=0 early-out
- R16. Drop cyclone/spiral features

## Scope Boundaries

- NOT adding cyclones (future enhancement)
- NOT volumetric 3D — 2D shell only
- NOT adding cloud shadows on surface
- NOT modifying uniforms or UI (already exist from v1)

## Key Technical Decisions

- **Schneider remap over threshold**: The `remap(noise, 1-cov, 1, 0, 1) * cov` technique naturally produces lighter small clouds and denser large clouds. The trailing `* coverage` term makes small clouds thinner at low coverage, producing a near-linear visual response. (see origin + `docs/research/cloud-layer-rendering.md` section 1.3)

- **Domain warping for BOTH noise and climate**: Two separate warp applications. (1) Warp the fBm sample position to create organic shapes (Quilez technique). (2) Warp the moisture lookup position to make climate zones wavy. This is the critical technique — it prevents both fBm uniformity AND latitude banding simultaneously.

- **Beer-Lambert opacity over linear alpha**: `alpha = 1 - exp(-density * thickness)` creates thin translucent clouds and thick opaque clouds. Linear alpha makes everything look like the same overlay.

- **Self-shadowing as primary depth cue**: Sample cloud density at a position offset toward the sun. The difference approximates how much cloud the light passes through. This single technique transforms flat discs into dimensional masses.

- **Drop cyclones entirely**: Remove the storm loop and all spiral-related code. The cyclone implementation was causing geometric artifacts (lollipop spirals) and dominating the visual. Focus on getting the foundation right.

## High-Level Technical Design

> *This illustrates the intended approach and is directional guidance for review, not implementation specification. The implementing agent should treat it as context, not code to reproduce.*

**Cloud density pipeline:**
```
sphere_pos → domain_warp(pos, seed) → warped_pos
warped_pos → fBm(5 octaves, freq ~5.0, lacunarity 2.1, gain 0.52) → raw_noise [0,1]

sphere_pos → climate_warp(pos) → warped_climate_pos
warped_climate_pos → compute_moisture() → moisture → normalize to [0,1]
moisture_norm → blend with global_coverage at 35% → local_coverage

raw_noise + local_coverage → schneider_remap(noise, 1-local_cov, 1, 0, 1) * local_cov → density [0,1]
```

**Cloud rendering pipeline:**
```
density → Beer-Lambert: alpha = 1 - exp(-density * 4.0)
density + light_dir → self_shadow: sample density at pos + light_dir * offset → shadow term
shadow term → color blend: warm-white (lit) ↔ blue-grey (shadowed)
optional: HG phase function → silver lining on backlit edges
final: mix(surface_color, cloud_color, cloud_alpha)
```

## Implementation Units

- [ ] **Unit 1: Rewrite cloud density function**

  **Goal:** Replace `compute_cloud_density()` with research-based implementation using domain-warped fBm, Schneider remap, and climate threshold modulation.

  **Requirements:** R1, R2, R3, R4, R5, R6, R7, R8, R9, R14, R15, R16

  **Dependencies:** None

  **Files:**
  - Modify: `src/shaders/preview_cubemap.wgsl`

  **Approach:**
  - Delete the entire existing `compute_cloud_density()` function and the `cloud_seed_hash()` helper
  - Remove all cyclone/storm loop code (R16)
  - Add a `remap(value, old_min, old_max, new_min, new_max)` utility function (check if one already exists in the shader)
  - New `compute_cloud_density(sphere_pos, height)` function:
    1. Early-out when `cloud_coverage <= 0` (R15)
    2. Build seed offset from `uniforms.cloud_seed` (already pre-hashed on CPU via `seed_to_offset()`)
    3. Domain-warp the sample position: 3 snoise calls at low frequency, warp strength ~0.6 (R1)
    4. 5-octave fBm on the warped position, base freq ~5.0, lacunarity 2.1, gain 0.52 (R2, R3)
    5. Remap noise to [0,1]
    6. Domain-warp the climate lookup position: 3 snoise calls to make latitude zones wavy (R8)
    7. Sample moisture at warped position, normalize to [0,1] (R7)
    8. Blend moisture with global coverage at ~35%: `mix(global_cov, moisture_norm, 0.35)` (R9)
    9. Apply Schneider remap: `remap(noise, 1 - local_cov, 1, 0, 1) * local_cov` (R5)
    10. Optional power adjustment on slider: `pow(coverage_slider, 0.8)` for linearity (R4, R6)

  **Patterns to follow:**
  - `compute_moisture()` for climate-driven spatial variation with existing tilt handling
  - Noise sampling patterns using `snoise()` across the shader
  - `smooth_step()` helper already defined in the shader

  **Test scenarios:**
  - Happy path: Build succeeds with new function, planet renders without GPU errors
  - Happy path: Coverage=0.5 shows approximately 40-60% of visible sphere with cloud patches
  - Edge case: Coverage=0.0 → no cloud density anywhere (early-out)
  - Edge case: Coverage=1.0 → most of the planet has cloud density > 0
  - Edge case: Coverage=0.1, 0.2, 0.3, 0.4 each show progressively more clouds without cliff jumps
  - Edge case: Different cloud_seed values produce visually distinct patterns

  **Verification:** Clouds distribute organically across the planet without visible latitude bands. Coverage slider produces smooth, approximately linear visual response from clear to overcast.

- [ ] **Unit 2: Rewrite cloud rendering with Beer-Lambert and self-shadowing**

  **Goal:** Replace cloud rendering block in `fs_main` with Beer-Lambert opacity, self-shadowing, and color variation.

  **Requirements:** R10, R11, R12, R13

  **Dependencies:** Unit 1

  **Files:**
  - Modify: `src/shaders/preview_cubemap.wgsl`

  **Approach:**
  - Replace the existing cloud rendering block in `fs_main` (the `if (cloud_density > 0.03)` section)
  - Beer-Lambert opacity: `cloud_alpha = 1.0 - exp(-density * thickness_param)` where thickness_param ~4.0 (R10)
  - Self-shadowing: sample `compute_cloud_density()` at `normalize(cloud_world + sun_dir * 0.03)`, compute shadow term as `exp(-shadow_density * 2.5)` (R11)
  - Cloud color: blend between shadow color `(0.55, 0.58, 0.65)` and lit color `(0.95, 0.95, 0.93)` based on shadow term (R12)
  - Optional: HG forward scattering for silver lining when sun is behind the cloud, using the existing `henyey_greenstein()` function already in the shader (R13)
  - Composite: `mix(lit_surface, cloud_color, cloud_alpha)`

  **Patterns to follow:**
  - `henyey_greenstein()` already defined in the shader for Mie scattering
  - `ray_march_atmosphere()` for sun direction and lighting patterns
  - Existing cloud rendering block structure in `fs_main`

  **Test scenarios:**
  - Happy path: Clouds show visible brightness variation — bright sun-facing surfaces, darker undersides
  - Happy path: Thin cloud edges are translucent (surface visible through), thick cores are opaque
  - Happy path: Night-side clouds are dark/barely visible
  - Edge case: Coverage=0.0 → no cloud rendering (early-out in density function)
  - Edge case: Planet with no atmosphere → clouds still render if coverage > 0
  - Integration: Self-shadow sampling calls compute_cloud_density which must work correctly from Unit 1

  **Verification:** Clouds have visible depth — bright tops, blue-grey shadows. Thin clouds are translucent. No flat white/grey uniform appearance. Clouds integrate correctly with atmosphere rendering (clouds before atmosphere).

## System-Wide Impact

- **Interaction graph:** Only `preview_cubemap.wgsl` fragment shader changes. No uniform, UI, Rust, or pipeline changes needed — all infrastructure exists from the v1 implementation.
- **Error propagation:** Shader-only visual feature. Worst case: visual artifact, no crash.
- **Unchanged invariants:** Export pipeline, atmosphere rendering, surface coloring, UI controls all unchanged.
- **Performance:** Unit 1 adds ~8 snoise calls (5 fBm + 3 warp) + 3 climate warp calls = ~11 snoise total. Unit 2 adds 1 self-shadow sample (another full density evaluation) for ~22 snoise total per cloud fragment. This is higher than the current implementation but within the frame budget for a preview renderer. If performance is tight, reduce fBm to 4 octaves or skip the silver lining HG calculation.

## Risks & Dependencies

| Risk | Mitigation |
|------|------------|
| Self-shadow doubles snoise cost per fragment | Self-shadow is a single offset sample, not a loop. Can reduce cloud fBm to 4 octaves if needed |
| Domain warp strength needs tuning | Research suggests 0.5-0.8; start at 0.6 and adjust visually |
| Beer-Lambert thickness param affects appearance | Start at 4.0 per research; parameter is a single constant, easy to tune |
| Schneider remap may produce different density distribution than expected | The `* coverage` trailing term is critical — ensures small clouds are thinner at low coverage |

## Sources & References

- **Origin document:** [docs/brainstorms/2026-03-31-cloud-layer-requirements.md](docs/brainstorms/2026-03-31-cloud-layer-requirements.md)
- **Research document:** [docs/research/cloud-layer-rendering.md](docs/research/cloud-layer-rendering.md)
- **Schneider/HZD**: Remap-based coverage, Beer-Powder lighting
- **Quilez**: Domain warping, 2D cloud layers, self-shadowing
- **Skybolt**: Planetary-scale cloud rendering patterns
- Existing shader: `src/shaders/preview_cubemap.wgsl` (compute_moisture, henyey_greenstein, ray_march_atmosphere)
- Existing uniforms: `src/preview.rs` (cloud_coverage, cloud_seed, cloud_altitude already present)
- Existing UI: `src/app.rs` (coverage slider, seed control already present)
