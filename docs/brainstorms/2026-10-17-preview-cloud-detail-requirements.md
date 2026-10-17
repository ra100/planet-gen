# Preview cloud detail requirements

Date: 2026-10-17
Origin: "Improve clouds quality, realism, natural look, still "physics" driven, but "artistically pleasing""

## Problem

The weather solver resolves clouds at one cubemap texel (about 26 km on Earth
at 384 per face). Its normalized condensate gives column optical depths of
roughly `2.0 * low`, `7.4 * deep` and `0.4 * high`, so the preview draws most
cloud as a thin, gray, translucent veil with soft bilinear edges. Real liquid
clouds are optically thick (tau 10 to 100) and seldom fill a 26 km area: thin
columns are broken cumulus over clear sky, thick ones are closed, bright decks
with sharp, billowed margins.

## Requirements

- R1. Cloud placement, amount and type stay the weather solver's. Detail may
  only scale existing condensate: empty weather stays empty and no density
  appears outside the solver's mass.
- R2. Thin liquid columns read as broken cloud with clear sky between puffs;
  thick columns read as closed decks with bright cores and no punched holes.
- R3. Margins look billowed like the accepted Blender bridge (13.3), not like
  an overlaid noise pattern: no puffs, marbling or stipple on uniform decks.
- R4. The solver's ripples, curls and cirrus swirls stay visible.
- R5. Lighting stays physically motivated: optical depth from a two-stream
  approximation, skylight on cloud tops, surface shadows from the same column.
- R6. No visible texel lattice and no shimmer when zoomed out.
- R7. One "Detail" control: 0 renders exactly the previous preview, 1 is the
  default. Saved with the planet file.
- R8. Export (`clouds.exr`, `cloud_density.exr`, drivers) and the Blender
  bridge stay unchanged.
- R9. Preview stays interactive: the added cost is a bounded per-pixel term,
  not per ray-march sample.
