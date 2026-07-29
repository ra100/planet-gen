# Procedural Terrain Baseline Boundary

Status: `NOT_RUN` for change approval. This record captures the available pre-U2.a evaluator artifact, not an accepted U2 baseline.

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
| Last accepted performance report | `NOT_RUN` | `perf_bench` emits console CSV for 256–2048 only; it has no persisted report, adapter metadata, or canonical run protocol. |

## Rollback Boundary

`last-accepted = NOT_RUN`. No U2 change may be accepted against this record until a complete accepted evaluator and performance baseline exists. The seed-42 artifacts above are characterization-only evidence and are not a rollback target.
