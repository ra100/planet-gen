---
title: "feat: Tectonic plate-driven terrain generation"
type: feat
status: active
date: 2026-03-30
origin: docs/brainstorms/2026-03-30-tectonic-terrain-requirements.md
---

# Tectonic Plate-Driven Terrain Generation

## Overview

Replace the current noise-only heightmap with a geologically structured terrain pipeline: Voronoi plates on sphere → boundary classification → height from geology → fBm detail on top. Moves terrain generation from the fragment shader to a GPU compute pipeline, storing results in a cubemap texture.

## Problem Frame

Current terrain is layered noise. Mountains appear randomly, continents are blobs, there's no geological structure. The research recommends Voronoi-based tectonic plates as the foundation for realistic terrain.

## Requirements Trace

- R1-R3: Plate generation and classification
- R4-R7: Boundary-driven terrain features
- R8-R10: Continental structure
- R11-R13: Geological features (hotspots, interiors, detail noise)
- R14-R17: Compute pipeline architecture

## Key Technical Decisions

- **Spherical Voronoi via cube-face projection**: Generate plate centers in 3D, compute Voronoi on the sphere by finding the nearest center for each pixel. No need for complex spherical Voronoi — just `distance(point, plate_center)` on the unit sphere works naturally.
- **Plates stored as a buffer of structs**: Each plate has center position, velocity vector, type (continental/oceanic), and index. Passed to compute shader as a storage buffer.
- **Two-pass compute pipeline**: Pass 1 assigns each pixel to a plate and computes boundary distances. Pass 2 generates height from plate type + boundary type + fBm detail.
- **Cubemap at 512x512 per face for preview**: 6 × 512 × 512 = 1.5M pixels. At ~7ms per tile on GPU, this generates in <100ms total.

## Implementation Units

- [ ] **Unit 1: Plate generation on CPU (Rust)**

**Goal:** Generate N tectonic plates with centers, velocities, and types. Pass to GPU as a buffer.

**Requirements:** R1, R2, R3

**Files:**
- Create: `src/plates.rs` — PlateConfig struct, plate generation from seed
- Modify: `src/lib.rs` — add module

**Approach:**
- `PlateConfig`: center (vec3), velocity (vec3), plate_type (continental=1/oceanic=0), index
- Generate N plate centers using Fibonacci sphere distribution (evenly spaced points on sphere) + noise perturbation from seed
- N = `6 + (mass_earth * 4.0 + tectonics_factor * 6.0) as u32` → range ~6-16
- Assign continental/oceanic: pick `N * (1 - ocean_fraction)` plates as continental
- Assign velocity vectors: random unit vectors scaled by tectonics_factor (more active = faster motion = more dramatic boundaries)
- Boundary type at any point = classify from relative velocity of two nearest plates:
  - dot(relative_velocity, boundary_normal) < -threshold → convergent
  - dot(relative_velocity, boundary_normal) > threshold → divergent
  - otherwise → transform

**Test scenarios:**
- Earth params (mass=1, ocean=0.7, tectonics=0.85): 10-12 plates, ~30% continental
- Different seeds produce different plate configurations
- All plate centers are on the unit sphere
- Continental fraction matches ocean_fraction

**Verification:** PlateConfig generates valid plate arrays with correct counts and types

---

- [ ] **Unit 2: Plate assignment compute shader**

**Goal:** For each pixel on the cubemap, find the nearest plate center and compute boundary information.

**Requirements:** R1 (GPU side), R14

**Files:**
- Create: `src/shaders/plates.wgsl` — compute shader
- Create: `src/terrain_compute.rs` — Rust-side compute pipeline orchestration

**Approach:**
- Input: plate buffer (N plates), cube face index, resolution
- Output per pixel: plate_index (u32), distance_to_boundary (f32), boundary_type (u32), second_nearest_plate (u32)
- For each pixel: project to sphere position, find two nearest plate centers by distance
- Boundary distance = distance to nearest - distance to second nearest (small = near boundary)
- Boundary type = classify from relative velocity of two nearest plates
- Store in an intermediate texture/buffer for Pass 2

**Test scenarios:**
- Each pixel is assigned to exactly one plate
- Boundary distances are smallest at Voronoi edges
- Boundary types are consistent along edges

**Verification:** Plate map texture shows distinct Voronoi regions with classified boundaries

---

- [ ] **Unit 3: Height generation compute shader**

**Goal:** Generate heightmap from plate assignments + boundary types + fBm detail.

**Requirements:** R4-R13

**Files:**
- Modify: `src/shaders/plates.wgsl` — add height generation pass
- Modify: `src/terrain_compute.rs` — orchestrate two-pass pipeline

**Approach:**
- Base elevation: continental plate → +0.3, oceanic plate → -0.4 (bimodal distribution per research 7.2)
- Boundary modifiers per type:
  - Convergent continental-continental: mountain ridge, height += 0.5 * exp(-boundary_dist²/σ²)
  - Convergent oceanic-continental: volcanic chain on continental side + trench on oceanic side
  - Divergent: mid-ocean ridge (underwater) or rift valley (on land)
  - Transform: subtle height variation, offset terrain
- Continental interior: smooth plateau with low-amplitude noise
- Island arcs: at oceanic-oceanic convergent, create arc-shaped elevated region
- Volcanic hotspots: 1-3 points from seed, add shield volcano profiles
- fBm detail: layer 8-octave noise at low amplitude (0.1-0.15) on top of geological structure
- Coastal shelves: smooth transition from continental to oceanic elevation over a margin width

**Test scenarios:**
- Mountains appear at convergent boundaries, not randomly
- Continental interiors are flatter than boundary zones
- Ocean floors have mid-ocean ridges at divergent boundaries
- Height distribution is bimodal (continental shelf + ocean floor peaks)

**Verification:** Height debug view shows geological structure, not noise blobs

---

- [ ] **Unit 4: Cubemap preview integration**

**Goal:** Wire the compute-generated cubemap into the existing preview pipeline.

**Requirements:** R14-R17

**Files:**
- Modify: `src/preview.rs` — add cubemap texture input, remove direct noise computation
- Modify: `src/shaders/preview.wgsl` — sample cubemap for height instead of computing fbm_preview()
- Modify: `src/app.rs` — orchestrate: generate plates → run compute → pass cubemap to preview

**Approach:**
- `generate_preview` flow becomes:
  1. Generate plates on CPU (Unit 1)
  2. Run compute pipeline to produce heightmap cubemap (Units 2+3)
  3. Upload cubemap to GPU texture (R16Float, filterable, with linear sampling)
  4. Preview shader samples `textureSample(height_cubemap, sampler, sphere_direction)` for height
  5. Temperature, moisture, biome pipeline continues as-is in fragment shader using the sampled height
- Remove `fbm_preview()`, `continental_base()`, `detail_fbm()` from preview.wgsl
- Keep: temperature, moisture (Hadley cells), Whittaker lookup, biome colors, altitude zonation, ocean ice, debug views

**Test scenarios:**
- Preview shows geological terrain structure (not noise blobs)
- Changing seed produces different plate configurations
- All existing parameters still affect the result (mass, distance, tilt, etc.)
- Debug views still work (height, temperature, moisture, biome, ocean/ice)
- Preview resolution control still works

**Verification:** `cargo run` shows a planet with visible plate-boundary mountains and geological structure

---

- [ ] **Unit 5: Voronoi edge warping for natural boundaries**

**Goal:** Make plate boundaries look natural (curved, irregular) instead of straight Voronoi lines.

**Requirements:** R1 (visual quality)

**Dependencies:** Unit 2

**Files:**
- Modify: `src/shaders/plates.wgsl` — warp distances before Voronoi assignment

**Approach:**
- Before computing nearest plate center, apply domain warping to the sample position
- `warped_pos = sphere_pos + noise(sphere_pos * 3.0) * 0.08`
- This makes Voronoi edges curved and irregular — plate boundaries look like real coastlines and mountain ranges instead of geometric lines
- Warping amount should be small enough to not break plate assignment consistency

**Test scenarios:**
- Plate boundaries are curved, not straight lines
- No pixel has ambiguous plate assignment due to excessive warping

**Verification:** Plate boundary map shows natural-looking irregular boundaries

## Risks & Dependencies

| Risk | Mitigation |
|------|------------|
| Cubemap seams when sampling between faces | Use R16Float with linear filtering (same approach we validated in Phase 3.5) |
| Voronoi computation expensive for many plates (N×pixel comparisons) | N is small (6-16), each comparison is just a dot product on sphere — trivially parallel on GPU |
| Transition from fragment-shader terrain may regress visual features | Keep all biome/climate code in fragment shader, only replace height source. Run existing tests to verify |
| Two-pass compute adds latency | Total is ~100-200ms for 512² × 6 faces — acceptable for parameter change, not per-frame |

## Sources & References

- **Origin document:** [docs/brainstorms/2026-03-30-tectonic-terrain-requirements.md](../brainstorms/2026-03-30-tectonic-terrain-requirements.md)
- Research section 7.3: Recommended terrain pipeline (Voronoi → ridges → fBm → erosion)
- Research section 7.2: Hypsometric curves (bimodal elevation distribution for Earth)
- Research section 2.1: Tectonic regimes
- Fibonacci sphere: standard algorithm for evenly distributing N points on a sphere
