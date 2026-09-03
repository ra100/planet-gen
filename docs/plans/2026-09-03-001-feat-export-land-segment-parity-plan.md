---
title: "feat: Port land-segment low-cloud integration into cloud export"
type: feat
status: completed
date: 2026-09-03
origin: docs/brainstorms/2026-09-03-export-land-segment-parity-requirements.md
---

# feat: Export land-segment parity (FE-089)

## Summary

Make `cloud_export.wgsl` integrate low clouds over land with the same shared
`weather_cloud_layers_land_segment` function preview uses, closing the
documented preview/export divergence (882b7cb NOTE).

## Design

In `cloud_export.wgsl` main(), replace the point sample inside the 8-step
radial march:

```wgsl
let segment_start = direction * (1.0 + f32(sample_index) * step_km / radius_km);
let segment_end   = direction * (1.0 + (f32(sample_index) + 1.0) * step_km / radius_km);
let sample = weather_cloud_layers_land_segment(
    direction, altitude_km, segment_start, segment_end, radius_km, footprint,
);
```

Radial points satisfy `length(p) = 1 + h/radius_km`, so the shared function's
altitude recovery is exact and each slice is monotonic in length (the
closest-point branch degenerates cleanly). Ocean texels hit the land gate's
early return and are bit-exact unchanged.

## Verification

1. Full lib suite (`--features validation --lib`): U5 parity/determinism green;
   any moved pins re-baselined per protocol (double-run, identical hashes).
2. Sweep validation: expected 17 failures only (15 parked U15 + 2 environmental
   perf gates); no new failures.
3. Permanent guard: extend the U5 shared-include assertion to require
   `weather_cloud_layers_land_segment` in cloud_export.wgsl.

## Risks

- Pin churn if any test pins absolute export values over land — expected and
  re-baselined with documented ruling, not forced.
