---
title: Tropical cyclone look and Storms controls
type: feat
status: completed
date: 2026-10-20
origin: R in the PlanetGen project thread (2026-10-20): the storm eye and the whole storm looked weird; implement the neglected Storms controls; check how experts approach it
---

# Tropical cyclone look and Storms controls

## Problem

- With Clouds > Structure on, a tropical cyclone rendered as a small puffy
  ring with a black pinhole eye, often beside a long unrelated cloud arc.
- Storms > Storm count and Storm size only seeded storm regions in the weather
  solver. Synthesis placed 0-3 tropical cyclones at random regardless of them,
  and nothing set how strong a storm is.

## What forecasters look at

- **Dvorak technique** (satellite intensity estimation): a storm's cloud
  pattern walks from a *curved band* wrapped part-way round the centre
  (T1-4.5), through the *shear* pattern (convection displaced from an
  exposed low-level centre), a *central dense overcast* (CDO, T2.5-5) and
  an *embedded* or *banding* eye, to the *eye* pattern (T4.5-8) where a
  clear warm eye sits in a ring of cold cloud.
  [Wikipedia](https://en.wikipedia.org/wiki/Dvorak_technique),
  [WMO training](https://severeweather.wmo.int/TCFW/RAI_Training2023/05_WMO-RAI_TCTrainingCourse_2023_Dvorak.pdf)
- **Structure** (Smith, LMU lecture notes on TC observations): eye radius
  small <= 15 km, medium 16-30 km, large 30-120 km; eyewalls slope outward
  30-60 degrees from vertical (the "stadium"); broken stratocumulus in the
  lowest kilometre of the eye; spiral rainbands 5-50 km wide and 100-300 km
  long with clear lanes a bit wider than the bands, crossing circles at
  10-25 degrees; anticyclonic cirrus outflow beyond a few hundred km.
  [Observations of Tropical Cyclones](https://www.meteo.physik.uni-muenchen.de/~roger/TCLecs/Tropical%20Cyclones_Observations.html)
- **Polygonal eyes** from eyewall mesovortices (Schubert et al. 1999):
  slightly polygonal, elliptical eyes in strong storms.

## Causes found

1. **Fronts cut the storm.** Templates were averaged by weight. The trailing
   tail of a storm-track cold front reaches ~3 radii (thousands of km) toward
   the tropics; where it crossed a tropical cyclone it halved the averaged
   cover, so the threshold cut the overcast along a straight chord. That is
   the "long arc next to a storm" and most of the ragged, holed core.
2. **Leaking weight.** The storm's weight was scaled by intensity, so the
   background regime's cover leaked in and the threshold punched holes into
   the overcast and filled or emptied the eye by chance.
3. **The eye depended on noise luck.** The eye (~20 km) is smaller than the
   noise features thresholded against it, so it came out either a perfect
   black hole or fully clouded.

## Changes

- `cloud_synthesis.rs`: Storm count (0-8) is the number of tropical cyclones
  on warm open ocean, scaled by eligibility (none below ~22 C tropical sea
  surface, all by ~26.5 C); Storm size scales the shield (~520 km radius on
  Earth at 1); new Storms > Intensity (0-1, default 0.7, `storm_intensity`
  in planet files and `ExportConfig`), each storm 0.7-1.2x of it. Storms keep
  apart, form between 8 degrees and 0.8 of the Hadley edge, and carry eye
  radius (12-45 km, smaller when stronger), spiral tightness, shear direction
  and shear in `extra`. `weather_systems_with_storms` takes the intensity.
- `cloud_synthesis.wgsl` `cs_tropical`: Dvorak sequence by intensity. Curved
  principal band, CDO growing with organization and displaced downshear in
  weak storms, eye opening between 0.5 and 0.85 with a slightly polygonal
  outline, eyewall ring, moat, principal / secondary / inner log-spiral bands
  of different lengths (not a symmetric pinwheel), cirrus canopy clear of the
  eye. Full weight over the storm; tropical templates override other systems
  (`cs_overlay`) instead of being averaged.
- Eye: the column's optical depth falls geometrically down the sloped wall
  onto broken low stratocumulus from the fine band (a third of the floor).
- Noise: each storm winds the noise around its centre by
  `k ln(1 + 1/r^2) / 2` plus a broad swirl, applied to the start point before
  the jet flow (applied after it, the angle swept points thousands of km and
  smeared the noise into rings). Inside the storm the shape takes 30 % of its
  synoptic band from the mesoscale band, so storm-sized noise does not erase
  whole arms.
- `cloud_closeup` prints storm centres in view coordinates and its watch jobs
  take `storm_intensity`, `storm_count`, `storm_size`.

## Results

Seed 42, `cloud_closeup` at zoom 5 / 14 / 40 (before: main, after: this
change), intensity sweep 0.15 / 0.45 / 0.65 / 1.0 and sizes 0.4 / 2.5 at
zoom 8 / 6 were reviewed; storm look tests in `cloud_synthesis.rs`
(`storm_controls_set_count_size_and_intensity`,
`cool_tropics_get_fewer_of_the_requested_storms`,
`tropical_cyclones_form_between_eight_degrees_and_the_hadley_edge`).
Only columns within a tropical cyclone change; everything else renders as
before.
