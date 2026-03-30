---
title: "feat: GPU hydraulic erosion for realistic terrain"
type: feat
status: active
date: 2026-03-30
---

# GPU Hydraulic Erosion

## Overview

Add a multi-pass GPU hydraulic erosion simulation to the compute pipeline. Runs after plate-based height generation, before the preview shader samples the cubemap. Creates V-shaped river valleys, smooth lowlands, and sharpened mountain ridges — the visual signatures of water erosion that make terrain look geological rather than procedural.

## Problem Frame

Current terrain has tectonic structure (plates, mountains at boundaries) and noise detail, but lacks erosion signatures. Real planets have river-carved valleys, sediment-filled lowlands, and water-worn coastlines. The terrain looks "freshly generated" — like it was placed yesterday rather than shaped over millions of years.

Research section 7.3 specifies hydraulic erosion as step 4 of the recommended terrain pipeline, with 20-50 GPU compute iterations.

## Requirements Trace

- R1. Hydraulic erosion simulation runs as a compute shader pass on the cubemap heightmap
- R2. Creates V-shaped valleys where water flows downhill (visible in Height debug view)
- R3. Smooths lowland/plains areas where sediment accumulates
- R4. Preserves mountain peaks and ridges (erosion carves between peaks, not through them)
- R5. Iteration count controllable (20-50 range) via a slider or derived from planet properties
- R6. Total erosion compute time < 500ms on RTX 3080-class GPU for 512x512 cubemap

## Scope Boundaries

- Not tracking persistent rivers or lakes (water is simulated then removed each iteration)
- Not creating river texture maps (just heightmap modification)
- Not simulating glacial erosion (U-shaped valleys — future feature)
- Not modifying biome colors based on erosion (future: alluvial plains could be more fertile)

## Key Technical Decisions

- **Grid-based erosion over particle-based**: Grid-based is better for GPU parallelism — each pixel can be processed independently per iteration. Particle-based requires random memory access patterns that are slower on GPU. (Research section confirms: "grid-based for GPU parallelism")
- **Erosion runs on all 6 cube faces independently**: Each face is a 512x512 grid. Cross-face water flow would require stitching, which adds complexity. Independent per-face erosion with matching border heights is sufficient for visual quality.
- **Erosion amount scales with tectonics_factor and planet age**: More tectonically active planets have more water cycle → more erosion. Older surfaces (stagnant lid) have accumulated more erosion.

## High-Level Technical Design

> *This illustrates the intended approach and is directional guidance for review, not implementation specification.*

```
Per iteration (20-50 total):
  For each pixel in heightmap:
    1. Compute water flow direction (steepest descent to lowest neighbor)
    2. Compute erosion amount: E = K × slope^n
       - K = erodibility (from planet properties)
       - slope = height difference to lowest neighbor
    3. Remove material from current pixel (erosion)
    4. Add material to lowest neighbor (deposition)
    5. Apply thermal erosion: if slope > talus angle, material slides downhill

After all iterations:
  Smooth the result slightly to remove single-pixel artifacts
```

## Implementation Units

- [ ] **Unit 1: Erosion compute shader**

**Goal:** WGSL compute shader that performs one iteration of hydraulic erosion on a heightmap buffer.

**Requirements:** R1, R2, R3, R4

**Dependencies:** Existing `terrain_compute.rs` pipeline

**Files:**
- Create: `src/shaders/erosion.wgsl` — single-iteration erosion compute shader
- Modify: `src/terrain_compute.rs` — add erosion pass orchestration

**Approach:**
- Input: heightmap buffer (f32 per pixel), erosion parameters uniform
- Output: modified heightmap buffer (in-place)
- Per pixel: find lowest of 4 neighbors (von Neumann), compute slope, erode proportional to slope, deposit at lowest neighbor
- Use double-buffering (read from buffer A, write to buffer B, swap) to avoid race conditions
- Parameters: erosion_rate, deposition_rate, min_slope threshold

**Patterns to follow:** Existing `plates.wgsl` compute shader structure (workgroup size 16x16, params uniform, storage buffers)

**Test scenarios:**
- Happy path: flat terrain stays flat (no erosion when no slope)
- Happy path: single peak erodes into a cone shape after 20 iterations
- Happy path: V-shaped valley forms between two adjacent peaks
- Edge case: minimum-height terrain (all zeros) produces no change
- Integration: erosion output feeds correctly into cubemap upload + preview

**Verification:** Height debug view shows smoother valleys and sharper ridges after erosion vs without

---

- [ ] **Unit 2: Multi-pass erosion orchestration**

**Goal:** Run N iterations of the erosion shader with double-buffering, controlled by planet parameters.

**Requirements:** R5, R6

**Dependencies:** Unit 1

**Files:**
- Modify: `src/terrain_compute.rs` — add `run_erosion()` method with iteration loop
- Modify: `src/app.rs` — call erosion after plate generation, add erosion slider

**Approach:**
- Create two heightmap buffers (A and B) per face
- Loop N times: dispatch erosion shader reading A → writing B, then swap
- N derived from planet properties: `base_iterations * tectonics_factor * surface_age_factor`
  - Earth-like: ~25-30 iterations
  - Young active surface: ~15 iterations
  - Old stagnant surface: ~40 iterations
- Add "Erosion" slider (0-50, default derived from physics) in Visual Overrides section
- Profile to ensure < 500ms total for 6 faces × N iterations

**Test scenarios:**
- Happy path: 0 iterations produces identical output to no-erosion path
- Happy path: 30 iterations on Earth params produces visibly smoother terrain
- Edge case: 50 iterations doesn't over-erode (terrain should still have structure)
- Performance: 30 iterations on 512x512 × 6 faces completes in < 500ms

**Verification:** Slider works, erosion visibly smooths terrain, performance target met

---

- [ ] **Unit 3: Thermal erosion pass**

**Goal:** Add thermal erosion (talus slope collapse) alongside hydraulic erosion for more realistic mountain profiles.

**Requirements:** R4

**Dependencies:** Unit 1

**Files:**
- Modify: `src/shaders/erosion.wgsl` — add thermal erosion within the same compute pass

**Approach:**
- After hydraulic erosion step, check if slope to any neighbor exceeds talus angle (~35-45°)
- If so, transfer material from higher to lower pixel (landslide)
- This creates scree slopes at the base of cliffs and prevents impossibly steep terrain
- Talus angle is a constant (not a parameter)

**Test scenarios:**
- Happy path: vertical cliff erodes into sloped profile after iterations
- Happy path: gentle slopes unchanged by thermal erosion

**Verification:** Mountain profiles show gradual talus slopes, no vertical cliffs in the heightmap

---

- [ ] **Unit 4: Coastal smoothing**

**Goal:** Smooth terrain near sea level to create continental shelves and gentle shorelines.

**Requirements:** R3

**Dependencies:** Unit 2

**Files:**
- Modify: `src/shaders/erosion.wgsl` or separate post-pass — smooth terrain near ocean_level

**Approach:**
- After erosion iterations, apply a smoothing pass specifically near the ocean level
- Pixels within a small height range of ocean_level get blurred with neighbors
- This creates the gradual continental shelf transition the research describes (step 5: coastal erosion)
- Only affects terrain near sea level, not mountain interiors

**Test scenarios:**
- Happy path: coastline terrain is smoother than inland terrain at same elevation
- Edge case: fully oceanic planet (no land) shows no effect

**Verification:** Coastlines look less sharp in Height debug view, smoother shelf visible

## Risks & Dependencies

| Risk | Mitigation |
|------|------------|
| Double-buffering requires 2× memory per face (2 × 512 × 512 × 4 bytes = 2MB per face, 12MB total) | Trivial on modern GPUs with GBs of VRAM |
| Cross-face erosion discontinuities | Each face erodes independently. The plate noise already ensures faces have similar height at boundaries. Accept minor edge artifacts for v1 |
| Performance: 50 iterations × 6 faces × 512² pixels | Each iteration is a simple 4-neighbor comparison — very GPU-friendly. Profile and reduce iterations if needed |
| Over-erosion flattening all terrain | Cap erosion rate and add minimum-slope threshold below which no erosion occurs |

## Sources & References

- Research section 2.4: Erosion rates, stream power law `E = K × A^m × S^n`
- Research section 7.3: Recommended terrain pipeline step 4 (20-50 iterations)
- Existing compute pipeline: `src/terrain_compute.rs`, `src/shaders/plates.wgsl`
