# High-Resolution Texture Generation & Virtual Texturing for Procedural Planets

_Research date: 2026-03-28_

---

## 1. Generating 32K+ Textures Procedurally

### Memory Reality Check

A single 32768x16384 equirectangular planet texture:

| Format | Bytes/pixel | Total Size | With mipmaps (~1.33x) |
|--------|-------------|------------|-----------------------|
| RGBA8 uncompressed | 4 | **2 GB** | ~2.66 GB |
| RGBA16F (HDR) | 8 | **4 GB** | ~5.32 GB |
| BC7 compressed | 1 (8 bpp) | **512 MB** | ~682 MB |
| BC1 compressed | 0.5 (4 bpp) | **256 MB** | ~341 MB |
| ASTC 6x6 | ~0.89 | ~457 MB | ~608 MB |

A full planet needs at minimum diffuse + normal + roughness = 3 maps. At 32K BC7: **~1.5 GB** just for surface textures with mipmaps. This exceeds typical VRAM budgets, making streaming/virtual texturing mandatory.

[BC7 format specification](https://learn.microsoft.com/en-us/windows/win32/direct3d11/bc7-format) |
[BCn compression overview](https://www.reedbeta.com/blog/understanding-bcn-texture-compression-formats/) |
[Texture compression in 2020](https://aras-p.info/blog/2020/12/08/Texture-Compression-in-2020/)

### Tiled Procedural Generation Pipeline

Rather than generating the full texture at once, divide the sphere into tiles and generate on demand:

1. **Cube-face tiling** -- 6 faces, each subdivided into an NxN grid of tiles (e.g., 256x256 px tiles). A 32K equirect = 6 cube faces of ~5461x5461 each, or ~21x21 tiles per face at 256px.
2. **Quadtree subdivision** -- tiles are generated at the LOD required by the current view. Close tiles get full resolution; distant tiles use coarser mip levels.
3. **Compute shader generation** -- each tile dispatched as a compute shader workgroup. Noise functions (FBM, ridged multifractal, Voronoi) evaluated per-texel with world-space coordinates.
4. **Border padding** -- each tile rendered with 1-4 texel overlap/padding to avoid seam artifacts at tile boundaries. Required for correct filtering and normal map computation.

**Tileable noise**: Sampling noise along a circumference embedded in higher-dimensional space produces seamless tileable noise. For cube-face seams, noise must be evaluated in 3D world space (not 2D tile space) to guarantee continuity.

[NVIDIA GPU Gems 3: Procedural Terrains](https://developer.nvidia.com/gpugems/gpugems3/part-i-geometry/chapter-1-generating-complex-procedural-terrains-using-gpu) |
[Procedural planetary surfaces (Toni Sagrista)](https://tonisagrista.com/blog/2021/procedural-planetary-surfaces/) |
[Tileable procedural shaders (GitHub)](https://github.com/tuxalin/procedural-tileable-shaders) |
[Procedural planet rendering (Jad Khoury)](https://jadkhoury.github.io/terrain_blog.html)

### Streaming Generation Strategy

- Generate tiles asynchronously on a background compute queue
- Priority queue ordered by screen-space error (tiles contributing most pixels first)
- Cache generated tiles to disk (compressed) so regeneration is avoided on revisit
- Budget: generate N tiles per frame (e.g., 4-8 tiles/frame at 256x256) to keep frame time stable

---

## 2. Virtual Texturing (SVT / Megatexture)

### Core Architecture

Virtual texturing decouples the *logical* texture resolution from *physical* GPU memory. The full planet texture is a "virtual" address space; only visible tiles are resident in a GPU-side cache.

**Three-pass pipeline:**

1. **Feedback pass** -- render scene with a special shader that outputs tile IDs (virtual page coordinates + mip level) to a low-resolution feedback buffer (e.g., screen/8 or screen/16). Each pixel stores which virtual tile it needs.
2. **Tile update** -- CPU reads back the feedback buffer (async, 2-3 frame latency). Processes unique tile requests. Loads/generates missing tiles into a **physical tile cache** (texture atlas).
3. **Final render** -- fragment shader translates virtual UVs to physical cache UVs via an **indirection texture** (page table). Each texel of the indirection texture stores the physical cache location + scale/bias for that virtual tile.

[Sparse Virtual Textures (Toni Sagrista, detailed implementation)](https://tonisagrista.com/blog/2023/sparse-virtual-textures/) |
[How Virtual Textures Really Work (shlom.dev)](https://www.shlom.dev/articles/how-virtual-textures-really-work/) |
[SVT implementation notes (Holger Dammertz)](https://holger.dammertz.org/stuff/notes_VirtualTexturing.html) |
[Virtual Texturing (PLAYERUNKNOWN Productions)](https://playerunknownproductions.net/news/virtual-texturing) |
[NVIDIA GTC 2010: Massive Texture Data](https://www.nvidia.com/content/GTC-2010/pdfs/2152_GTC2010.pdf)

### Indirection Texture

The indirection texture is a mipmapped texture where each pixel maps to one virtual tile:

```
indirection_texel = {
    physical_cache_x,   // tile column in cache atlas
    physical_cache_y,   // tile row in cache atlas
    mip_level_delta      // difference from requested mip to available mip
};
```

For a 32K virtual texture with 256px tiles: indirection texture = 128x64 pixels (at mip 0). Very small, always resident.

### Physical Tile Cache

A fixed-size texture atlas (e.g., 4096x4096 or 8192x8192) holding the currently resident tiles:

- 4096x4096 atlas with 256px tiles (+ 4px border) = 15x15 = 225 tile slots ~ **64 MB** at BC7
- 8192x8192 atlas with 256px tiles (+ 4px border) = 31x31 = 961 tile slots ~ **256 MB** at BC7
- LRU eviction: least-recently-used tiles replaced when cache is full

### Fallback Mip Chain

Always keep the lowest few mip levels fully resident (e.g., mip 7 = 256x128, trivially small). When a tile is not yet loaded, the shader falls back to the coarsest available mip via the `mip_level_delta` in the indirection texture. This prevents visual holes.

---

## 3. GPU Texture Compression

### BC7

- **Block size**: 4x4 texels, 128 bits (16 bytes) per block
- **Quality**: ~42 dB PSNR, near-transparent for diffuse/albedo
- **Compression ratio**: 4:1 vs RGBA8
- **Platform**: Desktop (DX11+, Vulkan, OpenGL 4.2+)
- **Real-time encoding**: Difficult. BC7 has 8 modes with complex endpoint selection. Brute-force is too slow for real-time.

**GPU encoders:**
- **Betsy** (Godot) -- compute shader encoder for BC1/3/4/5/6, ETC1/2, EAC. Does *not* include BC7 due to complexity. Written in GLSL for Vulkan. [Betsy GPU Compressor (Godot)](https://godotengine.org/article/betsy-gpu-texture-compressor/)
- **NVIDIA Texture Tools 3** -- GPU-accelerated BC7 compression using CUDA. Not real-time but much faster than CPU. [NVIDIA Texture Tools 3](https://developer.nvidia.com/gpu-accelerated-texture-compression)
- **ISPCTextureCompressor** (Intel) -- CPU-based ISPC (SIMD) BC7 encoder. Very fast for CPU, competitive with low-quality GPU encoders. [ISPCTextureCompressor](https://github.com/GameTechDev/ISPCTextureCompressor)
- **bc7enc_rdo** -- rate-distortion optimized BC7 encoder for maximum quality. [bc7enc_rdo (GitHub)](https://github.com/richgel999/bc7enc_rdo)
- **Compressonator 4.0** (AMD) -- GPU-based encoding for BCn formats. [Compressonator 4.0](https://gpuopen.com/learn/compressonator-4-0-utilize-the-power-of-gpu-based-encoding/)

**Practical approach for procedural planets**: Generate tiles as RGBA8 on GPU compute, compress to BC7 on CPU with ISPCTextureCompressor (fast enough for streaming pipeline), upload compressed tile to cache. Alternatively, store uncompressed in the cache and accept higher VRAM usage.

### ASTC

- **Block sizes**: 4x4 to 12x12, giving 8 bpp down to <1 bpp
- **Quality**: Comparable to BC7 at 4x4 (8 bpp); lower at larger block sizes
- **Platform**: Mobile (OpenGL ES 3.1+, Vulkan on mobile), desktop support via extensions
- **Real-time**: Similar challenges to BC7 for high-quality modes

[NVIDIA ASTC guide](https://developer.nvidia.com/astc-texture-compression-for-game-assets) |
[Compressed GPU formats review (Maister)](https://themaister.net/blog/2021/08/29/compressed-gpu-texture-formats-a-review-and-compute-shader-decoders-part-3-3/)

---

## 4. Memory Budgets for Planetary Textures

### Budget Calculation

Targeting a 6-8 GB VRAM GPU, typical allocation:

| Resource | Budget |
|----------|--------|
| Framebuffers (G-buffer, depth, etc.) | ~500 MB |
| Mesh data (planet + objects) | ~200 MB |
| **Texture cache (SVT)** | **1-2 GB** |
| Atmosphere / volumetrics | ~200 MB |
| Shadow maps | ~200 MB |
| Other (UI, post-process, etc.) | ~200 MB |
| **Remaining headroom** | ~2-4 GB |

With a **1 GB SVT cache** at BC7:
- Cache atlas 8192x8192: ~256 MB for one layer
- Three layers (diffuse + normal + roughness): ~768 MB
- Plus indirection textures + feedback buffers: ~10 MB
- Total: ~780 MB, fits comfortably in 1 GB budget

Tile count at BC7 in 768 MB: roughly 3 x 961 = 2883 tiles of 256x256. For a 32K planet with ~16,000 total tiles per layer, this means ~6% residency per layer at max mip -- more than sufficient since only one hemisphere (and much less at full detail) is visible at once.

### Streaming Budget Per Frame

- PCIe 3.0 x16: ~12 GB/s bandwidth
- Transfer queue budget: ~100-200 MB/frame at 60fps if unconstrained
- Practical limit: 4-16 tiles/frame (256px BC7 tiles = ~128 KB each) = 0.5-2 MB/frame
- SSD read: ~3-7 GB/s (NVMe), negligible latency for tile loads

[Unreal texture streaming](https://unrealartoptimization.github.io/book/pipelines/memory/) |
[UE5 texture streaming optimization](https://forums.unrealengine.com/t/how-to-improve-texture-streaming-gpu-performance-in-ue5-over-20-increase-on-gpu-memory-solved/267023)

---

## 5. Texture Atlas Approaches for Planet Tiles

### Cube Map Tiles

Divide each cube face into a quadtree. Each quadtree node = one tile.

```
Level 0: 6 tiles (one per face)
Level 1: 24 tiles (4 per face)
Level 2: 96 tiles
...
Level N: 6 * 4^N tiles
```

For 32K equivalent resolution with 256px tiles: level ~7 (6 * 4^7 = 98,304 tiles). Only a fraction resident at any time.

### Atlas Layout

Physical cache atlas options:

1. **2D Texture Atlas** -- simplest. One large texture, tiles packed in grid. Border pixels (gutter) needed for correct bilinear filtering. Typical: 4px border per tile (256+8 = 264px per slot). Wastes ~6% of atlas space on borders.

2. **Texture Array** -- each slice = one tile. Avoids border issues for filtering within a tile but cross-tile filtering still needs borders. Limited by `maxImageArrayLayers` (typically 2048).

3. **Texture Array of Atlases** -- hybrid. Each slice is an atlas page holding multiple tiles. Good balance of flexibility and hardware limits.

### Gaia Sky SVT Implementation

Gaia Sky's virtual texture system uses a quadtree where:
- Root (level 0) covers the whole area
- Each level has 4x the tiles of the level above, each covering 4x less area
- All tiles are the same pixel resolution (e.g., 512x512)
- Aspect ratio n:1 supported (n root tiles stacking horizontally)
- Tile files stored as `tx_[col]_[row].ext` (JPG or PNG)
- With 31 quadtree levels, each tile covers ~1cm on Earth's surface

[Gaia Sky virtual textures docs](https://gaia.ari.uni-heidelberg.de/gaiasky/docs/master/Virtual-textures.html) |
[Gaia Sky SVT PR](https://codeberg.org/gaiasky/gaiasky/pulls/695) |
[Virtual texture tools (langurmonkey)](https://codeberg.org/langurmonkey/virtualtexture-tools)

---

## 6. Feedback-Driven Virtual Texturing Pipeline

### Full Pipeline (per frame)

```
Frame N:
  1. FEEDBACK PASS (GPU)
     - Render scene at 1/8 or 1/16 resolution
     - Fragment shader: compute virtual tile coords from UVs + mip level
     - Output: RG16UI (tile_x, tile_y) + B8 (mip_level) to feedback RT
     - Cost: very cheap, low-res, simple shader

  2. READBACK (GPU -> CPU, async)
     - vkCmdCopyImageToBuffer from feedback RT to staging buffer
     - Uses double/triple buffering: read back frame N-2 while rendering frame N
     - Latency: 2-3 frames from request to tile availability

  3. TILE ANALYSIS (CPU)
     - Read staging buffer (mapped)
     - Deduplicate tile requests (hash set)
     - Compare against cache occupancy (what's already loaded)
     - Build load/evict lists
     - Priority: screen-space coverage, distance, mip delta

  4. TILE LOAD/GENERATE (CPU + async GPU)
     - Load from disk cache (if previously generated)
     - OR generate via compute shader on async compute queue
     - Compress to BC7 (CPU, ISPCTextureCompressor)
     - Upload to staging buffer

  5. TILE UPLOAD (transfer queue)
     - Copy from staging buffer to physical cache atlas
     - Update indirection texture

  6. FINAL RENDER (GPU)
     - Fragment shader: sample indirection texture -> get physical tile coords
     - Apply scale/bias to convert virtual UV to physical UV
     - Sample from physical cache atlas
     - If tile missing: fall back to coarser mip (always available)
```

### D3D12 Sampler Feedback

D3D12 provides hardware sampler feedback (MinMip feedback map) that automatically records which mip levels were accessed without a separate feedback pass. This eliminates the need for a custom feedback shader.

[D3D12 Sampler Feedback Streaming (Intel, GitHub)](https://github.com/GameTechDev/SamplerFeedbackStreaming) |
[Unity SVT documentation](https://docs.unity3d.com/Manual/svt-how-it-works.html) |
[Adaptive Virtual Textures in Far Cry 4 (GDC)](https://gdcvault.com/play/1021761/Adaptive-Virtual-Texture-Rendering-in)

---

## 7. Async Texture Upload & Transfer Queues

### Vulkan Transfer Queue Pattern

```cpp
// Setup: find dedicated transfer queue family
// (VK_QUEUE_TRANSFER_BIT but NOT VK_QUEUE_GRAPHICS_BIT)
uint32_t transferFamily = findDedicatedTransferQueue(physicalDevice);

// Per-frame tile upload:
// 1. Map staging buffer, memcpy tile data
void* mapped;
vkMapMemory(device, stagingMemory, offset, tileSize, 0, &mapped);
memcpy(mapped, tileData, tileSize);
vkUnmapMemory(device, stagingMemory);

// 2. Record transfer commands on transfer queue
VkCommandBuffer transferCmd = allocateTransferCommandBuffer();
vkBeginCommandBuffer(transferCmd, &beginInfo);

// Transition image layout for transfer
VkImageMemoryBarrier barrier = {
    .oldLayout = VK_IMAGE_LAYOUT_SHADER_READ_ONLY_OPTIMAL,
    .newLayout = VK_IMAGE_LAYOUT_TRANSFER_DST_OPTIMAL,
    .srcQueueFamilyIndex = graphicsFamily,
    .dstQueueFamilyIndex = transferFamily,
    // ...
};
vkCmdPipelineBarrier(transferCmd, ...);

// Copy staging -> cache atlas at tile offset
VkBufferImageCopy region = {
    .bufferOffset = 0,
    .imageSubresource = { VK_IMAGE_ASPECT_COLOR_BIT, 0, 0, 1 },
    .imageOffset = { tileX * tileStride, tileY * tileStride, 0 },
    .imageExtent = { tileWidth, tileHeight, 1 }
};
vkCmdCopyBufferToImage(transferCmd, stagingBuf, cacheImage,
                        VK_IMAGE_LAYOUT_TRANSFER_DST_OPTIMAL, 1, &region);

// Release ownership back to graphics queue
// (graphics queue must do a matching acquire barrier)
vkEndCommandBuffer(transferCmd);

// 3. Submit with timeline semaphore for sync
VkSubmitInfo submitInfo = { /* ... signal transferDoneSemaphore */ };
vkQueueSubmit(transferQueue, 1, &submitInfo, VK_NULL_HANDLE);
```

### Key Considerations

- **Double-buffer staging**: ring buffer or pool of staging buffers to avoid stalls. Map persistently with `VK_MEMORY_PROPERTY_HOST_VISIBLE_BIT | VK_MEMORY_PROPERTY_HOST_COHERENT_BIT`.
- **Queue family ownership transfer**: required when transfer and graphics queues are different families. Two barriers needed -- release on transfer queue, acquire on graphics queue.
- **Timeline semaphores**: preferred over binary semaphores for multi-frame async tracking. Graphics queue waits on transfer semaphore value before using updated tiles.
- **Batch uploads**: accumulate multiple tile copies into one command buffer submission to amortize submission overhead.
- **Budget**: limit uploads to N tiles per frame. Monitor transfer queue completion via fence/timeline semaphore.

[NVIDIA vk_async_resources sample](https://github.com/nvpro-samples/vk_async_resources) |
[Uploading textures the right way (Erfan Ahmadi)](https://erfan-ahmadi.github.io/blog/Nabla/imageupload) |
[Vulkan staging buffer tutorial](https://docs.vulkan.org/tutorial/latest/04_Vertex_buffers/02_Staging_buffer.html) |
[GPU memory pools in D3D12](https://therealmjp.github.io/posts/gpu-memory-pool/) |
[wgpu transfer queue support (issue)](https://github.com/gfx-rs/wgpu/issues/5576)

### Vulkan Sparse Residency (Hardware Virtual Texturing)

Vulkan's `VK_IMAGE_CREATE_SPARSE_RESIDENCY_BIT` provides hardware-level virtual texturing:

- Create a large virtual image (e.g., 32768x32768)
- Bind memory only to pages that are needed via `vkQueueBindSparse()`
- Memory can be rebound dynamically throughout the image lifetime
- Pages aligned to hardware sparse block size (typically 64KB = one 256x256 BC7 tile)
- Unbound pages can return zero or undefined data (implementation-dependent)

**Caveat**: Sparse binding on NVIDIA can be "painfully slow" per reports -- `vkQueueBindSparse` may stall. Software virtual texturing (indirection texture approach) may perform better in practice.

[Vulkan sparse resources spec](https://docs.vulkan.org/spec/latest/chapters/sparsemem.html) |
[Vulkan sparse binding overview (Sawicki)](https://www.asawicki.info/news_1698_vulkan_sparse_binding_-_a_quick_overview) |
[Sascha Willems sparse residency example](https://github.com/SaschaWillems/Vulkan/blob/master/examples/texturesparseresidency/texturesparseresidency.cpp) |
[NVIDIA sparse binding slow (forum)](https://forums.developer.nvidia.com/t/sparse-texture-binding-is-painfully-slow/259105)

---

## 8. Recommended Architecture for a Procedural Planet App

### Design: Software SVT + Procedural Compute + Async Upload

```
                    CPU                              GPU
            +-------------------+          +----------------------+
            | Feedback Analysis |<---------|  Feedback Pass (1/16)|
            | (frame N-2)       |          |  tile IDs + mip      |
            +--------+----------+          +----------------------+
                     |
                     v
            +-------------------+          +----------------------+
            | Priority Queue    |          |  Compute Queue       |
            | (tiles to load)   |--------->|  Noise generation    |
            +--------+----------+          |  per-tile (256x256)  |
                     |                     +----------+-----------+
                     v                                |
            +-------------------+                     v
            | ISPCTexComp BC7   |          +----------------------+
            | (CPU compress)    |          |  Transfer Queue      |
            +--------+----------+          |  staging -> cache    |
                     |                     +----------+-----------+
                     v                                |
            +-------------------+                     v
            | Staging Buffer    |          +----------------------+
            | (ring buffer)     |--------->|  Cache Atlas 8Kx8K   |
            +-------------------+          |  + Indirection Tex   |
                                           +----------+-----------+
                                                      |
                                                      v
                                           +----------------------+
                                           |  Final Render Pass   |
                                           |  virtual UV -> phys  |
                                           +----------------------+
```

### Key Parameters

| Parameter | Recommended Value |
|-----------|-------------------|
| Tile size | 256x256 px (+ 4px border = 260x260) |
| Cache atlas | 8192x8192 (961 tile slots per layer) |
| Layers | 3 (diffuse, normal, roughness) |
| Cache VRAM | ~780 MB (BC7) or ~3 GB (uncompressed RGBA8) |
| Feedback resolution | Screen / 16 |
| Feedback latency | 2-3 frames |
| Tiles uploaded/frame | 4-16 |
| Compression | BC7 via ISPCTextureCompressor (CPU) |
| Staging buffer | 16-32 MB ring buffer |
| Disk cache | LZ4-compressed BC7 tiles |

### Fallback Strategy

1. Mip 0 (1 tile per face = 6 tiles) always resident -- guaranteed coverage
2. Mip 1-2 (24-96 tiles) pre-loaded at startup -- fast initial view
3. Higher mips loaded on demand via feedback
4. If cache full, evict LRU tiles, shader falls back to lower mip seamlessly

---

## Sources (consolidated)

### Virtual Texturing
- [Sparse Virtual Textures (Toni Sagrista)](https://tonisagrista.com/blog/2023/sparse-virtual-textures/)
- [SVT (silverspaceship)](https://silverspaceship.com/src/svt/)
- [How Virtual Textures Really Work](https://www.shlom.dev/articles/how-virtual-textures-really-work/)
- [SVT notes (Holger Dammertz)](https://holger.dammertz.org/stuff/notes_VirtualTexturing.html)
- [SVT implementation (Nathan Gaer)](https://studiopixl.com/2022-04-27/sparse-virtual-textures)
- [Virtual Texturing (PLAYERUNKNOWN Productions)](https://playerunknownproductions.net/news/virtual-texturing)
- [NVIDIA GTC 2010: Massive Texture Data](https://www.nvidia.com/content/GTC-2010/pdfs/2152_GTC2010.pdf)
- [Adaptive VT in Far Cry 4 (GDC)](https://gdcvault.com/play/1021761/Adaptive-Virtual-Texture-Rendering-in)
- [Tiled Resources / PRT (diary of a graphics programmer)](http://diaryofagraphicsprogrammer.blogspot.com/2013/07/tiled-resources-partially-resident.html)

### Planet / Terrain Rendering
- [Gaia Sky SVT docs](https://gaia.ari.uni-heidelberg.de/gaiasky/docs/master/Virtual-textures.html)
- [Gaia Sky SVT PR #695](https://codeberg.org/gaiasky/gaiasky/pulls/695)
- [Virtual texture tools](https://codeberg.org/langurmonkey/virtualtexture-tools)
- [Procedural planetary surfaces](https://tonisagrista.com/blog/2021/procedural-planetary-surfaces/)
- [Procedural planet rendering (Jad Khoury)](https://jadkhoury.github.io/terrain_blog.html)
- [Ellipsoidal Clipmaps (ScienceDirect)](https://www.sciencedirect.com/science/article/abs/pii/S0097849315000916)
- [Geometry Clipmaps (Hoppe)](https://hhoppe.com/geomclipmap.pdf)
- [Planetary rendering with mesh shaders (TU Wien)](https://www.cg.tuwien.ac.at/research/publications/2020/rumpelnik_martin_2020_PRM/rumpelnik_martin_2020_PRM-Thesis.pdf)
- [Procedural Virtual Texture with Unity Terrain](https://github.com/ACskyline/PVTUT)

### Texture Compression
- [BC7 format (Microsoft)](https://learn.microsoft.com/en-us/windows/win32/direct3d11/bc7-format)
- [BCn formats review (Nathan Reed)](https://www.reedbeta.com/blog/understanding-bcn-texture-compression-formats/)
- [Texture Compression in 2020 (Aras P.)](https://aras-p.info/blog/2020/12/08/Texture-Compression-in-2020/)
- [NVIDIA ASTC guide](https://developer.nvidia.com/astc-texture-compression-for-game-assets)
- [Betsy GPU compressor (Godot)](https://godotengine.org/article/betsy-gpu-texture-compressor/)
- [NVIDIA Texture Tools 3](https://developer.nvidia.com/gpu-accelerated-texture-compression)
- [ISPCTextureCompressor](https://github.com/GameTechDev/ISPCTextureCompressor)
- [bc7enc_rdo](https://github.com/richgel999/bc7enc_rdo)
- [Compressonator 4.0 (AMD)](https://gpuopen.com/learn/compressonator-4-0-utilize-the-power-of-gpu-based-encoding/)
- [Compressed GPU formats (Maister)](https://themaister.net/blog/2021/08/29/compressed-gpu-texture-formats-a-review-and-compute-shader-decoders-part-3-3/)

### Vulkan / DX12 Upload & Sparse
- [NVIDIA vk_async_resources](https://github.com/nvpro-samples/vk_async_resources)
- [Uploading textures the right way](https://erfan-ahmadi.github.io/blog/Nabla/imageupload)
- [Vulkan staging buffer tutorial](https://docs.vulkan.org/tutorial/latest/04_Vertex_buffers/02_Staging_buffer.html)
- [Vulkan sparse resources spec](https://docs.vulkan.org/spec/latest/chapters/sparsemem.html)
- [Vulkan sparse binding overview](https://www.asawicki.info/news_1698_vulkan_sparse_binding_-_a_quick_overview)
- [Sascha Willems sparse residency](https://github.com/SaschaWillems/Vulkan/blob/master/examples/texturesparseresidency/texturesparseresidency.cpp)
- [D3D12 Sampler Feedback Streaming (Intel)](https://github.com/GameTechDev/SamplerFeedbackStreaming)
- [GPU memory pools in D3D12](https://therealmjp.github.io/posts/gpu-memory-pool/)
- [bgfx SVT example](https://github.com/bkaradzic/bgfx/blob/master/examples/40-svt/svt.cpp)

### Procedural Generation
- [NVIDIA GPU Gems 3: Procedural Terrains](https://developer.nvidia.com/gpugems/gpugems3/part-i-geometry/chapter-1-generating-complex-procedural-terrains-using-gpu)
- [GPU Gems 2: Tile-Based Texture Mapping](https://developer.nvidia.com/gpugems/gpugems2/part-ii-shading-lighting-and-shadows/chapter-12-tile-based-texture-mapping)
- [Tileable procedural shaders](https://github.com/tuxalin/procedural-tileable-shaders)
- [Unity SVT docs](https://docs.unity3d.com/Manual/svt-how-it-works.html)
- [Godot SVT proposal](https://github.com/godotengine/godot-proposals/issues/1834)
