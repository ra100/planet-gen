# Terrain Diffusion Native Spike Manifest

**Status:** `EXPORT_NO_GO`  
**Decision:** `NO-GO`  
**Scope:** Local isolated CUDA feasibility evidence only

> **APPROVED TO RUN AN ISOLATED NATIVE CUDA FEASIBILITY SPIKE. NOT APPROVED FOR APP/UI INTEGRATION, SCIENTIFIC SUITABILITY CLAIMS, OR MODEL DISTRIBUTION.**

## Provenance

| Field | Value | Status |
|---|---|---|
| source.repository | `https://github.com/xandergos/terrain-diffusion.git` | `VERIFIED remote` |
| source.commit | `6d770c943cf18f6732a7b15bf95f667cacf1ca17` | `VERIFIED` |
| source.tree | `12a2f7add9211b985b1db34808c081b8269f9536` | `VERIFIED` |
| source.exporter | `terrain_diffusion/onnx/export.py@6d770c943cf18f6732a7b15bf95f667cacf1ca17`, blob `affe13d4e6db6d461230ff232caecf3119286cbc` | `VERIFIED` |
| source.requirements | `requirements.txt@6d770c943cf18f6732a7b15bf95f667cacf1ca17`, blob `35d7711000cddc7375d7a3ca0e81d696c4dd110d` | `VERIFIED` |
| checkpoint.repository | `https://huggingface.co/xandergos/terrain-diffusion-90m` | `ACQUIRED` |
| checkpoint.revision | `2bb1a93141140040091d44f0770d4ba22c8cf145` | `VERIFIED` |
| runtime | `ort = "=2.0.0-rc.12"`, feature `nvidia`, dynamic path, CUDA-only | `CUDA provider registration PASS; all-node profile NOT_RUN until P3` |
| python | managed CPython `3.11.15` | `ACQUIRED hash-locked environment` |
| ort.asset | `onnxruntime-linux-x64-gpu-1.24.4.tgz` | `ACQUIRED` |
| ort.asset.url | `https://github.com/microsoft/onnxruntime/releases/download/v1.24.4/onnxruntime-linux-x64-gpu-1.24.4.tgz` | `ACQUIRED` |
| ort.asset.sha256 | `c5f804ff5d239b436fa59e9f2fb288a39f7eb9552f6a636c8b71e792e91a8808` | `VERIFIED` |
| cudnn.asset | `nvidia-cudnn-cu12==9.25.0.15`, `nvidia_cudnn_cu12-9.25.0.15-py3-none-manylinux_2_27_x86_64.whl` | `ACQUIRED` |
| cudnn.asset.url | `https://files.pythonhosted.org/packages/83/94/1e9882d2d4307560197881069dee9e4050cea8384ae77b330e1f8f722fdf/nvidia_cudnn_cu12-9.25.0.15-py3-none-manylinux_2_27_x86_64.whl` | `PINNED PyPI metadata` |
| cudnn.asset.sha256 | `4ea1ba443fa28ac6cf04b7a44a107dfd54cf355c2324938102ddb21778ab10ce` | `VERIFIED` |
| cudnn.asset.bytes | `751250833` | `VERIFIED` |
| cudnn.license | User explicitly accepted NVIDIA cuDNN license for this local non-commercial spike in this session via `--accept-nvidia-license` | `CONFIRMED` |
| python.tools | `uv 0.11.32`, committed hash-locked `tools/uv.lock` | `VERIFIED` |

## Execution Environment

| Field | Recorded host value | Target/minimum |
|---|---|---|
| GPU | RTX 4090, 24 GiB | 8 GiB minimum |
| Driver | `580.173.02` | — |
| Reported CUDA | `13.0` | Compatibility must be proven against selected `rc.12` ORT/CUDA/cuDNN libraries |
| Rust | `rustc 1.97.1` | — |
| Python | `3.11.15` | managed export version |
| Free disk | 83 GiB | — |
| Worker VRAM | `NOT_RUN` | `<= 4 GiB` |

The host probe describes execution only. It does not certify a target minimum or ORT bundle compatibility.

## Required Local Artifact Hashes

Every item is SHA-256 and starts `NOT_RUN`; a missing hash fails the associated gate.

| Artifact | SHA-256 | Status |
|---|---|---|
| pinned source checkout | `NOT_RUN` | `NOT_RUN` |
| checkpoint files | `config.json`, `base_model/config.json`, `coarse_model/config.json`, `decoder_model/config.json`, and the three pinned weights | `VERIFIED` |
| exported ONNX graphs | `NOT_RUN` | `NOT_RUN` |
| exporter request and I/O declaration | `NOT_RUN` | `NOT_RUN` |
| Python fixture tensors/outputs | `target/terrain-diffusion-native/pytorch-real-reference/manifest.json` | `PYTORCH_REAL_REFERENCE_PASS` |
| native fixture tensors/outputs | `NOT_RUN` | `NOT_RUN` |
| atlas | `NOT_RUN` | `NOT_RUN` |
| `posx.f32le`, `negx.f32le`, `posy.f32le`, `negy.f32le`, `posz.f32le`, `negz.f32le` faces | `NOT_RUN` | `NOT_RUN` |
| result/progress log | `NOT_RUN` | `NOT_RUN` |
| checkpoint.base | `e426277db86517335d4b0bc02b3d456bb812b2a8726ea010635686f77373be36`; `1014772076` bytes | `VERIFIED` |
| checkpoint.coarse | `13c21db4581d2072db56fd76fe3b92fa2d5efa8805be2aae0b63448b5d28ac5c`; `11200936` bytes | `VERIFIED` |
| checkpoint.decoder | `b6c7fa99f836ad75c514236c9529e18a68ea207ed59dd39fd1341fc9a8a03bcc`; `111709108` bytes | `VERIFIED` |

## Measurement Gates

| Gate | Status | Required evidence |
|---|---|---|
| ORT/CUDA/cuDNN preflight | `PASS` | pinned cuDNN 9.25.0.15 local wheel extracted under `runtime/deps/cudnn`; probe-only `LD_LIBRARY_PATH` resolves CUDA 12 and cuDNN 9; CUDA provider registration succeeds |
| Pinned export and ONNX I/O | `NO-GO` | Final held-out P2 gate failed; this export path is terminal and outputs were not promoted |
| Python/native parity | `NOT_RUN` | hash-bound fixture request and per-tensor comparison |
| Native determinism | `NOT_RUN` | same request, separate native runs, exact output hashes |
| Resource | `NOT_RUN` | peak worker VRAM `<= 4 GiB`, host telemetry |
| Cancellation | `NOT_RUN` | terminal `E_CANCELLED` phase/submission cancellation record and exit `130` |
| Projection | `NOT_RUN` | 2048x1024 atlas, 512 faces, periodic seams, 60°–75° blend, orientation, 12 edges, 8 corners |
| Height mapping | `NOT_RUN` | finite range, normalized height, canonical cubemap displacement |
| Procedural scientific control | `NO-GO` | existing Phase 5.24 normal/pole gate failure; not changed by this spike |
| Model-free locked build | `PASS` | `rtk cargo test --locked --manifest-path spikes/terrain-diffusion-native/Cargo.toml`: 11 passed |
| NVIDIA locked build | `PASS` | `rtk cargo test --locked --manifest-path spikes/terrain-diffusion-native/Cargo.toml --features nvidia`: 11 passed |
| CUDA provider assignment | `BOOTSTRAP_PASS` | session-options CUDA registration uses `.error_on_failure()` with no CPU provider; all-node profile remains `NOT_RUN` until P3 model graph exists |
| Negative CPU fallback | `NOT_RUN` | absent CUDA registration and forced CPU assignment both fail |
| Atomic worker publication | `NOT_RUN` | reserved `.incomplete`, verified evidence, atomic marker removal, crash/retry record |
| Disk/download safety | `PASS` | 20 GiB pre-network check passed; `205429115` byte `.part` was SHA-256 verified and atomically renamed |
| JSONL v1 protocol | `PASS` | model-free tests freeze exit mapping and ordered serialized record schema; successful probe emitted hello, four ordered progress records, then bootstrap-only done |
| Bootstrap ordering | `PASS` | disk gate, user-license-gated pinned cuDNN acquisition, and CUDA registration completed before any source/env/checkpoint activity |
| Finalized artifact publication | `NOT_RUN` | artifact paths emitted only after `.incomplete` removal and evidence verification |
| Restricted PyTorch real reference | `PASS` | two independent CUDA processes exactly matched all call records and final LE-f32 elevation; no ONNX/native/scientific claim |

## Protocol and Outputs

Stdout protocol is JSONL v1: UTF-8 RFC 8259 objects only, first `hello`, ordered lifecycle-guarded `progress`/post-publication `artifact` records, exactly one `done` or `error` terminal record, and stderr-only diagnostics. Bootstrap permits only `validate_bundle` then `load_runtime`, both represented as `preflight` progress; future `run` permits the frozen `preflight`, `acquire`, `export`, `python-parity`, `native-parity`, `project`, `validate`, `report` order. A stage may emit `0/1` before work and `1/1` after work; skips, regressions, repeat-after-complete, invalid totals, and post-terminal records fail. Bootstrap `done` adds ordered `scope:"bootstrap"`, `result:"PASS"`, and `technical:"NOT_RUN"`; it is never a technical-feasibility PASS. `artifact` remains `{"v":1,"event":"artifact","seq":...,"kind":...,"path":...,"sha256":...,"bytes":...}` and is rejected before finalized publication. `E_INPUT`, `E_GATE`, `E_RUNTIME`, `E_PREFLIGHT`, and `E_CANCELLED` map respectively to `2`, `3`, `4`, `5`, and `130`; cancellation sends `SIGTERM`, waits two seconds, then `SIGKILL`.

Bootstrap evidence begins with the 20 GiB disk gate. The manifest records whether the pinned digest-verified ORT archive was pre-staged or locally acquired as the only allowed pre-probe network transfer, followed by probe status. A failed probe records `E_PREFLIGHT` and forbids source clone, Python environment creation, and checkpoint acquisition.

Committed source is limited to `spikes/terrain-diffusion-native/`, including its `Cargo.lock` and hashed Python tools lock. All generated material remains under ignored `target/terrain-diffusion-native/`. No checkpoint, ONNX, source checkout, fixture, result, or model-derived asset is distributed or committed.

## Decisions

| Decision | Status | Evidence requirement |
|---|---|---|
| Technical native CUDA feasibility | `NO-GO` | final held-out P2 export gate failed; no further seed set or threshold change is permitted |
| App integration | `BLOCKED` | separate scientific/app approval after the current procedural `NO-GO` is resolved |
| Distribution | `NO-GO` | separate rights/distribution approval; excluded from this spike |

## Bootstrap Evidence

- Archive: `target/terrain-diffusion-native/onnxruntime-linux-x64-gpu-1.24.4.tgz`; `205429115` bytes; SHA-256 `c5f804ff5d239b436fa59e9f2fb288a39f7eb9552f6a636c8b71e792e91a8808`.
- User explicitly accepted the NVIDIA cuDNN license for this local non-commercial spike. PyPI package `nvidia-cudnn-cu12==9.25.0.15` was downloaded as `runtime/deps/nvidia_cudnn_cu12-9.25.0.15-py3-none-manylinux_2_27_x86_64.whl`, `751250833` bytes, SHA-256 `4ea1ba443fa28ac6cf04b7a44a107dfd54cf355c2324938102ddb21778ab10ce`, and extracted into `runtime/deps/cudnn/nvidia/cudnn/lib/`.
- Probe-only `LD_LIBRARY_PATH` was ORT lib + local cuDNN lib + `/usr/lib/x86_64-linux-gnu`. `ldd -r` resolves CUDA 12 and `libcudnn.so.9` for all three required ORT libraries. The CUDA plugin alone reports `undefined symbol: Provider_GetHost`; ORT resolves that bridge symbol when session-options CUDA registration runs successfully.
- Local feature-gated probe emitted six JSONL records and exit `0`: `hello` seq `0`; `validate_bundle` progress `0/1` seq `1`; `validate_bundle` progress `1/1` seq `2`; `load_runtime` progress `0/1` seq `3`; `load_runtime` progress `1/1` seq `4`; bootstrap `done` seq `5` with `technical:"NOT_RUN"`. No source checkout, Python environment, checkpoint, ONNX graph, or fixture was acquired.

**Final decision: `NO-GO`. P1 bootstrap and pinned checkpoint acquisition pass, but the final held-out P2 gate failed. P3–P5 are not pursued for this export path.**

## BE-003B1 Export Contract

The corrected authoritative source commit, tree, and blobs above replaced the unavailable prior pin. The model revision is `xandergos/terrain-diffusion-90m@2bb1a93141140040091d44f0770d4ba22c8cf145`, MIT and ungated. Export remains CPU-only and fail-closed; parity fixtures and all-node CUDA profiling remain pending BE-003B2/P3.

## Compatibility Patch Evidence

- Patch: `spikes/terrain-diffusion-native/tools/patches/onnx-conv2d-padding-int.patch`; SHA-256 `2988d503bac0f42774d4b16cd925cca20f99df1f32422995159eb16d8803b5ac`.
- The wrapper confirms pristine commit/tree/exporter/requirements blobs and a clean worktree, uses `git apply --check`, then requires patched exporter blob `4d67fe62429e15d89f485502def4d4293c3ad726` before exporting.
- The focused eager/traced check covers 1×1 padding 0, 3×3 padding 1, grouped 3×3 convolution, and `MPConv(no_padding=True)` with exact equality.
- A fresh temporary directory produced exactly `coarse_model.onnx`, `base_model.onnx`, and `decoder_model.onnx`. Each passed path-based ONNX checker/full structural Conv-pad validation, CPU ORT load, and deterministic finite zero-input execution.
- Final held-out evidence is retained locally at `target/terrain-diffusion-native/export-matrix.json`; temporary graphs were deleted. The exact material SHA-256 matched `43524ff251b67f16ae2c5e4893f74ba2d65783a9c92ae87e7bce1010711bcdd1`; counters 0–7 produced the prescribed seed list; all graph hashes matched calibration evidence. Coarse and decoder passed, but base failed seed `3904243609224554624` in output channel `3`: masked relative p99 `7.176121038669759e-04` exceeded the frozen `2.5e-04` limit. Whole-output and batch-element gates passed. This is the terminal technical `NO-GO` for this export path; no third seed set, threshold change, or automatic exporter change is allowed.
- The failed temporary output was deleted, `target/terrain-diffusion-native/models/` remains empty, and the source worktree was restored to exporter blob `affe13d4e6db6d461230ff232caecf3119286cbc` with an empty `git status --short`.

## BE-003D1A Restricted Real-Terrain Candidate

- User approved WorldClim local non-commercial use only. Official response identity: `49869449` bytes, SHA-256 `00513224583665ec0f2f955a4ec252730c4deb2004cce9e793492a3f26df4dcf`; headers are frozen locally in `target/terrain-diffusion-native/worldclim-observed-identity.json`.
- Restricted local stats: `source/data/global/synthetic_map_stats.json`, `12205` bytes, SHA-256 `0d2578c765a3cc4d21b58994fcd40d0dc0f657b41ad67ab6a5fe45cf6db535e4`; schema is 64 quantiles, 10 tables, and four finite scalar fields. Parameters are `frequency_mult=[1,1,1,1,1]`, `seed=1`, `drop_water_pct=0.5`.
- The WorldClim ZIP and all 19 extracted TIFFs were deleted after validation; ETOPO remains. WorldClim-derived data is local, restricted, and not distributed.
- Nonconformant candidate: `target/terrain-diffusion-native/models/nonconformant/ea8f36513df458ef87fbc6a52c4daf864d2527fcf22be8937f7b2d660b3b559b/`; status `NONCONFORMANT_CANDIDATE`, distribution `NO_GO`, real-terrain evaluation only. Graph hashes match calibration: coarse `764977a99931172c41b908b59b8095acc4c80f16c8ce7e54b8c3a22f16dd85e0`, base `9448d4931bd84edf23bff668e202177e3cb4d2907a5d695349cbb303e9de339a`, decoder `a5783c3708a53adf4b492e1b9c09919a6dec0ae07cfa25fd7231fb02ef0a37fe`.
- This override does not change P2 `NO-GO`, technical feasibility, app integration, or distribution decisions.

## BE-003D1B CUDA Replay Gate

- `onnxruntime-gpu==1.24.4` was hash-locked into the isolated environment. With local cuDNN 9 and Torch CUDA libraries on `LD_LIBRARY_PATH`, CUDA EP loaded but `ORT_ENABLE_ALL` session creation with `session.disable_cpu_ep_fallback=1` failed: graph nodes were assigned to the default CPU EP.
- This makes CUDA-only replay and the requested WorldPipeline adapter impossible without relaxing the explicit provider rule. No reference fixtures, real-call replay, profile claim, or scientific elevation comparison was run. Verdict: `REAL_TERRAIN_CANDIDATE_NO_GO` with preflight report `target/terrain-diffusion-native/real-terrain-cuda-preflight.json`.
- Contained fallback-enabled diagnostic evidence is `target/terrain-diffusion-native/candidate-ep-diagnostic.json`. CUDA executed the substantive model work; CPU profile events were Gather/Unsqueeze/Slice/Concat/Cast and scalar Add/Div/Sqrt/Mul bookkeeping. No CPU Conv, MatMul, normalization, or activation kernel was observed. Diagnostic-only timing: coarse CPU/CUDA `220/17827` µs, base `1614/282720` µs, decoder `1549/83225` µs over two runs. Optimized/fused profile events cannot prove one-to-one original-node coverage or tensor-copy byte counts.
- Provider-exception certification did not pass: over one warmup plus five measured runs, decoder CPU/CUDA time was `695.17/17444.83` µs per invocation (3.83% CPU share), exceeding the fixed 3% limit. Candidate remains blocked.

## BE-003D1B1 PyTorch Real Reference

- `tools/capture_pytorch_reference.py` ran the pinned fp32 CUDA `WorldPipeline` twice in independent Python processes with seed `1`, direct cache, `latents_batch_size=1`, `torch_compile=false`, `with_climate=false`, and bbox `[0,0,256,256)`.
- Before imports, both runs enforced the pinned source commit/tree/exporter/requirements blobs, every model/config/stats hash, committed Python lock/requirements hashes, and the exact managed interpreter. The pin-set SHA-256 is `e909cdaa0379a58732ed33850a4a018acddf5da2987fd40b512fe4c18482d6d7`.
- Both runs first passed normal source `git status --porcelain` cleanliness, then matched every semantic identity (which excludes sequence ordinal), feed/output hash, and exact final elevation bytes. The published local manifest status is `PYTORCH_REAL_REFERENCE_PASS`; it records 80 coarse, 74 base, and 4 decoder calls, three scheduler/window records, and canonical LE-f32 fixtures totaling `59853768` bytes. The independently summed published `.f32` files also total `59853768` bytes. Existing evidence was atomically archived under `target/terrain-diffusion-native/pytorch-real-reference.archived.1785219650395973126` before publication.
- Final elevation is finite, shape `256x256`, range `[-2061.1513671875, -33.5768928527832]`, SHA-256 `990f8e0ded02f2cde99727a9ce4a22fa88168c221cbe74849bae1dbeb91532e0`. First-process wall time was `4.096386466997501` seconds; sampled peak RSS was `1782415360` bytes and peak CUDA allocation was `2311734784` bytes.
- The fixture remains local/restricted. It supplies no ONNX replay, native parity, scientific suitability, app integration, or distribution approval.

## BE-003D1B2 Captured Replay and Propagated Candidate

- `tools/replay_real_terrain_candidate.py` validates the pinned source, weights, stats, lockfiles, reference fixture, candidate manifest, graph hashes, managed interpreter, and pinned ORT/cuDNN library paths before importing model or ORT modules. It uses `ORT_ENABLE_ALL`, CUDA then CPU providers, one active session, profile hashing, exact repeated-call checks, and an exact graph-node-bound CPU bookkeeping allowlist (`Gather`, `Unsqueeze`, `Slice`, `Concat`, `Cast`, `Add`, `Div`, `Sqrt`, `Mul`). CPU time is an absolute `<=2ms/invocation` gate; CPU share is recorded only.
- Superseded replay observations are non-authoritative. The graph-pooled errors and current replay evidence are retained only in the authoritative replay directory listed below.
- The propagated WorldPipeline adapter replaced the three loaded PyTorch model modules without modifying pinned source. Two independent runs produced exact elevation SHA-256 `1fe9fb8138cf52397e94f632167c2bff8ac30b8e403a035ea01a40851205b025` and exact call identity/feed/output SHA-256 `45df681febc0cadb7b374ab9d34208a0ac63c75c9ab5a53b201aba0f4ed256eb`; each used 80 coarse, 74 base, 4 decoder calls, three session loads, and two switches.
- Superseded pipeline observations are non-authoritative. Final metrics and resource evidence are retained only in the authoritative pipeline directory listed below.
- Verdict: `REAL_TERRAIN_CANDIDATE_NO_GO`. This preserves the P2/provider exception, scientific, app-integration, and distribution `NO-GO` decisions.

## BE-003D1B2 Review Remediation

- Prior runs are provisional and non-authoritative. The authoritative immutable evidence is `target/terrain-diffusion-native/real-terrain-candidate-replay.authoritative-3/` (index SHA-256 `81303e8b6f53f82c67ebf3649561d831b5fd67611659284e40e4b08ebe18830e`) and `target/terrain-diffusion-native/real-terrain-candidate-pipeline.authoritative-3/` (index SHA-256 `275e16b01aabb8bbc1bb3283e916cabc910a05e92efa2a140609d480c649565b`).
- The authoritative runner SHA-256 is `7c415e3dae78056ad22ccb41b40da8913edc5ed2e43efa6db714af0e13eb245c`; it records the accepted fixture-manifest SHA-256 `c10d628d4bbd8ac524ad4b9c3a00a2abe4493f7252b5451bc52dcc138081973c`, graph pins, diagnostic SHA-256, command, timestamps, `LD_LIBRARY_PATH`, lock pins, ordered ONNX schemas, canonical tensor-set validation, and profile hashes before atomic publication.
- Replay again completed 80 coarse, 74 base, and 4 decoder calls, each repeated exactly. All 158 frozen call gates failed. Exact diagnostic-hash-bound CPU tuples were complete with no forbidden CPU or unknown provider events, but base CPU bookkeeping reached `7361us` in a single model-run boundary, exceeding the user-authorized `<=2000us` absolute maximum. Thus replay is `CAPTURED_REPLAY_FAIL`.
- Both propagated runs were byte-identical: elevation SHA-256 `1fe9fb8138cf52397e94f632167c2bff8ac30b8e403a035ea01a40851205b025`; call identity/feed/output SHA-256 `45df681febc0cadb7b374ab9d34208a0ac63c75c9ab5a53b201aba0f4ed256eb`. Each held one live ORT session, loaded three graphs, switched twice, proved collection and 64 MiB-settled GPU memory before every switch, and retained raw profiles. CPU bookkeeping maxima were all below 2 ms; no forbidden provider event occurred.
- Process-level `nvidia-smi` polling every 50ms measured `4819255296` bytes peak GPU memory (59/57 samples, no misses) and RSS `6164807680`/`6163398656` bytes, exceeding the 4 GiB target. Final scientific metrics remain failed: normalized max `5.332542581e-4`, MAE `0.2084533572`, RMSE `0.2753341581`, p99 `0.6166992188`, bias `0.2009986839`, correlation `0.9999999242`; finite range `[-2061.12060546875, -33.539154052734375]`.
- Final verdict remains `REAL_TERRAIN_CANDIDATE_NO_GO`; P2/provider-exception, scientific, app, and distribution decisions remain unchanged.
