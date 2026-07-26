# Terrain Diffusion Integration Research

**Date:** 2026-07-12
**Decision scope:** Evaluate an offline terrain artifact workflow only. This is not a runtime integration commitment or legal advice.

## Evidence Boundaries

- **Verified facts** are drawn from the primary sources below and the inspected Planet Gen architecture.
- **Inferences** connect those facts to Planet Gen's existing cubemap terrain pipeline.
- **Recommendations** are project decisions for a bounded evaluation spike.

## Primary Sources

| Source | URL | Use |
|---|---|---|
| Project site | [terrain-diffusion](https://xandergos.github.io/terrain-diffusion/) | Method and availability overview |
| Pinned source repository | [xandergos/terrain-diffusion at `82a0431281f21a6ec3d691a12ee61525de5b0790`](https://github.com/xandergos/terrain-diffusion/tree/82a0431281f21a6ec3d691a12ee61525de5b0790) | Runtime, exporter, and configuration evidence |
| Pinned source README | [README](https://github.com/xandergos/terrain-diffusion/blob/82a0431281f21a6ec3d691a12ee61525de5b0790/README.md) | Published checkpoint names, supported commands, and source-data instructions |
| Pinned source configuration | [`configs/`](https://github.com/xandergos/terrain-diffusion/tree/82a0431281f21a6ec3d691a12ee61525de5b0790/configs) | Model-family configuration evidence |
| 30 m checkpoint | [model card](https://huggingface.co/xandergos/terrain-diffusion-30m) and [repository manifest](https://huggingface.co/xandergos/terrain-diffusion-30m/tree/main) | Checkpoint metadata and files must be recorded in the spike manifest |
| 90 m checkpoint | [model card](https://huggingface.co/xandergos/terrain-diffusion-90m) and [repository manifest](https://huggingface.co/xandergos/terrain-diffusion-90m/tree/main) | Checkpoint metadata and files must be recorded in the spike manifest |
| Paper | [arXiv:2512.08309v4](https://arxiv.org/abs/2512.08309v4) | Method and published benchmark evidence |

## Method Summary

**Verified:** Terrain Diffusion documents seed-consistent, unbounded planar terrain generation with climate data and a hierarchical model stack ([paper abstract](https://arxiv.org/abs/2512.08309v4), [paper HTML](https://arxiv.org/html/2512.08309v4), [pinned README](https://github.com/xandergos/terrain-diffusion/blob/82a0431281f21a6ec3d691a12ee61525de5b0790/README.md)).

**Inference:** This local planar refinement workflow is not evidence of a planet-scale tectonic or continent generator.

**Verified:** The public 30 m and 90 m repositories each report approximately 1.138 GB of storage ([30 m model card](https://huggingface.co/xandergos/terrain-diffusion-30m), [30 m manifest](https://huggingface.co/xandergos/terrain-diffusion-30m/tree/main), [30 m config](https://huggingface.co/xandergos/terrain-diffusion-30m/blob/main/config.json), [90 m model card](https://huggingface.co/xandergos/terrain-diffusion-90m), [90 m manifest](https://huggingface.co/xandergos/terrain-diffusion-90m/tree/main), [90 m config](https://huggingface.co/xandergos/terrain-diffusion-90m/blob/main/config.json)). The paper reports 90 m, T=2, 512² time-to-first-tile of about 1.72 s and 2.2 GB peak VRAM on an RTX 3090 Ti ([paper HTML](https://arxiv.org/html/2512.08309v4)).

**Unknown:** The reviewed public material does not establish a six-face 8K benchmark.

**Verified:** The pinned ONNX exporter targets only `coarse_model`, `base_model`, and `decoder_model` ([export source](https://github.com/xandergos/terrain-diffusion/blob/82a0431281f21a6ec3d691a12ee61525de5b0790/terrain_diffusion/onnx/export.py)).

**Unknown:** The reviewed material does not establish a native Rust runtime or released ONNX artifacts.

## Availability and Licensing

**Verified:** The pinned repository's [LICENSE](https://github.com/xandergos/terrain-diffusion/blob/82a0431281f21a6ec3d691a12ee61525de5b0790/LICENSE) declares an MIT license for repository code. The 30 m [model card](https://huggingface.co/xandergos/terrain-diffusion-30m) and [manifest](https://huggingface.co/xandergos/terrain-diffusion-30m/tree/main), plus the 90 m [model card](https://huggingface.co/xandergos/terrain-diffusion-90m) and [manifest](https://huggingface.co/xandergos/terrain-diffusion-90m/tree/main), are separate metadata sources; record their exact revisions and license fields in the spike manifest rather than treating repository-code licensing as checkpoint licensing.

**Verified:** The [pinned upstream README](https://github.com/xandergos/terrain-diffusion/blob/82a0431281f21a6ec3d691a12ee61525de5b0790/README.md) directs data preparation to [WorldClim 2.1](https://www.worldclim.org/data/worldclim21.html) and [ETOPO](https://www.ncei.noaa.gov/products/etopo-global-relief-model). Review [WorldClim terms](https://www.worldclim.org/data/licence.html), the [MERIT DEM product and terms page](https://hydro.iis.u-tokyo.ac.jp/~yamadai/MERIT_DEM/), [Copernicus Data Space terms](https://dataspace.copernicus.eu/terms-and-conditions), and the ETOPO product page before any shipping decision.

**Boundary:** Checkpoint metadata does not settle rights in upstream training data, generated outputs, redistribution, or commercial use. WorldClim, MERIT, Copernicus, and ETOPO provenance therefore remains a shipping blocker until reviewed for the intended use.

**Recommendation:** Do not ship weights, cloud inference, or commercial generated assets until provenance and upstream rights are clarified. This is a product-risk assessment, not legal advice.

**Unknown:** Whether the pinned source tree has a stable packaged release or a locked reproducibility story. Its [README](https://github.com/xandergos/terrain-diffusion/blob/82a0431281f21a6ec3d691a12ee61525de5b0790/README.md) and [requirements file](https://github.com/xandergos/terrain-diffusion/blob/82a0431281f21a6ec3d691a12ee61525de5b0790/requirements.txt) provide clone-and-install inputs, but this spike must record a complete environment manifest before claiming reproducibility.

## Whole-Planet Fit

**Verified:** The published training-data description excludes latitudes beyond +/-60 degrees ([paper HTML](https://arxiv.org/html/2512.08309v4)).

**Unknown:** The reviewed primary material does not establish spherical, cubemap-edge, cubemap-corner, or polar validation.

**Inference:** Directly treating six independently generated planes as a planet will likely create edge, corner, orientation, and polar discontinuities. Large export sizes amplify this risk: six 8K f32 height faces alone require about 1.5 GiB before intermediates.

**Recommendation:** A learned artifact must be validated as a cubemap import before it can be considered as a source for planetary terrain. It must not replace the procedural continent and tectonic generator.

## Local Architecture Seam

**Verified:** [`src/terrain_compute.rs`](../../src/terrain_compute.rs) defines the canonical interchange as `TectonicTerrain { faces: [Vec<f32>; 6], resolution }`; [`src/preview.rs`](../../src/preview.rs) is the inspected primary source for the preview upload path.

**Inference:** The smallest viable future seam is an alternate producer/importer that returns validated `TectonicTerrain` before existing consumers. Preview-only comparison can use the existing cubemap upload in `src/preview.rs` without changing the production terrain path.

**Required future artifact contract:**

- Source identity: procedural or imported artifact.
- Units, normalization, sea-level semantics, and rejection of non-finite samples.
- Cubemap face order/orientation plus edge and corner continuity rules.
- Deterministic provenance: model hash, revision, seed, sampler, conditioning, projection, manifest/content hash.
- Cached artifact reuse: preview and export must not rerun inference.
- Explicit preview/export parity and a procedural fallback; export must never silently fall back.

## Options

| Option | Fit | Cost and risk | Decision |
|---|---|---|---|
| Keep procedural terrain only | Proven spherical, deterministic, fast, dependency-free | No learned local refinement | Remains default |
| Embedded learned runtime | Poor today | Python pipeline is incomplete in ONNX; large weights and GPU/runtime dependencies | Reject for now |
| Cloud inference | Poor today | Cost, reproducibility, rights, and availability risk | Out of scope |
| Offline artifact import | Bounded experiment | Projection, seam, pole, validation, and provenance work | Evaluate first |
| Offline learned residual after import | Potential later enhancement | Requires a passing import and spherical validation | Defer |

## Recommendation

Adopt **procedural-first plus offline validated import**. Run a time-boxed evaluation spike that reproduces the upstream workflow without embedding it, evaluates projection/seam/pole behavior, compares a preview-only imported artifact against procedural output, and records memory/resource measurements. Keep the dependency-free Rust runtime and procedural path always available.

If the spike passes its gates, write a separate productization plan for a narrow `Procedural` versus `Imported artifact` source selection at the canonical terrain boundary. Consider a hybrid learned residual only after that import path and spherical validation pass. Reject an AI-only runtime route for now.

## Unknowns

- Whether suitable source output can meet cubemap edge, corner, and polar continuity requirements.
- Whether an offline generation workflow is reproducible from a pinned upstream revision on supported authoring hardware.
- Whether artifact quality and resource use remain acceptable at Planet Gen preview and export resolutions.
- Whether upstream asset terms permit the intended authoring, redistribution, and commercial uses.
