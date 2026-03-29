# Planet Gen

GPU-accelerated procedural planet generator for VFX. Produces physically plausible rocky planets with terrain, biomes, and oceans from a handful of input parameters.

Built with Rust, wgpu (WebGPU), and egui.

## Status

**Work in progress** — Phases 1-3 complete (project scaffold, terrain generation, planet physics). Preview renders a height-colored sphere with ocean/land coloring driven by planetary science rules.

## Quick Start

```
cargo run
```

Requires a GPU with Vulkan, Metal, or DX12 support (most GPUs from 2018+).

## Usage

The app opens with a sidebar of planet parameters and a 3D preview:

**Parameters:**
- **Distance (AU)** — Distance from the star (0.1-50). Affects planet type and temperature.
- **Mass (M⊕)** — Planet mass in Earth masses (0.01-10). Affects gravity, tectonics, terrain roughness.
- **[Fe/H]** — Stellar metallicity (-1 to 1). Shifts the frost line.
- **Tilt (°)** — Axial tilt (0-90).
- **Day (hours)** — Rotation period (1-1000).
- **Seed** — Random seed for terrain generation. Click the dice button to randomize.

**Preview:**
- Drag to rotate the planet
- Derived properties (planet type, tectonics, gravity, temperature, ocean coverage) update in real-time

## Architecture

- **Cube-sphere** with 6 faces, no pole pinching
- **GPU compute shaders** (WGSL) for terrain generation via multi-octave fBm
- **Planetary science rules** derive planet type, tectonic regime, atmosphere, gravity, temperature, and ocean coverage from input parameters
- **Ray-sphere preview** renderer with height-based coloring and diffuse lighting

## Project Structure

```
src/
  main.rs          — App entry point
  app.rs           — egui UI and app state
  gpu.rs           — wgpu device singleton
  planet.rs        — Planet physics and derived properties
  terrain.rs       — GPU terrain generation (fBm on cube-sphere)
  preview.rs       — Preview renderer (ray-sphere, render-to-texture)
  cube_sphere.rs   — Cube-to-sphere mapping
  noise.rs         — Simplex noise test harness
  compute.rs       — Gradient compute shader (hello world)
  shaders/
    terrain.wgsl   — fBm terrain generation
    preview.wgsl   — Planet preview rendering
    cube_sphere.wgsl — Cube-to-sphere mapping
    noise.wgsl     — 3D simplex noise
```

## Roadmap

See [Plans.md](Plans.md) for the full implementation plan.

- [x] Phase 1: Project scaffold & GPU hello world
- [x] Phase 2: Cube-sphere & noise generation
- [x] Phase 3: Planet physics & parameter derivation
- [ ] Phase 4: Biome & surface generation
- [ ] Phase 5: Tiled 8K generation & export
- [ ] Phase 6: Blender importer addon
- [ ] Phase 7: Polish & distribution

## License

MIT
