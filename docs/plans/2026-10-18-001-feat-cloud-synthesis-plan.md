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
