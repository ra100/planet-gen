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
| 4.5.1 | Continuous tectonic regime: replace binary mass/distance threshold with simplified Rayleigh number estimate. Ra ∝ g·ΔT·D³ — use mass→gravity, distance→ΔT, derive Ra continuously. Tectonics factor [0,1] instead of enum for terrain influence | Test: tectonics factor varies smoothly with mass 0.01→10 and distance 0.1→50; Earth params give ~0.8+, Mars ~0.3 | Phase 4 | cc:TODO |
| 4.5.2 | Continuous atmosphere model: replace discrete mass cutoffs (0.3, 2.0 M⊕) with escape velocity-based retention. v_esc = sqrt(2GM/R), compare to thermal velocity of gas species. Atmosphere density varies continuously | Test: atmosphere strength transitions smoothly across mass range; no abrupt jumps at 0.3 or 2.0 M⊕ | 4.5.1 | cc:TODO |
| 4.5.3 | MMSN plausibility check: use Σ(r) = 1700(r/AU)^(-3/2) g/cm² to compute isolation mass at given distance. Show warning in UI if user-input mass exceeds isolation mass (physically implausible without migration) | Test: at 1 AU isolation mass ~0.11 M⊕ shown; at 5 AU ~5-10 M⊕; warning appears for mass > isolation mass | 4.5.1 | cc:TODO |
| 4.5.4 | Fix fBm octave range: minimum 8 octaves per research (currently 6). Range 8-12 driven by surface activity level. Document rotation→lacunarity mapping as artistic (not physics-based) | Test: minimum octaves = 8 for any parameter combination; research reference in code comments | Phase 4 | cc:TODO |
| 4.5.5 | Physically-derived ocean fraction: replace heuristic (0.3+0.4*mass) with water budget model. Water delivery ∝ mass × distance factor (more water beyond frost line). Plate tectonics redistributes water to surface | Test: Earth params produce 0.65-0.75; Mars (0.1 M⊕) ≈ 0.0; icy world at 5 AU with 1 M⊕ ≈ 0.4+ | 4.5.1 | cc:TODO |

---

## Phase 5: Tiled Full-Resolution Generation & Export

Scale from preview (256²) to full 8K (8192²) via tiled generation and export to files.

| Task | 内容 | DoD | Depends | Status |
|------|------|-----|---------|--------|
| 5.1 | Tile coordinator: subdivide each face into 16×16 tiles of 512px, dispatch compute shaders per tile with correct UV offsets | Test: 6 × 256 = 1,536 tiles dispatched; adjacent tiles produce seamless output at borders | Phase 4 | cc:TODO |
| 5.2 | GPU→CPU readback pipeline: read tile results from GPU storage textures to CPU memory, assemble into full-face images | Test: assembled face image matches expected resolution (8192×8192); no visible tile seams | 5.1 | cc:TODO |
| 5.3 | EXR export: write height/displacement as 16-bit float EXR files (one per face or stitched) | Test: exported EXR opens in Blender, values match GPU output within float precision | 5.2 | cc:TODO |
| 5.4 | PNG export: write albedo, normal, roughness, ocean mask, ice mask as 8-bit PNG | Test: exported PNGs open correctly, color values match preview visually | 5.2 | cc:TODO |
| 5.5 | Background generation with progress: full generation runs on background thread, UI shows progress bar (% tiles complete) | Test: UI remains responsive during generation; progress bar updates; cancel button stops generation | 5.1 | cc:TODO |
| 5.6 | Output directory structure: organize exported files in `<output_dir>/<planet_name>/` with consistent naming | Test: generate produces expected file tree with all 6 map types | 5.3, 5.4 | cc:TODO |
| 5.7 | Performance validation: full 8K generation completes in <30s on RTX 3080-class GPU | Benchmark: timed generation from start to all files written, meets target | 5.5, 5.6 | cc:TODO |

---

## Phase 6: Blender Importer Addon

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

## Phase 7: Polish & Distribution

Error handling, cross-platform builds, and documentation.

| Task | 内容 | DoD | Depends | Status |
|------|------|-----|---------|--------|
| 7.1 | Error handling: GPU errors (OOM, device lost) caught and displayed in UI; graceful fallback messages for unsupported GPUs | Test: simulate OOM → error message shown, app doesn't crash | Phase 5 | cc:TODO |
| 7.2 | Cross-platform CI: GitHub Actions builds for Linux, macOS, Windows; artifacts uploaded to releases | CI green on all 3 platforms; downloadable binaries work | Phase 5 | cc:TODO |
| 7.3 | README: installation instructions, usage guide, parameter reference, example renders | README covers install → first planet → Blender import workflow | 7.2 | cc:TODO |
| 7.4 | Blender addon packaging: zip file with addon Python files, install instructions | Addon installs via Blender Preferences → Install from File | Phase 6 | cc:TODO |
