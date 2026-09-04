---
title: "feat: Vegetation-to-weather feedback (S1 inline proxy)"
type: feat
status: active
date: 2026-09-04
origin: docs/brainstorms/2026-09-04-vegetation-weather-feedback-requirements.md
---

# feat: Vegetation-to-weather feedback (FE-090, scope S1)

## Summary

Give the spinup's land evapotranspiration a per-texel vegetation state so a
Saharan cell and an Amazonian cell at the same temperature no longer
contribute identical ET. Scope S1 from the requirements doc: an inline
vegetation proxy computed from fields `advance_state` already samples — no new
cubemaps, no pipeline plumbing, no layout revision bump.

## Design

All changes are in `src/shaders/weather_spinup.wgsl`, inside
`advance_state`, immediately before the FE-086 `et_capacity` block (the
fields below are already in scope at that point: `wind`, `terrain` /
`rain_shadow` (= `terrain.lee_drying`, bounded [0,1]), `ice_fraction`,
`params.ocean_level`, `sample_height(pos)`).

### Vegetation proxy

```wgsl
// FE-090 (S1): per-texel vegetation density from existing fields — no new
// fields, no layout bump. Vegetation needs per-texel moisture (coasts wet,
// interiors dry; lee slopes dried by rain shadow), an altitude low enough for
// a treeline, and no persistent ice. Bounded in [0,1]. The 6..22 C warmth
// window stays in et_capacity below (single source of truth); this proxy adds
// the moisture/relief structure it currently lacks.
let elevation_km = max(sample_height(pos) - params.ocean_level, 0.0) * 5.0;
let continentality = smooth_step(0.15, 0.85, wind.a);
let veg_moisture = (1.0 - rain_shadow * 0.6)
    * mix(0.10, 1.0, 1.0 - continentality);
let vegetation_density = veg_moisture
    * (1.0 - smooth_step(2.2, 3.4, elevation_km))   // treeline cap ~3 km
    * (1.0 - ice_fraction);                         // frozen ground: no ET
```

Design notes:

- `wind.a` is the continentality channel (already used by
  `temperature_at` and inverted into `marine_fraction` at the same site),
  so coasts get dense vegetation and deep interiors sparse — this is what makes
  the Saharan cell transpire less than the Amazonian one.
- The interior floor of 0.10 (not 0) keeps deserts at a small but nonzero ET,
  which matches sparse-arid reality and avoids a hard discontinuity; the lee
  factor can push it toward ~0.04 in rain shadows.
- `calm_wet_land_mask` already receives `et_capacity`, so the FE-084
  calm-wet stratiform band inherits the per-texel structure for free — no
  separate edit needed there.

### Capacity change

```wgsl
let et_capacity = f32(water.local == 0.0)
    * clamp(params.coverage, 0.0, 1.0)
    * clamp(params.moisture, 0.0, 1.0)
    * smooth_step(6.0, 22.0, temperature_at(pos))
    * vegetation_density;   // FE-090 S1
```

Consequences:

- Land ET can only DECREASE relative to today (vegetation_density ≤ 1), so the
  FE-085/FE-086 ocean-dominant provenance semantics and the 0.3 share are
  untouched — vegetation coupling cannot flip them (requirement constraint 4).
- No new parameter or default-off gate: the proxy is always on and fully
  deterministic, so bit-exactness-at-defaults (constraint 5) is satisfied by
  construction; every downstream metric is re-measured in one validation run.
- Expected directions (to be measured, not assumed): A1b land-cloud fractions
  shift down in dry/interior cells and up-or-hold in wet coastal ones; A1a
  coast_corr should improve (coasts wetter relative to interiors); A1c
  cloud-free fraction may drift either way.

## Verification plan

1. **Pins (constraint 1):** the three weather.rs mass-fingerprint pins move.
   Re-baseline per protocol: clean release build, two runs, identical hashes,
   documented ruling in the test comment (same as RV-002 #4).
2. **Lib suite:** `cargo test --release --features validation --lib` — no new
   failures beyond the known set; ×2 identical per protocol.
3. **DS-046 re-measurement (constraint 2):** full sweep at
   ws ∈ {0.25, 0.5, 0.75, 1, 2} (+ ws=4 telemetry). Measure FIRST against the
   current thresholds; where physics plausibly moved a threshold, re-specify
   with a documented ruling (RV-002 #6 protocol) — never loosen to force green.
4. **U15 coupling (constraint 3):** parked known-blocked; spinup changes WILL
   move the anvil/shear fixture values. Report per-seed deltas in the run log;
   do NOT re-baseline or recalibrate while parked (user decision).
5. **A/B before/after:** density-map dumps from the pre-change baseline
   (target/val-perf-ignore-run) vs the post-change run, same 8 seeds; PSNR/SSIM
   via ffmpeg (magick is broken on this box); plus a physical-ordering check —
   matched-temperature land cells must show forest/cloudier > desert/drier.
6. **Perf:** queue p95 gates are environmental on this box (410+ ms generation
   floor vs 32.9 ms baseline, bit-identical code); use
   `PLANET_GEN_IGNORE_PERF_GATES=1` for the run and report perf separately —
   correctness failures must still be zero new.

## Tasks

| # | Task | DoD | Status |
|---|------|-----|--------|
| 1 | Implement S1 vegetation proxy + et_capacity multiply in weather_spinup.wgsl (FE-090 comment block) | shader compiles; lib suite runs | cc:TODO |
| 2 | Re-baseline the 3 weather.rs pins per protocol | clean build, two runs, identical hashes, ruling documented | cc:TODO |
| 3 | Lib suite green (no new failures beyond known set), ×2 | log saved under target/ | cc:TODO |
| 4 | DS-046 re-measurement + threshold rulings where physics moved | measured values recorded; any re-spec has a ruling comment | cc:TODO |
| 5 | U15 per-seed delta report (no recalibration while parked) | deltas in run log; status stays known-blocked | cc:TODO |
| 6 | A/B before/after density maps + PSNR/SSIM + physical-ordering check | numbers recorded in Plans.md | cc:TODO |
| 7 | Plans.md update + plan status → completed | section stamped with commit hash | cc:TODO |

## Out of scope

- S2 dedicated biome pre-pass (separate requirement doc if pursued).
- Dynamic vegetation evolution (explicitly excluded by the requirements doc).
- U15 re-baselining / gate recalibration (user decision, parked).
