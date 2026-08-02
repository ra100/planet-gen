# Terrain Diffusion LibTorch TorchScript Spike

Maintainer-only Stage A exporter. It consumes the accepted restricted PyTorch fixture from `target/terrain-diffusion-native/` and writes only ignored evidence below `target/terrain-diffusion-libtorch/`.

```text
rtk proxy target/terrain-diffusion-native/env/bin/python spikes/terrain-diffusion-libtorch/tools/export_torchscript.py --self-check
rtk proxy target/terrain-diffusion-native/env/bin/python spikes/terrain-diffusion-libtorch/tools/export_torchscript.py
```

`TORCHSCRIPT_PYTHON_REPLAY_PASS` authorizes only BE-004B. Any other result is a NO-GO; it does not approve native integration, scientific suitability, app/UI integration, or distribution.
