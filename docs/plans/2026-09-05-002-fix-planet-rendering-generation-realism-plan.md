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

### Follow-up — cloud-detail visual regression (2026-09-06)

The user found the new clouds worse: high-contrast, similarly sized camouflage
cells were dominating the weather field. The thresholded low/deep/cirrus noise
introduced above was too strong. Replaced it with bounded, near-unity modulation
at broader scales, attenuated inside dense columns (column mass is the fringe
proxy). Cirrus retains its own wind-filtered detail. Weather formation, lighting,
atmosphere, and coast-independent profile integration are unchanged.

The existing contour test accidentally inherited `cloud_advection = 0`, making
its detail comparison inert. It now enables detail and requires a nonzero fringe
effect. A new GPU test compares all three uniform cloud layers against detail-off
renders and limits core optical-depth changes, preventing the noise-mask regression.
Eight contour seeds retain occupied area within 0.22% and dense mean optical depth
within 0.15%; the original shape-preservation limits remain unchanged.

`realism_capture` now saves `actual-density-no-detail.png` and
`daylight-no-detail.png` alongside the active-detail captures. Visually inspected
seed 42 at 768 px in `/tmp/planet-gen-cloud-detail-fix`: broad structures remain
cohesive and the repeating high-contrast breakup is removed. The underlying coarse
weather resolution is still visible; this correction does not invent a new cloud
simulation to hide it. Focused detail regressions: 2 passed.
Seeds 137 and 999 were also captured and visually checked at 256 px in
`/tmp/planet-gen-cloud-detail-fix137` and `/tmp/planet-gen-cloud-detail-fix999`.
All three seeds preserve live/PNG parity within 1/255 for both color and density.
Full follow-up library suite: **197 passed, 0 failed, 3 ignored**. Changed Rust
files pass scoped formatting checks; `git diff --check` passes.

### Follow-up — blurred cloud shapes and misleading density view (2026-09-06)

The user rejected the softened result as well. Reducing opacity noise exposed
the broad weather blobs; it did not improve their silhouettes. The grayscale
view also had a concrete regression: it read transmittance from the air/cloud
compositor, so atmospheric extinction appeared as cloud density over empty sky.
Density mode now integrates cloud extinction alone, without changing the normal
color compositor. A GPU regression checks empty and populated weather with
atmospheric density 0 and 4; cloud-only pixels must be identical.

Experiments were visually reviewed, not accepted on test counts alone:

- A 256-face-texel spin-up (instead of the 128 cap) retained essentially the same
  blurred forms. Reverted; default weather generation cost/resolution is unchanged.
- Warped opacity noise produced marbling. Finer, stronger opacity noise produced
  a uniformly stippled surface. Both were removed.
- The retained approach displaces the sampled weather boundaries at three
  band-limited scales (13/43/119), in spherical 3D coordinates. It does not apply
  another thresholded occupancy mask. Empty channels stay empty, uniform decks
  retain their interiors, and each layer's detail switch remains independent.
  This is subgrid appearance reconstruction, not new simulated cloud formation
  or a claim of exact column-mass conservation under displacement.
- Normalized-condensate extinction is calibrated from 1.2 to 3.0, shared by
  preview and export. The 6.0 experiment was too opaque. This is an appearance
  calibration, not a measured atmospheric coefficient.
- The shared density shader now uses explicit LOD 0 (all its textures have one
  mip). Removed expression-specific export shader rewriting, which missed the
  newly displaced sample and caused compute-stage validation errors in testing.

The optical-depth tests keep their original relative-error limits. Two dense
fixtures use opacity 0.4 to retain their original effective optical depth and
avoid sRGB8 saturation, where white pixels cannot measure optical-depth changes.
The thin-shell oracle now computes the expected column analytically and accounts
for sRGB8 readback quantization instead of pinning the old calibrated pixel.

Reviewed captures: `/tmp/planet-gen-cloud-verified` (seed 42, 768 px) and
`/tmp/planet-gen-cloud-boundaries-seed137` (seed 137, 768 px). Both include daylight,
density, detail-off, night, and interactive-readback comparisons. Live/PNG maximum
channel difference remains 1/255. Cloud outlines are more irregular without the
repeated interior cells, but the broad weather organization remains stylized;
this is not a claim that the user's realism concern is fully resolved.
The capture tool now also produces density/daylight close-ups at zoom 1.55, with
matching detail-off images. Those were inspected for seed 42. The sparse-support
regression additionally enables displacement and still produces exactly zero
density outside authored support.

Focused export parity checks pass: tiled/direct optical-depth difference is zero,
and maximum shared-edge/corner difference is 0.00004624. Final full library suite:
**198 passed, 0 failed, 3 ignored** (328.85 s). The subsequently strengthened
sparse-support test also passes individually. All-target compilation, scoped
rustfmt checks, and `git diff --check` pass (existing OpenEXR/dead-code warnings
remain). Evidence uses llvmpipe; native GPU frame times have not been established,
and boundary reconstruction adds shader work. Changes remain uncommitted for
visual review.

### Follow-up — remaining large noise patches (2026-09-06)

The user preferred the boundary reconstruction but still noticed large noise
patches. This refinement keeps weather generation, cloud opacity, and the
existing noise streams unchanged:

- Correct the cloud pixel footprint to the differential of an orthographic
  sphere intersection. The old normalized `(x,y,0.5)` proxy doubled footprint
  size at disk center, suppressing the fine octaves there, and underestimated
  the footprint approaching the limb. The new calculation is bounded at the
  silhouette and scales with zoom. Terrain retains its previous filter, so this
  does not silently change terrain normals.
- Reduce broad boundary displacement from 0.035 to 0.018; retain smaller-scale
  displacement at 0.014/0.005. Reduce broad opacity modulation for all three
  layers, shifting the small remaining variation toward finer scales. The
  density-view atmosphere fix and extinction calibration remain unchanged.
- Add a GPU oracle for center, off-center, rotated-axis, limb, outer-limb, and
  zoom footprint behavior. Extend the oracle's readback from 18 to 24 values to
  include these new outputs. Existing cloud shape, support, and core-stability
  limits are not loosened.

Eight contour seeds retain occupied area within 0.36% and mean dense optical
depth within 0.35% of detail-off. Reviewed 768 px captures in
`/tmp/planet-gen-cloud-finer-final` (seed 42) and
`/tmp/planet-gen-cloud-finer137` (seed 137), including zoom-1.55 density close-ups.
Seed 137 was captured before restoring the independent terrain filter; density
is unaffected by that restoration. Broad weather-system interiors are still
present by design; this pass targets the coarse procedural perturbation and
missing finer detail, not a replacement weather model.

Validation: all 13 cloud regressions pass in the final full run, including the
new projection oracle. The full run reports **198 passed, 1 failed, 3 ignored**:
the unrelated exporter cancellation checkpoint test observed 1025 completed rows
instead of 1024 with two workers. Its isolated rerun passes; cancellation code
was not changed. All-target compilation and scoped formatting/diff checks pass.
Live/PNG maximum channel delta remains 1/255 on both captured seeds. Seed 42's
terrain-only PNG is byte-identical to the preceding accepted version. Changes
remain uncommitted for visual review.

### Follow-up — large-scale formation, not renderer detail (2026-09-06)

The user clarified that the large weather masses themselves still read as
noise. The preceding footprint/detail pass did not address this problem.

Controlled captures ruled out two initial hypotheses as the main cause:
disabling independent wind perturbations and bypassing the sparse marine-fetch
factor barely changed the large patches. Both experiments were reverted.

Two generation errors were then identified and corrected:

- The **published diagnosis**, after conservative transport, multiplied low
  condensate by three seeded erosion masks at frequencies 1, 3, and 5. Remove
  these masks; publish the bounded transported low mass directly. The previous
  no-noise regression inspected only spin-up finalization and missed this second
  pass. Extend it to the actual diagnosis and add a GPU reference comparison
  against the sampled final transported state on two seeds.
- Both terrain transects treated underwater relief as atmospheric mountains,
  allowing seabed slopes to create uplift, rain shadows, and cloud-height
  changes. Clamp each transect sample to sea level in both passes. A GPU
  regression now requires bit-identical mass and geometry for flat versus
  ridged seabeds under otherwise identical forcing.

Removing the masks alone exposed excessive warm-ocean blanket formation. Limit
marine vapor-to-condensate conversion using existing resolved convergence,
terrain lift, and frontal lift, while retaining weaker stable-deck formation.
Blend continuously by marine fraction, preserving inland transported-moisture
formation. This is a heuristic formation-rate change, not a new pressure-driven
circulation simulation. It changes conservative phase transfer rather than
painting holes into final cloud mass. Existing transport, coast/plume, coverage,
and water-budget thresholds remain unchanged. Published-output fingerprints
intentionally change with the formation model; compatibility timesteps do not.

Reviewed 768 px seed-42 and seed-137 captures in
`/tmp/planet-gen-cloud-resolved-formation` and
`/tmp/planet-gen-cloud-resolved-137`. The images have more connected decks and
fewer imposed noise cutouts, but broad decks remain prominent; realistic frontal
structure is still a limitation. Preview/PNG channel differences remain at most
1/255. This pass leaves the preceding renderer detail tuning unchanged. No
claim of final visual acceptance or native-GPU performance is made.

Validation: full validation-feature library suite **200 passed, 0 failed,
3 ignored** (379.35 s). The subsequently added GPU transported-mass publication
oracle also passes individually. Both revised fingerprint fixtures pass on
repeat runs. All-target validation-feature compilation, scoped Rust 2024
formatting, and diff checks pass; existing OpenEXR/dead-code warnings remain.
Seed 42's terrain-only capture is byte-identical to the preceding version.
Changes remain uncommitted for visual review.

### Follow-up — height-map contour artifacts (2026-09-08)

The user clarified that the smooth island/continent outlines were in the height
map, not clouds. The preceding cloud-clipping hypothesis did not explain those
screenshots. Inspect terrain in view mode 1, independently of the atmosphere and
clouds; the capture tool now saves full-disk and close-up height views.

Two terrain profile problems were corrected in both the production analytical
and alternate JFA generators using shared `terrain_profiles.wgsl` functions:

- Continental contrast used `sign(x) * abs(x)^0.35`, whose derivative is
  unbounded at zero. This amplifies broad noise zero contours into steep smooth
  lines. Replace it with an odd, monotone regularized curve with bounded slope
  and unchanged +/-1 endpoints. Existing noise streams and fine relief remain.
- Hotspots replaced terrain with an absolute cone inside a fixed footprint.
  On negative seabed, the footprint boundary therefore jumped from seabed to
  zero elevation. Add relative volcanic uplift instead, with value and slope
  tapering to zero at the perimeter; retain pre-existing relief beneath it.

Seed-42 controls: `/tmp/planet-gen-height-before` is the committed baseline;
`/tmp/planet-gen-height-after` changes only hotspots and leaves the broad
contour problem visible; `/tmp/planet-gen-height-continuous` includes both
corrections. Also reviewed seed 137 in
`/tmp/planet-gen-height-continuous137`. Broad noise contours soften without
adding a replacement noise layer. Actual elevation changes are intentional:
seed-42 measured ocean area moves from 65.12% to 67.10% at unchanged sea level,
primarily because fake raised hotspot footprints disappear. Cloud shaders and
weather formation are unchanged, though weather responds to the corrected land.

The GPU profile oracle tests monotonicity, bounded continental slope, symmetry,
endpoints, relative hotspot uplift over land/seabed, preservation of underlying
detail, and continuity at the footprint edge. It also compiles the alternate
JFA path and checks both generators invoke the shared profiles. All 13 terrain
tests pass, as does the subsequently strengthened oracle. All 48 downstream
weather tests pass with validation enabled. All-target compilation
with validation and scoped Rust formatting pass. Captures use llvmpipe; live/PNG
differences remain at most 1/255. Changes remain uncommitted for visual review.

### Follow-up — distinguish cloud-family appearance (2026-09-08)

Terrain fixes were committed as `2b56427` before this pass. The user requested
more cloud variance and visibly different types. This pass changes shared
preview/export density reconstruction, not weather formation or terrain:

- Deeper, dilute low-cloud layers receive small rounded puffs. Thin stable
  decks and dense overcast remain continuous. Low-cloud breakup diminishes
  beneath a deep-cloud column, preventing two competing puff patterns.
- Deep clouds receive a separate, larger-scale lobe field with a distinct seed
  stream. Lobes remain coherent through altitude instead of resampling a new
  pattern on each ray-march step.
- Thin cirrus receives stronger wind-filtered filament contrast, while dense
  high sheets retain the preceding restrained modulation.
- Puffs use overlapping compact spherical kernels with variable radii, not
  another thresholded coarse-noise mask. Their ensemble mean is normalized
  using the kernel integral and expected radius cubed. This is appearance-level
  normalization, not an assertion of exact per-weather-cell mass conservation.
  No density is introduced outside existing layer support. Subpixel puffs blend
  back to unity; dense cores retain the existing preservation gates.

The first seed-42 capture (`/tmp/planet-gen-cloud-families`) was too uniformly
popcorn-like. Reduced low/deep weights and varied radii produce the retained
version in `/tmp/planet-gen-cloud-families-varied` (seed 42) and
`/tmp/planet-gen-cloud-families137` (seed 137). The latter also includes isolated
deck/cumulus/storm/cirrus density captures with constant authored columns, useful
for separating morphology from the generated weather distribution. These are
diagnostic family fixtures, not globally realistic cloud distributions.

The new GPU oracle samples 1024 spherical directions and checks deterministic
output, near-unit ensemble mean, nontrivial bounded variance, distinct low/deep
fields, and exact unity for unresolved puffs. Existing dense-deck, empty-support,
cloud-system, optical-depth, and thin-layer tests retain their original limits.
Shared direct/tiled export parity passes (zero difference; maximum seam delta
0.00018889). Both captured seeds retain live/PNG differences <=1/255. Native GPU
frame time is unmeasured; the kernel adds work when resolved and occupied.
Final focused validation: all 14 cloud-rendering tests pass (167.59 s), plus
the shared export parity test. All-target validation-feature compilation and
scoped formatting/diff checks pass. No full-library rerun was performed in
this appearance-only pass.
Changes remain uncommitted for visual review.

### Follow-up — reject uniform cloud freckles (2026-09-08)

The user rejected the low-cloud result above as noise-like freckles. Variable
radii alone did not solve the dominant single-scale texture. Replace its
frequency-55 field with connected banks (14), subordinate lobes (32, 25% weight),
and restrained fine detail (73, at most 8%, only inside stronger banks). Reduce
the low-family blend from 45% to 35%. These forms still multiply only existing
low-cloud support; dense-deck protection and other cloud families are unchanged.
This is a spatial hierarchy, not just weaker contrast on the rejected dots.

Seed-42 before/after captures are `/tmp/planet-gen-cloud-families-varied` and
`/tmp/planet-gen-cloud-clusters`; second-seed capture is
`/tmp/planet-gen-cloud-clusters137`. The revised height-only/terrain inputs are
unchanged. The dominant low-cloud variation now reads as larger formations
instead of the nearly uniform bright-dot texture. Fine kernels are skipped
outside bank interiors; native GPU frame time remains unmeasured.

Add a GPU regression comparing local spatial differences against the rejected
single-scale field. It requires reduced fine-scale variation while retaining
nonzero variance, near-unit ensemble mean, and exact unity below resolution.
The eight-seed cloud-system test passes unchanged (occupied-area drift <=0.59%,
dense optical-depth drift <=0.41%). Direct/tiled export parity also passes.
All 15 focused cloud-rendering tests pass (214.81 s). The final optimization
skipping unused fine kernels passes the hierarchy oracle again. All-target
validation-feature compilation and formatting/diff checks pass. Both captured
seeds retain live/PNG channel differences <=1/255; seed-42 height PNG is
byte-identical to the preceding version. No full-library rerun was performed.
Changes remain uncommitted for visual review.
