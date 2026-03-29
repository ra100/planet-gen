---
date: 2026-03-29
topic: planet-gen-core
---

# Planet Gen — Procedural Planet Asset Generator

## Problem Frame

Creating believable planet assets for sci-fi VFX shorts requires either hand-painting textures (slow, requires specialized skill) or using existing tools that lack physical plausibility. A procedural generator grounded in real planetary science would produce convincing, varied planets on demand — with direct Blender integration to eliminate export/import friction.

## Requirements

**Core Generation Pipeline**

- R1. Generate physically plausible rocky/terrestrial planets from a small set of input parameters: star distance, planet mass, metallicity, axial tilt, rotation period, and a random seed
- R2. Use planetary science rules (frost line, MMSN surface density, Rayleigh number for tectonic regime) to derive planet composition, surface features, and atmosphere type from the input parameters
- R3. Generate terrain via GPU compute: multi-octave fBm noise (8-12 octaves) on a cube-sphere
- R3a. [v1.1] Add GPU hydraulic erosion (grid-based, 20-50 iterations) as a post-processing pass on generated terrain
- R4. Assign biomes using a Whittaker/Koppen lookup table driven by latitude-based temperature, elevation lapse rate, and moisture (ocean proximity + rain shadow)
- R5. Place impact craters via simple stamp-based placement scaled by surface age. [v1.1] Upgrade to Poisson disk sampling for more physically accurate distribution

**Output Maps**

- R6. Generate the following PBR texture maps per planet: Albedo, Height/Displacement, Normal, Roughness, Ocean mask, Ice cap mask
- R6a. [v1.1] Add Emission map (city lights / volcanic glow) as an optional output
- R7. Target 8K resolution (8192x8192 per cube face) as the default. The tiled pipeline (512px tiles) inherently scales to higher resolutions by increasing tile count — no additional architecture work needed
- R8. Output textures as uncompressed EXR (height/displacement, 16-bit float) and PNG (albedo, normal, roughness, masks) for direct Blender consumption. KTX2/BCn compression is a future target for standalone/engine use only

**Standalone App with Live Preview**

- R9. Standalone native app (Rust + egui + wgpu) with a 3D preview window showing the planet on a sphere
- R10. Preview mode (256x256 per face) updates in under 1 second when parameters change, allowing interactive tweaking before committing to full generation
- R11. App provides parameter controls (star distance, mass, metallicity, tilt, rotation, seed) and a "Generate" button that produces full-resolution textures
- R12. Generated textures are written to disk as files (see R8 for formats)

**Blender Integration (lightweight importer)**

- R13. A pure-Python Blender addon (no native code, no PyO3) that imports generated texture files from disk
- R14a. Importer creates a cube-sphere mesh with proper UVs ("Create Planet" mode) or applies textures to a user-selected object ("Apply to Selected" mode)
- R14b. Importer wires loaded images into a Principled BSDF node tree with correct map assignments (albedo→Base Color, normal→Normal, roughness→Roughness, height→Displacement)
- R14c. Support Cycles and EEVEE rendering targets

**Architecture**

- R15. Standalone native application implemented in Rust using wgpu for GPU compute and egui for the UI
- R16. Cube-sphere representation with 6 faces, compatible with GPU cubemap sampling, no pole pinching
- R17. Tiled generation: each face subdivided into tiles (e.g., 16x16 tiles of 512px each for 8K) to stay within GPU memory limits
- R18. Full generation runs on a background thread with a progress bar in the UI; preview updates are synchronous (fast enough at 256x256)
- R19. GPU errors (OOM, device lost, unsupported GPU) are displayed in the UI rather than silently crashing
- R20. The wgpu device is initialized once at app startup and persists for the session (singleton pattern), avoiding repeated 200-800ms cold-start overhead

## Scope Boundaries

- **Not in v1:** Gas giants, ice giants, icy moons — rocky/terrestrial only
- **Not in v1:** Real-time rendering engine (atmospheric scattering, FFT oceans, virtual texturing, LOD) — Blender handles rendering
- **Not in v1:** Unreal Engine plugin — architecture supports it later but not targeted now
- **Not in v1:** Cloud layer generation — can be added as a future texture map
- **Not in v1:** Ring systems, multi-body systems, binary stars
- **Not in v1:** Hydraulic erosion, emission maps — deferred to v1.1 (see R3a, R6a)
- **Not in v1:** Artistic override controls (terrain bias, color warmth, ocean coverage %)
- **Not in v1:** Planet presets (Earth-like, Mars-like, etc.) — future convenience feature

## Success Criteria

- Given a set of input parameters, the tool produces a visually distinct, physically plausible rocky planet with coherent biomes, terrain, and surface features
- Generated textures render convincingly in Blender Cycles at 1080p and 4K output
- A new planet can be previewed in under 1 second and fully generated (8K) in under 30 seconds on a modern GPU (RTX 3080-class)
- The Blender addon installs and works without requiring the user to manually compile anything or install Rust

## Key Decisions

- **Rust + wgpu over Vulkan**: wgpu provides WebGPU-level abstraction with much less boilerplate than raw Vulkan, while running natively. Suitable for someone new to GPU programming. Falls back to Vulkan/Metal/DX12 under the hood.
- **Standalone app + lightweight Blender importer over native Blender addon**: Eliminates the hardest engineering problem (PyO3/maturin cross-compilation against Blender's bundled Python per OS × Blender version). The standalone app provides live preview via egui+wgpu. The Blender importer is pure Python — trivial to distribute and maintain across Blender versions.
- **Physics-first, art direction later**: v1 generates from physics rules. Artistic overrides (terrain bias, color warmth, ocean coverage) deferred to a future version. Users can hand-tweak textures in Blender post-generation.
- **Rocky planets only for v1**: Keeps scope focused. Gas giants are a fundamentally different rendering problem (no solid surface, banded clouds) and can be added later.
- **8K default, scalable architecture**: 8K is sufficient for most VFX shots. The tiled pipeline design inherently supports higher resolutions by increasing tile count.

## Dependencies / Assumptions

- User has a WebGPU-capable GPU (most GPUs from 2018+ via wgpu's Vulkan/Metal/DX12 backends)
- Blender 4.x+ (Python 3.11+, modern addon API)
- User has a WebGPU-capable GPU (most GPUs from 2018+ via wgpu's Vulkan/Metal/DX12 backends)
- Blender 4.x+ for the importer addon (standard Python addon, no native dependencies)
- wgpu-rs is stable enough for compute shader workloads (used in production by Firefox, Bevy engine, etc.)
- egui + eframe provides the UI framework (mature, integrates with wgpu natively)

## Outstanding Questions

### Resolve Before Planning

_All resolved._

### Deferred to Planning

- [Affects R15][Needs research] Cross-platform distribution strategy for the standalone Rust binary (GitHub releases with pre-built binaries? cargo install? Flatpak/Homebrew?)
- [Affects R9][Technical] egui 3D viewport integration — render wgpu cube-sphere preview inside an egui panel. Investigate egui-wgpu integration patterns
- [Affects R3a][Technical] Grid-based hydraulic erosion confirmed as approach for v1.1 (better GPU parallelism than particle-based). Need per-iteration cost estimate at 8K
- [Affects R11][Technical] Parameter widget design — ranges and units for each input (star distance in AU, mass in Earth masses, etc.), slider vs. numeric input, tooltips for non-obvious parameters like metallicity

## Next Steps

→ `/ce:plan` for structured implementation planning
