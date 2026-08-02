---
title: Terrain Diffusion LibTorch TorchScript Stage A Spike
type: feat
status: completed
date: 2026-07-28
origin: BE-004A
---

# Terrain Diffusion LibTorch TorchScript Stage A Spike

Create a sibling maintainer-only exporter under `spikes/terrain-diffusion-libtorch/`. It consumes only the accepted local PyTorch reference fixture and writes generated evidence only below `target/terrain-diffusion-libtorch/`.

## Frozen contract

- Accept only fixture manifest `c10d628d4bbd8ac524ad4b9c3a00a2abe4493f7252b5451bc52dcc138081973c`, status `PYTORCH_REAL_REFERENCE_PASS`, 80/74/4 calls, `59853768` bytes, and elevation `990f8e0ded02f2cde99727a9ce4a22fa88168c221cbe74849bae1dbeb91532e0`.
- Require the full source/model/stats pin-set `e909cdaa0379a58732ed33850a4a018acddf5da2987fd40b512fe4c18482d6d7` and Python `3.11.15`, torch `2.4.1+cu121`, CUDA `12.1`, cuDNN `90100`, CXX11 ABI `false` before importing model modules.
- Trace only the three tensor wrappers with fixed batch-one real inputs: coarse `(x, noise, c0..c4)`, base `(x, noise, cond58)`, decoder `(x, noise)`. Use `.eval()`, CUDA, inference mode, `strict=True`, and `check_trace=True`; do not script, freeze, or optimize.
- Save exactly `coarse_model.pt`, `base_model.pt`, and `decoder_model.pt` from unique `.incomplete` files. Two fresh worker processes reload and replay every captured call twice.
- Gate every call and graph pool with normalized max `5e-5`, NMAE `1e-5`, NRMSE `1.5e-5`, p99 `4e-5`, normalized bias `1e-5`, cosine `.99999999`, masked-relative p99 `2.5e-4`, full mask coverage, and absolute max `5e-4`. Record 1e-5 allclose only as a diagnostic.

## Decision

Only `TORCHSCRIPT_PYTHON_REPLAY_PASS` authorizes BE-004B. Any other result preserves incomplete evidence and is an exact NO-GO. This spike makes no app, scientific, product, distribution, or Rust `tch` decision.

## Result

`TORCHSCRIPT_PYTHON_REPLAY_NO_GO`. The one controlled deterministic rerun produced matching two-worker first-pass aggregate hashes, but only 155/158 same-worker repeats were byte-exact; every one of 158 per-call and all three graph-pooled numeric gates failed. Failed evidence remains at `target/terrain-diffusion-libtorch/stage-a.50111fda503a4cd08fcf6373cea36143.incomplete/` with index SHA-256 `faac784631759c78411817a8714d0f595688e1e317ad092690458faa10cbc1fc`. BE-004B is blocked.
