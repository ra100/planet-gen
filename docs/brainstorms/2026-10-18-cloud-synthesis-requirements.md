# Cloud synthesis requirements

Date: 2026-10-18
Origin: "I strive to get this variance and quality of clouds from the planetgen
cloud layer (Solar System Scope `2k_earth_clouds.jpg`), of course in 2-8K
quality; now it's still just quite boring noise. What would a professional in
this field do?"

## Problem

Measured on the default planet (seed 42) with `cloud_capture`, the exported
cloud layer (`cloud_density.exr`, same shader as the preview) against Earth
climatology (ISCCP/MODIS):

| | Planet Gen | Earth |
|---|---|---|
| Cloud fraction (reflectance > 0.1) | 0.17 | ~0.6-0.67 |
| 50-60 S ocean | 0.16 | ~0.85 |
| Bright cloud (reflectance > 0.4) | 4 % of the sphere | ~25 % |
| Detail below 16 px at 2K | 4.6 % of variance | (resolution independent) |

The causes are structural, not a tuning problem:

1. Resolution. The solver resolves condensate at 384 texels per face (26 km
   on Earth, spun up at the same grid); 2-8K exports bilinearly magnify it.
   Nothing below one solver texel exists.
2. Organization. The solver has a three-cell circulation and noise eddies but
   no weather systems: no extratropical cyclones with comma clouds and
   trailing cold fronts, no dry slots, no tropical cyclones, no ridges.
3. Morphology. Every region shows the same wind-streaked ripple; there is no
   vocabulary of cloud types (stratocumulus closed cells, open cells behind
   cold fronts, trade cumulus fields, ITCZ convective clusters, cirrus
   streaks, stratiform frontal shields).
4. Contrast. Condensate is a smooth, mostly thin field, so the map is a gray
   veil instead of the bimodal clear/bright distribution of satellite images.

## What professionals do

Production planet and sky renderers (Horizon's Nubis, SpaceEngine, film
planet work) and satellite-image synthesis separate *where* cloud is from
*what it looks like*: a low-resolution weather or climate field sets cloud
fraction and type, synoptic systems are placed explicitly, and resolution-free
procedural detail conditioned on the cloud type fills each column, thresholded
by coverage (the coverage remap) so edges stay sharp and the amount stays
right.

## Requirements

- R1. Keep the solver as the source of physical hints: its condensate sets the
  regional moisture, its convective storms become clusters, its ice becomes
  cirrus. Strength 0 renders exactly the previous field.
- R2. Synoptic systems from physics, not random stamps: storm-track cyclones
  counted and sized from the most unstable baroclinic wavelength (so faster
  rotators get more, smaller lows), life-cycle stages from open wave to
  occluded spiral, ridges between them, secondary frontal waves, tropical
  cyclones only over warm open ocean.
- R3. Climatological regimes from the circulation belts the wind field already
  uses (Hadley edge, subpolar low, seasonal ITCZ trough), the surface and the
  temperature: ITCZ clusters, trade cumulus, west-coast stratocumulus decks,
  storm-track stratiform and open cells, polar stratus, dry subtropical land,
  jet cirrus.
- R4. Zonal cloud fraction close to Earth's: ITCZ maximum, subtropical
  minimum (dry land lowest), storm-track maxima near 0.8, global near 0.6.
- R5. Resolution independent: the expected cloud fraction of a pixel is the
  same at 2K and 8K; finer exports resolve more structure without aliasing,
  shimmer or seams (cube faces, poles, longitude wrap).
- R6. One control ("Structure", 0-1, default 1) shared by the preview and the
  export, saved with the planet file. Coverage and moisture keep their
  meaning.
- R7. Preview and export stay one shader (`cloud_synthesis.wgsl`), evaluated
  once per column; existing consumers of `cloud_density.exr` (the Blender
  bridge) keep working.
