# Contributing to Planet Gen

Thanks for your interest! Bug reports, planet screenshots, ideas, and pull requests are all welcome.

## Reporting a bug

Open an [issue](https://github.com/ra100/planet-gen/issues/new/choose) and include:

- Your OS, GPU, and Planet Gen version (shown in the window title, or `planet-gen --version`).
- What you did and what happened. A saved planet file (SAVE AS…) makes most problems reproducible.
- For startup or GPU problems, the terminal output of `RUST_LOG=info planet-gen`.

## Building

See [Build from source](README.md#build-from-source). The project uses stable Rust, wgpu, and egui; shaders are WGSL under `src/shaders/`.

## Before opening a pull request

CI runs these on Linux, macOS, and Windows; run them locally first:

```
cargo fmt -- --check
cargo clippy --all-targets -- -D warnings
cargo test --lib -- --skip gpu --skip preview --skip export
```

GPU tests are skipped in CI because hosted runners have no real GPU. If you change shaders, the preview, or export, also run the full `cargo test --release --lib` on a machine with a GPU.

- Keep pull requests focused on one change and describe what you checked. Before/after screenshots help a lot for visual changes.
- Add a line to `CHANGELOG.md` under `[Unreleased]` for anything users will notice.
- Commit messages start with a gitmoji: ✨ feature, 🐛 fix, ♻️ refactor, 📝 docs, 📋 plan.

## Project layout

[README.md](README.md#project-structure) lists the modules. Larger features start as a requirements note in `docs/brainstorms/` and a plan in `docs/plans/`, tracked in [Plans.md](Plans.md).

By contributing you agree that your contributions are licensed under the [MIT License](LICENSE).
