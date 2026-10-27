# Beautiful Rust skills (vendored)

`rust-best-practices/`, `rust-review/` and `rust-port/` are vendored from
[morganlinton/BeautifulRust](https://github.com/morganlinton/BeautifulRust)
v1.4.0, commit `58757ee890da9b3c8d5b785adc7a8d53cce27307`. Copyright (c) 2026
Morgan Linton, MIT licensed; each skill folder carries the upstream `LICENSE`.

Only the skill instructions are vendored: each `SKILL.md`, the reference
digests under `rust-best-practices/references/`, and the upstream drop-in lints
config under `rust-best-practices/lints/` for reference. Upstream evals,
benchmarks, banner and plugin metadata are left out. The files are unmodified;
to update, copy the same paths from a newer upstream commit and bump the commit
above.

## How this repo applies them

- `rust-best-practices` loads whenever Claude Code writes or changes Rust here.
  `rust-review` reviews code or a diff, and `rust-port` ports code from other
  languages.
- Planet Gen is an application, not a library, so the skill's application
  rules apply: simple concrete types, no speculative generality.
- The machine-checkable subset of the rules lives in `Cargo.toml` `[lints]` and
  `.clippy.toml`, enforced by CI's `cargo clippy --all-targets -- -D warnings`.
  The upstream `lints/lints.toml` is not adopted wholesale: its pedantic group,
  cast lints and library-only lints (`missing_docs`, `print_stdout`) would flag
  thousands of deliberate sites in GPU and numeric code.
