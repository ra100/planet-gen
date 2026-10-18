---
title: "feat: Cloud synthesis (weather systems, regimes, resolution-free detail)"
type: feat
status: active
date: 2026-10-18
origin: "docs/brainstorms/2026-10-18-cloud-synthesis-requirements.md"
---

# Cloud synthesis

The weather solver decides where moisture and cloud types are, on a 384 per
face grid with no weather systems. Synthesis rebuilds every column's cloud on
top of it: synoptic systems placed from baroclinic physics, climatological
regimes from the circulation belts, and resolution-free procedural morphology
thresholded at each regime's cloud fraction. One shader,
`cloud_synthesis.wgsl`, serves the preview and the cloud export; the
`cloud_synthesis` strength (Clouds > Structure) blends from the solver field
(0, exact) to the synthesized cloud (1, default).

## U1: Weather system catalogue (`cloud_synthesis.rs`)

`weather_systems(snapshot, strength, is_ocean)` is a pure function of the
weather snapshot, so preview and export build identical systems:

- Storm-track latitude between the Hadley edge and the subpolar low (the wind
  field's own formulas), 3 degrees equatorward and 12 % more numerous in the
  winter hemisphere.
- Cyclone spacing is the most unstable Eady wavelength, 4000 km on Earth at 45
  degrees, scaled by `1 / (Omega sin lat)`: Earth gets 6-8 lows per hemisphere,
  a planet rotating 3x faster about three times as many, smaller ones.
- Each low has a life-cycle stage (young waves equatorward, occluded lows
  poleward), intensity, aspect, front length, curvature and tilt.
- Mature lows spawn a secondary wave on their trailing cold front (55 %).
- A ridge (anticyclone) sits between consecutive lows.
- Tropical cyclones (0-3) form at 10-24 degrees, mostly in the summer
  hemisphere, only when the tropics exceed ~24 C and only over open water
  (centre and four points 300 km out), so preview and export agree near coasts.

48 systems fit the uniform block (`CloudSynthesisGpu`, 32 + 48 x 64 bytes).

## U2: System templates

In local coordinates (x east, y poleward, one system radius = 1):

- Extratropical cyclone: an open frontal wave (cold-front band bowing east and
  waving, warm-front cloud shield, warm and cold sectors) is un-wound by a
  twirl that grows with stage: young waves keep a straight front, mature lows
  curl the shield into a comma head with a dry slot, occluded lows become a
  spiral trailing a long front. The band's cold edge is sharp, its leading
  edge diffuse, and it widens toward the low.
- Tropical cyclone: eye, central dense overcast, two log-spiral rainbands
  (`theta + k ln r`), cirrus outflow.
- Ridge: subsidence lowers cover toward a thin stratocumulus sheet.

Each system also bends the noise domain around it (backward rotation about its
centre). Synoptic structure takes the full wind-up, mesoscale a fraction, fine
detail almost none: small eddies live too briefly to be wound into coherent
spirals. A broad swirl, tapered to zero inside the system's reach, gives the
S-shaped streaks between lows and ridges.

## U3: Regimes

Belts from the circulation latitude (`cs_pole`, the climate frame), a
coast-blurred land fraction, continentality and temperature:

| Regime | Where | Cover | Optical depth |
|---|---|---|---|
| ITCZ convective clusters | seasonal thermal trough, 3.5 deg sigma, ocean | 0.42 | 45 |
| Trade cumulus | tropical and subtropical ocean | 0.22 | 3.5 |
| Stratocumulus decks (closed cells) | subtropical ocean with land 450-1750 km to the east | 0.84 | 11 |
| Storm-track stratiform / open cells | 1.0-1.3 Hadley to the subpolar low, ocean | 0.46 | 11 |
| Polar stratus | poleward of the subpolar low | 0.68 / 0.35-0.5 land | 10 / 7 |
| Wet tropics | land near the ITCZ | 0.48 | 30 |
| Dry subtropical land | land outside the tropics' rain belt | 0.16 -> 0.02 inland | 4 |
| Mid-latitude land | storm-track land, more cumulus in summer | 0.3-0.45 | 11 |

The solver modulates cover by its regional condensate (450 km, x0.72-1.15),
its convective storms become clusters, its ice raises cirrus cover. Systems
override cover, depth and morphology where their features are.

## U4: Morphology and the two-level remap

- Noise: three fBm bands, synoptic (2400-300 km), mesoscale (150-37 km) and
  fine (19-2.3 km), gain 0.64 (cloud boundary dimension 1.35, Lovejoy 1982),
  the two largest octaves damped. The domain is compressed along the
  circulation pole (zonal elongation that vanishes at the poles) and warped
  with strain ~0.25.
- Cells (Worley, normalized by measured slice statistics): closed cells 30 km
  (bright polygons, wobbling darker borders), open cells 40 km (cloud walls
  around clear centres), convective clusters 450 km with 45 % firing plus
  85 km cells.
- Cloud fraction = macro fraction x fill. The macro field (synoptic and
  mesoscale bands plus cells) is thresholded at the quantile of
  `cover / fill`; the fine band at the quantile of `fill` (decks ~0.94,
  cumulus clusters ~0.5).
- Probability remap: unresolved octaves (smaller than the pixel) become edge
  softness, `mask = Phi((shape - t) / sqrt(unresolved + soft^2))`, so the
  expected cloud fraction does not depend on resolution.
- Cirrus: separate layer, stretched six-fold along the jets, thin (ice mass
  0.1-0.3).
- Masses: physical optical depth is mapped back through the preview/bridge
  curve, `tau = 36 (d - 0.05) t^0.6`, and split into low and deep families
  by regime; cirrus is the high family.

## U5: Integration

- `cloud_density.wgsl`: explicit-mass variants of the column, layer and
  segment samplers (`cloud_column_sample_mass`, ...). Solver-only paths sample
  exactly as before.
- Export: `CloudExportPipeline` binds the catalogue at binding 8;
  `cloud_density.exr`, `cloud_height.exr` and `clouds.exr` carry the
  synthesized columns; `cloud_drivers.exr` stays the solver's.
- Preview: binding 7; the synthesized column is evaluated once per ray (like
  detail noise) and once for the surface shadow; billow erosion fades out
  under synthesis; Maps > Cloud density / height show the synthesized column.
- App: Clouds > Structure (0-1, default 1), `PlanetFile::cloud_synthesis`
  (missing loads as 1), `ExportConfig::cloud_synthesis`; the catalogue is
  rebuilt when the front weather snapshot or the slider changes.
- Blender bridge: default billow Erosion 0.35 -> 0.15.
- `cloud_capture` renders the exported column to equirectangular maps and can
  watch a job file to iterate on the shader without regenerating weather.

## Verification

- Catalogue unit tests: lows per hemisphere, latitude band, radius, faster
  rotation gives more and smaller lows, tropical cyclones need warm open
  water, deterministic, off when disabled.
- `synthesized_cloud_export_is_deterministic_tile_independent_and_seam_safe`.
- Existing suites with synthesis 0 (exact solver path).
- Captures and statistics (`cloud_capture`, seed 42, 2K): see Results.

## Results so far (seed 42, 2K, `cloud_capture` + `scripts/cloud_eval/analyze.py`)

| | Baseline (solver) | v23 (commit dbaa872) | v25 (current) |
|---|---|---|---|
| Cloud fraction R > 0.1 | 0.165 | 0.44 | 0.40 |
| Bright fraction R > 0.4 | 0.036 | 0.14 | 0.126 |
| Zonal spectrum slope | -2.22 | -1.78 | -1.85 |
| Variance share < 16 px | 0.046 | 0.093 | 0.078 |
| Area with R < 0.01 (really clear) | | | 0.23 (reference alpha map: 0.79) |

v25 = v23 + measured simplex sigma (0.373, was assumed 0.26: low-cover
regimes were inflated, decks deflated), smooth probabilistic union in
`cs_blobs` (re-measured stats), 9-tap coast-blurred land fraction on a
circulation-pole ring (no seam at |y| > 0.9).

Reached: Earth-like zonal profile (ITCZ, subtropical minimum, storm tracks
0.7-0.78), coherent cyclones (comma head, dry slot, trailing front) at 8K,
ITCZ clusters, resolution-independent cover. Seeds 137 and 7 coherent.

## Handoff: open defects (v25 vs the NASA-derived reference)

1. **Gray veil instead of black clear sky.** 37 % of the sphere has
   0.01 < R < 0.1 (reference 14 %). Not cirrus (removing it changes < 3 %).
   Sources: trade-cumulus regime (cover 0.2, tau 1.8, fill 0.5 rendered as
   uniform gray at 2K) and wide logistic tails (`edge = soft + 0.25 wm.w`,
   soft 0.35 for cumulus). Satellite composites show sub-pixel cumulus as
   black or sparse speckle, so bias toward that look: lower trade cumulus
   tau/cover, smaller `soft`, maybe a cutoff `mask *= smooth_step(0.05, 0.2, mask)`
   for cumulus regimes only.
2. **"Camo" between systems.** Storm-track background (cover 0.4) thresholds
   isotropic fBm into black blotches on gray; the reference has big clear
   areas and flow-aligned streaks. Recommended fix (the main next step):
   **kinematic flow advection of the noise domain** (Neyret 2003 advected
   textures / Perlin-Neyret flow noise). Replace the one-shot per-system
   rotations in `cs_synoptic` with K = 6 backward steps; each step composes a
   rotation about every system centre by its angular rate
   `omega_i(d) = (0.55 twirl_i(d) + swirl_i(d)) * taper_i / K` re-evaluated at
   the current point (exact for one vortex, so isolated systems look as now),
   plus a differential zonal jet rotation about the circulation pole
   (`exp(-((|lat| - track)/12 deg)^2)`). Take the fine point after step 1,
   meso after step 2, synoptic after step 6 (replaces the 0.1/0.15/0.55
   fractions). Saddles between lows and ridges then stretch cloud into the
   filaments and tilted SW-NE bands the reference shows; reduce
   `CS_ZONAL_STRETCH` once the jet shear exists.
3. **Whirlpool cyclones.** Too tight, thin multi-arm spirals: lower twirl
   amplitude `(0.45 + 2.7 stage)` toward ~`(0.3 + 1.6 stage)`, widen the comma
   head, keep one wrap at most for occluded lows.
4. **Soap-bubble cumulus.** Flowers (110 km) and small Cb blobs (85 km) are
   round and uniform in size: elongate along the flow (use the advected
   point), widen the radius range, raise `ragged`, dim their tau.
5. **Offshore copy of the coastline.** `east_land` for the stratocumulus
   decks takes the max of 3 point probes (450/1100/1750 km east), so the deck
   edge is the coastline translated. Blur each probe (small ring like the
   land fraction) or use a smooth weight over 6+ probes.
6. Polar caps: smooth gray with dark streak artifacts at the circulation
   pole in the equirect map; check the zonal stretch near the pole.

## Handoff: status and how to continue locally

- Committed and pushed on `claude/dazzling-johnson-ha0i6a`. Everything
  builds with `cargo build --release --all-targets` (the `validation`
  feature's `sweep` binary was already broken before this work).
- **Not yet run:** `cargo test --release --lib` (new tests:
  `synthesized_cloud_export_is_deterministic_tile_independent_and_seam_safe`,
  `synthesized_cloud_amount_does_not_depend_on_export_resolution`,
  `cloud_synthesis_restructures_the_preview_and_strength_zero_is_exact`, and
  the 5 `cloud_synthesis` catalogue tests, which did pass), integration
  tests, `cargo fmt`, `cargo clippy --all-targets`.
- Iteration loop (no weather regeneration per change):
  ```
  mkdir out && cargo run --release --bin cloud_capture -- out 42 2048 --watch out/job.json > out/log.txt &
  scripts/cloud_eval/run_job.sh out v26                       # 2K global
  scripts/cloud_eval/run_job.sh out v26z src/shaders/cloud_synthesis.wgsl ',"width":8192,"rows":[1200,512]'
  python3 scripts/cloud_eval/analyze.py out 42-v26            # stats + overlay
  echo '{"quit":true}' > out/job.json
  ```
  A copy of the shader with a debug change can be passed as SHADER to A/B a
  hypothesis (that is how cirrus was ruled out for defect 1).
- Reference: three.js `examples/textures/planets/earth_clouds_1024.png`
  (alpha channel; NASA-derived; slope -1.37, gx/gy 0.83). The Solar System
  Scope URL is blocked from the cloud container; compare against it locally.
- Lit globe check: `realism_capture` (uploads the synthesis block).
- After the fixes: run tests/fmt/clippy, stamp Plans.md Phase 15 rows
  `cc:完了 [hash]`, set this plan `status: completed`.
