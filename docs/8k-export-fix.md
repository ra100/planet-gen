# 8K export with erosion — verification, 7 October 2026

The rebuilt app is `target/release/planet-gen`. A complete RTX 4090 export with
25 erosion iterations and every layer enabled passed in 233.17 seconds.
All eight outputs have 16,384 × 8,192 pixels (the application's 8K setting).
The output is in `target/8k-erosion-validation/seed42-1480948/`.

## Findings and changes

- Emission is enabled by default but previously selected a legacy full-image
  export path. At 8K, its memory estimate exceeds the 4 GiB preflight budget,
  so the default export was rejected before rendering. Emission now uses the
  same staged, bounded EXR writer as height and normals. The obsolete path was
  removed; small reference writers remain available to parity tests.
- Failed exports lost their visible status as soon as the worker stopped.
  Errors now appear in the existing dismissible banner. Worker panic messages
  retain their actual cause instead of always being described as GPU OOM.
- Erosion previously encoded every iteration for a face into one submission,
  creating a fresh binding group per pass. Bindings are now reused, and flow
  passes are submitted in batches of 32 with completion checks. Cancellation
  is checked between batches. Iteration count, flow passes, and shaders are
  unchanged; a GPU test verifies exact equality across batch boundaries,
  including odd height/water parity.
- Applying the 2K erosion delta to 8K terrain previously allocated another
  full six-face terrain. Reconstruction now modifies the existing terrain,
  removing a 1.5 GiB allocation. Erosion-disabled 8K exports skip the meso
  generation and reconstruction work. The existing 2K erosion resolution
  and full 8K terrain detail are preserved.
- Staged texture reads used `set_len` on an uninitialized float vector.
  Safe row appends replace that operation while still avoiding a zero-fill.

The original whole-application crash was not reproduced. The baseline RTX
4090 benchmark, which excludes emission, completed terrain and erosion but
cancelled during texture output at its four-minute deadline. Thus the default
emission rejection is confirmed; GPU workload and allocation changes address
additional resource pressure without claiming a proven cause for a native crash.

## Checks

- Full 8K export: passed, all layers and 25 erosion iterations, RTX 4090 Vulkan.
- Independent PNG/EXR header inspection: all eight files are 16,384 × 8,192.
- Export integration suite: 15 passed, one manual 8K test ignored by default.
- Export unit tests: 41 passed, one ignored.
- Erosion tests: seven passed.
- Staged-read tests: 19 passed, one ignored.
- Release build and library Clippy with warnings denied: passed. The build
  script reports its existing OpenEXR pkg-config fallback warning.
- Broader library run: 218 passed, two failed, three ignored. The failures are
  exact weather-field hash fixtures in
  `all_ocean_schedule_and_resolved_formation_match_the_pinned_fixture` and
  `provenance_mode_is_capability_gated_and_pins_both_formation_paths`.
  Weather code was unchanged; those failures were not rebaselined.

To repeat the full export:

```sh
rtk cargo test --release --test export_streaming eight_k_all_layers_with_erosion -- --ignored --nocapture
```

The existing `.serena/project.yml` changes were left untouched.
