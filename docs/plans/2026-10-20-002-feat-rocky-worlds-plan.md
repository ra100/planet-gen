---
title: Rocky worlds (Moon-like and Mars-like planets)
type: feat
status: completed
date: 2026-10-20
origin: R in the PlanetGen project thread (2026-10-20): extend the range of planets beyond Earth, focus on Moon-like and Mars-like rocky worlds with thin or no atmosphere and past or current tectonics, keep Earth from deteriorating
---

# Rocky worlds

Requirements: [docs/brainstorms/2026-10-20-rocky-worlds-requirements.md](../brainstorms/2026-10-20-rocky-worlds-requirements.md)

## Problem

MOON, MERCURY and MARS rendered as one Earth desert: plate continents with
camouflage blotches, blue lakes, a blue limb, no craters, disk albedo ~0.28
and red/blue ~1.3 for all three. The physics gave the Moon 0.02 bar, Mercury
0.05 bar and Mars 0.16 bar of air and liquid water.

## Approach

Read a rocky surface the way planetary geologists do: as a stratigraphic
history. A stagnant-lid crust keeps every event since the end of the heavy
bombardment, so the terrain is built as a sequence of units rather than as
noise: crust, a hemispheric dichotomy, basin-forming impacts, volcanic
resurfacing (maria, lowland plains, shield provinces), continued cratering
that dates each unit, and degradation. The look follows from the units:
maturity sets albedo, photometry follows regolith, and the atmosphere comes
from escape physics.

### Physics (`src/planet.rs`)

- **Cosmic shoreline** (Zahnle & Catling 2017): insolation against escape
  velocity^4, placed through Mars. A retention factor keeps Earth, Venus
  and Titan exact and leaves Mars ~6 mbar of its ~160 mbar inventory; the
  Moon and Mercury are airless. The greenhouse follows the retained
  pressure.
- **Liquid water needs the triple point** (6.1 mbar). Below it the water
  inventory becomes polar ice (`water_ice`) and the sea level drops to a dry
  datum, so Mars has no lakes.
- **Surface history**: stagnant lid from the tectonics factor; crater age
  4.4 Gyr on a lid, younger for larger bodies; crater erosion from
  pressure; maria on small bodies and shield volcanism on larger ones
  (Tharsis needs a long-lived mantle); dust in thin dry air; `airless`
  below 1 mbar.

### Terrain (`src/shaders/rocky.wgsl`, shared by preview and export)

- **Craters** follow Pike's morphometry: simple bowls d = 0.2 D with
  0.036 D rims; complex craters above 15 km x (1.62 / g) with flat floors,
  central peaks, depth 0.157 T (D / T)^0.301 and peak rings; ejecta thinning
  as r^-3 (McGetchin et al. 1973). Rims are rounded with a smooth minimum
  that widens with degradation.
- **Chronology**: Neukum's N(1 km) sets the crater density, with a
  saturation fill (Gault 1970) so 4.4 Gyr highlands look saturated rather
  than infinitely dense. The fill is normalised by the density the octaves
  place when saturated (0.107 D^-2 per km²), so young surfaces carry the
  chronology's own crater count. Diameters per octave follow a -2
  cumulative slope.
  Each octave hashes 4 craters into cells of 5 diameters inside a 3x3x3
  neighbourhood, so every crater is complete, tiles are seam free and
  export at 8K matches the preview.
- **Stratigraphy per crater**: each crater draws an age rank. Maria, plains
  and volcanic provinces bury craters older than their emplacement; the
  youngest ranks keep bright ejecta. Degradation (rim rounding, infill)
  grows with rank and erosion.
- **Basins**: a catalogue of up to 40 multi-ring basins (floor ring, peak
  ring, rim, outer scarp) with depth ~1.1 D^0.25 km. Younger basins
  excavate older ones instead of stacking on them, and lowland plains bury
  them (Utopia, Isidis). Flooded basins fill with lava to an irregular shore
  inside the rim, biased to one hemisphere like the lunar nearside.
- **Volcanic plains** beyond the basins (Procellarum), the **dichotomy**
  lowland cap (Mars' northern plains, up to a third of the planet), a
  **volcanic rise** on its boundary with shields up to ~21 km and a **rift
  canyon** radial to the rise (Valles Marineris).
- Plate relief fades with the lid; impacts apply in full. Earth's terrain
  bytes are unchanged (the branch is skipped when the lid is zero).

### Look (`preview_cubemap.wgsl`, `albedo_map.wgsl`, `roughness_map.wgsl`)

- **Regolith palette**: space-weathered lunar highlands (~0.14 at 550 nm),
  maria (~0.07, titanium-rich ones bluer), immature ejecta and rays about
  twice as bright and less red. Near the star (Mercury) weathering darkens
  the highlands and plains no longer stand out as dark maria. Steep walls
  show fresher material.
- **Mars**: bright oxidized dust (TES dust-cover index) over darker basalt
  and sand, cleared from steep slopes and valley floors and streaked by the
  winds; polar frost with a residual cap and spiral troughs plus a seasonal
  cap.
- **Photometry**: lunar-Lambert (McEwen 1991) with an opposition surge
  (Hapke 1986) on airless bodies, no skylight so shadows stay black.
- **Cast shadows** of the relief at low sun on regolith worlds: a horizon
  march over the height field on the curved sphere, softened over the
  star's disk (0.53° at 1 AU, scaled with distance). Near zero phase a
  shadow hidden behind its caster is not darkened, so the full-phase limb
  stays even.
- **Mars dust haze**: tau 0.4 with Wolff et al. (2009) single-scattering
  albedo, delta-M scaled forward peak, butterscotch limb instead of
  Rayleigh blue. Thin air also suppresses clouds.

### Controls

TERRAIN > surface history: **Plate activity**, **Crater age** (0-4.4 Gyr),
**Maria**, **Volcanoes** (each AUTO = physics) and **Dichotomy**.
CLIMATE > atmosphere: **Pressure** (10 ubar to 100 bar, log) and **Dust**
(AUTO). Double-click returns an override to AUTO. All are saved in the
planet file (missing = AUTO). Debug view **GEOLOGY** colours the units
(highlands tan, lowland plains green, volcanic red, maria blue, fresh
craters white, rays yellow).

### Past or current tectonics

- **Current stagnant lid** (Moon, Mercury): Plate activity 0, Crater age
  4.4, Maria AUTO.
- **Past volcanism on a lid** (Mars): Plate activity ~0.06, Volcanoes AUTO,
  Dichotomy 1; crater age 3.5-4 for Hesperian plains.
- **Former plates, now a lid**: Plate activity 0.3-0.5 keeps the eroded
  continents and ocean basins under the impact record; set Crater age to
  when plates stopped and Pressure low for an airless relic.
- **Global resurfacing** (Venus-like): Plate activity 0, Crater age 0.5,
  Volcanoes 1, Maria 0.6.
- **Active plates** (Earth): Plate activity AUTO; Crater age above 0 adds a
  sparse young record.

## Validation

`planet_capture` renders each preset from the same pipeline the app uses
(globe, four full-phase views, terminator, close-up, export albedo,
geology, an equirectangular height map) and writes stats. Before = main at
3d2d5f7; after = this branch.

| Body | Metric | Before | After | Reference |
|------|--------|--------|-------|-----------|
| Moon | disk albedo (linear, green) | 0.25 | 0.14 | geometric albedo 0.12 |
| Moon | red/blue | 1.30 | 1.24 | ~1.25 |
| Moon | pressure | 0.019 bar | 0 | none |
| Moon | maria (disk fraction) | - | 0.11 | 16 % of the surface (31 % nearside) |
| Moon | relief min / p01 / max | -3.6 / -2.6 / +14.5 km | -14.5 / -7.7 / +6.1 km | LOLA -9.1 / +10.8 km |
| Mars | red/blue | 1.30 | 3.29 | ~3.5 |
| Mars | pressure | 0.164 bar | 0.006 bar | 0.006 bar |
| Mars | liquid water | 4 % lakes | none, polar caps | none |
| Mars | relief min / p01 / max | -3.2 / -2.6 / +6.3 km | -10.2 / -6.7 / +19.3 km | MOLA -8.2 / +21.2 km |
| Mercury | disk albedo | 0.25 | 0.13 | geometric albedo 0.14 |
| Mercury | pressure | 0.047 bar | 0 | none |
| Earth | all preview captures | | identical (one pixel off by 1 of 262k in the terminator view) | |
| Earth | exported height, albedo, AO, normal, roughness, water mask, emission (water loss 0.5 and 0.35) | | byte-identical to main | |
| Earth | derived values, terrain bytes | | unit tests pin them | |
| Mars | export roughness under the polar frost | | 0.80 (ice) instead of 0.95 (regolith), as in the preview | |

Tests: `earth_is_unchanged_by_the_rocky_surface_model` (derived values),
`stagnant_lid_terrain_records_impacts_tiles_match_and_earth_is_unchanged`
(terrain bytes, relief, tile parity), the export albedo and roughness
parity cases for Moon- and Mars-like surfaces, `surface_overrides_roundtrip`
(planet file), and the physics tests for the shoreline, the triple point
and the history fields.

A second review pass checked the models against the references and found
the shadow march curving the wrong way (a convex sphere drops away below a
straight ray, +d²/2; the first version used -d²/2, which shadowed smooth
ground up to ~9° from the terminator), crater densities twice Neukum's on
unsaturated surfaces, a penumbra twice the solar disk, the Mars cap spiral
wound about the untilted axis, a small ejecta step at the crater cutoff and
the inspector's atmosphere label ignoring a pressure override. All are
fixed; none touches plate worlds.

The export check compares every file a 128-per-face Earth export writes on
main and on this branch (MD5). It caught two regressions an independent
review flagged after the first push: the export albedo had lost its
mountain snow, and one sea-level formula rounded differently in f32 when
water loss was not 0.5, which moved the eroded terrain. Both are fixed;
the terrain and export path keep their own rounding
(`scene::terrain_sea_level`).

## Limits and next steps

- Crater density below the height map's texel is only carried by the
  albedo halos; the close-up shows the smallest craters as dots.
- Relief extremes run deep: a crater inside a basin reaches -14.5 km on the
  Moon (LOLA -9.1 km), and Mercury's basins are as deep as the Moon's
  although its hot lithosphere relaxed them (real range -5.4 / +4.5 km).
- Dichotomy, Tharsis and the canyon are placed per seed rather than in
  Mars' exact arrangement.
- Mercury's lobate scarps (global contraction) and hollows are not modelled.
- Ice moons (Europa, Ganymede) and Venus-like hot surfaces are the natural
  next presets on the same stratigraphic model.
