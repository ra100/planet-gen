---
title: "feat: Save/load planet files"
type: feat
status: completed
date: 2026-09-21
origin: "User request: ability to save and load a planet file containing all settable parameters (docs/brainstorms/2026-09-21-save-load-planets-requirements.md)"
---

# Save/load planet files — implementation plan

## U1: `planet_file` module + dependencies

New `src/planet_file.rs` (registered in `src/lib.rs`):

- `FILE_FORMAT = "planet-gen"`, `FILE_VERSION = 1`.
- `PlanetFile` — flat JSON envelope, 47 fields: format/version + 6 physics +
  14 terrain/climate + 2 lighting + 10 layer toggles + 6 clouds/weather +
  8 surface extras + planet_name. `#[serde(default)]` at struct level;
  hand-written `Default` mirrors the app constructor (app.rs `new()`), so a
  missing field always means "factory default" and adding a field forces a
  compiler-checked default update. No `deny_unknown_fields`.
- `checked_load(json) -> Result<PlanetFile, String>` — parse → format magic →
  version (<1 reject, >current warn+attempt) → non-finite scan of all f32s →
  `PlanetParams::validate()`. Reject, don't clamp.
- `default_filename(name)` — sanitize free-text planet name (alnum/`_`/`-`,
  collapse separators, fallback "planet") + `.json`.

Cargo.toml: `serde` (derive), `serde_json`, `rfd = "0.17"` (default features;
Linux XDG-portal backend, no GTK headers).

Tests (inline module): roundtrip default; missing fields → factory defaults;
unknown fields ignored; wrong format rejected; zero version rejected;
non-finite rejected; out-of-range physics rejected; filename sanitization.

## U2: App wiring

`src/app.rs`:

- `file_msg: Option<(bool, String, Instant)>` field (ok, text, expiry) + init.
- `collect_settings() -> PlanetFile` — literal snapshot of the 47 fields.
- `apply_settings(&PlanetFile)` — assign all 47 fields (params rebuilt), then
  exactly `update_derived()` + `needs_terrain = true`. NOT cloud_seed/
  planet_name recompute (preset-specific lines), no early
  `invalidate_weather()`.
- `save_planet_file(ctx)` / `load_planet_file(ctx)` — rfd dialogs (blocking,
  main thread), JSON write/read, all-or-nothing apply, transient feedback via
  `set_file_msg` (8 s + `request_repaint_after`). Cancel = no-op.
- top_bar: SAVE/LOAD small buttons next to HELP; callout render for file_msg;
  expiry clear at top of `update()`.
- Shortcuts S/L in `handle_keyboard_shortcuts` (behind existing
  `wants_keyboard_input()` guard); status-bar hint string updated.

## DoD

- `cargo build`, `cargo test --lib`, `cargo clippy` clean.
- Manual: save → inspect JSON (47 fields, pretty) → mutate sliders/name → load
  → planet regenerates with saved values (cloud_seed + name verbatim); cancel
  = no-op; corrupted file and out-of-range file both rejected with ERROR
  callout and unchanged state.
