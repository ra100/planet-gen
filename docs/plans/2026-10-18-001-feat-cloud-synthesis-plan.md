---
title: "feat: Cloud synthesis (weather systems, regimes, resolution-free detail)"
type: feat
status: completed
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
- Polar lows (kind 3, 2-4 per hemisphere) fill the caps poleward of the
  subpolar low: uniform in area up to 86 degrees, mostly over open water,
  radius 0.15-0.3 of the Eady wavelength, occluded, short trailing front.
  Added after tropical cyclones, only while slots remain.

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
- Polar low: the extratropical template at an occluded stage.

The extratropical twirl is `(0.3 + 1.6 stage)(0.6 + 0.4 intensity)
exp(-r^2)`: at most about one wrap, so lows do not read as whirlpools. The
warm-front shield reaches 0.9-1.6 radii poleward (a broad comma head).

## U2b: Flow-advected noise (`cs_advect`)

The noise domain is carried backward through a steady flow in six steps
(advected texture, Neyret 2003). Each step rotates the point about every
system centre by that system's rate at the current point (`cs_circulation`:
0.45 of its wind-up plus a broad swirl, tapered to zero at its reach), which
is exact for an isolated vortex, plus the storm-track jet and polar
easterlies about the circulation pole, seen from the lows that travel with
the jet core (`CS_JET_ANGLE` 0.6 rad, 12 degree width; `CS_EASTERLY_ANGLE`
0.2). The jet's flanks shear the field into streaks and tilted bands, the
saddles between lows and ridges into filaments, and cyclone cores keep their
own clean wind-up. Fine detail takes half the first step, mesoscale the
first step, synoptic structure all six. With the shear in place the zonal
stretch dropped from 3.2 to 2.2.

## U3: Regimes

Belts from the circulation latitude (`cs_pole`, the climate frame), a
coast-blurred land fraction, continentality and temperature:

| Regime | Where | Cover | Optical depth |
|---|---|---|---|
| ITCZ convective clusters | seasonal thermal trough, 2.8 deg sigma, ocean | 0.42 | 45 |
| Trade cumulus | tropical and subtropical ocean | 0.14 | 3 |
| Stratocumulus decks (closed cells) | subtropical ocean with land to the east (weighted fan of taps 250-2000 km out) | 0.84 | 11 |
| Storm-track stratiform / open cells | 0.95-1.3 Hadley to the subpolar low, ocean | 0.4 | 6.5 |
| Polar stratus | poleward of the subpolar low | 0.58 / 0.3-0.45 land | 6 / 5 |
| Wet tropics | land near the ITCZ | 0.48 | 30 |
| Dry subtropical land | land outside the tropics' rain belt | 0.16 -> 0.02 inland | 4 |
| Mid-latitude land | storm-track land, more cumulus in summer | 0.22-0.38 (+0.08 summer) | 7 |

The solver modulates cover by its regional condensate (450 km, x0.72-1.15),
its convective storms become clusters, its ice raises cirrus cover. Systems
override cover, depth and morphology where their features are.

## U4: Morphology and the two-level remap

- Noise: three fBm bands, synoptic (2400-300 km), mesoscale (150-37 km) and
  fine (19-2.3 km), gain 0.64 (cloud boundary dimension 1.35, Lovejoy 1982).
  The synoptic band falls off from 1200 km (amplitudes 0.7, 0.8, 0.55,
  0.38), so thresholding leaves large clear and cloudy areas for the flow to
  draw into bands. The domain is advected (U2b), compressed along the
  circulation pole (zonal elongation 2.2 that vanishes at the poles) and
  warped with strain ~0.25.
- Cells (Worley, normalized by measured slice statistics): closed cells 30 km
  (bright polygons, wobbling darker borders), open cells 40 km (cloud walls
  around clear centres), convective clusters 450 km with 45 % firing plus
  85 km cells. Blobs are a smooth probabilistic union over the 3x3x3
  neighbourhood, each fading out within one cell; `ragged` swells blobs with
  the fine noise they sit in (the opposite sign drew rings).
- Cloud fraction = macro fraction x fill. The macro field (synoptic and
  mesoscale bands plus cells) is thresholded at the quantile of
  `cover / fill`; the fine band at the quantile of `fill` (decks ~0.94,
  cumulus clusters ~0.5).
- Probability remap: unresolved octaves (smaller than the pixel) become edge
  softness, `mask = Phi((shape - t) / sqrt(unresolved + soft^2))`, so the
  expected cloud fraction does not depend on resolution.
- In-cloud optical depth is lognormal in the excess over the threshold, thin
  margins and thick cores, plus the unresolved variance's lognormal mean
  (`0.605 unresolved`), so 1K and 4K maps keep the same mean optical depth.
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
  water, polar lows in both caps, deterministic, off when disabled.
- `synthesized_cloud_export_is_deterministic_tile_independent_and_seam_safe`.
- Existing suites with synthesis 0 (exact solver path).
- Captures and statistics (`cloud_capture`, seed 42, 2K): see Results.

## Results (seed 42, 2K, `cloud_capture` + `scripts/cloud_eval/analyze.py`)

| | Baseline (solver) | v25 (handoff, 9c6613b) | Final |
|---|---|---|---|
| Cloud fraction R > 0.1 | 0.165 | 0.40 | 0.42 |
| Bright fraction R > 0.4 | 0.036 | 0.126 | 0.226 |
| Clear R < 0.01 | | 0.23 | 0.39 |
| Veil 0.01 < R < 0.1 | | 0.37 | 0.19 |
| Zonal spectrum slope | -2.22 | -1.85 | -1.86 |
| Variance share < 16 px | 0.046 | 0.078 | 0.079 |

Seeds 137 and 7 (final): cloud fraction 0.45 / 0.42, clear 0.37 / 0.38,
veil 0.19 / 0.20. The reference alpha map has 0.79 of its area below 0.1
alpha, but it is a sparse fair-weather composite; it served as a
qualitative guide (big clear areas, flow-aligned streaks), not a target.

## Defects resolved after the handoff

1. Camouflage between systems: flow-advected noise (U2b) and a synoptic band
   that falls off from 1200 km.
2. Gray veil: trade cumulus 0.14 cover, tau 3, tighter edges; cumulus
   clusters 80 % filled and following the large bands; broken cumulus
   margins fade below 4-25 % cover; thickness grows with the shape excess.
   Clear 0.23 -> 0.39, veil 0.37 -> 0.19.
3. Whirlpool cyclones: about one wrap at most, wider core and comma head.
4. Soap-bubble cumulus and ITCZ pearls: the ragged sign followed the noise
   the wrong way (rings); clusters now follow the synoptic and mesoscale
   bands with wider radii, flowers are drawn out along the trades.
5. Offshore coastline copy: a weighted fan of soft taps replaces the max of
   three point probes for the decks' land to the east.
6. Polar caps: polar lows bring circulation to the caps, whose cloud was
   isotropic noise under a solid-body rotation. No artifact sits at the
   circulation poles in the current maps; the horizontal streaks near the
   top and bottom rows of the equirect captures are the projection's
   stretching at the geographic poles.

Known remainder: at 2K the subtropical trade cumulus reads as fine
salt-and-pepper grain (sub-pixel cumulus); it disappears at 8K.

## Tooling

- The `validation` feature's `sweep` binary was already broken before this
  work.
- Iteration loop (no weather regeneration per change):
  ```
  mkdir out && cargo run --release --bin cloud_capture -- out 42 2048 --watch out/job.json > out/log.txt &
  scripts/cloud_eval/run_job.sh out v26                       # 2K global
  scripts/cloud_eval/run_job.sh out v26z src/shaders/cloud_synthesis.wgsl ',"width":8192,"rows":[1200,512]'
  python3 scripts/cloud_eval/analyze.py out 42-v26            # stats + overlay
  echo '{"quit":true}' > out/job.json
  ```
  A copy of the shader with a debug change can be passed as SHADER to A/B a
  hypothesis (that is how cirrus was ruled out for the gray veil, and how a
  debug return of `w_deck` showed the coastline copies). The catalogue is
  built when the server starts, so changes to `cloud_synthesis.rs` need a
  rebuild and restart; the startup render (`clouds-SEED.png`) uses the
  current shader.
- Reference: three.js `examples/textures/planets/earth_clouds_1024.png`
  (alpha channel; NASA-derived; slope -1.37, gx/gy 0.83). The Solar System
  Scope URL is blocked from the cloud container; compare against it locally.
- Lit globe check: `realism_capture` (uploads the synthesis block).
- Close-ups as the app shows them zoomed in (~1 km per pixel at zoom 20):
  `cloud_closeup OUT 42 512 --views "44,157,20;-44,-74,20" --watch OUT/job.json`,
  then write `{"tag":"v2","shader":"/abs/path/cloud_synthesis.wgsl"}` to the
  job file. `--rotation-hours H` and `--wind-scale W` regenerate the planet
  with another day length or wind. Seed 42 at 44,157 is a closed-cell deck,
  -44,-74 storm-track open cells, 37,-113 cirrus over broken cumulus.
