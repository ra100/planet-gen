# Planet Gen Blender add-on

Install `blender_addon/` as an add-on (zip the folder, or symlink it into
Blender's `scripts/addons/` as `planetgen_addon`). Blender 4.1 or newer. The
panel is under **View3D > Sidebar > Planet Gen**.

## Cloud bridge

Planet Gen decides *where* clouds are from its weather solver (land, sea,
temperature, pressure, wind, terrain lift). `cloud_density.exr` carries that
placement, but at any export size it is too soft at close range. The bridge keeps
the solver's clouds and adds detail at render time by *edge erosion*: inverted
Worley billows (Schneider / Horizon Zero Dawn style) eat into thin areas and
edges, while thick cores stay smooth. No pattern is laid over the cloud, so the
detail always follows the solver's shapes.

Every export with the **clouds** layer writes:

| File | Content | Used by the bridge |
|------|---------|--------------------|
| `cloud_density.exr` | Y optical depth, export size (includes the app's Clouds > Advanced > Thin lift) | yes, the cloud |
| `cloud_height.exr` | Y cloud top / 20 km, `height_km` | no (volumetric/PCO setups) |
| `wind_drivers.exr` | RGB tangent wind (Y-up planet space), A speed, weather resolution | yes, streaks the detail |

Opt-in under the clouds layer (off by default):

| File | Content |
|------|---------|
| `clouds.exr` | Six-channel reconstruction (optical depth + geometry), large |
| `cloud_drivers.exr` | R low, G deep (convective), B high (cirrus) mass, A coverage |
| `climate_drivers.exr` | R land, G thermal (0 at -25 C, 1 at 30 C), B cloud top / 20 km, A continentality |

Driver maps are equirectangular at weather resolution (1536 x 768 for faces of
384 px or more), sampled at pixel centres, with longitude 0 in the centre column.

### Use

1. **Import Planet** and pick the export folder.
2. Select the cloud shell (a sphere slightly larger than the planet) and press
   **Drive Clouds**. It builds the `PlanetGen Billow Clouds` material.
3. Optionally select the planet and press **Aligned Surface** to put the
   exported albedo on it in the same frame, so clouds sit over the right land.

Headless, against any `.blend`:

```
blender scene.blend --background --python blender_addon/cloud_bridge.py -- \
    --drivers /path/to/export/planet --object Clouds --surface Planet --save out.blend
```

Re-running either path with a new export re-points the existing images and
rebuilds the node groups, so every material using them picks up the change.

### Controls on the cloud group

| Input | Effect |
|-------|--------|
| Density Low / High | Contrast on the solver's optical depth: below Low is clear, above High a full column that billows no longer erode |
| Scale | Base billow cell frequency (250 ~ 50 km cells on an Earth-size planet); six octaves down to 1/32 |
| Erosion | How far billows eat into thin areas and edges; cores are kept. Default 0.15: exports with Clouds > Structure already carry margins, cells and gaps; use ~0.35 for solver-only exports (Structure 0) |
| Variety Scale | Planet-scale noise that varies Erosion 0.5-1.5x, so some systems are puffier |
| Wisps | Fine FBM added to the billows |
| Zonal Stretch | Compresses the detail along the pole axis; elongates billows east-west |
| Wind Streak | Displaces the detail along the solver wind |
| Depth Gain | Physical optical depth per unit of solver depth above Low (36: a 0.35 column is tau ~11), as in the app preview |
| Contrast | Depth scales by thickness^Contrast, so haze between the solver's ripples stays faint and their cores turn bright |
| Diffusion | Two-stream reflectance `opacity = a tau / (1 + a tau)`: brightness keeps rising with thickness instead of clipping to flat white, so structure inside decks stays visible |
| Relief | Bump from the visible cloud: sunlit and shaded sides on the solver's ripples and cells |
| Longitude Offset | Turns the maps around the pole (in turns) if the surface is rotated |

The group also outputs the eroded `Density`. The sampler group
(`PlanetGen Driver Sampler`) exposes density, wind and, when the opt-in maps
were exported, the cloud families and climate fields for custom node work.

### Frame

Blender is Z-up and Planet Gen is Y-up: planet `(x, y, z)` is Blender
`(x, z, -y)`. Longitude 0 faces Blender `-Y`. The mapping uses object
coordinates, so it works on any sphere mesh regardless of its UVs.
