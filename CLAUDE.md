# CLAUDE.md

## Project Overview

Planet Gen is a GPU-accelerated procedural planet generator built with Rust + wgpu. It produces physically plausible planets with biomes, atmospheres, oceans, terrain, and shallow volumetric clouds from user-controlled parameters, with bounded tiled export up to 8K.

## Tech Stack

- **Language:** Rust
- **GPU:** wgpu (WebGPU API)
- **UI:** egui / eframe
- **Shaders:** WGSL (compute + fragment)
- **Build:** `cargo build` / `cargo run`
- **Tests:** `cargo test --lib`

## Repository Structure

```
src/
  app.rs          — Main application, UI, parameter sliders
  preview.rs      — Preview renderer, uniforms, persistent-GPU field bindings
  weather.rs      — Revisioned front/back cloud mass and geometry cubemaps
  export.rs       — Tiled EXR export, including six-channel cloud reconstruction output
  planet.rs       — PlanetParams, DerivedProperties, physics model
  lib.rs          — Module declarations
  shaders/
    preview_cubemap.wgsl — Fragment shader: terrain, climate, bounded cloud volume
    weather_field.wgsl   — Cloud mass and geometry field generation
    cloud_density.wgsl   — Shared preview/export density definitions
    cloud_export.wgsl    — Direct optical-depth and reconstruction-channel export
docs/
  research/       — Research documents (planetary science, GPU techniques)
  brainstorms/    — Requirements documents
  plans/          — Implementation plans (active plans for upcoming work)
Plans.md          — Master progress tracker across all phases
```

## Architecture

### Preview Pipeline

User parameters → `PlanetParams` → `DerivedProperties` (physics model) → revisioned `WeatherSnapshot` → persistent weather cubemaps + `PreviewUniforms` → fragment shader renders a lit sphere with:

- Continental structure (low-freq noise + domain warping)
- Multi-octave fBm terrain detail (8-12 octaves)
- Hadley cell atmospheric circulation → moisture
- Rain shadows from wind-terrain interaction
- Whittaker biome lookup (temperature × moisture)
- Altitude zonation (forest → alpine → rock → snow)
- Ocean and polar ice rendering
- Crater stamping
- Bounded ray-marched cloud volume, lighting, and surface shadows

Export repeats the weather sequence from an immutable snapshot at bounded field resolution. `clouds.exr` contains direct optical depth plus reconstruction channels; preview visibility and opacity do not affect export.

### Key Physical Models

- Frost line → planet type classification
- Continuous tectonics factor from Rayleigh number estimate
- Atmosphere strength from escape velocity + greenhouse feedback
- MMSN isolation mass for plausibility warnings
- Bimodal elevation (continental +0.3, oceanic -0.4)

### User Parameters

**Physics:** star distance (AU), mass (M_Earth), metallicity, axial tilt, rotation period, seed
**Visual overrides:** continental scale, water loss

## Workflow

### After completing implementation work

1. Update `Plans.md` — mark completed tasks with `cc:完了 [commit_hash]`
2. If a plan in `docs/plans/` is fully completed, set its frontmatter `status: completed`
3. Completed plan files can be deleted once their tasks are tracked in Plans.md

### Planning new features

1. Write requirements in `docs/brainstorms/`
2. Create implementation plan in `docs/plans/` with frontmatter (title, type, status, date, origin)
3. Add a new phase section to `Plans.md` linking to the plan

### Commits

Follow existing commit message style with gitmoji prefixes: ✨ feature, 🐛 fix, ♻️ refactor, 📋 plan, 📝 docs
