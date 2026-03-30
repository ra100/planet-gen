---
title: "feat: Redesign erosion for channel carving + detail preservation"
type: feat
status: active
date: 2026-03-30
origin: docs/brainstorms/2026-03-30-erosion-redesign-requirements.md
---

# Redesign Erosion for Channel Carving + Detail Preservation

## Overview

Replace the current diffusion-like erosion with a two-tier system: concentrated flow routing that carves visible river valleys, plus detail-preserving weathering that enhances rather than destroys terrain character.

## Problem Frame

The current erosion algorithm (stream-power with 8-pass flow accumulation) acts as uniform smoothing because water never concentrates into channels — it propagates only ~8 cells before being consumed. Thermal erosion aggressively removes detail from all steep terrain. The net effect is loss of plate-generated terrain structure with no visible geological features added. (see origin: docs/brainstorms/2026-03-30-erosion-redesign-requirements.md)

## Requirements Trace

- R1. Visible river valley channels at planet scale via steepest-descent flow concentration
- R2. Channel depth increases with upstream drainage area (sqrt scaling)
- R3. Channels terminate at ocean level or flat basins (lake beds)
- R4. Channel paths follow terrain structure (plate boundaries as drainage divides)
- R5. Preserve high-frequency detail at plate boundaries and mountain ridges
- R6. Soften mountain peaks slightly (round, not flatten)
- R7. Add roughening noise to eroded lowland areas
- R8. Remove thermal erosion
- R9. Works at 512x512 preview and up to 8192 export resolution
- R10. Erosion slider 0-50 controls intensity, default 25 shows clear channels
- R11. GPU compute shader, same ping-pong buffer architecture

## Scope Boundaries

- No particle-based hydraulic simulation
- No sediment transport or alluvial fans
- No visible river water/color — terrain channels only
- Lake detection is geometric (flat basins) not hydrological

## Context & Research

### Relevant Code and Patterns

- `src/shaders/erosion.wgsl` — Current shader with `accumulate_flow` and `erode` entry points
- `src/terrain_compute.rs` — `ErosionPipeline` with 4-binding layout (input_height, output_height, params, water), ping-pong buffers, per-face processing
- `src/terrain_compute.rs:ErosionParams` — 8-field uniform struct (32 bytes, 16-aligned)
- All compute shaders use `@workgroup_size(16, 16)`, dispatch `(res + 15) / 16`

### Key Existing Pattern

The `accumulate_flow` pass currently:
1. Each pixel starts with 1.0 rainfall
2. Receives water from uphill neighbors that consider it their lowest neighbor
3. Transfer is partial (0.25 per iteration) across all 4 directions
4. Runs 8 sub-passes per erosion iteration

**Root cause of diffusion**: Water splits equally among directions instead of flowing to the single steepest neighbor. After 200 total sub-passes (8 × 25 iterations), flow has spread out instead of concentrating.

## Key Technical Decisions

- **D8 steepest-descent routing over split-direction**: Send all water to the single lowest neighbor instead of splitting. This is the standard approach in hydrology (D8 algorithm) and naturally creates channel concentration without needing atomics or path tracing.
- **More sub-passes for longer propagation**: Increase from 8 to 64 sub-passes per iteration. At 512 resolution, this allows drainage paths up to ~64 cells long — sufficient for visible river-scale features.
- **Resolution-adaptive sub-passes**: Scale sub-passes with resolution (64 for 512, 128 for 8192) so export resolution gets proportionally longer drainage.
- **Separate channel carving from weathering**: The erode pass distinguishes high-drainage (channel carving) from low-drainage (gentle weathering). This is one shader with two behaviors, not two separate passes.
- **Remove thermal erosion entirely**: It destroys detail at all slopes without adding realism at planet scale. (see origin)

## Open Questions

### Resolved During Planning

- **How many sub-passes?** 64 at 512 resolution, scaling linearly with resolution / 8. This gives drainage paths roughly 1/8th of face width — enough for continent-spanning rivers.
- **Channel depth function**: `sqrt(drainage) * slope * rate` (stream-power law, already used but now with concentrated flow it will actually create channels).

### Deferred to Implementation

- Exact erosion_rate and deposition_rate values need tuning after seeing concentrated flow results
- Whether lake basin detection needs explicit flat-filling or emerges naturally from deposition

## High-Level Technical Design

> *This illustrates the intended approach and is directional guidance for review, not implementation specification.*

```
Per erosion iteration:
  1. Reset water buffer to 1.0 per pixel (rainfall)
  2. Flow accumulation (64 sub-passes):
     For each pixel:
       Find steepest-descent neighbor (lowest of 8 neighbors)
       Send ALL water to that neighbor: water[neighbor] += water[self]
       water[self] = 1.0 (reset to base rainfall)
     → After 64 passes, valley pixels have high water values (hundreds)
       Ridge pixels have low water (near 1.0)
  3. Erosion pass:
     For each pixel:
       If drainage > threshold:
         Channel carve: depth = sqrt(drainage) * slope * K_channel
       Else:
         Gentle weathering: soften peaks slightly
       Add roughening noise to low-elevation eroded areas
       Skip ocean pixels
       Deposition only at convergence points (high drainage + low slope)
```

## Implementation Units

- [ ] **Unit 1: Rewrite flow accumulation to D8 steepest-descent**

**Goal:** Replace split-direction flow with single-steepest-neighbor routing so water concentrates into channels

**Requirements:** R1, R2, R4

**Dependencies:** None

**Files:**
- Modify: `src/shaders/erosion.wgsl` (accumulate_flow entry point)
- Test: `src/terrain_compute.rs` (existing erosion tests)

**Approach:**
- In `accumulate_flow`, find the lowest of 8 neighbors (not 4 — add diagonals)
- Send entire water amount to that neighbor via the output buffer
- Reset own water to 1.0 (base rainfall) after sending
- Use the ping-pong pattern: read water from input buffer, write to output buffer
- This requires changing the water buffer from single read-write to a ping-pong pair

**Patterns to follow:**
- Existing `accumulate_flow` structure for bounds checking and neighbor access
- `get_h()` helper pattern for neighbor sampling

**Test scenarios:**
- Happy path: After flow accumulation on a simple slope, valley pixels should have water >> 1.0 while ridge pixels stay near 1.0
- Edge case: Flat terrain should distribute water roughly evenly (no NaN or explosion)
- Integration: Full erosion pipeline (accumulate + erode) still produces valid heightmap with no NaN

**Verification:**
- `test_tectonic_terrain_generates` still passes (no NaN, valid ranges)
- `test_export_small_resolution` still passes

- [ ] **Unit 2: Add water ping-pong buffer to ErosionPipeline**

**Goal:** Support the D8 routing which needs to read from one water buffer and write to another

**Requirements:** R11

**Dependencies:** Unit 1 (shader expects two water buffers)

**Files:**
- Modify: `src/terrain_compute.rs` (ErosionPipeline, bind group layout, erode method)
- Modify: `src/shaders/erosion.wgsl` (add second water binding)

**Approach:**
- Add binding 4: `water_out` (read-write storage buffer) to the bind group layout
- Create water_a and water_b buffers in the erode method (same as height ping-pong pattern)
- Alternate water buffers each sub-pass: even passes read water_a/write water_b, odd passes reverse
- Update ErosionParams if needed for sub-pass count
- Reset water buffers to 1.0 between flow accumulation rounds

**Patterns to follow:**
- Existing height ping-pong pattern (bg_a_to_b, bg_b_to_a) in the erode method

**Test scenarios:**
- Happy path: Pipeline creates correct bind groups with 5 bindings without GPU validation errors
- Edge case: Odd and even iteration counts both produce correct final water buffer selection

**Verification:**
- `cargo build` succeeds with updated bind group layout
- All existing tests pass

- [ ] **Unit 3: Rewrite erosion pass for channel carving + weathering**

**Goal:** Replace uniform erosion with drainage-aware carving that creates channels where water concentrates and gentle weathering elsewhere

**Requirements:** R1, R2, R3, R5, R6, R7, R8

**Dependencies:** Units 1, 2

**Files:**
- Modify: `src/shaders/erosion.wgsl` (erode entry point)

**Approach:**
- Remove thermal erosion entirely (R8)
- High-drainage pixels (> threshold): apply stream-power carving `K * sqrt(drainage) * slope`
  - This creates V-shaped valleys where flow concentrates
  - Depth scales with sqrt of upstream area (R2)
- Low-drainage pixels: gentle peak softening only
  - Average with neighbors weighted by elevation difference
  - Small effect — rounds peaks without flattening (R6)
- Add roughening noise to eroded lowland pixels (R7)
  - `height += snoise(pos * 20.0) * 0.01 * erosion_factor`
- Deposition at convergence points: high drainage + low slope → sediment fill
  - Creates natural flat basin floors at river endpoints (R3)
- Skip ocean pixels (existing behavior)

**Patterns to follow:**
- Existing erode function structure
- snoise usage from plates.wgsl for roughening noise

**Test scenarios:**
- Happy path: After erosion, terrain has lower values in valleys between ridges (channel formation)
- Happy path: Mountain ridges retain height variation (not smoothed flat)
- Edge case: Ocean pixels unchanged after erosion
- Edge case: Zero erosion iterations produces unchanged terrain

**Verification:**
- `test_tectonic_terrain_has_mountains` still passes (peaks preserved)
- `test_tectonic_terrain_has_bimodal_distribution` still passes (land/ocean ratio maintained)

- [ ] **Unit 4: Resolution-adaptive sub-pass count + parameter tuning**

**Goal:** Scale flow propagation with resolution so channels form at both preview and export resolutions

**Requirements:** R9, R10

**Dependencies:** Units 1-3

**Files:**
- Modify: `src/terrain_compute.rs` (ErosionPipeline::erode method — sub-pass count logic)
- Modify: `src/shaders/erosion.wgsl` (erosion rate constants if needed)

**Approach:**
- Compute sub-passes as `max(64, resolution / 8)` — ensures propagation covers ~1/8 face width
- At 512: 64 sub-passes. At 8192: 1024 sub-passes
- Tune erosion_rate for visible but not excessive channel depth at default 25 iterations
- Tune the drainage threshold that separates channel carving from gentle weathering

**Patterns to follow:**
- Existing flow_sub_iterations constant (currently hardcoded to 8)

**Test scenarios:**
- Happy path: At resolution 64 with 2 iterations, erosion completes without timeout
- Happy path: At resolution 512 with 25 iterations, channels are visibly deeper than surrounding terrain
- Edge case: Erosion slider at 0 produces no change; at 50 produces deep channels without terrain collapse

**Verification:**
- Full test suite passes
- Export at small resolution completes in reasonable time

## System-Wide Impact

- **Interaction graph:** ErosionPipeline is called from `app.rs::regenerate_terrain()` and `export.rs::run_export()`. Both paths pass the same parameters. No other consumers.
- **Error propagation:** GPU shader errors surface as wgpu validation errors at pipeline creation time. Runtime NaN in heightmap would propagate to preview and export.
- **State lifecycle risks:** Adding a 5th binding to the bind group layout changes the pipeline. Old cached pipelines would be invalid — but pipelines are recreated each session, not persisted.
- **API surface parity:** The export pipeline's albedo_map.wgsl and roughness_map.wgsl shaders consume the eroded heightmap but don't need changes — they read the final height values regardless of how erosion produced them.

## Risks & Dependencies

| Risk | Mitigation |
|------|------------|
| 64+ sub-passes may be slow at preview resolution | Profile and reduce if > 200ms. Can reduce default iterations from 25 to 15 if needed |
| Channel carving may create unrealistic straight lines | The terrain already has domain warping; channels follow the warped terrain which creates natural curves |
| Roughening noise in erosion shader needs same snoise function | Erosion shader doesn't currently include noise.wgsl — need to concatenate it at shader load time |

## Sources & References

- **Origin document:** [docs/brainstorms/2026-03-30-erosion-redesign-requirements.md](docs/brainstorms/2026-03-30-erosion-redesign-requirements.md)
- Related code: `src/terrain_compute.rs`, `src/shaders/erosion.wgsl`
- D8 flow routing: standard hydrology algorithm (steepest-descent to single neighbor)
- Stream-power erosion law: E = K * A^m * S^n (m=0.5, n=1.0)
