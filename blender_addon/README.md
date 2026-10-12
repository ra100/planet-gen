# Planet Gen Blender add-on

Install `blender_addon/` as an add-on (zip the folder, or symlink it into
Blender's `scripts/addons/` as `planetgen_addon`). Blender 4.1 or newer. The
panel is under **View3D > Sidebar > Planet Gen**.

## Cloud bridge

Planet Gen decides *where* clouds are from its weather solver (land, sea,
temperature, pressure, wind, terrain lift). A 16K `clouds.exr` is the only way
to get that placement sharp at close range. The bridge avoids it: Planet Gen
exports small driver maps, and Blender generates the fine detail procedurally at
render time with the multi-octave noise recipe from the v10 planet shader.

Every export with the **clouds** layer writes three extra maps next to
`clouds.exr`. They come from the weather field, so their size depends only on
the weather resolution (1536 x 768 for any export with faces of 384 px or more), never on the
export size:

| File | R | G | B | A |
|------|---|---|---|---|
| `cloud_drivers.exr` | low mass | deep (convective) mass | high (cirrus) mass | coverage |
| `climate_drivers.exr` | land fraction | thermal (0 at -25 C, 1 at 30 C) | cloud top / 20 km | continentality |
| `wind_drivers.exr` | wind x | wind y | wind z | wind speed |

Wind is the tangent wind in Planet Gen's Y-up planet space. All three maps are
equirectangular, sampled at pixel centres, with longitude 0 in the centre
column.

### Use

1. **Import Planet** and pick the export folder.
2. Select the cloud shell (a sphere slightly larger than the planet) and press
   **Drive Clouds**. It builds the `PlanetGen Driven Clouds` material.
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
| W, Roughness, Distortion, Detail, Scale | The v10 detail noise, unchanged |
| Drive Strength | 0 is the original v10 look exactly; 1 is fully Planet Gen placed |
| Coverage Gain | Scales solver coverage. Set automatically so the 98th percentile of cloudy texels reads as full cover |
| Fill Boost | How much dense coverage fills gaps in the detail, and sparse coverage thins it |
| Background Cover | Share of the v10 procedural mask kept in clear regions |
| Wind Streak | Stretches detail along the solver's wind |
| Longitude Offset | Turns the drivers around the pole (in turns) if the surface is rotated |

Cloud top height raises the bump strength where columns are tall. The sampler
group (`PlanetGen Driver Sampler`) also exposes land, thermal, continentality
and the individual cloud families for custom node work.

### Frame

Blender is Z-up and Planet Gen is Y-up: planet `(x, y, z)` is Blender
`(x, z, -y)`. Longitude 0 faces Blender `-Y`. The mapping uses object
coordinates, so it works on any sphere mesh regardless of its UVs.
