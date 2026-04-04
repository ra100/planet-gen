# Planet Gen Plans.md

作成日: 2026-03-29
Requirements: [docs/brainstorms/2026-03-29-planet-gen-requirements.md](docs/brainstorms/2026-03-29-planet-gen-requirements.md)

---

## Completed Phases (archived)

| Phase | Summary | Tasks | Final commit |
|-------|---------|-------|-------------|
| 1 | Project scaffold & GPU hello world | 4/4 | [516a8d7] |
| 2 | Cube-sphere & noise generation | 4/4 | [6b2f515] |
| 3 | Planet physics & parameter derivation | 4/4 | [a061118] |
| 3.5 | Terrain & preview fixes | 5/5 | [de1d3f6] |
| 4 | Biome & surface generation | 9/9 | [c0df99d] |
| 4.5 | Research alignment fixes | 6/6 | [1112a72] |
| 4.6 | Physics-driven terrain & climate (Hadley, rain shadow, domain warp) | 4/4 | [d8d7bd3] |
| 4.7 | Visual control parameters (continental scale, water loss) | 4/4 | [d8d7bd3] |
| 4.8 | Tectonic plate-driven terrain (Voronoi, boundary classification) | 5/5 | [4c93ee5] |
| 5 | Tiled full-resolution generation & 8K EXR export | 7/7 | [d8d7bd3] |
| 5.5 | Preview interaction & visual enhancements (zoom, Mie scattering) | 9/9 | [c56a543] |
| 5.6 | Cloud layer (Schneider remap, Beer-Lambert, cyclone storms) | 9/9 | [da89ff5] |
| 5.7 | Starfield, city lights & star color | 5/5 | [a83332d] |
| 5.8 | Visual polish & layer toggle system (5/6 done) | 5/6 | [b48cd68] |
| 5.9 | Pure noise terrain rebuild | 5/5 | [6d96e4b] |
| 5.10 | Biome rendering refinement | 5/5 | [c2f23e1] |
| 5.11 | UI refactor & equirectangular export | 4/4 | [74339e7] |
| 5.12 | Multi-pass GPU plate terrain (JFA distance fields) | 10/10 | [d017196] |
| 5.13 | Wire continent controls to pipeline | 5/5 | [f0b2a06] |
| 5.14 | Terrain variety, 12-biome system, regional climate, ocean currents | 6/6 | [ba19f5f] |
| 8.5 | Performance (benchmark, progressive erosion, moisture-weighted) | 6/6 | [edb3904] |
| 6.0–6.3 | HEALPix orogen port | Archived to branch `archive/healpix-orogen` | [1aac311] reverted |

**Total completed: ~120 tasks across 22 phases**

---

## Open tasks from completed phases

| Task | 内容 | Status |
|------|------|--------|
| 5.8.2 | Export cloud + night light layers as textures | cc:TODO |
| 8a.7 | Performance + visual comparison: screenshot comparison and docs/research/ update | cc:TODO |

---

## Phase 5.15: Cloud Layer Overhaul

Multiple overlaid cloud layers with distinct cloud types, wind-driven warp following atmospheric circulation, and better cloud distribution from the Hadley cell wind model.

Current system: 2 layers (low stratus/cumulus blend + high cirrus), cyclone vortex warp, climate-modulated coverage.
Target: 3-4 distinct layers, wind-streaked shapes, latitude-coherent cloud bands.

| Task | 内容 | DoD | Depends | Status |
|------|------|-----|---------|--------|
| 5.15.0 | Fix cloud artifacts: reduce wind warp strength, merge all cloud types into single density with alpha compositing instead of separate layers, fix cubemap face boundary banding | Clouds look natural — no vertical streaks, no banding lines, smooth coverage | Phase 5.14 | cc:完了 |
| 5.15.0b | Add debug views: Cloud density, Wind direction, Ocean currents, Moisture map — as new view_mode options in the shader + UI dropdown | Each debug view selectable from View Mode dropdown; shows the raw data driving the layer | 5.15.0 | cc:完了 |
| 5.15.1 | Wind-driven cloud warp: replace random domain warp with wind_direction_vec-based stretching. Low clouds warp along trade winds/westerlies, cirrus along jet stream. Warp strength scales with wind speed at altitude | Cloud shapes visibly elongated along wind direction; trade wind zone clouds streak E-W, westerlies zone W-E | Phase 5.14 | cc:完了 |
| 5.15.2 | Stratocumulus layer: new mid-level layer between low cumulus and high cirrus. Cellular/honeycomb pattern from abs(noise) with open-cell (marine) and closed-cell (land) variants. Covers subtropical ocean regions | Visible cellular cloud pattern over subtropical oceans; distinct from puffy cumulus and smooth stratus | 5.15.1 | cc:完了 |
| 5.15.3 | Latitude-banded cloud distribution: ITCZ thick convective band, subtropical clear zone, mid-latitude frontal bands, polar thin overcast. Replace uniform coverage slider with climate-driven baseline + slider as multiplier | Clear subtropical gaps visible; thick ITCZ band; frontal cloud bands at mid-latitudes | 5.15.2 | cc:完了 |
| 5.15.4 | Per-layer rendering: render low (cumulus/stratus), mid (stratocumulus), and high (cirrus) as separate passes with distinct altitude offsets, opacity, and self-shadow. Each layer casts shadow on layers below | Visible depth parallax between layers; low clouds shadow surface, cirrus shadows low clouds | 5.15.3 | cc:完了 |
| 5.15.5 | Cloud type from climate: ITCZ → tall cumulonimbus (bright white), subtropics → thin stratocumulus, mid-lat → mixed frontal, polar → thin stratus. Cloud type auto-selected from latitude + moisture, cloud_type slider becomes a bias | Cloud appearance changes with latitude without user intervention; slider fine-tunes | 5.15.4 | cc:完了 |

---

## Phase 7: Blender Importer Addon

Pure-Python Blender addon that imports generated textures and sets up materials.

| Task | 内容 | DoD | Depends | Status |
|------|------|-----|---------|--------|
| 7.1 | Addon skeleton: Blender addon with `bl_info`, register/unregister, sidebar panel in 3D Viewport | Addon installs in Blender 4.x, panel appears in N-panel | - | cc:TODO |
| 7.2 | "Import Planet" operator: file browser to select planet output directory, load all texture files as Image datablocks | Test: all texture files load into Blender's Image Editor | 7.1 | cc:TODO |
| 7.3 | Material builder: create Principled BSDF node tree, wire albedo→Base Color, normal→Normal Map→Normal, roughness→Roughness, height→Displacement | Test: material node tree is correctly wired; render shows textured planet | 7.2 | cc:TODO |
| 7.4 | "Create Planet" mode: generate a UV sphere/icosphere with cube-projection UVs, apply material | Test: one-click produces a textured sphere in the scene | 7.3 | cc:TODO |
| 7.5 | "Apply to Selected" mode: apply material to user's selected mesh object | Test: selecting an existing sphere and clicking "Apply" textures it correctly | 7.3 | cc:TODO |
| 7.6 | Cycles + EEVEE compatibility: material works in both render engines (Displacement node setup differs) | Test: render in both Cycles and EEVEE produces correct results | 7.3 | cc:TODO |

---

## Phase 8: Advanced Visual Features

| Task | 内容 | DoD | Depends | Status |
|------|------|-----|---------|--------|
| 8.1 | Lava glow along plate boundaries: volcanic emission at tectonic faults, tectonic activity slider | Orange-red glow at convergent/divergent boundaries, configurable intensity | Phase 4.8 | cc:TODO |
| 8.2 | Lens flare near planet limb: procedural flare when sun is near the edge | Cinematic lens flare effect, subtle and adjustable | Phase 5.7 | cc:TODO |
| 8.3 | Ocean specular / sun glint: bright reflection on water surface toward sun | Visible sun glint on oceans, PBR-correct | Phase 4 | cc:TODO |
| 8.4 | Ring system: Saturn-like rings with color gradients, transparency, shadow casting on planet | Configurable ring tilt, inner/outer radius, color gradient, planet shadow on rings | Phase 5 | cc:TODO |
| 8.5 | Ring export: single pixel width gradient texture (at least 4K) for Blender use | Exported 4K+ gradient PNG with transparency for ring shader | 8.4, Phase 7 | cc:TODO |

---

## Phase 9: Advanced Tectonics

Three-tier tectonic simulation with UI toggle between modes.

### Phase 9a: Better Boundary Physics

| Task | 内容 | DoD | Depends | Status |
|------|------|-----|---------|--------|
| 9a.1 | Research: survey tectonic plate simulation techniques | Research doc in docs/research/ | - | cc:TODO |

(8a.2-8a.6 completed: Euler pole velocities, boundary classification, subduction/rift terrain, perf benchmark)

### Phase 9b: Plate Motion Simulation (continental drift)

| Task | 内容 | DoD | Depends | Status |
|------|------|-----|---------|--------|
| 9b.1 | Research: plate motion algorithms (Euler poles, velocity fields, collision detection) | Research doc | Phase 9a | cc:TODO |
| 9b.2 | Plate velocity field: motion vectors per plate, relative velocities at boundaries | Velocity vectors in Plates debug view | 9b.1 | cc:TODO |
| 9b.3 | Time-stepping: N geological timesteps, accumulate collision/rift history | Geological age slider | 9b.2 | cc:TODO |
| 9b.4 | Collision history → terrain | Older planets = more complex terrain | 9b.3 | cc:TODO |
| 9b.5 | Continental assembly/breakup | Supercontinents form and break with age | 9b.4 | cc:TODO |

### Phase 9c: Mantle Convection (future goal)

| Task | 内容 | DoD | Depends | Status |
|------|------|-----|---------|--------|
| 9c.1 | Research: simplified mantle convection (Rayleigh-Bénard on sphere) | Feasibility doc | Phase 9b | cc:TODO |
| 9c.2 | Convection cell simulation | Pattern visible in debug view | 9c.1 | cc:TODO |
| 9c.3 | Derive plate boundaries from convection | Plates emerge from convection | 9c.2 | cc:TODO |
| 9c.4 | Full pipeline integration | End-to-end convection-driven generation | 9c.3 | cc:TODO |

---

## Phase 10: Polish & Distribution

| Task | 内容 | DoD | Depends | Status |
|------|------|-----|---------|--------|
| 10.1 | Error handling: GPU OOM/device lost → UI error message | App doesn't crash on GPU errors | Phase 5 | cc:TODO |
| 10.2 | Cross-platform CI: GitHub Actions for Linux, macOS, Windows | CI green on all 3 | Phase 5 | cc:TODO |
| 10.3 | README: install, usage guide, parameter reference, example renders | Full documentation | 10.2 | cc:TODO |
| 10.4 | Blender addon packaging: zip + install instructions | Install via Preferences | Phase 7 | cc:TODO |
