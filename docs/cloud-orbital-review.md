# Orbital cloud morphology

Historical, rejected iteration: the warped body field remained visibly noisy.
It is superseded by the [formation revision](cloud-formation-review.md).

The previous cloud refinement still stamped similarly sized, wind-aligned ovals
across broad weather banks. The kernel placed a puff in each lattice cell, and
its largest scale corresponded to hundreds of kilometres on an Earth-sized planet.
Reducing the fine noise did not fix that structure.

## Changes

- Removed the puff lattice and its overlapping spherical kernels entirely.
  Seeded, gently warped continuous bodies now refine the transported weather.
  Fine structure refines the edges of those bodies instead of adding another
  independent blanket of bright dots.
- Appearance scales use the planet's radius: approximately 140 km for low-cloud
  aggregation, 35 km for fine cells, and 110 km for storm structure. These are
  procedural characteristic scales, not measured diameters of individual clouds.
  Boundary displacements are also expressed in kilometres.
- Strong breakup is reserved for deeper shallow convection. Stable low decks
  remain continuous, dense cores retain their weather shape, and marine cells
  have more influence over ocean than over land. The existing weather simulation
  still supplies the climate, moisture, circulation, mass, and layer heights.
- Wind deforms the structure locally rather than stretching every puff into the
  same oval. Unoccupied weather remains unoccupied.
- Detail filtering includes the bandwidth of the warped body and widens its
  transitions with pixel footprint. Unresolved structure fades to its mean;
  close orbital views reveal smaller features.
- Increased optical-depth calibration from 3 to 5 and brightened liquid-cloud
  scattering. Camera extinction, sun attenuation, export opacity, and surface
  shadow calibration use the shared extinction constant. Thin cirrus retains its
  separate optical weighting. Night illumination remains sun gated.

NASA reports typical marine cells on the order of 15–45 km, with smaller examples
around 10–15 km: [MISR reference](https://science.nasa.gov/earth/earth-observatory/closed-small-cell-clouds-in-the-south-pacific-2387/).
Its [open and closed cloud examples](https://science.nasa.gov/earth/earth-observatory/ex-cell-ent-clouds-off-chiles-coast-150926/)
also show why broken convection and stable sheets should not share one puff pattern.

## Matched local captures

Generated images are stored in the ignored `output/cloud-orbital-review/` directory.
The before images are the previous cloud implementation, with the same weather,
seed, terrain, camera, light, and image size as the revised seed 42 captures.

| View | Previous puffs | Revised morphology |
| --- | --- | --- |
| Globe | [Before](../output/cloud-orbital-review/before/daylight.png) | [After](../output/cloud-orbital-review/after/daylight.png) |
| Closeup | [Before](../output/cloud-orbital-review/before/daylight-closeup.png) | [After](../output/cloud-orbital-review/after/daylight-closeup.png) |
| Orbital view, zoom 3.5 | [Before](../output/cloud-orbital-review/before/daylight-orbital.png) | [After](../output/cloud-orbital-review/after/daylight-orbital.png) |
| Density only | [Before](../output/cloud-orbital-review/before/actual-density.png) | [After](../output/cloud-orbital-review/after/actual-density.png) |

[Seed 73 orbital view](../output/cloud-orbital-review/seed73/daylight-orbital.png)
provides another weather layout. No-detail captures, isolated cloud families,
backlighting, and night views are included in each revised capture directory.

## Checks and limits

- Release binaries build with `cargo build --release --bins --features validation`.
- All targets compile with `cargo check --all-targets --features validation`.
- Captures exercise WGSL pipeline creation and the interactive/offline rendering paths.
- Seed 42 and seed 73 daylight and density captures differ from interactive output
  by at most one byte value per channel. Night-cloud interiors have a 95th
  percentile channel value of 5 on the 0–255 scale for both seeds.
- `git diff --check` passes.
- Existing morphology oracle fixtures were updated to reference the replacement
  body function, bounded wind deformation, and the new optical calibration.
  Unit tests were not added or run.

Captures use software Vulkan (`llvmpipe`), so no hardware GPU speedup is claimed.
The uniform layout and ray sample count are unchanged. Sunlight still uses an
analytical local column, and multiple scattering is approximated; this is an
improvement to procedural orbital rendering, not a photographic reconstruction
or a cloud microphysics simulation.
