# Terrain Diffusion LibTorch Spike Manifest

## Scope

Committed source is limited to `spikes/terrain-diffusion-libtorch/`. Generated model/evidence files are limited to ignored `target/terrain-diffusion-libtorch/`. No model-derived artifact is committed or distributed.

## Accepted input provenance

| Input | Required value |
|---|---|
| Fixture manifest | `c10d628d4bbd8ac524ad4b9c3a00a2abe4493f7252b5451bc52dcc138081973c` |
| Fixture status | `PYTORCH_REAL_REFERENCE_PASS` |
| Calls | coarse `80`, base `74`, decoder `4` |
| Fixture bytes | `59853768` |
| Elevation SHA-256 | `990f8e0ded02f2cde99727a9ce4a22fa88168c221cbe74849bae1dbeb91532e0` |
| Full pin set SHA-256 | `e909cdaa0379a58732ed33850a4a018acddf5da2987fd40b512fe4c18482d6d7` |
| Python / torch | `3.11.15` / `2.4.1+cu121` |
| CUDA / cuDNN / ABI | `12.1` / `90100` / `false` |

## Stage A gates

The exporter validates path safety, >=5 GiB free disk, source cleanliness, immutable source/model/stats pins, environment, fixture tensor-set membership/hash/shape/semantic identity, output schemas/dtypes/finite values, source cleanliness after export, exact same-worker repeat outputs, and two worker reports. It records wall time, RSS, and process CUDA allocation without applying a propagated runtime threshold.

No Stage A evidence is atomically published unless every replay and numeric gate passes. Failure preserves only its unique `.incomplete` evidence directory and does not unlock BE-004B.

## BE-004A result

- Verdict: `TORCHSCRIPT_PYTHON_REPLAY_NO_GO`; one controlled rerun only. Evidence: `target/terrain-diffusion-libtorch/stage-a.50111fda503a4cd08fcf6373cea36143.incomplete/`; index SHA-256: `faac784631759c78411817a8714d0f595688e1e317ad092690458faa10cbc1fc`; exporter SHA-256: `c9858ded382ebfadadd84c81173efc3f35b8dc577d8cf9f2c714e6c706570304`.
- `CUBLAS_WORKSPACE_CONFIG=:4096:8` was set before importing torch. Immediately after import and before any CUDA/model load or execution, the Torch API controls enabled deterministic algorithms, disabled cuDNN benchmark and matmul/cuDNN TF32, enabled deterministic cuDNN, and set highest float32 matmul precision. Post-export and post-replay immutable pin validation passed.
- Saved module hashes/sizes: coarse `5382009a3122d4cbc0fc9451d2d2299393a23e52cceb4885936c7f046d49bfc2` / `11341073`; base `fc1e656de89fcbcadd2f520609bba23641cc1ae0b072fec5ede9feed15a6b24a` / `1015273456`; decoder `b88f6ca91c76fc98c0782ce7f728ae2bdc0e3be516c15ebf2f3f602b6e4574a3` / `112201552` bytes. Operator counts were coarse `31`, base `36`, decoder `32`; no `PythonOp` was found.
- Both workers' first-pass aggregate SHA-256 was `96d40db7ccc9d372e326098f3603c4a86c9f328201d7af3dde72d843e41abcdc`. Per-call: 155/158 same-worker repeats were byte-exact; maximum repeat difference was `8.493661880493164e-06`; all 158 numeric per-call gates failed. All 3 graph-pooled gates failed. Peak worker CUDA allocation was `2004910592` bytes; wall/RSS were `23.776s` / `1333796864` bytes and `23.505s` / `1334759424` bytes.
- Graph-pool normalized max / NMAE / NRMSE / p99 / masked-relative p99 / absolute max: coarse `0.00885010016759858` / `0.0002728431667470761` / `0.00044500020804208077` / `0.0018195799094827333` / `0.018661270314268966` / `0.011128101497888565`; base `0.01435553190399147` / `0.0002396796969104117` / `0.00034444410288031344` / `0.001047468421198933` / `0.009793146378745139` / `0.01277470588684082`; decoder `0.00973801577604838` / `0.00017262095736887117` / `0.00030752452467547824` / `0.0012783464803728033` / `0.009026888022841503` / `0.011638164520263672`. All violated unchanged frozen gates.
