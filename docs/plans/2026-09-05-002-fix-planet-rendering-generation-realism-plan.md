---
title: "fix: Planet rendering and generation realism"
type: fix
status: complete
date: 2026-09-05
origin: "Planet rendering review and user authorization to implement with Terra/Sol"
---

# Planet rendering and generation realism

## Objective

Make the native preview show a coherently lit rocky planet with plausible atmosphere thickness, readable landforms, and clouds that respond consistently to weather and illumination. Fix the measured rendering and generation defects, verify the result with reproducible images, and preserve existing export functionality.

The user authorized implementation on 2026-09-05. No additional approval is needed for the scoped local changes. Do not commit, push, or publish unless asked. Existing untracked files and the separate `planet_heightmap_generation` project are user work and outside this change.

## Baseline evidence

Review and diagnostic source: `/tmp/planet-gen-realism-review/REVIEW.md` and `capture.rs`. Captures use the current shaders, seed 42, cloud seed 1042, 768-pixel terrain/preview, 384-pixel weather, and native defaults. The renderer ran on llvmpipe; native display composition and hardware performance were not measured.

Measured defects:

- Saved daylight center RGB `(67, 89, 121)` versus interactive target `(14, 25, 48)`.
- Clouds remain bright over the dark hemisphere with atmosphere and city lights disabled.
- Wind receives 0.00007272 radians/second where 1.0 Earth-relative rotation is expected; Hadley boundary is about 60 degrees instead of 30.
- Zeroing plate velocities or changing range width from 0.10 to 0.25 produces identical height cubemaps.
- Earth-like Rayleigh/Mie scale heights are approximately 108/27 km instead of a reference 8/1.2 km.
- Cloud density shows broad, smooth bands and continent-shaped clear regions.
- Weather and biome equatorial baseline temperatures differ by approximately 15 C; their upstream probes disagree in sign.
- GGX omits masking/shadowing; water receives a duplicate specular highlight.
- Reported effective ocean coverage is 31.8%, versus measured surface coverage of approximately 67.1%.

Review correction: `cloud_export.wgsl` already calls the shared land-segment integrator (FE-089). The comment in `cloud_density.wgsl` claiming otherwise is stale. Preserve and verify that existing parity; do not reimplement the old export fix. The remaining concern is coastline-dependent integration and cloud-source behavior.

## Design decisions

1. Use explicit color-space entry points: keep lighting in linear space, tone-map once, and encode once for each presentation target. Debug views must follow the same presentation contract.
2. Keep wind's existing public Earth-relative convention, name it clearly, and convert physical angular velocity exactly once at relevant callers. WeatherSnapshot retains radians/second.
3. Share the climate baseline and elevation conversion across biome, weather, and export consumers. Define base temperature as global-reference temperature, with one documented equator-to-pole profile. Probe upstream opposite the flow vector. Regional effects may vary by consumer only when intentional and named.
4. Atmosphere scale height and shell cutoff are separate physical quantities. Derive plausible scale heights from temperature/gravity, calibrate the Earth default near 8/1.2 km, and integrate far enough for density to be negligible. Share scattering direction conventions, planet solar occlusion, star illumination, and linear compositing with clouds.
5. Keep a bounded realtime renderer. Improve sample placement, integration, and subgrid detail before globally multiplying resolution or ray steps. Preserve deterministic weather and zero-source/zero-coverage behavior.
6. Connect velocity-driven convergent/divergent/transform features to the active `plates.wgsl` path; do not switch wholesale to the currently unused multipass generator. Preserve smooth continent transitions and add detail around actual geological structure.
7. Keep Water loss as the existing authored sea-level control. Report measured solid-angle-weighted ocean coverage rather than labeling the water-budget input as coverage. This preserves the current default's roughly Earth-like water extent. Clearly distinguish water budget/target metadata from measured coverage where both are needed.
8. Treat pressure estimation as a heuristic, independent of the normalized retention score; Earth should calibrate to approximately 1 bar and physically thick cases must not be artificially capped at 1 bar. Do not claim a full atmosphere-composition or climate simulation.

## Work packages and ownership

| ID | Work | Owner | Acceptance | Status |
| --- | --- | --- | --- | --- |
| R1 | Correct live/PNG encoding; repair low/deep detail A/B configuration | Terra | Interactive texture and PNG match within quantization tolerance; all detail switches actually alter nonzero layers | implemented |
| R2 | Cloud solar visibility, night illumination, phase convention, and atmosphere/cloud composition | Terra | Dark-side clouds lose the bright floor; daylight/crescent/limb remain readable; toggles remain independent | implemented |
| R3 | Physical atmosphere scale, curved solar path/planet shadow, star tint; remove or properly gate post-tone-map flare | Terra with Sol's physical properties | Earth reference thickness; no hard outer shell; finite output at full backlighting and extreme supported sizes | implemented |
| R4 | Complete GGX masking/shadowing, single water reflection, physical normal scale/filtering | Terra | No grazing-angle blowup; one shadowed ocean glint; sensible roughness and stable zoom/limb detail | implemented |
| G1 | Fix rotation-unit contracts and align native-default capture inputs | Sol | 12/24/48-hour inputs remain distinct; Earth cells return near expected latitudes; preview/export inputs agree | implemented |
| G2 | Activate tectonic stress and range width; distribute hotspots over the sphere | Sol | Velocity and width changes measurably affect appropriate features; no effect from velocity at zero tectonic activity; no octant bias or new seams | implemented |
| G3 | Measured ocean coverage, consistent preview/export water/erosion state, independent pressure estimate | Sol | UI coverage agrees with area-weighted generated terrain; no stale water/climate state after regeneration; export preserves authored inputs | implemented |
| C1 | Shared climate baseline/elevation and upstream convention | Coordinator after renderer handoff | Matched positions have matching baseline temperatures; upstream mountain dries lee side; biome/weather/export remain consistent | implemented |
| C2 | Improve cloud subgrid structure and remove artificial coastline integration discontinuity | Terra; coordinator validates across climate update | Broken cells/filaments and structured decks at 768px; smooth prescribed fields do not acquire coast outlines; density remains supported by weather mass | implemented |
| V1 | Permanent diagnostic captures and behavioral regressions; final integration review | Coordinator | Before/after images and relevant test results recorded; no unexplained new failures | verified |

File ownership while agents run: Terra owns `src/preview.rs`, `src/shaders/preview_cubemap.wgsl`, `src/shaders/cloud_density.wgsl`, `src/shaders/cloud_export.wgsl`, and rendering tests. Sol owns `src/app.rs`, `src/planet.rs`, `src/plates.rs`, `src/terrain_compute.rs`, `src/shaders/plates.wgsl`, `src/shaders/wind_field.wgsl`, `src/export.rs`, and its focused tests. Coordinator owns the new diagnostic executable, plan, and later shared-climate integration. Coordinate before crossing file boundaries or changing shared uniform layouts.

## Sequence

1. Preserve baseline artifacts and create a reproducible native-default diagnostic. Start R1-R4/C2 and G1-G3 in parallel with separate file ownership.
2. Integrate physical atmosphere properties and updated generation into the renderer. Then unify climate functions and upstream behavior across consumers.
3. Re-render fixed views after the foundational fixes. Tune cloud structure and landform hierarchy using those images; record what improved and what remains approximate.
4. Run focused behavioral tests, then the relevant library/export suites and formatting checks. Update expected fingerprints only after confirming that changes are intentional, deterministic, and physically justified.
5. Update this plan with changes, results, and remaining limitations, and hand off the implemented local changes.

## Validation

Use small deterministic GPU fixtures for correctness, and 768-pixel captures for visual judgment. Include seeds 42, 137, and 999 where practical; use full daylight, side light, crescent, full backlighting, clouds off, atmosphere off, surface only, and density views. Keep camera, seed, and exposure fixed for before/after comparisons.

Required behavioral checks:

- Live/readback color parity, including dark colors and debug views.
- Night cloud radiance decreases markedly when direct sunlight is removed; zero clouds/zero mass stays empty.
- Forward scattering peaks for the correct physical geometry; zero-length half-vector cases remain finite.
- Earth wind ratio is 1.0; ordinary rotation controls do not collapse to a shared clamp.
- Plate velocities and range width affect active terrain; output stays deterministic and cubemap seams remain bounded.
- Ocean coverage measurement uses solid-angle weights and reflects current generated terrain.
- Climate baseline and upstream convention agree across preview, weather, and exports.
- Cloud profile integration does not introduce a land-mask edge on otherwise identical columns; preview/export optical-depth semantics agree.
- Relevant export cancellation/memory contracts continue passing; do not expand 8K memory or GPU budgets silently.

Run `rtk cargo fmt -- --check`, focused tests, and `rtk cargo test --features validation --lib` after integration. Inspect existing failures before updating assertions. Older plans identify parked U15 gates and environmental performance failures; these are context, not automatic exemptions. Report observed failures and do not loosen thresholds to hide defects. Hardware timing claims require hardware; llvmpipe timings are diagnostic only.

## References

- [Bruneton Earth atmosphere parameters](https://ebruneton.github.io/precomputed_atmospheric_scattering/atmosphere/demo/demo.cc.html)
- [PBRT phase conventions](https://www.pbr-book.org/4ed/Volume_Scattering/Phase_Functions)
- [PBRT microfacet masking/shadowing](https://www.pbr-book.org/4ed/Reflection_Models/Roughness_Using_Microfacet_Theory)
- [PBRT Fresnel and water refractive index](https://www.pbr-book.org/3ed-2018/Reflection_Models/Specular_Reflection_and_Transmission)

## Implementation results

Terra and Sol were launched as requested and made partial changes, then both stopped with usage-limit errors. The coordinator audited their unfinished edits, corrected integration issues, and completed the local implementation. No commits or external publication were made.

### Changes

- Separate linear/offline and gamma-encoded/egui fragment entry points; the live attachment remains UNORM. Hardware sRGB encoding is used only for offline capture.
- Earth reference molecular/aerosol scale heights are approximately 8/1.2 km; shell cutoff is 12 molecular scale heights (~96 km). Scale height varies with temperature/gravity. Curved solar-path integration, planet shadow, a consistent orthographic phase convention, shared star illumination, and linear air/cloud interleaving replace the thick blue veil and artificial night floors. The post-tone-map flare is removed.
- GGX now includes Smith masking/shadowing, a safe backlit half vector, physical water Fresnel, and one cloud-shadowed reflection. Terrain normal strength uses kilometers/radius and footprint-aware central differences.
- Low/deep detail switches now actually work. Detail frequencies span larger decks and smaller cells, with footprint attenuation that no longer divides its own filter out. Thin layer integration applies to low/deep/high clouds over both land and water, with no terrain-dependent cap or integration blend.
- Physical rotation rates are converted at production/export/diagnostic callers. Wind is regenerated with the weather snapshot, including after erosion, rather than retaining an earlier field.
- The active terrain shader uses plate motion for collision belts, spreading ridges/rifts, and shear relief. Range width now affects these belts; inherited intraplate ranges are subdued. Hotspots use uniform azimuth/polar cosine instead of a positive-octant direction. Unresolved detail octaves are attenuated.
- Preview, weather initialization/spin-up, and albedo/roughness/emission exports use shared climate functions. Upstream is opposite flow. Preview ocean-current anomalies remain explicitly separate. The warmer corrected climate exposed early saturation of the coverage control; source supply now also responds continuously to coverage, stays bounded, and preserves the default midpoint.
- Pressure is a documented inventory heuristic calibrated to 1 bar for Earth, independent of the retention score. OCEAN displays measured solid-angle-weighted terrain coverage, while WATER BUDGET displays the authored input. Export respects the erosion toggle.

### Evidence and reproducibility

New diagnostic: `src/bin/realism_capture.rs`.

```sh
rtk cargo run --bin realism_capture -- /tmp/planet-gen-realism-after-final 42 768
rtk cargo run --bin realism_capture -- /tmp/planet-gen-realism-seed137 137 256
rtk cargo run --bin realism_capture -- /tmp/planet-gen-realism-seed999 999 256
```

These output default/crescent, daylight, complete backlighting, cloud/atmosphere toggles, surface-only, density, and interactive-readback images. Seed 42 is captured at the baseline's 768 px; other seeds are 256 px. All use cloud seed = planet seed + 1000 and a 384 px weather field.

Measured after the fixes:

- Maximum live/PNG channel delta: **1/255** for default, daylight, and cloud-density views on all three seeds.
- Earth: **1 bar**, **7.996 km** molecular scale height, **95.950 km** shell cutoff.
- Seed 42 actual ocean coverage: **65.12%**, distinguished from **31.82%** water budget. Other captured seeds: 67.90% and 62.34%.
- Plate-velocity and range-width perturbations both change generated terrain. The zero-activity regression verifies that neither can inject active boundary relief when tectonics are off.
- Dedicated GPU regression checks the 6.5 C/km lapse rate, sea-level invariance, GGX peak and grazing masking, cloud day/night contrast, and live/readback parity.
- Coverage fixture occupied fractions are 20.41%, 45.35%, and 53.63% for controls 0.25/0.50/0.75. Original monotonicity, minimum expansion, and support-retention gates are retained.

Visual inspection: the night hemisphere is dark, the limb is thin, the duplicate glint is gone, and clouds have broken multiscale structure. This is a corrected procedural renderer, not a claim of photorealistic atmospheric fluid simulation.

### Validation notes

- Baseline library suite: 190 passed, 3 ignored.
- New focused realism regressions: 5 passed.
- All-target compilation succeeds (existing dead-code/OpenEXR-discovery warnings remain).
- Changed Rust files pass scoped rustfmt checking; `git diff --check` passes. Repository-wide `cargo fmt -- --check` still identifies pre-existing formatting in untouched `src/ui/theme.rs` and `src/ui/widgets.rs`; those files are intentionally preserved.
- Final integrated library suite: **196 passed, 0 failed, 3 ignored** (`rtk cargo test --features validation --lib -- --test-threads=2`). An earlier in-flight run compiled a reserved WGSL identifier introduced during editing; that error was fixed before this successful final run.
- Test fixtures now illuminate material/visibility checks from the day side, isolate shadow structure on neutral material, require identical cloud columns over ocean/coast/land, and use a sufficiently cold global temperature for frozen-source checks. Climate fingerprints were regenerated for the explicitly changed temperature model. Assertions were not relaxed to conceal defects.

### Remaining approximations / limits

- Single-scattering atmosphere plus a bounded cloud multiple-scattering approximation; no composition-dependent spectra, full climate simulation, or volumetric terrain silhouette.
- Large cloud systems still come from the existing finite weather spin-up. Some broad decks and ocean-dominant distributions remain stylized; the renderer no longer introduces a separate coastline discontinuity.
- Export moisture/material classification remains a simplified approximation even though the temperature baseline is shared.
- All GPU evidence here used llvmpipe. Native-window composition and hardware frame times were not measured; three pre-existing manual performance tests remain ignored.
