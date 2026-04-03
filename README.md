# Planet Gen

GPU-accelerated procedural planet generator for VFX. Produces physically plausible rocky planets with plate tectonics, terrain, biomes, climate, clouds, and oceans from a handful of input parameters.

Built with Rust, wgpu (WebGPU), and egui.

## Quick Start

```
cargo run
```

Requires a GPU with Vulkan, Metal, or DX12 support (most GPUs from 2018+).

## Features

- **Plate tectonics simulation** on HEALPix spherical grid — BFS flood-fill plates, Euler pole velocities, convergent/divergent boundary detection, collision stress fields
- **Geologically-driven terrain** — convergent mountain ridges with asymmetric subduction, fold ridges, mid-ocean ridges, continental rift valleys, continental shelf profiles, stress-modulated noise detail
- **Whittaker biome system** — temperature x moisture lookup with altitude zonation (forest, alpine, rock, snow)
- **Hadley cell climate** — latitude-driven temperature, wind-terrain rain shadows, continentality effects
- **Cloud layers** — stratus/cumulus blend, orographic lift, cyclone systems
- **GPU-rendered preview** — real-time cubemap-sampled sphere with diffuse + specular lighting, ambient occlusion, normal mapping
- **Progressive erosion** — GPU hydraulic erosion on cubemap faces
- **EXR export** — height, albedo, normal, roughness, emission, water mask, AO maps at up to 8K resolution

## Usage

The app opens with a sidebar of planet parameters and a 3D preview.

**Physics Parameters:**
- **Distance (AU)** — Distance from star. Affects planet type and temperature.
- **Mass (M_Earth)** — Planet mass. Affects gravity, tectonics, terrain roughness.
- **[Fe/H]** — Stellar metallicity. Shifts the frost line.
- **Tilt** — Axial tilt (0-90 deg). Affects seasonal variation and biome bands.
- **Day (hours)** — Rotation period. Affects wind patterns and terrain lacunarity.
- **Seed** — Random seed for all procedural generation.

**Visual Overrides:**
- **Continents** — Number and size variety of landmasses
- **Plates** — Number of tectonic plates
- **Mountain scale / Detail** — Terrain feature amplitude
- **Ocean level** — Water coverage control
- **Cloud coverage / type** — Atmosphere visuals
- **Season** — Winter/equinox/summer for biome response

**Preview:** Drag to rotate, scroll to zoom, middle-click to pan. Multiple view modes: Normal, Heightmap, Biome, Climate, Plate structure, Boundary stress.

## Architecture

- **HEALPix plate simulation** (`plate_sim.rs`) — Fibonacci sphere plate seeding, BFS flood-fill, boundary/coast distance fields, super-plate clustering, collision stress via Euler pole velocities
- **HEALPix terrain generation** (`healpix_terrain.rs`) — CPU terrain on spherical grid: base elevation, convergent mountains, fold ridges, divergent features, continental shelves, stress-driven noise. Resampled to cubemap via IDW interpolation
- **GPU preview** (`preview_cubemap.wgsl`) — Fragment shader samples height cubemap, computes temperature/moisture/biomes per pixel, renders with lighting, clouds, atmosphere, and night lights
- **Cube-sphere** with 6 faces, no pole pinching
- **Planetary science model** (`planet.rs`) — Derives planet type, tectonic regime, atmosphere, gravity, temperature, ocean coverage from input parameters

## Project Structure

```
src/
  main.rs            — App entry point
  app.rs             — egui UI, parameter sliders, pipeline orchestration
  gpu.rs             — wgpu device singleton
  planet.rs          — Planet physics model and derived properties
  healpix.rs         — HEALPix spherical pixelization (nested scheme)
  plate_sim.rs       — HEALPix plate tectonics simulation
  healpix_terrain.rs — Terrain generation on HEALPix grid + cubemap resampling
  terrain_compute.rs — GPU tectonic terrain compute pipeline
  preview.rs         — Preview renderer (cubemap upload, render-to-texture)
  export.rs          — Tiled EXR export pipeline
  plates.rs          — GPU plate data structures
  cube_sphere.rs     — Cube-to-sphere coordinate mapping
  noise.rs           — Noise test harness
  shaders/
    preview_cubemap.wgsl — Main preview: cubemap terrain + biomes + climate
    terrain_from_plates.wgsl — GPU plate-based terrain (legacy)
    normal_map.wgsl    — Normal map generation from heightmap
    roughness_map.wgsl — Roughness map from terrain features
    noise.wgsl         — 3D simplex noise
    cube_sphere.wgsl   — Cube-to-sphere mapping
```

## Roadmap

See [Plans.md](Plans.md) for the full implementation plan.

- [x] Phase 1: Project scaffold & GPU hello world
- [x] Phase 2: Cube-sphere & noise generation
- [x] Phase 3: Planet physics & parameter derivation
- [x] Phase 4: Biome, climate & atmosphere rendering
- [x] Phase 5: Terrain rebuild, erosion & EXR export
- [x] Phase 6.0: HEALPix spherical grid
- [x] Phase 6.1: HEALPix plate simulation
- [x] Phase 6.2: HEALPix terrain generation (orogen port)
- [x] Phase 6.3: Integration, performance, export support
- [ ] Phase 6.3.4-5: Parameter tuning & legacy cleanup
- [ ] Phase 7: Blender importer addon

## License

MIT
