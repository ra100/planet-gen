# Save/load planet files — requirements

## Problem

A configured planet cannot be persisted. Presets are hardcoded (`PRESETS`,
app.rs) and every artistic override — terrain scale, water loss, clouds,
rings, lighting — is lost when the app exits. Reproducing a planet by hand
is error-prone; sharing one with another artist is impossible.

## Goal

Two top-bar actions:

- **SAVE** — native OS save dialog → JSON file containing every user-settable
  generation parameter. Default filename derived from the planet name.
- **LOAD** — native OS open dialog (JSON filter) → parse, validate, apply,
  full terrain regeneration. Viewport state (rotation/zoom/pan/view mode) is
  restored from the file.

## Scope decisions (user-ruled)

**Included in the file** (52 fields):

- Physics: `PlanetParams` (star distance, mass, metallicity, axial tilt,
  rotation period, seed).
- All visual overrides: terrain (continental scale, water loss, moisture,
  season, erosion, height/mountain/boundary/warp/detail scales, age override,
  plate/continent counts, size variety), lighting (sun azimuth/elevation),
  all 10 layer toggles, clouds/weather (coverage, cloud seed, opacity, wind
  scale, storm count/size), surface extras (lava glow, ring inner/outer/tilt/
  opacity, night lights, star color temp, city light hue).
- Viewport state (added v2 on user request): view mode, planet rotation,
  zoom, pan — a loaded planet looks exactly as it was left.
- Identity: `planet_name`.

**Excluded:**

- UI shell state (`active_tab`, `show_help`) — per-session, not a property of
  the planet.
- Export settings (`export_resolution`, 7 export layer toggles) — pipeline
  configuration, not planet content.
- `DerivedProperties` — deterministic from `PlanetParams`; recomputed on load.

## Design decisions

- **Format:** pretty-printed JSON, flat struct with a leading
  `format: "planet-gen"` magic string and `version: 1`. Single-author format;
  stays greppable/diffable. Extension `.json`.
- **Forward compatibility:** missing fields fall back to factory defaults
  (struct-level `#[serde(default)]` + hand-written `Default` mirroring the app
  constructor, so adding a field forces a deliberate default choice); unknown
  fields are ignored so newer files load in older apps.
- **Native dialogs via `rfd`** (0.17, default features). eframe 0.33 has no
  built-in file-dialog feature (verified against egui source through 0.36).
  Blocking API called from `App::update` — main thread, satisfies rfd's
  threading requirement; the app already blocks on terrain regen by design.
- **Validation on load is reject-don't-clamp:** parse → envelope check
  (format magic, version) → non-finite scan of every float (hand-edited
  `1e999` saturates to inf and must not reach GPU uniforms) → existing
  `PlanetParams::validate()`. Out-of-range physics means corruption or
  hand-editing; clamping would silently produce a different planet than the
  one saved. Clamp-on-load is a reasonable v2 option, parked for now.
- **Load is all-or-nothing:** the file is fully parsed and validated before
  any app state is assigned. A failed load never mutates the current planet.
- **Apply mirrors `apply_preset`'s tail** (`update_derived()` +
  `needs_terrain = true`) but deliberately does NOT recompute `cloud_seed` or
  `planet_name` from the seed — both are restored verbatim from the file.
  Weather rebuild rides the existing two-frame defer inside
  `regenerate_terrain()`.
- **Feedback:** transient top-bar callout (8 s, auto-expiring) for success and
  error; canceling a dialog is a silent no-op.

## Non-goals

- No preset management UI (presets remain hardcoded).
- No file watching / recent-files list.
- No command-line argument to open a file at startup (no clap in the project;
  could be a v2).
