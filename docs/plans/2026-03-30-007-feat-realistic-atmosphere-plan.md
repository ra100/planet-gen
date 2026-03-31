---
title: "feat: Realistic atmosphere rendering with ray-marched scattering and AA"
type: feat
status: active
date: 2026-03-30
---

# Realistic Atmosphere Rendering

## Overview

Replace the current 3-line single-scatter Rayleigh approximation with a proper ray-marched atmosphere that produces visible atmospheric shell beyond the planet edge, wavelength-dependent scattering (blue sky + red sunsets), and natural antialiasing at the sphere boundary. Add UI toggle to enable/disable.

## Problem Frame

The current atmosphere implementation (`preview_cubemap.wgsl:496-508`) blends a fixed blue color at the planet limb using `pow(1.0 - NdotV, 3.0)`. This produces:
- No atmosphere visible beyond the planet edge (hard cutoff at `r2 > 1.0`)
- No sunset/sunrise reddening at the terminator
- No optical depth variation (atmosphere is same color everywhere)
- Hard aliased edge where the sphere meets the background

A ray-marched atmosphere shell naturally solves all four issues: it extends beyond the solid surface, accumulates wavelength-dependent optical depth, produces red at the terminator, and creates a soft edge that acts as natural antialiasing.

## Requirements Trace

- R1. Atmosphere shell visible as thin glowing ring beyond planet edge
- R2. Wavelength-dependent Rayleigh scattering (blue overhead, red at terminator/limb)
- R3. Optical depth integration along view ray (thicker atmosphere = more scattering)
- R4. Smooth antialiased edge at planet-to-space boundary
- R5. UI toggle to enable/disable atmosphere rendering
- R6. Physics-derived atmosphere density used as default (from `DerivedProperties::atmosphere_strength`)
- R7. No precomputed LUTs — keep it real-time in the fragment shader
- R8. Performance: acceptable frame rate for interactive preview (<16ms per frame)

## Scope Boundaries

- NOT implementing full Bruneton/Hillaire precomputed atmospheric model (that's a future effort)
- NOT adding Mie scattering (aerosols/dust) — Rayleigh only for v1
- NOT rendering clouds — separate feature
- NOT modifying the export pipeline shaders — preview only
- NOT adding atmosphere to `preview.wgsl` (legacy shader) — `preview_cubemap.wgsl` only

## Key Technical Decisions

- **Ray-marching in fragment shader**: 8-12 steps along view ray through atmosphere shell. No LUTs needed, acceptable cost for a preview window. The atmosphere shell is thin (1.0 to ~1.04 planet radii) so few steps suffice.

- **Two-sphere intersection**: Ray-sphere test against outer atmosphere radius (1.0 + atmosphere_height) in addition to planet surface (1.0). Rays that miss the planet but hit the atmosphere shell render the atmospheric glow ring.

- **Wavelength-dependent scattering coefficients**: Use standard Rayleigh coefficients (λ⁻⁴ dependence): `β_R = (5.5e-6, 13.0e-6, 22.4e-6)` for RGB. Blue scatters ~4× more than red, producing blue sky overhead and red/orange at the terminator where blue is scattered away.

- **Exponential density falloff**: `density(h) = exp(-h / H₀)` with scale height H₀ derived from `atmosphere_height` uniform. Denser near surface, thins exponentially.

- **Antialiasing via atmosphere**: The atmosphere shell extends beyond the planet, creating a natural soft transition from surface → atmosphere → space. No separate AA pass needed. For the background-to-atmosphere boundary, use a 1-2 pixel smoothstep on the outer sphere intersection distance.

## Implementation Units

- [ ] **Unit 1: Two-sphere ray intersection**

  **Goal:** Replace single `intersect_sphere` with a function that returns both planet surface hit and atmosphere shell entry/exit distances.

  **Requirements:** R1, R4

  **Dependencies:** None

  **Files:**
  - Modify: `src/shaders/preview_cubemap.wgsl`

  **Approach:**
  - Add `intersect_atmosphere(uv, planet_radius, atm_radius) -> (hit_planet, t_near, t_far, planet_t)` that tests ray against both spheres
  - When ray hits atmosphere but misses planet: render atmosphere-only (the glow ring)
  - When ray hits both: render surface + atmosphere in front of camera
  - Apply 1-2 pixel `smoothstep` at the outer atmosphere boundary for soft edge (R4)
  - Keep `atmosphere_height` uniform (already exists, currently unused)

  **Patterns to follow:**
  - Current `intersect_sphere` at `preview_cubemap.wgsl:43-49`
  - Standard ray-sphere intersection: solve quadratic `t² + 2bt + c = 0`

  **Test scenarios:**
  - Happy path: Earth-like planet shows blue limb ring extending beyond the surface
  - Edge case: atmosphere_density = 0.0 → no atmosphere rendered (same as current background)
  - Edge case: very thin atmosphere (low-mass planet) → barely visible ring
  - Edge case: very thick atmosphere → wide visible ring

  **Verification:** Planet renders with visible atmospheric ring beyond the edge, smooth transition to background

- [ ] **Unit 2: Ray-marched optical depth with Rayleigh scattering**

  **Goal:** Replace the simple `pow(thickness, 3)` blend with proper optical depth integration along the view ray through the atmosphere shell.

  **Requirements:** R2, R3

  **Dependencies:** Unit 1

  **Files:**
  - Modify: `src/shaders/preview_cubemap.wgsl`

  **Approach:**
  - Add `ray_march_atmosphere(ray_origin, ray_dir, t_start, t_end, sun_dir, steps) -> (in_scatter, transmittance)` function
  - March 8-12 steps from `t_start` to `t_end`
  - At each step: compute height above surface, density via `exp(-h/H0)`, accumulate optical depth
  - At each step: compute in-scattered light from sun direction using Rayleigh phase function `3/(16π) * (1 + cos²θ)`
  - Use wavelength-dependent scattering coefficients: `β_R = vec3(5.5, 13.0, 22.4) * 1e-6` (normalized for planet-scale)
  - For surface pixels: march from camera entry into atmosphere to surface hit
  - For atmosphere-only pixels: march from entry to exit of atmosphere shell
  - Combine: `final = surface_color * transmittance + in_scatter`

  **Patterns to follow:**
  - Research doc on Rayleigh scattering: phase function, optical depth, Beer-Lambert transmittance
  - Current atmosphere code at `preview_cubemap.wgsl:496-508` (replace entirely)

  **Test scenarios:**
  - Happy path: planet shows blue atmosphere that transitions to reddish near the terminator
  - Happy path: night side has no atmosphere glow (sun behind planet)
  - Edge case: sun at 90° to view → atmosphere ring bright on sunlit side, dark on night side
  - Edge case: atmosphere_density = 0.0 → transmittance = 1.0, in_scatter = 0.0 (no effect)

  **Verification:** Visible color shift from blue (overhead) to orange/red (terminator). Atmosphere glow ring shows sun-dependent illumination.

- [ ] **Unit 3: UI toggle and atmosphere density slider**

  **Goal:** Add UI controls to enable/disable atmosphere and adjust its strength.

  **Requirements:** R5, R6

  **Dependencies:** Unit 1

  **Files:**
  - Modify: `src/app.rs`

  **Approach:**
  - Add `show_atmosphere: bool` field to `PlanetGenApp` (default: `true`)
  - Set `atmosphere_density` to `self.derived.atmosphere_strength` when enabled, `0.0` when disabled
  - Add checkbox "Atmosphere" in the Visual Controls section (near the Relief slider)
  - Toggling triggers `needs_render = true`
  - Keep `atmosphere_height` derived from planet physics: `0.02 + 0.02 * atmosphere_strength` (2-4% of planet radius)

  **Patterns to follow:**
  - Existing slider pattern at `app.rs:371-377` (Relief slider)
  - `view_mode` toggle pattern for checkbox style

  **Test scenarios:**
  - Happy path: checkbox toggles atmosphere visibility on/off
  - Happy path: default is on for Earth-like planets
  - Edge case: airless planets (atmosphere_strength ≈ 0) → atmosphere effectively invisible even when "on"

  **Verification:** Checkbox toggles atmosphere on/off in preview. Atmosphere strength matches physics model.

- [ ] **Unit 4: Wire atmosphere_height uniform**

  **Goal:** Ensure the `atmosphere_height` uniform carries a meaningful value for the ray-marcher to use as scale height.

  **Requirements:** R3, R6

  **Dependencies:** Unit 3

  **Files:**
  - Modify: `src/app.rs`
  - Modify: `src/preview.rs` (if `atmosphere_height` default needs updating)

  **Approach:**
  - Set `atmosphere_height` in `preview_uniforms()` to a physics-derived value: `0.02 + 0.02 * self.derived.atmosphere_strength`
  - This gives Earth-like planets ~3.4% radius atmosphere shell (≈ 8.5km scale height / 6371km radius ≈ 0.0013, but exaggerated for visual clarity at preview scale)
  - Airless planets get ~2% (barely visible)

  **Patterns to follow:**
  - Current `atmosphere_height: 0.0` at `app.rs:107`

  **Test scenarios:**
  - Happy path: Earth-like planet has visible atmosphere shell
  - Edge case: low-mass planet has thin atmosphere shell

  **Verification:** Atmosphere thickness varies visibly between high-atmosphere and low-atmosphere planets.

## System-Wide Impact

- **Interaction graph:** Only `preview_cubemap.wgsl` fragment shader is modified. No callbacks, no middleware. `app.rs` adds a UI control. `preview.rs` uniform struct is unchanged (fields already exist).
- **Error propagation:** None — shader changes are self-contained. Worst case: visual artifact, no crash.
- **Performance:** Ray-marching 8-12 steps per fragment adds GPU cost. At preview resolution (256×256 per face → ~390K fragments for visible hemisphere), this is ~3-4M texture-free arithmetic operations — well within budget for interactive preview.
- **Unchanged invariants:** Export pipeline shaders (`albedo_map.wgsl`, `roughness_map.wgsl`, etc.) are NOT modified. Exported textures are unaffected.

## Risks & Dependencies

| Risk | Mitigation |
|------|------------|
| Ray-march too expensive for preview | Start with 8 steps, adjustable. Early-out when transmittance < 0.01 |
| Scattering coefficients produce unrealistic colors | Use well-established Rayleigh coefficients from research. Tune scale factor for planet-scale rendering |
| Atmosphere too subtle or too strong at default density | Use `atmosphere_strength` from physics model. User can toggle off if unwanted |

## Sources & References

- Current atmosphere: `src/shaders/preview_cubemap.wgsl:496-508`
- Rayleigh scattering research: `docs/research/` (phase function, optical depth, Beer-Lambert)
- PreviewUniforms: `src/preview.rs:20` (atmosphere_density, atmosphere_height already defined)
- App UI: `src/app.rs:371-377` (slider pattern to follow)
