---
title: Native Terrain Diffusion CUDA Feasibility Spike
type: feat
status: dropped
date: 2026-07-26
origin: GEN-006, GEN-007, GEN-008
---

# Native Terrain Diffusion CUDA Feasibility Spike

> **ARCHIVED / NO-GO.** The retained spike records a rejected whole-planet direction: the final held-out export gate failed, so no further spike work is planned. See [the archived roadmap entry](../../Plans.md#archived-terrain-diffusion-and-imported-terrain) and [native evidence](../research/terrain-diffusion-native-spike-manifest.md).

> **APPROVED TO RUN AN ISOLATED NATIVE CUDA FEASIBILITY SPIKE. NOT APPROVED FOR APP/UI INTEGRATION, SCIENTIFIC SUITABILITY CLAIMS, OR MODEL DISTRIBUTION.**

## Summary

Run a local, isolated native CUDA feasibility spike for the pinned 90 m model. The spike is a separate Rust process under `spikes/terrain-diffusion-native/`; root application and Cargo files are explicitly out of scope. Its only possible approval is technical feasibility. The current Phase 5.24 procedural evaluator remains `NO-GO`, so app/scientific approval is blocked regardless of spike output.

## Frozen Inputs

- Source/exporter: `xandergos/terrain-diffusion@6d770c943cf18f6732a7b15bf95f667cacf1ca17`, `requirements.txt`, and `terrain_diffusion/onnx/export.py`.
- Checkpoint: `xandergos/terrain-diffusion-90m@2bb1a93141140040091d44f0770d4ba22c8cf145`.
- Python: maintainer-only `3.11.15`; export is pinned and local under `target/terrain-diffusion-native/env/`.
- Rust runtime: `ort = "=2.0.0-rc.12"` with opt-in `nvidia`; ORT loads only from the preflight-selected dynamic library path. CUDA-only, no CPU provider/fallback.
- Host probe, not a target minimum: RTX 4090 24 GiB; driver `580.173.02`; reported CUDA `13.0`; `rustc 1.97.1`; Python `3.11.15`; 83 GiB free.
- Target: 8 GiB minimum GPU, worker VRAM `<= 4 GiB`.

## Fixed Isolated File Scope

Create only these source paths during implementation:

```text
spikes/terrain-diffusion-native/Cargo.toml
spikes/terrain-diffusion-native/Cargo.lock
spikes/terrain-diffusion-native/src/main.rs
spikes/terrain-diffusion-native/src/cli.rs
spikes/terrain-diffusion-native/src/preflight.rs
spikes/terrain-diffusion-native/src/protocol.rs
spikes/terrain-diffusion-native/src/export.rs
spikes/terrain-diffusion-native/src/ort_runner.rs
spikes/terrain-diffusion-native/src/projection.rs
spikes/terrain-diffusion-native/src/gates.rs
spikes/terrain-diffusion-native/src/fixtures.rs
spikes/terrain-diffusion-native/src/cancel.rs
spikes/terrain-diffusion-native/tools/uv.lock
spikes/terrain-diffusion-native/tools/requirements-hashed.txt
spikes/terrain-diffusion-native/tools/capture_pytorch_reference.py
```

`spikes/terrain-diffusion-native/` source is committed. Generated files live only below ignored `target/terrain-diffusion-native/{env,source,weights,onnx,fixtures,results}/`. Do not modify the root manifest, lockfile, application, UI, shaders, or product docs.

## Fixed Runner Contract

Commands use `rtk` and are invoked from the isolated crate. The first operation is a 20 GiB disk preflight. If ORT is not pre-staged, the sole allowed pre-probe network operation is downloading and SHA-256-verifying the pinned official ORT archive before extracting it for the runtime/CUDA probe. `preflight` then validates the exact dynamic ORT library plus compatible CUDA and cuDNN libraries; a CUDA 13.0 report alone is insufficient. Failure stops the run before source checkout, Python environment creation, checkpoint download, or export; source/checkpoint acquisition after a failed probe is forbidden.

The runner modules own: strict CLI/path validation (`cli`), dynamic library/provider proof (`preflight`), stdout JSONL v1 records (`protocol`), pinned Python invocation (`export`), CUDA-only ORT (`ort_runner`), canonical atlas/cubemap conversion (`projection`), frozen checks (`gates`), hash-bound local files (`fixtures`), and phase-bound cancellation (`cancel`). JSONL v1 is defined in the requirements document and has first-record `hello`, exactly one `done` or `error` terminal record, no stdout diagnostics, and no artifact record before finalized publication.

Phases are `preflight -> acquire -> export -> python-parity -> native-parity -> project -> validate -> report`. Exit codes are `0` all requested technical gates pass, `2` invalid CLI/path/manifest, `3` parity/determinism/resource/projection failure, `4` ORT/CUDA/cuDNN initialization or inference failure, `5` pre-acquisition disk/preflight refusal, and `130` cancellation.

## Implementation Units

### P0 — Isolated crate and protocol (archived)

Create only the listed committed spike crate and strict runner grammar. `Cargo.toml` fixes `default=[]`, `nvidia=["dep:ort"]`, and optional `ort = "=2.0.0-rc.12"` with `default-features=false` plus `std,ndarray,load-dynamic,cuda,api-24`. Emit ordered phase records; paths are relative to the spike output root. No root workspace membership or root Cargo changes.

### P1 — Runtime preflight (archived)

Probe the official GPU 1.24.4 asset and its SHA-256, `libonnxruntime.so.1.24.4`, shared/CUDA provider libraries, CUDA 12, cuDNN 9, and compatible driver before acquisition. Run `rtk ldd -r` and a CUDA-only ORT provider registration/session proof; stop on any absence or incompatibility. A CUDA 13.0 host report is insufficient; CPU fallback is forbidden.

### P2 — Pinned local acquisition/export (`NO-GO`)

After P1, require 20 GiB free disk, prepare Python 3.11.15 with committed `uv 0.11.32` lock and `--require-hashes`, pin revisions and three checkpoint SHA-256s, then export locally. Download only the approved approximately 1.1 GiB checkpoint into `.part`, hash, and atomically cache it. Record the fixed export schema and every intermediate LE-f32 fixture. No download or conversion runs before P1 passes.

The final held-out P2 gate is terminal `NO-GO`: all source/patch/checkpoint material and graph hashes matched, but base-model seed `3904243609224554624`, output channel `3` exceeded the masked relative-p99 limit (`7.176121038669759e-04 > 2.5e-04`). No outputs were promoted and P3–P5 are not pursued for this export path.

BE-003D1A is an explicit exception for local WorldClim-restricted, real-terrain evaluation only: it may retain a `NONCONFORMANT_CANDIDATE` with the same graph hashes and the held-out breach recorded. It does not alter this P2 `NO-GO` or permit P3–P5.

BE-003D1B verdict: `REAL_TERRAIN_CANDIDATE_NO_GO`. GPU ORT loads with local cuDNN, but CUDA-only session creation fails because graph nodes remain assigned to CPU while CPU fallback is prohibited. No full replay is possible under the fixed provider rule.

BE-003D1B2 verdict: `REAL_TERRAIN_CANDIDATE_NO_GO`. The user-approved hash-bound CPU bookkeeping exception allowed captured replay and the two-process propagated WorldPipeline test. All 158 captured calls completed twice, with CPU bookkeeping below 2 ms/invocation and substantive CUDA execution, but every call-level frozen gate failed. The pipeline final output was bit-exact across processes but failed the final scientific gate and exceeded the 4 GiB RSS target. This does not alter P2/provider-exception, app, scientific, or distribution `NO-GO` decisions.

BE-003D1B2 review remediation supersedes all prior replay/pipeline reports with atomically published authoritative evidence. It validates every fixture tensor and ordered schema before imports, binds exact CPU tuples to the diagnostic hash, tags model-run boundaries for max CPU timing, polls current-PID GPU memory, proves session collection/settlement, and indexes raw artifacts. Replay now demonstrates a base CPU-bookkeeping maximum of 7361 us (over the 2 ms waiver); the double-run pipeline is exact but fails both scientific and 4 GiB resource gates. Verdict remains `REAL_TERRAIN_CANDIDATE_NO_GO`.

BE-003D1B1 PyTorch reference verdict: `PYTORCH_REAL_REFERENCE_PASS`. Two independent CUDA processes exactly matched all three model call records and the final 256² LE-f32 elevation after enforcing normal source cleanliness plus source/model/stats/lock/interpreter pins before imports; semantic call identity excludes sequence ordinal, fixture bytes are independently summed from published files, and failed staging is preserved. The local fixture is input evidence for a later replay only. It does not change the ONNX, native, scientific, app, or distribution `NO-GO`/`NOT_RUN` statuses.

### P3 — Python/native parity and deterministic CUDA runner (archived)

Create hash-bound deterministic fixture requests and Python outputs. The native `ort` runner registers CUDA with `.error_on_failure()`, disables CPU fallback, records strict/deterministic settings and provider assignment/profile proving every node is CUDA, checks cancellation before submission, records peak worker VRAM, and repeats the request for exact-hash determinism. Model-free tests must compile without ORT; both test modes use `--locked`.

### P4 — Atlas and canonical cubemap projection (archived)

Generate the 2048x1024 atlas for 512 faces. Apply the frozen face equations, texel centres, lon/lat periodic lookup, polar origin/radius/angle transform, quintic 60°–75° blend, orientation/files, numeric seam/edge/corner/pole tolerances, and `H=(m-min)/(max-min)` displacement `1+0.01*H`. Preserve non-finite/range failures.

### P5 — Technical gates and evidence (archived)

Require recorded Python/native parity, exact native repeatability, `<=4 GiB` worker VRAM, orientation, 12 edge, 8 corner, finite/range, pole-cap/ring, cancellation, provider-negative-test, and SHA-256 fixture evidence. Publication atomically reserves `<run>.incomplete`, verifies canonical files/evidence, removes the marker only on success, rejects readers of incomplete runs, and retains crash residue without reuse. Missing evidence is `FAIL`.

### P6 — Result manifest and decision split (archived)

Write only the local result manifest. Set a technical decision from P1–P5; leave app integration `BLOCKED` because the Phase 5.24 procedural evaluator is `NO-GO`, and leave distribution `NO-GO` because no model/ONNX/checkpoint distribution is in scope.

### P7 — Review and cleanup (archived)

Confirm all generated materials remain ignored under `target/terrain-diffusion-native/`, no source/weight/ONNX/fixture asset entered Git, no root files changed, and docs have no implied product/scientific approval.

## Stop Conditions

- No compatible ORT/CUDA/cuDNN bundle: stop before model download/export.
- Any provenance/model/ONNX/fixture hash, I/O declaration, or measurement missing: fail evidence.
- CPU provider selected, peak worker VRAM above 4 GiB, parity/determinism failure, projection failure, or cancellation: technical `NO-GO`.

## Decision Record

| Decision | Initial status | Change authority |
|---|---|---|
| Technical native CUDA feasibility | `PENDING` | P0–P7 evidence only |
| App-integration GO | `BLOCKED` | Separate approval after the procedural scientific `NO-GO` is corrected and independently reviewed |
| Distribution GO | `NO-GO` | Separate rights/distribution review; this spike distributes nothing |

## Verification

```text
rtk git diff --check
rtk cargo check --locked --manifest-path spikes/terrain-diffusion-native/Cargo.toml
rtk cargo test --locked --manifest-path spikes/terrain-diffusion-native/Cargo.toml
rtk cargo check --locked --manifest-path spikes/terrain-diffusion-native/Cargo.toml --features nvidia
rtk cargo test --locked --manifest-path spikes/terrain-diffusion-native/Cargo.toml --features nvidia
```

The latter two commands apply only after implementation; documentation creation does not run downloads, builds, or model inference.
