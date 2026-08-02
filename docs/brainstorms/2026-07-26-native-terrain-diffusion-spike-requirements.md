# Native Terrain Diffusion CUDA Feasibility Spike Requirements

**Date:** 2026-07-26  
**Status:** Approved to run an isolated spike  
**Architecture:** GEN-006, GEN-007, GEN-008  
**Depends on:** Phase 5.24's current procedural evaluator result

> **APPROVED TO RUN AN ISOLATED NATIVE CUDA FEASIBILITY SPIKE. NOT APPROVED FOR APP/UI INTEGRATION, SCIENTIFIC SUITABILITY CLAIMS, OR MODEL DISTRIBUTION.**

## Problem Frame

Determine whether the pinned 90 m Terrain Diffusion checkpoint can be exported once by a maintainer and invoked through native Rust CUDA inference within the fixed resource and cubemap-projection contract. This is a local, disposable feasibility experiment, not a product terrain source.

The current procedural evaluator is `NO-GO`: frozen normal and pole gates fail. Therefore technical success here cannot approve scientific suitability or app integration.

## Scope

- Create and run only a separate `spikes/terrain-diffusion-native/` crate/process. Root `Cargo.toml`, `Cargo.lock`, `src/`, shaders, app, UI, and normal `cargo` commands remain untouched.
- Use `ort = "=2.0.0-rc.12"` with opt-in `nvidia`, a dynamically discovered ONNX Runtime library path, CUDA execution only, and no CPU fallback.
- Before any checkpoint download or Python export, probe the exact ORT/CUDA/cuDNN library set selected by `rc.12`; stop with recorded failure when it is unavailable. The host's reported CUDA 13.0 is not evidence that this bundle is compatible.
- Permit a maintainer-only, pinned Python 3.11.15 export environment. Users never install or run Python.
- Download and convert only into ignored `target/terrain-diffusion-native/`; do not commit, distribute, or cache checkpoints, ONNX, generated fixtures, environments, or results.
- Generate 512x512 cubemap faces through a 2048x1024 planar atlas, then apply the frozen periodic-seam and 60°–75° polar-blend projection.
- Measure native/Python parity, determinism, resource use, cancellation, and projection gates. Missing evidence is a failure.

## Frozen Provenance and Artifact Contract

| Item | Pin |
|---|---|
| Upstream source | `xandergos/terrain-diffusion@6d770c943cf18f6732a7b15bf95f667cacf1ca17` |
| Exporter | `terrain_diffusion/onnx/export.py` and `requirements.txt` at that source pin |
| Checkpoint | `xandergos/terrain-diffusion-90m@2bb1a93141140040091d44f0770d4ba22c8cf145` |
| Runtime | `ort = "=2.0.0-rc.12"`, feature `nvidia`; dynamic ORT path selected by preflight |
| Python | `3.11.15`, maintainer-only under `target/terrain-diffusion-native/env/` |
| ONNX | Exported locally from the pinned source/checkpoint; every produced model SHA-256 is required evidence |

The exporter must record each ONNX graph's exact input names, shapes, dtypes, output names, dynamic axes, exported filename, and SHA-256 in the manifest before native execution. The Rust runner accepts only that recorded I/O contract; it must not guess, transpose, coerce, or silently replace an input/output.

The fixture bundle is local-only and contains the exporter request, deterministic noise/conditioning tensors, expected Python outputs, native outputs, atlas, six canonical faces (`posx`, `negx`, `posy`, `negy`, `posz`, `negz`), and SHA-256 for every file. No fixture hash may be omitted or inferred.

### BE-003D1B1 PyTorch Reference Amendment

The restricted candidate may additionally produce a local PyTorch CUDA reference only: bbox `[0,0,256,256)`, `with_climate=false`, fp32, seed `1`, `latents_batch_size=1`, `torch_compile=false`, and direct cache. Before imports it must enforce normal source `git status --porcelain` cleanliness plus source commit/tree/exporter/requirements blobs, all model/config/stats hashes, committed lock/requirements hashes, and the managed interpreter. It wraps the three pinned models without source edits, preserves canonical LE-f32 feeds/outputs below 100 MiB, independently sums published `.f32` fixture files and requires the reported total to match, uses a semantic call identity excluding the sequence ordinal, rejects duplicate/empty/unsupported outputs and non-finite/mutated inputs, and requires a separate process to match every call record and final elevation byte-for-byte. Staging is unique and preserved on failure; replacement archives existing evidence explicitly. This is ONNX-replay input evidence only, not parity, scientific, app, or distribution approval.

### QA-007 Runtime and Reproducibility Amendment

The committed isolated crate has its own `Cargo.toml` and `Cargo.lock`. Its feature model is exactly `default = []` and `nvidia = ["dep:ort"]`; `ort` is optional and exactly `=2.0.0-rc.12` with `default-features = false` and features `std`, `ndarray`, `load-dynamic`, `cuda`, and `api-24`. Model-free tests compile without ORT. NVIDIA commands are `rtk cargo check --locked --manifest-path spikes/terrain-diffusion-native/Cargo.toml --features nvidia` and `rtk cargo test --locked --manifest-path spikes/terrain-diffusion-native/Cargo.toml --features nvidia`; model-free commands omit `--features nvidia`.

P1 uses only the official `onnxruntime-linux-x64-gpu-1.24.4.tgz` release asset: `https://github.com/microsoft/onnxruntime/releases/download/v1.24.4/onnxruntime-linux-x64-gpu-1.24.4.tgz`, SHA-256 `c5f804ff5d239b436fa59e9f2fb288a39f7eb9552f6a636c8b71e792e91a8808`. Required sonames are `libonnxruntime.so.1.24.4`, `libonnxruntime_providers_shared.so`, and `libonnxruntime_providers_cuda.so`. P1 runs `rtk ldd -r` on each provider, verifies CUDA major 12, cuDNN major 9, and an NVIDIA driver compatible with CUDA 12, then opens a CUDA-only ORT session. Any unresolved library, incompatible major, provider registration failure, or absent `CUDAExecutionProvider` stops before network acquisition. Reported CUDA 13.0 is never accepted by itself.

The committed `spikes/terrain-diffusion-native/tools/uv.lock` is generated by `uv 0.11.32`; its installer uses `uv pip install --require-hashes -r requirements-hashed.txt`. Python is exactly `3.11.15`. P2 refuses an absent/mismatched lock, source revision, package hash, checkpoint revision, or checkpoint asset hash. The checkpoint SHA-256 values are `base_model/diffusion_pytorch_model.safetensors=e426277db86517335d4b0bc02b3d456bb812b2a8726ea010635686f77373be36`, `coarse_model/diffusion_pytorch_model.safetensors=13c21db4581d2072db56fd76fe3b92fa2d5efa8805be2aae0b63448b5d28ac5c`, and `decoder_model/diffusion_pytorch_model.safetensors=b6c7fa99f836ad75c514236c9529e18a68ea207ed59dd39fd1341fc9a8a03bcc`.

Before any network request, P1 checks at least 20 GiB free under `target/terrain-diffusion-native/`; insufficient space exits `5`. The user-approved approximately 1.1 GiB checkpoint download writes `*.part`, verifies the frozen SHA-256, then atomically renames into the local cache. Failed `.part` files are removed; mismatches are never reused.

## Frozen Projection and Height Contract

- The source atlas is exactly 2048x1024 for 512x512 faces: four horizontal 512-pixel tiles by two vertical 512-pixel tiles, sampled with periodic atlas wrap in both axes.
- Face sampling is texel-centred. Neighbours crossing a tile edge use periodic atlas coordinates before cubemap extraction; seams are never repaired by post-hoc edge copying.
- In the polar caps, use the periodic result below 60°, the polar mapping above 75°, and the exact quintic blend specified below between them. The same rule applies to both hemispheres.
- Canonical face order is `posx`, `negx`, `posy`, `negy`, `posz`, `negz`; orientation follows the existing Phase 5.24 cubemap contract. The spike must emit its orientation fixture and fail rather than reinterpret a face.
- Convert finite model elevation `m` to normalized height `H=(m-min)/(max-min)` using the recorded fixture range, retaining out-of-range values. Fail when the span is `<= 1e-6` or a value is non-finite. Sphere displacement is `cube_to_sphere(face,u,v) * (1 + 0.01 * H)`.

### Complete Projection Contract

For texel centres, `a=2*(x+0.5)/N-1` and `b=2*(y+0.5)/N-1`, with `N=512`. The unnormalised cube vectors are `posx=(1,-b,-a)`, `negx=(-1,-b,a)`, `posy=(a,1,b)`, `negy=(a,-1,-b)`, `posz=(a,-b,1)`, and `negz=(-a,-b,-1)`; `cube_to_sphere(face,a,b)=normalize(vector)`. Canonical output order/files are `posx.f32le`, `negx.f32le`, `posy.f32le`, `negy.f32le`, `posz.f32le`, `negz.f32le`.

For normalized direction `(X,Y,Z)`, `lon=atan2(Z,X)`, `lat=asin(clamp(Y,-1,1))`, `u=fract(lon/(2*pi)+0.5)`, and `v=clamp(0.5-lat/pi,0,1)`. Atlas lookup is bilinear at `(u*2048-0.5, v*1024-0.5)` with longitude modulo 2048 and latitude clamped. The polar transform is `r=(pi/2-abs(lat))/(pi/2)`, `theta=atan2(Z,X)`, `polar_u=0.5+0.5*r*cos(theta)`, `polar_v=0.5-0.5*r*sin(theta)`; it uses the same periodic horizontal atlas lookup. `w=t*t*t*(t*(t*6-15)+10)` where `t=clamp((abs(lat)-60°)/15°,0,1)`, and output is `(1-w)*periodic+w*polar`. This is the required 60°–75° quintic blend, not a repair pass.

Numeric gates are: orientation exact; each of 12 edge paired-height max `<=1e-5`; each of 8 three-face corner spans `<=1e-5`; seam p95 `<=5e-6`; finite/range exact; polar cap/ring normalized-height p95 `<=2e-3`; and polar slope p95 `<=2°`. All comparisons occur on the recorded LE-f32 fixtures before any visualization.

### Frozen Export and Parity Fixture Schema

The exporter invokes upstream functions in this exact order: `portable_rng.PCG64` -> `random_sampler.marsaglia` -> `WorldPipeline` conditioning -> coarse base pass -> fine base pass -> decoder -> `onnx.export`. Seed derivation is `pcg_seed=seed`, `tile_seed=PCG64(seed).advance(tile_y*atlas_tiles_x+tile_x)`, and tile coordinates are row-major `(tile_y,tile_x)`. It uses 20 EDM/DPM steps with `sigma_i=(80^(1/7)+(i/19)*(0.002^(1/7)-80^(1/7)))^7`, `sigma_next` from `i+1`, `c_skip=1/(sigma_i^2+1)`, `c_out=-sigma_i/sqrt(sigma_i^2+1)`, `c_in=1/sqrt(sigma_i^2+1)`, and `c_noise=ln(sigma_i)/4`; no scheduler constant may be recomputed differently by Rust.

Coarse and fine base passes are both required. Tiles are 512x512, stride 384, row-major, with separable Hann window `0.5-0.5*cos(2*pi*i/511)`, normalized by accumulated weights; cache keys are `(pass,tile_y,tile_x,step)`. The conditioning vector is exactly 58 LE-f32 values serialized in exporter order: seed, atlas/tile coordinates, both pass identifiers, then the upstream `WorldPipeline` conditioning tensor in declared order; its names and values are written to metadata. Laplacian boundaries are reflect-101, upsample is bilinear `align_corners=false`, and decode follows the pinned decoder graph.

For every request the exporter serializes raw LE-f32 plus `{name,shape,dtype,byte_len,sha256}` for PCG/Marsaglia noise, conditioning-58, every step input/output, both base passes, overlap accumulators/weights, Laplacian, upsample, decoder input/output, atlas, and canonical faces. Resolved constants are included in the same metadata. Python/native tensor tolerance is absolute `1e-5`, relative `1e-5`; final atlas/face values are exact LE-f32 bytes; any mismatch fails.

## Protocol

### Stdout JSONL v1

Stdout is UTF-8 JSON Lines: exactly one RFC 8259 JSON object followed by one LF per line, with no BOM, blank lines, pretty printing, or stdout text outside these records. Diagnostics are stderr only. Serialization is compact JSON and keys appear in the listed order; strings use RFC 8259 JSON escaping (`"`, `\\`, `\b`, `\f`, `\n`, `\r`, `\t`, or lowercase `\u00xx`), never locale formatting. Every path is normalized relative to the finalized run directory.

The first record is exactly one `hello`; `seq` starts at `0` and increments by one on all later records. `phase` is one of `preflight`, `acquire`, `export`, `python-parity`, `native-parity`, `project`, `validate`, or `report`. Progress has `total>0`, `0<=completed<=total`, and nondecreasing `completed` within a phase.

```json
{"v":1,"event":"hello","seq":0,"run_id":"<ascii-run-id>","pid":<u32>,"output_root":"<relative-path>"}
{"v":1,"event":"progress","seq":<u64>,"phase":"<phase>","completed":<u64>,"total":<u64>,"message":"<json-string>"}
{"v":1,"event":"artifact","seq":<u64>,"kind":"<source|onnx|fixture|atlas|face|evidence|result>","path":"<relative-path>","sha256":"<64-lowercase-hex>","bytes":<u64>}
{"v":1,"event":"error","seq":<u64>,"code":"<E_INPUT|E_GATE|E_RUNTIME|E_PREFLIGHT|E_CANCELLED>","category":"<input|gate|runtime|preflight|cancelled>","retriable":<true|false>,"message":"<json-string>"}
{"v":1,"event":"done","seq":<u64>,"technical":"PASS","app_integration":"BLOCKED","distribution":"NO_GO","result_dir":"<relative-path>"}
```

`artifact` records may appear only after all canonical files (`posx.f32le`, `negx.f32le`, `posy.f32le`, `negy.f32le`, `posz.f32le`, `negz.f32le`), hashes, and evidence are verified and `.incomplete` is removed. Exactly one terminal record occurs: successful runs emit `done` then exit `0`; all failures emit `error` then their mapped exit; no records follow a terminal record. `E_INPUT/input` maps to `2` and is not retriable; `E_GATE/gate` maps to `3` and is not retriable; `E_RUNTIME/runtime` maps to `4` and is retriable; `E_PREFLIGHT/preflight` maps to `5` and is retriable; `E_CANCELLED/cancelled` maps to `130` and is retriable. Cancellation emits only terminal `E_CANCELLED`, never `done` or `artifact`.

## Bootstrap Order

The first operation is the 20 GiB disk preflight. If ORT is already pre-staged, verify its pinned archive digest and extract it; otherwise the only permitted pre-probe network operation is acquiring and SHA-256-verifying the pinned official ORT archive, then extracting it under the run runtime directory. Run the runtime/CUDA compatibility probe next. If it fails, stop before cloning source, creating the Python environment, or downloading the approximately 1.1 GiB checkpoint. Source or checkpoint acquisition after a failed probe is forbidden.

Exit `0` means all requested technical gates passed; `2` means CLI/path/manifest input failure; `3` means a parity, determinism, resource, or projection gate failed; `4` means ORT/CUDA/cuDNN initialization or inference failure; `5` means pre-acquisition disk/preflight refusal; `130` means cancellation. A missing library, model, hash, or measurement is `FAIL`, never a fallback or pass.

## Worker Publication and CUDA Exclusivity

The worker atomically reserves a unique final result directory by creating `<run>.incomplete`; an existing final or incomplete name is rejected. RAII cleanup removes handled failed/cancelled directories. It writes canonical faces, metadata, hashes, provider assignment/profile, and evidence under the incomplete directory, verifies every required file, then removes `.incomplete` as the final atomic publication step. Readers reject any directory containing `.incomplete`; crash residue is retained for diagnosis but never reused, and a retry reserves a new run id.

Cancellation sends `SIGTERM`, waits two seconds, then sends `SIGKILL`; phase checkpoints occur before acquisition, each export graph, each tile, each native submission, projection, and publication. Cancellation exits `130`, leaves no complete artifact, and cannot remove the incomplete marker.

CUDA is mandatory: session CUDA registration uses `.error_on_failure()`, CPU EP fallback is disabled, and deterministic/strict session settings are recorded. ORT profiling plus provider-assignment output must show every graph node assigned to `CUDAExecutionProvider`; a CPU/unknown node fails. Negative tests intentionally remove CUDA registration and force a CPU assignment, and must fail rather than execute.

## Resource and Decision Gates

- Target minimum is an 8 GiB GPU and worker VRAM must remain `<= 4 GiB`; the spike records host telemetry but must not treat it as the minimum.
- The host probe is an execution environment only: RTX 4090 24 GiB, driver `580.173.02`, reported CUDA `13.0`, `rustc 1.97.1`, Python `3.11.15`, and 83 GiB free disk.
- Native output must match recorded Python output within the frozen per-tensor parity tolerance recorded by the exporter; repeat native runs must produce identical hashes under the same request.
- Projection must pass the canonical orientation, all 12 edge, 8 corner, finite-height, and polar-cap/ring checks.
- Cancellation is checked between phases and before each native inference submission; cancellation produces no approval and no partial result marked `PASS`.

## Non-goals

- No app/UI, preview/export, procedural replacement, source selector, root dependency, or root crate change.
- No CPU fallback, remote inference, user Python runtime, automatic download, checkpoint/ONNX/fixture distribution, git asset, rights conclusion, or scientific claim.
- No claim that the current procedural `NO-GO` was corrected.

## Acceptance Criteria

**Current P2 status: terminal `NO-GO`.** The final held-out gate reproduced the prescribed material, counters, seeds, and calibration graph hashes, but base-model seed `3904243609224554624`, output channel `3` failed masked relative p99 (`7.176121038669759e-04 > 2.5e-04`). No further seed set or threshold change is allowed; P3–P5 are not pursued.

**BE-003D1A exception:** explicit user approval permits a local, WorldClim-restricted `NONCONFORMANT_CANDIDATE` for real-terrain evaluation only. It remains `distribution: NO_GO` and does not change P2, technical, scientific, or app decisions.

**BE-003D1B verdict: `REAL_TERRAIN_CANDIDATE_NO_GO`.** Fixed CUDA-only ORT session creation rejects the candidate because nodes are assigned to CPU with fallback disabled. This stops replay before any candidate claim.

1. Committed spike source (including `Cargo.lock` and hashed Python tools lock) is confined to `spikes/terrain-diffusion-native/`; only generated environment, source checkout, weights, ONNX, fixtures, and results are ignored under `target/terrain-diffusion-native/`.
2. Preflight proves a compatible ORT/CUDA/cuDNN set before any model acquisition; otherwise it stops.
3. The manifest records every required provenance/artifact hash and measurement or remains `NOT_RUN`/`FAIL`.
4. Technical `GO`, app-integration `GO`, and distribution `GO` are separate decisions. Only the first can change in this spike.
