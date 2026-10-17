---
title: "feat: Plate motion history (geologic time, collision history, supercontinent cycle)"
type: feat
status: completed
date: 2026-10-17
origin: "Plans.md Phase 9b tasks 9b.3–9b.5; user request in the PlanetGen project thread to continue 9b.3–9b.5"
---

# Plate motion history — implementation plan

Phase 9b.2 made plate motion visible (Euler velocity arrows in the Plates
view). This plan makes the motion *act*: plates move over geologic time,
collide, merge and rift, and the record of those events shapes the terrain.

## Constraints

- **Default output is unchanged.** Geologic time 0 Myr (the default) must
  produce bit-identical plates and terrain, so the existing pinned GPU
  fixtures and the planet look users already have stay the same.
- **One plate buffer.** Terrain dispatch already takes one plate storage
  buffer (`create_plates_buffer`, `generate`, `dispatch_tile`, the preview
  Plates view). History rides in the same buffer as trailing records after
  the real plates, so no bind group layout changes and no call-site churn.
- **Preview/export parity.** App preview and tiled export both build the
  plate records through the same function from the same parameters.

## Representation

A simulated plate owns one or more **seeds** (points on the unit sphere)
that move rigidly with the plate's Euler rotation. Terrain assignment is
"nearest seed, then per-plate noise bias", which is exactly the current
`plate_distance` when a plate has one seed at its center. Multi-seed plates
let a merged supercontinent keep its full area instead of collapsing into a
single Voronoi cell.

**History features** are great-circle segments with a kind, an amplitude and
a width, carried by the plate they formed on:

| Kind | Formed when | Terrain |
|------|-------------|---------|
| Suture | two continental plates collide and merge | eroded orogen belt (Appalachians/Urals analogue) |
| Arc | an oceanic seed subducts under another plate | volcanic cordillera / island arc on the overriding side |
| Rift | a supercontinent splits | rift trough / passive margin |

Amplitude decays with feature age (erosion, e-folding ~600 Myr, with a floor
for ancient roots) and width grows with age.

## GPU layout (trailing records in `array<Plate>`)

```
[0 .. P)           real plates; _pad = bitcast(seed_start | seed_count << 16) when count > 1, else 0
[P]                meta record: plate_type = -1 marker, center = bitcast(seed_total, feature_total, 0)
[P+1 .. P+1+S)     seeds: center = position, plate_type = owning plate index
[.. +2F)           features, two records each: (a, kind, b, amplitude), (width, age_frac, 0, ...)
```

With no multi-seed plates and no features nothing is appended, so the
buffer is byte-identical to today's. `plates::plate_count` finds `P` from
the marker; `num_plates` uniforms keep meaning "real plates".

## Units

| Unit | What | Done when |
|------|------|-----------|
| U1 (9b.3) | `tectonic_history` CPU simulation: Rodrigues rotation of seeds/features per step (5 Myr), plate reorganisation (new Euler poles ~every 150 Myr), collision detection on seed separation, subduction (oceanic seed consumed, arc recorded), seafloor spreading (new oceanic seeds fill gaps). `GEOLOGIC TIME` slider 0–3000 Myr, saved in planet files, carried in `TerrainGenerationParams` to export. | 0 Myr is bit-identical; history is deterministic per seed; seeds stay on the sphere; continental area within tolerance |
| U2 (9b.4) | `plates.wgsl` reads seeds and features: multi-seed `plate_distance`, nearest-seed boundary normals, feature relief (sutures, arcs, rifts) scaled by age decay | Older planets have more relief features; a terrain test shows 0 Myr equals the no-history terrain bit for bit and 1500 Myr differs |
| U3 (9b.5) | Continental assembly and breakup: continental collisions merge plates (area-weighted Euler vector); merged plates with ≥3 seeds rift after a seeded tenure (200–350 Myr) into two diverging plates, recording rift features | A seeded run shows continental plate count fall (assembly) then rise (breakup) |
| U4 | Plates view shows seeds' plates and history features (suture purple, arc orange, rift teal); the view mirrors `plates.wgsl` assignment (the 9b.2 overlay copied the unused multi-pass shader's warp and bias) | Overlay matches terrain plate assignment |
