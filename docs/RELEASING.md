# Releasing

Releases are built by [`.github/workflows/release.yml`](../.github/workflows/release.yml).

1. Bump `version` in `Cargo.toml` (and run `cargo build` so `Cargo.lock` follows).
2. In `CHANGELOG.md`, rename `## [Unreleased]` to `## [X.Y.Z] - YYYY-MM-DD` and start a new empty `## [Unreleased]` above it. The release notes are copied from this section.
3. If the Blender add-on changed, bump `version` in both `blender_addon/blender_manifest.toml` and `bl_info` in `blender_addon/__init__.py` (the workflow checks they match).
4. Merge to `main`, then tag and push:

   ```
   git tag vX.Y.Z
   git push origin vX.Y.Z
   ```

5. The workflow checks the tag matches `Cargo.toml`, builds every bundle, and creates a **draft** release with the files and a `SHA256SUMS.txt`. Check the draft on the Releases page and press Publish.

| Bundle | Built on | Notes |
| --- | --- | --- |
| `planet-gen-X.Y.Z-linux-x86_64.tar.gz` | Ubuntu 22.04 | OpenEXR linked statically from vcpkg; binary, `.desktop` entry, icon, `install.sh` |
| `planet-gen-X.Y.Z-macos-arm64.dmg` | macOS (Apple Silicon) | `Planet Gen.app`, OpenEXR static, ad-hoc signed (not notarized) |
| `planet-gen-X.Y.Z-windows-x86_64.zip` | Windows | `planet-gen.exe` with the OpenEXR DLLs beside it |
| `planet-gen-blender-X.Y.Z.zip` | Ubuntu | Blender 4.2+ extension (manifest at the zip root) |
| `planet-gen-blender-X.Y.Z-legacy.zip` | Ubuntu | Blender 4.1 add-on (`planetgen_addon/` folder) |

Each platform job runs the packaged binary with `--version`, and the Linux and macOS jobs fail if the binary still links OpenEXR dynamically.

## Dry runs

Run the workflow from the Actions tab (Run workflow), or open a pull request that touches `packaging/`, `build.rs`, `blender_addon/` or the workflow. Both build every bundle and upload them as workflow artifacts without creating a release.

## Not done yet

- macOS notarization and Windows code signing (both need paid certificates; until then users see a Gatekeeper or SmartScreen prompt the first time).
- Intel macOS and Linux ARM builds.
