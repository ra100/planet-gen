# Vegetation → weather feedback requirements

## Problem

Land evapotranspiration in the spinup has no vegetation state. The FE-086
capacity term is:

```wgsl
let et_capacity = f32(water.local == 0.0)
    * clamp(params.coverage, 0.0, 1.0)   // global user slider
    * clamp(params.moisture, 0.0, 1.0)   // global user slider
    * smooth_step(6.0, 22.0, temperature_at(pos));
```

Per-texel variation comes only from the temperature window. A Saharan cell, an
Amazonian cell, and a Siberian cell at the same temperature contribute the same
ET. The product's terrain-aware weather story (orographic condensation, LCL
lowering, rain shadows, venturi speed boost) already reads terrain; vegetation
is the one classical surface control still missing.

## What spinup already has (candidate inputs, no new fields)

- `temperature_at(pos)` — warmth window already used.
- `terrain.ascent` / `terrain.lee_drying` (terrain_transect) — relief and
  rain-shadow drying: lees are dry, windward slopes wet.
- `water.local`, `water.fetch` — marine proximity/fetch.
- `wind.a` (marine fraction), pressure field, ice fractions.
- `sample_height(pos)` — altitude for a treeline cap (~3 km, cold + thin air).

## Candidate scopes

### S1 — inline vegetation proxy (recommended first step)

Compute vegetation density inline per texel from existing fields:
warmth × per-texel moisture proxy × treeline cap × ice zero, with lee_drying
suppressing it. Multiply `et_capacity` by it. No new textures, no new pass,
no revision bump to the weather field layout. Cost: everything downstream moves
(pins, gates) — see constraints.

### S2 — dedicated biome field pre-pass (later option)

Generate a vegetation/biome cubemap at field resolution (Whittaker-style
temperature × moisture lookup like preview's), bind it to spinup. More faithful
and reusable by export/biome rendering, but adds a field, plumbing through
WeatherSnapshot/pipelines, and export parity questions.

### Explicitly out of scope

- Dynamic vegetation evolution (weather changing vegetation over time). Spinup
  is a static sequence; feedback here is one-way surface control. Any dynamic
  coupling is a separate requirement document.

## Physical expectations (testable)

- Wet tropical forest: ET at or near the current 0.27 ceiling; desert and
  polar/tundra/alpine-above-treeline: near zero.
- Lee/interior basins: reduced ET widens effective rain-shadow drying.
- Inland vapor penetration at low wind should IMPROVE where vegetation is dense
  (relevant to the A1b low-wind floor story).

## Constraints (the real cost)

1. **Pins:** all weather mass-fingerprint pins move (currently 3 in weather.rs).
   Re-baseline per protocol: clean build, two runs, identical hashes.
2. **DS-046 A gates:** A1a coast_corr, A1b land-cloud fractions, A1c
   cloud-free fraction all re-measure; thresholds re-specified with documented
   rulings (same protocol as RV-002 #2/#6 — measure first, then set).
3. **U15 coupling risk:** anvil/shear fixtures are parked known-blocked; spinup
   changes WILL move their values. Re-baselining them is a user decision —
   do not quietly recalibrate while parked.
4. **Doctrine:** FE-085/FE-086 land ET stays ocean-dominant provenance at the
   0.3 share; vegetation coupling must not flip provenance semantics.
5. **Bit-exactness at defaults:** if a default-off gate (e.g., strength 0.0)
   is used for rollout, verify bit-exact behavior before enabling.

## Acceptance shape

- Gate plan accompanies implementation: which DS-046 thresholds get
  re-measured, expected directions (A1b up in vegetated low-wind cases), and
  a before/after A/B (density maps + PSNR/SSIM like RV-003) showing the
  vegetation signature is visible but physically ordered (forests wetter than
  deserts at matched temperature).
