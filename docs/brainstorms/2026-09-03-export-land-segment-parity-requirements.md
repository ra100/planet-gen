# Export land-segment parity requirements

## Context

d8c7de5 ("Fix low-cloud sampling over land") added segment-mean low-cloud
integration to the preview-only land-segment path
(`weather_cloud_layers_land_segment` in `src/shaders/cloud_density.wgsl`).
882b7cb documented that `cloud_export.wgsl` integrates `weather_cloud_sample`
without it: "preview and export diverge over land. Left as-is for
leader/designer decision."

RV-003 follow-up A/B (user-approved, throwaway worktree at e6f38e7 with the
land-segment blend disabled) measured the divergence on real validation scenes:
cloud density maps differ by PSNR 27-31 dB / SSIM 0.89-0.94 across the 8
global seeds (target/wt-ab-run vs target/val-rv003-run8). The divergence is
clearly visible, not sub-perceptual.

Decision (user, 2026-09-03): port the segment integration into export so exported
low clouds over land match the preview fix. Export lags; preview is the fixed
behavior.

## Requirements

- R1: Export low-layer integration over land must use the same shared
  `weather_cloud_layers_land_segment` function as preview (single source of
  truth in cloud_density.wgsl).
- R2: Ocean texels must be bit-exact unchanged — the land gate
  (`land_factor <= 0.0` early return) already guarantees this by construction.
- R3: Export has no camera ray; its march is radial per texel direction. The
  segment for sample i is the radial slice between altitudes i*step_km and
  (i+1)*step_km along the texel direction. This semantics must be documented
  in the shader comment.
- R4: U5 parity/determinism suite (direct vs tiled, repeat runs, seam safety)
  stays green.
- R5: No new validation gate failures; expected total remains 17
  (15 parked U15 + 2 environmental perf gates).
- R6: Any affected bit-exact pins are re-baselined per protocol (clean build,
  two runs, identical hashes) with the ruling documented.

## Out of scope

- Changing preview behavior (preview keeps d8c7de5's fix as-is).
- Camera-chord vs radial segment unification for off-axis views — export is
  world-space per-texel data; radial is its natural definition.
