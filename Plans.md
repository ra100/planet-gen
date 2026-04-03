# Planet Gen Plans.md

作成日: 2026-03-29
Requirements: [docs/brainstorms/2026-03-29-planet-gen-requirements.md](docs/brainstorms/2026-03-29-planet-gen-requirements.md)

---

## Phase 1: Project Scaffold & GPU Hello World

Bootstrap the Rust project, get wgpu device initialization working, and prove compute shaders run.

| Task | 内容 | DoD | Depends | Status |
|------|------|-----|---------|--------|
| 1.1 | Rust project init: `cargo init`, add wgpu, egui/eframe, bytemuck, image, exr crate dependencies | `cargo build` succeeds with all deps | - | cc:完了 [9d4314e] |
| 1.2 | wgpu device singleton: init adapter + device + queue at startup, store in app state | Unit test: device initializes and reports adapter name | 1.1 | cc:完了 [7a188d2] |
| 1.3 | Minimal compute shader: WGSL shader that writes a gradient to a 256×256 storage texture | Test: shader dispatches, readback buffer contains expected gradient values | 1.2 | cc:完了 [35931ad] |
| 1.4 | egui app shell: eframe window with a sidebar panel (placeholder sliders) and a main area displaying the compute shader output as a texture | App launches, shows gradient texture in main area and sliders in sidebar | 1.3 | cc:完了 [516a8d7] |

---

## Phase 2: Cube-Sphere & Noise Generation

Implement the core geometry representation and fBm terrain generation on the GPU.

| Task | 内容 | DoD | Depends | Status |
|------|------|-----|---------|--------|
| 2.1 | Cube-to-sphere mapping: WGSL function `cube_to_sphere(face, uv) → vec3<f32>` with unit tests for all 6 faces | Test: points at face centers and corners map to correct sphere positions; no NaN at edges | Phase 1 | cc:完了 [4a16025] |
| 2.2 | Simplex/Perlin noise in WGSL: 3D noise function usable in compute shaders | Test: noise output for known seed matches expected range [-1, 1], visually non-uniform | 2.1 | cc:完了 [f905943] |
| 2.3 | Multi-octave fBm compute shader: 8 octaves of noise applied to all 6 cube faces at 256×256 (preview res) | Test: generates 6 heightmaps, values in expected range, different faces show continuous terrain across edges | 2.2 | cc:完了 [d2a17ec] |
| 2.4 | Preview renderer: render cube-sphere heightmap as a lit sphere in the egui viewport (render-to-texture → `ui.image()`) | Rotating planet preview visible in app, updates when noise seed changes | 2.3 | cc:完了 [6b2f515] |

---

## Phase 3: Planet Physics & Parameter Derivation

Implement the science rules that turn user parameters into planet properties.

| Task | 内容 | DoD | Depends | Status |
|------|------|-----|---------|--------|
| 3.1 | Parameter struct: `PlanetParams { star_distance_au, mass_earth, metallicity, axial_tilt_deg, rotation_period_h, seed }` with validation and defaults | Struct compiles, default produces Earth-like values, validation rejects nonsense (negative mass, etc.) | Phase 1 | cc:完了 [e96ddba] |
| 3.2 | Planet type derivation: frost line calc, planet type classification (hot rocky / terrestrial / icy), tectonic regime (Rayleigh number) | Test: Earth params → terrestrial + plate tectonics; Mars params → stagnant lid; 5 AU → icy | 3.1 | cc:完了 [e96ddba] |
| 3.3 | Derived properties: surface gravity, base temperature profile (latitude-based), ocean level, atmosphere type | Test: Earth params produce ~9.8 m/s² gravity, ~15°C avg temp, ~71% ocean coverage | 3.2 | cc:完了 [e96ddba] |
| 3.4 | Wire params to UI: egui sliders for all 6 inputs with ranges (distance: 0.1-50 AU, mass: 0.01-10 M⊕, etc.), derived properties shown as read-only labels | Changing sliders updates derived properties in real-time; tooltips explain each parameter | 3.3 | cc:完了 [a061118] |

---

## Phase 3.5: Terrain & Preview Fixes

Fix issues found during user testing. Root cause: terrain params mapped via discrete categories instead of continuous functions, seed hash broken at large values, preview seam from cubemap texel boundaries.

| Task | 内容 | DoD | Depends | Status |
|------|------|-----|---------|--------|
| 3.5.1 | Seamless preview: remove cubemap indirection for preview — sample 3D noise directly in the preview fragment shader using sphere position, eliminating face seam artifacts entirely | No visible seam when rotating the planet; preview matches terrain generation output | Phase 2 | cc:完了 [de1d3f6] |
| 3.5.2 | Continuous parameter→terrain mapping: replace discrete `match` on planet type/tectonics with continuous functions. Use research spectral exponents (β): Earth=2.0, Mars=2.38, Venus=1.47 → persistence = 10^(-β/20). Distance drives base temperature → terrain roughness continuously. Mass drives amplitude via g∝M^0.46 continuously. All 6 slider values should produce visible continuous change | Test: moving any slider by 10% produces a visibly different (but not randomly different) planet. No flat regions where slider has no effect | 3.5.1 | cc:完了 [de1d3f6] |
| 3.5.3 | Fix seed hash in WGSL: current integer hash may overflow incorrectly in WGSL. Use a float-based hash (fract/sin) or validated u32 hash. Test with seeds 0, 1, 42, 100000, 999999, 4294967295 | Test: all test seeds produce distinct, non-garbage terrain. Adjacent seeds (41,42,43) produce visually different but plausible planets | Phase 2 | cc:完了 [de1d3f6] |
| 3.5.4 | Meaningful metallicity effect: metallicity shifts the frost line (already in DerivedProperties) and affects terrain spectral exponent β. Higher metallicity → more rocky minerals → rougher terrain (higher β). Should look like a continuous roughness change, not a random seed shift | Test: sweeping metallicity -1→+1 at fixed seed produces a smooth transition from smoother to rougher terrain | 3.5.2 | cc:完了 [de1d3f6] |
| 3.5.5 | Planet fills preview area: adjust ray-sphere camera/FOV so the planet sphere fills ~85% of the preview render area | Planet visually fills most of the preview square with small margin | 3.5.1 | cc:完了 [de1d3f6] |

---

## Phase 4: Biome & Surface Generation

Generate biomes, albedo colors, and surface properties from terrain + planet physics.

| Task | 内容 | DoD | Depends | Status |
|------|------|-----|---------|--------|
| 4.1 | Temperature map: latitude + elevation lapse rate + noise → per-pixel temperature on cube faces | Test: temperature decreases from equator to poles and with elevation; values in physically plausible range | Phase 2, 3.3, Phase 3.5 | cc:完了 [c0df99d] |
| 4.2 | Moisture map: noise-based + ocean proximity bonus + rain shadow from wind direction | Test: moisture higher near oceans, lower in rain shadows behind mountains | 4.1 | cc:完了 [c0df99d] |
| 4.3 | Whittaker biome lookup: 7×9 table as a GPU texture, sample with (temp, moisture) → biome ID | Test: known (temp, moisture) pairs return correct biome; covers all 15+ biome types | 4.2 | cc:完了 [c0df99d] |
| 4.4 | Albedo generation: biome ID → base color + noise variation per biome | Test: desert is tan/brown, tropical rainforest is dark green, ice is white; colors vary within each biome | 4.3 | cc:完了 [c0df99d] |
| 4.5 | Normal map generation: compute normals from heightmap via central differences | Test: normals point outward on flat areas, tilt correctly on slopes | Phase 2 | cc:完了 [6c5c386] |
| 4.6 | Roughness map: biome-dependent base roughness + noise | Test: water/ice smooth (low roughness), rock/desert rough (high roughness) | 4.3 | cc:完了 [6c5c386] |
| 4.7 | Ocean & ice masks: threshold height for ocean, threshold temperature for ice caps | Test: ocean mask covers areas below sea level; ice caps at poles for Earth-like params | 4.1 | cc:完了 [c0df99d] |
| 4.8 | Crater stamping: stamp crater shapes (rim + floor + ejecta) on heightmap, count scaled by derived surface age | Test: older surfaces get more craters; crater shapes have raised rim and depressed floor | Phase 2, 3.2 | cc:完了 [6c5c386] |
| 4.9 | Preview integration: all maps (albedo, height, biomes) visible in the preview sphere with <1s update at 256×256 | Changing any parameter updates the preview planet within 1 second | 4.4, 4.5, 4.6, 4.7 | cc:完了 [c0df99d] |

---

## Phase 4.5: Research Alignment Fixes

Align physics model with research data. Replace heuristic thresholds with research-backed continuous formulas.

| Task | 内容 | DoD | Depends | Status |
|------|------|-----|---------|--------|
| 4.5.1 | Continuous tectonic regime: replace binary mass/distance threshold with simplified Rayleigh number estimate. Ra ∝ g·ΔT·D³ — use mass→gravity, distance→ΔT, derive Ra continuously. Tectonics factor [0,1] instead of enum for terrain influence | Test: tectonics factor varies smoothly with mass 0.01→10 and distance 0.1→50; Earth params give ~0.8+, Mars ~0.3 | Phase 4 | cc:完了 [1112a72] |
| 4.5.2 | Continuous atmosphere + greenhouse feedback: replace discrete mass cutoffs with escape velocity-based retention AND add greenhouse feedback — colder equilibrium planets accumulate more CO₂ → stronger greenhouse, extending habitable zone to ~0.95-1.7 AU instead of current sharp dropoff at ~1.15 AU. Use carbonate-silicate cycle approximation | Test: atmosphere strength transitions smoothly; habitable zone extends to ~1.6 AU without freezing over; Earth at 1.0 AU gives ~15°C, at 1.5 AU gives ~0-5°C (cold but not ice planet) | 4.5.1 | cc:完了 [1112a72] |
| 4.5.3 | MMSN plausibility check: isolation mass warning in UI | Test: warning shown for implausible mass | 4.5.1 | cc:完了 [1112a72] |
| 4.5.4 | fBm octaves 8-12 per research | Test: minimum 8 octaves | Phase 4 | cc:完了 [1112a72] |
| 4.5.5 | Physical ocean fraction from water budget model | Test: Earth ~0.5-0.7 | 4.5.1 | cc:完了 [1112a72] |
| 4.5.6 | Continental structure: low-freq base + detail fBm, bimodal for plate tectonics | Test: visible continent-scale landmasses | Phase 4 | cc:完了 [1112a72] |

---

## Phase 4.6: Physics-Driven Terrain & Climate

Interconnected physical systems: Hadley cell atmospheric circulation, domain warping for geological terrain, rain shadows, and altitude zonation.

Plan: [docs/plans/2026-03-30-002-feat-physics-driven-terrain-plan.md](docs/plans/2026-03-30-002-feat-physics-driven-terrain-plan.md)

| Task | 内容 | DoD | Depends | Status |
|------|------|-----|---------|--------|
| 4.6.1 | Hadley cell moisture model: three-cell circulation replacing noise-based moisture, latitude-banded climate zones | Deserts at ~30° N/S, green equatorial band, tilt shifts bands | Phase 4 | cc:完了 [d8d7bd3] |
| 4.6.2 | Wind direction and rain shadow: wind from Hadley model creates dry leeward zones on mountains | Visible wet/dry asymmetry on mountain ranges | 4.6.1 | cc:完了 [d8d7bd3] |
| 4.6.3 | Domain warping for geological terrain: warp continental_base noise for ridges, irregular coastlines | Coastlines irregular, mountains rougher than lowlands | Phase 4 | cc:完了 [d8d7bd3] |
| 4.6.4 | Altitude zonation: forest → alpine → rock → snow bands on mountains, latitude-dependent snow line | Visible horizontal color banding on mountain slopes | 4.6.1 | cc:完了 [d8d7bd3] |

---

## Phase 4.7: Visual Control Parameters

Artistic override parameters for continent size, polar ocean ice, and water loss.

Plan: [docs/plans/2026-03-30-001-feat-visual-control-parameters-plan.md](docs/plans/2026-03-30-001-feat-visual-control-parameters-plan.md)

| Task | 内容 | DoD | Depends | Status |
|------|------|-----|---------|--------|
| 4.7.1 | Continental scale slider (0.5-4.0): controls frequency of continental_base noise layer | Different continent sizes at different values, preview updates <1s | Phase 4 | cc:完了 [d8d7bd3] |
| 4.7.2 | Polar ocean ice rendering: ocean rendered as ice when temperature < -2°C | Polar regions show ice on ocean, not just on land | Phase 4 | cc:完了 [d8d7bd3] |
| 4.7.3 | Water loss slider (0.0-1.0): reduces effective ocean fraction below physics-derived value | Slider reduces ocean coverage smoothly, exposed areas show land biomes | Phase 4 | cc:完了 [d8d7bd3] |
| 4.7.4 | Uniform struct alignment: ensure PreviewUniforms stays 16-byte aligned with new fields | cargo test passes, no GPU validation errors | 4.7.1, 4.7.3 | cc:完了 [d8d7bd3] |

---

## Phase 4.8: Tectonic Plate-Driven Terrain

Replace noise-only heightmap with geologically structured terrain: Voronoi plates on sphere → boundary classification → height from geology → fBm detail. Two-pass GPU compute pipeline producing cubemap texture.

Plan: [docs/plans/2026-03-30-003-feat-tectonic-terrain-plan.md](docs/plans/2026-03-30-003-feat-tectonic-terrain-plan.md)
Requirements: [docs/brainstorms/2026-03-30-tectonic-terrain-requirements.md](docs/brainstorms/2026-03-30-tectonic-terrain-requirements.md)

| Task | 内容 | DoD | Depends | Status |
|------|------|-----|---------|--------|
| 4.8.1 | Plate generation on CPU (Rust): Fibonacci sphere, N=6-16 plates, continental/oceanic types, velocities | Earth params → 10-12 plates, ~30% continental | Phase 4 | cc:完了 [e599cd7] |
| 4.8.2 | Plate assignment compute shader: Voronoi on sphere, boundary distance and type | Distinct Voronoi regions with classified boundaries | 4.8.1 | cc:完了 [9155677] |
| 4.8.3 | Height generation compute shader: plate elevation + boundary terrain + fBm detail | Mountains at convergent boundaries, bimodal distribution | 4.8.2 | cc:完了 [9155677] |
| 4.8.4 | Cubemap preview integration: compute → R16Float cubemap → preview shader | Preview shows geological terrain, all features work | 4.8.3 | cc:完了 [4c93ee5] |
| 4.8.5 | Voronoi edge warping: domain warp for natural boundaries | Curved irregular plate boundaries | 4.8.2 | cc:完了 [9155677] |

---

## Phase 5: Tiled Full-Resolution Generation & Export

Scale from preview (256²) to full 8K (8192²) via tiled generation and export to files.

| Task | 内容 | DoD | Depends | Status |
|------|------|-----|---------|--------|
| 5.1 | Tile coordinator: subdivide each face into 16×16 tiles of 512px, dispatch compute shaders per tile with correct UV offsets | Test: 6 × 256 = 1,536 tiles dispatched; adjacent tiles produce seamless output at borders | Phase 4 | cc:完了 [d8d7bd3] |
| 5.2 | GPU→CPU readback pipeline: read tile results from GPU storage textures to CPU memory, assemble into full-face images | Test: assembled face image matches expected resolution (8192×8192); no visible tile seams | 5.1 | cc:完了 [d8d7bd3] |
| 5.3 | EXR export: write height/displacement as 16-bit float EXR files (one per face or stitched) | Test: exported EXR opens in Blender, values match GPU output within float precision | 5.2 | cc:完了 [d8d7bd3] |
| 5.4 | PNG export: write albedo, normal, roughness, ocean mask, ice mask as 8-bit PNG | Test: exported PNGs open correctly, color values match preview visually | 5.2 | cc:完了 [d8d7bd3] |
| 5.5 | Background generation with progress: full generation runs on background thread, UI shows progress bar (% tiles complete) | Test: UI remains responsive during generation; progress bar updates; cancel button stops generation | 5.1 | cc:完了 [d8d7bd3] |
| 5.6 | Output directory structure: organize exported files in `<output_dir>/<planet_name>/` with consistent naming | Test: generate produces expected file tree with all 6 map types | 5.3, 5.4 | cc:完了 [d8d7bd3] |
| 5.7 | Performance validation: full 8K generation completes in <30s on RTX 3080-class GPU | Benchmark: timed generation from start to all files written, meets target | 5.5, 5.6 | cc:完了 [d8d7bd3] |

---

## Phase 5.5: Preview Interaction & Visual Enhancements

Viewport controls, atmosphere rendering, and UI polish for the preview.

| Task | 内容 | DoD | Depends | Status |
|------|------|-----|---------|--------|
| 5.5.1 | Viewport zoom (scroll wheel) and pan (middle mouse drag) | Zoom in/out, drag planet around viewport | Phase 5 | cc:完了 [03721e1] |
| 5.5.2 | Inverse drag direction + cursor-centered zoom | Drag feels like moving the planet; zoom centers on cursor | 5.5.1 | cc:完了 [ab10d3d] |
| 5.5.3 | Fix seasonal biome instability: mean annual climate for biome type, seasonal for color modulation | Forests stay stable across seasons | Phase 4 | cc:完了 [26b54e0] |
| 5.5.4 | Improve tectonic plate shapes: per-plate noise bias + stronger domain warping | Less convex, more organic plate boundaries | Phase 4.8 | cc:完了 [1d730e5] |
| 5.5.5 | Advanced Tweaks panel: mountain height, boundary width, shape warp, detail scale, plate count sliders | Exposed terrain controls with tooltips | Phase 4.8 | cc:完了 [1024f30] |
| 5.5.6 | Fix mountain height clipping at high mountain_scale | No flat plateaus at extreme settings | 5.5.5 | cc:完了 [f787c86] |
| 5.5.7 | Fix grid-aligned erosion artifacts (MFD flow routing) + scrollable side panel | Smooth erosion on steep terrain | Phase 4.8 | cc:完了 [e073f4e] |
| 5.5.8 | Fix slider sticking during GPU work: process UI before blocking GPU | Mouse release captured correctly | 5.5.5 | cc:完了 [cb707bf] |
| 5.5.9 | Mie scattering for atmospheric haze and sun glow (Henyey-Greenstein phase function) | Blue limb glow + bright sun-side haze | Phase 4 | cc:完了 [c56a543] |

---

## Phase 5.6: Cloud Layer

Procedural cloud system: Schneider remap, domain-warped fBm, Beer-Lambert opacity, self-shadowing, cyclone storms.

Plan: [docs/plans/2026-03-31-003-feat-cloud-layer-v2-plan.md](docs/plans/2026-03-31-003-feat-cloud-layer-v2-plan.md)
Requirements: [docs/brainstorms/2026-03-31-cloud-layer-requirements.md](docs/brainstorms/2026-03-31-cloud-layer-requirements.md)
Research: [docs/research/cloud-layer-rendering.md](docs/research/cloud-layer-rendering.md)

| Task | 内容 | DoD | Depends | Status |
|------|------|-----|---------|--------|
| 5.6.1 | Cloud uniforms + UI: coverage slider, seed control, randomize button | Cloud section in sidebar with coverage and seed | Phase 5.5 | cc:完了 [f8722c9] |
| 5.6.2 | Cloud density: Schneider remap, domain-warped 5-octave fBm, climate threshold modulation | Organic cloud shapes, no latitude banding, linear slider response | 5.6.1 | cc:完了 [0def439] |
| 5.6.3 | Cloud rendering: Beer-Lambert opacity, self-shadowing, HG silver lining | Bright tops, blue-grey shadows, translucent thin edges | 5.6.2 | cc:完了 [0def439] |
| 5.6.4 | Terrain-aware clouds: orographic lift, ocean/land influence, convection, weather scale | Clouds cluster over warm oceans and mountain windward sides | 5.6.3 | cc:完了 [de6bd6a] |
| 5.6.5 | Seasonal clouds: moisture and temperature follow season slider | Cloud patterns shift with seasons | 5.6.4 | cc:完了 [2e0dbea] |
| 5.6.6 | Two-layer rendering: low cumulus/stratus shell + high cirrus layer with parallax | Visible depth between cloud layers | 5.6.3 | cc:完了 [248c1eb] |
| 5.6.7 | Cloud shadows on surface + dual-noise cloud types (stratus/cumulus blend) | Surface darkened under clouds, mixed cloud textures | 5.6.3 | cc:完了 [b37ec7c] |
| 5.6.8 | Cloud type slider: smooth stratus (0) ↔ puffy cumulus (1) | User-controllable cloud style | 5.6.7 | cc:完了 [f6520bb] |
| 5.6.9 | Cyclone storms: count slider (0-8), size slider, vortex warp, spiral arm carving, Coriolis-correct | Visible storm systems with eye, spiral arms, configurable count/size | 5.6.2 | cc:完了 [da89ff5] |

---

## Phase 5.7: Starfield, City Lights & Star Color

Background environment, night-side civilization, and star type lighting.

| Task | 内容 | DoD | Depends | Status |
|------|------|-----|---------|--------|
| 5.7.1 | Starfield background with sun orb: hash-based stars, color variation, sun disc with glow | Stars visible behind planet, sun at light_dir position | Phase 5.5 | cc:完了 [361b6de] |
| 5.7.2 | Night-side city lights + day-side urban grey patches: procedural urban density from climate data | Warm glow on dark side, grey patches on day side, Development slider | Phase 5.6 | cc:完了 [02e4109] |
| 5.7.3 | City light color slider: warm amber → white LED → cool blue | Configurable night light color | 5.7.2 | cc:完了 [91bd657] |
| 5.7.4 | Star color temperature slider: blue O-star → sun G-star → red M-dwarf, tints all lighting + clouds + sun orb | Planet illumination matches star type | Phase 5.5 | cc:完了 [9fea2ab] |
| 5.7.5 | City lights under clouds with scattered glow: dimmed by cloud cover, soft glow through thin clouds | Lights properly occluded, scattered through clouds | 5.7.2, Phase 5.6 | cc:完了 [a83332d] |

---

## Phase 5.8: Visual Polish & Layer Toggle System

Plan: [docs/plans/2026-04-02-001-feat-layer-toggle-system-plan.md](docs/plans/2026-04-02-001-feat-layer-toggle-system-plan.md)
Requirements: [docs/brainstorms/2026-04-02-layer-toggle-system-requirements.md](docs/brainstorms/2026-04-02-layer-toggle-system-requirements.md)

Terrain rendering improvements and additional visual features.

| Task | 内容 | DoD | Depends | Status |
|------|------|-----|---------|--------|
| 5.8.1 | Ambient occlusion in terrain valleys + toggle checkbox | Visible darkening in valleys, toggleable via UI | Phase 4.8 | cc:完了 [88f83f1] |
| 5.8.2 | Export cloud + night light layers as textures: add cloud density and city lights to 8K export pipeline | Cloud and night light PNGs exported alongside albedo/normal/roughness | Phase 5, 5.6, 5.7 | cc:TODO |
| 5.8.3 | Polar ice: NASA-like rendering — thickness variation, pressure ridges, land snow at 0°C, coastline blending | Realistic polar ice with thin/thick variation and smooth transitions | Phase 4 | cc:完了 [b3f4961] |
| 5.8.4 | Smooth roughness map: continuous smooth_step + noise variation, fix blocky biome boundaries | No visible hard edges in roughness debug view | Phase 4 | cc:完了 [ae4cbbc] |
| 5.8.5 | Ocean plate boundary smoothing: blur depth + noise to soften tectonic ridges on ocean floor | No hard lines visible in ocean coloring | Phase 4.8 | cc:完了 [9138df7] |
| 5.8.6 | Fix speckled coastlines: erosion roughening noise fades near ocean level | Clean coastlines without tiny land/water cells | Phase 4.8 | cc:完了 [b48cd68] |

---

## Phase 5.9: Pure Noise Terrain Rebuild

Replace Voronoi plate-based terrain with layered noise approach. Plates caused persistent artifacts (noodle ridges, boundary ghosting, puzzle-piece continents).

Plan: [docs/plans/2026-04-02-002-fix-terrain-artifacts-extend-geology-plan.md](docs/plans/2026-04-02-002-fix-terrain-artifacts-extend-geology-plan.md)
Requirements: [docs/brainstorms/2026-04-02-physics-terrain-rebuild-requirements.md](docs/brainstorms/2026-04-02-physics-terrain-rebuild-requirements.md)

| Task | 内容 | DoD | Depends | Status |
|------|------|-----|---------|--------|
| 5.9.1 | Pure noise terrain: domain-warped continental noise + ridged multifractal mountains + hotspots | Organic continent shapes, no Voronoi artifacts, seed-dependent | Phase 4.8 | cc:完了 [040e7a3] |
| 5.9.2 | Bimodal shaping: pow(0.35) for solid continents without channel fragmentation | Continents are coherent masses, not torn webs | 5.9.1 | cc:完了 [4aa7fbd] |
| 5.9.3 | PCG hash for seed offsets: replace golden-ratio hash to eliminate cross-seed correlation | Each seed produces completely different planet | 5.9.1 | cc:完了 [02b2aeb] |
| 5.9.4 | Full water level range: water_loss=0 → ocean world, water_loss=1 → desert | Water loss slider controls full range of ocean coverage | 5.9.1 | cc:完了 [e9f0fbe] |
| 5.9.5 | Atmospheric moisture slider: decouple climate wetness from sea level control | Independent control of biome greenness vs water surface | 5.9.4 | cc:完了 [6d96e4b] |

---

## Phase 5.10: Biome Rendering Refinement

Regional color variance, moisture rebalance, and realistic snow rules.

Plan: [docs/plans/2026-04-02-003-feat-biome-rendering-refinement-plan.md](docs/plans/2026-04-02-003-feat-biome-rendering-refinement-plan.md)

| Task | 内容 | DoD | Depends | Status |
|------|------|-----|---------|--------|
| 5.10.1 | Regional color variance: low-freq noise selects biome sub-variants (red/tan/dark sand, emerald/olive forest) | Different desert regions show different sand colors on same planet | Phase 5.9 | cc:完了 [4c3c1ee] |
| 5.10.2 | Moisture rebalance: soften ocean_fraction scaling so low-water planets aren't all desert | Earth-like planets show recognizable climate zones | 5.10.1 | cc:完了 [4c3c1ee] |
| 5.10.3 | Slope-aware snow: steep slopes shed snow, extreme peaks above cloud layer too dry | Mountains show patchy snow on ridges, exposed rock on cliffs | 5.10.1 | cc:完了 [4c3c1ee] |
| 5.10.4 | Fix snow tracing coastline: cold_snow threshold from 2°C to -3°C | Snow only in genuinely cold regions, not near-coast elevated areas | 5.10.3 | cc:完了 [0f9d0b7] |
| 5.10.5 | Fix roughness pixelation, city density scaling, emission export | Smooth roughness, sparse cities at low dev, clean emission view | 5.10.1 | cc:完了 [c2f23e1] |

---

## Phase 5.11: UI Refactor & Export Overhaul

Right panel for export, collapsible layers, equirectangular EXR export.

| Task | 内容 | DoD | Depends | Status |
|------|------|-----|---------|--------|
| 5.11.1 | UI refactor: right panel for derived properties + export, collapsible render layers | Two-panel layout, export checkboxes for layer selection | Phase 5.10 | cc:完了 [89955c9] |
| 5.11.2 | Equirectangular export: cubemap-to-equirect conversion with bilinear interpolation | Single 2:1 EXR files instead of 6 cube face files | 5.11.1 | cc:完了 [0b8ca12] |
| 5.11.3 | All exports as EXR with ZIP16 compression | 32-bit float precision, good compression, universal support | 5.11.2 | cc:完了 [74339e7] |
| 5.11.4 | All render layers ON by default (water, ice, biomes, clouds, atmosphere, cities, erosion) | App launches with full rendering, not bare terrain | Phase 5.10 | cc:完了 [3a5fa10] |

---

## Phase 5.12: Multi-Pass GPU Plate Terrain

Replace pure-noise terrain with multi-pass GPU plate system using Jump Flooding Algorithm for smooth distance fields. Mountains form at collision zones, continental shelves from coast distance, stress-driven roughness variation.

Requirements: [docs/brainstorms/2026-04-03-multipass-plate-terrain-requirements.md](docs/brainstorms/2026-04-03-multipass-plate-terrain-requirements.md)
Reference: planet_heightmap_generation/ (JS/WebGL implementation with BFS distance fields)

| Task | 内容 | DoD | Depends | Status |
|------|------|-----|---------|--------|
| 5.12.1 | Pass 1 shader: Voronoi plate assignment per pixel + boundary detection | Each pixel stores plate_idx; boundary pixels marked where neighbors differ | - | cc:完了 [d017196] |
| 5.12.2 | Pass 2 shader: JFA distance field (ping-pong buffers, O(log n) passes) | Smooth distance-to-boundary field per pixel; no sharp Voronoi edges | 5.12.1 | cc:完了 [d017196] |
| 5.12.3 | Rust pipeline: multi-pass dispatch orchestration (buffers, bind groups, sequencing) | All passes dispatch in sequence per face, readback produces TectonicTerrain | 5.12.1, 5.12.2 | cc:完了 [d017196] |
| 5.12.4 | Pass 3: stress computation + boundary type classification (convergent/divergent/transform) | Per-pixel stress from plate velocities; boundary type from relative motion dot product | 5.12.3 | cc:完了 [d017196] |
| 5.12.5 | Pass 3: collision mountains with asymmetric subduction profiles | Mountains at convergent zones; steeper oceanic side + trench, gentler back-arc plateau | 5.12.4 | cc:完了 [d017196] |
| 5.12.6 | Pass 3: fold ridges parallel to plate motion direction | Linear ridge/valley patterns within mountain zones aligned with Euler pole | 5.12.5 | cc:完了 [d017196] |
| 5.12.7 | Pass 3: continental shelves + ocean floor from coast distance field | Shelf (0-5 cells), slope (5-12 cells), abyssal plain (12+); active vs passive margin width | 5.12.4 | cc:完了 [d017196] |
| 5.12.8 | Pass 3: stress-driven roughness + fBm detail | Craggy near orogens, smooth in cratons; noise amplitude scales with stress | 5.12.5, 5.12.7 | cc:完了 [d017196] |
| 5.12.9 | Pass 3: divergent boundaries (mid-ocean ridges, continental rift valleys) | Subtle elevation at divergent; rift depression on land | 5.12.4 | cc:完了 [d017196] |
| 5.12.10 | Integration: wire into app.rs, remove old noise terrain, verify <2s preview | Full pipeline end-to-end; fragment shader unchanged; water_loss/moisture sliders work | 5.12.8, 5.12.9 | cc:完了 [d017196] |

---

## Phase 6.0: HEALPix Orogen Port — Infrastructure

HEALPix sphere grid module with index↔position conversion, neighbor lookup, and cubemap resampling.

Requirements: [docs/brainstorms/2026-04-03-orogen-port-requirements.md](docs/brainstorms/2026-04-03-orogen-port-requirements.md)

| Task | 内容 | DoD | Depends | Status |
|------|------|-----|---------|--------|
| 6.0.1 | HEALPix module: nested-scheme pixel index ↔ 3D sphere position (vec3) | `healpix::pix2vec(nside, ipix)` and `healpix::vec2pix(nside, vec)` pass unit tests for all 12*nside^2 pixels | - | cc:完了 [c7e75f6] |
| 6.0.2 | HEALPix neighbor lookup: 8 neighbors per pixel (SW,W,NW,N,NE,E,SE,S) | `healpix::neighbors(nside, ipix) -> [u32; 8]` correct for interior + edge + corner pixels | 6.0.1 | cc:完了 [c7e75f6] |
| 6.0.3 | HEALPix → cubemap resampling: sample HEALPix buffer onto 6-face cubemap | `healpix::to_cubemap(data, nside, face_res) -> [Vec<f32>; 6]` with bilinear interpolation; output matches existing TectonicTerrain format | 6.0.1 | cc:完了 [c7e75f6] |
| 6.0.4 | Integration test: generate flat HEALPix buffer, resample to cubemap, render in preview | Preview shows a uniform sphere with no face-boundary seams or distortion artifacts | 6.0.3 | cc:完了 [c7e75f6] |

---

## Phase 6.1: HEALPix Orogen Port — Plate Simulation

Voronoi plate assignment, BFS distance fields, and super-plate clustering on HEALPix grid.

| Task | 内容 | DoD | Depends | Status |
|------|------|-----|---------|--------|
| 6.1.1 | Plate seed generation on HEALPix: 10-20 seeds from Fibonacci sphere + noise | Seeds well-distributed; continental/oceanic type assigned from ocean_fraction | Phase 6.0 | cc:完了 [0688c5c] |
| 6.1.2 | BFS flood-fill plate assignment with noise perturbation | Each HEALPix pixel assigned to nearest plate; boundaries are organic (not pure Voronoi) | 6.1.1 | cc:完了 [0688c5c] |
| 6.1.3 | BFS distance-to-boundary field (global, all boundaries) | Smooth distance field across entire sphere; no face-boundary artifacts; unit test verifies continuity | 6.1.2 | cc:完了 [0688c5c] |
| 6.1.4 | BFS distance-to-coast field (continental↔oceanic boundaries only) | Separate distance field for shelf/slope profiles; 0 at coast, increasing inland and offshore | 6.1.2 | cc:完了 [0688c5c] |
| 6.1.5 | Super-plate clustering: group 10-20 small plates into 3-5 super-plates | Super-plate assignment per pixel; blend 5% small-plate + 95% super-plate for continent-scale structure | 6.1.2 | cc:完了 [0688c5c] |
| 6.1.6 | Stress computation: collision stress at each pixel from plate velocity × boundary normal | Per-pixel stress field [0,1]; high at convergent, low at divergent/interior; decays with distance | 6.1.3 | cc:完了 [0688c5c] |

---

## Phase 6.2: HEALPix Orogen Port — Terrain Generation

Orogeny, shelves, stress-driven roughness on HEALPix, then GPU noise detail.

| Task | 内容 | DoD | Depends | Status |
|------|------|-----|---------|--------|
| 6.2.1 | Base elevation from plate type + super-plate structure | Continental pixels elevated, oceanic depressed; super-plate blending creates broad continent shapes | Phase 6.1 | cc:完了 [b8af38f] |
| 6.2.2 | Convergent mountain ridges with asymmetric subduction profiles | Mountains at convergent boundaries; steeper subducting side + trench, gentler back-arc plateau | 6.2.1, 6.1.6 | cc:完了 [b8af38f] |
| 6.2.3 | Fold ridges parallel to plate motion direction | Linear ridge/valley patterns within mountain zones; aligned with Euler pole velocity | 6.2.2 | cc:完了 [b8af38f] |
| 6.2.4 | Divergent boundaries: mid-ocean ridges + continental rift valleys | Subtle ocean ridge elevation; land rift depression; stress-modulated | 6.2.1, 6.1.6 | cc:完了 [b8af38f] |
| 6.2.5 | Continental shelf profile from coast distance field | Shelf (0-5 units), slope (5-12), abyssal plain (12+); active vs passive margin width | 6.2.1, 6.1.4 | cc:完了 [b8af38f] |
| 6.2.6 | Stress-driven roughness: GPU noise detail amplitude scales with stress | Craggy near orogens, smooth in cratons; ridged multifractal + fBm layered via GPU compute shader | 6.2.2, 6.2.5 | cc:完了 [b8af38f] |
| 6.2.7 | HEALPix→cubemap pipeline: resample terrain to cubemap, upload as R16Float | Existing fragment shader renders the new terrain; biomes/climate/atmosphere work unchanged | 6.2.6, 6.0.3 | cc:完了 [b8af38f] |

---

## Phase 6.3: HEALPix Orogen Port — Integration & Tuning

Wire into app, parameter tuning, export support, cleanup.

| Task | 内容 | DoD | Depends | Status |
|------|------|-----|---------|--------|
| 6.3.1 | Wire HEALPix terrain pipeline into app.rs regenerate_terrain() | New pipeline replaces noise terrain for preview; all UI parameters (seed, mountain_scale, etc.) work | Phase 6.2 | cc:完了 [45789c7] |
| 6.3.2 | Performance profiling: full pipeline < 3s at Nside=256 | Timed end-to-end; BFS + terrain + resample + upload within budget | 6.3.1 | cc:完了 [45789c7] |
| 6.3.3 | Export support: HEALPix terrain at Nside=512 for high-res export | Export pipeline uses HEALPix terrain → cubemap → equirect EXR; existing maps (albedo, roughness, etc.) work | 6.3.1 | cc:完了 [45789c7] |
| 6.3.4 | Parameter tuning: mountain height, shelf width, stress decay, noise detail | Earth-like seed produces recognizable tectonic features; visual quality approaches reference | 6.3.1 | cc:TODO |
| 6.3.5 | Remove old noise terrain code from plates.wgsl (keep as backup branch) | TerrainComputePipeline uses HEALPix exclusively; old single-pass shaders archived | 6.3.4 | cc:TODO |

---

## Phase 7: Blender Importer Addon

Pure-Python Blender addon that imports generated textures and sets up materials.

| Task | 内容 | DoD | Depends | Status |
|------|------|-----|---------|--------|
| 6.1 | Addon skeleton: Blender addon with `bl_info`, register/unregister, sidebar panel in 3D Viewport | Addon installs in Blender 4.x, panel appears in N-panel | - | cc:TODO |
| 6.2 | "Import Planet" operator: file browser to select planet output directory, load all texture files as Image datablocks | Test: all texture files load into Blender's Image Editor | 6.1 | cc:TODO |
| 6.3 | Material builder: create Principled BSDF node tree, wire albedo→Base Color, normal→Normal Map→Normal, roughness→Roughness, height→Displacement | Test: material node tree is correctly wired; render shows textured planet | 6.2 | cc:TODO |
| 6.4 | "Create Planet" mode: generate a UV sphere/icosphere with cube-projection UVs, apply material | Test: one-click produces a textured sphere in the scene | 6.3 | cc:TODO |
| 6.5 | "Apply to Selected" mode: apply material to user's selected mesh object | Test: selecting an existing sphere and clicking "Apply" textures it correctly | 6.3 | cc:TODO |
| 6.6 | Cycles + EEVEE compatibility: material works in both render engines (Displacement node setup differs) | Test: render in both Cycles and EEVEE produces correct results | 6.3 | cc:TODO |

---

## Phase 7: Advanced Visual Features

Post-Blender visual enhancements for cinematic renders.

| Task | 内容 | DoD | Depends | Status |
|------|------|-----|---------|--------|
| 7.1 | Lava glow along plate boundaries: volcanic emission at tectonic faults, tectonic activity slider | Orange-red glow at convergent/divergent boundaries, configurable intensity | Phase 4.8 | cc:TODO |
| 7.2 | Lens flare near planet limb: procedural flare when sun is near the edge | Cinematic lens flare effect, subtle and adjustable | Phase 5.7 | cc:TODO |
| 7.3 | Ocean specular / sun glint: bright reflection on water surface toward sun | Visible sun glint on oceans, PBR-correct | Phase 4 | cc:TODO |
| 7.4 | Ring system: Saturn-like rings with color gradients, transparency, shadow casting on planet | Configurable ring tilt, inner/outer radius, color gradient, planet shadow on rings | Phase 5 | cc:TODO |
| 7.5 | Ring export: single pixel width gradient texture (at least 4K) for Blender use | Exported 4K+ gradient PNG with transparency for ring shader | 7.4, Phase 6 | cc:TODO |

---

## Phase 7.5: Performance

Benchmarking and optimization infrastructure.

| Task | 内容 | DoD | Depends | Status |
|------|------|-----|---------|--------|
| 7.5.1 | Performance benchmark binary: `src/bin/perf_bench.rs` | `cargo run --release --bin perf_bench` outputs CSV timing | Phase 5 | cc:完了 [b3cfbf6] |
| 7.5.2 | Document performance bottlenecks | Analysis in docs/research/performance-analysis.md | 7.5.1 | cc:完了 [d30ea78] |
| 7.5.3 | Resolution-adaptive erosion: scale iterations with resolution (256→5, 512→10, 768→15, 1024+→25) | 2K generation under 4s; visual quality similar at each resolution | - | cc:完了 [5d7b697] |
| 7.5.4 | Progressive terrain: show un-eroded preview immediately, apply erosion in batches (5 iters/batch), re-render after each batch | Planet visible within 100ms of parameter change; erosion refines progressively | 7.5.3 | cc:完了 [b8d86f7] |
| 7.5.5 | Re-run perf_bench after optimizations, compare before/after | Updated performance-analysis.md with comparison table | 7.5.3, 7.5.4 | cc:完了 [c7e75f6] |
| 7.5.6 | Moisture-weighted erosion: scale erosion strength by latitude-based moisture in erosion.wgsl. Desert mountains stay sharp/craggy, tropical mountains get smooth rounded valleys. Factors: rainfall (primary), slope steepness (secondary) | Visible difference between desert (sharp) and tropical (smooth) mountains at same elevation | Phase 4.8 | cc:完了 [edb3904] |

---

## Phase 8: Advanced Tectonics

Three-tier tectonic simulation with UI toggle between modes. Each tier adds realism at the cost of computation time.

### Phase 8a: Better Boundary Physics (improve current Voronoi system)

| Task | 内容 | DoD | Depends | Status |
|------|------|-----|---------|--------|
| 8a.1 | Research: survey tectonic plate simulation techniques (boundary classification, subduction, rifts, transform faults) | Research doc in docs/research/ with techniques and references | - | cc:TODO |
| 8a.2 | UI: Add "Tectonics Mode" dropdown (Quick / Classified) in Advanced Tweaks | Dropdown visible, defaults to Quick (current behavior) | - | cc:完了 228e3d1 |
| 8a.3 | Plate velocities: physics-derived Euler pole rotation, tangent to sphere, magnitude from tectonics_factor | dot(velocity, center) < 0.1 for all plates; velocity scales with tectonics_factor | - | cc:完了 4535bfa |
| 8a.4 | Boundary classification: convergent/divergent/transform from relative velocity in GPU shader; Quick mode unchanged | Classified mode shows distinct terrain per boundary type | 8a.3 | cc:完了 228e3d1 |
| 8a.5 | Subduction + rift + transform terrain: trench+arc at ocean-continent, broad plateau at cont-cont, rift valley at divergent | Visible terrain differences at each boundary type in Classified mode | 8a.4 | cc:完了 228e3d1 |
| 8a.6 | Performance benchmark: Quick vs Classified at 768px in perf_bench, PASS/WARN against 50ms budget | Timing printed by perf_bench binary | 8a.4, 8a.5 | cc:完了 0f1609d |
| 8a.7 | Performance + visual comparison: screenshot comparison and docs/research/ update | docs/research/performance-analysis.md updated | 8a.6 | cc:TODO |

### Phase 8b: Plate Motion Simulation (continental drift)

| Task | 内容 | DoD | Depends | Status |
|------|------|-----|---------|--------|
| 8b.1 | Research: plate motion algorithms (Euler poles, velocity fields on sphere, collision detection) | Research doc with algorithm selection and tradeoffs | Phase 8a | cc:TODO |
| 8b.2 | Plate velocity field: assign motion vectors to each plate, compute relative velocities at boundaries | Velocity vectors visible in Plates debug view | 8b.1 | cc:TODO |
| 8b.3 | Time-stepping: iterate plate positions over N geological timesteps, accumulate collision/rift history | "Geological age" slider controls timesteps (0=young, 1=ancient) | 8b.2 | cc:TODO |
| 8b.4 | Collision history → terrain: accumulated collisions build mountain chains, rifts create ocean basins | Older planets have more complex terrain from plate history | 8b.3 | cc:TODO |
| 8b.5 | Continental assembly/breakup: plates merge at collisions, split at rifts over time | Supercontinents form and break apart with age slider | 8b.4 | cc:TODO |
| 8b.6 | Performance + visual comparison: benchmark all 3 modes | Timing + visual comparison updated | 8b.5 | cc:TODO |

### Phase 8c: Mantle Convection (future goal)

| Task | 内容 | DoD | Depends | Status |
|------|------|-----|---------|--------|
| 8c.1 | Research: simplified mantle convection models suitable for real-time (Rayleigh-Bénard on sphere) | Research doc with feasibility assessment | Phase 8b | cc:TODO |
| 8c.2 | Convection cell simulation: compute upwelling/downwelling zones on sphere surface | Convection pattern visible in new debug view | 8c.1 | cc:TODO |
| 8c.3 | Derive plate boundaries from convection: plates form between convection cells, motion driven by drag | Plates emerge naturally from convection pattern | 8c.2 | cc:TODO |
| 8c.4 | Integration: convection → plates → boundaries → terrain pipeline | Full convection-driven planet generation works end-to-end | 8c.3 | cc:TODO |

---

## Phase 9: Polish & Distribution

Error handling, cross-platform builds, and documentation.

Error handling, cross-platform builds, and documentation.

| Task | 内容 | DoD | Depends | Status |
|------|------|-----|---------|--------|
| 8.1 | Error handling: GPU errors (OOM, device lost) caught and displayed in UI; graceful fallback messages for unsupported GPUs | Test: simulate OOM → error message shown, app doesn't crash | Phase 5 | cc:TODO |
| 8.2 | Cross-platform CI: GitHub Actions builds for Linux, macOS, Windows; artifacts uploaded to releases | CI green on all 3 platforms; downloadable binaries work | Phase 5 | cc:TODO |
| 8.3 | README: installation instructions, usage guide, parameter reference, example renders | README covers install → first planet → Blender import workflow | 8.2 | cc:TODO |
| 8.4 | Blender addon packaging: zip file with addon Python files, install instructions | Addon installs via Blender Preferences → Install from File | Phase 6 | cc:TODO |
