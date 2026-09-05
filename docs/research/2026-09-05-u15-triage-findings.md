# U15 triage findings — 15 parked `--weather-validation` failures

Date: 2026-09-05 · Follows `docs/handoffs/2026-09-04-u15-triage-handoff.md` · HEAD at start: `f5ece36`

## 1. Reproduction (requirement 1) — CONFIRMED

Fresh run on current HEAD (`target/val-u15-triage-run1.log`, `--size 512`):
**15 automated gate failures = 8 × `plume_pass` + 6 × `anvil_pass` + 1 × org count (2/8)** — matches the handoff.

Corrections to the handoff table:
- seed 19 **plume2 also fails axis** (34.8°); every other plume2 axis is ≤ 17.4°.
- The "compliant anvil response" lines are a separate gate (`anvil_pass`, 8-storm fixture), not just superset telemetry. All 6 failing seeds show **negative downwind high-cloud centroid shift** (−1.4…−68.2 texels) and/or pca > 20° (seed 7: 39.4°, 37: 70.2°, 73: 85.8°).

## 2. Per-seed attribution (requirement 2) — current HEAD

| seed | L2/L1 (≥1.50) | S2/S1 (≥0.75) | ax1 (≤30°) | ax2 (≤30°) | failing sub-gates |
|------|---------------|---------------|------------|------------|-------------------|
| 7    | 1.865 ✓       | **0.735 ✗**   | **62.7 ✗** | 9.4 ✓      | sharpness, axis@ws1 |
| 19   | **1.286 ✗**   | 1.167 ✓       | **58.7 ✗** | **34.8 ✗** | elongation, axis@ws1+ws2 |
| 37   | **1.475 ✗**   | 0.783 ✓       | **35.9 ✗** | 11.1 ✓     | elongation, axis@ws1 |
| 73   | **1.479 ✗**   | 0.860 ✓       | **45.4 ✗** | 1.4 ✓      | elongation, axis@ws1 |
| 101  | **1.257 ✗**   | 0.936 ✓       | 22.5 ✓     | 17.4 ✓     | elongation only |
| 211  | 2.049 ✓       | 0.818 ✓       | **69.0 ✗** | 6.7 ✓      | axis@ws1 only |
| 509  | 1.825 ✓       | 0.861 ✓       | **59.2 ✗** | 5.0 ✓      | axis@ws1 only |
| 997  | 1.936 ✓       | 0.791 ✓       | **64.7 ✗** | 11.5 ✓     | axis@ws1 only |

All other sub-gates (p95 ≥ 0.04, Neff ≥ 32, corridor ≤ 5%, support ≤ 5%, boundary shell ≤ 1%, B2/B1, determinism) pass on **all 8 seeds at both scales**.

Field-structure analysis of the dumped response fields (`PLANET_GEN_U15_DUMP_TRAIL=1`, `target/u15-triage-2/analyze_trail.py`):
- Response = condensate only (finalize writes low/deep/high; vapor is not in the output).
- **ws=1: B(meridional) ≥ L(alongwind) on every seed** — the plume is a fat blob, mass peaks ~0.20 rad downwind, 0% within the source radius; PCA axis of that blob is noise (59–69° on 4 seeds).
- **ws=2: proper streaks** — L > B, centroid +0.37 rad downwind, axis 1.4–17.4° on 7/8.
- Crosswind width ≈ 0.26–0.29 rad at **both** scales (eddy+numerical diffusion dose is wind-scale-invariant because CFL substeps scale with wind); alongwind span ∝ ws.

## 3. Bisection + mechanism (requirement 3, systemic ws=1 misalignment)

Fixture and gate code are byte-identical `e1b260d..8837b21`; only physics drifted (+559 shader lines over 9 commits).

| commit | U15 trail result |
|--------|------------------|
| `e1b260d` (Jul 20) | **GREEN** — L2/L1 1.703–2.368 all seeds (Plans.md line) |
| `9b0267b` | no-op in this fixture (stratiform regime gated off: effective_speed 0.91/1.82 > 0.70) |
| **`0e2b044`** "Improve terrain-driven cloud formation" | **RED** — all 8 seeds fail axis at BOTH scales (ax1 31–50°, ax2 39–47°), L2/L1 0.99–1.15 |
| `d47263f` | still red (ax1 12–44° on 7/8; L2/L1 all < 1.5) |
| `8837b21` (pre-eddy, `target/val-u15-pre-eddy-run.log`) | still red (ax1 4–43° on 7/8; L2/L1 0.89–1.98) — eddy diffusion aggravates but did not cause the failure |
| current HEAD | ax1 22–69° on 7/8; L2/L1 1.26–2.05; S2/S1 all pass except seed 7 (0.735) |

**Mechanism (`0e2b044`):** the commit replaced `surface_supply = mix(0.50, 0.60, marine_fraction)` with a marine-only supply ("terrain only converts or removes transported vapor"). Inland `q_target` dropped from ≈ 0.25 q_sat to ≈ 0 (FE-084's ET restores only ~5%: q_target_land ≈ 0.013 q_sat). The dew floor `q_lcl = min(lift_ceiling, 0.7·q_target)` therefore fell from ≈ 0.16 q_sat — *below* exported marine vapor content (0.2–0.45 q_sat), which made conversion gradual along the path — to ≈ 0.009 q_sat, so exported marine vapor now **dumps one-shot at the coast**. Result: blob instead of streak; PCA axis arbitrary; no wind-scale elongation. The gate's bands were measured against the pre-`0e2b044` transport-limited regime (L2/L1 1.70–2.37).

The handoff's "mesoscale steering" hypothesis is refuted for this fixture: `create_test_textures` bypasses `wind_field.wgsl`; wind is exactly the zonal shear, pressure constant → convergence and frontal lift are zero everywhere.

## 4. Option B lever experiments (measured, not shipped)

| lever | idea | result |
|-------|------|--------|
| L1 | dew floor scaled by `source_owned_share` (provenance-weighted parcel memory) | **no-op** — production `generate()` runs with provenance disabled (`source_owned_share = 0` in this fixture) |
| L2 | self-referential land-only floor `0.5·state.x` | tiny deltas, still red (15/15) — advection dilution drops vapor below the activation threshold within ~0.1 rad of the coast; gradual conversion never engages along the path |
| L3 | absolute land-only dew floor 0.16 q_sat ("retained source-region humidity", half the marine ceiling) | still red (15/15); axis improved slightly (seed 7: 62.7→56.8°) but sharpness regressed (S2/S1 seed 7: 0.735→**0.565**) and seed 19 elongation regressed (1.286→1.028). A-gates barely moved (land_cloud 17.4/30.8/36.9/51.8%; A1c 34.1%). **Reverted.** |

The only complete physics fix is restoring a static inland humidity source (≈ the old `surface_supply` 0.50, or ~10× FE-084 ET strength), which contradicts FE-084's accepted design ("the SOURCE budget stays capped") and would re-specify the land-cloud regime of every world (A1b/A1c re-measurement + perceptual review required). That is a new feature phase, not triage.

## 5. Decision proposal — Option C gate surgery (for user ruling)

Per the handoff: no band changes ship without explicit approval; numbers below are measured, not fitted. Precedent on record: Plans.md FE-083 line already reported these fixtures as "encoding pre-FE-083 transport semantics — reported, not weakened."

**C-1 · per-plume axis gate (7/8 seeds failing at ws=1).**
In the current regime the ws=1 plume is diffusion-limited: B ≥ L on all seeds, so PCA major-axis alignment is not a meaningful wind-ownership diagnostic there. The meaningful signals are present and measured on all 8 seeds: downwind centroid +0.22 rad (+32–35 texels), ~0% upwind mass, corridor/support/boundary sub-gates all green.
*Proposal:* apply `axis ≤ 30°` to **plume2 (ws=2) only** (measured 1.4–34.8°, 7/8 pass; seed 19 marginal). For plume1 (ws=1), replace with a downwind-organization check: centroid ≥ +8 texels downwind AND upwind response mass ≤ 5% (measured: passes all 8 with large margin).
*Variant:* apply `axis ≤ 30°` conditionally only when L/B ≥ 1.25; otherwise apply the downwind-organization check.

**C-2 · elongation ratio L2/L1 ∈ [1.50, 2.50] (4 seeds failing: 19/37/73/101).**
Measured current distribution: **1.257–2.049** (pre-regime-change: 1.70–2.37). The lower bound encoded transport-limited scaling (L ∝ wind speed); in the current dilution-limited regime reach scales sub-linearly with wind, so the physical assertion "stronger wind stretches the plume" is L2/L1 > 1.0 with an upper bound against runaway stretch.
*Options for ruling:* (a) keep [1.50, 2.50] and park seeds 19/37/73/101 as known-blocked; (b) re-specify to **[1.10, 2.75]** — passes all 8 with ≥ 0.16 margin on the worst seed (1.257). Not a fit-to-noise: the band change is justified by the regime change in §3, and option (a) remains available if you prefer the conservative reading.

**C-3 · sharpness S2/S1 ∈ [0.75, 1.25] (1 seed marginal: 7 at 0.735).**
Not a regime artifact — sharpness is a meaningful diagnostic and FE-090 already healed the other four seeds. *Recommendation:* keep the band; park seed 7 (2% below) or revisit after C-1/C-2 land.

**P3 · storm organization (2/8, separate gate).** Known-blocked since the parked FE-034–047 experiment ("no viable parameter-only path"; user chose to park). All 6 failing anvil seeds show upwind high-cloud centroids — the high reservoir spreads isotropically around convective cores (diffusion-dominated, same Péclet issue as the plume blob). Needs a dedicated storm-organization physics phase or its own re-spec; out of scope for this triage. Recommend: keep parked, documented.

**Net effect if C-1 + C-2(b) are approved:** 15 → **7** failures (6 anvil + 1 org count), all organization-related and already known-blocked. If C-2(a): 15 → **11**.

## 5a. Ruling (user, 2026-09-05) and implementation

User rulings on the three proposals:
- **C-1 approved as recommended:** axis ≤ 30° applies to plume2 (ws=2) only; plume1 (ws=1) is gated by a downwind-organization check — shape-ROI centroid ≥ +8 texels downwind of the source AND upwind response mass ≤ 5%.
- **C-2 approved:** L2/L1 re-specified to **[1.10, 2.75]**.
- **C-3/P3:** user chose to **also relax sharpness to S2/S1 ∈ [0.70, 1.25]** (overriding the keep-as-is recommendation); organization stays parked.

Implementation (`src/bin/sweep.rs`): two new `U15PlumeMetrics` fields — `downwind_centroid_texels` (signed alongwind offset of the shape-ROI centroid relative to the source, same texel scale as `centroid_texels`) and `upwind_response_fraction` (shape-ROI mass share with zonal < 0) — computed from the existing shape-ROI population; `plume_pass` restructured per the ruling; spec string updated. No shader or physics change; A-gates and pins untouched.

Validation (`target/val-u15-surgery.log`, `--size 512`): **total gate failures 15 → 8** = 6 anvil + 1 org count (2/8) + 1 plume. Plume sub-gates measured: downwind centroid +33…+36 texels at ws=1 (gate ≥ +8, large margin), upwind mass 0% on all seeds (gate ≤ 5%), L2/L1 1.257–2.049 within [1.10, 2.75], S2/S1 0.735–1.167 within [0.70, 1.25]. Seeds 509/997 fully green; seeds 7/37/73/101/211 plume-PASS (anvil still FAIL). **Remaining disclosed marginal:** seed 19 plume2 axis 34.8° > 30° — the case flagged in the proposal ("measured 1.4–34.8°, 7/8 pass"); kept failing per the never-force-green rule, parked as known-blocked with numbers on record. Bin suite: 52 passed (serial; the parallel GPU-init SIGSEGV flake is pre-existing and documented in Plans.md). Lib suite: 179 passed / 3 ignored. Final confirmation on the exact committed tree (`a9fe637`, rebuilt release binary): `target/val-u15-final.log` reproduces **8 failures** (6 anvil + org count + seed-19 plume) and the full validation-feature suite passes serially — **288 passed / 3 ignored across 11 suites**.

## 6. P3 storm-organization investigation (post-surgery, 2026-09-05)

The remaining complex (6 anvil failures + org count 2/8, FE-034–047 "no viable parameter-only path") was reopened with a new hypothesis: the **metric**, not the physics, was mis-specified. CONFIRMED by measurement.

**Mechanism found.** `u15_compliant_anvil_metrics` (pre-change) aggregated *all* deep components across the whole sphere into one mass-weighted centroid, evaluated "downwind" at that phantom point, and ran PCA of globe-scattered outside-high in its tangent frame. With 8 seeded centers spread over the sphere this records aggregation geometry, not anvil physics — measured aggregate shifts were −68.2…−1.4 texels against a +0.5 requirement. Struct names (`minimum_downwind_centroid_texels`, per-core `core{}=` display) show per-core was the original intent.

**Per-core measurement (env-gated diagnostic, then full re-implementation).** Per core = own ch1-mass-weighted centroid, own local fixture wind for the downwind reference, unlabeled high (ch2 ≥ 0.02) assigned to nearest core within a capture zone. All **27 cores across all 8 seeds drift DOWNWIND +10.0…+17.4 texels** — 20–35× the +0.5 requirement. Physics passes the drift intent overwhelmingly; it always had.

**Per-core gate (thresholds unchanged: outside ≥ 0.10, each-core shift ≥ +0.5, each-core PCA ≤ 20°):**
- Passes anvil: **7/37/73**; org count **3/8** (was 2/8); total validation failures **8 → 7**.
- Set change is honest both ways: seeds 7/37/73 newly pass; **seeds 509/997 newly FAIL** — they had passed the aggregate metric only via phantom geometry. Remaining anvil failures are single-core PCA-angle outliers: seed 19 core2 32.5°, seed 101 core2 49.5° / core3 35.6°, seed 211 core1 21.9°, seed 509 core1 21.6°, seed 997 core3 25.0°. Shifts on all of those cores are still +10…+17 and outside-fraction ~1.0 — angle is the only failing sub-criterion.
- Elongation telemetry (major/minor axis ratio of captured anvil): outliers sit at 5.7–6.6, indistinguishable from passing cores (5.3–10.1) — these are genuinely elongated structures, not blob PCA noise.

**Ruled-out explanations for the angle outliers.**
- *Capture-zone neighbor contamination:* shrinking capture 3R→2R made things **worse** (8 failures; seed 7 core0 19.1°→22.0°, seed 19 three cores over) — tilt is intrinsic, not zone overlap.
- *Seeded-center proximity:* nearest-center separations don't correlate with tilt (seed 7 has a 0.060-rad pair yet passes all angles ≤ 19.1°; seed 509's nearest pair is 0.42 rad yet tilts 21.6°).
- *Upwind drift:* none — every core positive at both radii.

**Interpretation.** The fixture traps high cloud in ×8 convergent inflow (6–12× base flow inside the core zone, zero at 0.11 rad); anvil mass escapes only by diffusion and is then swept on great-circle-ish paths across sphere-wide interference fields (35–45% of high mass ends beyond 3R of every core — far-field). Near some cores the captured elongated structure's major axis lands 21–50° off the local downwind vector. That is a shape-alignment question about long-lived high-channel transport in this fixture, not a drift/organization failure; per-core measurement already proves the anvil-side physics organizes downwind.

**Status.** Per-core implementation (same thresholds, capture = 3× core radius, nearest-core assignment) sits **uncommitted in the working tree pending user ruling** — it changes gate semantics (which seeds pass/fail), so it follows the same ruling discipline as C-1/C-2/C-3. A-gates bit-identical to baseline (`val-u15-percore.log` vs `val-u15-final.log`: land_cloud 17.5/30.8/37.0/51.9%, coast_corr ≤ 0.056, A1c 34.1%); no shader change. Logs: `target/val-u15-anvil-core.log` (diagnostic), `target/val-u15-percore.log` (per-core gate, 7 failures), `target/val-u15-cap2r.log` (2R probe, 8 failures).

## 7. Artifacts

- Logs: `target/val-u15-triage-run1.log` (baseline), `-run2.log` (+dumps), `-pre-eddy-run.log`, `-bisect-a.log`, `-bisect-b.log`, `-lever1/2/3.log`, `-surgery.log`, `target/val-u15-final.log` (committed-tree confirmation), `target/val-u15-anvil-core.log` (P3 per-core diagnostic), `target/val-u15-percore.log` (per-core gate 3R, 7 failures), `target/val-u15-cap2r.log` (2R capture probe).
- Dumps + analysis: `target/u15-triage-*/u15tr_seed_*_ws{1,2}_response.png`, `target/u15-triage-2/analyze_trail.py` (cube-layout PNG → angular stats; mapping verified roundtrip err 0).
- Instrumentation (committed): env-gated `PLANET_GEN_U15_DUMP_TRAIL=1` trail-response dump in `src/bin/sweep.rs`.
- Working worktree: `target/u15-pre-eddy-wt` (`8837b21`) — remove once this triage closes.
