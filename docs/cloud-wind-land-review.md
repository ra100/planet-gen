# Strong wind, green land and cirrus

This pass addresses broken cloud banks at high wind, scarce clouds above green
continents, and missing thin cirrus.

See the [filament follow-up](cloud-filament-review.md) for strain-aware breakup
that preserves this pass's circulation.

## Strong wind

- The half-time transport predictor now limits its reconstructed slopes again
  after outflow. Previously, a face that was positive before prediction could
  become negative afterward and feed negative condensate into the neighboring
  column. The new limiting keeps these predicted cloud faces nonnegative.
- Above 1.25× wind, convergence-driven formation transitions toward an
  exponential response instead of clipping quickly to full lift. This retains
  density differences within a strongly stretched front.
- The smallest steering eddies reduce gradually with strong wind, reaching
  70% of their normal amplitude at 3.75×. Broad curls and the intermediate
  eddy scale remain. There is no additional blur over the rendered clouds.

The previous cubemap edge geometry correction remains. The wind speed cap,
384-texel weather grid and transport schedule are unchanged.

## Clouds above green land

Weather now receives the terrain seed and the same regional moisture factor
used by surface shading. Both use shared rainfall belts, regional variation
and the climate moisture control. Wet continental interiors are no longer
classified as nearly dry solely because they are far from the coast.

Vegetation, elevation, frost and rain shadow determine the canopy source. Wet
land begins with finite, sub-saturated boundary-layer humidity instead of
spending the entire short spin-up recharging dry air. Canopy humidity supports
local condensation, while evapotranspiration conductance still caps the actual
water source. Coverage and moisture at zero still produce no clouds.

The regional climate is shared with albedo; local rain-shadow and continentality
sampling remain approximations. This does not paint a cloud mask over green
pixels: humidity is transported and converted before rendering.

## Cirrus

- Ordinary transported upper ice can exist without a low or deep cloud bank
  underneath. Previously diagnosis clipped its amount to local lower cloud
  cover, erasing detached wisps.
- Frontal and terrain-assisted ascent can freeze available upper-air vapor.
  Transfer is bounded by available water and a local ice capacity, and moves
  its source ownership with the water. Existing deep-cloud detrainment still
  supplies anvils.
- Upper-air exchange is enabled in the normal transport path, using its
  existing vapor reservoir rather than drawing ordinary cirrus directly from
  the whole boundary-layer vapor column.
- Ice uses a modest directional shear relative to the lower wind. Its speed
  stays within the existing transport bound, allowing thin trails to extend
  beyond and cross lower cloud banks.

Cirrus keeps the existing low optical weight. No new opacity noise texture or
puff lattice is introduced.

## Captures

The capture utility now saves isolated low/cloud-ice views, surface-only views,
raw cloud columns and their settings. `--wind-scale` allows identical controls
to be reviewed at different wind strengths.

| Case | Density | Daylight | Isolated cirrus |
| --- | --- | --- | --- |
| Risa controls, 2.5× wind | [Close](../output/cloud-wind-land-review/risa-wind25/density-closeup.png) | [Close](../output/cloud-wind-land-review/risa-wind25/daylight-closeup.png) | [Daylight](../output/cloud-wind-land-review/risa-wind25/cirrus-daylight.png) |
| Risa controls, 4× wind | [Close](../output/cloud-wind-land-review/risa-wind4/density-closeup.png) | [Close](../output/cloud-wind-land-review/risa-wind4/daylight-closeup.png) | [Density](../output/cloud-wind-land-review/risa-wind4/cirrus-density.png) |
| Seed 73, 1× wind | [Close](../output/cloud-wind-land-review/seed73-wind1/density-closeup.png) | [Close](../output/cloud-wind-land-review/seed73-wind1/daylight-closeup.png) | [Daylight](../output/cloud-wind-land-review/seed73-wind1/cirrus-daylight.png) |

[Previous Risa density](../output/cloud-flow-review/risa-clouds/density-closeup.png)
provides the earlier 2.5× comparison. These are 512-pixel llvmpipe captures with
the utility's synthetic Earth terrain. Risa imports cloud controls from the
saved JSON; it is not a reconstruction of its complete terrain. Surface-only
captures now use the actual climate moisture factor as well, so surface colors
can differ from earlier review captures.

## Review results

All three raw cloud fields are finite and bounded between zero and one.
Interactive density and daylight views match their PNGs within one byte per
color channel. Detached upper ice is present in all three scenes, while the
isolated daylight views show thin trails outside lower cloud banks.

A color-based surface probe compares pixels with green-dominant surface color
against warm, dry-looking ground in the surface-only capture. The fraction of
those pixels whose density-view gray value exceeds 30/255 is:

| Case | Green surface probe | Dry surface probe |
| --- | --- | --- |
| Risa, 2.5× | 86.8% | 19.9% |
| Risa, 4× | 87.7% | 28.0% |
| Seed 73, 1× | 96.5% | 65.3% |

These are rendered image probes, not physical cloud-cover measurements or a
realism score. Their purpose is to check the requested association with green
ground. Visual inspection also found retained folds and curls at 4× wind.
Long bands, pointed fringes and smooth interiors remain visible; their removal
is not claimed.

## Limits and build

Formation coefficients, initial humidity and directional shear are calibrated
approximations. Fine cloud turrets remain below the weather grid's resolution;
broad interiors can still look smooth. This is a visual improvement to the
reduced model, not a claim that it reproduces orbital photographs.

Release builds of `planet-gen` and `realism_capture`, and `cargo check
--all-targets`, completed. Formal automated tests were not run. Restart
`target/release/planet-gen` and regenerate the clouds to load the new shaders.
`git diff --check` passed and the graft cache was refreshed.
