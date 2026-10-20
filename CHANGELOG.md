# Changelog

All notable changes to Planet Gen are recorded here. The format follows
[Keep a Changelog](https://keepachangelog.com/en/1.1.0/) and the project uses
[Semantic Versioning](https://semver.org/) (pre-1.0: minor releases may change
behaviour).

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
