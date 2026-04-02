---
title: "fix: City slider transition, background terrain generation, perf investigation"
type: fix
status: active
date: 2026-03-31
---

# Fix City Transition, Background Terrain Gen, Performance

## Overview

Three fixes: smoother city density slider at low values, move terrain generation to background thread so loading overlay actually shows, and investigate performance bottlenecks.

## Requirements Trace

- R1. City slider transition is gradual — 0.01 shows sparse cities, not a cliff from nothing to heavy
- R2. Loading overlay visible DURING terrain generation, not after
- R3. UI remains responsive during terrain generation (no freeze)

## Scope Boundaries

- NOT rewriting the terrain compute pipeline
- NOT adding GPU async compute (future optimization)
- Performance investigation is analysis only — fixes deferred to findings

## Key Technical Decisions

- **Background thread for terrain gen**: The current `regenerate_terrain()` blocks the main thread. Move it to `std::thread::spawn` with a channel to send results back. The existing export pipeline uses `ExportHandle` with a similar pattern — follow that.
- **City threshold uses `pow(dev, 2.0)`**: At low values (0.01), `pow(0.01, 2.0) = 0.0001` → almost no cities. At 0.5, `pow(0.5, 2.0) = 0.25` → moderate. At 1.0, still 1.0 → heavy. This creates a gentle ramp instead of a cliff.

## Implementation Units

- [ ] **Unit 1: Fix city slider transition**

  **Goal:** Make Development slider gradual at low values instead of cliff at 0.01.

  **Requirements:** R1

  **Dependencies:** None

  **Files:**
  - Modify: `src/shaders/preview_cubemap.wgsl`

  **Approach:**
  - In `compute_urban_density`, apply `pow(dev, 2.0)` to the development value before using it as threshold
  - This creates a quadratic ramp: 0.01→0.0001, 0.1→0.01, 0.5→0.25, 1.0→1.0
  - Alternatively, adjust the threshold formula: `(1.0 - pow(dev, 0.5)) * 0.35` to expand the low end

  **Test scenarios:**
  - Happy path: dev=0.01 → only a few tiny city dots visible
  - Happy path: dev=0.1 → sparse cities on prime coastal land
  - Happy path: dev=1.0 → heavy urbanization (same as before)

  **Verification:** Sliding from 0 to 0.1 shows a smooth ramp of increasing city density.

- [ ] **Unit 2: Background terrain generation**

  **Goal:** Move terrain generation to a background thread so UI stays responsive and loading overlay is visible.

  **Requirements:** R2, R3

  **Dependencies:** None

  **Files:**
  - Modify: `src/app.rs`

  **Approach:**
  - Add a terrain generation state: `terrain_generating: bool` flag or `Option<TerrainHandle>`
  - When `needs_terrain` is true: set `terrain_generating = true`, show overlay, then on next frame spawn background work
  - Problem: GPU operations (`terrain_compute.generate()`) require the GPU context which isn't Send. The compute pipeline uses `gpu.device` and `gpu.queue` which are `Arc<GpuContext>` — check if they're Send+Sync
  - If GPU context is Send: spawn thread directly with Arc clone
  - If NOT Send: use a two-frame approach — frame 1 shows overlay + schedules, frame 2 does the work. This at least lets the overlay paint once before blocking
  - The existing `ExportHandle` pattern in export.rs uses `Arc<GpuContext>` in a background thread — follow that pattern

  **Patterns to follow:**
  - `ExportHandle` in `src/export.rs` for background GPU work pattern
  - `src/app.rs` existing `export_handle` polling pattern

  **Test scenarios:**
  - Happy path: Click 2K resolution → overlay appears immediately → terrain generates → preview updates
  - Happy path: UI remains responsive during terrain generation (can still scroll sidebar)
  - Edge case: Clicking a slider while terrain is generating → queues next generation

  **Verification:** Loading overlay visible for the full duration of terrain generation. No UI freeze.

- [ ] **Unit 3: Performance investigation**

  **Goal:** Profile terrain generation to identify bottlenecks and document findings.

  **Requirements:** R3 (informational)

  **Dependencies:** Unit 2 (timing becomes measurable with background gen)

  **Files:**
  - Modify: `src/app.rs` (add timing instrumentation)

  **Approach:**
  - Add `std::time::Instant` timing around each phase: plate generation, compute shader, erosion
  - Print timing to console/log
  - Measure at 512, 768, 1024, 2048 resolutions
  - Document which phase dominates and potential optimizations
  - Known suspects: erosion pipeline scales O(n²) per iteration, 25 iterations at 2K = 100M pixels/iteration

  **Test scenarios:**
  - Happy path: Timing output printed showing ms per phase

  **Verification:** Console shows timing breakdown per terrain generation phase.

## Sources & References

- `src/app.rs`: `regenerate_terrain()`, `ExportHandle` pattern
- `src/export.rs`: Background GPU work with `Arc<GpuContext>`
- `src/shaders/preview_cubemap.wgsl`: `compute_urban_density` threshold
