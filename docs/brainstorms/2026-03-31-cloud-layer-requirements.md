---
date: 2026-03-31
revised: 2026-07-10
topic: cloud-layer
---

# Shallow Volumetric Cloud Layer

## Summary

Replace the flat cloud shells with an interactive shallow volume for Earth-like rocky planets. One GPU-generated weather field will drive cloud depth, lighting, surface shadows, preview, and export.

---

## Problem Frame

The current clouds read as painted overlays. Density is evaluated on two infinitesimally thin shells, lighting does not integrate through cloud depth, and large noise regions produce broad grey sheets. Cloud placement is also disconnected from the moisture and pressure fields already available to the renderer.

Preview and export use different cloud algorithms, so the authored result is not the exported result. Repeated visual tuning cannot resolve these structural limitations.

---

## Key Flows

- F1. Interactive planet preview
  - **Trigger:** The user enables clouds or changes cloud, climate, terrain, or lighting controls.
  - **Steps:** The weather state updates, the bounded cloud volume is rendered, and lighting and shadows respond to the same density field.
  - **Outcome:** Orbiting and editing remain interactive while clouds show visible depth.
  - **Covered by:** R1-R12, R15
- F2. Cloud export
  - **Trigger:** The user exports cloud-related maps.
  - **Steps:** Export samples the authored weather state and produces maps consistent with the preview.
  - **Outcome:** Exported coverage and major formations match the visible planet.
  - **Covered by:** R13-R14

---

## Requirements

**Cloud volume**

- R1. Low clouds occupy a bounded atmospheric layer with spatially varying base altitude and thickness rather than a single spherical shell.
- R2. Preview rendering integrates density through the cloud layer using 6-10 samples, with empty-space early exit where practical.
- R3. The vertical density profile produces soft bases and tops, dense cores, broken edges, and visible parallax at the limb.
- R4. High cirrus remains a distinct, thinner layer with sparse fibrous formations rather than a second opaque shell.

**Weather state and shape**

- R5. A persistent weather field controls local coverage, cloud base, thickness, and cloud character.
- R6. Weather placement responds to atmospheric moisture, pressure convergence and divergence, temperature, terrain lift, rain shadows, latitude, and season without tracing coastlines or forming rigid latitude bands.
- R7. Cloud formations contain coherent clear regions, fronts, cells, and broken systems across multiple spatial scales; additive detail must not create a global grey veil.
- R8. Explicit storms remain localized additions to the weather field and must not dominate unrelated cloud systems.
- R9. Coverage changes produce a smooth increase in occupied cloud area across the full control range, with zero coverage producing no cloud contribution.

**Lighting and integration**

- R10. Opacity uses integrated optical depth so thin edges remain translucent and dense cores become opaque.
- R11. Direct lighting accounts for cloud self-shadowing and forward scattering, producing bright sun-facing regions, darker interiors, and restrained silver linings.
- R12. Surface cloud shadows use the same density field and sun direction as visible clouds, with soft edges appropriate to cloud altitude and thickness.

**Consistency and controls**

- R13. Preview and export derive from the same weather state and density definition.
- R14. Export provides both a ready-to-use integrated optical-depth map and reconstruction channels for coverage, cloud base, thickness, cloud character, and cirrus while preserving the major formations visible in preview.
- R15. Existing coverage, seed, opacity, storm, wind, layer visibility, and season controls remain functional and deterministic.

**Performance and validation**

- R16. Weather generation and cloud rendering remain GPU-resident during interactive use; no per-frame GPU-to-CPU readback is allowed.
- R17. At the standard preview resolution, cloud-enabled orbiting and control changes remain responsive, targeting at least 30 frames per second on the project's baseline GPU.
- R18. Deterministic reference renders cover clear, scattered, overcast, storm, backlit, limb, and polar views.

---

## Acceptance Examples

- AE1. **Covers R1-R4, R10-R11.** Given scattered clouds and a grazing camera angle, when the planet is rotated, cloud masses show bounded depth, internal shading, and parallax without a visible shell edge.
- AE2. **Covers R6-R9.** Given Earth-like climate parameters, when coverage is set near 0.5, the result contains coherent cloudy and clear weather regions without continent outlines, horizontal bands, or a global grey veil.
- AE3. **Covers R9, R15-R17.** Given clouds are enabled, when coverage is changed from zero through intermediate values to one, occupied area increases continuously, zero has no cloud cost beyond the early-out path, and interaction remains responsive.
- AE4. **Covers R11-R12.** Given a low sun angle, when a dense cloud system crosses the lit hemisphere, its bright edge, shaded interior, and soft surface shadow move consistently with the sun.
- AE5. **Covers R13-R15.** Given a fixed seed and parameters, when cloud maps are exported, the optical-depth map aligns with the preview and the reconstruction channels describe the same major systems and clear regions.

---

## Success Criteria

- Clouds no longer read as textures pasted onto the planet in normal, limb, or backlit views.
- Scattered and partly cloudy settings show recognizable weather systems with varied thickness and genuinely clear gaps.
- Lighting supplies depth without turning clouds uniformly grey or uniformly white.
- Preview and export agree for the same seed and controls.
- The standard preview remains interactive with clouds enabled.
- Planning can trace every implementation unit and validation render to an R-ID.

---

## Scope Boundaries

- Earth-like rocky planets are the first target.
- The cloud volume is shallow and planet-scale, not a general 3D voxel atmosphere.
- Ground-level and flight-level cloud rendering are deferred.
- Full atmospheric fluid dynamics and time-evolving numerical weather prediction are deferred.
- The geological terrain pipeline is not replaced in this phase.
- Non-rocky, icy, and gas-giant cloud regimes are deferred.

---

## Key Decisions

- **Shallow volume over layered shells:** A bounded ray-marched layer fixes silhouettes, parallax, and optical depth without the cost of a full 3D atmosphere.
- **Interactive quality first:** A short ray march is preferred over maximum close-up quality because planet editing must remain responsive.
- **Shared weather state:** Preview, shadows, and export must not maintain separate procedural interpretations of clouds.
- **GPU residency:** Cloud state stays on the GPU so additional depth does not introduce readback stalls.

---

## Dependencies / Assumptions

- The existing cubemap terrain, climate inputs, wind field, and preview renderer remain available during this phase.
- Planet radius can be derived well enough to express plausible cloud altitudes relative to the surface.
- The baseline performance target will be measured on the same hardware and preview resolution used for current visual acceptance.

---

## Outstanding Questions

### Deferred to Planning

- [Affects R2, R17][Technical] Determine the smallest sample count and empty-space strategy that meet both visual and frame-time targets.
- [Affects R5-R6][Technical] Select weather-field channels and update triggers without duplicating existing wind and climate data.
- [Affects R11][Needs research] Calibrate the minimum light sampling and phase approximation needed for convincing planet-scale clouds.
- [Affects R18][Technical] Define the baseline GPU, render resolution, camera poses, and image comparison thresholds.
