# Tools and Production Systems: Deep-Dive Research

**Consolidated Research Report**
**Date: 2026-04-02**
**Sources: planet-procedural-generation-tools.md, planet-simulation-frameworks.md, procedural-planet-systems-survey.md**

---

## Table of Contents

1. [Executive Summary](#1-executive-summary)
2. [Production Systems Deep-Dive](#2-production-systems-deep-dive)
3. [Noise Libraries & Tools](#3-noise-libraries--tools)
4. [Simulation Frameworks](#4-simulation-frameworks)
5. [GPU Pipeline Patterns](#5-gpu-pipeline-patterns)
6. [ML/AI Terrain Generation](#6-mlai-terrain-generation)
7. [Anti-Repetition Techniques](#7-anti-repetition-techniques)
8. [References](#8-references)

---

## 1. Executive Summary

This document consolidates deep-dive research on production planet generation systems, noise libraries, simulation frameworks, GPU pipeline architecture, and emerging ML techniques. It complements the parent `final.md` document (which covers brief system lessons and recommended tech stack tables) by providing the detailed architectural analysis, benchmarks, and implementation specifics needed for engineering decisions.

**Key findings:**

- **SpaceEngine** achieves ~1 terapixel effective resolution per cube face through a meshless vertex-shader architecture with quadtree LOD depth 12 and 256x256 texture nodes, using static memory allocation for 10x loading speed improvement.
- **Elite Dangerous (Stellar Forge)** uniquely uses physics-based formation simulation rather than pure noise -- accretion, tectonic, and bombardment simulations feed into terrain parameters.
- **No Man's Sky** uses CPU-based Dual Contouring on SDF voxels, with star positions as seeds for deterministic generation of 18+ quintillion planets.
- **GPU terrain generation** provides ~10x speedup over CPU; GPU Work Graphs (DX12, 2024) represent the state of the art at 34.8 GB of geometry from 51 KB of generation code.
- **Simplex noise** offers ~30% cost reduction per octave over Perlin in 3D, with fewer directional artifacts.
- **Progressive refinement** from 256^2 preview (<1 ms) to 16K production (~5.5 s per face) enables interactive editing workflows.
- **Split-by-cube-face** is the recommended multi-GPU strategy with near-linear scaling and minimal inter-GPU communication.
- **Diffusion models** (Earthbender, TerraFusion) have surpassed GANs for ML terrain generation as of 2025.

---

## 2. Production Systems Deep-Dive

### 2.1 SpaceEngine Architecture

**Projection and LOD:**
SpaceEngine uses a cube-sphere projection: each planet has six faces, each subdivided as a quadtree. Nodes subdivide when their textures become too stretched on screen (below a 256-pixel threshold). Each texture node is 256x256 pixels, with a maximum quadtree depth of level 12 for Earth-sized planets, yielding an effective resolution per cube face of 256 \* 2^12 = 1,048,576 pixels (~1 terapixel total across all faces).

**Noise approach:**
Fractal noise is multi-octave Perlin noise. Surface detail reaches ~10 m per triangle in geometry, extended to millimeters via shader-based detail texturing. The biome assignment system operates in shaders (not latitude-based), using palette presets per planet class. Four slope levels are multiplied by four layers per biome; two large-scale levels (10 m/px) are mixed with two shader-blended layers for fine detail.

**Meshless terrain rendering:**
SpaceEngine's key innovation is that geometry is built entirely in the vertex shader via heightmap texture fetch. There is no CPU mesh -- mountain silhouettes load before texture data because geometry is independent of textures. This eliminates the CPU mesh generation bottleneck that limits systems like No Man's Sky.

**Performance characteristics:**

- VRAM: 2+ GB with S3TC texture compression (2-4x memory reduction, hardware-decompressed at zero ALU cost)
- RAM for terrain: 100-150 MB
- Static memory allocation: large texture array pre-allocated at startup, eliminating dynamic allocation overhead and providing a 10x loading speed improvement
- Loading at LOD 0: ~5 seconds; LOD 1: ~30 seconds (RTX 2080, 4K display)
- At speed 8: 60+ fps; at speed 20: 18 fps with 12-second LOD 1 loading
- Texture arrays eliminate per-node texture unit switching overhead
- Maximum cache: 2,048 textures per type (OpenGL limit)

**Two-shader optimization:**
A complex close-range shader handles full biome blending, slope analysis, and detail texturing. A fast distant shader is used for far-away terrain, significantly reducing fragment shader cost for the majority of the planet surface.

**Priority-based loading:**
Closer areas load first. Normal maps are prioritized before color textures because normals are more perceptually important for conveying terrain shape. Detail textures use coordinate distortion to reduce visible tiling artifacts.

**Climate model:**
SpaceEngine implements a physically derived climate model with energy transport, greenhouse effects, and tidal locking calculations. This feeds into procedural atmosphere and surface appearance.

Sources: [SpaceEngine Terrain 2.0](https://spaceengine.org/news/blog190328/), [Terrain Engine Upgrade #1](https://spaceengine.org/news/blog171016/), [Terrain Engine Upgrade #3](https://spaceengine.org/news/blog171120/), [Procedural Generation Wiki](https://spaceengine.fandom.com/wiki/Procedural_Generation)

---

### 2.2 Outerra Terrain Engine

**Architecture:**
Outerra uses chunked LOD based on quadtree subdivision over a spherical planet. Three independent fractal noise channels operate per quadtree node: (1) elevation seeded from heightmap data, (2-3) detail material mixing. A fourth channel provides global slope. The system uses adaptive mesh tessellation in vertex shaders, deforming a spherical base mesh with both vertical elevation and horizontal displacement.

**Horizontal displacement (unique innovation):**
Unlike most terrain engines that only displace vertices vertically, Outerra applies horizontal displacement from its fractal channels. This produces overhangs, better cliff geometry, and more realistic rocky features -- capabilities typically reserved for voxel-based systems but achieved here on a mesh.

**Real data integration:**
Outerra seamlessly blends real-world data (SRTM/NASADEM elevation, satellite orthoimagery, OpenStreetMap vectors) with procedural fractal generation. Fractal algorithms refine and introduce details parametrized by elevation and land class, providing detail from orbital altitudes down to ~1 cm resolution.

**Technical details:**

- OpenGL 3.3+, fully asynchronous: majority of work on GPU, rest distributed to multiple CPU cores
- Double-precision mathematics for accurate map projections at planetary scale
- Hilbert curve variants for locality-preserving quadtree traversal (improves cache coherence)
- Vertex morphing for smooth LOD transitions (no popping artifacts)
- CDLOD (Continuous Distance-Dependent LOD) with geomorphing

**GPU performance observations:**

- Optimal mesh size: ~5k triangles works well across GPU vendors
- Both NVIDIA and AMD perform best at 5-20k triangles per instanced draw call
- AMD GCN 1.1+: performance almost doubles at the 5k triangle threshold
- NVIDIA: 30% performance hit when culling is disabled; minimum mesh size should be ~80 triangles

Sources: [Outerra Official](https://www.outerra.com/), [Outerra Blog: Procedural Grass Performance](https://outerra.blogspot.com/2016/01/procedural-rendering-performance-test-1.html), [GameDev.net Discussion](https://www.gamedev.net/forums/topic/643870/what-terrain-rendering-technique-does-the-outerra-engine-use/)

---

### 2.3 No Man's Sky: Seed-Based Determinism

**Voxel/SDF architecture:**
No Man's Sky uses voxel-based world generation with Signed Distance Fields (SDF). Mesh extraction is via Dual Contouring (not Marching Cubes) from octree-structured SDF data. This choice is significant: Dual Contouring preserves sharp edges and produces better cliff and overhang features than Marching Cubes.

**Seed-based determinism deep-dive:**
The position of each star serves as its seed. Pseudorandom numbers generated from that position determine the entire planetary system -- orbital parameters, planet types, terrain, biomes, flora, and fauna. This enables 18.4 quintillion unique but fully deterministic planets.

The hierarchical seed derivation is critical:

1. **Galaxy seed** -> star position seeds
2. **Star position** -> system properties (star type, planet count, orbital parameters)
3. **Planet seed** -> terrain noise parameters, biome distribution, atmosphere
4. **Surface seeds** -> flora (L-systems), fauna, resource placement

**Key implementation patterns:**

- Hash functions (not sequential PRNGs) are used for spatially-indexed generation, so any location can be queried without generating all preceding locations
- Hierarchical seeds prevent the "butterfly effect" where changing one parameter cascades to everything downstream
- The engine is agnostic about content source: generative and hand-authored content are interchangeable through the same pipeline

**CPU-bound limitation:**
Notably, No Man's Sky performs mesh generation on the CPU, which was identified as the key performance bottleneck in their GDC 2017 presentation. The generation pipeline runs: voxel generation -> polygonization -> texturing -> population -> simulation, with continuous real-time generation while the player moves and no loading screens.

**Terrain deformability:**
Because the world is stored as voxel SDF data, terrain is deformable by players. This is a capability not available with pure noise-based heightmap systems.

Sources: [GDC Vault: Continuous World Generation in NMS](https://www.gdcvault.com/play/1024265/Continuous-World-Generation-in-No), [NMS Modding Wiki: Terrain Generation](https://nmsmodding.fandom.com/wiki/Terrain_Generation), [Rambus: Algorithms of No Man's Sky](https://www.rambus.com/blogs/the-algorithms-of-no-mans-sky-2/)

---

### 2.4 Elite Dangerous: Physics-Based Generation (Stellar Forge)

**Hierarchical physical simulation:**
Stellar Forge is not a noise approximation system. It performs actual physical simulation of planetary formation from nebulous gases through a top-down hierarchy: galaxy -> star system -> planet -> surface features. Real stellar catalog data (Hipparcos, Gliese catalogs) seeds the galaxy, with procedural generation filling in the remaining ~400 billion star systems to create a 1:1 Milky Way.

**Formation simulation pipeline:**

1. Galaxy-scale material and age distribution functions ensure correct spiral arm structure
2. Accretion simulation determines chemical composition, mass, and orbital parameters
3. Tectonic simulations model geological history
4. Crater placement reflects simulated bombardment history (not random scatter)
5. Surface noise functions take point position + planet ID + astronomical data as inputs

**Dual-precision floating point:**
Elite Dangerous uses both native 64-bit and emulated 64-bit (two 32-bit floats packed together) for millimeter precision across billions of millimeters of planetary surface. This dual-precision approach enables the system to work correctly on GPUs that lack native double-precision support.

**Surface rendering:**

- Cube-based quadtree subdivision with uniformly spaced vertices for both physics and rendering
- Above a certain LOD threshold: flat geometry with generated normal/height textures
- Below threshold: full tessellated geometry at target resolution
- Wang tiling for texture variation; triplanar blending on curved surfaces
- Material blending modulated by geological type

**Spatial encoding:**
64-bit integer encoding packs: sector coordinates + octree layer + system ID + body ID. This enables efficient server-client synchronization via the deterministic seed hierarchy -- clients can regenerate any location from its ID without downloading terrain data.

**Top-down data availability:**
A critical design principle: parent parameters are always accessible when generating children. When generating surface detail for a planet, the system has access to system-level properties (star type, age, metallicity) and planet-level properties (mass, composition, tectonic history). This enables physically informed noise rather than generic procedural noise.

Sources: [Stellar Forge Wiki](https://elite-dangerous.fandom.com/wiki/Stellar_Forge), [80.lv: Generating the Universe in Elite Dangerous](https://80.lv/articles/generating-the-universe-in-elite-dangerous), [PC Gamer: Science Behind Elite Dangerous Planets](https://www.pcgamer.com/the-mind-bending-science-behind-the-planets-of-elite-dangerous/)

---

### 2.5 GPU vs CPU Terrain Generation Benchmarks

**General speedup:** ~10x for GPU over CPU for fractal noise generation, normal map computation, and erosion simulation. The advantage comes from per-pixel noise evaluation being embarrassingly parallel.

**Specific benchmarks across systems:**

| System / Hardware                 | Operation               | Performance               | Year |
| --------------------------------- | ----------------------- | ------------------------- | ---- |
| GPU Gems 3 / GeForce 8800         | Marching cubes terrain  | 260 blocks/sec (Method 3) | 2008 |
| DX11 compute / ATI HD 6870        | Terrain generation pass | ~10 ms per pass           | 2012 |
| GPU Work Graphs / RX 7900 XTX     | Full procedural scene   | 3.13-3.74 ms              | 2024 |
| FastNoiseLite / Intel 7820X (CPU) | 3D Perlin               | 47.93M pts/sec            | --   |
| FastNoiseLite / Intel 7820X (CPU) | 3D Simplex              | 36.83M pts/sec            | --   |
| FastNoiseLite / Intel 7820X (CPU) | 3D Value                | 64.13M pts/sec            | --   |
| Compute shader generic            | Fractal noise vs CPU    | ~10x speedup              | --   |

**Noise algorithm performance comparison:**

| Algorithm           | Samples/pixel (3D) | Relative Cost   | Notes                                                |
| ------------------- | ------------------ | --------------- | ---------------------------------------------------- |
| Value noise         | 8 lookups          | 0.8x            | Lowest quality, fastest                              |
| Perlin              | 8 gradient lookups | 1.0x (baseline) | Grid artifacts at diagonals                          |
| Simplex             | 4 gradient lookups | ~0.7x in 3D     | Fewer artifacts, better scaling to higher dimensions |
| Worley/Voronoi      | N point searches   | 2-5x            | Cellular patterns                                    |
| FBm (8 octaves)     | 8 \* base          | 8x base         | Standard terrain                                     |
| Hybrid Multifractal | 8 \* base + extra  | ~9x base        | Best peaks/valleys ratio                             |

For planet generation using 3D noise on a sphere surface, simplex offers ~30% cost reduction per octave over Perlin. The advantage grows in higher dimensions (4D, 6D).

**Estimated compute times by resolution (modern GPU, RTX 4080 class, 8-octave simplex):**

| Resolution    | Pixels | Noise Only | Noise+Erosion (100 iter) | Full Pipeline |
| ------------- | ------ | ---------- | ------------------------ | ------------- |
| 1K (1024^2)   | 1.05M  | ~0.3 ms    | ~15 ms                   | ~25 ms        |
| 2K (2048^2)   | 4.19M  | ~1.0 ms    | ~55 ms                   | ~90 ms        |
| 4K (4096^2)   | 16.8M  | ~4 ms      | ~200 ms                  | ~350 ms       |
| 8K (8192^2)   | 67.1M  | ~16 ms     | ~800 ms                  | ~1.4 s        |
| 16K (16384^2) | 268M   | ~65 ms     | ~3.2 s                   | ~5.5 s        |
| 32K (32768^2) | 1.07B  | ~260 ms    | ~13 s                    | ~22 s         |

Full pipeline = noise + erosion + normal map + texturing + AO. Times are per cube face; multiply by 6 for full planet.

**Commercial tool reference points:**

- Instant Terra: interactive parameter changes at 16K x 16K resolution
- Gaea: 16x16 tile build (~16K) takes ~40 min CPU, ~10 min on 4-machine cluster
- World Machine: practical single-file limit ~8192x8192; tiling needed beyond
- GPU erosion at 2048x2048: interactive rates (~30+ iterations/sec)
- CPU erosion abandoned at 1024x1024 -- too slow (multiple sources confirm)

---

## 3. Noise Libraries & Tools

### 3.1 Noise Library Comparison

| Library           | Language(s)                             | Noise Types                                        | GPU Support                  | Fractal Modes                       | License | Status                                |
| ----------------- | --------------------------------------- | -------------------------------------------------- | ---------------------------- | ----------------------------------- | ------- | ------------------------------------- |
| **libnoise**      | C++                                     | Perlin, Ridged MF, Voronoi                         | None                         | Module chaining                     | LGPL    | Unmaintained (foundational reference) |
| **FastNoiseLite** | 15 languages (C/C++/Rust/HLSL/GLSL/...) | OpenSimplex2, Cellular, Perlin, Value, Value Cubic | HLSL, GLSL, CUDA (community) | FBm, Ridged, PingPong, Domain Warp  | MIT     | Active (v1.1.1, March 2024)           |
| **ANL**           | C++                                     | Perlin, Ridged MF, Gradient, 2D-6D                 | None                         | Node-graph composition              | --      | Low activity                          |
| **noise-rs**      | Rust                                    | Perlin, Simplex, Worley, Value                     | None                         | Module chaining (libnoise-inspired) | --      | Active (v0.9)                         |
| **webgl-noise**   | GLSL                                    | Perlin 2D/3D/4D, Simplex, Cellular                 | Native (GLSL)                | Manual octave stacking              | MIT     | Maintained (stegu fork)               |

**Key characteristics:**

**libnoise:** The `NoiseMapBuilderSphere` class generates equirectangular maps by sampling noise along the sphere surface using lat/lon coordinates. Tutorial 8 demonstrates a hierarchy of over 100 noise functions for planetary terrain. Modular architecture with generators, combiners, selectors, and modifiers. Foundational but superseded by faster libraries.

**FastNoiseLite:** The recommended general-purpose choice. Supports float and double precision. Dedicated HLSL and GLSL implementations for GPU acceleration. Community extensions include a CUDA wrapper (FastNoiseLiteCUDA). Domain warp support (Progressive and Independent modes) is particularly useful for terrain generation. Latest release: v1.1.1, 301 commits, 13 releases.

**ANL (Accidental Noise Library):** Provides 2D, 3D, 4D, and 6D noise variants with a node-graph architecture for connecting noise functions as black-box modules. Useful as an architectural reference for modular noise pipelines. Influenced the design of many subsequent libraries.

**noise-rs:** Rust-native port inspired by libnoise. Includes an explicit `complexplanet.rs` example demonstrating a hierarchy of over 100 noise functions for planetary terrain elevation. The natural choice for Rust projects but lacks GPU shader implementations.

**webgl-noise (Ashima Arts / Gustavson):** Self-contained GLSL noise with no dependency on external data (no lookup textures). Not quite as fast as texture-based implementations on desktop GPUs but more scalable and convenient. Includes `psrdnoise` functions for periodic, rotating, gradient-returning simplex noise. Makes good use of unused ALU resources when run concurrently with texture-intensive rendering.

### 3.2 Commercial Terrain Generators

**World Machine:**

- Industry-standard node-graph terrain generator
- Strong erosion toolset (hydraulic, thermal, snow)
- Supports very large terrains with tiled generation and streaming-friendly outputs
- Practical single-file limit ~8192x8192; tiling needed beyond
- Best for: established studio pipelines, large-scale tiling/streaming workflows
- Used by: CBS Studios, BBC, Kojima Productions, Microsoft, NASA

**Gaea (QuadSpinner):**

- Built by a former World Machine plugin (GeoGlyph) developer
- GPU-accelerated, emphasizing physically plausible terrains with advanced erosion
- Three workflow modes: Layers, Graph, Sculpt
- Directed erosion system (user-guided erosion paths)
- Preview: up to 4K (experimental); production builds up to 256K with Professional edition
- Erosion 2: up to 10x faster than v1
- Weakness: poor documentation hampers learning curve
- Best for: most realistic, physically plausible terrains; AAA or cinematic quality
- Used by: Lucasfilm Games, Larian Studios

**World Creator:**

- 100% GPU-powered, real-time WYSIWYG terrain creation
- Artist-friendly with strong brush tools, procedural presets, and integrated vegetation/object placement
- Best for: rapid prototyping, level design, quick iteration within a single app

### 3.3 Blender A.N.T. Landscape Addon

"Another Noise Tool" uses different procedural noises to generate landscapes directly in Blender. Was bundled with Blender 4.1; now an extension with limited support.

- **Settings groups:** Main Settings (object/mesh size, subdivisions), Noise Settings (noise type, octaves, frequency, lacunarity), Displace Settings (terrain height, edge falloff)
- **Under the hood:** Noise is a black-and-white procedural texture; height is a multiply operation, offset is an add operation
- **Extended version:** TXA (Textured ANT) addon adds texture resolution controls
- **Planet use:** Can generate on sphere primitives for planet-like objects; multiple noise types and fractal options available

Other Blender planet tools: b3dplanetgen (open-source addon), Procedural Planet Generator by jumpingpuzzle (26 pre-designed node groups, commercial), Planet and Space Objects Generator by Hlavka (open-source), and various free procedural planet shader collections on ArtStation.

---

## 4. Simulation Frameworks

### 4.1 N-Body Simulation Frameworks

#### REBOUND

Multi-purpose N-body integrator for gravitational dynamics. Simulates motion of particles under gravity -- planets, moons, ring particles, dust.

- **Integrators:** Leap-frog, Symplectic Epicycle Integrator (SEI), Wisdom-Holman (WHFast), IAS15 (high-accuracy adaptive)
- **Key features:** Barnes-Hut tree gravity, plane-sweep collision detection, MPI + OpenMP parallelization, Python and C API
- **Install:** `pip install rebound`
- **Used for:** Planetary ring dynamics, planet formation, orbital mechanics, asteroid dynamics, exoplanet transit timing
- **License:** GPL, 100% open source
- [github.com/hannorein/rebound](https://github.com/hannorein/rebound), [Rein & Liu 2012, A&A 537, A128](https://www.aanda.org/articles/aa/full_html/2012/01/aa18085-11/aa18085-11.html)

#### ChaNGa (Charm N-body GrAvity solver)

Collisionless and SPH N-body simulations with cosmological or isolated boundary conditions.

- **Key features:** Barnes-Hut tree gravity (derived from PKDGRAV), SPH hydrodynamics (from GASOLINE), Charm++ parallelization for extreme scalability
- **Used for:** Galaxy formation, cosmological simulations, protoplanetary disk modeling
- **License:** GPL v2
- [github.com/N-BodyShop/changa](https://github.com/N-BodyShop/changa), [Jetley et al. 2008](https://charm.cs.illinois.edu/newPapers/08-03/paper.pdf)

#### PKDGRAV3

High-performance N-body code optimized for trillion-particle cosmological simulations.

- **Key features:** Fast Multipole Method (FMM), individual adaptive time steps, GPU acceleration, designed for supercomputer-scale runs
- **Used for:** Cosmological structure formation, planetesimal dynamics (with collision modules like EDACM)
- [github.com/wullm/pkdgrav3](https://github.com/wullm/pkdgrav3), [Potter et al. 2017](https://comp-astrophys-cosmol.springeropen.com/articles/10.1186/s40668-017-0021-1)

### 4.2 Mantle Convection Solvers

#### ASPECT (Advanced Solver for Problems in Earth's ConvecTion)

Parallel finite element code for thermal convection in planetary mantles with adaptive mesh refinement.

- **Built on:** deal.II (FEM), Trilinos (linear algebra), p4est (parallel meshes)
- **Key features:** Modular plugin architecture, multigrid solvers, scales to hundreds-to-thousands of cores
- **Used for:** Mantle convection, lithosphere deformation, inner core convection, subduction modeling
- **License:** GPL v2+
- [aspect.geodynamics.org](https://aspect.geodynamics.org/), [Heister et al. 2017, GJI 210(2):833](https://academic.oup.com/gji/article/210/2/833/3807083)

#### CitcomS

Finite element code for compressible thermochemical convection in spherical shell geometry.

- **Key features:** Spherical geometry (full or regional), compressible formulation, MPI parallelization
- **Validation:** Benchmarked against ASPECT with results agreeing within 1%
- **License:** GPL v2
- [github.com/geodynamics/citcoms](https://github.com/geodynamics/citcoms), [GMD 2023 benchmark](https://gmd.copernicus.org/articles/16/3221/2023/)

#### TerraFERMA (Transparent Finite Element Rapid Model Assembler)

Multiphysics code for rapid, reproducible construction of coupled geodynamic models.

- **Built on:** FEniCS (problem description), PETSc (solvers), SPuD (options management)
- **Key features:** Emphasizes transparency and reproducibility
- **Used for:** Mantle convection, subduction zone modeling, magma transport
- **License:** LGPL v3+
- [terraferma.github.io](https://terraferma.github.io/), [Wilson & Spiegelman 2017, G-Cubed 18:769-810](https://agupubs.onlinelibrary.wiley.com/doi/full/10.1002/2016GC006702)

#### Underworld2

Particle-in-cell finite element code for 2D/3D geodynamics with Python interface.

- **Key features:** Hybrid mesh+particle approach (accurate velocity on mesh, accurate material tracking on particles), Jupyter notebook workflow, UWGeodynamics high-level module for rapid prototyping
- **License:** BSD-like
- [underworldcode.org](https://www.underworldcode.org/intro-to-underworld/), [Mansour et al. 2020, JOSS](https://www.theoj.org/joss-papers/joss.01797/10.21105.joss.01797.pdf)

### 4.3 Input Parameter System: Stellar Composition as Master Control

**Key insight from modern exoplanet science:** Stellar composition is the master control variable. The host star's elemental abundances (especially Fe, Si, Mg, C, O) set the building blocks available for planet formation, while orbital distance determines which volatiles condense into solids.

**Principal initial conditions:**

| Parameter            | Role                                       | Effect on Final State                                                                  |
| -------------------- | ------------------------------------------ | -------------------------------------------------------------------------------------- |
| Stellar [Fe/H]       | Sets total metal budget                    | Higher metallicity -> more solid material -> easier giant planet formation             |
| Stellar Mg/Si ratio  | Controls silicate mineralogy               | High Mg/Si -> forsterite (Mg2SiO4) dominated; low Mg/Si -> enstatite (MgSiO3) + quartz |
| Stellar C/O ratio    | Controls volatile chemistry                | Low C/O -> more water ice available beyond snow line                                   |
| Disk mass & lifetime | Sets available material and accretion time | Determines maximum achievable planet mass                                              |
| Orbital distance     | Determines condensation temperature        | Inside snow line: dry, Fe/Si-rich. Outside: water ice incorporated                     |
| Formation timing     | When embryo reaches pebble isolation mass  | Early -> gas accretion -> gas giant; late -> rocky/icy                                 |

**Minimal physical parameter set for procedural generation (6 parameters):**

1. Mass (log-uniform, ~0.1 to ~1000 Earth masses)
2. Semi-major axis (log-uniform, ~0.01 to ~100 AU)
3. Stellar luminosity (determines equilibrium temperature)
4. Fe/Si ratio (0.5 to 2.0, from stellar abundances)
5. Water fraction (0 to 0.5, from formation location)
6. Age (0.1 to 10 Gyr)

From these six, one can derive: radius, surface gravity, atmospheric retention, surface temperature, tectonic regime, ocean coverage, and atmospheric composition.

**For visual/terrain generation, add:** rotation rate (weather patterns, oblateness), obliquity (seasonal variation), and a noise seed (deterministic procedural surface detail).

Sources: [Bitsch & Battistini 2020, A&A](https://www.aanda.org/articles/aa/full_html/2022/04/aa42738-21/aa42738-21.html), [Cabral et al. 2023, A&A](https://www.aanda.org/articles/aa/full_html/2023/10/aa46697-23/aa46697-23.html)

### 4.4 Parameterized Convection Scaling Laws

Thermal evolution of terrestrial planets is modeled using parameterized convection, reducing the full 3D convection problem to a 1D energy balance. The key relationship is between the Nusselt number (Nu, dimensionless heat flux) and the Rayleigh number (Ra, convective vigor):

```
Nu = a * Ra^beta
```

**Key values of beta:**

- Classical boundary layer theory: beta = 1/3
- Numerical simulations in spherical shells (basally heated): beta = 0.294 +/- 0.004
- Internally heated systems: beta = 0.337 +/- 0.009
- At Ra = 10^9, beta = 0.29 gives ~32% lower surface heat flux than beta = 1/3

**Rayleigh number definition:**

```
Ra = (rho * g * alpha * delta_T * d^3) / (kappa * eta)
```

where rho = density, g = gravity, alpha = thermal expansivity, delta_T = temperature contrast, d = mantle thickness, kappa = thermal diffusivity, eta = viscosity.

**Stagnant lid vs. plate tectonics regimes:**
Different tectonic modes produce dramatically different cooling histories. Stagnant lid planets (Mars, likely most rocky exoplanets) cool slower because the rigid lid insulates the interior:

- Plate tectonics: Nu ~ Ra^(1/3)
- Stagnant lid: Nu ~ Ra_i^(1/3) \* exp(-theta/3), where theta is the Frank-Kamenetskii parameter

**Cooling timescale:**

```
tau_cool ~ (rho * Cp * R^2) / k
```

For Earth-sized planets: ~10 Gyr. For super-Earths, the timescale increases as R^2, so larger planets retain heat longer and remain geologically active longer.

Sources: [Wolstencroft et al. 2009, PEPI](https://www.sciencedirect.com/science/article/pii/S0031920109001216), [Tosi et al. 2019, PEPI](https://www.sciencedirect.com/science/article/abs/pii/S0031920118301936), [Stevenson et al. 1983](https://www.sciencedirect.com/science/article/abs/pii/0040195181902055)

### 4.5 Mass-Radius Relations

#### Empirical (Observed Population)

Broken power law with three regimes:

| Regime       | Mass Range                | Relation      | Planet Type                      |
| ------------ | ------------------------- | ------------- | -------------------------------- |
| Small        | M < ~4.4 Earth masses     | R ~ M^0.27    | Rocky/terrestrial                |
| Intermediate | ~4.4 to ~127 Earth masses | R ~ M^0.67    | Sub-Neptunes, Neptunes           |
| Giant        | > ~127 Earth masses       | R ~ M^(-0.06) | Gas giants (electron degeneracy) |

Source: [Bashi et al. 2017, A&A](https://www.aanda.org/articles/aa/full_html/2017/08/aa29922-16/aa29922-16.html)

#### Theoretical (Composition-Dependent)

For solid exoplanets (Seager et al. 2007), valid up to ~20 Earth masses:

```
log10(R/R_earth) = k1 + (1/3) * log10(M/M_earth) - k2 * (M/M_earth)^k3
```

where k1, k2, k3 depend on composition.

Zeng et al. models provide specific curves for: pure iron (smallest radius), Earth-like (32.5% Fe + 67.5% MgSiO3), pure MgSiO3, 50% H2O worlds, 100% H2O, and H2/He envelopes (0.1-5%, dramatically inflated radii).

#### ML-Based

- **ExoMDN** (Baumeister et al. 2023): Mixture Density Network trained on 5.6 million synthetic planets. Given mass, radius, and equilibrium temperature, returns full posterior distributions in < 1 second. Open source: [github.com/philippbaumeister/ExoMDN](https://github.com/philippbaumeister/ExoMDN)
- **Random forest models** (Marif et al. 2023): Capture non-linear dependence of radius on mass, stellar irradiation, and age better than simple power laws
- **Bayesian MCMC methods**: Rigorous uncertainty quantification but computationally expensive (hours per planet)

---

## 5. GPU Pipeline Patterns

### 5.1 Async Compute Pipeline Architecture

```
Frame N Timeline:
=========================================================================

Graphics Queue:    [Shadow Maps N]---[G-Buffer N]---[Lighting N]---[Post N]
                                                        |
Compute Queue 0:   [Noise Gen face A]---[Erosion face A]---[Normal+AO face A]
                         |                    |
Compute Queue 1:   [Noise Gen face B]---[Erosion face B]---[Normal+AO face B]
                                                                |
Copy Queue:        [Upload tile data]----------[Readback results]--------
```

**Key principles (from NVIDIA and AMD documentation):**

1. Overlap workloads using different datapaths: FP/ALU, Memory, RT Core, Tensor Core, Graphics pipe
2. Avoid overlapping workloads that read/write the same resource (data hazards)
3. Avoid combining high L1/L2 cache usage + high VRAM throughput workloads (cache thrashing)
4. Single compute queue usually sufficient -- AMD: "no significant benefit from more than one compute queue"
5. Manual work scheduling outperforms automatic overlap
6. Subchannel switches trigger Wait-For-Idle (WFI) draining all warps -- async compute fills these gaps
7. Command lists must be large enough to justify fence synchronization overhead

**Recommended overlap combinations:**

- Math-limited noise compute + shadow map rasterization (graphics-pipe dominated)
- DLSS (Tensor-heavy) + acceleration structure building (FP/ALU dependent)
- Post-processing frame N + shadow maps frame N+1
- Erosion simulation (memory bandwidth bound) + G-buffer fill (graphics bound)

**Measured gains:**

- Double-buffered data uploads via copy queue: ~10% of total frame time saved (AMD measured)
- General async compute overlap: 5-30% throughput gain depending on workload complementarity

Sources: [NVIDIA: Advanced API Performance Async Compute](https://developer.nvidia.com/blog/advanced-api-performance-async-compute-and-overlap/), [AMD GPUOpen: Concurrent Execution Async Queues](https://gpuopen.com/learn/concurrent-execution-asynchronous-queues/)

### 5.2 Progressive Refinement (Preview-to-Full-Quality)

```
Stage 0 (Instant):     Low-res noise (256^2), no erosion
                        Display: flat-shaded preview
                        Time: < 1 ms

Stage 1 (~50ms):       Medium noise (1K), 10 erosion iterations
                        Display: basic heightmap + simple texture
                        Time: ~50 ms

Stage 2 (~500ms):      High noise (4K), 100 erosion iterations
                        Display: eroded terrain + biome colors
                        Time: ~350 ms

Stage 3 (~5s):         Full noise (16K), 500 erosion iterations
                        + normal maps + AO
                        Display: production quality
                        Time: ~5.5 s

Stage 4 (background):  Ultra (32K), 1000+ erosion iterations
                        + detail textures + PBR materials
                        Display: final export quality
                        Time: ~22 s per face
```

**Implementation approach (Khoury dual-framebuffer technique):**

- Maintain two alternating framebuffers
- Track which texture regions are new vs. reusable
- Discretize displacement vectors to prevent interpolation smoothing
- Each stage reads from previous stage's output, computes only the delta
- Progressive mesh uses 4-8 refinement scheme (twice as gradual as quadtree transitions)
- ROAM (Real-time Optimally Adapting Mesh) allows fine-tuning priority thresholds, frame rate limits, or triangle count caps

### 5.3 Multi-GPU Strategies

**Strategy 1: Split by cube face (RECOMMENDED)**

```
GPU 0: faces 0,1        (2 faces)
GPU 1: faces 2,3        (2 faces)
GPU 2: faces 4,5        (2 faces)
Compose: stitch edges on host or via peer-to-peer transfer
```

- Natural parallelism: faces are independent except at edges
- Edge stitching requires ~256 pixels of overlap per edge
- Near-linear scaling (3 GPUs -> ~2.8x speedup)
- Matches CHOPIN sort-last rendering paradigm
- Best for planet generation: minimal inter-GPU communication

**Strategy 2: Split by octave**

```
GPU 0: octaves 1-3 (low frequency, large features)
GPU 1: octaves 4-6 (medium frequency)
GPU 2: octaves 7-9 (high frequency, fine detail)
Final: sum all octave results on GPU 0
```

- Requires inter-GPU transfers of full-resolution buffers per octave
- Lower bandwidth efficiency due to serial dependency on summation
- Better for very high octave counts (12+)

**Strategy 3: Data parallelism (tile-based)**

```
16K heightmap split into 4K tiles:
GPU 0: tiles [0,0]-[1,1]   (4 tiles)
GPU 1: tiles [2,0]-[3,1]   (4 tiles)
GPU 2: tiles [0,2]-[1,3]   (4 tiles)
GPU 3: tiles [2,2]-[3,3]   (4 tiles)
Compose: overlap borders for erosion continuity
```

- Most flexible approach
- Erosion requires halo exchange (border cells) between tiles each iteration
- Gaea uses this approach: "4-machine network drops 40-minute build to 10-12 minutes"
- Scaling limited by halo exchange bandwidth at high iteration counts

### 5.4 Memory Bandwidth Analysis

**Per-operation bandwidth consumption at 16K resolution (268M pixels per face):**

| Operation                          | Bytes/pixel | Total @ 16K | Notes                               |
| ---------------------------------- | ----------- | ----------- | ----------------------------------- |
| 8-octave noise read                | 128B        | 34.4 GB     | 16B per 3D texture fetch            |
| Erosion (per iter)                 | ~96B (R+W)  | 25.8 GB     | height+water+flux+velocity+sediment |
| Normal map gen                     | 20B (R+W)   | 5.4 GB      | 3x3 neighborhood read               |
| AO computation                     | ~256B R     | 68.7 GB     | 32 rays \* 8 samples each           |
| Texturing                          | ~80B (R+W)  | 21.5 GB     | multi-layer material lookup         |
| **Total (noise + 1 erosion iter)** |             | **~155 GB** |                                     |

**Modern GPU bandwidth context:**

- RTX 4090: 1,008 GB/s -> single face noise + 1 erosion iter: ~154 ms bandwidth-limited floor
- RX 7900 XTX: 960 GB/s -> similar
- RTX 5090: ~1,792 GB/s -> ~86 ms

**Mitigation strategies:**

1. **GPU Work Graphs:** Eliminate CPU-GPU round trips entirely (34.8 GB -> 51 KB demonstrated)
2. **Texture compression** (S3TC/BC7): 4x memory reduction, hardware decompressed at no ALU cost
3. **Kernel fusion:** Combine noise + erosion + texturing into single dispatch to maximize L1 cache hits
4. **Tile-based compute:** Process 64x64 tiles to fit in shared memory (typical 128 KB L1/shared)
5. **Mesh shaders:** Lower memory footprint, less VRAM access, frees bandwidth for compute
6. **Static allocation** (SpaceEngine approach): Eliminates allocation overhead completely
7. **Incremental updates** (Khoury approach): Only recompute changed regions

### 5.5 GPU Erosion Simulation

**Shallow-water hydraulic erosion (GPU compute shader):**

- Grid cells store: terrain height, water height, suspended sediment, outflow flux (4 directions), velocity vector
- Per iteration: ~48 bytes read + 48 bytes write per cell
- Pipe model: virtual pipes between cells model water flow
- Cost scales linearly with cell count (confirmed in literature)
- At 2048x2048: interactive rates (~30+ iterations/sec on modern GPU)
- At 4096x4096: still interactive for single iterations
- CPU abandoned at 1024x1024 -- too slow (multiple sources confirm)
- Single floating-point buffer approach (ignoring race conditions) is fastest; converges after extra iterations despite non-deterministic intermediates

**Thermal erosion:**

- Simpler than hydraulic; talus angle threshold drives material redistribution
- Can be merged into hydraulic pass with virtual pipe model
- Lower compute cost per iteration

### 5.6 Hash Functions for GPU (Brief)

Jarzynski & Olano (JCGT 2020) evaluated hash functions for procedural generation on GPU. Key recommendations:

- **pcg3d / pcg4d:** Best quality/speed tradeoff for multidimensional hashing
- **xxhash32:** Good default for non-multidimensional cases (~50x faster than MD5)
- **PCG hash (32-bit, RXS-M-XS):** Recommended default GPU hash function -- better performance and statistical quality than Wang hash

```glsl
uint pcg_hash(uint input) {
    uint state = input * 747796405u + 2891336453u;
    uint word = ((state >> ((state >> 28u) + 4u)) ^ state) * 277803737u;
    return (word >> 22u) ^ word;
}
```

For detailed GPU hash usage in erosion and terrain compute shaders, see `gpu-compute-erosion.md`.

Sources: [JCGT paper](https://jcgt.org/published/0009/03/02/), [Nathan Reed summary](https://www.reedbeta.com/blog/hash-functions-for-gpu-rendering/)

### 5.7 Equirectangular Seam Handling and Pole Distortion

**Seam at +/-180 degrees longitude:**
When using 3D noise sampled on the sphere, there is no seam at the antimeridian because the 3D sampling is inherently continuous. Problems only arise with 2D noise approaches.

**Generating seamless equirectangular maps (the gold standard):**
Sample 3D noise on the sphere surface -- iterate over spherical coordinates (phi, theta), convert to Cartesian (x, y, z), and sample 3D noise at those points:

```
For each pixel (u, v):
    theta = u * 2*PI          // longitude
    phi   = v * PI            // latitude
    x = sin(phi) * cos(theta)
    y = cos(phi)
    z = sin(phi) * sin(theta)
    height = noise3D(x * scale, y * scale, z * scale)
```

**Pole distortion problem:**
The top and bottom rows each represent a single point stretched across the full width, causing extreme feature stretching, changed noise statistics, and "pinching" artifacts near poles.

**Solutions (ranked by effectiveness):**

1. **3D noise sampling** (above) -- uniform noise density on sphere surface
2. **Cubemap approach:** Generate noise on six cube faces, then convert to equirectangular. Cube corners distort less dramatically than equirectangular poles
3. **Post-processing:** Apply latitude-dependent filtering to compensate for density variation
4. **Adaptive sampling:** Increase sampling density near poles in the equirectangular image

**Resolution standards:**

| Resolution | Dimensions   | Use Case                                  |
| ---------- | ------------ | ----------------------------------------- |
| 4K         | 4096 x 2048  | Distant viewing, game planets             |
| 8K         | 8192 x 4096  | High quality, close flyby                 |
| 16K        | 16384 x 8192 | Very high quality, requires LOD streaming |

Width is always double height. Higher resolutions require more noise octaves to avoid visible smoothness at close range.

---

## 6. ML/AI Terrain Generation

### 6.1 GANs (Generative Adversarial Networks)

**Early approach (Beckham & Pal, 2017):** Spatial GANs trained on NASA satellite imagery (heightmaps + textures as 4-channel images). Limited to random generation without user control. Historically significant as the first GAN-based terrain approach.

Source: [github.com/christopher-beckham/gan-heightmaps](https://github.com/christopher-beckham/gan-heightmaps)

### 6.2 Diffusion Models (Current State-of-the-Art)

**Earthbender (SIGGRAPH MIG 2025):**
Interactive system for sketch-based terrain heightmap generation using a guided diffusion model. Uses a custom-trained ControlNet steering Stable Diffusion v1.5. Multi-channel semantic sketch input: red = mountains, blue = rivers/roads, green = lakes. Significantly outperforms traditional GANs (Pix2PixHD) in data efficiency and structural fidelity.

Source: [ACM - Earthbender](https://dl.acm.org/doi/full/10.1145/3769047.3769053)

**TerraFusion (2025):**
Joint generation of terrain geometry AND texture using latent diffusion models. User-guided control over the generation process, addressing the limitation of geometry-only or texture-only approaches.

Source: [ArXiv - TerraFusion](https://arxiv.org/html/2505.04050v1)

### 6.3 Style Transfer (2024)

Combines procedural noise generation with Neural Style Transfer, drawing style from real-world height maps. Achieves diverse terrains aligned with real-world morphological characteristics at low computational cost. Evaluated using Structural Similarity (SSIM) metric.

Source: [ArXiv](https://arxiv.org/html/2403.08782v1), [GitHub](https://github.com/fmerizzi/Procedural-terrain-generation-with-style-transfer)

### 6.4 Wave Function Collapse for Biome Constraint Satisfaction

WFC generates content by propagating constraints from an example or rule set.

**Applied to terrain:**

- **Two-pass approach:** First pass generates biome distribution (forest, sea, desert, etc.), second pass generates terrain specific to each biome
- **Consistency management:** Decide biome type in advance, disable tiles that do not fit that biome
- **Terrain heightmaps via WFC:** Recent work (ArXiv, Dec 2024) applies WFC to SRTM elevation data, using slopes as input rather than raw heights. Statistical analysis confirms structural characteristics are preserved
- **Strengths:** Guaranteed constraint satisfaction (no invalid biome adjacencies), controllable output, deterministic from seed
- **Weaknesses:** Slower than pure noise approaches, requires careful tile/rule design, can fail to converge if constraints are overconstrained

Sources: [github.com/mxgmn/WaveFunctionCollapse](https://github.com/mxgmn/WaveFunctionCollapse), [ArXiv WFC for SRTM (2024)](https://arxiv.org/abs/2412.04688), [Boris the Brave: WFC Tips](https://www.boristhebrave.com/2020/02/08/wave-function-collapse-tips-and-tricks/)

### 6.5 AutoBiomes: Multi-Biome Landscapes

Academic system (The Visual Computer, 2020) for generating vast terrains with plausible biome distributions. Combines synthetic procedural terrain generation with digital elevation models and simplified climate simulation.

- **Pipeline:** Temperature, wind, and precipitation simulation -> biome distribution -> asset placement via rule-based local-to-global model
- **Key contribution:** Addresses multi-biome landscape generation (scarcely explored at time of publication)

Source: [Springer - AutoBiomes](https://link.springer.com/article/10.1007/s00371-020-01920-7)

---

## 7. Anti-Repetition Techniques

### 7.1 Wang Tiling (Non-periodic Tiling)

Modifications to procedural noise functions can directly produce Wang tile sets, enabling non-periodic tiling at small performance cost while maintaining or reducing memory consumption. The key insight is that tile boundaries are designed to match seamlessly regardless of arrangement, and aperiodic tile sets prevent the eye from detecting repeating patterns.

Source: [ACM - Non-periodic Tiling of Procedural Noise Functions (2018)](https://dl.acm.org/doi/10.1145/3233306)

### 7.2 Procedural Stochastic Texturing

Unity Labs technique that procedurally generates infinite textures matching input appearance, avoiding tiling. Assigns random offsets and orientations per tile with smooth interpolation at boundaries. Enables a single small texture to cover arbitrarily large surfaces without visible repetition.

Source: [Unity - Procedural Stochastic Texturing](https://unity.com/archive/blog/engine-platform/procedural-stochastic-texturing-in-unity)

### 7.3 Inigo Quilez Texture Variation

Assigns random offsets and orientations to each tile, with smooth floating-point index transitions across boundaries to interpolate between virtual patterns. Simple to implement in shaders, low performance overhead, widely adopted in ShaderToy and production pipelines.

Source: [iquilezles.org - Texture Repetition](https://iquilezles.org/articles/texturerepetition/)

### 7.4 Elite Dangerous Approach

Elite Dangerous uses Wang tiling combined with triplanar blending on curved surfaces, with material blending modulated by geological type. This provides both micro-scale anti-repetition (Wang tiles) and macro-scale variation (geology-driven material mixing).

---

## 8. References

### Production Systems

1. SpaceEngine Terrain 2.0 - https://spaceengine.org/news/blog190328/
2. SpaceEngine Terrain Engine Upgrade #1 - https://spaceengine.org/news/blog171016/
3. SpaceEngine Terrain Engine Upgrade #3 - https://spaceengine.org/news/blog171120/
4. SpaceEngine Procedural Generation Wiki - https://spaceengine.fandom.com/wiki/Procedural_Generation
5. Outerra Official - https://www.outerra.com/
6. Outerra Procedural Grass Performance - https://outerra.blogspot.com/2016/01/procedural-rendering-performance-test-1.html
7. GDC Vault: Continuous World Generation in NMS - https://www.gdcvault.com/play/1024265/Continuous-World-Generation-in-No
8. NMS Modding Wiki: Terrain Generation - https://nmsmodding.fandom.com/wiki/Terrain_Generation
9. Rambus: Algorithms of No Man's Sky - https://www.rambus.com/blogs/the-algorithms-of-no-mans-sky-2/
10. Stellar Forge Wiki - https://elite-dangerous.fandom.com/wiki/Stellar_Forge
11. 80.lv: Generating the Universe in Elite Dangerous - https://80.lv/articles/generating-the-universe-in-elite-dangerous
12. PC Gamer: Science Behind Elite Dangerous Planets - https://www.pcgamer.com/the-mind-bending-science-behind-the-planets-of-elite-dangerous/

### Noise Libraries & Tools

13. libnoise - https://libnoise.sourceforge.net/
14. FastNoiseLite - https://github.com/Auburn/FastNoiseLite
15. Accidental Noise Library - https://accidentalnoise.sourceforge.net/
16. noise-rs - https://github.com/Razaekel/noise-rs
17. webgl-noise (stegu fork) - https://github.com/stegu/webgl-noise
18. World Machine - https://www.world-machine.com/
19. Gaea (QuadSpinner) - https://quadspinner.com/
20. A.N.T. Landscape - https://extensions.blender.org/add-ons/antlandscape/

### Simulation Frameworks

21. REBOUND - https://github.com/hannorein/rebound
22. ChaNGa - https://github.com/N-BodyShop/changa
23. PKDGRAV3 - https://github.com/wullm/pkdgrav3
24. ASPECT - https://aspect.geodynamics.org/
25. CitcomS - https://github.com/geodynamics/citcoms
26. TerraFERMA - https://terraferma.github.io/
27. Underworld2 - https://www.underworldcode.org/intro-to-underworld/

### GPU Pipeline & Performance

28. NVIDIA GPU Gems 3, Ch. 1 - https://developer.nvidia.com/gpugems/gpugems3/part-i-geometry/chapter-1-generating-complex-procedural-terrains-using-gpu
29. GPU Work Graphs (ACM HPG 2024) - https://dl.acm.org/doi/10.1145/3675376
30. AMD GPUOpen: Work Graphs Procedural Generation - https://gpuopen.com/learn/work_graphs_mesh_nodes/work_graphs_mesh_nodes-procedural_generation/
31. NVIDIA: Advanced API Performance Async Compute - https://developer.nvidia.com/blog/advanced-api-performance-async-compute-and-overlap/
32. AMD GPUOpen: Concurrent Execution Async Queues - https://gpuopen.com/learn/concurrent-execution-asynchronous-queues/
33. Jad Khoury: Procedural Planet Rendering - https://jadkhoury.github.io/terrain_blog.html
34. CDLOD paper - https://aggrobird.com/files/cdlod_latest.pdf
35. Geometry Clipmaps (SIGGRAPH 2004) - https://hhoppe.com/geomclipmap.pdf

### Hash Functions & Determinism

36. Jarzynski & Olano: Hash Functions for GPU (JCGT 2020) - https://jcgt.org/published/0009/03/02/
37. Nathan Reed: Hash Functions for GPU Rendering - https://www.reedbeta.com/blog/hash-functions-for-gpu-rendering/

### Equirectangular & Projection

38. Toni Sagrista: Procedural Planetary Surfaces - https://tonisagrista.com/blog/2021/procedural-planetary-surfaces/
39. Paul Bourke: Converting Cubemaps - https://paulbourke.net/panorama/cubemaps/
40. Acko.net: Making Worlds 1 - https://acko.net/blog/making-worlds-1-of-spheres-and-cubes/

### ML/AI Terrain

41. Earthbender (SIGGRAPH MIG 2025) - https://dl.acm.org/doi/full/10.1145/3769047.3769053
42. TerraFusion (2025) - https://arxiv.org/html/2505.04050v1
43. Style Transfer terrain (2024) - https://arxiv.org/html/2403.08782v1
44. GAN heightmaps (Beckham & Pal, 2017) - https://github.com/christopher-beckham/gan-heightmaps
45. WaveFunctionCollapse - https://github.com/mxgmn/WaveFunctionCollapse
46. WFC for SRTM terrain (2024) - https://arxiv.org/abs/2412.04688
47. AutoBiomes (2020) - https://link.springer.com/article/10.1007/s00371-020-01920-7

### Anti-Repetition

48. Wang Tiling of Procedural Noise (ACM 2018) - https://dl.acm.org/doi/10.1145/3233306
49. Unity Procedural Stochastic Texturing - https://unity.com/archive/blog/engine-platform/procedural-stochastic-texturing-in-unity
50. Inigo Quilez: Texture Repetition - https://iquilezles.org/articles/texturerepetition/

### Planet Science & Mass-Radius

51. Bitsch & Battistini 2020, A&A - https://www.aanda.org/articles/aa/full_html/2022/04/aa42738-21/aa42738-21.html
52. Zeng et al. 2019, PNAS - https://lweb.cfa.harvard.edu/~lzeng/planetmodels.html
53. Seager et al. 2007, ApJ 669 - https://arxiv.org/abs/0707.2895
54. Bashi et al. 2017, A&A - https://www.aanda.org/articles/aa/full_html/2017/08/aa29922-16/aa29922-16.html
55. ExoMDN - https://github.com/philippbaumeister/ExoMDN
56. Dorn et al. 2015, A&A - https://www.aanda.org/articles/aa/full_html/2015/05/aa24915-14/aa24915-14.html

### Thermal Evolution & Scaling Laws

57. Wolstencroft et al. 2009, PEPI - https://www.sciencedirect.com/science/article/pii/S0031920109001216
58. Tosi et al. 2019, PEPI - https://www.sciencedirect.com/science/article/abs/pii/S0031920118301936
59. Stevenson et al. 1983, Tectonophysics - https://www.sciencedirect.com/science/article/abs/pii/0040195181902055

---

_Total distinct sources: 59_
_Consolidated from 3 research documents, 2026-04-02_
