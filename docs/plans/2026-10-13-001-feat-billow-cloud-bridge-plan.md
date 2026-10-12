---
title: "feat: Billow edge-erosion cloud bridge"
type: feat
status: active
date: 2026-10-13
origin: "User request: implement the edge-erosion (billow) cloud detail validated on the Risa scene for the Planet Gen -> Blender path; drop or default-exclude unused cloud outputs"
---

# Billow edge-erosion cloud bridge

The v10 recipe in the first bridge laid procedural noise over the solver's clouds
and read as a pattern on top of the cloud. Tests on the Risa scene showed that
the solver map itself should stay the cloud, with detail only eroding thin areas
and edges.

## U1: Bridge

- `cloud_bridge.py` builds `PlanetGen Billow Clouds`: `cloud_density.exr`
  contrast-mapped to m, six inverted Worley F1 octaves + zero-mean FBM wisps on a
  zonally stretched, wind-warped domain, erosion
  `m * clamp((bn - lo)/(1 - lo)) / bn` with `bn = 1 - exp(-3m)`,
  `lo = Erosion * variety * (1 - billow)`; opacity `1 - exp(-k * density)`.
- v10 detail stack, coverage gain, fill, background cover and top bump removed.
- Required maps: `cloud_density.exr`, `wind_drivers.exr`; cloud/climate drivers
  are sampled only when present.

## U2: Export defaults

- `ExportLayers::cloud_reconstruction` (`clouds.exr`) and
  `ExportLayers::cloud_drivers` (`cloud_drivers.exr`, `climate_drivers.exr`),
  both off by default; UI sub-options under Clouds.
- `wind_drivers.exr`, `cloud_density.exr`, `cloud_height.exr` stay with clouds.
- Add-on import no longer loads `clouds.exr` / cloud textures it never used.

## Verification

- `cloud_maps_skip_reconstruction_unless_requested`; streaming export tests,
  with the streamed test opting in to the reconstruction and all drivers.
- Headless render on the Risa export: disk view matches the map's placement;
  oblique 1000 km view shows puffy edges and smooth cores.
