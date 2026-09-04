# U15 Triage Handoff (2026-09-04)

Purpose: a fresh session picks up the **U15 triage** — the 15 parked known-blocked
gate failures in `sweep --weather-validation` — and drives them to a decision +
execution. This doc is self-contained; it supersedes the older "U15 triage"
paragraph in Plans.md (RV-003 section) where they conflict.

## 1. Current state

HEAD: `61eecc1`. Working tree clean except user-untracked research files.
Latest full validation run: `target/val-fe090-run.log` (post-FE-090, post-RV-003 #3).
Total gate failures: **15** = 6 "compliant anvil response" lines + 8 "shear-driven
plume fixture" lines + "qualified storm organization 2/8". Both per-seed line types
report on the *same* underlying plumes (the anvil-response line is a superset with
size/anvil telemetry; the shear line is the compact summary) — treat them as one
failure per seed plus the organization gate.

### Measured sub-gate decomposition (val-fe090-run.log, current)

Gate logic: `src/bin/sweep.rs` ~L5538-5548 (`plume_pass`) — per plume:
`response_p95 >= 0.04`, `effective_n >= 32`, **`axis_wind_degrees <= 30`**,
`outer_support <= 0.05`, `boundary_shell <= 0.01`, `outside_corridor <= 0.05`;
plus ratios `L2/L1 ∈ [1.50, 2.50]`, `B2/B1 ∈ [0.80, 1.25]`, `S2/S1 ∈ [0.75, 1.25]`,
deterministic. Organization (per seed, ~L5649-5660): `outside_high >= 0.10`,
`downwind_centroid >= 0.5 texels`, `pca <= 20°`.

| seed | L2/L1 (≥1.5) | S2/S1 (≥0.75) | plume1 axis (≤30°) | failing sub-gates |
|------|--------------|---------------|--------------------|-------------------|
| 7    | 1.865 ✓      | **0.735 ✗**   | **62.7 ✗**         | sharpness (marginal), axis |
| 19   | **1.286 ✗**  | 1.167 ✓       | **58.7 ✗**         | elongation, axis |
| 37   | **1.475 ✗**  | 0.783 ✓       | **35.9 ✗**         | elongation (marginal), axis |
| 73   | **1.479 ✗**  | 0.860 ✓       | **45.4 ✗**         | elongation (marginal), axis |
| 101  | **1.257 ✗**  | 0.936 ✓       | 22.5 ✓             | elongation |
| 211  | 2.049 ✓      | 0.818 ✓       | **69.0 ✗**         | axis only |
| 509  | 1.825 ✓      | 0.861 ✓       | ~59.8 ✗            | axis only |
| 997  | 1.936 ✓      | 0.791 ✓       | ~63 ✗              | axis only |

**Correction to the older Plans.md triage (run 8):** it listed one mode per seed and
under-reported the axis gate. The per-plume `axis_wind_degrees <= 30` check applies
to BOTH plumes (`plume.iter().all(...)`), so scale-1 (ws=1) plume misalignment is a
**systemic, separate failure mode**: 7 of 8 seeds fail it (all except 101). FE-090
(vegetation proxy, commit `7a7ea42`) incidentally fixed sharpness on 4 of 5 seeds
(run 8: 0.59/0.67/0.74/0.68/0.63 → now only seed 7 at 0.735 fails) and improved all
elongation values (19: 1.09→1.286, 37: 1.09→1.475, 73: 1.33→1.479, 101: 1.03→1.257),
but the failure set stayed 15 because axis carries most seeds.

### Three distinct physical problems

1. **Axis misalignment (systemic, 7/8 seeds):** ws=1 plume response mass is oriented
   ~36–69° off the imposed wind. The fixture's controlled exterior + mesoscale
   steering in the spinup bends weak-wind plumes. This is a *direction* problem, not
   a wind-speed-scaling problem.
2. **Elongation collapse (4 seeds):** ws=2 plumes barely longer than ws=1 (L2/L1 < 1.5).
   Cause per the run-8 analysis: the FE-084/FE-086 land-ET/RH-blend rework adds
   distributed moisture along transport paths, and sub-linear forcing
   (`forcing_scale = pow(wind_scale, 0.85)`, DS-046 #3) reduces the relative ws=2
   boost — stronger wind no longer stretches plumes proportionally.
3. **Sharpness collapse (now only seed 7, marginal):** mostly healed by FE-090's
   convective-catalyst pathway (see coupling map below).

## 2. The decision fork

- **Option A — recalibrate the bands** to current measurements. Rejected in advance
  by the user: the ratios encode "stronger wind = longer, sharper anvils"; fitting
  them to a regime where that relationship is broken converts a physics check into a
  tautology. Do not propose this without new evidence.
- **Option B — fix the physics** so ws=2 plumes are genuinely longer/sharper and
  weak-wind plumes align with the wind. Candidate levers (none proven yet; each must
  be measured against the coupling map in §5):
  - Forcing exponent 0.85 → higher (restores ws=2 stretch) — but 0.85 was chosen in
    DS-046 #3 to close the transport/lift gap at ws=2; changing it reopens that.
  - Mesoscale steering / pressure-field curvature in `advance_state` (the likely
    axis-misalignment driver for weak wind).
  - Storm-catalyst wind dependence (`storm_catalyst = convective_catalyst *
    (1 - stratiform_regime)`; FE-084/FE-090 now modulate it per-texel).
  - Anvil partitioning in the phase-change block.
- **Option C — hybrid / gate surgery** (needs explicit user ruling): e.g. split the
  axis gate from the morphology ratios, demote organization to telemetry, or
  re-specify per sub-gate with documented rulings (RV-002 #6 protocol: measure
  first, then set). Only acceptable pieces: demoting *telemetry* items or narrowing
  a gate's scope with a physical rationale — never loosening a band to fit.

Recommended posture for the new session: **investigate Option B levers with small
measured experiments first** (axis is the biggest win — fixing it clears seeds
211/509/997 outright and helps 7/19/37/73); if no lever preserves the A-gates,
bring a per-sub-gate Option C proposal with numbers for the user to rule on.

## 3. Repo mechanics (read before running anything)

- **Every shell command is prefixed `rtk`** (token optimizer; passes through if it
  has no filter). If `rtk` is not on PATH, use `/home/ra100/.local/bin/rtk`.
  For debugging a crash, drop the prefix.
- Build: `rtk cargo build --release --features validation --bin sweep`
- Validate (perf is PARKED — always use the env var):
  `PLANET_GEN_IGNORE_PERF_GATES=1 rtk target/release/sweep --weather-validation --size 512 --output-dir <dir>`
  Exit 101 while gates are red. Perf p95 failures print as "ignored via
  PLANET_GEN_IGNORE_PERF_GATES=1" and are non-fatal (user ruling 2026-09-04;
  switch implemented in `c50c7b8`).
- Lib suite: `rtk cargo test --release --features validation --lib` (~3.5 min,
  currently 179 passed / 0 failed / 3 ignored).
- Sweep bin unit tests (`cargo test --bin sweep`): **SIGSEGV under parallel
  execution — pre-existing** (reproduces on pristine tree; GPU-driver/concurrency
  flake on this loaded box). Run with `-- --test-threads=1` if needed (all 52 pass
  serially, ~30 s).
- Pin protocol: clean build, two runs, identical hashes, documented ruling in the
  test comment. Current pins (`src/weather.rs` ~L2531/2535/2650) are all-ocean
  fixtures — land-only spinup changes are bit-exact against them (FE-090 proved
  this: zero churn).
- Gate philosophy: **never force a gate green.** Measure first; re-specify only
  with a documented ruling; report environmental/known-blocked items instead of
  hiding them.
- Workflow after implementation work: update `Plans.md` (`cc:完了 [commit_hash]`),
  plan docs in `docs/plans/` (frontmatter: title/type/status/date/origin),
  requirements in `docs/brainstorms/`, gitmoji commit prefixes (✨ feature, 🐛 fix,
  ♻️ refactor, 📋 plan, 📝 docs).
- **NEVER commit the user's untracked files:** `.impeccable/`, `.tmp-research/`,
  `DESIGN.md`, `PRODUCT.md`, `opencode.json`, `planet_heightmap_generation/`,
  `reference/*.jpg`.
- Image tooling: `read_image` may be unavailable (model-dependent) — describe
  numerically. PPM→PNG: `ffmpeg -y -loglevel error -i in.ppm out.png`. Image diff:
  `ffmpeg -loglevel info -i a -i b -lavfi psnr -f null -` (grep "average:") and
  `-lavfi ssim` (grep "All:"). magick AppImage is broken (FUSE).

## 4. Key file map

- `src/bin/sweep.rs` (~10k lines): U15 lives here.
  - `U15_VALIDATION_SEEDS` L20: `[7, 19, 37, 73, 101, 211, 509, 997]`
  - Fixture constants ~L4557+: `U15_TRAIL_SOURCE = [0.52268726, 0.0, 0.8525245]`,
    source radius 0.10, shore/shape/support/boundary-shell radii nearby.
  - `run_u15_field_validation` (entry; called from the weather-validation flow)
  - Per-seed row + `plume_pass` ~L5538-5702; organization gate ~L5649-5660
  - Fixture spec string (repro command with all gate values) ~L5803:
    shear wind `((1+shear*tanh(y/.08))/(1+shear))*cross(Y,p)`, tangent-divergence-
    free, speed bound [·,1], snapshot.wind_scale = 1 and 2, source=ocean_patch,
    control=matched_exterior_land_and_continentality, coverage:1 moisture:1
    temp_c:15 pressure:1013 tilt:0 season:.5 storms:0.
  - DS-046 A-gates ~L6419-6905 (constants L6439 T_COAST=0.068, L6450
    A1B_T_LOW_WIND=0.15); gate range now [0.25, 4.0] (RV-003 #3).
- `src/shaders/weather_spinup.wgsl`: spinup physics. `advance_state` (~L747):
  `forcing_scale = pow(wind_scale, 0.85)` (DS-046 #3 comment), transport +
  ft_backtrace caps (RV-002/003), `calm_wet_land_mask` (FE-084/FE-090),
  `storm_catalyst`, phase-change block, FE-090 vegetation proxy at the et_capacity
  site (~L855-881).
- `src/weather.rs`: mass-fingerprint pins + CFL mirror (`outgoing_cfl_with_interval`).
- Logs: `target/val-fe090-run.log` (latest per-seed numbers),
  `target/val-rv003-run8.log` (run 8 with the original triage measurements).

## 5. Coupling / risk map (what moving what breaks)

All DS-046 A-gates are currently GREEN and must stay green:
- A1a coast_corr < 0.068 at ws ∈ {0.25, 4.0}: 0.038 / 0.017.
- A1b land_cloud(warm) ≥ target (0.15 floor ws<0.75, else 0.25) at all four points:
  17.5% / 30.8% / 37.0% / 51.9%. **Depends on the FE-084 RH blend + inland moisture
  mechanism — the same mechanism that causes the U15 elongation collapse.** Any
  lever that strengthens ws=2 stretch via inland moisture risks A1b/A1c.
- A1c raw free region ≤ 35% at ws=4: **34.1% — thin margin (0.9 pt).** Anything
  that thins high-wind coverage hits this first.
- FE-090 vegetation proxy multiplies et_capacity and feeds calm_wet_land_mask →
  convective catalyst. It moved U15 responses +7–31% and healed most sharpness
  failures — do not revert it; build on it.
- Pins: land-only changes are bit-exact (all-ocean fixtures); a change that touches
  ocean cells or the all-ocean path WILL move pins → re-baseline per protocol.

## 6. Recommended first steps

1. Fresh validation run (perf-ignore env var) to confirm the §1 table reproduces on
   current HEAD; save log under `target/`.
2. Per-seed attribution pass: for each failing seed, list exactly which sub-gates
   fail (the §1 table is a start; verify p95/Neff/corridor/shell from the full log
   lines) and read the fixture geometry around the plume (field views are dumped as
   `u15_seed_*` images in the output dir when persistence is on).
3. Axis investigation (biggest win): instrument or worktree-test what bends ws=1
   plumes — candidates: mesoscale steering terms, pressure-field curvature, the
   controlled-exterior land/continentality match, transport vs lift speed mismatch
   at low wind. Small experiments, measure A-gate deltas each time.
4. Elongation investigation: forcing-exponent and catalyst levers; check A1b/A1c
   sensitivity (A1c margin is 0.9 pt).
5. Write requirements (`docs/brainstorms/`) → plan (`docs/plans/`, FE number at
   plan creation) → implement → validate → stamp Plans.md. If the outcome is a
   gate-surgery proposal instead of a physics fix, bring per-sub-gate numbers for
   the user's ruling; do not ship band changes unilaterally.

## 7. Recent commit chain (context)

`c50c7b8` perf-ignore switch → `e53667b`/`7a7ea42`/`2c24912`/`c4ed256` FE-090
vegetation proxy (accepted as-is) → `2da39f4`/`54769bb` RV-003 #3 ws=4 gate range →
`61eecc1` performance parked.
