# Procedural Terrain Baseline Boundary

Status: `NOT_RUN` for change approval. This record captures the available pre-U2.a evaluator artifact, not an accepted U2 baseline.

## Version Policy

`procedural-terrain-u2-v1` names the current-pipeline U2 baseline. Final `procedural-terrain-*-v1` acceptance remains reserved for U7 because its required macro, hydrology, residual, and final-weather stages do not exist before U3–U6. This policy does not change `last-accepted=NOT_RUN`.

## Provenance

| Field | Value |
|---|---|
| Recorded | 2026-07-29 |
| Git revision | `614c58bf98a174d6c6492ee0f94cfc002c7d5f29` |
| Dirty-worktree diff SHA-256 | `b472987499a6b24f56d0286468bf77038432960587fa2090b8774463b61b13ce` |
| `Cargo.lock` SHA-256 | `2cd34afb75f364aa9ce649e09b7790814e391cdcd73e1db5c1a8fa624cb65cea` |
| Evaluator SHA-256 | `28c2549e62bfa39c2d935b768426cfc5c1067005e2a93190ae28735e0bb6b5ca` |
| Evaluator loader SHA-256 | `4947fea192151e54257f1dff8ddd3487cb5c08106c002a4018085ebff83388bc` |
| Adapter identity | `NOT_RUN` — evaluator output does not record an adapter |

## Available Evaluator Artifact

The evaluator's existing `capture-control` path hard-codes seed `42`. It has no preset or seed argument, so this is evidence for only the seed-42 portion of `procedural-terrain-768-v1`; seed `997` is `NOT_RUN`.

```text
cargo run --bin procedural_terrain_eval -- capture-control --resolution 768 \
  --dir target/procedural-terrain-baseline-768-seed42
cargo run --bin procedural_terrain_eval -- validate-control --resolution 768 \
  --dir target/procedural-terrain-baseline-768-seed42
cargo run --bin procedural_terrain_eval -- capture-control --resolution 768 \
  --dir target/procedural-terrain-baseline-768-seed42-repeat
cargo run --bin procedural_terrain_eval -- compare-bytes \
  --first target/procedural-terrain-baseline-768-seed42 \
  --second target/procedural-terrain-baseline-768-seed42-repeat
```

| Item | Value |
|---|---|
| Control artifact | `target/procedural-terrain-baseline-768-seed42/` |
| Repeat artifact | `target/procedural-terrain-baseline-768-seed42-repeat/` |
| Resolution / seed | `768` / `42` |
| FNV-1a-64 | `f2769772e4602729` |
| Content-set SHA-256, both captures | `b0beebaab4d0af4ed6c69124fac7ce4d52a31b133735eebf94ba5c61248dc73e` |
| Exact repeat | `gate.byte_equality=PASS` |
| Control validation | size, finite, orientation, range, edge, corner, and pole slope `PASS`; normal and pole elevation `FAIL` |

| Face | SHA-256 |
|---|---|
| `posx.f32le` | `60a2665540d70f80c701714a195f1b878c89742846d7d9f2c46246ab80de1b75` |
| `negx.f32le` | `0f149bf7fb58f68371f174b8ec0e0035a1204a5d0efec2e22799d4565c13ea83` |
| `posy.f32le` | `2f9dfe7c46c72a70a7fd8ea141c3a22b19f834f98ec4ef8549eefe608f6ce523` |
| `negy.f32le` | `cbe5e2787e09d2b7c643fdcf459d8f27fd54ba1bfd667a5bd3b73b150ad70327` |
| `posz.f32le` | `53adb9cc5228f6faefec12b6a1d8a5533ca76b8be991150a0dfcdaf6c6176ee4` |
| `negz.f32le` | `81d2d5cc27c6cd87762fa4ba5605036f416cd914d129a299400ba13e10ee64c0` |

### Evaluator Report

```text
result_version=1
command=validate-control
artifact=control
resolution=768
gate.size=PASS
gate.finite=PASS
gate.orientation_fixture=PASS
gate.control_range=PASS
gate.height_edge=PASS
gate.height_corner=PASS
gate.normal=FAIL
gate.pole_elevation=FAIL
gate.pole_slope=PASS
metric.fnv1a64=f2769772e4602729
metric.height_edge_p95=0.023057170
metric.height_corner_p95=0.024363194
metric.normal_p95_deg=16.654115677
metric.normal_max_deg=61.390102386
metric.pole_elevation_north_p50=0.596950531
metric.pole_elevation_south_p50=0.128587052
metric.pole_slope_north_p50=0.007236107
metric.pole_slope_south_p50=0.007191223
```

## Missing Canonical Evidence

| Requirement | Status | Reason |
|---|---|---|
| Seed `997` artifact | `NOT_RUN` | Existing evaluator cannot select a seed. |
| Canonical preset parameter record | `NOT_RUN` | Existing evaluator exposes only resolution. |
| Adapter / limits / RSS | `NOT_RUN` | Existing evaluator does not emit them. |
| Last accepted evaluator report | `NOT_RUN` | The captured seed-42 control has failing normal and pole-elevation gates. |
| Last accepted performance report | `NOT_RUN` | The U2 report protocol is available, but no canonical 768/8K measured repetition set has been published. |

## U2 Performance Evidence Protocol

`cargo run --release --bin perf_bench -- --u2-not-run-report` publishes a durable `NOT_RUN` record under `target/procedural-terrain-evidence/`. The record includes the U2 preset, timing and memory fields, gate statuses, artifact SHA-256 values, and a SHA-256 manifest digest. It is intentionally not a performance claim and never updates `last-accepted.json`.

`cargo run --release --bin perf_bench -- --u2-768` performs one unmeasured 768 warm-up plus one measured current-preview regeneration, then publishes actual adapter, device limits, stage timings, owned-live-byte estimate, and Linux RSS when available. `generation_inclusive_ms` and `erosion_inclusive_ms` include their internal GPU readback/mapping waits; they do not claim a separable readback phase. `upload_sync_ms` includes an explicit queue completion wait. It cannot advance `last-accepted.json` because the required 8K evidence is absent. `cargo run --release --bin perf_bench -- --u2-8k` is the only command that runs the expensive 8K end-to-end export phase; it must be invoked explicitly and remains non-accepting until all required stage measurements and run repetitions are present. The authoritative Cold8k deadline is `240,000 ms` (four minutes).

The U2 768 mode uses the existing preview's 15-iteration adaptive erosion budget at 768 rather than the 25-iteration export/default budget. This changes benchmark accounting only; it does not alter preview, export, terrain, or erosion behavior.

Canonical 768 warm and 8K cold measurements remain `NOT_RUN`; adapter limits, owned live bytes, and RSS remain `NOT_RUN` until a measured run records them. Only a report with all required U2 gates passing may replace `last-accepted.json`.

### U2-006 representative 768 result

The representative adapter run `u2-768-warm-seed42-rep1-1785334494108772784` recorded NVIDIA GeForce RTX 4090 (Vulkan), generation `10.906492ms`, erosion `1053.29134ms`, upload enqueue `4.452403ms`, total `1068.650885ms`, owned live bytes `28311552`, RSS `705695744`, and device limits `268435456` max buffer bytes / `134217728` storage-binding bytes / `8192` texture dimension. This pre-remediation evidence is retained unchanged; its upload field was enqueue timing, not completion-synchronized timing. The 768 warm gate is `FAIL` because total exceeds the one-second hard limit; 8K remains `NOT_RUN`. Its manifest is `1c568e77ad3dab1cec3838495fb75f5d1c553451780b4df1082604f6e8183659`, and `last-accepted.json` remains absent/unadvanced. No 8K run was attempted after this hard-gate failure.

After aligning the benchmark with the existing 768 preview erosion budget, `u2-768-warm-seed42-rep1-1785335058012584537` recorded generation-inclusive `10.738536ms`, erosion-inclusive `646.166165ms`, upload-sync `3.910108ms`, and total `660.815269ms` on the same adapter. The 768 warm gate is `PASS`; owned bytes and RSS gates are also `PASS`. Its manifest is `f08ff4914954035d65280f050d0fae9b31e965e40c7e2d107c1d9db4e5564689`. Completion remains `FAIL` and `last-accepted.json` remains absent because 8K evidence is `NOT_RUN`.

### U2-008 8K limit boundary

Tile selection keeps the fixed six-texel halo and bounds a tile's input plus output/staging footprint by `(tile + 12)^2 * (4 + 2 * output_element_bytes)` against both queried buffer limits. On the RTX 4090's 128MiB storage-binding limit, a requested 2048 tile reduces to 1024; the configured 512 tile is already legal. The historical legacy path retained six full terrain faces, one full face map, and one full equirectangular layer; its `6 * n² * 4 + n² * 16 + 2 * n² * 16 = 4.5GiB` estimate caused the preserved failed run `u2-8k-cold-seed42-rep1-1785451466987823955` to stop before allocation. U2-013 retains exact halo-synchronised erosion and accounts for every concurrently allocated erosion strip: four storage buffers plus one staging buffer per strip. The 8K owned-live-byte budget is now 4GiB; export preflight takes the maximum of the staged-export peak and terrain-plus-aggregate-erosion peak, and rejects configurations above it before allocation. U2-012 still selects staged EXR/PNG layers only and explicitly excludes legacy emission. That estimate is not runtime proof; a fresh complete 8K evidence run remains required.

The later complete cold run `u2-8k-cold-seed42-rep1-1785691878476508601` is retained at `target/procedural-terrain-evidence/u2-8k-cold-seed42-rep1-1785691878476508601/manifest.json` and `stage-journal.json`. It recorded total `240335.084225 ms`, exceeding the `240000 ms` Cold8k deadline; `cold_8k=FAIL` and completion is `FAIL`. The user explicitly accepts U2 as slow for 8K and authorizes U3 to proceed by manual product waiver. This is not a measured benchmark `PASS`, does not update `last-accepted.json`, and leaves the failed timing evidence and every final acceptance gate unchanged.

## Rollback Boundary

`last-accepted = NOT_RUN`. No U2 change may be accepted against this record until a complete accepted evaluator and performance baseline exists. The seed-42 artifacts above are characterization-only evidence and are not a rollback target.
