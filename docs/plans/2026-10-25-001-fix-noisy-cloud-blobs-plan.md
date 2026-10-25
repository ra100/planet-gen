---
title: "fix: Exported clouds cut at two pixels, so Blender close-ups lose the noisy blobs"
type: fix
status: completed
date: 2026-10-25
origin: "R: another round on the noisy cloud blobs in Blender (render after phase 17.1-17.4)"
---

# Exported clouds without pixel speckle

After the first close-up pass, R's Blender renders of a dense cumulus field
still read as noise: a carpet of small, equal, soft-shaded blobs. Reproduced
headless with the `planet_risa_06` preset on 8K bands over the ITCZ cumulus
east of the central continent and over the earlier trade-cumulus close-up.

## Cause

At 8K and above the fine band counts as resolved, so broken cumulus is cut
into individual clouds and holes, and cluster margins are frayed, by
thresholding it. The finest octave that still counts as resolved has lobes of
about two pixels, so the threshold drew holes, dots and a web of cell walls
one or two pixels wide inside every cluster. In the map that is speckle; Blender
samples it with cubic interpolation and every speck becomes a smooth round
blob of the same size. The preview draws its pixels as they are, so the same
cut reads as fine detail there.

## Change

`cloud_synthesis.wgsl`:

- `cs_fbm` also returns the fine band as it cuts clouds (`holes`,
  `holes_unresolved`): the same octaves, each counted as resolved only at
  `hole_scale` times the footprint. With a scale of 1 it equals `hi`.
- The fill holes, their margin ramp and domes, and the ragged and frayed edges
  of clusters, flowers and cells use that field.
- `cs_magnified`, a private flag the export entry points set
  (`cloud_export_columns.wgsl`, both the staged faces and the equirect
  inspection path): the cut uses a scale of 2
  (`CS_MAGNIFIED_HOLE_FOOTPRINT`), and nearly full decks stay whole from a
  fill of 0.7-0.9 instead of 0.8-0.95. Finer octaves still vary brightness
  (`texture`). The preview leaves the flag off and renders as before.

The cloud fraction is unchanged: the probability remap already accounts for
the unresolved variance of whatever field is thresholded.

## Rejected on the way

- No fill holes at all at 8K (fill as brightness): smooth but featureless
  cotton masses.
- A scale of 3: the same as no holes at 8K.
- Cumulus lobes shaded by a second bump in the bridge (`1 - F1^2` of the
  billow cells, then Smooth F1 domes): crease lines read as cracked stone,
  then as fish scales. The bridge is unchanged.

## Verification

- `exported_clouds_are_not_cut_at_one_pixel_and_keep_their_cover`: on an 8K
  band across the tropics, pixel-to-pixel roughness over variance 0.024 for the
  export's cut against 0.25 for the preview's cut, cloud fraction 0.263 both.
- Preset, ITCZ band (16384 x 1100): roughness 0.207 -> 0.019, cloud fraction
  (Y > 0.05) 0.532 both, mean optical depth -14 % (softer margins of the
  wider cut).
- Headless Cycles before/after (current main's bridge on both): 200 km and
  500 km over the ITCZ field, the 600 km trade-cumulus close-up and 2500 km,
  all from 8K maps, and the globe from a 4K map. The first pass compared its
  mid-range view on a 4K map, where the fine band never resolves; from 8K the
  same view was popcorn, now soft cumulus. The 4K globe is unchanged. Images in
  project files `blender-noisy-blobs/`.
