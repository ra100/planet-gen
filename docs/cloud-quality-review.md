# Cloud quality review

Historical, rejected iteration: its ordered puffs remained visible.
The current [formation revision](cloud-formation-review.md) removes the
presentation noise fields and records the latest captures and limitations.

## Cause and changes

The transported weather field already produces fronts, storm systems, and clear
areas. Its presentation added three scales of boundary displacement throughout
each cloud, several independent density noise fields, and another fine puff
field inside cumulus banks. These layers made coherent weather look granular.

- Boundary displacement now uses two smaller scales and fades out in dense
  cloud interiors. Empty weather remains empty; the weather field still determines
  cloud support and the seed, moisture, circulation, and terrain response.
- Cumulus uses connected banks and smaller lobes, with no third puff field.
  Thin stable decks stay continuous. Fine density modulation is weaker.
- Storm detail stays coherent through the column, including the existing bounded
  wind shear. The independent altitude-dependent noise field was removed.
- Cirrus keeps directional filaments with less fine noise and lower contrast.
- A normalized approximation of higher scattering orders brightens cloud bodies
  while retaining sun attenuation and the existing night ambient. It uses the
  same star color and atmospheric transmission as direct light.
- Empty layers skip their detail calculations. Removing the fine cumulus field
  also removes a 27-cell neighborhood evaluation when that field was resolved;
  the extra storm noise and third boundary octave are gone. Ray counts are unchanged.

Density, cloud shadows, and exported cloud maps share `cloud_density.wgsl`.
Color captures and the interactive preview share the cloud lighting implementation.

The hierarchy follows the large-form-first approach described by
[Guerrilla's volumetric cloud presentation](https://www.guerrilla-games.com/read/the-real-time-volumetric-cloudscapes-of-horizon-zero-dawn).
[NASA's open and closed cell examples](https://science.nasa.gov/earth/earth-observatory/open-and-closed-cell-clouds-over-the-pacific-ocean-43795/)
illustrate why stable decks and broken convective clouds need different structure.

## Local visual evidence

Images are generated artifacts in the ignored `output/` directory.

| View | Before | After |
| --- | --- | --- |
| Seed 42, daylight | [Before](../output/cloud-quality-review/before/daylight.png) | [After](../output/cloud-quality-review/after/daylight.png) |
| Seed 42, closeup | [Before](../output/cloud-quality-review/before/daylight-closeup.png) | [After](../output/cloud-quality-review/after/daylight-closeup.png) |
| Seed 42, density only | [Before](../output/cloud-quality-review/before/actual-density.png) | [After](../output/cloud-quality-review/after/actual-density.png) |

The captures include isolated stable decks, cumulus, storms, and cirrus, plus
daylight, closeups, backlighting, and night views. No-detail views isolate the
underlying weather from presentation detail. Seed 42 uses 512-pixel images;
[seed 73 daylight](../output/cloud-quality-review/seed73/daylight.png) uses 384 pixels.

## Build and rendering checks

- `cargo build --release --bins --features validation`
- `cargo check --all-targets --features validation`
- `git diff --check`
- GPU capture: `realism_capture OUTPUT_DIR SEED SIZE --clouds-only`.
- The capture tool compares interactive and offline color encoding for daylight
  and density views: both seeds differed by at most one byte value per channel.
- Seed 42's night-cloud image has mean interior RGB values of approximately
  `(1.00, 1.25, 1.91)` on the 0–255 scale. The added scattering retains a dark
  night side rather than producing daylight illumination there.

Captures ran through software Vulkan (`llvmpipe`), so these results do not establish
a hardware GPU performance improvement. Existing OpenEXR metadata and unused-code
warnings remain. Unit tests were not run.

## Remaining limits

This remains a procedural representation of clouds viewed from orbit. Sunlight
uses the existing analytical local column integration rather than tracing across
neighboring cloud columns; the higher-order scattering term is an approximation.
The changes improve coherent shapes and lighting without replacing the weather
simulation or claiming photographic cloud reconstruction.
