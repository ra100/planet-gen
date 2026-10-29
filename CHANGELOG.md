# Changelog

All notable changes to Planet Gen are recorded here. The format follows
[Keep a Changelog](https://keepachangelog.com/en/1.1.0/) and the project uses
[Semantic Versioning](https://semver.org/) (pre-1.0: minor releases may change
behaviour).

## [Unreleased]

### Added

- Giant planets: above ~3 M⊕ (Mass now reaches 4000 M⊕) a gas envelope
  replaces the terrain with a banded cloud deck. Band count follows the
  rotation, size and temperature (Rhines scale); Jupiter-like giants get
  belts, zones, a great red spot, white ovals, hot spots and polar cyclones,
  Saturn-like ones muted hazy bands and a polar hexagon, ice giants a
  greenish-blue methane deck with bright clouds and a dark spot. Hotter
  giants follow the Sudarsky classes from white water clouds to dark blue
  and near-black roasters with a thermal glow. Colours are calibrated to
  measured planet albedos. **Envelope** (AUTO) overrides the mass rule.
- Icy worlds: dry, airless bodies beyond the frost line get an ice crust
  whose look follows the crater age: Europa's lineae and chaos, Ganymede's
  grooved bright terrain, Callisto's dark cratered lag. **Ice crust** (AUTO)
  under surface history.
- Presets JUPITER, SATURN, URANUS, NEPTUNE, HOT JUPITER, EUROPA, GANYMEDE
  and CALLISTO.

### Fixed

- Rings: they now lie in the planet's equatorial plane, cross in front of
  the planet and hide behind it, with a Saturn-like structure (translucent
  C ring and Cassini Division, dense B ring, A ring with gaps), thin-layer
  lighting, the planet's shadow on the rings and the rings' shadow on the
  planet. **Ring opacity** became **Ring density** (optical depth relative
  to Saturn's); the separate ring tilt is gone.

### Changed

- Contributor tooling: the Beautiful Rust skills are vendored for Claude Code,
  and clippy now rejects `unwrap()` outside tests, `#[allow]` in place of a
  reasoned `#[expect]`, and undocumented `unsafe` blocks. Capture tools now
  name what failed when they stop.

## [0.3.0] - 2026-10-27

### Added

- Downloadable builds: tagged releases now ship a Windows zip, a macOS app
  (Apple Silicon `.dmg`), a Linux tarball with an `install.sh` for the
  application menu, and the Blender add-on as a 4.2+ extension zip (plus a
  4.1 legacy zip). No Rust or OpenEXR install is needed to run them.
- `planet-gen --version` and `--help`.
- README sections for downloading, a first planet, and troubleshooting;
  CONTRIBUTING.md, issue templates, and the MIT `LICENSE` file.

### Changed

- On Windows, release builds no longer open a console window beside the app.
- `cargo run --release` starts the app without `--bin planet-gen`.

### Fixed

- Exported cloud heights no longer float thin low cloud at 2-5 km under a
  trace of cirrus. Base and thickness mixed the low, deep and cirrus layers by
  optical weight, so a little cirrus lifted a low sheet to a slab between the
  layers where neither had cloud. Low and deep still mix; the cirrus layer's
  heights take over only where cirrus dominates the optical weight (share
  0.2-0.8). On a 16K band, columns with a 3-7 km base fell from 2.5 % to
  0.6 % of the cloud.
- Exported clouds at 8K and 16K no longer fill broken cumulus with one- and
  two-pixel holes and specks, which Blender close-ups magnified into a carpet
  of equal soft blobs. The export cuts individual clouds and holes only from
  structure at least two pixels across; the cloud cover is unchanged and the
  app preview is identical.
- `cloud_height.exr` heights no longer jump between neighbouring pixels. At
  16K, a hole in a low cloud deck took the height of the cirrus above it, up
  to 12 km higher than the pixel next to it. The layer mix now comes from the
  cloud system, about 25 km across. Clear texels carry the heights of the
  cloud around them instead of zero. The Cloud top view matches.

## [0.2.0] - 2026-10-20

### Added

- SAVE +1 button for incremental saves, next to SAVE, SAVE AS… and LOAD.
- Current-file name in the top bar and window title, with `•` while there are
  unsaved changes; the window title also shows the app version.

### Changed

- SAVE now overwrites the current file (Ctrl+S) and SAVE AS… opens the dialog
  (S); before, the SAVE button always opened the dialog.
- Incremental save continues on the version it writes, so Ctrl+S afterwards
  overwrites the new version rather than the original file.
- Saves are written to a temporary file and renamed into place.

### Fixed

- Incremental save bumps an existing version instead of appending another:
  `abc.001.json` now gives `abc.002.json`, not `abc.001.001.json`. Trailing
  numbers after `.`, `_` or `-` (optionally `v`-prefixed) are recognised and
  keep their padding, and the next number is past the highest existing
  version of the same file.

## [0.1.0]

First version: GPU procedural planet generation, preview, planet files and
tiled export.
