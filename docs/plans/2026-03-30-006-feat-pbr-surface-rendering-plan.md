---
title: "feat: PBR surface rendering with normal mapping"
type: feat
status: active
date: 2026-03-30
---

# PBR Surface Rendering with Normal Mapping

## Overview

Replace the current flat Lambert shading with physically-based surface rendering. The planet should look like a space photograph — height-derived normals create visible terrain relief, ocean has specular highlights with Fresnel, and the lighting model uses Cook-Torrance BRDF. This is the foundation for later atmosphere and cloud layers.

## Problem Frame

The current preview uses `surface_color * (0.15 + 0.85 * ndotl)` — basic Lambert shading on the geometric sphere normal. No terrain detail is visible in the lighting; height only affects color. Ocean looks identical to land in terms of reflectance. The result looks like a painted ball rather than a physical object.

## Requirements Trace

- R1. Height-derived normal mapping: terrain relief visible in lighting (mountains cast micro-shadows, valleys are darker)
- R2. PBR lighting: Cook-Torrance BRDF with diffuse + specular components
- R3. Ocean specular: water surfaces show sun reflection (specular highlight) with Fresnel effect
- R4. Roughness-driven shading: rough terrain (desert, rock) looks matte; smooth surfaces (ice, ocean) look reflective
- R5. Rim lighting / limb darkening: edges of the sphere darken naturally from the viewing angle
- R6. No performance regression on rotation (rendering must stay fast — all in fragment shader)
- R7. Debug views continue to work (bypass PBR lighting, show raw data)
- R8. Designed for future atmosphere layer (shader structured so atmosphere pass can be added later)

## Scope Boundaries

- No atmosphere rendering in this pass (designed for, not implemented)
- No cloud layer
- No shadow mapping (self-shadowing from terrain — too expensive for a sphere ray-cast)
- No subsurface scattering (ice translucency etc.)
- No changes to the terrain compute or export pipeline

## Key Technical Decisions

- **Fragment-shader normal mapping over normal cubemap texture**: Compute normals from the height cubemap via central differences directly in the fragment shader. This avoids a second cubemap texture upload and stays consistent with the current single-texture architecture. Cost: 4 extra texture samples per pixel (trivial at preview resolution).
- **Simplified Cook-Torrance over full PBR**: Use GGX distribution + Schlick Fresnel + Lambert diffuse. Skip geometric attenuation term (Smith GGX) for simplicity — the visual difference is minimal at planet scale.
- **Roughness computed in-shader from biome data**: Reuse the existing roughness logic (temperature/moisture → roughness value) rather than adding a roughness cubemap texture. Keeps the uniform/texture interface unchanged.
- **Separate PBR from debug views**: The `if (view_mode > 0)` early-return stays. PBR only applies to the normal (view_mode == 0) render path.

## Open Questions

### Resolved During Planning

- **How to compute normals from cubemap?** Sample 4 neighbors of the height cubemap at ±step in the rotated sphere direction, compute cross-product tangent-space normal. Step size ~0.005 gives good detail without aliasing.
- **What roughness range?** Ocean: 0.05 (near-mirror). Ice: 0.15. Forest: 0.5. Desert/rock: 0.8. Same values as the roughness debug view and export shader.
- **Fresnel at planet scale?** Schlick approximation with F0=0.02 (water) gives realistic ocean darkening at steep angles and bright glancing reflections at limb.

### Deferred to Implementation

- Exact height_scale multiplier for normal perturbation — needs visual tuning
- Whether the specular highlight needs tonemapping to avoid HDR clipping on bright ocean reflections

## High-Level Technical Design

> *This illustrates the intended approach and is directional guidance for review, not implementation specification.*

```
Fragment shader flow (view_mode == 0):

1. Ray-sphere intersection → hit point, geometric normal
2. Rotate normal by view matrix → rotated position
3. Sample height cubemap at rotated position
4. Compute surface color (existing gradient_color pipeline)
5. [NEW] Compute terrain normal from height cubemap:
   - Sample height at 4 neighbors (±step along tangent directions)
   - Central differences → tangent-space perturbation
   - Perturb geometric normal → shading normal
6. [NEW] Compute roughness from temperature/moisture
7. [NEW] PBR shading:
   - Diffuse: Lambert * albedo * (1 - F)
   - Specular: GGX_D * Fresnel_Schlick / (4 * NdotV * NdotL)
   - Ambient: low constant * albedo
   - Result: ambient + (diffuse + specular) * NdotL
8. [NEW] Rim darkening: darken at glancing angles (1 - NdotV)^power
9. Output lit color
```

## Implementation Units

- [ ] **Unit 1: Height-derived normal mapping in fragment shader**

**Goal:** Compute terrain normals from the height cubemap so lighting reveals terrain relief

**Requirements:** R1, R6

**Dependencies:** None

**Files:**
- Modify: `src/shaders/preview_cubemap.wgsl`
- Test: `src/preview.rs` (existing tests)

**Approach:**
- Add a `compute_terrain_normal` function that takes the sphere position and geometric normal
- Sample height cubemap at 4 neighboring positions (tangent-space offsets along the sphere surface)
- Use central differences to compute height gradient → tangent-space normal
- Perturb the geometric normal by the terrain normal
- Use the perturbed normal for all lighting calculations
- The step size should be relative to sphere size (~0.005 works for 512 cubemap)

**Patterns to follow:**
- `src/shaders/normal_map.wgsl` — same central-difference technique but for flat textures
- Existing `textureSample(height_tex, height_sampler, ...)` calls in `compute_moisture`

**Test scenarios:**
- Happy path: Preview renders non-empty at 256px (existing test still passes)
- Happy path: Rotating the planet with a mountain range shows varying light/shadow on the terrain
- Edge case: Flat ocean areas should have near-geometric normals (no noise in normal)

**Verification:**
- `test_preview_renders_non_empty` still passes
- Visual: mountains and ridges show distinct light/shadow compared to flat plains

- [ ] **Unit 2: PBR lighting model (Cook-Torrance)**

**Goal:** Replace Lambert shading with physically-based BRDF

**Requirements:** R2, R3, R4, R5

**Dependencies:** Unit 1

**Files:**
- Modify: `src/shaders/preview_cubemap.wgsl`
- Test: `src/preview.rs`

**Approach:**
- Add in-shader roughness computation (reuse biome_roughness_value logic from roughness debug view)
- Implement GGX normal distribution function: `D = alpha^2 / (pi * (NdotH^2 * (alpha^2 - 1) + 1)^2)`
- Implement Schlick Fresnel: `F = F0 + (1 - F0) * (1 - HdotV)^5`
- F0 = 0.02 for dielectric (land), 0.04 for water (ocean)
- Combine: `specular = D * F / (4 * NdotV * NdotL + 0.001)`
- Diffuse: `(1 - F) * albedo / pi`
- Final: `ambient + (diffuse + specular) * NdotL * light_color`
- Add rim darkening: multiply result by `pow(NdotV, 0.15)` for subtle limb effect

**Patterns to follow:**
- Case 7u roughness debug view for roughness computation
- Standard Cook-Torrance PBR model (common in game engines)

**Test scenarios:**
- Happy path: Preview renders without NaN or all-black pixels
- Happy path: Ocean areas show a bright specular highlight near the light direction
- Happy path: Desert/rock areas look matte, ocean/ice look reflective
- Edge case: Pixels at the sphere edge (glancing angle) darken smoothly, no hard edge
- Edge case: Night side (NdotL < 0) is dark but not pure black (ambient light)

**Verification:**
- All existing tests pass
- Visual: clear specular dot on ocean, matte mountains, smooth limb darkening

- [ ] **Unit 3: Ensure debug views bypass PBR**

**Goal:** Debug visualization modes show raw data without PBR lighting

**Requirements:** R7

**Dependencies:** Unit 2

**Files:**
- Modify: `src/shaders/preview_cubemap.wgsl`

**Approach:**
- The existing `if (uniforms.view_mode > 0u)` early-return already bypasses lighting
- Verify that none of the new PBR code runs before the debug view check
- The debug views should continue to return unlit flat colors

**Test scenarios:**
- Happy path: View mode 1 (height) shows grayscale height without specular highlights
- Happy path: All 8 view modes produce distinct, non-black output

**Verification:**
- Switching between Normal and debug views works correctly
- Debug views show flat unlit data

- [ ] **Unit 4: Add PreviewUniforms fields for future atmosphere**

**Goal:** Reserve uniform space for atmosphere parameters without implementing atmosphere rendering

**Requirements:** R8

**Dependencies:** Unit 2

**Files:**
- Modify: `src/preview.rs` (PreviewUniforms struct)
- Modify: `src/shaders/preview_cubemap.wgsl` (Uniforms struct)
- Modify: `src/app.rs` (build_uniforms)
- Test: `src/preview.rs`

**Approach:**
- Repurpose the existing `_pad` fields or extend the uniform struct
- Add: `atmosphere_density: f32` (0.0 = no atmosphere, 1.0 = Earth-like), `atmosphere_height: f32` (scale height in planet radii)
- Default both to 0.0 so current behavior is unchanged
- The atmosphere rendering pass will be added in a future plan
- Add UI sliders in the sidebar (disabled/grayed out with "Coming soon" tooltip)

**Patterns to follow:**
- Existing uniform struct layout and 16-byte alignment convention
- Existing slider patterns in app.rs

**Test scenarios:**
- Happy path: Default atmosphere_density=0.0 produces identical output to before
- Edge case: Struct alignment valid — `cargo test` passes, no GPU validation errors

**Verification:**
- All existing tests pass with expanded uniforms
- App builds and runs without GPU errors

## System-Wide Impact

- **Interaction graph:** Only the preview fragment shader changes. Export pipeline (albedo_map.wgsl) is NOT affected — it generates unlit albedo for Blender materials.
- **Error propagation:** NaN in normal computation could produce black pixels — guard with `max(NdotL, 0.0)` and `max(NdotV, 0.001)`.
- **Unchanged invariants:** Export textures, terrain generation, and all debug views remain unchanged. The `build_uniforms()` function in app.rs is the only Rust code that needs updating (for new atmosphere fields).

## Risks & Dependencies

| Risk | Mitigation |
|------|------------|
| Normal mapping from cubemap creates seam artifacts at face boundaries | Linear cubemap filtering already handles this; test at face edges |
| PBR specular too bright on ocean (HDR clipping) | Clamp specular contribution or add simple tonemapping |
| Performance regression from extra texture samples | Only 4 extra samples per pixel; measured at negligible cost on existing hardware |

## Future Considerations

These materially affect the current design (shader structure) but are not implemented now:

- **Atmosphere layer**: Will add a second pass after surface shading — Rayleigh scattering haze, limb glow, sunset coloring. The shader is structured so this can be inserted between PBR result and final output.
- **Cloud layer**: Will need a second cubemap texture (cloud density). The bind group layout may need expanding (binding 3 for cloud cubemap). The uniform struct reserves space for atmosphere params to minimize future churn.
- **Gas giant mode**: Would replace the surface pipeline entirely with volumetric band rendering. Orthogonal to this plan — no design constraints needed now.

## Sources & References

- Related code: `src/shaders/preview_cubemap.wgsl`, `src/preview.rs`, `src/app.rs`
- PBR reference: Cook-Torrance BRDF with GGX distribution (standard real-time PBR model)
- Normal mapping from heightmap: same technique as `src/shaders/normal_map.wgsl`
