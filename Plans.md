# Planet Gen Plans.md

作成日: 2026-03-29
Requirements: [docs/brainstorms/2026-03-29-planet-gen-requirements.md](docs/brainstorms/2026-03-29-planet-gen-requirements.md)

---

## Completed Phases (archived)

| Phase | Summary | Tasks | Final commit |
|-------|---------|-------|-------------|
| 1 | Project scaffold & GPU hello world | 4/4 | [516a8d7] |
| 2 | Cube-sphere & noise generation | 4/4 | [6b2f515] |
| 3 | Planet physics & parameter derivation | 4/4 | [a061118] |
| 3.5 | Terrain & preview fixes | 5/5 | [de1d3f6] |
| 4 | Biome & surface generation | 9/9 | [c0df99d] |
| 4.5 | Research alignment fixes | 6/6 | [1112a72] |
| 4.6 | Physics-driven terrain & climate (Hadley, rain shadow, domain warp) | 4/4 | [d8d7bd3] |
| 4.7 | Visual control parameters (continental scale, water loss) | 4/4 | [d8d7bd3] |
| 4.8 | Tectonic plate-driven terrain (Voronoi, boundary classification) | 5/5 | [4c93ee5] |
| 5 | Tiled full-resolution generation & 8K EXR export | 7/7 | [d8d7bd3] |
| 5.5 | Preview interaction & visual enhancements (zoom, Mie scattering) | 9/9 | [c56a543] |
| 5.6 | Cloud layer (Schneider remap, Beer-Lambert, cyclone storms) | 9/9 | [da89ff5] |
| 5.7 | Starfield, city lights & star color | 5/5 | [a83332d] |
| 5.8 | Visual polish & layer toggle system (5/6 done) | 5/6 | [b48cd68] |
| 5.9 | Pure noise terrain rebuild | 5/5 | [6d96e4b] |
| 5.10 | Biome rendering refinement | 5/5 | [c2f23e1] |
| 5.11 | UI refactor & equirectangular export | 4/4 | [74339e7] |
| 5.12 | Multi-pass GPU plate terrain (JFA distance fields) | 10/10 | [d017196] |
| 5.13 | Wire continent controls to pipeline | 5/5 | [f0b2a06] |
| 5.14 | Terrain variety, 12-biome system, regional climate, ocean currents | 6/6 | [ba19f5f] |
| 8.5 | Performance (benchmark, progressive erosion, moisture-weighted) | 6/6 | [edb3904] |
| 6.0–6.3 | HEALPix orogen port | Archived to branch `archive/healpix-orogen` | [1aac311] reverted |
| 5.15 | Cloud layer overhaul (types, layers, storms, wind model) | 11/11 | [da89ff5] |
| 5.16 | Cloud & wind quality pass (coverage, detail, seasonal, föhn) | 8/8 | [b48cd68] |
| 5.17 | GPU cloud advection (semi-Lagrangian, source/sink) | 6/6 | [ae73bb0] |
| 5.18 | Pressure-based wind model (continentality, pressure, Coriolis) | 7/7 | [09637ba] |
| 5.19 | Climate model refinement (Kaspi-Showman, ExoPlaSim, monsoon) | 7/7 | [4e2d088] |
| 5.20 | Wind-shaped cloud system (streamline warp, continentality modulation) | 6/6 | [49051bd] |

**Total completed: ~165 tasks across 28 phases**

---

## Open tasks from completed phases

| Task | 内容 | Status |
|------|------|--------|
| 5.8.2 | Export cloud + night light layers as textures | cc:完了 [9ed788c] |
| 8a.7 | Performance + visual comparison: screenshot comparison and docs/research/ update | cc:完了 [9ed788c] |

---

## Phase 5.21: Unified Wind Pipeline

Replace dual wind models (analytical per-pixel + GPU cubemap) with a single cubemap wind source for all effects. Currently the GPU computes a pressure-derived wind field (~160ms) but only the continentality channel is used — the actual wind vectors are wasted. This phase wires the cubemap wind into moisture, clouds, and currents for physically consistent wind everywhere.

Plan: [docs/plans/2026-04-06-unified-wind-pipeline.md](docs/plans/2026-04-06-unified-wind-pipeline.md)

| Task | 内容 | DoD | Depends | Status |
|------|------|-----|---------|--------|
| 5.21.1 | `sample_wind_tangent(pos)` helper with auto-fallback | Helper compiles, cubemap when ON, analytical when OFF | Phase 5.20 | cc:完了 |
| 5.21.2 | Wire cubemap wind into moisture rain shadow | Rain shadows follow pressure wind | 5.21.1 | cc:完了 |
| 5.21.3 | Wire cubemap wind into orographic clouds | Mountain clouds follow pressure wind | 5.21.2 | cc:完了 |
| 5.21.4 | Wire cubemap wind into ocean currents (Ekman transport) | Currents use wind-derived east direction | 5.21.3 | cc:完了 |
| 5.21.5 | Merge debug views 14+18 into single Wind view | One view, cubemap when ON, analytical when OFF | 5.21.4 | cc:完了 |
| 5.21.6 | Remove CloudAdvectionPipeline + cloud_advect.wgsl, fix sweep.rs | Build succeeds, ~430 lines removed | 5.21.5 | cc:完了 |
| 5.21.7 | Build + test validation | 42 tests pass, runtime shader compiles | 5.21.6 | cc:完了 |

---

## Phase 5.22: Terrain-Aware Wind

Reintroduce continent/mountain effects on wind WITHOUT coastline ghosting. Uses the wobble mechanism (shift cell boundary latitude) instead of direct wind vector modification. Terrain influence flows through the existing `lat_deg + wobble` path — broad, gradual, no sharp edges.

Plan: [docs/plans/2026-04-07-terrain-aware-wind.md](docs/plans/2026-04-07-terrain-aware-wind.md)

| Task | 内容 | DoD | Depends | Status |
|------|------|-----|---------|--------|
| 5.22.1 | Continental wobble: `cont * 5° * season_factor` shifts cell boundaries poleward over continents in summer | Cell boundaries shift over large continents. No ghosting | Phase 5.21 | cc:完了 |
| 5.22.2 | Elevation wobble: `smooth_step(0.10, 0.25, elev) * 3°` for mountains >3km | Cell boundaries deflect around major ranges | 5.22.1 | cc:完了 |
| 5.22.3 | Mountain speed boost: `1 + smooth_step(0.08, 0.20, elev) * 0.3` | Faster wind near high terrain | 5.22.2 | cc:完了 |
| 5.22.4 | Build + test validation | 42 tests pass, runtime verified | 5.22.3 | cc:完了 |

---

## Phase 5.23: Shallow Volumetric Clouds

Plan: [docs/plans/2026-07-10-001-feat-shallow-volumetric-clouds-plan.md](docs/plans/2026-07-10-001-feat-shallow-volumetric-clouds-plan.md)

| Unit | 内容 | Status |
|------|------|--------|
| U8 | Dependency migration baseline | cc:完了 [929bfed] |
| U1 | Unified GPU presentation | cc:完了 [c201739] |
| U2 | GPU-resident dynamics textures | cc:完了 [929bfed] |
| U9 | Diagnostic cloud mass and geometry | cc:完了 [e239a13] (test support [6daff58]) |
| U10 | Atomic expanded weather fields | cc:完了 [e239a13] (test support [6daff58]) |
| U13 | Broad existing cloud-family density | cc:完了 [e239a13] (test support [6daff58]) |
| U12 | Activated moisture spin-up | cc:完了 [d9e9e29] (always-on bounded 128²/16-pass spin-up; transport diagnostic verified 1.6688% total drift and +0.0070 downwind condensate-centroid redistribution) |
| U14 | Marine forcing integrated into spin-up and regime diagnosis | cc:完了 [07f4e5d] (marine decks/trade cumulus, coast continuity, coverage response, and seams validated) |
| U15 | Weather wind-scale and convective/anvil organization | cc:完了 [e1b260d] (release-518 authoritative 512px shear `.10` production plume run passed all frozen Wind 1/2 ownership gates; L2/L1 `1.70325..2.36816`, B2/B1 `1.02871..1.06631`, S2/S1 `.84076..1.06981`.) |
| U3 | Shared shallow-volume density and ray marching | cc:完了 [e1b260d] (release-518 authoritative 512px local jitter run passed the low/deep isotropic identity and high-only local symmetric dominant-octave gates.) |
| U16 | Land-profile segment integration and approved `.990` topology bound | cc:完了 [d8c7de5] (QA-020 implemented and validated: GPU oracle, native, U3/U14/U15, lib, sweep, fmt, clippy, and build passed. `.990` is user-approved; all other gates are unchanged.) |
| U4 | Cloud lighting and surface shadows | cc:完了 [3c9266a] (QA-036 final validation complete: exact fingerprint and outside-sphere equality; candidate on/off `83.836/10.379 ms` against committed U16 `86.829/10.536 ms`, ratios `.9655/.9851`; visual criteria PASS. U14 `277.062/42.354 ms` and U15 seed-997 frozen fragmentation exact-match candidate vs detached HEAD, so they remain baseline blockers, not U4 regressions. See `docs/research/shallow-volumetric-cloud-validation.md`.) |
| U5 | Preview/export parity and channels | cc:完了 [21be76c] |
| U6 | Visual, seam, and parity validation | cc:完了 [33d55a2] |
| U11 | Performance, latency, and stress validation | cc:完了 [e0f611d] (direct EXR export; 2K stress gate PASS) |
| U7 | Remove superseded paths and document | cc:完了 [uncommitted] (verified superseded paths absent; accepted persistent-weather, bounded-volume, direct six-channel export architecture documented) |

### Deferred storm organization follow-up

| Task | 内容 | Status |
|------|------|--------|
| FE-034–047 | Clean-slate storm organization experiment | blocked/deferred; reverted to accepted checkpoint [0e2b044] |
| U15 size/anvil/organization gates | 15 (was 21 at FE-081; per-seed triage in the RV-003 section below) compliant-anvil / shear-driven-plume gate failures across the frozen seeds remain known-blocked after FE-081 de-classification | known-blocked; do not mark green. Diagnose with the per-seed cloud organization validation harness added in [da390a7] (`cargo run --release --features validation --bin sweep -- --weather-validation`) |

The experiment corrected its selection and harness, but the bounded organizer with conservative coupling did not form qualifying U15 deep cores. Safe parameter sweeps found no viable parameter-only path. The user chose to park the work and revert; retain the terrain-driven and marine/sea-cloud gains already present at `0e2b044`. Next priority: diagnose the polar-cap cloud behavior before reopening storm organization.

FE-081 (uncommitted on top of `3011d61`) removed marine_climate conversion inflation in the spin-up; the two affected U14 source-flow gates (windward_p90, land_p90) were rebaselined from the multi-seed distribution (median − 2σ over the 8 frozen seeds) rather than single-seed floors. The U15 size/anvil/organization failures listed above are pre-existing known-blockers, unrelated to FE-081.

### DS-046 weather structure (FE-083, uncommitted on top of `8837b21`)

| Task | 内容 | Status |
|------|------|--------|
| FE-083 #1 | Bounded local eddy stabilization κ∇²(state), κ=8×10⁶ m²/s (√(κ·25600s)≈450 km), explicit blend capped at 0.24 (bounded local extrema). The cubemap stencil is not area-weighted or proven conservative across seams. | cc:完了 [uncommitted] |
| FE-083 #2 | Mesoscale perturbation: Mode 2 pressure octaves pos×8/×24 ±2.5 hPa (streams 22/23) + Mode 3 wind steering octaves (streams 20/21) for trajectory curvature and interior reach | cc:完了 [uncommitted] |
| FE-083 #3 | Scaling asymmetry fixed by raising forcing exponent 0.3→0.85 (transport stays linear; wind_scale=1 unchanged baseline). Decision rationale: linear transport vs ^0.3 forcing caused max-wind interior drain and low-wind coastal mask; 0.85 closes most of the gap while keeping headroom against the raised convergence gain | cc:完了 [uncommitted] |
| FE-083 #4 | Convergence weight in lcl_lift raised 0.70→0.95 once mesoscale signal existed | cc:完了 [uncommitted] |
| FE-083 #5 | Interior thermal convection deployed after A1 still failed: hot-lowland uplift into lcl_lift (band relative to base_temp_c), strictly gated by inland_provenance; repartitions existing vapor only | cc:完了 [uncommitted] |
| DS-046 A1 | Re-specified per DS-046 ruling (2026-08-24): **A1a** coastline decoupling — coastline-correlation of largest connected cloud-free region < T_coast=0.068 at ws {0.25,2} (detector promoted from advisory; T_coast = 50% × FE-081-era baseline at 882b7cb: −0.068/+0.135) → PASSES ws=0.25 (−0.043), FAILS ws=2 (0.083 ≥ 0.068); **A1b** land cloud ≥25% at every tested ws → PASSES ws=1/2 (30.3/37.9%), FAILS ws=0.25 (17.0%); **A1c** raw cloud-free ≤35% sphere at ws=max backstop → PASSES (33.9%); legacy habitable-band raw-area metric kept as telemetry (42.3/32.7/24.0%) | partial; do not mark green |
| DS-046 A2 | High-pass (<~1500 km) condensate RMS ≥15% of total with multi-octave energy — PASSES: 93.3% high-pass share, 90.3% sub-octave share | cc:完了 [uncommitted] |
| DS-046 A3 | Along/cross autocorrelation ratio ≤3:1 at ws=2 — PASSES: ratio 1.22 (n=10800) | cc:完了 [uncommitted] |

Validation (`target/release/sweep --weather-validation --size 512 --output-dir target/val-032`): total gate failures **23 → 19** vs the pristine-tree baseline re-measured on the same machine (baseline log `/tmp/opencode/val-baseline.log`). All five `U15 seed {X} size` gates now pass; organization improved 1/8→2/8 (still failing); generation p95 35.7 ms ≤ 36 ms with diffusion cost included; seed-211 L2/L1 = 2.042 (in required 1.5–2.5); anvil/shear plume fixtures unchanged (they encode pre-FE-083 transport semantics — reported, not weakened). A1 residual analysis: deep interiors cannot receive vapor under frozen constraints (evaporation gated by water.local×fetch, NO_SOURCE exact zero, provenance-gated phase change, locked 20×1280 s horizon), and Hadley subtropical dry belts are modeled physics, not the continent-mask defect; the ≤10% largest-free threshold needs re-specification (e.g. exclude subsidence bands) before it can go green. Three legacy mass-fingerprint pins rebaselined per the documented protocol (clean build run twice, identical hashes). Known unrelated flakes: parallel `--bin sweep` test GPU-init SIGSEGV and the export midflight-cancellation race both reproduce on the pristine tree under load.

Re-validation after the DS-046 A1 re-specification (`target/release/sweep --weather-validation --size 512 --output-dir target/val-033`, log `target/val-033-run.log`): total gate failures **19 → 17**; generation p95 34.8 ms ≤ 36 ms. New A1 block contributes exactly 2 failures: A1a ws=2 coastline-correlation 0.083 vs T_coast 0.068 (baseline coupling 0.135 reduced 38%, short of the 50% bar) and A1b ws=0.25 land cloud 17.0%. A1c backstop and A2/A3 pass.

### FE-084 bounded land evapotranspiration (uncommitted on top of FE-083)

Doctrine change (user-directed, supersedes DS-046 strict marine-only SOURCING only; the ban on fake land cloud stamping stands): land may now mint real vapor through bounded evapotranspiration — soil-moisture + vegetation recycling, physically standard.

| Task | 内容 | Status |
|------|------|--------|
| FE-084 #1 | ET source term in q_target: LAND_ET_STRENGTH=0.27 (~40% of 0.68 peak ocean supply), scaled by coverage×moisture×smooth_step(6,22 °C) surface temperature; gated water.local==0 exactly. The source-owned, ocean-dominant tracer mints land share at ×0.3 (not marine-origin evidence once ET is active); RH-target blend extends toward marine regime on transpiring land; climate-referenced maturity gates (humidity_gate / inland_provenance / warm_surface_lift) | cc:完了 [uncommitted] |
| FE-084 #2 | A1a variants (land-gated, coastal-band, eddy-κ) were implemented and measured; global variants break A1c (marine deck cohesion collapses → largest raw clear region 33.9→58%), local variants are inert on the corr metric (dominated by open-ocean boundary geometry) → reverted to DS-046 baseline steering. wind_scale changes weather spin-up transport/forcing, not wind_field reconstruction. | reverted; see A1 note |
| FE-084 #3 | Exact-zero invariants verified: NO_SOURCE bit-zero (incl. new U14 cold-inland control via 6 °C ET floor), coverage/moisture=0 exact zero, dry/stable controls hold; three doctrine pins re-specified (land recharge bounded, land provenance >0 at reduced share, storm deep-mass bounded by local ET budget); all-ocean fingerprint pins untouched (ET inert on ocean cells) | cc:完了 [uncommitted] |
| FE-088 | User-approved 24×1280 s horizon experiment was run twice at `target/val-040/run-{1,2}` and reverted: ws=2 lower-hemisphere occupancy fell 0.28979→0.28289 and condensate mean 0.04215→0.04169 versus `val-035`; A1a/A1b still fail while A1c/A2/A3 pass; 18 total failures. Next minimal untested schedule increment: 22×1280 s (28,160 s). | reverted; do not rebaseline the 36 ms gate without a measured p95 breach |

Validation (`target/val-034`, log `target/val-034-run.log`): total gate failures **17 = 17** (≤17 requirement met). A1b improved 17.0→17.8% (still <25%); A1a unchanged 0.083 (still ≥0.068); A1c 33.9% ✓; A2 93.3%/90.3% ✓; A3 1.22 ✓; generation p95 32.9 ms ≤ 36 ms ✓; seed-211 L2/L1 = 2.15 ✓; U14 exact-zero restored after ET thermal floor. Structural finding: A1b ≥25% is unreachable under the ≤25%-strength cap because occupancy threshold 0.05 needs ~0.04 condensate while the ET ceiling at harness coverage=0.5 sustains ≤~0.02 within the pinned 20×1280 s schedule; and at ws=2 the A1a boundary-decoupling and A1c deck-cohesion gates trade off through total cloud cover — every mechanism that passes one fails the other. Resolution requires a user decision: raise the A1b occupancy threshold or extend the spinup schedule, and re-pair T_coast with true-ws-dynamics A1c margins.

`target/val-034` is the canonical latest performance evidence for this uncommitted series. The land source-owned scalar includes an unowned bootstrap, so its final ΣP/ΣT is not an observable exact 0.3 ratio; the focused tracer regression instead enforces the 0.3 mixing cap (with f16 headroom) and exact zero source ownership when coverage, moisture, or the temperature gate disables land ET.

### FE-086 review corrections (uncommitted)

| Task | 内容 | Status |
|------|------|--------|
| FE-086 | Corrected ET strength/provenance documentation, renamed U15 scalar semantics to source-owned/ocean-dominant, removed unsupported eddy-conservation and wind-field rebuild claims, and added source-cap/zero-gate tracer coverage. | cc:完了 [uncommitted] |

### RV-002 review fixes (uncommitted, post-FE-088)

Delegated read-only review (RV-001, rust-reviewer contract) of the uncommitted cloud rework; this section implements the agreed fix sequence. Per the user decision recorded at val-034: test the land ET ceiling rather than extend the spinup schedule (FE-088 already falsified that lever), and re-specify the A1a detector instead of loosening T_COAST.

| Task | 内容 | Status |
|------|------|--------|
| RV-002 #1 | P0: ft_backtrace now scales by clamp(wind_scale, 0.0, 2.0) like every other reservoir (weather_spinup.wgsl) — the FT/BL partition no longer depends on wind_scale in an undocumented way. Bit-exact at ws=1 (fingerprint values unchanged with/without this edit). | cc:完了 [uncommitted] |
| RV-002 #2 | A1a detector re-specification: Ds046Grid.subsidence mask (tilt-corrected \|lat\| 20..=35°, same latitude as the warm mask); ds046_largest_free_region gained exclude_subsidence — A1a excludes Hadley subsidence belts from free-region growth (modeled physics, not a coastline-tracing defect); A1c keeps zero exclusions. Measured against unchanged T_COAST=0.068: ws=0.25 −0.039→0.038 ✓; ws=2 0.084→0.040 ✓. No recalibration needed. | cc:完了 [uncommitted] |
| RV-002 #3 | A1b ET-ceiling experiment (validation runs 3–5): LAND_ET_STRENGTH raised to 0.35/0.45/0.68 — every A-metric and the all-land doctrine value byte-identical at each value; the strength constant is not the binding limit for A1b (land q_target stays far below q_sat: sub-saturated ET vapor cannot condense without convergence). Reverted to 0.27. Resolved by RV-002 #6 (gate re-specification, measured green). | cc:完了 [uncommitted] |
| RV-002 #4 | Pin maintenance: three mass-fingerprint pins were stale against the uncommitted FE-084/FE-086 tree BEFORE this work (proven pre-existing by a revert experiment — identical hashes with/without the RV-002 edits): all-ocean 1378261711573443365→7702921282530312997, provenance legacy 16396317793306051365→4130945357148873509, provenance off 17385149919850611493→2621374564975805221 (weather.rs). Values verified identical across repeated deterministic release builds. | cc:完了 [uncommitted] |
| RV-002 #5 | Flagged, not re-pinned: preview::tests::cloud_land_segment_profile_preserves_ocean_and_repairs_tall_low_shells. Attribution complete: fails identically on pristine 8837b21 (worktree run) — predates the uncommitted rework; no file in the committed rework touches its code path. Measured ratio 1.0417 vs [0.98,1.02] (compact=0.02659, tall=0.02770). Behavior origin: d8c7de5 "Fix low-cloud sampling over land" (segment-mean low-cloud integration in the preview-only land-segment path); 882b7cb "Remove static masks from clouds" added the NOTE documenting the resulting preview/export divergence as "Left as-is for leader/designer decision". A/B experiment (land-segment blend disabled, throwaway worktree at e6f38e7): the test STILL fails with the land-segment path neutralized — the tall/compact drift is in shared sampling (weather_cloud_sample), NOT the preview-only land feature. Render visibility of the documented preview/export divergence: density maps differ by PSNR 27–31 dB / SSIM 0.89–0.94 across the 8 validation seeds (target/wt-ab-run vs target/val-rv003-run8) — clearly visible, not sub-perceptual; export lags the preview fix from d8c7de5. User decision (2026-09-03): (a) re-spec executed — band re-specified to [0.95, 1.06] around the measured 1.0417 with documented ruling in preview.rs (test passes); (b) port approved as FE-089 below. | cc:完了 [dc278ee] |
| RV-002 #6 | A1b gate re-specification (user-approved option 1): first tried a fetch restriction (warm land within N texels of a coastline) — run-6 profile showed the occupied fraction FLAT with coastal distance at ws=0.25 (≤2: 20.7% → all: 17.5%): low-wind land cloud is local physics (stratiform decks + orographic condensation), not fetch-delivered, so no radius can reach 25%. Final re-spec: full 0.25 target at ws ≥ 1.0; floor DS046_A1B_T_LOW_WIND=0.15 at ws=0.25 (catches structural collapse of land cloud without assuming the pinned horizon delivers marine vapor at low wind). Measured green: 17.5% / 30.8% / 37.0% vs 15% / 25% / 25%. | cc:完了 [uncommitted] |

Validation (target/val-rv002-run7, log target/val-rv002-run7.log): total gate failures **17** = 14 U15 anvil/shear fixtures (pre-existing, reported-not-weakened) + organization 2/8 (pre-existing) + 2 performance gates; logic-gate failures are therefore **15 ≤ 17**. ALL DS-046 A gates green: A1a both ws ✓, A1b all ws ✓ (re-specified, #6), A1c 33.9% ✓, A2/A3 unchanged ✓; lib suite 178 passed / 1 failed (only the flagged preview test, #5) / 3 ignored, ×2 identical per protocol; bin ET-share test green. Note: GPU generation p95 (492–517 ms vs the 36 ms gate) and U3 render p95 (~5.5 s) failed in every RV-002 run — sustained external load on this box (load average ~18; val-034 measured 32.9 ms here); re-run in a quiet window for performance evidence.

### RV-003 wind_scale cap raised to 4.0 (uncommitted)

User request: allow wind beyond 2.0 so clouds can be carried farther inland ("clouds still end up mid-continent"). The [0,2] cap was enforced in three places — the shader transport displacement (transport_substeps, weather_spinup.wgsl), the ft_backtrace displacement (RV-002 P0 line), and the Rust CFL mirror (outgoing_cfl_with_interval, weather.rs) — plus the UI slider. All four raised to 4.0 in lockstep. The substep scheduler derives its count from the same clamped displacement, so the per-substep step stays within MAX_SUBSTEP_TEXELS at any wind (no stability change; ws=4 simply doubles the substeps of ws=2). No test scene uses wind_scale > 2.0, so every pin and gate is bit-exact — verified: lib suite 178 passed / 1 failed (only the flagged preview test), identical to before the change.

| Task | 内容 | Status |
|------|------|--------|
| RV-003 #1 | Transport + ft_backtrace displacement cap raised 2.0 → 4.0 (weather_spinup.wgsl), Rust CFL mirror (weather.rs) and UI slider (app.rs) in lockstep; bit-exact for wind_scale ≤ 2.0. | cc:完了 [uncommitted] |
| RV-003 #2 | ws=4.0 added to the DS-046 A-metrics sweep as a telemetry-only point (no gates above 2.0), with equirect cloud-map dumps at ws=2 and ws=4 (ds046_cloud_map_ws{2,4}.ppm + PNG). Measured: land_cloud(warm) 37.0% → **51.9%** (+14.9 pt — vapor now crosses full continents), coast_corr 0.040 → 0.017 (coastline-tracing defect essentially gone at extreme wind), largest_free(habitable) 5.4% → 6.2%. Gate set unchanged: total failures 17 = pre-existing + perf load artifacts. | cc:完了 [uncommitted] |
| RV-003 #3 | ws=4 gate re-spec (user-approved): DS-046 validated range extended [0.25, 2.0] → [0.25, 4.0] — A1a now gated at {0.25, 4.0}, A1b target applied at all four points, A1c backstop moved ws=2 → ws=4. First measurement (target/val-ws4-gate-run.log): A1c raw free region **34.1% ≤ 35%** ✓ (thin margin — 0.9 pt), A1b@4 51.9% ≥ 25% ✓, A1a@4 coast_corr 0.017 < 0.068 ✓; total failures still 15 = parked U15 only, zero new. No threshold changed — pure range extension locking in the RV-003 #2 telemetry values as gates. | cc:完了 [2da39f4] |

Validation (target/val-rv003-run8, log target/val-rv003-run8.log): all gates at ws ≤ 2 identical to run 7; ws=4 telemetry as above. Cloud maps: target/val-rv003-run8/ws2.png / ws4.png (equirect, land warm-tinted, ocean blue; condensate low+deep normalized). Note: the forcing term (wind_scale^0.85) and lift terms already responded super-linearly above 2 before this change — only transport was frozen at 2; RV-003 makes transport consistent with them. RV-003 #3 validation (target/val-ws4-gate-run.log): extended gate range [0.25, 4.0] green on first measurement — A1c@4 34.1% ≤ 35%, A1b@4 51.9% ≥ 25%, A1a@4 0.017 < 0.068; parked U15 failures unchanged (15).

U15 triage (measured, run 8): the 15 failures decompose into three distinct modes across 8 frozen seeds plus the organization gate. (a) Elongation collapse L2/L1 < 1.5 — ws=2 plumes barely longer than ws=1: seeds 19 (1.09), 37 (1.09), 73 (1.33), 101 (1.03). (b) Sharpness collapse S2/S1 < 0.75 — ws=2 plumes blunter than expected: seeds 7 (0.59), 37 (0.67), 73 (0.74, marginal), 211 (0.68), 997 (0.63). (c) Axis misalignment — seed 509 scale-1 plume axis 59.8° from the wind (>30° gate). Organization: 2/8 seeds qualify (known since the FE-034–047 experiment was parked with "no viable parameter-only path"). Physical reading: the land-ET/RH-blend rework — the same change that fixed A1b — adds distributed moisture along transport paths, and sub-linear forcing (ws^0.85) reduces the relative ws=2 boost; together they weaken the wind-speed dependence of anvil morphology, so stronger wind no longer stretches/sharpens plumes proportionally. Status: kept known-blocked pending user decision — recalibrating the ratio bands would gut the gates' meaning (they encode "stronger wind = longer, sharper anvils"), while fixing the physics risks regressing A1b/A1c, which depend on the same inland-moisture mechanism.

### FE-089 export land-segment parity (uncommitted)

User-approved follow-up to RV-002 #5: close the documented preview/export divergence by porting d8c7de5's segment-mean low-cloud integration into cloud_export.wgsl. Requirements: docs/brainstorms/2026-09-03-export-land-segment-parity-requirements.md; plan: docs/plans/2026-09-03-001-feat-export-land-segment-parity-plan.md.

| Task | 内容 | Status |
|------|------|--------|
| FE-089 #1 | cloud_export.wgsl main() radial march now calls the shared weather_cloud_layers_land_segment with radial band-edge segments (segment_start/end = direction × (1 + h/radius_km)); ocean texels bit-exact via the land gate early return; U5 test updated to assert export uses the shared land-segment path (the old weather_cloud_sample assertion pointed at the entry point that no longer appears in the file). | cc:完了 [dc278ee] |
| FE-089 #2 | Validation: full lib suite green — 179 passed / 0 failed / 3 ignored (target/val-fe089-lib2.log), zero pin churn so no re-baselining needed; sweep at exactly 17 failures = 15 parked U15 + 2 environmental perf gates, no new failures (target/val-fe089-run.log). | cc:完了 [dc278ee] |

### Perf-gate ignore switch (c50c7b8)

User-approved: allow ignoring the environmental performance gates to continue work. `PLANET_GEN_IGNORE_PERF_GATES=1` downgrades the three queue p95 checks (generation, render, U3 render fixture) from fatal to reported-only; correctness gates — including parked U15 — are never ignored, and perf stays fatal by default. Verified: target/val-perf-ignore-run.log — 2 perf failures listed as ignored, 15 U15 failures remain fatal (exit 101). Note the environmental load has worsened since val-034 (generation min 664 ms vs 410 ms; U3 p95 10.4 s) — real perf evidence still needs a quiet-window run in the author's normal session.

### FE-090 vegetation → weather feedback (S1 implemented)

User-approved scope: S1 inline per-texel vegetation proxy (the requirements doc's recommended first step). Land ET has no vegetation state today — et_capacity is global-coverage × global-moisture × temperature window, so a Saharan and an Amazonian cell at the same temperature contribute identical ET. Requirements: docs/brainstorms/2026-09-04-vegetation-weather-feedback-requirements.md; plan: docs/plans/2026-09-04-001-feat-vegetation-weather-feedback-plan.md (FE-090, completed).

| Task | 内容 | Status |
|------|------|--------|
| FE-090 #1 | S1 vegetation proxy + et_capacity multiply in weather_spinup.wgsl: veg_moisture = (1 − rain_shadow·0.6) × mix(0.10, 1.0, marine), treeline cap smooth_step(2.2, 3.4 km), ice zero; calm-wet mask inherits via et_capacity | cc:完了 [7a7ea42] |
| FE-090 #2 | Pins: ZERO churn — all three fingerprint fixtures are all-ocean where et_capacity is exactly 0 (water.local gate); bit-exact by construction, verified green ×2. Ruling documented in the plan's Findings. | cc:完了 [7a7ea42] |
| FE-090 #3 | Lib suite green ×2: 179 passed / 0 failed / 3 ignored (target/val-fe090-lib.log, val-fe090-lib3.log). One run-2 failure was an unrelated pure-CPU export-staging race under concurrent load (rows_completed 1025 vs 1024); passed in isolation and on the clean rerun. | cc:完了 [7a7ea42] |
| FE-090 #4 | DS-046 re-measured (target/val-fe090-run): A metrics IDENTICAL to baseline at reported precision — A1a 0.038/0.056/0.040 vs T_COAST 0.068, A1b 17.5/30.8/37.0% ≥ targets, A1c 33.9% ≤ 35%, A2/A3 unchanged → no threshold re-specification needed | cc:完了 [7a7ea42] |
| FE-090 #5 | U15 per-seed deltas (parked, NOT re-baselined): plume response_p95 scale1 +7.5…+13.4%, scale2 +25.8…+31.4% across all 8 seeds; failing set unchanged (same 15). Pathway: et_capacity feeds calm_wet_land_mask (FE-084) — sparse vegetation weakens the stratiform regime and strengthens the convective catalyst; direction physically consistent (dry surfaces → more deep convection) | cc:完了 [7a7ea42] |
| FE-090 #6 | A/B vs baseline target/val-perf-ignore-run: all 16 U14 fixture density images bit-identical; DS-046 equirect cloud maps sub-pixel drift only (ws2: 45/32768 px, max Δ2/255; ws4: 53/32768 px, max Δ1/255) — below perceptual threshold | cc:完了 [7a7ea42] |
| FE-090 #7 | Plans.md + plan doc stamped (status: completed) | cc:完了 |

Validation (target/val-fe090-run.log, PLANET_GEN_IGNORE_PERF_GATES=1): total gate failures 15 = the same parked U15 set (zero new, zero resolved) + 2 perf gates reported-ignored. Interpretation (plan Findings): in the DS-046 scene ET source magnitude is not the binding limit for land-cloud formation (RV-002 #3: 2.5× LAND_ET_STRENGTH left every A-metric byte-identical — land q_target ≪ q_sat, condensation needs convergence regardless), so S1's scene-scale cloud signature is sub-perceptual there; its measured effect is convective-response modulation (U15 deltas above). User ruling (2026-09-04): **accepted as-is** — the proxy is physically correct plumbing and the sub-perceptual scene-scale signature is documented behavior, not a defect; amplification or deferral remains available if a later phase-change change makes ET rate-limiting.

---

## Archived: Terrain Diffusion and Imported Terrain

Terrain Diffusion is dropped as a whole-planet product path: the procedural control remains `NO-GO`, native export is `NO-GO`, and TorchScript replay is `NO-GO`. The committed evaluator, spike plans, and research manifests are retained as negative evidence; no Terrain Diffusion or imported-terrain implementation is active. See [the procedural-terrain plan](docs/plans/2026-07-28-003-feat-procedural-terrain-realism-plan.md), [evaluation manifest](docs/research/terrain-diffusion-evaluation-manifest.md), [native manifest](docs/research/terrain-diffusion-native-spike-manifest.md), and [LibTorch manifest](docs/research/terrain-diffusion-libtorch-spike-manifest.md).

Archived evaluation plan: [docs/plans/2026-07-12-001-feat-terrain-diffusion-evaluation-plan.md](docs/plans/2026-07-12-001-feat-terrain-diffusion-evaluation-plan.md)

| Unit | 内容 | Status |
|------|------|--------|
| U1-U4 | Evaluation-only artifact harness | Archived / not planned; local evidence remains `NO-GO` / `NOT_RUN` as recorded. |

### Archived imported-terrain path

The former import path is also retired. Its infrastructure is retained only as historic evidence; approved candidate is NOT AVAILABLE and product activation/readiness are NOT RUN. Approval rejects `KNOWN_NO_GO_CONTROL_FNV=243e1887675e77a8` and `KNOWN_NO_GO_VALIDATOR_COMMIT=0cba9579e68f0fc72a75d21db2d655496ef76d09`.

Plan: [docs/plans/2026-07-26-001-feat-approved-terrain-artifact-import-plan.md](docs/plans/2026-07-26-001-feat-approved-terrain-artifact-import-plan.md)

| Unit | 内容 | Status |
|------|------|--------|
| U1-U4 | Retained import infrastructure | Archived [a1fcf3f7e60a097a83d241fd7b9a9c46e99aa7ac]; no product activation. |
| U5 | Real candidate activation evidence | Archived / NOT RUN; no activation is planned. |

---

## Phase 7: Blender Importer Addon

Pure-Python Blender addon that imports generated textures and sets up materials.

| Task | 内容 | DoD | Depends | Status |
|------|------|-----|---------|--------|
| 7.1 | Addon skeleton: Blender addon with `bl_info`, register/unregister, sidebar panel in 3D Viewport | Addon installs in Blender 4.x, panel appears in N-panel | - | cc:完了 [53eaff9] |
| 7.2 | "Import Planet" operator: file browser to select planet output directory, load all texture files as Image datablocks | Test: all texture files load into Blender's Image Editor | 7.1 | cc:完了 [53eaff9] |
| 7.3 | Material builder: create Principled BSDF node tree, wire albedo→Base Color, normal→Normal Map→Normal, roughness→Roughness, height→Displacement | Test: material node tree is correctly wired; render shows textured planet | 7.2 | cc:完了 [53eaff9] |
| 7.4 | "Create Planet" mode: generate a UV sphere/icosphere with cube-projection UVs, apply material | Test: one-click produces a textured sphere in the scene | 7.3 | cc:完了 [53eaff9] |
| 7.5 | "Apply to Selected" mode: apply material to user's selected mesh object | Test: selecting an existing sphere and clicking "Apply" textures it correctly | 7.3 | cc:完了 [53eaff9] |
| 7.6 | Cycles + EEVEE compatibility: material works in both render engines (Displacement node setup differs) | Test: render in both Cycles and EEVEE produces correct results | 7.3 | cc:完了 [53eaff9] |

---

## Phase 8: Advanced Visual Features

| Task | 内容 | DoD | Depends | Status |
|------|------|-----|---------|--------|
| 8.1 | Lava glow along plate boundaries: volcanic emission at tectonic faults, tectonic activity slider | Orange-red glow at convergent/divergent boundaries, configurable intensity | Phase 4.8 | cc:完了 [eff66f0] |
| 8.2 | Lens flare near planet limb: procedural flare when sun is near the edge | Cinematic lens flare effect, subtle and adjustable | Phase 5.7 | cc:完了 [eff66f0] |
| 8.3 | Ocean specular / sun glint: bright reflection on water surface toward sun | Visible sun glint on oceans, PBR-correct | Phase 4 | cc:完了 [eff66f0] |
| 8.4 | Ring system: Saturn-like rings with color gradients, transparency, shadow casting on planet | Configurable ring tilt, inner/outer radius, color gradient, planet shadow on rings | Phase 5 | cc:完了 [eff66f0] |
| 8.5 | Ring export: single pixel width gradient texture (at least 4K) for Blender use | Exported 4K+ gradient PNG with transparency for ring shader | 8.4, Phase 7 | cc:完了 [eff66f0] |

---

## Phase 9: Advanced Tectonics

Three-tier tectonic simulation with UI toggle between modes.

### Phase 9a: Better Boundary Physics

| Task | 内容 | DoD | Depends | Status |
|------|------|-----|---------|--------|
| 9a.1 | Research: survey tectonic plate simulation techniques | Research doc in docs/research/ | - | cc:TODO |

(FE-054: local Euler plate velocities, boundary classification, subduction/rift terrain, and perf benchmark verified; cc:完了)

### Phase 9b: Plate Motion Simulation (continental drift)

| Task | 内容 | DoD | Depends | Status |
|------|------|-----|---------|--------|
| 9b.1 | Research: plate motion algorithms (Euler poles, velocity fields, collision detection) | Research doc | Phase 9a | cc:TODO |
| 9b.2 | Plate velocity field: motion vectors per plate, relative velocities at boundaries | Velocity vectors in Plates debug view | 9b.1 | cc:TODO |
| 9b.3 | Time-stepping: N geological timesteps, accumulate collision/rift history | Geological age slider | 9b.2 | cc:TODO |
| 9b.4 | Collision history → terrain | Older planets = more complex terrain | 9b.3 | cc:TODO |
| 9b.5 | Continental assembly/breakup | Supercontinents form and break with age | 9b.4 | cc:TODO |

### Phase 9c: Mantle Convection (future goal)

| Task | 内容 | DoD | Depends | Status |
|------|------|-----|---------|--------|
| 9c.1 | Research: simplified mantle convection (Rayleigh-Bénard on sphere) | Feasibility doc | Phase 9b | cc:TODO |
| 9c.2 | Convection cell simulation | Pattern visible in debug view | 9c.1 | cc:TODO |
| 9c.3 | Derive plate boundaries from convection | Plates emerge from convection | 9c.2 | cc:TODO |
| 9c.4 | Full pipeline integration | End-to-end convection-driven generation | 9c.3 | cc:TODO |

---

## Phase 10: Polish & Distribution

| Task | 内容 | DoD | Depends | Status |
|------|------|-----|---------|--------|
| 10.1 | Error handling: GPU OOM/device lost → UI error message | App doesn't crash on GPU errors | Phase 5 | cc:完了 [155a63f] |
| 10.2 | Cross-platform CI: GitHub Actions for Linux, macOS, Windows | CI green on all 3 | Phase 5 | cc:完了 [155a63f] |
| 10.3 | README: install, usage guide, parameter reference, example renders | Full documentation | 10.2 | cc:完了 [155a63f] |
| 10.4 | Blender addon packaging: zip + install instructions | Install via Preferences | Phase 7 | cc:完了 [155a63f] |

---

## Recorded Amendments

- 2026-07-19: U14's historical `-10°C` cool-marine dominance fixture is superseded because it represents pack ice under the Earth model. The `+5°C` matched ocean/inland fixture tests open-water stratocumulus; the geographic polar/pack-ice gate and production persistent-ice settings (`-15..-6°C`, supply suppression `.25`, phase penalty `.15`) remain unchanged.
