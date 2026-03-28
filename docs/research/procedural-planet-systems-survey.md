# Procedural Planet Generation Systems & Performance Optimization Survey
_Research date: 2026-03-28 (updated with performance focus)_

---

## 1. Existing Systems

### 1.1 SpaceEngine

**Technical approach:**
- Fractal noise (multi-octave Perlin noise) for heightmap generation
- Cube-sphere projection: each planet face is a quadtree, six faces per planet, vertices "folded" to sphere
- Quadtree LOD: nodes subdivide when textures become too stretched on screen (< 256px threshold)
- Biome assignment via shaders (not latitude-based), using palette presets per planet class
- OpenGL + GLSL pipeline, written in C++

**Resolution / detail:**
- Texture nodes: 256x256 pixels each
- Maximum quadtree depth: level 12 for Earth-sized planets
- Effective resolution per cube face: 256 * 2^12 = 1,048,576 px (approximately 1 terapixel total)
- Surface detail down to ~10m per triangle in geometry, extended to millimeters via shader detail texturing
- 4 slope levels x 4 layers per biome; two large-scale levels (10m/px) mixed with two shader-blended layers

**Performance:**
- VRAM: 2+ GB with S3TC texture compression (2-4x memory reduction, hardware-decompressed)
- RAM for terrain: 100-150 MB
- Static memory allocation: large texture array pre-allocated at startup (eliminates dynamic allocation overhead, 10x loading speed improvement)
- Loading at LOD 0: ~5 seconds; LOD 1: ~30 seconds (RTX 2080, 4K display)
- Loading speed 8 -> 60+ fps; speed 20 -> 18 fps with 12-second LOD 1 loading
- Texture arrays eliminate per-node texture unit switching overhead
- Maximum cache: 2,048 textures per type (OpenGL limit)

**Notable innovations:**
- Meshless terrain rendering: geometry built entirely in vertex shader via heightmap texture fetch
- Geometry independent of textures: mountain silhouettes load before texture data
- Priority-based loading: closer areas load first; normal maps before color (normals more perceptually important)
- Detail textures with coordinate distortion to reduce tiling artifacts
- Two-shader approach: complex close-range shader vs. fast distant shader
- Climate model with energy transport, greenhouse effects, tidal locking (physically derived)

Sources:
- [SpaceEngine Terrain 2.0](https://spaceengine.org/news/blog190328/)
- [Terrain Engine Upgrade #1](https://spaceengine.org/news/blog171016/)
- [Terrain Engine Upgrade #3](https://spaceengine.org/news/blog171120/)
- [Procedural Generation Wiki](https://spaceengine.fandom.com/wiki/Procedural_Generation)

---

### 1.2 Outerra

**Technical approach:**
- Chunked LOD based on quadtree subdivision over a spherical planet
- 3 independent fractal noise channels per quadtree node: (1) elevation seeded from heightmap data, (2-3) detail material mixing; 4th channel: global slope
- Adaptive mesh tessellation in vertex shaders; deforms spherical base mesh with vertical elevation + horizontal displacement
- OpenGL 3.3+, fully asynchronous: majority on GPU, rest distributed to multiple CPU cores
- Double-precision mathematics for accurate map projections at planetary scale
- Integrates real data: SRTM/NASADEM elevation, satellite orthoimagery, OpenStreetMap vectors
- Hilbert curve variants for locality-preserving quadtree traversal

**Resolution / detail:**
- Seamless from orbital altitudes down to ground level at ~1 cm resolution
- Fractal algorithms refine and introduce details parametrized by elevation and land class
- Multi-layer texturing: satellite imagery + procedural overlays

**Performance:**
- Vertex morphing for smooth LOD transitions (no popping artifacts)
- Instanced rendering optimizations
- Optimal mesh size: ~5k triangles works well across GPU vendors
- Both NVIDIA and AMD best at 5-20k triangles per instanced call
- AMD GCN 1.1+: performance almost doubles at 5k threshold
- NVIDIA: 30% hit when culling disabled; keep mesh above 80 triangles minimum

**Notable innovations:**
- Horizontal displacement (not just height) from fractal channels -- produces overhangs and better cliffs
- Real-data + procedural seamless blend pipeline
- Fully async GPU/CPU work distribution
- CDLOD (Continuous Distance-Dependent LOD) with geomorphing

Sources:
- [Outerra Official](https://www.outerra.com/)
- [Outerra Grokipedia](https://grokipedia.com/page/outerra)
- [Outerra Procedural Grass Performance](https://outerra.blogspot.com/2016/01/procedural-rendering-performance-test-1.html)
- [GameDev.net Outerra Discussion](https://www.gamedev.net/forums/topic/643870-what-terrain-rendering-technique-does-the-outerra-engine-use/)

---

### 1.3 No Man's Sky

**Technical approach:**
- Voxel-based world generation using Signed Distance Fields (SDF)
- Mesh extraction via Dual Contouring (not Marching Cubes) from octree-structured SDF data
- World space divided into discrete 3D volumes (chunks), each storing SDF data
- Multiple layered noise functions defined in VoxelGeneratorSettings
- CPU-based mesh generation (notably not GPU)
- Engine agnostic about content source: generative and hand-authored content interchangeable

**Resolution / detail:**
- 18+ quintillion unique planets, each with unique landscapes, ecosystems, biomes
- Terrain deformable by players (voxel modification)

**Performance:**
- Continuous real-time generation while player moves, no loading screens
- GDC 2017 pipeline: voxel generation -> polygonization -> texturing -> population -> simulation
- CPU-bound mesh generation was the key bottleneck

**Notable innovations:**
- Dual Contouring produces sharper features than Marching Cubes (preserves edges)
- Seamless artist-procedural content pipeline
- Seed-based deterministic generation (same seed = same universe)
- L-systems for flora generation

Sources:
- [GDC Vault: Continuous World Generation in NMS](https://www.gdcvault.com/play/1024265/Continuous-World-Generation-in-No)
- [NMS Modding Wiki: Terrain Generation](https://nmsmodding.fandom.com/wiki/Terrain_Generation)
- [What the code says about NMS procedural generation](https://www.gamedeveloper.com/programming/what-the-code-of-i-no-man-s-sky-i-says-about-procedural-generation)

---

### 1.4 Elite Dangerous (Stellar Forge)

**Technical approach:**
- Hierarchical top-down physical simulation: galaxy -> star system -> planet -> surface features
- Not noise approximation: actual physical simulation of planetary formation from nebulous gases
- Accretion simulation determines chemical composition, mass, orbital parameters
- Tectonic simulations; crater placement reflects simulated bombardment history
- Surface: cube-based quadtree subdivision, uniformly spaced vertices for physics + rendering
- Noise functions take point position + planet ID + astronomical data as inputs
- Dual-precision floating point: native 64-bit + emulated 64-bit (two 32-bit floats) for millimeter precision across billions of millimeters
- Real stellar catalog integration (Hipparcos, Gliese) seeds the galaxy

**Resolution / detail:**
- 400 billion star systems (1:1 Milky Way)
- Landable planets: progressive quadtree subdivision based on player distance
- Above certain LOD: flat geometry + generated normal/height textures
- Below: full tessellated geometry at target resolution

**Performance:**
- 64-bit integer spatial encoding: sector coords + octree layer + system ID + body ID
- Efficient server-client synchronization via deterministic seed hierarchy
- Wang-tiling for texture variation; tri-planar blending on curved surfaces
- Material blending modulated by geological type

**Notable innovations:**
- Physics-based formation simulation (not just noise) -- crater placement from actual simulated impacts
- Top-down data availability: parent parameters always accessible when generating children
- Galaxy-scale material and age distribution functions ensure correct spiral arm structure

Sources:
- [Stellar Forge Wiki](https://elite-dangerous.fandom.com/wiki/Stellar_Forge)
- [80.lv: Generating the Universe in Elite Dangerous](https://80.lv/articles/generating-the-universe-in-elite-dangerous)
- [PC Gamer: Science Behind Elite Dangerous Planets](https://www.pcgamer.com/the-mind-bending-science-behind-the-planets-of-elite-dangerous/)

---

### 1.5 Engine Plugins

#### Unreal Engine
- **WorldScape Plugin**: 64-bit precision, noise-based biome system in C++, custom gravity, multiplayer
- **Procedural Planet Creator (UE5)**: Blueprint-based, customizable surface + atmosphere + rings
- **Free Planet Project (UE 5.3-5.4)**: Volumetric clouds + atmosphere, interactive parameters

#### Unity
- **MapMagic 2**: streaming piece-by-piece generation as player moves
- **Gaia Pro (Unity Verified)**: world building with terrain, vegetation, weather
- **Storm**: unified terrain + biome + city + dungeon + optimization pipeline
- **UnityProceduralPlanets (GitHub)**: GPU-mostly procedural planet generator, license-free

Sources:
- [WorldScape on Fab](https://www.unrealengine.com/marketplace/en-US/product/worldscape-pro-plugin-make-planet-and-infinite-world)
- [Unity Procedural Planets GPU](https://github.com/JakubNei/UnityProceduralPlanets)
- [Procedural Worlds](https://www.procedural-worlds.com/)

---

### 1.6 Academic / Research Systems

**NVIDIA GPU Gems 3, Chapter 1 (Ryan Geiss, 2008):**
- Marching Cubes on GPU with 32x32x32 voxel blocks (33x33x33 corner points)
- 9 noise octaves using 16^3 repeating 3D noise textures
- Three implementation methods on GeForce 8800:
  - Method 1: 6.6 blocks/sec (geometry shader heavy, 105 floats output per voxel)
  - Method 2: 144 blocks/sec (22x faster, moved work to vertex shader, 20 bytes per triangle)
  - Method 3: 260 blocks/sec (80% over Method 2, indexed geometry, single vertex per execution)
- 28 bytes per vertex (16B position+AO, 12B normal)
- Ambient occlusion: 32 rays per vertex, Poisson distribution, 16 short-range + 4 long-range samples per ray
- LOD: variable block world-space size (1x, 2x, 4x) all at 32^3 internal resolution
- ~300 dynamic vertex buffers for visible blocks
- Triplanar texturing with altitude-based striations

**GPU Work Graphs (DirectX 12, HPG 2024 Best Paper):**
- Shaders dynamically schedule new workloads on GPU, eliminating CPU bottleneck
- Hierarchical: World -> Chunk (8x8) -> Tile (8x8) -> DetailedTile (16x16)
- Terrain mesh: 8x8 grid per thread group = 81 vertices, 128 triangles
- LOD 0: 64 thread groups/chunk; LOD 3: 1 thread group/chunk
- Dense grass: 512 thread groups, 65,536 vertices, 49,152 triangles per detailed tile
- Sparse grass: 8 thread groups, 1,024 vertices, 512 triangles
- 79,710 instances augmented in 3.74 ms (RX 7900 XTX)
- Unique animated trees rendered in 3.13 ms
- Memory: 34.8 GB of tree geometry from 51 KB of generation code
- 1.64x faster than ExecuteIndirect
- Entire world fully procedurally generated every frame, entirely on GPU

**Jad Khoury - Procedural Planet Rendering (OpenGL):**
- FBM + Hybrid Multifractal combination for realistic terrain variation
- GPU tessellation shaders for adaptive mesh density based on camera distance
- Incremental buffer updates: two alternating framebuffers, only compute new regions
- Discretized displacement vectors to prevent interpolation smoothing
- Altitude + slope based texture selection with interpolation zones

Sources:
- [NVIDIA GPU Gems 3 Ch.1](https://developer.nvidia.com/gpugems/gpugems3/part-i-geometry/chapter-1-generating-complex-procedural-terrains-using-gpu)
- [GPU Work Graphs Paper (ACM)](https://dl.acm.org/doi/10.1145/3675376)
- [AMD GPUOpen: Work Graphs Procedural Generation](https://gpuopen.com/learn/work_graphs_mesh_nodes/work_graphs_mesh_nodes-procedural_generation/)
- [Jad Khoury: Procedural Planet Rendering](https://jadkhoury.github.io/terrain_blog.html)

---

## 2. Cross-System Comparison

| System | Noise Type | LOD System | Projection | Max Resolution | GPU/CPU | Key Innovation |
|--------|-----------|------------|------------|---------------|---------|---------------|
| SpaceEngine | Multi-octave Perlin fractal | Quadtree, screen-space | Cube-sphere | ~1 terapixel/face, mm via shader | GPU (OpenGL) | Meshless vertex-shader terrain |
| Outerra | 3-channel fractal + real data | Chunked quadtree CDLOD | Spherical mesh | ~1 cm | GPU (OpenGL 3.3+) | Horizontal displacement, real data |
| No Man's Sky | Layered noise (SDF) | Octree chunks | Voxel/SDF | Player-relative | CPU mesh gen | Dual Contouring, deformable |
| Elite Dangerous | Physics-informed noise | Quadtree patches | Cube quadtree | Millimeter precision | CPU+GPU | Physical formation simulation |
| GPU Gems 3 | 9-octave 3D Perlin | Block size scaling | Marching Cubes | 32^3 per block | GPU (DX10) | Stream output polygonization |
| GPU Work Graphs | Hierarchical procedural | Thread group LOD | Mesh nodes | Per-frame gen | GPU (DX12) | 34.8GB -> 51KB, 3.13ms trees |

---

## 3. Performance & Pipeline Analysis

### 3.1 GPU vs CPU Terrain Generation

Approximate speedup: **~10x** for GPU over CPU for fractal noise generation, normal map computation, and erosion simulation. The advantage comes from per-pixel noise evaluation being embarrassingly parallel.

Specific benchmarks:
- GPU Gems 3 Method 3: 260 blocks/sec on GeForce 8800 (2008 hardware)
- DX11 compute shader terrain: ~10 ms per generation pass on ATI HD 6870 (2012)
- GPU Work Graphs (2024): 3.13-3.74 ms for complex procedural scenes on RX 7900 XTX
- FastNoiseLite CPU (Intel 7820X, 3D): Perlin 47.93M pts/sec, Simplex 36.83M pts/sec, Value 64.13M pts/sec

### 3.2 Noise Algorithm Performance

| Algorithm | Samples/pixel (2D) | Samples/pixel (3D) | Relative Cost | Notes |
|-----------|-------------------|-------------------|--------------|-------|
| Value noise | 4 lookups | 8 lookups | 0.8x | Lowest quality, fast |
| Perlin | 4 gradient lookups | 8 gradient lookups | 1.0x (baseline) | Grid artifacts at diagonals |
| Simplex | 3 gradient lookups | 4 gradient lookups | ~0.7x in 3D | Less artifacts, better scaling |
| Worley/Voronoi | N point searches | N point searches | 2-5x | Cellular patterns |
| FBM (8 octaves) | 8 * base | 8 * base | 8x base | Standard terrain |
| Hybrid Multifractal | 8 * base + extra | 8 * base + extra | ~9x base | Best peaks/valleys |

For planet generation using 3D noise on a sphere surface, simplex offers ~30% cost reduction per octave over Perlin. Advantage grows in higher dimensions.

### 3.3 Estimated Compute Times by Resolution

Based on linear scaling of per-pixel compute, using available benchmarks:

```
Resolution    Pixels        Noise Only*   Noise+Erosion(100 iter)*   Full Pipeline*
-----------------------------------------------------------------------------------------------
1K (1024^2)   1.05M         ~0.3 ms       ~15 ms                     ~25 ms
2K (2048^2)   4.19M         ~1.0 ms       ~55 ms                     ~90 ms
4K (4096^2)   16.8M         ~4 ms         ~200 ms                    ~350 ms
8K (8192^2)   67.1M         ~16 ms        ~800 ms                    ~1.4 s
16K (16384^2) 268M          ~65 ms        ~3.2 s                     ~5.5 s
32K (32768^2) 1.07B         ~260 ms       ~13 s                      ~22 s

* Estimates for modern GPU (RTX 4080 class). 8-octave simplex noise.
  Erosion: shallow-water GPU compute shader.
  Full pipeline: noise + erosion + normal map + texturing + AO.
  Per cube face; multiply by 6 for full planet.
  Actual times vary widely by implementation quality.
```

Reference points:
- Instant Terra: interactive parameter changes at 16K x 16K resolution
- Gaea: 16x16 tile build (~16K) takes ~40 min CPU, ~10 min on 4-machine cluster; Erosion 2 up to 10x faster than v1
- GPU erosion at 2048x2048: interactive rates (Jako 2011, Mei et al. 2007)
- Compute cost scales linearly with cell count for erosion (confirmed in literature)
- World Machine: practical single-file limit ~8192x8192; tiling needed beyond
- Gaea preview: up to 4K (experimental); production builds up to 256K with Professional edition

### 3.4 GPU Erosion Simulation Specifics

**Shallow-water hydraulic erosion (GPU compute shader):**
- Grid cells store: terrain height, water height, suspended sediment, outflow flux (4 directions), velocity vector
- Per iteration: ~48 bytes read + 48 bytes write per cell
- Pipe model: virtual pipes between cells model water flow
- Cost scales linearly with cell count (confirmed)
- At 2048x2048: interactive rates (~30+ iterations/sec on modern GPU)
- At 4096x4096: still interactive for single iterations
- CPU abandoned at 1024x1024 -- too slow (multiple sources confirm)
- Single floating-point buffer approach (ignoring race conditions) is fastest; converges after extra iterations despite non-deterministic intermediates

**Thermal erosion:**
- Simpler than hydraulic; talus angle threshold drives material redistribution
- Can be merged into hydraulic pass with virtual pipe model
- Lower compute cost per iteration

### 3.5 Async Compute Pipeline Architecture

```
Frame N Timeline:
=================================================================================

Graphics Queue:    [Shadow Maps N]---[G-Buffer N]---[Lighting N]---[Post N]
                                                        |
Compute Queue 0:   [Noise Gen face A]---[Erosion face A]---[Normal+AO face A]
                         |                    |
Compute Queue 1:   [Noise Gen face B]---[Erosion face B]---[Normal+AO face B]
                                                                |
Copy Queue:        [Upload tile data]----------[Readback results]--------
```

**Key principles (from NVIDIA and AMD documentation):**

1. Overlap workloads using **different datapaths**: FP/ALU, Memory, RT Core, Tensor Core, Graphics pipe
2. Avoid overlapping workloads that read/write the **same resource** (data hazards)
3. Avoid combining high L1/L2 cache usage + high VRAM throughput workloads (cache thrashing)
4. Single compute queue usually sufficient -- AMD: "no significant benefit from more than one compute queue"
5. Manual work scheduling **outperforms** automatic overlap
6. Subchannel switches trigger Wait-For-Idle (WFI) draining all warps -- async compute fills these gaps
7. Command lists must be large enough to justify fence synchronization overhead

**Recommended overlap combinations:**
- Math-limited noise compute overlapped with shadow map rasterization (graphics-pipe dominated)
- DLSS (Tensor-heavy) with acceleration structure building (FP/ALU dependent)
- Post-processing of frame N overlapped with shadow maps for frame N+1
- Erosion simulation (memory bandwidth bound) overlaps with G-buffer fill (graphics bound)

**Measured gains:**
- Double-buffered data uploads via copy queue: ~10% of total frame time saved (AMD measured)
- General async compute overlap: 5-30% throughput gain depending on workload complementarity
- Verification: GPUView visualizes actual queue parallelism vs. serialized execution

Sources:
- [NVIDIA: Advanced API Performance Async Compute](https://developer.nvidia.com/blog/advanced-api-performance-async-compute-and-overlap/)
- [AMD GPUOpen: Concurrent Execution Async Queues](https://gpuopen.com/learn/concurrent-execution-asynchronous-queues/)

### 3.6 Progressive Refinement (Preview-to-Full-Quality)

```
Stage 0 (Instant):     Low-res noise (256^2), no erosion
                        Display: flat-shaded preview
                        Time: < 1 ms

Stage 1 (~50ms):        Medium noise (1K), 10 erosion iterations
                        Display: basic heightmap + simple texture
                        Time: ~50 ms

Stage 2 (~500ms):       High noise (4K), 100 erosion iterations
                        Display: eroded terrain + biome colors
                        Time: ~350 ms

Stage 3 (~5s):          Full noise (16K), 500 erosion iterations
                        + normal maps + AO
                        Display: production quality
                        Time: ~5.5 s

Stage 4 (background):   Ultra (32K), 1000+ erosion iterations
                        + detail textures + PBR materials
                        Display: final export quality
                        Time: ~22 s per face
```

**Implementation approach (Khoury technique):**
- Maintain two alternating framebuffers
- Track which texture regions are new vs. reusable
- Discretize displacement vectors to prevent interpolation smoothing
- Each stage reads from previous stage's output, computes only delta
- Progressive mesh uses 4-8 refinement scheme (twice as gradual as quadtree transitions)
- ROAM (Real-time Optimally Adapting Mesh) allows fine-tuning priority thresholds, frame rate limits, or triangle count caps

### 3.7 Multi-GPU Strategies

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

### 3.8 Memory Bandwidth Analysis

**Per-operation bandwidth consumption at 16K resolution (268M pixels per face):**

```
Operation              Bytes/pixel    Total @ 16K      Notes
---------------------------------------------------------------------------
8-octave noise read    8 * 16B = 128B  34.4 GB         16B per 3D texture fetch
Erosion (per iter)     ~48B R + 48B W  25.8 GB         height+water+flux+velocity+sediment
Normal map gen         12B R + 8B W    5.4 GB           3x3 neighborhood read
AO computation         ~256B R         68.7 GB          32 rays * 8 samples each
Texturing              ~64B R + 16B W  21.5 GB          multi-layer material lookup
---------------------------------------------------------------------------
Total per face (noise + 1 erosion iter):  ~155 GB bandwidth consumed
```

**Modern GPU bandwidth context:**
- RTX 4090: 1,008 GB/s -> single face noise+1 erosion iter: ~154 ms bandwidth-limited floor
- RX 7900 XTX: 960 GB/s -> similar
- RTX 5090: ~1,792 GB/s -> ~86 ms
- Note: actual times higher due to compute ALU time, cache misses, synchronization

**Mitigation strategies:**
1. **GPU Work Graphs**: eliminate CPU-GPU round trips entirely (34.8 GB -> 51 KB demonstrated)
2. **Texture compression** (S3TC/BC7): 4x memory reduction, hardware decompressed at no ALU cost
3. **Kernel fusion**: combine noise + erosion + texturing into single dispatch to maximize L1 cache hits
4. **Tile-based compute**: process 64x64 tiles to fit in shared memory (typical 128KB L1/shared)
5. **Mesh shaders**: lower memory footprint, less VRAM access, frees bandwidth for compute
6. **Static allocation** (SpaceEngine approach): eliminates allocation overhead completely
7. **Incremental updates** (Khoury approach): only recompute changed regions

---

## 4. Key Takeaways for Planet Generator Pipeline

1. **Cube-sphere + quadtree** is the industry standard projection (SpaceEngine, Elite Dangerous, Outerra, academic work)
2. **GPU noise generation** is ~10x faster than CPU; fuse octaves into single dispatch for cache efficiency
3. **256x256 tile nodes** (SpaceEngine) or **32x32 voxel blocks** (GPU Gems) are practical GPU work units
4. **GPU Work Graphs** (DX12, 2024) represent the state of the art: 34.8 GB -> 51 KB, sub-4ms generation, 1.64x faster than ExecuteIndirect
5. **Async compute** provides 5-30% throughput gain by overlapping ALU-heavy noise with graphics-heavy rendering
6. **Progressive refinement** enables interactive editing: 256^2 preview in <1ms, 16K production in ~5.5s per face
7. **Split-by-face** multi-GPU is most practical: near-linear scaling, minimal inter-GPU communication
8. **Memory bandwidth** is the primary bottleneck at high resolutions; tile-based compute and kernel fusion critical
9. **Erosion** dominates compute time at high iteration counts; GPU shallow-water model scales linearly with cells; single float buffer with race conditions is fastest converging approach
10. **Dual precision** (Elite Dangerous approach) is necessary for planetary-scale coordinate accuracy
11. **Simplex noise** preferred over Perlin for 3D planet generation: ~30% cost reduction per octave, fewer artifacts
12. **Hybrid Multifractal** (Musgrave) combined with FBM produces the most realistic terrain variation
