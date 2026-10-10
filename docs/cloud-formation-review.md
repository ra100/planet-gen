# Cloud formation revision

The subsequent [ribbon and density refinement](cloud-ribbon-review.md) corrects
transport compression and softens the dense cores. Captures below record this
earlier formation pass; the newer review contains the current captures.

The two previous appearance revisions were rejected: regularly spaced puffs
were replaced by a continuous noise field, but that field was still visible as
an opacity texture. Those experiments are superseded.

## Current changes

- Removed the puff/body functions, cloud density noise, boundary noise and
  cirrus noise from the shared preview/export density shader. The renderer
  uses transported low, deep and high condensate directly. Changing a rendering
  seed cannot stamp a different texture onto a fixed weather field.
- Removed the small, independent wind component noises that repeatedly drove
  convergence. Latitude wobble no longer increases its frequency near the poles.
  Existing circulation, continentality, terrain deflection and stream steering
  still drive transport. Broad seeded pressure variation remains in the weather
  model; randomness is not removed from weather formation.
- Increased the transport resolution cap from 128 to 384, matching the normal
  weather output instead of interpolating a much coarser solution.
- This pass reduced the eddy coefficient from 8,000,000 to 10,000 m²/s.
  Because its stencil averages four neighbors, effective diffusivity on the
  nominal grid is one quarter of the coefficient. Over the 25,600-second
  spin-up, the characteristic mixing length changes from about 226 km to
  about 8 km. The later refinement uses a coefficient of 100,000 m²/s,
  giving about 25 km. The stability clamp and CFL substeps remain.
- Diagnose convergence at the resolved grid scale. Diagnose fronts over a fixed
  physical distance, independently of grid spacing; sampling a longer pressure
  baseline also avoids amplifying half-float rounding error.
- Removed the fixed marine condensation ceiling at 70% of local target humidity.
  Marine condensation now responds to convergence, frontal/orographic lift and
  a cool stable boundary layer. Vapor-to-condensate transfers remain bounded by
  the existing reservoirs and source budgets.
- Corrected storm-center jitter: the seed helper returns offsets in 0–100, while
  both formation shaders previously treated them as 0–1.
- Lighting is evaluated within the occupied portion of each ray segment instead
  of above a thin cloud when the midpoint misses it. Dense columns are lit near
  their visible boundary, using an approximate extinction-weighted mean depth,
  rather than at a dark midpoint deep inside the cloud. Increased bounded multiple
  scattering and calibrated extinction to 4.5. Sunlight, density diagnostics,
  export density and surface shadows use the shared extinction coefficient.
- Removed camera ray noise. Daylight cloud color is white-balanced relative to
  the Sun preset; star color controls and sun-gated night lighting remain.

The isolated pressure-cell experiment produced oversized smooth cutouts and was
discarded. It is not part of the final implementation.

## Visual review

The generated PNGs are local, ignored artifacts. Seed 42 uses the same terrain,
weather controls, camera and light as the previous review; the weather field
itself changes because formation and transport changed.

| View | Rejected appearance pass | Current formation pass |
| --- | --- | --- |
| Globe | [Previous](../output/cloud-orbital-review/after/daylight.png) | [Current](../output/cloud-formation-review/seed42/daylight.png) |
| Orbital crop | [Previous](../output/cloud-orbital-review/after/daylight-orbital.png) | [Current](../output/cloud-formation-review/seed42/daylight-orbital.png) |
| Condensate | [Previous](../output/cloud-orbital-review/after/actual-density.png) | [Current](../output/cloud-formation-review/seed42/actual-density.png) |

The capture utility can also read cloud controls from a planet JSON:

```sh
rtk proxy env RUST_LOG=warn target/release/realism_capture \
  output/cloud-formation-review/risa-clouds 751545802 512 \
  --clouds-only --cloud-settings /home/ra100/Downloads/planet_risa.json
```

This uses Risa's cloud seed, coverage 0.26, moisture 0.9, wind scale 2.5 and storm
controls with the utility's synthetic Earth terrain and camera. It does not
reproduce all terrain, planet physics and camera settings in that file.

- [Risa cloud controls, globe](../output/cloud-formation-review/risa-clouds/daylight.png)
- [Risa cloud controls, orbital crop](../output/cloud-formation-review/risa-clouds/daylight-orbital.png)

## Checks and limits

Release builds for `planet-gen` and `realism_capture`, plus `cargo check
--all-targets`, completed. Captures run the actual GPU shaders on llvmpipe;
interactive preview and offline PNG output differ by at most one byte per color
channel. Automated tests were not run. Existing fixtures that required the
removed opacity texture were updated; obsolete body-noise oracles were removed.

This removes the repeated rendering texture. Close orbital views still expose
the weather grid's limited spatial detail: individual kilometre-scale cloud
turrets are not resolved. The images should not be described as a completed NASA
photo match. A separate pressure-cell trial was rejected during this review.

The finer transport grid costs more generation work and GPU memory. Its six
RGBA16Float state textures grow from about 4.5 MiB to 40.5 MiB; the optional
R16Float provenance pair budget grows from 384 KiB to 3456 KiB. Removing density
noise makes rendering simpler, but no overall speedup is claimed.

Restart `target/release/planet-gen` to load the rebuilt shaders and regenerate
weather. An already-running process retains its previous shaders.
