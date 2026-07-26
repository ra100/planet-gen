# Terrain Diffusion Integration Research

**Date:** 2026-07-12  
**Decision scope:** Specify an evaluation-only harness for offline terrain artifacts. This is not runtime integration, an upstream evaluation result, or legal advice.

## Evidence Boundary

- **Harness verification** means the local binary checked supplied canonical artifacts and its procedural control.
- **Terrain Diffusion evaluation** is **NOT RUN** until an upstream output, projection, manifest, resource capture, and review evidence exist.
- A missing measurement is failure / `NOT RUN`, never inferred success. Passing the harness does not prove upstream compatibility, rights, quality, or product value.

## Research Basis

| Source | URL | Use |
|---|---|---|
| Project site | [terrain-diffusion](https://xandergos.github.io/terrain-diffusion/) | Method overview |
| Pinned source | [xandergos/terrain-diffusion `82a0431281f21a6ec3d691a12ee61525de5b0790`](https://github.com/xandergos/terrain-diffusion/tree/82a0431281f21a6ec3d691a12ee61525de5b0790) | Runtime/exporter evidence |
| 30 m checkpoint | [model card](https://huggingface.co/xandergos/terrain-diffusion-30m) | Future manifest input |
| 90 m checkpoint | [model card](https://huggingface.co/xandergos/terrain-diffusion-90m) | Future manifest input |
| Paper | [arXiv:2512.08309v4](https://arxiv.org/abs/2512.08309v4) | Published method/benchmark evidence |

**Verified:** Terrain Diffusion documents seed-consistent planar terrain generation with a hierarchical model stack. The public 30 m and 90 m repositories each report approximately 1.138 GB. The paper reports 90 m, T=2, 512² TTFT about 1.72 s and 2.2 GB peak VRAM on an RTX 3090 Ti. No reviewed source establishes cubemap, polar, six-face 8K, released ONNX, or native Rust support.

**Boundary:** Repository code declares MIT, but checkpoint and upstream-data rights remain separate. Do not ship weights, cloud inference, or generated assets; no external result, license conclusion, hardware capture, candidate artifact, or human score is recorded here.

## Current Local Seam

`src/terrain_compute.rs` defines `TectonicTerrain { faces: [Vec<f32>; 6], resolution }`. `src/cube_sphere.rs` and `src/shaders/cube_sphere.wgsl` define layers `0..=5` as `+X,-X,+Y,-Y,+Z,-Z`; `PreviewRenderer::upload_terrain` in `src/preview.rs` uploads them as R16Float.

`app.rs` immediately feeds terrain into wind/weather/cloud state and `export.rs` independently regenerates procedural terrain. Neither is an evaluation seam. The future harness may use only public `GpuContext::new`, `TerrainComputePipeline::generate`, `PreviewRenderer::upload_terrain`, and `PreviewRenderer::render`; no app/export/weather/shader/Cargo source change is needed.

## Frozen Raw Face Contract

A canonical artifact is a directory containing exactly these files, no implicit glob/order:

| Layer | Filename | Face |
|---:|---|---|
| 0 | `posx.f32le` | `+X` |
| 1 | `negx.f32le` | `-X` |
| 2 | `posy.f32le` | `+Y` |
| 3 | `negy.f32le` | `-Y` |
| 4 | `posz.f32le` | `+Z` |
| 5 | `negz.f32le` | `-Z` |

Each file is exactly `4*N*N` bytes: IEEE-754 binary32, little-endian, row-major. `index = y*N+x`; `x=0` is the left column, `x=N-1` right, `y=0` first/top row, and `y=N-1` bottom. A texel center maps to `u=(x+0.5)/N`, `v=(y+0.5)/N`, then `s=2u-1`, `t=2v-1`. The canonical mapping is exactly `cube_to_sphere(face,u,v)`; no loader transform is permitted. Capture reserves the final artifact directory with `create_dir`, writes `.incomplete` until all six files validate, and removes it last. Readers reject `.incomplete`; a hard crash may leave visible invalid residue, while ordinary errors remove the reserved directory. This is not an atomic directory swap claim. Absolute and `..` paths and pre-existing symlink escapes are rejected during preflight; component-wise checks prevent ordinary configuration escapes. The evaluator assumes its working tree is not concurrently mutated by an untrusted local process: it is a local offline research tool, not a privileged security boundary.

Edge names are `L=x0`, `R=xN-1`, `T=y0`, `B=yN-1`. `same` means paired index `j=i`; `reverse` means `j=N-1-i`. These are the complete 12 unique adjacencies derived from the current mapping:

| Edge A | Edge B | Index relation |
|---|---|---|
| `+X.L` | `+Z.R` | same |
| `+X.R` | `-Z.L` | same |
| `+X.T` | `+Y.R` | reverse |
| `+X.B` | `-Y.R` | same |
| `-X.L` | `-Z.R` | same |
| `-X.R` | `+Z.L` | same |
| `-X.T` | `+Y.L` | same |
| `-X.B` | `-Y.L` | reverse |
| `+Y.T` | `-Z.T` | reverse |
| `+Y.B` | `+Z.T` | same |
| `-Y.T` | `+Z.B` | same |
| `-Y.B` | `-Z.B` | reverse |

The complete eight corners are compared as all three listed texels, with mismatch `max-min`:

| Direction | Incident texels |
|---|---|
| `(+,+,+)` | `+X.TL`, `+Y.BR`, `+Z.TR` |
| `(+,+,-)` | `+X.TR`, `+Y.TR`, `-Z.TL` |
| `(+,-,+)` | `+X.BL`, `-Y.TR`, `+Z.BR` |
| `(+,-,-)` | `+X.BR`, `-Y.BR`, `-Z.BL` |
| `(-,+,+)` | `-X.TR`, `+Y.BL`, `+Z.TL` |
| `(-,+,-)` | `-X.TL`, `+Y.TL`, `-Z.TR` |
| `(-,-,+)` | `-X.BR`, `-Y.TL`, `+Z.BL` |
| `(-,-,-)` | `-X.BL`, `-Y.BL`, `-Z.BR` |

`TL`, `TR`, `BL`, and `BR` mean the obvious top/bottom and left/right intersection of the contract above. The validator has a built-in asymmetric orientation fixture at `N=17`: each canonical sample is the exact f32 value `1_000_000*face + 1_000*y + x`. `fixture-orientation` writes/validates this fixture byte-for-byte. It passes only the six canonical filenames and untransformed row-major samples; swapping any pair of faces, reversing rows, reversing columns, or transposing a face must exit `FAIL orientation_fixture`. The edge table is also checked with the exact `same`/`reverse` transforms above; the harness never corrects a candidate by flipping it.

## Frozen CPU Measurements

All candidate measurements are normalized by the control's global finite range: `H=(h-control_min)/(control_max-control_min)`. The control span must be `> 1e-6`; otherwise all height/normal/pole gates fail `FAIL control_range`. Candidate values outside `[0,1]` are retained, not clamped.

For each face, interior jumps are absolute normalized differences for horizontal and vertical neighbors where both texels lie strictly inside the border (`1..N-2`), yielding no edge samples. For every edge table row, emit `N` paired absolute normalized height jumps. For each corner row, emit one normalized three-face `max-min` mismatch. Sort every unweighted collection ascending; percentile `p(q)` is linear interpolation between `a[floor(q*(n-1))]` and `a[ceil(q*(n-1))]`. Empty sets fail.

Candidate height p95 passes only when it is `<= max(1.5*candidate_interior_p95, 1.10*control_edge_p95)`; candidate corner p95 uses the same formula with candidate/control corner and interior values. This is the frozen baseline-relative rule.

Height-to-surface is `P(face,x,y)=cube_to_sphere(face,u,v)*(1+0.01*H)`. Normals use finite differences of `P`: centered `P(x+1,y)-P(x-1,y)` and `P(x,y+1)-P(x,y-1)` where available, otherwise the corresponding one-sided inward difference; normalize their cross product, then negate when its dot product with `cube_to_sphere` is negative. Cross-edge angle is `acos(clamp(dot(nA,nB),-1,1))` in degrees. Candidate p95 must be `<= max(5, control_p95+2)` and max `<=15`.

For poles, classify center directions by latitude `asin(direction.y)` from `cube_to_sphere`. Cap is `|lat|>=75`; ring is `60<=|lat|<75`, per hemisphere. Each texel has weight `(1+s*s+t*t)^(-1.5)`. For normalized elevation and central/one-sided surface-slope magnitude, weighted p10/p50/p90 is the first sorted value whose cumulative weight is `>=q*total_weight`. For a ring magnitude `>=0.05`, each cap quantile passes only when `abs(cap-ring)/abs(ring) <= 0.20`; if ring magnitude `<0.05`, use `abs(cap-ring) <= 0.02`. Empty cap/ring, non-finite input/derived values, or missing visual review fails / `NOT RUN` as applicable.

Exact byte equality across two separately invoked `capture-control --dir` outputs, checked only by a third `compare-bytes --first --second` process, is determinism authority. FNV-1a-64 (`offset_basis=14695981039346656037`, `prime=1099511628211`, byte update `hash=(hash xor byte)*prime mod 2^64`) is printed as a fixed-width lowercase 16-hex human-readable, non-cryptographic identity only. No dependency is added. External source/checkpoint SHA-256 belongs only to a future manifest input and is `NOT RUN`, not computed by the binary.

## Frozen Procedural Control and Render

The only control is built-in `earthlike-v1`, not a config system: `PlanetParams::default()` (`1 AU`, `1 Earth mass`, `0 metallicity`, `23.4°`, `24 h`, seed `42`) then `DerivedProperties::from_params`. Use `N` supplied on the CLI; `continental_scale=1`, `water_loss=0`, `num_plates_override=0`, `num_continents=0`, `continent_size_variety=0`, and `generate_plates(PlateGenParams { seed:42, mass_earth, ocean_fraction, tectonics_factor, continental_scale:1, num_plates_override:0, num_continents:0, continent_size_variety:0 })`.

The procedural compute shader samples inclusive face endpoints. To produce the raw contract's texel centers without changing product code, the evaluator generates a source terrain at `2*N+1` (checked against its evaluator-only source-resolution bound) and serializes only odd source samples `(2*x+1,2*y+1)` into the canonical `N×N` artifact. Use `TerrainComputePipeline::generate` with every argument: `gpu`, generated plates, that source resolution, seed `42`; app formulas `amplitude=0.6+0.6*mass^0.3`, `frequency=(1+0.5*mass^0.2)*continental_scale`, `octaves=(8+4*(tilt/90)*tectonics_factor) as u32`, `gain=2^(-((clamp(1.47+0.91*clamp(ln(distance)/ln(3),0,1)+0.3*metallicity,1.2,3)-1)/2))`, `lacunarity=1.9+0.2*clamp(24/rotation_h,0.5,2)`, then `mountain_scale=1`, `boundary_width=.10`, `warp_strength=1`, `detail_scale=1`, derived gravity, derived tectonics, derived surface age, and continental scale `1`. Erosion policy is disabled: do not construct/call `ErosionPipeline`.

Render only with existing public `PreviewRenderer::new`, `upload_terrain`, and `render`. Every `preview-control`/`preview-candidate` invocation is one fresh OS process; validation and control generation are separate invocations too. Fixed size is 512. `render` receives `cloud_view=None`, `weather_views=None`, and these frozen uniforms: identity rotation; `light_dir=[0.5,0.7,-1.0]`; control ocean level `-0.5+1.7*ocean_fraction`; derived base temperature/ocean fraction/axial tilt/radius/rotation rate/pressure; `view_mode=0`, season `.5`, height scale `3`, zoom `1`, pan `0`; atmosphere/cloud/night/city/lava/rings all `0`; `show_ao=1`, water/ice/biomes/clouds/atmosphere/cities/cloud shadows `0`; pads `0`. `render` already creates RGBA8 sRGB target and strips row padding on readback. Save exactly `artifacts/terrain-diffusion-eval/{control|candidate}-512.png` via existing `image::RgbaImage::from_raw(512,512,pixels).save`; write no product output.

## Recommendation

Keep procedural terrain as the sole product source. The linked plan may implement only this local gatekeeper. Planar projection, upstream inference, runtime/product integration, export parity, license clearance, resource capture, and human review remain deferred and honest `NOT RUN`.
