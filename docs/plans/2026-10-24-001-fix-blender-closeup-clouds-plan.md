---
title: "fix: Natural cloud close-ups in Blender (export band limit, cumulus domes, bridge filtering)"
type: fix
status: completed
date: 2026-10-24
origin: "R: Blender close-ups of planet_risa_06 show small hard blobs; the cloud map looks like patches with flat, hard density levels"
---

# Natural cloud close-ups in Blender

Close renders of the cloud bridge showed clusters of small, hard-edged,
dark-rimmed blobs, and the exported `cloud_density.exr` read as flat white
patches with holes punched in them. Reproduced headless with R's preset
(`planet_risa_06`, Structure 0.63, Detail 0.63, coverage 0.4) at an 8K export,
600 km above a trade cumulus field off the central continent's east coast.

## Causes

1. **Export detail finer than its pixels.** Cloud columns were staged on cube
   faces with the cube texel as the synthesis footprint (`2 / N`), then
   resampled into the `2N x N` equirect map, whose pixels span `pi / N`: 1.6x a
   face-centre texel, 3x a corner one. Structure resolved at the texel (the
   fine band's cloud holes, cell walls) was thresholded there and aliased into
   speckle in the map ("Swiss cheese").
2. **Flat cumulus sheets.** Once the fine band resolves (8K), cumulus clusters
   were one optical depth with holes thresholded out of it; magnified, a flat
   level with hard holes.
3. **Bridge sampling and scale.** The map was sampled linearly, so the contrast
   curve and relief drew the texel grid into every edge; billow cells were
   ~50 km (20 texels at 8K), a second pattern over structure the map already
   holds, while sub-texel detail barely touched the visible margins
   (Erosion 0.15). The relief used a fixed 12.7 km bump per unit of opacity:
   fine on a globe, but up close every puff became a steep-walled lump with a
   dark rim.

## Changes

- `cloud_export_columns.wgsl`: the staged footprint is the equirect pixel,
  `pi / N`. `cloud_capture`'s equirect path already used it, so its crops now
  match the export.
- `cloud_synthesis.wgsl`: resolved clouds are domes. Optical depth follows
  `exp(0.8 hi)` of the fine band, normalized to the mean over the cloudy part,
  so tops are thick and margins thin. Gated by the existing breakup weight, so
  2K/4K exports and unresolved preview pixels are unchanged.
- `cloud_bridge.py`:
  - density sampled with cubic interpolation;
  - `Scale` set by Drive Clouds to four map texels per base cell
    (`2 H / (4 pi)`, ~1300 for 8K);
  - billow octaves and wisps fade to their mean once their cells are a few
    pixels wide (camera distance x 8e-4 rad), so far views thin smoothly
    instead of graining;
  - Erosion default 0.15 -> 0.5;
  - relief bump distance scales with camera distance: 0.002 from 1.5 radii
    out, down to 2 km per unit of opacity up close.
- Tooling: `cloud_capture --file PLANET.json` renders a saved planet through
  the scene path (terrain, erosion, storms); `planet_capture --export FACE_RES`
  runs the app's export (albedo and cloud maps) for a planet file;
  `scripts/cloud_eval/render_bridge.py` renders an export through the bridge
  in headless Cycles.

## Rejected on the way

- Billow octaves in the relief height: even pixel-filtered, the Worley creases
  shade as stone or fabric grain.
- Thickness `(tau / 10)^0.6` as relief height: rounded worm-like lumps up close
  and a crumpled-paper globe.
- A 2x export footprint: smooth but featureless gray patches; the bridge's
  erosion then had to invent all cumulus structure.

## Verification

- `staged_cloud_faces_carry_no_detail_finer_than_the_equirect_pixel`: the
  staged map's pixel-to-pixel roughness over its variance is below the direct
  equirect map's (0.0206 vs 0.0223; the texel footprint gave 0.0251).
- Headless Cycles renders, before/after: close-up (600 km), mid-range
  (2500 km) and globe. "Before" is main's bridge on main's export, emulated
  by synthesizing at the old texel footprint and resampling. The globe keeps
  its look (an earlier version with thickness relief grained it); the
  mid-range trades the before's popcorn clusters and smooth gray veils for
  soft cumulus and fields of small puffs. Images in project files
  `blender-closeup/`.
- App preview close-ups at zoom 20 (seed 42 deck, open cells, cirrus, trade
  cumulus): unchanged apart from slightly brighter cumulus tops.
