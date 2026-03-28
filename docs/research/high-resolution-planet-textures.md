# High-Resolution Texture Generation and LOD Systems for Procedural Planets

*Research date: 2026-03-28 (updated) | 32 sources*

---

## Table of Contents

1. [Techniques for Generating High-Resolution Textures](#1-techniques-for-generating-high-resolution-textures)
2. [Tiled / Streaming Texture Generation](#2-tiled--streaming-texture-generation)
3. [Virtual Texturing and Texture Streaming from Disk](#3-virtual-texturing-and-texture-streaming-from-disk)
4. [Mipmap Generation for Procedural Textures](#4-mipmap-generation-for-procedural-textures)
5. [Compression Formats: BC7, ASTC](#5-compression-formats-bc7-astc)
6. [Memory Budgets for 32K Textures](#6-memory-budgets-for-32k-textures)
7. [Sparse Virtual Texture Pipeline Details](#7-sparse-virtual-texture-pipeline-details)
8. [CDLOD (Continuous Distance-Dependent LOD)](#8-cdlod-continuous-distance-dependent-lod)
9. [Chunked LOD](#9-chunked-lod)
10. [Geometry Clipmaps for Spherical Terrain](#10-geometry-clipmaps-for-spherical-terrain)
11. [GPU Tessellation-Based LOD](#11-gpu-tessellation-based-lod)
12. [Mesh Shader LOD for Planets](#12-mesh-shader-lod-for-planets)
13. [Cube-to-Sphere Projections](#13-cube-to-sphere-projections)
14. [Crack-Free Rendering Techniques](#14-crack-free-rendering-techniques)
15. [LOD System Comparison and Recommendations](#15-lod-system-comparison-and-recommendations)

---

## 1. Techniques for Generating High-Resolution Textures

### 1.1 GPU-Based Noise Generation

The dominant approach for procedural planet textures is **GPU fragment/compute shader noise evaluation**. Rather than pre-computing textures on the CPU, noise functions (Perlin, Simplex, fractional Brownian motion) run directly on the GPU.

**Implementation:**
- Sample 3D noise at the surface position of each texel. For a sphere, convert each texel's UV to a 3D Cartesian point on the unit sphere, then evaluate noise at that point. This produces seamless results without polar distortion [1].
- Use **Fractional Brownian Motion (FBM)** or **Hybrid Multifractal** algorithms with 8-16 octaves for terrain detail. Parameters: `lacunarity` (frequency multiplier per octave, typically 2.0), `persistence` (amplitude decay, typically 0.5) [2].
- Store heightmap output in **GL_R32F framebuffers** for full floating-point precision [3].
- GPU generation is "almost instantaneous, even with high resolutions" compared to CPU approaches [2].

**Cubemap-Based Generation:**
Instead of equirectangular projection (which causes severe polar distortion), generate six cubemap faces and inflate the cube to a sphere. For each face, march through UV coordinates, project onto the sphere surface, and sample 3D noise at those positions [4][5].

**Incremental/Partial Updates:**
Use **double-buffered framebuffers** where only newly-exposed terrain regions are recomputed when the camera moves. The shader reads previously-rendered values from an alternate buffer, reducing per-frame computation dramatically [3].

**Output Maps Generated Per Planet:**
- Diffuse/Albedo (RGBA8 or sRGBA8)
- Elevation/Height (R32F or R16F)
- Normal (RG16 or RGB8, derived from height)
- Specular/Roughness (R8)
- Cloud layer (optional, RGBA8)

### 1.2 Maximum Texture Dimensions

| API | Guaranteed Minimum Max 2D Size | Typical Modern GPU Max |
|-----|-------------------------------|----------------------|
| Direct3D 11 | 16,384 x 16,384 | 16,384 |
| Direct3D 12 | 16,384 x 16,384 | 16,384 |
| Vulkan | 4,096 (minimum required) | 16,384 - 32,768 |
| OpenGL 4.x | 16,384 | 16,384 - 32,768 |

To exceed 16K or 32K, you **must** use tiled/virtual texturing; no single GPU texture allocation can hold a 32K x 32K RGBA8 map in one resource on most hardware [6][7].

### References
- [1] [Procedural Planetary Surfaces - Toni Sagrista](https://tonisagrista.com/blog/2021/procedural-planetary-surfaces/)
- [2] [Procedural Planetary Surfaces (GPU update note)](https://tonisagrista.com/blog/2021/procedural-planetary-surfaces/)
- [3] [Procedural Planet Rendering - Jad Khoury](https://jadkhoury.github.io/terrain_blog.html)
- [4] [Mapping Planets - In Media Res](https://metriximor.wordpress.com/2019/01/19/mapping-planets/)
- [5] [Planet Generation Part II - Shane Enishry](https://www.shaneenishry.com/blog/2014/08/02/planet-generation-part-ii/)
- [6] [GPU Texture Max Dimensions - gpuweb issue](https://github.com/gpuweb/gpuweb/issues/1327)
- [7] [Vulkan Limits Documentation](https://docs.vulkan.org/spec/latest/chapters/limits.html)

---

## 2. Tiled / Streaming Texture Generation

### 2.1 Why Tile-Based Generation Is Required

A single 32K x 32K RGBA8 texture requires **4 GB uncompressed**. This exceeds VRAM on most consumer GPUs and exceeds the maximum texture dimension on all current APIs. The solution is to subdivide generation into tiles.

### 2.2 Tile Generation Architecture

**Approach:** Divide each cubemap face (or equirectangular map) into an NxN grid of tiles. Generate each tile independently in a compute/fragment shader, writing to a tile-sized render target.

**Typical Tile Sizes:**
| Tile Size | Tiles for 32K face | Memory per RGBA8 tile |
|-----------|-------------------|----------------------|
| 256 x 256 | 128 x 128 = 16,384 | 256 KB |
| 512 x 512 | 64 x 64 = 4,096 | 1 MB |
| 1024 x 1024 | 32 x 32 = 1,024 | 4 MB |
| 2048 x 2048 | 16 x 16 = 256 | 16 MB |

**Implementation Details:**
1. **Per-tile coordinate mapping:** Each tile shader invocation receives tile coordinates (tx, ty) and computes the world-space position for each texel by offsetting into the global UV space.
2. **Overlap borders:** Include 1-4 texel borders for seamless filtering across tile boundaries. A 256px tile with 2px border becomes 260px during generation, trimmed on write-back.
3. **Generation pipeline:** Generate tiles in priority order (closest to camera first), typically 4-16 tiles per frame to maintain interactive frame rates.
4. **Write to staging buffer:** Each completed tile is written to a CPU-readable staging buffer, then compressed and/or stored to disk.

**GPU Memory During Generation:**
Only 1-4 tiles need to be resident simultaneously. For 1024x1024 tiles in R32F (height), that is just 4 MB per tile in flight, compared to 4 GB for the full map [8][9].

### 2.3 Sparse / Tiled Resources (Hardware Support)

Modern APIs provide hardware-accelerated partial residency:

- **DirectX 12:** `CreateReservedResource()` + `UpdateTileMappings()` on ID3D12CommandQueue. Tiles are 64 KB each. Tier 2+ supports partial mip residency [10].
- **Vulkan:** Sparse binding / sparse residency via `VK_IMAGE_CREATE_SPARSE_BINDING_BIT`. Memory pages are typically 64 KB [10].
- **Unity:** `SparseTexture` class wraps the above [11].

Sparse textures let you allocate a 32K x 32K virtual texture but only back visible tiles with physical memory, keeping VRAM usage proportional to screen coverage.

### References
- [8] [Tile-Based Texture Mapping - GPU Gems 2, Ch.12](https://developer.nvidia.com/gpugems/gpugems2/part-ii-shading-lighting-and-shadows/chapter-12-tile-based-texture-mapping)
- [9] [Why Tiled Resources Are Needed - Microsoft](https://learn.microsoft.com/en-us/windows/win32/direct3d11/why-are-tiled-resources-needed-)
- [10] [Sparse Resources Investigation - gpuweb](https://github.com/gpuweb/gpuweb/issues/455)
- [11] [Sparse Textures - Unity Manual](https://docs.unity3d.com/Manual/SparseTextures.html)

---

## 3. Virtual Texturing and Texture Streaming from Disk

### 3.1 MegaTexture (id Tech 5/6)

The pioneering virtual texturing system, developed by id Software for *RAGE* (2011).

**Architecture:**
- A single unique texture covering the entire game world, up to **128,000 x 128,000 pixels** [12].
- Stored on disk as ~20 GB of compressed texture data.
- Subdivided into small **pages** (typically 128x128 or 256x256 pixels).
- At runtime, a feedback pass determines which pages are visible at which mip level.
- Required pages are loaded from disk, transcoded on background CPU threads, and uploaded to a GPU-side **page cache** (texture atlas).
- A **page table** (indirection texture) maps virtual UV coordinates to physical atlas locations.

**Why it was abandoned (id Tech 7):** High disk space requirements, texture pop-in artifacts during fast camera movement, complexity of the transcoding pipeline, and the rise of better alternatives (sparse resources, sampler feedback) [12].

### 3.2 Unreal Engine Virtual Texturing

UE provides two complementary systems:

**Streaming Virtual Texturing (SVT):**
- Replaces traditional texture streaming with page-based streaming from disk.
- Reduces peak VRAM usage for large texture sets.
- Page sizes are configurable (default 128x128 or 256x256 with BC compression).

**Runtime Virtual Texturing (RVT):**
- Generates texture pages on the GPU at runtime (ideal for procedural content).
- A feedback buffer identifies which virtual pages are needed.
- Pages are rendered into a physical page cache.
- Base Color stored as RGB compressed to BC1; YCoCg variant adds 25% memory for higher quality.
- Well-suited for landscape/terrain shading with decal-like materials [13].

### 3.3 Modern Virtual Texturing Pipeline (PlayerUnknown Productions)

A state-of-the-art implementation achieving **70-80% less texture memory on the GPU** [14]:

**Pipeline stages per frame:**
1. **Feedback write:** During rendering, the shader writes tile requests to a feedback buffer (stochastically, 1 pixel per 4x4 block to reduce overhead).
2. **Feedback readback:** Double/triple-buffered GPU-to-CPU readback to avoid stalls.
3. **CPU processing:** Assemble load requests, mark residency changes.
4. **Async upload:** Load tile data from disk via background threads, upload to GPU via transfer queue.

**Hardware features leveraged:**
- **DX12 Reserved Resources:** Format-agnostic partial residency in a shared heap.
- **Sampler Feedback (DX12):** Hardware captures which tiles were actually sampled, replacing analytical approximations. Supported on AMD RDNA 2/3, NVIDIA Ampere+, Intel Alchemist+.
- **DirectStorage:** Bypasses CPU for disk-to-GPU transfers [14].

### 3.4 Async Texture Upload Pipeline

**Staging Buffer Design:**
- Allocate a fixed staging buffer (e.g., 64 MB) in `DEVICE_LOCAL | HOST_VISIBLE` memory.
- Use a range allocator to manage sub-allocations within this pool.
- When staging memory fills, submit the current batch and reuse the buffer [15].

**Transfer Queues:**
- Use dedicated async transfer/copy queues (Vulkan `VK_QUEUE_TRANSFER_BIT`, DX12 copy queue) to avoid blocking the graphics queue.
- Respect hardware alignment: `optimalBufferCopyRowPitchAlignment`, `minImageTransferGranularity`.
- Fence/semaphore tracking ensures staging memory is freed only after upload completes [15].

**Bandwidth considerations:**
- PCIe 4.0 x16: ~25 GB/s theoretical, ~12-15 GB/s practical for texture uploads.
- A single 32K x 32K RGBA8 (4 GB) would take ~0.27-0.33s to stream at full bandwidth.
- In practice, only visible tiles stream per frame: at 60fps with 256x256 BC7 tiles, streaming 16 tiles/frame = 16 x 32KB = 512 KB/frame = ~30 MB/s.

### References
- [12] [MegaTexture - Wikipedia](https://en.wikipedia.org/wiki/MegaTexture)
- [13] [Virtual Texturing in UE - Epic Games](https://dev.epicgames.com/documentation/en-us/unreal-engine/virtual-texturing-in-unreal-engine)
- [14] [Virtual Texturing - PlayerUnknown Productions](https://playerunknownproductions.net/news/virtual-texturing)
- [15] [Uploading Textures to GPU - The Good Way](https://erfan-ahmadi.github.io/blog/Nabla/imageupload)

---

## 4. Mipmap Generation for Procedural Textures

### 4.1 Online (Runtime) Mipmap Generation

For procedurally generated textures that change at runtime, mipmaps must be computed on the fly.

**Compute Shader Approach (Recommended):**
- The `nvpro_pyramid` library demonstrates a cache-aware compute shader that generates all mip levels in fewer dispatches than the naive blit approach [16].
- **Fast pipeline:** Generates multiple levels per dispatch using register shuffles within GPU subgroups (warps). Threads share intermediate results via registers rather than main memory.
- **General pipeline:** For non-power-of-2 textures, uses shared memory communication with 3x3 kernels for odd dimensions.

**Performance (RTX 3090, sRGBA8):**

| Resolution | Compute Shader | Blit Method | Speedup |
|-----------|---------------|-------------|---------|
| 4096 x 4096 | 113 us | 161 us | 1.43x |
| 2048 x 2048 | 36 us | 63 us | 1.75x |
| 3840 x 2160 | 76 us | 95 us | 1.25x |

**Key advantages over blit:**
- Works on compute-only queues (can overlap with graphics work).
- Eliminates per-level synchronization barriers.
- Custom reduction kernels (not limited to bilinear) [16].

**Per-Tile Mipmap Generation:**
When generating textures in tiles, each tile must generate its own mip chain. The border overlap region (Section 2.2) ensures correct filtering at tile edges. For a 256x256 tile: mip levels 0-8 (256, 128, 64, 32, 16, 8, 4, 2, 1), adding 33% memory overhead.

### 4.2 Offline (Precomputed) Mipmap Generation

For textures that are generated once and cached to disk:
- Generate the full mip chain during the offline bake step.
- Store all mip levels in the texture file (KTX2, DDS format).
- At load time, stream individual mip levels independently (lower mips load first for fast preview).

**Mipmap Storage Overhead:**
The complete mip chain adds exactly **1/3 (33.33%)** of the base level size:
- Geometric series: 1 + 1/4 + 1/16 + 1/64 + ... = 4/3 of base level.
- For a 32K x 32K RGBA8 base (4 GB), total with mipmaps = **5.33 GB**.

### 4.3 Procedural Mip Levels (Noise Anti-Aliasing)

An alternative to downsampling: generate each mip level directly with band-limited noise.

- At mip level N, reduce the number of noise octaves by N (since octaves above the Nyquist frequency for that resolution would alias).
- A 32K base with 12 octaves of FBM: mip 0 = 12 octaves, mip 1 = 11 octaves, ..., mip 11 = 1 octave.
- Produces theoretically correct results (no aliasing) but requires re-evaluating noise per mip level.
- Practical for runtime virtual texturing where each tile is generated at a specific mip level.

### References
- [16] [vk_compute_mipmaps - NVIDIA](https://github.com/nvpro-samples/vk_compute_mipmaps)

---

## 5. Compression Formats: BC7, ASTC

### 5.1 Format Overview

| Format | Bits/Pixel | Block | Channels | Platform | Best For |
|--------|-----------|-------|----------|----------|----------|
| **BC1** | 4 bpp | 4x4 → 8B | RGB + 1-bit A | PC/Console | Low-quality albedo, cutouts |
| **BC4** | 4 bpp | 4x4 → 8B | R (8 values) | PC/Console | Height maps, roughness (single channel) |
| **BC5** | 8 bpp | 4x4 → 16B | RG (2x BC4) | PC/Console | Tangent-space normal maps (RG) |
| **BC6H** | 8 bpp | 4x4 → 16B | RGB HDR (float16) | PC/Console | HDR environment maps |
| **BC7** | 8 bpp | 4x4 → 16B | RGBA (8-bit) | PC/Console | High-quality albedo RGBA |
| **ASTC 4x4** | 8 bpp | 4x4 → 16B | 1-4 channels | Mobile/Some PC | Universal high-quality |
| **ASTC 6x6** | 3.56 bpp | 6x6 → 16B | 1-4 channels | Mobile | Balanced quality/size |
| **ASTC 8x8** | 2 bpp | 8x8 → 16B | 1-4 channels | Mobile | Aggressive compression |

All BCn and ASTC formats decompress in hardware at full texture fetch rate -- compressed data stays compressed in VRAM [17][18].

### 5.2 Quality Characteristics

**BC7 vs ASTC 4x4:** Nearly identical quality at 8 bpp. Both exceed 42 dB PSNR on standard test images. BC7 has a "massive difference" in quality over BC1, with "no perceptible difference from uncompressed originals at normal viewing distances" [17].

**ASTC 6x6 vs BC1:** ASTC at 3.56 bpp surpasses BC1 at 4 bpp by ~1.5 dB despite using 10% fewer bits [18].

**Per-Map-Type Recommendations:**

| Map Type | Recommended Format | Rationale |
|----------|-------------------|-----------|
| Albedo (RGBA) | BC7 / ASTC 4x4 | Highest quality for color, handles alpha |
| Height (R) | BC4 / ASTC 4x4 (1ch) | 8 gradient values per block; excellent for smooth data |
| Normal (RG) | BC5 / ASTC 4x4 (2ch) | Two independent channels, reconstruct Z in shader |
| Roughness (R) | BC4 / ASTC 4x4 (1ch) | Single channel, 4 bpp sufficient |

### 5.3 GPU Real-Time Compression

For procedural textures generated at runtime, compression must happen on the GPU.

**Tellusim SDK Benchmarks (Apple M1 Max, 1024x512 image):**

| Format | GPU Time | CPU Fast Time | GPU PSNR | CPU PSNR |
|--------|----------|--------------|----------|----------|
| BC1 | 0.4 ms | 28 ms | ~39 dB | 39.83 dB |
| BC7 | 1.0 ms | 105 ms | 44.85 dB | 48.27 dB |
| ASTC 4x4 | 2.2 ms | 100 ms | 44.97 dB | 48.13 dB |
| ASTC 5x5 | 3.1 ms | 138 ms | 40.96 dB | 44.48 dB |

GPU encoding is **28-105x faster** than CPU but with a 3-4 dB quality penalty [19].

**Betsy (Godot GPU Compressor):**
- Implements BC1, 3, 4, 5, 6 and ETC1/2 via GLSL compute shaders.
- ETC2 RGB: 3.16s vs 39.29s for CPU (etc2comp) on Kodim test set = **12x speedup** [20].

**AMD Compressonator 4.2:**
- BC7 GPU encoding via DirectX Compute and OpenCL.
- Max quality mode: 38% faster than v4.0, +0.6 dB improvement [21].

**Practical Pipeline for Procedural Planets:**
1. Generate tile in compute shader (e.g., 256x256 RGBA8).
2. Compress tile via GPU BC7 compute shader (~0.25 ms for 256x256).
3. Copy compressed tile (32 KB) to staging buffer.
4. Stream to disk cache for future loads.

### References
- [17] [Understanding BCn Texture Compression - Nathan Reed](https://www.reedbeta.com/blog/understanding-bcn-texture-compression-formats/)
- [18] [ASTC Format Overview - ARM](https://github.com/ARM-software/astc-encoder/blob/main/Docs/FormatOverview.md)
- [19] [GPU Texture Encoder - Tellusim](https://tellusim.com/gpu-encoder/)
- [20] [Betsy GPU Texture Compressor - Godot Engine](https://godotengine.org/article/betsy-gpu-texture-compressor/)
- [21] [Compressonator 4.2 - AMD GPUOpen](https://gpuopen.com/learn/compressonator-4-2/)

---

## 6. Memory Budgets for 32K Textures

### 6.1 Uncompressed Memory Requirements

**Formula:** `width x height x bytes_per_pixel`

For a single 32,768 x 32,768 texture:

| Map Type | Format | Bytes/Pixel | Base Size | With Mipmaps (+33%) |
|----------|--------|------------|-----------|-------------------|
| Albedo | RGBA8 | 4 | **4,096 MB** | 5,461 MB |
| Height | R32F | 4 | **4,096 MB** | 5,461 MB |
| Roughness | R8 | 1 | **1,024 MB** | 1,365 MB |
| Normal | RG16 | 4 | **4,096 MB** | 5,461 MB |
| **Total** | | **13 B/texel** | **13,312 MB** | **17,749 MB** |

**Full planet (6 cubemap faces):**

| | Base | With Mipmaps |
|--|------|-------------|
| Single face | 13,312 MB | 17,749 MB |
| **6 faces** | **79,872 MB (78 GB)** | **106,496 MB (104 GB)** |

This is clearly infeasible without virtual texturing or compression.

### 6.2 Compressed Memory Requirements

Using BC formats (PC) at 32,768 x 32,768:

| Map Type | Uncompressed Format | Compressed Format | Compressed bpp | Base Size | With Mipmaps |
|----------|--------------------|--------------------|---------------|-----------|-------------|
| Albedo | RGBA8 (4 B/px) | **BC7** | 8 bpp (1 B/px) | **1,024 MB** | 1,365 MB |
| Height | R32F (4 B/px) | **BC4** (from R16 quantized) | 4 bpp (0.5 B/px) | **512 MB** | 683 MB |
| Roughness | R8 (1 B/px) | **BC4** | 4 bpp (0.5 B/px) | **512 MB** | 683 MB |
| Normal | RG16 (4 B/px) | **BC5** | 8 bpp (1 B/px) | **1,024 MB** | 1,365 MB |
| **Total** | 13 B/texel | | 2.5 B/texel | **3,072 MB** | **4,096 MB** |

**Full planet (6 cubemap faces), compressed:**

| | Base | With Mipmaps |
|--|------|-------------|
| Single face | 3,072 MB | 4,096 MB |
| **6 faces** | **18,432 MB (18 GB)** | **24,576 MB (24 GB)** |

**Compression ratio:** 4.3:1 over uncompressed (13 B/texel down to ~3 B/texel).

> **Note on Height Maps:** BC4 compresses R8/R16 single-channel data well (8 gradient values per 4x4 block), but R32F heightmaps should be quantized to R16 first. If full R32F precision is required, store height uncompressed (4 GB per face) or use a custom encoding.

### 6.3 ASTC Compressed Memory (Mobile/Cross-Platform)

| Map Type | ASTC Block | bpp | 32K x 32K Size | With Mipmaps |
|----------|-----------|-----|----------------|-------------|
| Albedo RGBA | 4x4 | 8 | 1,024 MB | 1,365 MB |
| Albedo RGBA | 6x6 | 3.56 | 456 MB | 608 MB |
| Albedo RGBA | 8x8 | 2 | 256 MB | 341 MB |
| Height R | 4x4 (1ch) | 8 | 1,024 MB | 1,365 MB |
| Normal RG | 4x4 (2ch) | 8 | 1,024 MB | 1,365 MB |

ASTC 6x6 provides an interesting middle ground: better quality than BC1 at lower bpp, useful for aggressive budgets.

### 6.4 Practical VRAM Budgets with Virtual Texturing

With virtual texturing, only visible tiles reside in VRAM. Practical budgets:

**Assumptions:**
- Tile size: 256 x 256 pixels
- Visible area: ~10% of one cubemap face at any time
- 4 map types (albedo BC7 + height BC4 + roughness BC4 + normal BC5)

**Per-tile memory (256x256, BC compressed):**

| Map | Format | Tile Size |
|-----|--------|-----------|
| Albedo | BC7 (1 B/px) | 64 KB |
| Height | BC4 (0.5 B/px) | 32 KB |
| Roughness | BC4 (0.5 B/px) | 32 KB |
| Normal | BC5 (1 B/px) | 64 KB |
| **Total per tile** | | **192 KB** |

**VRAM for visible tiles (10% of one 32K face):**
- Total tiles per face: 128 x 128 = 16,384
- 10% visible: ~1,638 tiles
- 1,638 tiles x 192 KB = **315 MB** (plus ~5 mip levels at ~80 MB) = **~400 MB**

**Page table overhead:**
- Indirection texture for 32K face at 256px tiles: 128x128 entries x 4 bytes = 64 KB per map.
- Total for 4 maps x 6 faces = 1.5 MB (negligible).

**Recommended VRAM Budget by GPU Class:**

| GPU VRAM | Tile Cache Size | Approximate Visible Coverage | Quality |
|----------|----------------|------------------------------|---------|
| 4 GB | 256 MB | ~1,300 tiles (8% of face) | Low: visible popping |
| 8 GB | 512 MB | ~2,600 tiles (16% of face) | Medium: smooth for single planet |
| 12 GB | 1 GB | ~5,200 tiles (32% of face) | High: comfortable margin |
| 16+ GB | 2 GB | ~10,400 tiles (64% of face) | Ultra: minimal streaming |

### 6.5 Disk Storage Budget

Full planet at 32K per cubemap face, BC compressed, with mipmaps:

| Storage | Size |
|---------|------|
| 6 faces x 4 maps, BC compressed + mipmaps | **24.6 GB** |
| With LZ4/zstd on-disk compression (~2:1 on BC data) | **~12 GB** |
| Single face, all maps | **~4.1 GB** |

For multiple planets, disk becomes the primary constraint. Consider:
- Regenerating from seed (zero disk, GPU cost at runtime).
- Caching only recently-viewed tiles (LRU eviction).
- Hybrid: store low-res mips on disk, generate high-res tiles on demand.

---

## Summary: Recommended Architecture for 32K+ Planet Textures

```
                     +-----------------------+
                     |   Noise Parameters    |
                     |   (seed, octaves,     |
                     |    lacunarity, etc.)  |
                     +-----------+-----------+
                                 |
                    GPU Compute Shader
                    (per-tile, 256x256)
                                 |
                     +-----------v-----------+
                     | Tile Render Targets   |
                     | Albedo RGBA8          |
                     | Height R32F           |
                     | Normal RG16           |
                     | Roughness R8          |
                     +-----------+-----------+
                                 |
                    GPU BC7/BC4/BC5 Compress
                    (compute shader, ~1ms/tile)
                                 |
              +------------------+------------------+
              |                                     |
     +--------v--------+              +-------------v---------+
     | Virtual Texture  |              | Disk Cache            |
     | Page Cache (VRAM)|              | (KTX2/DDS + LZ4)     |
     | 256-2048 MB      |              | LRU eviction          |
     +---------+--------+              +-----------+-----------+
               |                                   |
               +-------> Render <------ Async Load |
                     (indirection +                 |
                      page table)                   |
```

**Key design decisions:**
1. **Generate in tiles** (256x256 or 512x512) to fit GPU memory.
2. **Compress immediately** via GPU compute (BC7/BC4/BC5).
3. **Virtual texturing** with sparse resources for VRAM management.
4. **Feedback-driven streaming** with sampler feedback (DX12) or manual feedback buffer.
5. **Async disk cache** with dedicated transfer queue uploads.
6. **Procedural mip levels** (reduce noise octaves per level) for theoretically correct anti-aliasing.

---

---

## 7. Sparse Virtual Texture Pipeline Details

### 7.1 Full Pipeline Architecture

The virtual texturing system operates as a closed-loop with three separated concerns [17][18]:

1. **Addressing** (GPU): Maps virtual UV coordinates to physical texture atlas locations
2. **Feedback** (GPU): Records which pages were accessed and at what resolution
3. **Residency** (CPU): Decides which pages must occupy GPU memory

### 7.2 Page Table Implementation

The page table is a 2D texture where each texel corresponds to one virtual page. A common encoding packs metadata into a 32-bit integer [17]:

- **Bit 0:** Residency flag (page present in GPU memory)
- **Bits 1-8:** Physical page X coordinate in atlas
- **Bits 9-16:** Physical page Y coordinate in atlas
- **Remaining bits:** LOD hints, eviction state, debugging flags

**Page table sizes for different virtual texture resolutions (128x128 page size):**

| Virtual Resolution | Pages Per Axis | Page Table Size | Page Table Memory |
|-------------------|---------------|-----------------|-------------------|
| 8K x 8K | 64 | 64 x 64 | 16 KB |
| 16K x 16K | 128 | 128 x 128 | 64 KB |
| 32K x 32K | 256 | 256 x 256 | 256 KB |
| 64K x 64K | 512 | 512 x 512 | 1 MB |
| 128K x 128K | 1024 | 1024 x 1024 | 4 MB |

### 7.3 Mip Level Selection Shader

The shader computes required mip level using screen-space derivatives [17]:

```glsl
// Virtual texture mip level calculation
float computeVirtualMip(vec2 uv, vec2 virtualSize) {
    vec2 dx = dFdx(uv) * virtualSize;
    vec2 dy = dFdy(uv) * virtualSize;
    float d = max(dot(dx, dx), dot(dy, dy));
    return floor(0.5 * log2(d));
}
```

### 7.4 Virtual-to-Physical Address Translation

```glsl
// Pseudocode for virtual texture sampling
vec4 sampleVirtualTexture(vec2 virtualUV, sampler2D pageTable, sampler2D physicalAtlas) {
    float mip = computeVirtualMip(virtualUV, virtualTextureSize);

    // Scale UV to page grid at this mip level
    vec2 pageGridSize = max(virtualTextureSize / (pageSize * exp2(mip)), 1.0);
    vec2 pageIndex = floor(virtualUV * pageGridSize);
    vec2 inPageOffset = fract(virtualUV * pageGridSize);

    // Lookup page table for physical location
    vec4 pageEntry = texelFetch(pageTable, ivec2(pageIndex), int(mip));

    // Check residency, fall back to coarser mip if not resident
    while (pageEntry.a < 0.5 && mip < maxMip) {
        mip += 1.0;
        pageGridSize = max(virtualTextureSize / (pageSize * exp2(mip)), 1.0);
        pageIndex = floor(virtualUV * pageGridSize);
        inPageOffset = fract(virtualUV * pageGridSize);
        pageEntry = texelFetch(pageTable, ivec2(pageIndex), int(mip));
    }

    // Compute physical atlas UV
    vec2 physicalUV = (pageEntry.rg * 255.0 * pageSize + inPageOffset * pageSize) / atlasSize;
    return texture(physicalAtlas, physicalUV);
}
```

### 7.5 Feedback Buffer Rendering

A separate reduced-resolution pass generates a compact summary of page requests [17][18]:

```glsl
// Feedback pass fragment shader - outputs page requests instead of color
void feedbackPass(vec2 virtualUV) {
    float mip = computeVirtualMip(virtualUV, virtualTextureSize);
    vec2 pageGridSize = max(virtualTextureSize / (pageSize * exp2(mip)), 1.0);
    vec2 pageIndex = floor(virtualUV * pageGridSize);

    // Encode: page X, page Y, mip level, valid flag
    gl_FragColor = vec4(pageIndex.x / 255.0, pageIndex.y / 255.0, mip / 15.0, 1.0);
}
```

**Feedback buffer sizing:**
- Render at 1/8 to 1/16 of screen resolution (e.g., 240x135 for 1080p)
- Stochastic jitter across frames catches pages missed by low-resolution sampling
- PlayerUnknown Productions uses 4x4 pixel block sampling with frame rotation [7]
- Double/triple-buffer GPU-to-CPU readback to avoid pipeline stalls

### 7.6 Physical Atlas (Page Cache) Management

The physical atlas is a fixed-size texture storing actual page data [17][18]:

**LRU eviction strategy:**
1. Each page tracks last-used frame number
2. When atlas is full, evict least-recently-used page
3. Lowest mip pages are pinned (never evicted) to prevent sampling holes
4. 4 high-res pages can be replaced by 1 coarser page (75% savings) for memory pressure

**Practical atlas sizes:**

| Atlas Resolution | Pages (128x128) | Memory (BC7) | Memory (RGBA8) |
|-----------------|-----------------|--------------|----------------|
| 2048 x 2048 | 16 x 16 = 256 | 4 MB | 16 MB |
| 4096 x 4096 | 32 x 32 = 1024 | 16 MB | 64 MB |
| 8192 x 8192 | 64 x 64 = 4096 | 64 MB | 256 MB |
| 16384 x 16384 | 128 x 128 = 16384 | 256 MB | 1024 MB |

### 7.7 Hardware Virtual Texturing vs Software

**Hardware (DX12 Reserved Resources, Vulkan Sparse):**
- Page table translation handled in silicon
- Tile size fixed at 64 KB (DX12 standard)
- AMD RDNA 2/3, NVIDIA Ampere+: good performance
- AMD RDNA 1 and earlier: performance issues with reserved resources
- A 2025 paper ("The Sad State of Hardware Virtual Textures") documents that sparse texture binding can be painfully slow on some drivers, causing frame rate stuttering [19]

**Software (preferred by most modern engines):**
- Explicit control over residency, feedback, eviction
- Cross-platform determinism
- No driver-specific performance cliffs
- Slightly higher shader overhead for indirection

### References (Section 7)
- [17] [How Virtual Textures Really Work - shlom.dev](https://www.shlom.dev/articles/how-virtual-textures-really-work/)
- [18] [Sparse Virtual Textures - Studio Pixl](https://studiopixl.com/2022-04-27/sparse-virtual-textures)
- [19] [The Sad State of Hardware Virtual Textures - HAL Science 2025](https://hal.science/hal-05138369/file/The_Sad_State_of_Hardware_Virtual_Textures.pdf)

---

## 8. CDLOD (Continuous Distance-Dependent LOD)

### 8.1 Overview

CDLOD was developed by Filip Strugar and is structured around a **quadtree of regular grids** where different levels represent different LODs. Its key innovation is that the LOD function is uniform across the entire rendered mesh, based on precise 3D distance between observer and terrain [20][21].

### 8.2 Quadtree Structure

The terrain area is divided into a uniform quadtree:
- Each node covers a rectangular area of the heightmap
- Each node contains a bounding box with height sampling for accurate distance calculations
- Deeper tree levels = finer LOD
- Typical depth: 8-12 levels for planet-scale terrain

**LOD range calculation:**
```
lodRanges[i] = minLodDistance * 2^i
```

LOD ranges increase exponentially by factor of 2 to prevent nodes from spanning multiple LOD ranges.

### 8.3 LOD Selection Algorithm

Per-frame traversal from top node downward:

```pseudocode
function selectLOD(node, camera):
    if not frustumCull(node.boundingBox, camera):
        return  // skip invisible nodes

    dist = distance(camera.position, node.boundingSphere.center)

    if node.level == 0:  // finest level
        renderNode(node)
    elif dist > lodRanges[node.level]:
        renderNode(node)  // far enough, render at this LOD
    else:
        // Close enough to need higher detail
        for each child in node.children:
            selectLOD(child, camera)
```

### 8.4 Vertex Morphing (Crack-Free Transitions)

The core of CDLOD's crack-free approach: vertices morph continuously between LOD levels in the vertex shader. Morphing starts at 50% of the distance between adjacent LOD ranges [21]:

```glsl
// CDLOD vertex morphing
uniform float morphStart;  // 0.5 (morph begins at 50% of LOD range)

float computeMorphFactor(float distance, float lowRange, float highRange) {
    float factor = (distance - lowRange) / (highRange - lowRange);
    return clamp(factor / morphStart - 1.0, 0.0, 1.0);
}

// Morph vertex position in object space
vec2 morphVertex(vec2 meshPos, float meshDim, float morphValue) {
    vec2 fraction = fract(meshPos * meshDim * 0.5) * 2.0 / meshDim;
    return meshPos - fraction * morphValue;
}
```

**How it works:** Each vertex at an even grid position snaps to its coarser-level equivalent when the morph factor reaches 1.0. This eliminates T-junctions because the morphed fine mesh exactly matches the coarse mesh at transitions.

### 8.5 Rendering Pipeline

Each selected node is rendered by covering its area with one mesh grid at a given resolution:
- Typically a **33x33 or 65x65 vertex grid** per node
- Same mesh is reused for all nodes (instanced rendering)
- Vertex shader scales/translates the mesh to cover the node area
- Height is sampled from a heightmap texture
- Morph factor is computed per-vertex based on camera distance

**Performance characteristics:**
- Shader Model 3.0+ compatible
- ~6L + 5 draw calls for L levels (similar to clipmaps)
- Better screen-triangle distribution than clipmaps
- No stitching meshes required between LOD levels
- Source code available: [CDLOD on GitHub](https://github.com/fstrugar/CDLOD) (DX9, public domain)

### 8.6 Adapting CDLOD to Spherical Planets

For planet rendering, adapt CDLOD by:
1. Start with 6 quadtrees (one per cube face)
2. Project cube positions to sphere in the vertex shader
3. Replace 2D distance with 3D distance to terrain surface point
4. Height sampling uses cubemap texture lookup
5. LOD ranges may need adjustment for curved horizon (nodes behind the horizon should be culled)

An Android implementation exists: [terrain-sandbox](https://github.com/sduenasg/terrain-sandbox) using CDLOD with OpenGL ES 3.0 on a spherical planet.

### References (Section 8)
- [20] [CDLOD Paper - Filip Strugar](https://aggrobird.com/files/cdlod_latest.pdf)
- [21] [CDLOD Terrain Implementation - svnte.se](https://svnte.se/cdlod-terrain)

---

## 9. Chunked LOD

### 9.1 Overview

Chunked LOD was developed by Thatcher Ulrich (SIGGRAPH 2002). Each "chunk" is a rectangular, precomputed section of optimized geometry organized in a quadtree [22].

### 9.2 Architecture

**Quadtree of Chunks:**
- Each chunk is a self-contained mesh (vertex buffer + index buffer)
- Chunks at each level cover the same area but with different triangle counts
- Parent chunk covers 4x the area of each child at half the resolution
- Chunks are pre-built offline and stored on disk

**Key advantages:**
- Very low CPU overhead (no per-vertex LOD decisions at runtime)
- High triangle throughput (large batches of optimized geometry)
- Integrated texture LOD with geometry LOD
- Efficient integration with out-of-core storage (streaming from disk)
- Smooth vertex morphing without vertex pops

### 9.3 Chunk Selection

```pseudocode
function selectChunks(node, camera, errorThreshold):
    screenSpaceError = computeScreenSpaceError(node, camera)

    if screenSpaceError < errorThreshold:
        renderChunk(node)  // this LOD is fine enough
    elif node.hasChildren:
        for each child in node.children:
            selectChunks(child, camera, errorThreshold)
    else:
        renderChunk(node)  // finest available LOD
```

**Screen-space error metric:**
```
screenSpaceError = (geometricError * screenHeight) / (distance * 2 * tan(fov/2))
```

Where `geometricError` is the maximum deviation (in world units) between this LOD and the full-resolution mesh.

### 9.4 Crack Prevention: Skirts

Rather than forcing chunk edges to match, Chunked LOD uses **skirts** -- vertical strips of geometry hanging below the terrain edge [22][23]:

- Skirts extend downward from every edge vertex
- Height of skirt = maximum possible height error at that LOD level
- Fills any gap caused by LOD mismatch between adjacent chunks
- For spherical terrain: skirts point toward planet center, with maximum length = distance from terrain vertex to center minus minimum terrain radius

**Skirt generation pseudocode:**
```pseudocode
for each edge_vertex in chunk:
    skirt_vertex = edge_vertex
    skirt_vertex.position -= normal * skirtHeight
    addTriangle(edge_vertex, next_edge_vertex, skirt_vertex)
    addTriangle(next_edge_vertex, next_skirt_vertex, skirt_vertex)
```

### 9.5 Spherical Adaptation

For planet rendering [23]:
- Start with a triangulated cube (or icosahedron)
- 6 quadtrees rooted at cube faces
- Each chunk's vertices are projected onto the sphere
- Chunks near cube corners experience stretching; mitigate by splitting at smaller sizes near corners
- LOD metric accounts for spherical curvature (chunks behind horizon are culled early)

### References (Section 9)
- [22] [Rendering Massive Terrains using Chunked LOD - Thatcher Ulrich](https://tulrich.com/geekstuff/sig-notes.pdf)
- [23] [Spherical Chunked LOD with Skirts - Ogre Forums](https://forums.ogre3d.org/viewtopic.php?t=69780)

---

## 10. Geometry Clipmaps for Spherical Terrain

### 10.1 Overview

Geometry clipmaps (Losasso & Hoppe 2004, GPU implementation by Asirvatham & Hoppe 2005) cache terrain in a set of nested regular grids centered on the viewer. Each level is twice the spatial extent of the previous but at the same vertex resolution, keeping triangle sizes uniform in screen space [24][25].

### 10.2 Ring Structure

Grid size **n = 2^k - 1** (typically n = 255). Only the finest level is a complete grid square; all others are hollow rings:

```
Level 0 (finest):  255 x 255 grid, spacing s
Level 1:           255 x 255 ring, spacing 2s
Level 2:           255 x 255 ring, spacing 4s
...
Level L-1:         255 x 255 ring, spacing 2^(L-1) * s
```

Each ring is subdivided into **12 blocks** of size m x m where m = (n+1)/4 = 64 for n = 255.

### 10.3 Memory Footprint

Per clipmap level:
- One n x n single-channel float elevation texture
- One 2n x 2n RGBA8 normal map (higher resolution for better shading)

**Total for the continental US heightmap (216,000 x 93,600 at 30m spacing):**
- Raw data: 40 GB
- Compressed: 355 MB (>100:1 compression)
- GPU memory: O(n^2 * L) where L = number of levels

### 10.4 Toroidal Addressing (Incremental Update)

As the viewer moves, clipmap windows translate within their pyramid levels using **wraparound addressing**. This converts L-shaped update regions into + shapes, requiring only two quads per frame update [24]:

```pseudocode
// Per-level toroidal update
function updateClipmap(level, newCenter):
    offset = newCenter - level.oldCenter
    // Quantize offset to grid spacing
    offset = round(offset / level.gridSpacing) * level.gridSpacing

    if offset == 0: return  // no update needed

    // Update only the new strip(s) exposed by movement
    // Horizontal strip if moved in X
    if offset.x != 0:
        updateStrip(level, horizontal, offset.x)
    // Vertical strip if moved in Y
    if offset.y != 0:
        updateStrip(level, vertical, offset.y)

    level.oldCenter = newCenter
```

### 10.5 Transition Blending

At level boundaries, a transition region of width w = n/10 morphs geometry and textures [24]:

```glsl
// Vertex shader transition blending
float alpha = computeTransitionBlend(vertexPos, levelExtent, transitionWidth);
float z_fine = textureLod(heightmap, fineUV, fineLevel).r;
float z_coarse = textureLod(heightmap, coarseUV, coarseLevel).r;
float z_blended = mix(z_fine, z_coarse, alpha);
```

### 10.6 Upsampling Algorithm

Uses tensor-product four-point subdivision with mask weights (-1/16, 9/16, 9/16, -1/16) [24]:
- Even-even positions: 1 texture lookup (direct copy)
- Odd-even/even-odd: 4 lookups (1D interpolation)
- Odd-odd positions: 16 lookups (2D interpolation)

### 10.7 Performance Metrics (GPU Gems 2)

On contemporary hardware [24]:
- **130 FPS** with view frustum culling
- **60 million triangles/second** rendering rate
- Upsampling: 1.0 ms per full 255x255 level
- Decompression: 8 ms per level
- Normal-map computation: 0.6 ms per level
- Draw calls: average 6L + 5 (71 for L=11 levels)
- 87 fps (decompressed terrain), 120 fps (synthesized terrain)

### 10.8 Spherical Adaptations

**Ellipsoidal Clipmaps** (Shader & Stamminger 2015) adapt geometry clipmaps to planet-scale rendering [25]:
- Divide ellipsoid into three partitions, seamlessly stitched
- Underlying ellipsoidal grid generated on the fly in the vertex shader
- Guarantees sub-pixel precision of Earth reference ellipsoid surface
- Exploits GPU single-precision float arithmetic
- Constant memory footprint, all vertices in video memory
- No explicit vertex I/O at runtime

**Spherical Clipmaps** (Clasen & Hege) render terrain on spherical surfaces using clipmaps with high geometry throughput of GPU rendering large static triangle sets displaced by height map textures.

### References (Section 10)
- [24] [GPU-Based Geometry Clipmaps - GPU Gems 2, Ch.2](https://developer.nvidia.com/gpugems/gpugems2/part-i-geometric-complexity/chapter-2-terrain-rendering-using-gpu-based-geometry)
- [25] [Ellipsoidal Clipmaps - Computers & Graphics 2015](https://www.sciencedirect.com/science/article/abs/pii/S0097849315000916)

---

## 11. GPU Tessellation-Based LOD

### 11.1 Overview

Hardware tessellation (DX11+, OpenGL 4.0+, Vulkan) subdivides coarse patches on the GPU, eliminating CPU-side mesh generation. The tessellation control shader (TCS/hull shader) sets tessellation factors; the tessellation evaluation shader (TES/domain shader) places generated vertices [26][27].

### 11.2 Tessellation Factor Calculation

**Camera distance approach:**
```glsl
// Tessellation control shader
float computeTessFactor(vec3 edgeMidpoint, vec3 cameraPos) {
    float dist = distance(edgeMidpoint, cameraPos);
    float factor = maxTessLevel * (1.0 - clamp(dist / maxDistance, 0.0, 1.0));
    return max(factor, 1.0);
}

gl_TessLevelOuter[0] = computeTessFactor(edgeMid01, cameraPos);
gl_TessLevelOuter[1] = computeTessFactor(edgeMid12, cameraPos);
gl_TessLevelOuter[2] = computeTessFactor(edgeMid23, cameraPos);
gl_TessLevelOuter[3] = computeTessFactor(edgeMid30, cameraPos);
gl_TessLevelInner[0] = (gl_TessLevelOuter[1] + gl_TessLevelOuter[3]) * 0.5;
gl_TessLevelInner[1] = (gl_TessLevelOuter[0] + gl_TessLevelOuter[2]) * 0.5;
```

**Sphere projection approach** (preferred -- avoids artifacts with perpendicular edges):
```glsl
float computeTessFactorSphere(vec3 v0, vec3 v1, mat4 viewProj) {
    vec3 center = (v0 + v1) * 0.5;
    float radius = distance(v0, v1) * 0.5;

    // Project sphere to screen space
    vec4 clipCenter = viewProj * vec4(center, 1.0);
    float screenDiameter = (radius * 2.0 * screenHeight) / clipCenter.w;

    // Target: one triangle per N pixels
    float targetTriangleWidth = 8.0;  // pixels
    return clamp(screenDiameter / targetTriangleWidth, 1.0, 64.0);
}
```

Maximum tessellation level: **64** (OpenGL/Vulkan), **64** (DX11/12).

### 11.3 Crack-Free Edge Matching

The critical requirement: adjacent patches must produce identical vertices along shared edges [26].

**Solution: scale factors + power-of-two clamping:**
1. After quadtree construction, find neighbors for each patch
2. If a smaller patch borders a larger one, its shared edge gets a scale factor of 0.5
3. Clamp tessellation levels to powers of two
4. Use `fractional_even_spacing` in the TES to ensure symmetric vertex placement

```glsl
// Tessellation evaluation shader
layout(quads, fractional_even_spacing, ccw) in;

void main() {
    vec2 uv = gl_TessCoord.xy;
    // Bilinear interpolation of control points
    vec3 p = mix(
        mix(cp[0], cp[1], uv.x),
        mix(cp[3], cp[2], uv.x),
        uv.y
    );
    // Displace by heightmap
    float height = texture(heightmap, computeHeightmapUV(p)).r;
    p = normalize(p) * (planetRadius + height);
    gl_Position = viewProj * vec4(p, 1.0);
}
```

### 11.4 Terrain-Adaptive LOD with GPU Tessellation

A research implementation (2021) achieved terrain-adaptive LOD control on GPU tessellation for large-scale terrain [28]:
- Tessellation factors adapt based on both camera distance and terrain roughness
- Rougher terrain gets higher tessellation, smooth areas get less
- Significantly reduces triangle count in flat areas
- Dynamic stitching strips (DSS) fill gaps between patches at different LOD levels

### 11.5 Performance Characteristics

| Approach | CPU Cost | GPU Cost | Triangle Efficiency | Max Detail |
|----------|---------|---------|-------------------|-----------|
| Pre-tessellated mesh | High (mesh gen) | Low (static VB) | High | Limited by VB size |
| GPU tessellation | Very low | Medium (tess stages) | Medium | 64x subdivisions |
| Hybrid (coarse mesh + tess) | Low | Medium | High | Very high |

**Key limitation:** Tessellation hardware has a maximum factor of 64, meaning a single quad patch can produce at most ~4096 triangles. For planet-scale detail, combine tessellation with a quadtree of coarse patches.

### References (Section 11)
- [26] [Tessellated Terrain Rendering with Dynamic LOD - Victor Bush](https://victorbush.com/2015/01/tessellated-terrain/)
- [27] [Multi-resolution Terrain Rendering with GPU Tessellation - ResearchGate](https://www.researchgate.net/publication/271736902_Multi-resolution_terrain_rendering_with_GPU_tessellation)
- [28] [Large-scale Terrain-Adaptive LOD with GPU Tessellation - ScienceDirect](https://www.sciencedirect.com/science/article/pii/S1110016821000326)

---

## 12. Mesh Shader LOD for Planets

### 12.1 Overview

Mesh shaders (NVIDIA Turing+, AMD RDNA 2+, DX12 Ultimate / Vulkan) replace the traditional vertex+geometry+tessellation pipeline with a compute-like model that directly emits meshlets to the rasterizer [29][30].

### 12.2 Pipeline Architecture

```
Task Shader (optional)          Mesh Shader              Fragment Shader
- Frustum culling              - Generate vertices       - Shading
- LOD selection                - Generate primitives     - Texture sampling
- Occlusion culling            - Emit meshlet            - Virtual texture lookup
- Spawn mesh shaders           (max 256 verts,
                                max 256 primitives)
```

**Key advantages over tessellation for terrain:**
- Task shader performs coarse-grained culling before any geometry is generated
- No fixed 64x tessellation limit
- Better mapping to modern GPU compute hardware
- Can replace two compute shaders with a single task shader

### 12.3 Planet Terrain with Mesh Shaders

A bachelor thesis from TU Wien (Rumpelnik 2020) demonstrated planetary rendering with mesh shaders [30]:

**Approach:**
1. Six quadtrees (one per cube face) manage LOD
2. Task shader performs per-node frustum culling and LOD selection
3. Mesh shader generates terrain meshlets with displacement from heightmap
4. Vertices projected from cube to sphere in the mesh shader

**Comparison with tessellation pipeline:**
- Mesh shaders provide more flexible geometry amplification
- Task shader culling eliminates work earlier in the pipeline
- Better GPU occupancy for irregular terrain topology

### 12.4 Meshlet Organization for Terrain

```pseudocode
// Terrain meshlet: 16x16 vertex grid = 256 vertices, 15x15x2 = 450 triangles
// Split into sub-meshlets of 64 vertices, 126 triangles each

struct TerrainMeshlet {
    vec3 boundingSphere;
    float maxError;      // geometric error for LOD
    uint vertexOffset;
    uint vertexCount;    // max 256
    uint triangleOffset;
    uint triangleCount;  // max 256
};
```

### 12.5 LOD Selection in Task Shader

```glsl
// Task shader: decide whether to render this node or subdivide
taskPayloadSharedEXT TaskPayload payload;

void main() {
    uint nodeIdx = gl_WorkGroupID.x;
    TerrainNode node = nodes[nodeIdx];

    // Frustum cull
    if (!isVisible(node.bounds, viewProj)) {
        return;  // emit 0 mesh shader workgroups
    }

    // Screen-space error metric
    float screenError = node.geometricError * screenHeight /
                        (distance(cameraPos, node.center) * 2.0 * tan(fov * 0.5));

    if (screenError < errorThreshold) {
        // Render this node's meshlets
        payload.nodeIndex = nodeIdx;
        EmitMeshTasksEXT(node.meshletCount, 1, 1);
    }
    // else: children will be dispatched by CPU-side quadtree traversal
}
```

### References (Section 12)
- [29] [Using Mesh Shaders for CLOD Terrain - SIGGRAPH 2020](https://dl.acm.org/doi/10.1145/3388767.3407391)
- [30] [Planetary Rendering with Mesh Shaders - TU Wien 2020](https://www.cg.tuwien.ac.at/research/publications/2020/rumpelnik_martin_2020_PRM/rumpelnik_martin_2020_PRM-Thesis.pdf)

---

## 13. Cube-to-Sphere Projections

### 13.1 Why Cube-to-Sphere

A sphere is most practically represented as a cube with 6 faces projected outward. This avoids the polar singularities of latitude-longitude grids where grid spacing approaches zero near the poles. Cube maps also align well with GPU cubemap texture hardware [31][32].

### 13.2 Projection Methods Compared

| Projection | Max/Min Area Ratio | Angular Distortion | Complexity | Best For |
|------------|-------------------|-------------------|------------|----------|
| **Gnomonic** (normalize) | 5.2:1 | High at corners | Trivial | Quick prototypes |
| **Tangential** | ~5.2:1 | High | Low | Legacy systems |
| **Adjusted Gnomonic** (Lerbour) | 1.41:1 | Low | Low | Good general purpose |
| **Quadrilateralized Spherical Cube (QSC)** | 1.0:1 (equal area) | Limited | Medium | Scientific visualization |
| **Ellipsoidal Cube Map (ECM)** | ~1.0:1 | Very low | High | Precision geodetic |

### 13.3 Implementation

**Simple normalization (gnomonic):**
```glsl
vec3 cubeToSphere(vec3 cubePos) {
    return normalize(cubePos);
}
```
Problem: severe area distortion at cube corners (5.2x), meaning corner texels cover 5x more area.

**Adjusted mapping (Lerbour -- recommended):**
```glsl
// Reduces max distortion from 5.2:1 to 1.41:1
vec3 cubeToSphereAdjusted(vec3 p) {
    vec3 p2 = p * p;
    vec3 result;
    result.x = p.x * sqrt(1.0 - p2.y * 0.5 - p2.z * 0.5 + p2.y * p2.z / 3.0);
    result.y = p.y * sqrt(1.0 - p2.z * 0.5 - p2.x * 0.5 + p2.z * p2.x / 3.0);
    result.z = p.z * sqrt(1.0 - p2.x * 0.5 - p2.y * 0.5 + p2.x * p2.y / 3.0);
    return result;
}
```

This "adjusted spherical cube" mapping has a maximum-to-minimum area distortion of only 1.414:1, far better than most non-equal-area projections [31].

### 13.4 Impact on LOD

Area distortion affects LOD: regions with higher distortion need more subdivision to maintain uniform screen-space resolution. With gnomonic projection, corner nodes need ~2.3x more subdivision depth than center nodes. The adjusted mapping nearly eliminates this problem.

### References (Section 13)
- [31] [Comparison of Spherical Cube Map Projections - Dimitrijevic & Lambers](https://docslib.org/doc/860041/comparison-of-spherical-cube-map-projections-used-in-planet-sized-terrain-rendering)
- [32] [Making Worlds 1: Of Spheres and Cubes - Acko.net](https://acko.net/blog/making-worlds-1-of-spheres-and-cubes/)

---

## 14. Crack-Free Rendering Techniques

### 14.1 The T-Junction Problem

Anywhere two patches meet at different LOD levels, T-junctions create gaps. One side has a straight edge while the other side has two edges with a vertex in the middle. The middle vertex rarely lies exactly on the straight edge, producing visible holes, shading discontinuities, and z-fighting [26][33].

### 14.2 Solution Taxonomy

| Technique | Complexity | Quality | CPU Cost | GPU Cost |
|-----------|-----------|---------|---------|---------|
| **Vertex morphing** (CDLOD) | Medium | Excellent | Low | Low (vertex shader) |
| **Skirts** (Chunked LOD) | Low | Good | Very low | Low (extra triangles) |
| **Edge stitching strips** | Medium | Good | Medium | Low |
| **Tessellation edge matching** | Medium | Excellent | Low | Medium (tess stage) |
| **Overlapping tiles** (No Man's Sky) | Low | Good | Very low | Low (overdraw) |
| **Transition blending** (Clipmaps) | Medium | Excellent | Low | Low (vertex shader) |

### 14.3 Vertex Morphing (CDLOD Approach)

Vertices smoothly morph between LOD levels based on distance. When a vertex reaches its LOD boundary, it has already morphed to exactly match the coarser level. No cracks ever appear because the transition is continuous [20][21].

**Pros:** Zero visual artifacts, mathematically correct
**Cons:** Requires vertex shader computation, slightly more complex LOD logic

### 14.4 Skirts (Chunked LOD Approach)

Vertical strips of geometry extend below every edge of every chunk:

```pseudocode
skirtHeight = maxHeightError * 2  // conservative
for each edge vertex V:
    V_below = V - surfaceNormal * skirtHeight
    emit triangle strip connecting V to V_below
```

For spherical terrain, skirts point toward the planet center [22][23].

**Pros:** Dead simple, no inter-chunk communication needed
**Cons:** Wastes triangles on invisible geometry, visible from extreme angles

### 14.5 Tessellation Edge Matching

The tessellation control shader explicitly matches edge tessellation factors between adjacent patches [26]:

1. For each patch, check 4 neighbors (N, S, E, W)
2. If neighbor is larger (coarser LOD), scale shared edge factor by 0.5
3. Use `fractional_even_spacing` for symmetric vertex placement
4. Clamp to powers of two for guaranteed matching

### 14.6 Dynamic Stitching Strips (DSS)

A 2019 approach uses dynamically generated thin strips to bridge LOD gaps [33]:
- Strips are generated between adjacent patches at different LODs
- Strip resolution matches the finer of the two patches
- Vertices interpolate between fine and coarse edge positions
- Provides smooth transitions with minimal overdraw

### 14.7 Transition Blending (Clipmap Approach)

A transition region of width w = n/10 at level boundaries blends between fine and coarse geometry [24]:

```glsl
float alpha = smoothstep(levelExtent - transitionWidth, levelExtent, dist);
float height = mix(fineHeight, coarseHeight, alpha);
```

### References (Section 14)
- [33] [Multilevel Terrain Rendering with Dynamic Stitching Strips - MDPI 2019](https://www.mdpi.com/2220-9964/8/6/255)

---

## 15. LOD System Comparison and Recommendations

### 15.1 Feature Comparison

| Feature | CDLOD | Chunked LOD | Clipmaps | GPU Tessellation | Mesh Shaders |
|---------|-------|-------------|----------|-----------------|-------------|
| **Crack-free** | Morphing (excellent) | Skirts (good) | Blending (excellent) | Edge matching (excellent) | Custom (varies) |
| **CPU cost** | Low (quadtree traverse) | Low (quadtree traverse) | Very low (shift only) | Very low | Very low |
| **GPU cost** | Low | Low | Low | Medium (tess stages) | Medium |
| **Draw calls** | ~6L+5 | ~nodes visible | ~6L+5 | ~patches visible | ~meshlets |
| **Streaming** | Easy (quadtree = natural tile hierarchy) | Easy (chunks = tiles) | Medium (ring updates) | Hard (no natural tiling) | Medium |
| **Max detail** | Unlimited (add quadtree levels) | Limited by chunk count | Limited by L levels | Max 64x per patch | Max 256 verts/meshlet |
| **Spherical** | Natural (6 quadtrees) | Natural (6 quadtrees) | Requires adaptation | Natural | Natural |
| **Hardware req** | SM 3.0+ | SM 2.0+ | SM 3.0+ (vtx tex fetch) | SM 5.0+ (DX11+) | DX12 Ultimate / Vulkan |
| **VT integration** | Natural | Natural | Natural | Possible | Natural |

### 15.2 Recommended Architecture for a Planet Renderer

**Primary LOD: CDLOD with Virtual Texturing**

CDLOD is recommended as the primary LOD system for a procedural planet renderer because:
1. It provides mathematically correct crack-free transitions via vertex morphing
2. The quadtree structure maps directly to virtual texture page hierarchy
3. Low hardware requirements (works on older GPUs)
4. Well-documented with public-domain reference implementation
5. Natural adaptation to spherical geometry

**Hybrid Enhancement: CDLOD + GPU Tessellation**

For maximum detail at close range:
1. CDLOD manages macro-scale LOD (quadtree levels 0-8)
2. GPU tessellation adds micro-detail within the finest CDLOD patches
3. Tessellation factor driven by screen-space error metric
4. Heightmap detail beyond the CDLOD grid resolution

**Future-Proof: Mesh Shader Path**

For DX12 Ultimate / Vulkan hardware:
1. Task shader replaces CPU quadtree traversal (GPU-driven rendering)
2. Mesh shader generates terrain meshlets directly
3. Entire LOD pipeline on GPU, zero CPU geometry bottleneck
4. Requires NVIDIA Turing+ or AMD RDNA 2+

### 15.3 Memory Budget Summary

**Complete planet renderer at 32K per cubemap face:**

| Component | Memory |
|-----------|--------|
| Virtual texture page cache (BC7/BC4/BC5) | 256-1024 MB |
| Page table (6 faces x 4 maps) | ~1.5 MB |
| Feedback buffer (240x135 RGBA8) | ~130 KB |
| CDLOD heightmap cache (6 faces, streaming) | 64-256 MB |
| CDLOD mesh grid (reused per node) | ~65 KB per grid |
| Normal maps (per-patch, 128x128 RGBA8) | 64 MB (1024 cached patches) |
| **Total VRAM** | **400 MB - 1.5 GB** |

### 15.4 Performance Targets

| Metric | Target | Notes |
|--------|--------|-------|
| Triangle count | 1-4M per frame | CDLOD with ~200-500 visible nodes |
| Draw calls | 50-200 | Instanced rendering, one mesh per LOD level |
| Texture tiles streamed/frame | 4-16 | 128-512 KB/frame at 60 fps |
| LOD transitions | <1 frame | Vertex morphing eliminates popping |
| Ground-level detail | ~1m resolution | At finest CDLOD level with tessellation |
| Orbital view | Full planet visible | Coarsest LOD levels, ~10K triangles |

---

## Sources

1. [Procedural Planetary Surfaces - Toni Sagrista](https://tonisagrista.com/blog/2021/procedural-planetary-surfaces/)
2. [Procedural Planet Rendering - Jad Khoury](https://jadkhoury.github.io/terrain_blog.html)
3. [Generating Complex Procedural Terrains Using the GPU - NVIDIA GPU Gems 3](https://developer.nvidia.com/gpugems/gpugems3/part-i-geometry/chapter-1-generating-complex-procedural-terrains-using-gpu)
4. [Tile-Based Texture Mapping - GPU Gems 2, Ch.12](https://developer.nvidia.com/gpugems/gpugems2/part-ii-shading-lighting-and-shadows/chapter-12-tile-based-texture-mapping)
5. [MegaTexture - Wikipedia](https://en.wikipedia.org/wiki/MegaTexture)
6. [Virtual Texturing in UE - Epic Games](https://dev.epicgames.com/documentation/en-us/unreal-engine/virtual-texturing-in-unreal-engine)
7. [Virtual Texturing - PlayerUnknown Productions](https://playerunknownproductions.net/news/virtual-texturing)
8. [Understanding BCn Texture Compression - Nathan Reed](https://www.reedbeta.com/blog/understanding-bcn-texture-compression-formats/)
9. [ASTC Format Overview - ARM Software](https://github.com/ARM-software/astc-encoder/blob/main/Docs/FormatOverview.md)
10. [Texture Compression in 2020 - Aras Pranckevi](https://aras-p.info/blog/2020/12/08/Texture-Compression-in-2020/)
11. [GPU Texture Encoder - Tellusim](https://tellusim.com/gpu-encoder/)
12. [Betsy GPU Texture Compressor - Godot Engine](https://godotengine.org/article/betsy-gpu-texture-compressor/)
13. [vk_compute_mipmaps - NVIDIA](https://github.com/nvpro-samples/vk_compute_mipmaps)
14. [Uploading Textures to GPU - The Good Way](https://erfan-ahmadi.github.io/blog/Nabla/imageupload)
15. [Sparse Resources Investigation - gpuweb](https://github.com/gpuweb/gpuweb/issues/455)
16. [Terrain Rendering Using GPU-Based Geometry Clipmaps - GPU Gems 2](https://developer.nvidia.com/gpugems/gpugems2/part-i-geometric-complexity/chapter-2-terrain-rendering-using-gpu-based-geometry)
17. [How Virtual Textures Really Work - shlom.dev](https://www.shlom.dev/articles/how-virtual-textures-really-work/)
18. [Sparse Virtual Textures - Studio Pixl](https://studiopixl.com/2022-04-27/sparse-virtual-textures)
19. [The Sad State of Hardware Virtual Textures - HAL Science 2025](https://hal.science/hal-05138369/file/The_Sad_State_of_Hardware_Virtual_Textures.pdf)
20. [CDLOD Paper - Filip Strugar](https://aggrobird.com/files/cdlod_latest.pdf)
21. [CDLOD Terrain Implementation - svnte.se](https://svnte.se/cdlod-terrain)
22. [Rendering Massive Terrains using Chunked LOD - Thatcher Ulrich](https://tulrich.com/geekstuff/sig-notes.pdf)
23. [Spherical Chunked LOD with Skirts - Ogre Forums](https://forums.ogre3d.org/viewtopic.php?t=69780)
24. [GPU-Based Geometry Clipmaps - GPU Gems 2, Ch.2](https://developer.nvidia.com/gpugems/gpugems2/part-i-geometric-complexity/chapter-2-terrain-rendering-using-gpu-based-geometry)
25. [Ellipsoidal Clipmaps - Computers & Graphics 2015](https://www.sciencedirect.com/science/article/abs/pii/S0097849315000916)
26. [Tessellated Terrain with Dynamic LOD - Victor Bush](https://victorbush.com/2015/01/tessellated-terrain/)
27. [Multi-resolution Terrain with GPU Tessellation - ResearchGate](https://www.researchgate.net/publication/271736902_Multi-resolution_terrain_rendering_with_GPU_tessellation)
28. [Terrain-Adaptive LOD with GPU Tessellation - ScienceDirect 2021](https://www.sciencedirect.com/science/article/pii/S1110016821000326)
29. [Mesh Shaders for CLOD Terrain - SIGGRAPH 2020](https://dl.acm.org/doi/10.1145/3388767.3407391)
30. [Planetary Rendering with Mesh Shaders - TU Wien 2020](https://www.cg.tuwien.ac.at/research/publications/2020/rumpelnik_martin_2020_PRM/rumpelnik_martin_2020_PRM-Thesis.pdf)
31. [Comparison of Spherical Cube Map Projections - Dimitrijevic & Lambers](https://docslib.org/doc/860041/comparison-of-spherical-cube-map-projections-used-in-planet-sized-terrain-rendering)
32. [Making Worlds 1: Of Spheres and Cubes - Acko.net](https://acko.net/blog/making-worlds-1-of-spheres-and-cubes/)
33. [Multilevel Terrain with Dynamic Stitching Strips - MDPI 2019](https://www.mdpi.com/2220-9964/8/6/255)
34. [Planetary Terrain Rendering - dexyfex.com](https://dexyfex.com/2015/11/30/planetary-terrain-rendering/)
35. [CDLOD Reference Implementation - GitHub](https://github.com/fstrugar/CDLOD)
36. [BC7E GPU Port - Aras P](https://github.com/aras-p/bc7e-on-gpu)
37. [Quadtree LOD for Planets - GitHub](https://github.com/tanerius/Quadtree_LOD)
38. [Terrain LOD Papers Archive - VTerrain.org](http://vterrain.org/LOD/Papers/)
