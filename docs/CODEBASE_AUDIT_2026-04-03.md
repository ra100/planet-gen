# Planet Gen — Full Codebase Audit

**Date:** 2026-04-03
**Scope:** Entire codebase vs plans, brainstorms, research docs
**Goal:** Identify unused UI, dead code, stale plans, and items for cleanup

---

## 1. Dead Code & Unused Modules

### P0 — Remove immediately

| Item | Location | Reason | Action |
|------|----------|--------|--------|
| `terrain.rs` module | `src/terrain.rs` (~298 lines) | Zero imports anywhere. Superseded by `terrain_compute.rs` | Delete file, remove `pub mod terrain;` from lib.rs |
| `compute.rs` module | `src/compute.rs` | Zero callers. Debug gradient shader host — Phase 1 artifact | Delete file, remove `pub mod compute;` from lib.rs |
| `gradient.wgsl` | `src/shaders/gradient.wgsl` | Only referenced by dead `compute.rs` | Delete |
| `preview.wgsl` | `src/shaders/preview.wgsl` | Replaced by `preview_cubemap.wgsl` + compute pipeline | Delete |
| `terrain.wgsl` | `src/shaders/terrain.wgsl` | Only referenced by dead `terrain.rs` | Delete |
| `craters.wgsl` | `src/shaders/craters.wgsl` | Crater compute shader — never instantiated or dispatched | Delete |

### P1 — Review before removing

| Item | Location | Reason | Action |
|------|----------|--------|--------|
| `noise_test.wgsl` | `src/shaders/noise_test.wgsl` | Test shader loaded by `noise.rs::run_noise_test()` — never called from app | Delete unless needed for `cargo test` |
| HEALPix modules | `src/healpix.rs`, `src/healpix_terrain.rs`, `src/plate_sim.rs` | Fully implemented but **reverted** at commit `1aac311`. Not wired into app.rs. Only used internally by each other | Keep if Phase 6.3.4-6.3.5 is still planned; otherwise archive to branch |

**Total dead code: ~600+ lines of Rust + 5-6 orphaned shaders**

---

## 2. Unused UI Elements

### P0 — Export checkboxes (not connected)

Seven export layer checkboxes are rendered in the UI but **never passed** to the export pipeline:

| Field | Defined | Rendered | Passed to export? |
|-------|---------|----------|--------------------|
| `export_albedo` | `app.rs:73` | `app.rs:876` | **NO** |
| `export_roughness` | `app.rs:74` | `app.rs:877` | **NO** |
| `export_clouds` | `app.rs:75` | `app.rs:878` | **NO** |
| `export_height` | `app.rs:76` | `app.rs:879` | **NO** |
| `export_emission` | `app.rs:77` | `app.rs:880` | **NO** |
| `export_water_mask` | `app.rs:78` | `app.rs:881` | **NO** |
| `export_normals` | `app.rs:79` | `app.rs:882` | **NO** |

The `ExportConfig` struct and `spawn_export()` function unconditionally export ALL layers regardless of checkbox state.

**Action:** Either connect these to `ExportConfig` to make selective export work, or remove the checkboxes to avoid user confusion.

---

## 3. Plans.md Status Discrepancies

### P0 — Incorrect completion markers

| Phase | Task | Plans.md says | Reality | Action |
|-------|------|---------------|---------|--------|
| 6.3 | 6.3.1 "Wire HEALPix into app.rs" | cc:完了 [45789c7] | Was wired in then **reverted** at `1aac311`. Not active. | Change to `cc:reverted` or re-mark as TODO |
| 6.3 | 6.3.2 "Performance profiling" | cc:完了 [45789c7] | Profiling was for HEALPix path which is no longer active | Change to `cc:reverted` |
| 6.3 | 6.3.3 "Export support for HEALPix" | cc:完了 [45789c7] | Same — reverted with HEALPix pipeline | Change to `cc:reverted` |

### P1 — Phase numbering collision

Plans.md has **two** "Phase 7" sections:
1. "Phase 7: Blender Importer Addon" (tasks 6.1-6.6)
2. "Phase 7: Advanced Visual Features" (tasks 7.1-7.5)

The Blender Importer tasks are numbered 6.x but live under a "Phase 7" heading. This is confusing.

**Action:** Renumber to Phase 7 (Blender) and Phase 8 (Visual), or merge them.

---

## 4. Stale Plan Documents

### Plans that reference superseded approaches

| Plan file | Status issue | Action |
|-----------|-------------|--------|
| `2026-03-30-003-feat-tectonic-terrain-plan.md` | Original Voronoi plate plan — superseded by Phase 5.9 (noise rebuild), then 5.12 (multi-pass GPU), then 6.x (HEALPix) | Mark status: `superseded` |
| `2026-03-30-004-feat-erosion-redesign-plan.md` | Erosion redesign — may be partially implemented | Verify vs current `erosion.wgsl` and update |
| `2026-03-31-002-feat-cloud-layer-plan.md` | Cloud layer v1 — superseded by v2 (`003`) | Mark status: `superseded` |
| `2026-03-30-006-feat-pbr-surface-rendering-plan.md` | PBR plan — partially done (normal/roughness maps exist, metallic/emissive incomplete) | Mark status: `in_progress` or `partial` |
| `2026-03-30-007-feat-realistic-atmosphere-plan.md` | Atmosphere plan — only Mie scattering toggle exists, no ray-marching | Mark status: `in_progress` |

### Brainstorms without corresponding plans

| Brainstorm | Has plan? | Action |
|-----------|-----------|--------|
| `2026-04-02-physics-terrain-rebuild-requirements.md` | No plan doc (but covered by Phase 5.9 in Plans.md) | Add plan or note in brainstorm |
| `2026-04-03-multipass-plate-terrain-requirements.md` | No plan doc (but covered by Phase 5.12 in Plans.md) | Add plan or note in brainstorm |
| `2026-04-03-orogen-port-requirements.md` | No plan doc (but covered by Phase 6.x in Plans.md) | Add plan or note in brainstorm |

---

## 5. Active Pipeline Summary

The **current active terrain pipeline** in `app.rs::regenerate_terrain()` is:

```
1. plates::generate_plates()          → PlateGpu (CPU)
2. TerrainComputePipeline::generate() → TectonicTerrain (GPU: plate_assign → JFA → terrain_from_plates)
3. ErosionPipeline (progressive)      → erosion.wgsl
4. preview_cubemap.wgsl               → Fragment shader (biomes, climate, atmosphere, clouds, cities)
```

**NOT in the active pipeline:**
- `terrain.rs` (original noise terrain) — dead
- `healpix.rs` / `healpix_terrain.rs` / `plate_sim.rs` (HEALPix orogen) — implemented but reverted
- `compute.rs` (gradient test) — dead
- `craters.wgsl` (crater compute) — never integrated

---

## 6. Remaining TODO Items (from Plans.md)

| Task | Description | Notes |
|------|-------------|-------|
| 5.8.2 | Export cloud + night light layers | Feature gap — these layers render in preview but don't export |
| 6.3.4 | Parameter tuning for HEALPix terrain | Blocked — HEALPix pipeline is reverted |
| 6.3.5 | Remove old noise terrain code | Blocked — HEALPix pipeline is reverted |
| 8a.7 | Performance + visual comparison docs | Missing documentation |
| Phase 7 | Blender Importer Addon (6 tasks) | Not started |
| Phase 7 | Advanced Visual Features (5 tasks) | Not started |
| Phase 8b | Plate Motion Simulation (5 tasks) | Not started |
| Phase 8c | Mantle Convection (4 tasks) | Not started |
| Phase 9 | Polish & Distribution (4 tasks) | Not started |

---

## 7. Cleanup Checklist

### Immediate (safe to do now)

- [ ] Delete `src/terrain.rs` + remove from `lib.rs`
- [ ] Delete `src/compute.rs` + remove from `lib.rs`
- [ ] Delete orphaned shaders: `gradient.wgsl`, `preview.wgsl`, `terrain.wgsl`, `craters.wgsl`
- [ ] Review and likely delete `noise_test.wgsl`
- [ ] Fix Plans.md Phase 6.3.1-6.3.3 status (mark as reverted)
- [ ] Fix Plans.md Phase 7 numbering collision

### Decision needed

- [ ] Export checkboxes: connect to pipeline OR remove from UI?
- [ ] HEALPix modules: keep for future work OR archive to branch?
- [ ] Phase 6.3.4-6.3.5: still planned (with HEALPix revival) OR cancelled?
- [ ] Stale plan documents: mark superseded OR delete?

### Documentation cleanup

- [ ] Mark superseded plans with `status: superseded` in frontmatter
- [ ] Add cross-references to 3 orphan brainstorms
- [ ] Update `deep-review-research-vs-implementation.md` if stale
