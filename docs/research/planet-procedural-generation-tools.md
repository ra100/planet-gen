# Procedural Planet Rendering: Tools, GPU, Seeds, Projections & Inverse Generation

**Deep Research Report**
**Date: 2026-03-27**

---

## Table of Contents

1. [Existing Tools & Libraries](#1-existing-tools--libraries)
2. [GPU Acceleration & Real-time Generation](#2-gpu-acceleration--real-time-generation)
3. [Seed-based Deterministic Generation](#3-seed-based-deterministic-generation)
4. [Equirectangular Projection & Sphere Mapping](#4-equirectangular-projection--sphere-mapping)
5. [Inverse / Constrained Procedural Generation](#5-inverse--constrained-procedural-generation)
6. [Sources Index](#6-sources-index)

---

## 1. Existing Tools & Libraries

### 1.1 libnoise (C++)

A portable, open-source C++ library for generating coherent noise. Supports Perlin noise, ridged multifractal noise, and Voronoi diagrams. Uses a modular architecture where noise modules can be chained together via combiners, selectors, and modifiers to build complex terrain functions from simple building blocks.

- **Key feature**: The `NoiseMapBuilderSphere` class generates equirectangular maps by sampling noise along the surface of a sphere using latitude/longitude coordinates. Tutorial 8 demonstrates creating full planetary terrain with a 2:1 width:height ratio (e.g. 512x256) for seamless spherical wrapping.
- **Noise module hierarchy**: Over a hundred noise functions can be composed in a hierarchy of groups and subgroups for complex planetary surfaces.
- **Status**: Original project on SourceForge is largely unmaintained; superseded by newer libraries but remains a foundational reference.
- **Source**: [libnoise homepage](https://libnoise.sourceforge.net/) | [Tutorial 8: Spherical Planetary Terrain](https://libnoise.sourceforge.net/tutorials/tutorial8.html)

### 1.2 FastNoiseLite

Fast, portable noise library supporting **15 languages**: C#, C++98, C99, HLSL, GLSL, Go, Java, JavaScript/TypeScript, Rust, Fortran, Zig, PowerShell, Odin, Haxe, Pascal, and GML. Created by Auburn (Jordan Peck).

- **Noise types**: OpenSimplex2, OpenSimplex2S, Cellular (Voronoi), Perlin, Value, Value Cubic
- **Fractal modes**: FBm, Ridged, PingPong, Domain Warp (Progressive/Independent)
- **GPU support**: Dedicated HLSL and GLSL shader implementations for GPU-accelerated noise. Community extensions include a CUDA wrapper ([FastNoiseLiteCUDA](https://github.com/NeKon69/FastNoiseLiteCUDA)) and a Godot runtime shader plugin.
- **Precision**: Supports both single-precision (float) and double-precision (double).
- **Latest release**: v1.1.1 (March 5, 2024), 301 commits, 13 releases total.
- **Source**: [GitHub - Auburn/FastNoiseLite](https://github.com/Auburn/FastNoiseLite)

### 1.3 Accidental Noise Library (ANL)

A modular noise library by Joshua Tippetts providing 2D, 3D, 4D, and 6D noise variants. Functions are connected as black-box modules, building complex functions from simple building blocks (similar to a node graph).

- **Capabilities**: Perlin noise, ridged multifractal, gradient noise, fractal layering, color space visualization, procedural textures, heightmaps, and volumetrics.
- **Architecture**: Framework for connecting small functions together; influenced the design of many subsequent noise libraries.
- **Source**: [Accidental Noise Library (SourceForge)](https://accidentalnoise.sourceforge.net/) | [GitHub mirror - JTippetts/accidental-noise-library](https://github.com/JTippetts/accidental-noise-library)

### 1.4 noise-rs (Rust)

Rust procedural noise generation library, a port inspired by libnoise. The `noise` crate (v0.9) on crates.io provides gradient noise with modular composition.

- **Planet generation**: Includes an explicit `complexplanet.rs` example demonstrating a hierarchy of over a hundred noise functions for planetary terrain elevation.
- **Features**: NoiseFn modules that can be chained via `get()` calls, seamless tiling, multiple noise types.
- **Source**: [GitHub - Razaekel/noise-rs](https://github.com/Razaekel/noise-rs) | [crates.io/crates/noise](https://crates.io/crates/noise)

### 1.5 Commercial Terrain Generators

#### World Machine
- Industry-standard node-graph terrain generator. Supports very large terrains with tiled generation and streaming-friendly outputs. Strong erosion toolset.
- Best for: established studio pipelines, large-scale tiling/streaming workflows.
- **Source**: [VionixStudio comparison](https://vionixstudio.com/2021/05/01/world-creator-vs-world-machine-vs-gaea/)

#### Gaea (by QuadSpinner)
- Built by an ex-World Machine developer. GPU-accelerated, emphasizing physically plausible terrains with advanced erosion simulation.
- Best for: most realistic, physically plausible terrains; procedural, repeatable workflows for AAA or cinematic-quality landscapes.
- Weakness: poor documentation hampers learning curve.
- **Source**: [Polycount discussion](https://polycount.com/discussion/228295/gaea-vs-world-machine-vs-world-creator-vs-instant-terra)

#### World Creator
- 100% GPU-powered, real-time WYSIWYG terrain creation. Artist-friendly with strong brush tools, procedural presets, and integrated vegetation/object placement.
- Best for: rapid prototyping, level design, iteration within a single app.
- **Source**: [VionixStudio comparison](https://vionixstudio.com/2021/05/01/world-creator-vs-world-machine-vs-gaea/)

### 1.6 Blender: A.N.T. Landscape Addon

"Another Noise Tool" -- uses different procedural noises to generate landscapes directly in Blender. Was bundled with Blender 4.1; now an extension with limited support.

- **Settings**: Main Settings (object/mesh size, subdivisions), Noise Settings (noise type, octaves, frequency, lacunarity), Displace Settings (terrain height, edge falloff).
- **Under the hood**: Noise is a black-and-white procedural texture; height is a multiply operation, offset is an add operation.
- **Extended version**: TXA (Textured ANT) addon adds texture resolution controls.
- **Source**: [Blender Extensions - A.N.T.Landscape](https://extensions.blender.org/add-ons/antlandscape/) | [Blender Manual - ANT Landscape](https://docs.blender.org/manual/en/3.6/addons/add_mesh/ant_landscape.html) | [GitHub - nerk987/txa_ant](https://github.com/nerk987/txa_ant)

### 1.7 Python Libraries

#### OpenSimplex
- Python implementation of OpenSimplex noise (patent-free alternative to Simplex noise). Supports seeds for deterministic generation. Available on PyPI.
- N-dimensional gradient noise avoiding the directional artifacts of Perlin noise.
- **Source**: [PyPI - opensimplex](https://pypi.org/project/opensimplex/)

#### Noise, NumPy, SciPy
- `noise` package provides Perlin noise in Python. NumPy and SciPy (especially `scipy.ndimage`) are used for post-processing: Gaussian smoothing, erosion simulation via iterative filters, slope analysis.
- **Source**: [Red Blob Games: Making maps with noise](https://www.redblobgames.com/maps/terrain-from-noise/)

### 1.8 Open-Source Planet Generators (GitHub)

| Project | Description | Technique |
|---------|-------------|-----------|
| [jpbetz/planet-generator](https://github.com/jpbetz/planet-generator) | 3D procedural planet using 3D Perlin noise for seamless terrain | 3D noise on sphere |
| [Hoimar/Planet-Generator](https://github.com/Hoimar/Planet-Generator) | Godot addon with layered noise + dynamic LOD terrain chunks | Quadtree LOD, GDScript |
| [nkeenan38/Procedural-Planet-Generator](https://github.com/nkeenan38/Procedural-Planet-Generator) | Tectonic plate simulation: icosahedron subdivision + flood fill | Plate tectonics |
| [raguilar011095/planet_heightmap_generation](https://github.com/raguilar011095/planet_heightmap_generation) | Browser-based with tectonic simulation, erosion, interactive editing | Web-based, tectonics |
| [JakubNei/procedural-planets-generator](https://github.com/JakubNei/procedural-planets-generator) | Custom C# engine; mesh, normal map, biome splat maps all GPU-generated | Compute shaders |
| [Nokitoo/planet-generator](https://github.com/Nokitoo/planet-generator) | OpenGL planet generator | GPU rendering |

---

## 2. GPU Acceleration & Real-time Generation

### 2.1 GPU Gems 3, Chapter 1: Generating Complex Procedural Terrains Using the GPU

The foundational NVIDIA reference for GPU terrain generation. Uses marching cubes to convert implicit density functions into polygonal meshes. The density function layers multiple noise octaves at different frequencies and amplitudes.

- **Three methods** with progressive optimization:
  - Method 1: Two passes (density + vertex generation) -- ~6.6 blocks/sec
  - Method 2: Three passes with stream-out queries -- ~144 blocks/sec (22x faster)
  - Method 3: Five passes with vertex deduplication -- ~260 blocks/sec (80% faster than Method 2)
- **Pipeline**: Pixel shader evaluates density at 33^3 texture coordinates; geometry shader applies marching cubes; stream output collects vertices.
- **Texturing**: Triplanar projection to minimize distortion on steep surfaces.
- **Ambient occlusion**: 32 rays per vertex with short-range (density volume) and long-range (density function) sampling.
- **Source**: [NVIDIA GPU Gems 3, Ch. 1](https://developer.nvidia.com/gpugems/gpugems3/part-i-geometry/chapter-1-generating-complex-procedural-terrains-using-gpu)

### 2.2 Compute Shader Terrain Generation

Modern approaches recommend doing all noise generation in compute shaders for efficient DX12/Vulkan control with data streaming back to CPU as needed. Running fractal noise in pixel/compute shaders yields roughly 10x performance gain over CPU implementations.

- **Practical implementations**: Heightmaps based on 2D fractal Brownian motion (fBm), combining FBM and Hybrid Multifractal algorithms for peaks and trenches.
- **Hydraulic erosion**: Can be run on compute shaders for massive speedup.
- **Source**: [GameDev.net - Procedural terrain noise on GPU](https://www.gamedev.net/forums/topic/698814-procedural-terrain-whats-the-best-approach-to-calculate-noise-in-the-gpu/) | [WorldKit (Unity compute shader API)](https://github.com/vkDreamInCode/WorldKit)

### 2.3 Jadkhoury: Procedural Planet Rendering

A detailed blog post on GPU-based procedural planet rendering combining FBM with Hybrid Multifractal algorithms.

- **Key innovation**: Dual framebuffer swapping -- reads previous heightmap data while writing new values. Only newly visible terrain regions recalculate; stable areas read from prior buffer.
- **Discretized displacement**: Vectors aligned to texel boundaries prevent interpolation artifacts.
- **Texturing**: Height-based blending (sand -> grass -> snow) enhanced with slope analysis via normal vectors.
- **Source**: [Jadkhoury - Procedural Planet Rendering](https://jadkhoury.github.io/terrain_blog.html)

### 2.4 GLSL Noise: Ashima Arts / Stefan Gustavson webgl-noise

GLSL source code for Perlin noise (2D, 3D, 4D) in both modern simplex and classic versions, plus periodic noise and Worley (cellular) noise.

- **Key advantage**: Completely self-contained with no dependency on external data (no lookup textures). Scalable to massive parallelism.
- **Performance**: Not quite as fast as texture-based implementations on desktop GPUs but more scalable and convenient. Makes good use of unused ALU resources when run concurrently with texture-intensive rendering.
- **Authors**: Ashima Arts (now defunct) and Stefan Gustavson. Cloned to [stegu/webgl-noise](https://github.com/stegu/webgl-noise).
- **License**: MIT
- **Includes**: `psrdnoise` functions for periodic, rotating, gradient-returning simplex noise.
- **Source**: [GitHub - ashima/webgl-noise](https://github.com/ashima/webgl-noise) | [stegu/webgl-noise](https://github.com/stegu/webgl-noise) | [npm - webgl-noise](https://www.npmjs.com/package/webgl-noise)

### 2.5 Adaptive Level-of-Detail for Planet Rendering

#### CDLOD (Continuous Distance-Dependent Level of Detail)
By Filip Strugar (2010). Structured around a quadtree of regular grids rather than nested grids. LOD function is based on precise 3D distance between observer and terrain. Uses a novel transition technique between LOD levels for smooth, accurate results.
- **Source**: [CDLOD paper (PDF)](https://aggrobird.com/files/cdlod_latest.pdf) | [GitHub - fstrugar/CDLOD](https://github.com/fstrugar/CDLOD)

#### Geometry Clipmaps
By Losasso and Hoppe (SIGGRAPH 2004). Caches terrain in nested regular grids centered about the viewer, stored as vertex buffers in video memory, incrementally refilled as viewpoint moves. Spherical clipmaps extension renders "rings" with different LOD around viewer position on the planet.
- **Source**: [Geometry Clipmaps paper (PDF)](https://hhoppe.com/geomclipmap.pdf) | [ACM Digital Library](https://dl.acm.org/doi/abs/10.1145/1015706.1015799)

#### Planetary Rendering with Mesh Shaders (2020)
Bachelor thesis (TU Wien) exploring mesh shaders for planet rendering with adaptive tessellation.
- **Source**: [TU Wien thesis (PDF)](https://www.cg.tuwien.ac.at/research/publications/2020/rumpelnik_martin_2020_PRM/rumpelnik_martin_2020_PRM-Thesis.pdf)

### 2.6 Quadtree Sphere Tessellation

The "quadcube" approach: start with a cube whose faces are divided into regular grids, project every surface point out to an enclosing sphere. Each face becomes a quadtree structure where each level splits four ways for more detail.

- **Advantages over lat/lon grids**: Simpler calculations (no complex trigonometry), GPU-native cubemap texture support, smaller area distortions, distortions mirror perspective projection.
- **Alternatives**: Icosahedron subdivision (icosphere) -- equal-area triangles but more complex indexing. Octahedron with binary tree -- right-angled triangles ideal for binary tree subdivision.
- **Source**: [Acko.net - Making Worlds 1: Of Spheres and Cubes](https://acko.net/blog/making-worlds-1-of-spheres-and-cubes/) | [vterrain.org - Terrain LOD on Spherical Grids](http://vterrain.org/LOD/spherical.html)

---

## 3. Seed-based Deterministic Generation

### 3.1 Seed-to-Hash-to-Parameters Pipeline

A single seed value deterministically produces the same output every time, enabling:
- Sharing worlds between players (e.g., Minecraft seed sharing)
- Debugging specific scenarios
- Regenerating content identically

The typical pipeline: **Seed -> Hash function -> Derived parameters (noise offsets, frequencies, biome thresholds, etc.) -> Noise evaluation -> Terrain**.

### 3.2 Hash Functions for GPU Rendering (Jarzynski & Olano, 2020)

The definitive evaluation of hash functions for procedural generation on GPU. Published in Journal of Computer Graphics Techniques (JCGT), Vol. 9, No. 3, 2020. Evaluates quality via TestU01 "Big Crush" test suite and GPU execution speed via benchmarking.

**Key findings:**
- **pcg3d / pcg4d**: Fall on the Pareto frontier (best quality/speed tradeoff). Recommended as default for multidimensional high-quality hash functions.
- **xxhash32**: Good default choice for other (non-multidimensional) cases. ~50x faster than MD5 with comparable random properties.
- **PCG hash (32-bit, RXS-M-XS variant)**: "Should probably be your default GPU hash function" -- slightly better performance and much better statistical quality than Wang hash.

PCG implementation (GLSL-style):
```glsl
uint pcg_hash(uint input) {
    uint state = input * 747796405u + 2891336453u;
    uint word = ((state >> ((state >> 28u) + 4u)) ^ state) * 277803737u;
    return (word >> 22u) ^ word;
}
```

- **Source**: [JCGT paper](https://jcgt.org/published/0009/03/02/) | [Nathan Reed's blog summary](https://www.reedbeta.com/blog/hash-functions-for-gpu-rendering/) | [ShaderToy demo](https://www.shadertoy.com/view/XlGcRh)

### 3.3 xxHash

High-performance non-cryptographic hash function. Useful for procedural generation because it supports a seed concept -- different seeds for different random properties (entity attributes, grid cells), with the entity index or cell coordinate as input.

- ~50x faster than MD5 with comparable random properties.
- **Source**: [Runevision blog - Primer on Repeatable Random Numbers](https://blog.runevision.com/2015/01/primer-on-repeatable-random-numbers.html)

### 3.4 SplitMix64

Maintains one 64-bit state variable, returns 64 bits per call. Passes BigCrush. Commonly used to calculate initial states for other PRNGs (e.g., xoshiro/xoroshiro family).

- **Implementation**: Three XOR-shift-multiply rounds with constants `0xbf58476d1ce4e5b9` and `0x94d049bb133111eb`.
- **Properties**: Good avalanche properties, fast, deterministic given same seed. Not cryptographically secure.
- **Use case**: Seed initialization, parameter distribution across procedural systems.
- **Source**: [Rosetta Code - SplitMix64](https://rosettacode.org/wiki/Pseudo-random_numbers/Splitmix64) | [PCG-random.org analysis](https://www.pcg-random.org/posts/critiquing-pcg-streams.html)

### 3.5 Avoiding Visible Repetition

#### Wang Tiling (Non-periodic Tiling)
Modifications to procedural noise functions can directly produce Wang tile sets, enabling non-periodic tiling at small performance cost while maintaining or reducing memory consumption. Published in ACM Proceedings on Computer Graphics and Interactive Techniques, 2018.
- **Source**: [ACM - Non-periodic Tiling of Procedural Noise Functions](https://dl.acm.org/doi/10.1145/3233306)

#### Procedural Stochastic Texturing
Unity Labs technique that procedurally generates infinite textures matching input appearance, avoiding tiling. Assigns random offsets and orientations per tile with smooth interpolation at boundaries.
- **Source**: [Unity - Procedural Stochastic Texturing](https://unity.com/archive/blog/engine-platform/procedural-stochastic-texturing-in-unity)

#### Texture Variation (Inigo Quilez)
Assigns random offsets and orientations to each tile, with smooth floating-point index transitions across boundaries to interpolate between virtual patterns.
- **Source**: [iquilezles.org - Texture Repetition](https://iquilezles.org/articles/texturerepetition/)

---

## 4. Equirectangular Projection & Sphere Mapping

### 4.1 How Equirectangular Maps Work

Maps parallels of latitude to rows in an image and meridians of longitude to columns. Creates a 2:1 aspect ratio rectangular image where longitude spans the full width and latitude spans the full height. This is the standard format for planetary texture maps.

- **Source**: [Wikipedia - Equirectangular Projection](https://en.wikipedia.org/wiki/Equirectangular_projection) | [PanoTools Wiki](https://wiki.panotools.org/Equirectangular_Projection)

### 4.2 Generating Seamless Equirectangular Maps from Noise

**The critical technique**: Sample 3D noise on the sphere surface instead of 2D noise on the image plane. Iterate over spherical coordinates (phi, theta), convert to Cartesian coordinates (x, y, z), and sample 3D noise at those points. This guarantees seamless wrapping without discontinuities at poles or edges.

```
For each pixel (u, v) in the equirectangular image:
    theta = u * 2*PI          // longitude: 0 to 2*PI
    phi = v * PI              // latitude: 0 to PI
    x = sin(phi) * cos(theta)
    y = cos(phi)
    z = sin(phi) * sin(theta)
    height = noise3D(x * scale, y * scale, z * scale)
```

- Fractal layering: Re-sample noise at higher frequencies and lower amplitudes (octaves) for multi-scale detail.
- Key parameters: seed, noise type, fractal algorithm, scale, octave count, frequency, lacunarity, output range, power functions.
- **Source**: [Toni Sagrista - Procedural Planetary Surfaces](https://tonisagrista.com/blog/2021/procedural-planetary-surfaces/)

### 4.3 Seam Handling at +/-180 Degrees Longitude

When using 3D noise sampled on the sphere, there is no seam at the antimeridian because the 3D sampling is inherently continuous. Problems only arise with 2D noise approaches. Practical advice: avoid having landmasses cross the 180-degree longitude line; have landmasses either fill poles completely or leave them empty.

- **Source**: [Worldbuilding Workshop - Polar Region Distortion](https://worldbuildingworkshop.com/2023/03/18/polar-region-distortion-on-full-world-maps/)

### 4.4 Pole Distortion Compensation

The top and bottom rows of an equirectangular image each represent a single point (North/South Pole) stretched across the full width. This creates:
- Extreme stretching of features near poles
- Changed noise statistics based on sampling density
- "Pinching" artifacts

**Solutions**:
1. **3D noise sampling** (see 4.2) -- the gold standard; noise density is uniform on the sphere surface
2. **Cubemap approach**: Generate noise on six cube faces, then convert to equirectangular. Less polar distortion since cube corners distort less dramatically than equirectangular poles.
3. **Post-processing**: Apply latitude-dependent filtering to compensate for density variation.
4. **Adaptive sampling**: Increase sampling density near poles in the equirectangular image.

- **Source**: [Wikiversity - Equirectangular Maps and Distortion](https://en.wikiversity.org/wiki/Equirectangular_projection/Maps_and_Distortion) | [Worldbuilding Pasta - Counting Area on Equirectangular Maps](https://worldbuildingpasta.blogspot.com/2025/03/hurried-thoughts-counting-area-on.html)

### 4.5 Resolution Considerations

| Resolution | Dimensions | Use Case |
|------------|-----------|----------|
| 4K | 4096 x 2048 | Good for distant viewing, game planets |
| 8K | 8192 x 4096 | High quality, close flyby acceptable |
| 16K | 16384 x 8192 | Very high quality, requires LOD streaming |

Width should always be double height for equirectangular maps. Higher resolutions require more noise octaves to avoid visible smoothness at close range.

### 4.6 Cubemap Alternative

Cubemaps avoid many equirectangular problems:
- Less polar distortion (corner distortion is mild compared to pole stretching)
- GPU-native format (hardware cubemap sampling)
- Each face is a regular grid -- straightforward noise generation
- Convert cubemap <-> equirectangular as needed

Generate noise per cube face using 3D coordinates, then optionally convert to equirectangular for compatibility with standard tools.

- **Source**: [Paul Bourke - Converting to/from cubemaps](https://paulbourke.net/panorama/cubemaps/) | [Acko.net - Making Worlds](https://acko.net/blog/making-worlds-1-of-spheres-and-cubes/)

### 4.7 Applying Maps to UV Sphere in 3D Software

A UV sphere's texture coordinates inherently match the equirectangular projection -- meridians and latitude circles map to straight lines in UV space. Standard UV mapping in Blender, Unity, or other 3D tools automatically applies equirectangular textures correctly to UV spheres.

- **Source**: [ShaderToy - Equirectangular map sampling](https://www.shadertoy.com/view/4lycz3) | [WebGPU Unleashed - Equirectangular Rendering](https://shi-yan.github.io/webgpuunleashed/Advanced/equirectangular_rendering.html)

---

## 5. Inverse / Constrained Procedural Generation

### 5.1 Inverse Procedural Modeling

The problem: given a desired terrain output (or properties), find the procedural model parameters that produce it. This reverses the normal "parameters -> output" direction.

**Automatic Differentiable Procedural Modeling (ADPM)**: Solves the inverse problem by making procedural models differentiable, allowing gradient-based optimization to transfer user modifications back to procedural parameters.

- **Source**: [ResearchGate - Methods for Procedural Terrain Generation: A Review](https://www.researchgate.net/publication/333858117_Methods_for_Procedural_Terrain_Generation_A_Review)

### 5.2 Constrained Optimization Approaches

- **Intelligent agents**: Generate terrain elevation heightmaps according to designer-defined constraints.
- **Boundary-constrained polynomials**: Optimized terrain production using minimum operations per pixel.
- **Parameter control**: Synthesized terrain details controlled by frequency, orientation, and amplitude. Gradient constraints for higher-order accuracy.
- **Neural network optimization**: Train networks to take semantic input descriptions and output optimal procedural parameter values.
- **Source**: [Real-time Terrain Enhancement with Controlled Procedural Patterns (2024)](https://onlinelibrary.wiley.com/doi/10.1111/cgf.14992)

### 5.3 Machine Learning for Terrain Generation

#### GANs (Generative Adversarial Networks)
Early approach (Beckham & Pal, 2017): Spatial GANs trained on NASA satellite imagery (heightmaps + textures as 4-channel images). Limited to random generation without user control.
- **Source**: [GitHub - christopher-beckham/gan-heightmaps](https://github.com/christopher-beckham/gan-heightmaps) | [ArXiv paper](https://arxiv.org/pdf/1707.03383)

#### Diffusion Models (Current State-of-the-Art)

**Earthbender (SIGGRAPH MIG 2025)**: Interactive system for sketch-based terrain heightmap generation using a guided diffusion model. Uses a custom-trained ControlNet steering Stable Diffusion v1.5. Multi-channel semantic sketch input: red = mountains, blue = rivers/roads, green = lakes. Significantly outperforms traditional GANs (Pix2PixHD) in data efficiency and structural fidelity.
- **Source**: [ACM - Earthbender (SIGGRAPH 2025)](https://dl.acm.org/doi/full/10.1145/3769047.3769053)

**TerraFusion (2025)**: Joint generation of terrain geometry AND texture using latent diffusion models. User-guided control over generation.
- **Source**: [ArXiv - TerraFusion](https://arxiv.org/html/2505.04050v1)

#### Style Transfer (2024)
Combines procedural noise generation with Neural Style Transfer, drawing style from real-world height maps. Achieves diverse terrains aligned with real-world morphological characteristics at low computational cost. Evaluated using Structural Similarity (SSIM) metric.
- **Source**: [ArXiv - Procedural terrain generation with style transfer](https://arxiv.org/html/2403.08782v1) | [GitHub](https://github.com/fmerizzi/Procedural-terrain-generation-with-style-transfer)

### 5.4 Wave Function Collapse (WFC) for Biome Placement

WFC generates content by propagating constraints from an example or rule set. Applied to terrain:

- **Two-pass approach**: First pass generates biome distribution (forest, sea, desert, etc.), second pass generates terrain specific to each biome for refined detail.
- **Consistency management**: Decide biome type in advance, disable tiles that do not fit that biome.
- **Terrain heightmaps via WFC**: Recent work (ArXiv, Dec 2024) applies WFC to SRTM elevation data, using slopes as input rather than raw heights. Statistical analysis confirms structural characteristics are preserved.
- **Source**: [GitHub - mxgmn/WaveFunctionCollapse](https://github.com/mxgmn/WaveFunctionCollapse) | [ArXiv - WFC for Terrain using SRTM data (2024)](https://arxiv.org/abs/2412.04688) | [Boris the Brave - WFC Tips and Tricks](https://www.boristhebrave.com/2020/02/08/wave-function-collapse-tips-and-tricks/)

### 5.5 AutoBiomes: Procedural Multi-Biome Landscapes

Academic system (The Visual Computer, 2020) for generating vast terrains with plausible biome distributions. Combines synthetic procedural terrain generation with digital elevation models (DEMs) and simplified climate simulation.

- **Pipeline**: Temperature, wind, and precipitation simulation -> biome distribution -> asset placement via rule-based local-to-global model.
- **Key contribution**: Addresses the scarcely explored topic of multi-biome landscape generation.
- **Source**: [Springer - AutoBiomes](https://link.springer.com/article/10.1007/s00371-020-01920-7) | [PDF](https://cgvr.cs.uni-bremen.de/papers/cgi20/AutoBiomes.pdf)

---

## 6. Sources Index

All distinct sources found during this research, organized by topic:

### Tools & Libraries
1. **libnoise homepage** - https://libnoise.sourceforge.net/ - C++ coherent noise library with spherical terrain tutorials
2. **libnoise Tutorial 8: Spherical Terrain** - https://libnoise.sourceforge.net/tutorials/tutorial8.html - Creating equirectangular planetary maps
3. **FastNoiseLite (GitHub)** - https://github.com/Auburn/FastNoiseLite - v1.1.1, March 2024. 15-language portable noise library with HLSL/GLSL GPU support
4. **FastNoiseLiteCUDA** - https://github.com/NeKon69/FastNoiseLiteCUDA - CUDA wrapper for GPU kernel noise generation
5. **Accidental Noise Library** - https://accidentalnoise.sourceforge.net/ - Modular 2D-6D noise with node-graph architecture
6. **noise-rs (Rust)** - https://github.com/Razaekel/noise-rs - Rust noise crate with complexplanet example
7. **VionixStudio: World Creator vs World Machine vs Gaea** - https://vionixstudio.com/2021/05/01/world-creator-vs-world-machine-vs-gaea/ - 2021 comparison of commercial terrain tools
8. **Polycount: Gaea vs World Machine vs World Creator** - https://polycount.com/discussion/228295/gaea-vs-world-machine-vs-world-creator-vs-instant-terra - Community comparison discussion
9. **A.N.T. Landscape (Blender Extensions)** - https://extensions.blender.org/add-ons/antlandscape/ - Blender procedural terrain addon
10. **OpenSimplex (PyPI)** - https://pypi.org/project/opensimplex/ - Python OpenSimplex noise with seed support
11. **Red Blob Games: Making maps with noise** - https://www.redblobgames.com/maps/terrain-from-noise/ - Tutorial on noise-based terrain + biome generation

### GPU Acceleration & LOD
12. **NVIDIA GPU Gems 3, Ch. 1** - https://developer.nvidia.com/gpugems/gpugems3/part-i-geometry/chapter-1-generating-complex-procedural-terrains-using-gpu - Foundational GPU terrain generation (marching cubes, 260 blocks/sec)
13. **Jadkhoury: Procedural Planet Rendering** - https://jadkhoury.github.io/terrain_blog.html - FBM + Hybrid Multifractal with dual framebuffer innovation
14. **Ashima Arts webgl-noise (GitHub)** - https://github.com/ashima/webgl-noise - GLSL Perlin/Simplex/Cellular noise, MIT license
15. **Stefan Gustavson webgl-noise fork** - https://github.com/stegu/webgl-noise - Maintained fork after Ashima Arts closure
16. **CDLOD paper** - https://aggrobird.com/files/cdlod_latest.pdf - Filip Strugar, 2010. Quadtree-based continuous distance LOD
17. **Geometry Clipmaps (SIGGRAPH 2004)** - https://hhoppe.com/geomclipmap.pdf - Losasso & Hoppe, nested regular grids
18. **Acko.net: Making Worlds 1** - https://acko.net/blog/making-worlds-1-of-spheres-and-cubes/ - Quadcube tessellation, chunked LOD comparison
19. **TU Wien: Planetary Rendering with Mesh Shaders (2020)** - https://www.cg.tuwien.ac.at/research/publications/2020/rumpelnik_martin_2020_PRM/rumpelnik_martin_2020_PRM-Thesis.pdf - Bachelor thesis on mesh shader planet rendering

### Seed-based Generation & Hash Functions
20. **Jarzynski & Olano: Hash Functions for GPU Rendering (JCGT 2020)** - https://jcgt.org/published/0009/03/02/ - Definitive GPU hash evaluation; pcg3d/pcg4d and xxhash32 recommended
21. **Nathan Reed: Hash Functions for GPU Rendering** - https://www.reedbeta.com/blog/hash-functions-for-gpu-rendering/ - Accessible summary recommending PCG as default GPU hash
22. **Runevision: Primer on Repeatable Random Numbers** - https://blog.runevision.com/2015/01/primer-on-repeatable-random-numbers.html - xxHash for procedural generation seeds
23. **Rosetta Code: SplitMix64** - https://rosettacode.org/wiki/Pseudo-random_numbers/Splitmix64 - Reference implementation
24. **Alain.xyz: Noise Generation Survey** - https://alain.xyz/blog/noise-generation-survey - Comprehensive survey of noise types, hashing, domain warping
25. **Ninjapretzel: Procedural Generation Hashing** - https://ninjapretzel.github.io/ProcGen/01hashing.html - Tutorial on hash-based procedural generation

### Anti-Repetition Techniques
26. **ACM: Non-periodic Tiling of Procedural Noise Functions (2018)** - https://dl.acm.org/doi/10.1145/3233306 - Wang tiling for noise
27. **Unity: Procedural Stochastic Texturing** - https://unity.com/archive/blog/engine-platform/procedural-stochastic-texturing-in-unity - Infinite texture generation without tiling
28. **Inigo Quilez: Texture Repetition** - https://iquilezles.org/articles/texturerepetition/ - Random offset/orientation technique

### Equirectangular Projection
29. **Toni Sagrista: Procedural Planetary Surfaces (2021)** - https://tonisagrista.com/blog/2021/procedural-planetary-surfaces/ - 3D noise sampling on sphere for seamless equirectangular maps
30. **Worldbuilding Workshop: Polar Region Distortion** - https://worldbuildingworkshop.com/2023/03/18/polar-region-distortion-on-full-world-maps/ - Practical pole distortion advice
31. **Paul Bourke: Converting Cubemaps** - https://paulbourke.net/panorama/cubemaps/ - Cubemap <-> equirectangular conversion
32. **Wikipedia: Equirectangular Projection** - https://en.wikipedia.org/wiki/Equirectangular_projection - Reference

### Inverse/ML/Constrained Generation
33. **Earthbender (SIGGRAPH MIG 2025)** - https://dl.acm.org/doi/full/10.1145/3769047.3769053 - ControlNet-guided diffusion for sketch-based terrain
34. **TerraFusion (ArXiv 2025)** - https://arxiv.org/html/2505.04050v1 - Joint terrain geometry + texture via latent diffusion
35. **Procedural terrain with style transfer (ArXiv 2024)** - https://arxiv.org/html/2403.08782v1 - Neural Style Transfer on procedural noise
36. **GAN heightmaps (Beckham & Pal, 2017)** - https://github.com/christopher-beckham/gan-heightmaps - First GAN-based terrain from NASA satellite data
37. **WaveFunctionCollapse (GitHub)** - https://github.com/mxgmn/WaveFunctionCollapse - Reference WFC implementation by Maxim Gumin
38. **WFC for Terrain using SRTM data (ArXiv Dec 2024)** - https://arxiv.org/abs/2412.04688 - WFC applied to real elevation data
39. **AutoBiomes (The Visual Computer, 2020)** - https://link.springer.com/article/10.1007/s00371-020-01920-7 - Multi-biome procedural landscapes with climate simulation
40. **Boris the Brave: WFC Tips and Tricks (2020)** - https://www.boristhebrave.com/2020/02/08/wave-function-collapse-tips-and-tricks/ - Practical WFC guidance including biome approaches

---

*Total distinct sources: 40*
*Research conducted: 2026-03-27*
