# Cloud ribbons and density transitions

**Superseded by [Cloud flow and ripples](cloud-flow-review.md).** The visual
review found that this pass weakened the preferred ripples and cyclone-like
curls while leaving smooth, brush-like bodies. Its wider formation transition,
stronger mixing and rain coefficients were reverted. Reduced saturation alone
was not a sufficient visual acceptance criterion.

This pass follows the density review: retain curled flow, ripples and varying
opacity, while reducing the bright, broken ribbons and pointed opaque ends.

## Changes

- Corrected the MUSCL–Hancock update. The half-step predictor supplies face
  fluxes; the full step now starts from the original cell average. Previously
  it started from the predicted average, applying an additional half step of
  convergence and concentrating condensate into sharp, unstable-looking bands.
- Kept local mixing light. The four-neighbor averaging stencil now uses a
  coefficient of 100,000 m²/s, equivalent to about 25,000 m²/s on the nominal
  grid. The previous pass used 10,000, effectively 2,500 m²/s. No extra texture,
  dispatch or noise field was added.
- Widened the formation eligibility transition from 0.02 to 0.06. Marine
  parcel capacity now ranges from 1.02 to 0.65 times the local humidity target,
  instead of 1.08 to 0.48. Strong lifting converts less vapor immediately;
  weaker margins retain a more gradual response.
- Increased the rain sink smoothly when condensate exceeds half the existing
  climate-dependent humidity capacity. The excess-water rate ranges from 0.22 to 0.62;
  dilute margins below capacity retain their previous dissipation. The sink is
  bounded by available condensate and uses the existing ownership accounting.
- Added an orbital density capture alongside the orbital daylight view.

These are formation and transport changes. The shared renderer continues to
use the weather's condensate directly, with no opacity noise or puff lattice.
Existing wind steering, cloud seeds, moisture, land, terrain and storm controls
continue to organize the field.

The marine and rainfall coefficients are calibrated for this reduced weather
model; they are not measured cloud microphysics.

## Captures

Each comparison uses the same seed, terrain, controls, camera and 512-pixel
output. Risa uses cloud controls read from `~/Downloads/planet_risa.json` with
the capture utility's synthetic Earth terrain; this is not a reconstruction of
the complete saved planet.

| Controls | Previous density | Updated density | Updated daylight |
| --- | --- | --- | --- |
| Risa | [Before](../output/cloud-formation-review/risa-clouds/density-closeup.png) | [After](../output/cloud-ribbon-review/risa-clouds/density-closeup.png) | [Orbital](../output/cloud-ribbon-review/risa-clouds/daylight-orbital.png) |
| Seed 42 | [Before](../output/cloud-formation-review/seed42/density-closeup.png) | [After](../output/cloud-ribbon-review/seed42/density-closeup.png) | [Orbital](../output/cloud-ribbon-review/seed42/daylight-orbital.png) |
| Seed 73 | [Before](../output/cloud-formation-review/seed73/density-closeup.png) | [After](../output/cloud-ribbon-review/seed73/density-closeup.png) | [Orbital](../output/cloud-ribbon-review/seed73/daylight-orbital.png) |

[Risa orbital density](../output/cloud-ribbon-review/risa-clouds/density-orbital.png)
shows the remaining bands without lighting or surface color affecting the view.
The captures are local ignored artifacts.

The corrected transport reduced repeated edge teeth and disconnected bright
fragments. Wider gray margins and broad curls remain. This is a targeted
improvement to the density field; individual cloud turrets remain below the
weather grid's spatial resolution.

For the density closeups, near-white means an encoded gray value above 220/255;
fragments are four-connected near-white regions containing at least five
pixels. Intermediate gray covers values strictly between 45 and 180.

| Controls | Near-white pixels, before → after | Fragments, before → after | Intermediate gray, before → after |
| --- | --- | --- | --- |
| Risa | 1.32% → 0.20% | 37 → 10 | 29.6% → 33.2% |
| Seed 42 | 1.64% → 0.38% | 47 → 13 | 40.6% → 43.6% |
| Seed 73 | 1.15% → 0.16% | 34 → 5 | 40.7% → 41.6% |

These image measurements describe saturation and tonal variation, not physical
cloud coverage or proof of photographic realism. Some narrow orographic bands
remain visible.

## Build and review

Release builds of `planet-gen` and `realism_capture`, and `cargo check
--all-targets`, completed. GPU captures use llvmpipe. For all three controls,
density and daylight PNGs agree with the interactive preview within one byte
per color channel. `git diff --check` passed. Automated tests were not run.
Restart `target/release/planet-gen` and regenerate the clouds to use the updated
embedded shaders.
