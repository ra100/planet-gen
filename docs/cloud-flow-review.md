# Cloud flow and ripples

Follow-up: [Strong wind, green land and cirrus](cloud-wind-land-review.md)
adds transport limiting, vegetation-linked humidity and independent upper ice.

This pass follows feedback that the previous smoothing pass lost the preferred
ripples and cyclone-like curls, retained angular strands, and made the clouds
look like broad brush strokes.

## Changes

- Restored the earlier, lighter mixing and sharper formation eligibility,
  marine parcel capacity and rain response. The corrected full-time
  MUSCL–Hancock update remains: its flux predictor supplies the half-time face
  states, while the full update starts from the original cell average.
- Corrected transport geometry. A cubemap's neighboring-cell directions are
  oblique to the shared edges. The old flux used these directions as normals
  and neighboring-cell distances as edge lengths. Both the half-time predictor
  and full update now use the spherical mapping's edge normal multiplied by
  its angular length (`J * grad(s)` or `J * grad(t)`). Source ownership uses the
  same corrected flux. This reduces artificial compression that depended on
  the orientation of the cube face.
- Kept the circulation axis fixed near the poles. Blending it into a second
  axis moved a frame singularity into the polar weather belt. The zonal flow
  now fades at the actual pole.
- Added smaller curls to the seeded streamfunction and reduced the derivative
  sampling interval to resolve them. These eddies deform transported water
  before rendering. Their speed variation survives the final wind bound, and
  they remain active where the zonal circulation is weak. The finest octave
  fades out on low-resolution wind grids.
- Added local evaporation where a dense, narrow liquid-cloud ridge borders
  drier air. Water and its ownership move back into the vapor reservoir. This
  targets exposed tips rather than applying additional smoothing to the whole
  field. Existing storm catalysts protect their cores.

The renderer continues to use the weather columns directly. The experimental
fine opacity texture added grain without improving cloud structure and was
removed. A higher-resolution weather probe sharpened the same broad shapes;
production weather remains capped at 384 texels per face.

## Captures

These local PNGs use the same synthetic Earth terrain, cloud controls and camera
as the previous captures. Risa reads only cloud settings from
`~/Downloads/planet_risa.json`; it does not reconstruct the entire saved planet.
All outputs are 512 pixels square.

| Controls | Previous smoothing pass | Current density | Current daylight |
| --- | --- | --- | --- |
| Risa | [Density](../output/cloud-ribbon-review/risa-clouds/density-closeup.png) | [Density](../output/cloud-flow-review/risa-clouds/density-closeup.png) | [Orbital](../output/cloud-flow-review/risa-clouds/daylight-orbital.png) |
| Seed 42 | [Density](../output/cloud-ribbon-review/seed42/density-closeup.png) | [Density](../output/cloud-flow-review/seed42/density-closeup.png) | [Orbital](../output/cloud-flow-review/seed42/daylight-orbital.png) |
| Seed 73 | [Density](../output/cloud-ribbon-review/seed73/density-closeup.png) | [Density](../output/cloud-flow-review/seed73/density-closeup.png) | [Orbital](../output/cloud-flow-review/seed73/daylight-orbital.png) |

[Risa's earlier preferred flow](../output/cloud-formation-review/risa-clouds/density-closeup.png)
is included for comparison. The earlier numerical transport defect is not
restored to reproduce its edge teeth.

Visual inspection of all three density closeups found clearer folds, curled
ends and irregular fringes than in the smoothing pass. Risa's lower bank has
broader, smoother turns with internal curls; its upper bank retains layered
ripples. Daylight orbital views show more broken-up margins, but the broad
interiors remain smooth. Some long bands and pointed fringes remain. The
captures do not justify calling those issues completely resolved.

## Geometry check

A numerical calculation compared both flux geometries against an analytically
divergence-free rotation on a sphere. At 500 off-center cells on the +Z face,
the median absolute angular divergence error fell from 0.6976877 to 0.0000044;
the 95th percentile fell from 1.2613559 to 0.0000128. This checks the edge
geometry, not the complete weather model or global conservation across seams.

## Cost and limits

The wind preparation pass evaluates more streamfunction octaves. Edge
evaporation adds two neighboring state reads per source update. Transport no
longer evaluates `acos` for its shared-edge lengths. Texture resolution, the
25,600-second spin-up horizon, dispatch schedule and CFL limits are unchanged.
No performance speedup is claimed without a benchmark.

Fine, individual cloud turrets remain below the weather grid's resolution.
Broad liquid banks can still look smoother than orbital photography. These
changes improve resolved flow and density variation; they do not constitute a
photographic realism claim or a complete atmospheric simulation. Formation
coefficients and edge entrainment are calibrated approximations.

## Build and review

Release builds of `planet-gen` and `realism_capture`, and `cargo check
--all-targets`, completed. GPU captures use llvmpipe. Formal automated tests
were not run. All three controls' density and daylight PNGs match the
interactive preview within one byte per color channel. Their density images
are identical with rendering detail enabled and disabled: no additional
opacity field is being stamped into the rendered clouds. `git diff --check`
passed and the graft cache was refreshed. Restart `target/release/planet-gen`
and regenerate the clouds to load the rebuilt embedded shaders.
