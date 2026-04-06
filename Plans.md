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
| 5.8.2 | Export cloud + night light layers as textures | cc:完了 [9ed788c] |
| 8a.7 | Performance + visual comparison: screenshot comparison and docs/research/ update | cc:完了 [9ed788c] |

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
| 5.15.6 | Cloud opacity slider (0-1) multiplying Beer-Lambert alpha for both layers | Slider fades clouds smoothly | 5.15.0 | cc:完了 |
| 5.15.7 | Storm variety: per-storm unique size (0.5x-2.0x), tropical tighter, mid-lat broader | Visible size variation between storms | 5.15.0 | cc:完了 |
| 5.15.8 | Puffier storm clouds: cumulus peaks inside vortex, clear eye with towering wall | Puffy storm clouds, visible eye | 5.15.7 | cc:完了 |
| 5.15.9 | Wind model: smooth Hadley/Ferrel/Polar cells, Coriolis meridional, terrain deflection | Smooth curved flow in debug view | 5.15.0b | cc:完了 |

---

## Phase 5.16: Cloud & Wind Quality Pass

Fix coverage scaling, storm artifacts, cloud detail, seasonal wind, and mountain interaction.
Research: [docs/research/cloud-rendering.md], [docs/brainstorms/2026-03-31-cloud-layer-requirements.md]

| Task | 内容 | DoD | Depends | Status |
|------|------|-----|---------|--------|
| 5.16.1 | Fix coverage scaling: slider=1.0 overrides climate suppression via `max(climate, coverage*0.85)` floor | slider=1.0 ≈ 90%+ coverage | Phase 5.15 | cc:完了 |
| 5.16.2 | Cloud detail: 6 octaves (was 5), base freq 7.0 (was 5.0), weather-region noise breaks uniform spread | More cloud systems, wispy edges, regional clear patches | 5.16.1 | cc:完了 |
| 5.16.3 | Seasonal wind: cell boundaries shift with thermal equator (season * tilt * 0.4) | Wind asymmetric between hemispheres at solstice | 5.16.1 | cc:完了 |
| 5.16.4 | Stronger mountain clouds 0.25 (was 0.10) + föhn gap lee-side suppression | Visible buildup on windward, clear gap on leeward | 5.16.2 | cc:完了 |
| 5.16.5 | Storm spirals: noise-perturbed angle (±0.35), capped vortex at min distance, softer gaps | No smooth tails, turbulent edges | 5.16.4 | cc:完了 |
| 5.16.6 | Seasonal ocean currents: winter hemisphere +50% current strength | Currents stronger in winter | 5.16.3 | cc:完了 |
| 5.16.7 | Cloud type regions: low-freq noise (0.8) + latitude bias selects stratus/cumulus/thin per region; varies fBm gain and warp per zone | Debug shows different texture patterns across planet | 5.16.2 | cc:完了 |
| 5.16.8 | Fix coast edges: smooth_step land_factor replaces hard threshold; föhn gap smooth | No hard edges at continent borders in debug | 5.16.7 | cc:完了 |

---

## Phase 5.17: GPU Cloud Advection

Replace per-pixel noise clouds with GPU compute-generated cloud density texture advected by wind over N iterations. Clouds flow along atmospheric circulation, accumulate over convergence zones, and dissipate over divergence zones. Produces physically motivated cloud patterns that respect wind direction and terrain.

| Task | 内容 | DoD | Depends | Status |
|------|------|-----|---------|--------|
| 5.17.1 | Cloud density cubemap (R16Float, 6×256²) initialized from noise | Texture created, noise-initialized | Phase 5.16 | cc:完了 [ae73bb0] |
| 5.17.2 | Wind computed inline in advection shader (Hadley+seasonal) | Wind drives advection direction | 5.17.1 | cc:完了 [ae73bb0] |
| 5.17.3 | Semi-Lagrangian advection with ping-pong buffers | 30 steps shifts density along wind | 5.17.2 | cc:完了 [ae73bb0] |
| 5.17.4 | Source/sink: ITCZ condensation, ocean boost, subtropical suppression, rain shadow | Clouds accumulate at ITCZ, dissipate over deserts | 5.17.3 | cc:完了 [ae73bb0] |
| 5.17.5 | 30-step pipeline in regenerate_terrain(), 119ms at 256px | Advected clouds within 0.5s | 5.17.4 | cc:完了 [ae73bb0] |
| 5.17.6 | Preview samples cloud_tex (binding 3), 70/30 blend with noise detail | Wind-aligned patterns + noise detail | 5.17.5 | cc:完了 [ae73bb0] |

---

## Phase 5.18: Pressure-Based Wind Model

Replace latitude-only wind with pressure-gradient-derived wind that varies by longitude, terrain, and land/ocean distribution. Inspired by [planet_heightmap_generation](https://github.com/raguilar011095/planet_heightmap_generation) wind.js. Makes cloud advection produce visible, physically motivated cloud movement.

Plan: [docs/plans/2026-04-05-pressure-wind-model.md](docs/plans/2026-04-05-pressure-wind-model.md)

| Task | 内容 | DoD | Depends | Status |
|------|------|-----|---------|--------|
| 5.18.1 | GPU continentality: compute shader BFS from ocean cells on cubemap | Continentality cubemap 0=coast, 1=interior; debug view shows smooth gradient | Phase 5.17 | cc:完了 [cb33c99] |
| 5.18.2 | Pressure field shader: ITCZ low, subtropical highs, continental thermal, elevation, noise | Pressure cubemap; debug view blue=low red=high; ITCZ varies by longitude | 5.18.1 | cc:完了 [cb33c99] |
| 5.18.3 | Pressure gradient → wind: finite differences + Coriolis deflection + surface friction | Wind cubemap shows trades, westerlies, monsoon deflection | 5.18.2 | cc:完了 [cb33c99] |
| 5.18.4 | Wire wind cubemap into cloud advection shader (replaces inline wind_at) | Advection uses precomputed pressure-derived wind | 5.18.3 | cc:完了 [cb33c99] |
| 5.18.5 | Make advected clouds the PRIMARY density source (per-pixel noise adds detail only) | Toggle ON/OFF shows clear cloud movement difference | 5.18.4 | cc:完了 [cb33c99] |
| 5.18.6 | Add Pressure + Continentality debug views to view mode dropdown | Selectable from UI; shows raw pressure/continentality data | 5.18.2 | cc:完了 [cb33c99] |
| 5.18.7 | Tune and validate: compare with Earth-like wind/cloud patterns | Monsoon shift over continents, maritime westerlies, subtropical clear zones | 5.18.5 | cc:完了 [09637ba] |

---

## Phase 5.19: Climate Model Refinement

Apply quantitative data from worldbuildingpasta research to improve physical accuracy of wind, clouds, and precipitation.

Research: [docs/research/worldbuildingpasta-climate-research.md](docs/research/worldbuildingpasta-climate-research.md)

| Task | 内容 | DoD | Depends | Status |
|------|------|-----|---------|--------|
| 5.19.1 | Parameterize Hadley cell boundaries by rotation rate using Kaspi & Showman 2015 table | Cell boundaries shift with rotation_period_h; 16-day planet has single wide cell, 12h planet has 5+ cells | Phase 5.18 | cc:完了 [4e2d088] |
| 5.19.2 | Pressure-dependent wind/precipitation scaling from ExoPlaSim data | Wind speed scales as `22*(1/P)^0.15`, precipitation as `P^-0.5`; thinner atmospheres = stronger winds, more rain | 5.19.1 | cc:完了 [4e2d088] |
| 5.19.3 | Improve ITCZ longitude variation: monsoon rules, thermal equator follows land | ITCZ shifts 15-20° poleward over large continents in summer; trade wind reversal visible in wind debug | 5.19.1 | cc:完了 [4e2d088] |
| 5.19.4 | Fix straight-line cloud stretching: add small-scale turbulence to wind field | Cloud patterns show turbulent eddies, not straight lines; visible in zoomed cloud view | 5.19.3 | cc:完了 [4e2d088] |
| 5.19.5 | Apply moisture penetration distance rules: onshore 2000-3000km, offshore 1000km | Continental interiors >3000km from coast are dry; rain shadow at >2km mountain relief | 5.19.2 | cc:完了 [4e2d088] |
| 5.19.6 | Hadley cell width vs temperature: widen 1° per 4°C warming up to 21°C global mean | Hot planets have wider tropics; cold planets have narrower Hadley cells | 5.19.1 | cc:完了 [4e2d088] |
| 5.19.7 | Altitude-latitude equivalence: 1km altitude ~ 8° poleward for snow/vegetation lines | Mountain biome zonation follows 6.5°C/km lapse rate correctly | - | cc:完了 [4e2d088] |

---

## Phase 5.20: Wind-Shaped Cloud System

Replace the broken compute-based cloud advection pipeline with per-pixel wind streamline tracing. Clouds are visibly stretched along wind direction — trade wind trails, westerly frontal bands, monsoon cloud trains — all at full preview resolution with zero cubemap seam artifacts.

**Approach:** For each cloud pixel, trace backward along the wind field for N steps. Use the integrated streamline position as the noise coordinate. Adjacent pixels trace to similar upstream positions → clouds naturally elongate into wind-aligned trails. The existing `wind_direction_at()` (terrain-deflected) provides the wind, `compute_moisture()` provides the climate awareness, and the continentality cubemap (from Phase 5.18, already smooth) provides coast/interior contrast.

| Task | 内容 | DoD | Depends | Status |
|------|------|-----|---------|--------|
| 5.20.1 | Remove cloud advection compute from rendering path. Keep wind field compute for debug views. Remove advection-related UI (Steps, Blend). Keep Wind Advection checkbox as toggle for new system | Clouds render identically with toggle OFF. No compute advection dispatched. Debug views (Wind, Pressure, Continentality) still work | Phase 5.19 | cc:完了 [49051bd] |
| 5.20.2 | Implement `wind_streamline_warp()`: multi-step backward trace along wind field in per-pixel shader. Replace the current `wind_stretch = tangent * 0.08` with N-step Euler integration along `wind_direction_at()`. Return the traced-back sphere position as noise coordinate | Cloud shapes visibly elongated along wind: trade wind zone shows E-W trails, westerlies show W-E streaks. Visible difference from OFF (round blobs) vs ON (elongated trails) | 5.20.1 | cc:完了 [49051bd] |
| 5.20.3 | Add "Wind Trail" strength slider (0.0-1.0) controlling streamline trace length. 0 = no wind influence (current round blobs), 0.5 = moderate elongation, 1.0 = long wind trails | Slider continuously interpolates between round and elongated clouds | 5.20.2 | cc:完了 [49051bd] |
| 5.20.4 | Sample continentality cubemap in cloud coverage: ocean cells get +20% coverage, deep interior (continentality > 0.7) gets -30% coverage. Smooth sampling to avoid face seams | Continental interiors visibly drier than coasts. No cubemap face seam artifacts in clouds | 5.20.2 | cc:完了 [49051bd] |
| 5.20.5 | Apply wind trail to cirrus layer: high-altitude cirrus should be heavily wind-streaked (jet stream). Separate trail strength for cirrus (2-3x the low cloud trail) | Cirrus shows strong directional streaking, visibly different from low clouds | 5.20.2 | cc:完了 [49051bd] |
| 5.20.6 | Tune and validate: compare with satellite imagery for trade wind cumulus trails, mid-latitude frontal cloud bands, ITCZ thick band, subtropical clear zones | Zoomed view shows recognizable wind-shaped cloud patterns at multiple latitudes | 5.20.5 | cc:完了 [49051bd] |

---

## Phase 7: Blender Importer Addon

Pure-Python Blender addon that imports generated textures and sets up materials.

| Task | 内容 | DoD | Depends | Status |
|------|------|-----|---------|--------|
| 7.1 | Addon skeleton: Blender addon with `bl_info`, register/unregister, sidebar panel in 3D Viewport | Addon installs in Blender 4.x, panel appears in N-panel | - | cc:完了 [53eaff9] |
| 7.2 | "Import Planet" operator: file browser to select planet output directory, load all texture files as Image datablocks | Test: all texture files load into Blender's Image Editor | 7.1 | cc:完了 [53eaff9] |
| 7.3 | Material builder: create Principled BSDF node tree, wire albedo→Base Color, normal→Normal Map→Normal, roughness→Roughness, height→Displacement | Test: material node tree is correctly wired; render shows textured planet | 7.2 | cc:完了 [53eaff9] |
| 7.4 | "Create Planet" mode: generate a UV sphere/icosphere with cube-projection UVs, apply material | Test: one-click produces a textured sphere in the scene | 7.3 | cc:完了 [53eaff9] |
| 7.5 | "Apply to Selected" mode: apply material to user's selected mesh object | Test: selecting an existing sphere and clicking "Apply" textures it correctly | 7.3 | cc:完了 [53eaff9] |
| 7.6 | Cycles + EEVEE compatibility: material works in both render engines (Displacement node setup differs) | Test: render in both Cycles and EEVEE produces correct results | 7.3 | cc:完了 [53eaff9] |

---

## Phase 8: Advanced Visual Features

| Task | 内容 | DoD | Depends | Status |
|------|------|-----|---------|--------|
| 8.1 | Lava glow along plate boundaries: volcanic emission at tectonic faults, tectonic activity slider | Orange-red glow at convergent/divergent boundaries, configurable intensity | Phase 4.8 | cc:完了 [eff66f0] |
| 8.2 | Lens flare near planet limb: procedural flare when sun is near the edge | Cinematic lens flare effect, subtle and adjustable | Phase 5.7 | cc:完了 [eff66f0] |
| 8.3 | Ocean specular / sun glint: bright reflection on water surface toward sun | Visible sun glint on oceans, PBR-correct | Phase 4 | cc:完了 [eff66f0] |
| 8.4 | Ring system: Saturn-like rings with color gradients, transparency, shadow casting on planet | Configurable ring tilt, inner/outer radius, color gradient, planet shadow on rings | Phase 5 | cc:完了 [eff66f0] |
| 8.5 | Ring export: single pixel width gradient texture (at least 4K) for Blender use | Exported 4K+ gradient PNG with transparency for ring shader | 8.4, Phase 7 | cc:完了 [eff66f0] |

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
| 10.1 | Error handling: GPU OOM/device lost → UI error message | App doesn't crash on GPU errors | Phase 5 | cc:完了 [155a63f] |
| 10.2 | Cross-platform CI: GitHub Actions for Linux, macOS, Windows | CI green on all 3 | Phase 5 | cc:完了 [155a63f] |
| 10.3 | README: install, usage guide, parameter reference, example renders | Full documentation | 10.2 | cc:完了 [155a63f] |
| 10.4 | Blender addon packaging: zip + install instructions | Install via Preferences | Phase 7 | cc:完了 [155a63f] |
