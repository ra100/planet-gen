# Water erosion update

## Changes

- Annual runoff uses the planet seed, spherical latitude and axial tilt, elevation, temperature, atmospheric moisture, and ocean supply. Regional variation uses the same seed offsets as the surface climate. Frozen and dry terrain receive less liquid runoff.
- Multiple-flow drainage carries water from wet catchments into dry lowlands. Ocean cells absorb it instead of supplying freshwater.
- River incision requires concentrated drainage, is bounded by local relief, and fades toward sea level. Wet hillslope relaxation is weaker than the previous pass. The repeated lowland roughening noise is removed.
- Gentle, shallow shores receive bounded sediment smoothing. The slope thresholds match beach material selection; steep headlands are protected. Concentrated river mouths receive less smoothing. The coastal pass preserves the land/water classification.
- Preview and export receive the same climate inputs. Changing atmospheric moisture now regenerates terrain.

This is a terrain authoring approximation. Wave smoothing represents local sediment redistribution, rather than a storm simulation; deposition does not track a conserved sediment budget. Drainage still operates within individual cube faces, with synchronized halos between row tiles.

## Computation

The previous flow shader recomputed a neighboring cell's eight downhill slopes for every incoming flow contribution, on every flow subpass. The new preparation pass caches the inverse slope sum and annual runoff once per terrain iteration. Flow subpasses read that cache and eight neighboring heights. Rainfall noise and climate calculations no longer run inside every flow subpass.

The preparation pass resets runoff each iteration, so progressive preview batches do not inherit stale water from earlier terrain. Height and water buffer parity and tiled halo exchange are retained. Redundant CPU initialization and uploads for the second height buffer and both water buffers are removed.

Zero atmospheric moisture skips flow accumulation while keeping coastal smoothing. Zero ocean supply skips hydraulic erosion. The routing cache costs eight bytes per pixel; binding limits, tile sizing, and export memory estimates account for it.

## Build and captures

Completed:

- `cargo build --release --bins`
- `cargo check --all-targets --features validation`
- `git diff --check`
- Headless wet, dry, and zero-moisture comparison renders. Existing unit tests were updated for the new climate argument and buffer footprint, but were not run.

The environment selected Vulkan llvmpipe software rendering. A matched capture workload with 512px cube faces, 1024px renders, and 0/25/50 erosion iterations took 63.99 seconds before the routing optimization and 36.91 seconds after it. This includes initialization and six image captures; it is not a hardware GPU erosion benchmark.

The final wet shader took 4.95 seconds for 50 iterations. Mean absolute height changes were 0 for deep ocean, 0.0004344 for the shore band, and 0.0028563 inland. Shore-band roughness, measured by the mean absolute cardinal Laplacian on the original shore mask, fell to 97.8% of its initial value.

Final captures, all with 50 erosion iterations:

| Atmospheric moisture | Deep ocean mean change | Shore mean change | Inland mean change | Shore roughness ratio |
| --- | ---: | ---: | ---: | ---: |
| 1.0 | 0 | 0.0004344 | 0.0028563 | 0.978 |
| 0.2 | 0 | 0.0001946 | 0.0000696 | 0.985 |
| 0.0 | 0 | 0.0001861 | 0 | 0.987 |

At moisture 0.2, inland changes were about 41 times smaller than in the wet case. Zero moisture still smooths shores, while inland terrain stays unchanged. The zero-moisture pass took 0.68 seconds with flow accumulation skipped. These timings describe software rendering only.

![Wet planet after 50 iterations](../output/water-erosion-review/wet.png)

![Dry planet after 50 iterations](../output/water-erosion-review/dry.png)

![Zero atmospheric moisture after 50 iterations](../output/water-erosion-review/zero.png)

Reproduce captures:

```sh
rtk cargo run --release --bin erosion_compare -- /tmp/erosion-wet 1
rtk cargo run --release --bin erosion_compare -- /tmp/erosion-dry 0.2
rtk cargo run --release --bin erosion_compare -- /tmp/erosion-zero 0
```

## References

- [USGS: waves, currents, and storm surges](https://pubs.usgs.gov/of/2003/of03-337/waves.html) describes redistribution of beach and nearshore sediments by waves and currents.
- [FastFlow, author version](https://www-sop.inria.fr/reves/Basilic/2024/JKGFC24/FastFlowPG2024_Author_Version.pdf) describes terrain authoring with flow routing, stream power, and deposition. This implementation retains its existing multiple-flow iteration approach rather than implementing that paper's solver.
