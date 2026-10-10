# Cloud filament review

Follow-up to [the wind, land and cirrus pass](cloud-wind-land-review.md).
This pass keeps its wind field and changes cloud-water dissipation in
`src/shaders/weather_spinup.wgsl`.

## What changed

- Evaporation and condensation now use the same local parcel capacity. In
  descending air, low cloud returns to vapor faster. Cloud supported by ascent
  is spared that extra evaporation.
- Edge entrainment responds to symmetric wind strain. Rigid rotation alone
  does not increase that strain response, helping preserve the rotating cores.
- Neighboring air is sampled across the compressive axis when strain is
  resolved. Previously the samples were always perpendicular to the wind,
  missing thin banks that crossed the flow.
- The cloud-water gate now includes the smaller reservoirs that produce
  visible filaments. The previous 0.08–0.28 gate mostly reached unusually
  water-heavy ridges.
- Evaporated liquid and its detailed source ownership return to the vapor
  reservoir. These are local transfers; this review does not establish global
  conservation of the cubemap transport stencil.

The strain calculation reuses the four wind samples already taken for lift.
The edge stencil still takes two neighboring state samples. Texture counts,
weather resolution, dispatch schedule, render sampling and saved settings are
unchanged.

Two circulation trials were discarded: increasing all eddies exaggerated
zigzags, and adding strong broad curl produced rounded, oily patches. Neither
trial is in the final shader.

## Visual comparison

512 px captures use the previous review's camera, 384 px weather faces and
synthetic Earth-like terrain. Risa imports cloud controls from
`~/Downloads/planet_risa.json`; these are not captures of its complete saved
terrain.

| Case | Before | After | Daylight after |
| --- | --- | --- | --- |
| Risa, wind 2.5 | [Density](../output/cloud-wind-land-review/risa-wind25/density-closeup.png) | [Density](../output/cloud-filament-review/risa-wind25/density-closeup.png) | [Daylight](../output/cloud-filament-review/risa-wind25/daylight-closeup.png) |
| Risa, wind 4 | [Density](../output/cloud-wind-land-review/risa-wind4/density-closeup.png) | [Density](../output/cloud-filament-review/risa-wind4/density-closeup.png) | [Daylight](../output/cloud-filament-review/risa-wind4/daylight-closeup.png) |
| Seed 73, wind 1 | [Daylight](../output/cloud-wind-land-review/seed73-wind1/daylight.png) | [Daylight](../output/cloud-filament-review/seed73-wind1/daylight.png) | [Close-up](../output/cloud-filament-review/seed73-wind1/daylight-closeup.png) |

Risa retains the layered upper ripples and lower rotating system. Several
connectors through clear air become thinner, separated wisps. Seed 73 changes
more visibly: the ocean ribbons break into smaller, irregular cloud groups,
while substantial continental cloud remains. This is a visual judgment, not a
photographic realism score.

[Isolated Risa cirrus](../output/cloud-filament-review/risa-wind25/cirrus-density.png)
still shows the thin upper trails independently of the liquid cloud layer.

## Capture checks

| Case | Mean low cloud, before → after | Detached ice area, before → after | Cloud over green / dry screen pixels, after |
| --- | --- | --- | --- |
| Risa, wind 2.5 | 0.027746 → 0.016942 | 6.52% → 12.42% | 84.4% / 19.5% |
| Risa, wind 4 | 0.025808 → 0.016759 | 6.39% → 10.31% | 86.4% / 27.3% |
| Seed 73, wind 1 | 0.062650 → 0.035085 | 4.44% → 15.04% | 94.2% / 62.7% |

Means and detached ice fractions use cubemap solid-angle weights. Detached ice
means high mass > 0.005 with low + deep mass < 0.005. More detached ice mainly
reflects the liquid layer dissolving below it. The screen probe uses cloud gray
> 30/255 and surface colors to distinguish green and dry pixels; it is a
limited association check, not physical cloud coverage. The lower cloud mass
is an expected consequence of the new transfers, not the acceptance criterion.

All three final fields are finite and in [0, 1]. Density and daylight
preview/export captures differ by at most one channel byte. Disabling the old
opacity-detail controls makes no change to density in any case.

`cargo check --all-targets`, release builds of `planet-gen` and
`realism_capture`, and `git diff --check` completed successfully. The capture
utility exercised the final WGSL on llvmpipe. The local Graft index was refreshed.

## Remaining limits

Broad circulation bands and some long connected cloud cores remain. This is
still a steady, simplified column-weather model. The pass improves local
breakup; it does not supply time-evolving fronts or resolve small cumulus
geometry at orbital camera distances. The utility's synthetic terrain and
software GPU also limit what these captures establish about the full app on
the user's hardware.

Restart `target/release/planet-gen` and regenerate the weather to use this pass.
