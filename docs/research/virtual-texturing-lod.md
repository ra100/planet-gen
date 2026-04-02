# Virtual Texturing & LOD Deep Dive for Procedural Planets

_Consolidated research -- 2026-04-02 | Merged from high-resolution-virtual-texturing.md, planetary-lod-systems.md, high-resolution-planet-textures.md_

> **Scope:** This document covers the algorithmic and architectural details of LOD systems and virtual texturing pipelines for planetary rendering. For the high-level architecture recommendation (cube sphere + quadtree LOD, 32K tiled generation scheme, memory budget table, tech stack) see `final.md` sections 9.2--9.5.

---

## Table of Contents

1. [Executive Summary](#1-executive-summary)
2. [LOD Algorithms](#2-lod-algorithms)
3. [Crack-Fixing and LOD Transitions](#3-crack-fixing-and-lod-transitions)
4. [Virtual Texturing Architecture](#4-virtual-texturing-architecture)
5. [Texture Compression](#5-texture-compression)
6. [Streaming Pipeline](#6-streaming-pipeline)
7. [Engine-Specific Implementations](#7-engine-specific-implementations)
8. [References](#8-references)

---

## 1. Executive Summary

Rendering a procedural planet from orbital view down to ground level requires two tightly coupled systems: a **geometry LOD** system that manages mesh complexity across 7+ orders of magnitude in viewing distance, and a **virtual texturing** system that streams texture data on demand from a logical address space far larger than physical VRAM.

**Key conclusions from research:**

- **CDLOD** (Filip Strugar 2010) is the best-documented quadtree LOD algorithm with built-in crack elimination via vertex morphing, public-domain reference code, and natural mapping to cube-sphere planets.
- **Geometry clipmaps** (Losasso/Hoppe 2004) offer an alternative with extremely low CPU cost but require adaptation for spherical surfaces.
- **GPU tessellation** works best as a refinement layer on top of a quadtree, not as the sole LOD mechanism (limited to 64x subdivision per patch).
- **Mesh shaders** (Turing+/RDNA 2+) are the future path, moving the entire LOD pipeline onto the GPU.
- **Software virtual texturing** (indirection texture + page cache) is preferred over hardware sparse residency due to cross-platform consistency and avoidance of driver performance cliffs.
- **Feedback-driven streaming** with 2--3 frame latency is the standard approach; DX12 sampler feedback can replace the manual feedback pass on supported hardware.

---

## 2. LOD Algorithms

### 2.1 CDLOD -- Continuous Distance-Dependent Level of Detail

Published by Filip Strugar (2010, Journal of Graphics, GPU, and Game Tools). The key innovation is a uniform LOD function based on precise 3D distance, with smooth geomorphing that eliminates cracks without any mesh stitching.

**Quadtree structure:**

- Each node covers a rectangular heightmap area and stores min/max height for bounding-box distance calculations.
- Each successive level renders 4x more triangles.
- Every node is rendered using the **same square grid mesh** (e.g., 33x33 or 65x65 vertices). The vertex shader transforms this single mesh to fit each node's area, position, and height. This saves GPU memory dramatically.
- Typical depth: 8--12 levels for planet-scale terrain.

**LOD range calculation:**

```
lodRanges[i] = minLodDistance * 2^i
```

Ranges increase exponentially by factor 2 to prevent nodes from spanning multiple LOD ranges.

**LOD selection algorithm (per frame):**

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

**Vertex morphing (crack elimination):**

Vertices morph continuously between LOD levels in the vertex shader. Morphing starts at 50% of the distance between adjacent LOD ranges:

```glsl
// morphFactor: 0 = about to split, 1 = about to merge
float computeMorphFactor(float distance, float lowRange, float highRange) {
    float factor = (distance - lowRange) / (highRange - lowRange);
    return clamp(factor / 0.5 - 1.0, 0.0, 1.0);
}

// Morph vertex position -- even-grid vertices snap to coarser-level equivalents
vec2 morphVertex(vec2 meshPos, float meshDim, float morphValue) {
    vec2 fraction = fract(meshPos * meshDim * 0.5) * 2.0 / meshDim;
    return meshPos - fraction * morphValue;
}
```

When the morph factor reaches 1.0, the fine mesh exactly matches the coarse mesh, eliminating T-junctions. Morphing occurs on the vertical (height) coordinate only.

**Planet adaptation (6 quadtrees on a cube sphere):**

```glsl
vec3 spherePos = normalize(cubePos) * planetRadius;
```

Heights sampled from a heightmap texture per-vertex. LOD ranges may need adjustment for curved horizon culling.

**Rendering characteristics:**

- ~6L + 5 draw calls for L levels
- Shader Model 3.0+ compatible
- Better screen-triangle distribution than clipmaps
- No stitching meshes required
- Reference implementation: [github.com/fstrugar/CDLOD](https://github.com/fstrugar/CDLOD) (DX9, public domain)
- Android/GLES 3.0 port: [github.com/sduenasg/terrain-sandbox](https://github.com/sduenasg/terrain-sandbox)

### 2.2 ROAM -- Real-time Optimally Adapting Meshes

Uses two priority queues to drive split and merge operations on bintree triangles (longest-edge bisection). Maintains continuous triangulations with triangle stripping, view frustum culling, and geomorphing in a single per-frame refinement pass. Historically important but largely superseded by quadtree-based approaches due to higher CPU cost.

### 2.3 Geometry Clipmaps (Losasso/Hoppe 2004)

Treats terrain as a 2D elevation image organized into a mipmap pyramid of L levels. Rather than storing the entire pyramid, the system caches **n x n sample windows** within each level -- nested regular grids centered about the viewer.

**Grid structure:**

- Grid size must be odd: n = 2^k - 1 (typically **n = 255**).
- At n = 255, triangles are approximately 5 pixels wide at 1024x768.
- Each clipmap ring is broken into **12 blocks** of m x m vertices where m = (n+1)/4 = 64.
- Vertex coordinates stored as SHORT2 (4 bytes per vertex); height fetched from texture.
- Only the finest level is a complete grid square; all others are hollow rings.

**Toroidal addressing (incremental update):**

As the viewer moves, clipmap windows translate within their pyramid levels using wraparound addressing. Only L-shaped strips of newly exposed data need updating:

```pseudocode
function updateClipmap(level, newCenter):
    offset = newCenter - level.oldCenter
    offset = round(offset / level.gridSpacing) * level.gridSpacing
    if offset == 0: return
    if offset.x != 0: updateStrip(level, horizontal, offset.x)
    if offset.y != 0: updateStrip(level, vertical, offset.y)
    level.oldCenter = newCenter
```

**Level transition blending:**

Smooth transitions use a blending parameter in the vertex shader across a transition region of width w = n/10:

```glsl
float alpha_x = clamp((abs(pos.x - viewer.x) - offset) / width, 0.0, 1.0);
float alpha_y = clamp((abs(pos.y - viewer.y) - offset) / width, 0.0, 1.0);
float alpha = max(alpha_x, alpha_y);
float z_blended = mix(z_fine, z_coarse, alpha);
```

**GPU data packing optimization:**

Two elevation values packed into a single 32-bit float -- integer part holds fine-level z_f, fractional part holds scaled (z_c - z_f). Enables blending with a **single texture lookup** instead of three.

**Upsampling algorithm:**

Tensor-product four-point subdivision with mask weights (-1/16, 9/16, 9/16, -1/16):

- Even-even positions: 1 texture lookup (direct copy)
- Odd-even/even-odd: 4 lookups (1D interpolation)
- Odd-odd positions: 16 lookups (2D interpolation)

**Performance (GeForce 6800 GT, 2005):**

| Metric                   | Value                    |
| ------------------------ | ------------------------ |
| Frame rate (n=255, L=11) | **130 fps**              |
| Triangle throughput      | **60M tri/sec**          |
| Frame rate (n=127)       | **298 fps**              |
| Draw calls per frame     | ~71 (with culling, L=11) |
| Upsampling               | 1.0 ms per 255x255 level |
| Decompression            | 8.0 ms per level         |
| Normal computation       | 0.6 ms per level         |
| US terrain (40 GB raw)   | 355 MB compressed        |

**Spherical adaptation -- Ellipsoidal Clipmaps (Shader & Stamminger 2015):**

Divides ellipsoidal surface into three partitions seamlessly stitched together. The grid is generated on the fly in the vertex shader rather than preloaded, guaranteeing sub-pixel precision of Earth's reference ellipsoid surface with constant memory footprint.

### 2.4 GPU Tessellation

The DX11/OpenGL 4.x/Vulkan tessellation pipeline consists of:

1. **Tessellation Control Shader (TCS)**: Sets per-edge tessellation levels
2. **Fixed-function Tessellator**: Subdivides patches
3. **Tessellation Evaluation Shader (TES)**: Positions generated vertices

Maximum hardware tessellation level: **64** (all APIs).

**LOD calculation -- screen-space sphere method (preferred):**

```glsl
float computeTessFactorSphere(vec3 v0, vec3 v1, mat4 viewProj) {
    vec3 center = (v0 + v1) * 0.5;
    float radius = distance(v0, v1) * 0.5;
    vec4 clipCenter = viewProj * vec4(center, 1.0);
    float screenDiameter = (radius * 2.0 * screenHeight) / clipCenter.w;
    float targetTriangleWidth = 8.0;  // pixels
    return clamp(screenDiameter / targetTriangleWidth, 1.0, 64.0);
}
```

This handles edge perpendicularity correctly -- an edge physically close to the camera but perpendicular to the view gets low tessellation due to its small screen footprint.

**NVIDIA adaptive terrain tessellation:**

```glsl
float screenSpaceTessFactor(vec4 p0, vec4 p1) {
    vec4 midPoint = 0.5 * (p0 + p1);
    float radius = distance(p0, p1) / 2.0;
    vec4 v0 = viewMatrix * midPoint;
    return clamp(diameter * screenSize / (fov * v0.z), 1.0, maxTessLevel);
}
```

View frustum culling is also performed in the TCS to skip off-screen patches entirely.

**Terrain-adaptive LOD (2021):** Tessellation factors adapt based on both camera distance and terrain roughness. Rougher terrain gets higher tessellation while smooth areas get less. Dynamic Stitching Strips (DSS) fill gaps between patches at different LOD levels.

**Key limitation:** Maximum factor of 64 means a single quad patch produces at most ~4096 triangles. For planet-scale detail, tessellation must be combined with a quadtree of coarse patches.

### 2.5 Mesh Shaders for Planetary Terrain

Mesh shaders (NVIDIA Turing+, AMD RDNA 2+, DX12 Ultimate / Vulkan) replace the traditional vertex+geometry+tessellation pipeline with a compute-like model that directly emits meshlets to the rasterizer.

**Pipeline architecture:**

```
Task Shader (optional)          Mesh Shader              Fragment Shader
- Frustum culling              - Generate vertices       - Shading
- LOD selection                - Generate primitives     - Texture sampling
- Occlusion culling            - Emit meshlet            - Virtual texture lookup
- Spawn mesh shaders           (max 256 verts,
                                max 256 primitives)
```

**Advantages over tessellation for terrain:**

- Task shader performs coarse-grained culling before any geometry is generated
- No fixed 64x tessellation limit
- Better mapping to modern GPU compute hardware
- Can replace two compute shaders with a single task shader

**Planetary rendering (Rumpelnik, TU Wien 2020):**

1. Six quadtrees (one per cube face) manage LOD
2. Task shader performs per-node frustum culling and LOD selection
3. Mesh shader generates terrain meshlets with displacement from heightmap
4. Vertices projected from cube to sphere in the mesh shader
5. Provides uniform terrain resolution in all directions
6. Avoids popping/swimming artifacts common in quadtree or clipmap methods

**LOD selection in task shader:**

```glsl
taskPayloadSharedEXT TaskPayload payload;

void main() {
    uint nodeIdx = gl_WorkGroupID.x;
    TerrainNode node = nodes[nodeIdx];

    if (!isVisible(node.bounds, viewProj)) return;  // emit 0 mesh shaders

    float screenError = node.geometricError * screenHeight /
                        (distance(cameraPos, node.center) * 2.0 * tan(fov * 0.5));

    if (screenError < errorThreshold) {
        payload.nodeIndex = nodeIdx;
        EmitMeshTasksEXT(node.meshletCount, 1, 1);
    }
}
```

**Meshlet organization for terrain:**

```pseudocode
// 16x16 vertex grid = 256 vertices, 15x15x2 = 450 triangles
// Split into sub-meshlets of 64 vertices, 126 triangles each
struct TerrainMeshlet {
    vec3 boundingSphere;
    float maxError;
    uint vertexOffset;
    uint vertexCount;    // max 256
    uint triangleOffset;
    uint triangleCount;  // max 256
};
```

### 2.6 View-Dependent Mesh Refinement and Screen-Space Error Metrics

**Fundamental screen-space error formula:**

```
rho = (e * screenHeight) / (2 * d * tan(fov / 2))

where:
    e = geometric error of the LOD level (meters)
    d = distance from camera to node center (meters)
    fov = vertical field of view (radians)
    screenHeight = viewport height in pixels
```

If rho exceeds a threshold (e.g., 1--4 pixels), the node must be refined. This is the standard metric used in Cesium, Google Earth, and virtual globe engines.

**SpaceEngine's simplified error metric:**

SpaceEngine replaced its pixel-size-based metric with:

```
error = distance_to_node_edge / node_size
```

Calculated in "unwarped" coordinates (before cube-to-sphere projection), this handles nodes at varying positions and cube corners uniformly without separate level-specific formulas.

SpaceEngine uses 6 quadtrees per planet (one per cube face), base resolution 256x256 textures with 33x33 vertex grids. "Virtual levels" separate geometry from texture resolution: geometry at level 9 uses height/normal maps from level 12. Maximum theoretical resolution: **1 terapixel** per cube face (~9.5 m/pixel at equator for Earth).

**Hoppe's view-dependent refinement (1997):**

Three criteria for progressive mesh refinement:

1. **View frustum**: Skip refinement for off-screen regions
2. **Surface orientation**: Reduce detail for back-facing regions
3. **Screen-space geometric error**: Projected error must be below threshold

Geomorphs (smooth vertex interpolation) eliminate popping: 1--4 pixels of geometric error can be nearly imperceptible.

### 2.7 Hybrid Approaches

**CPU quadtree + GPU tessellation (most common):**

1. CPU: coarse-grained quadtree generates patches, performs view frustum culling
2. GPU: tessellation shaders refine patches using displacement mapping
3. During tessellation, three factors considered: distance to camera, screen-space projection error, and **variance of height** (terrain roughness)

**Outerra's hybrid:**

- Chunked LOD quadtree subdivision for coarse terrain management
- GPU tessellation for fine-grained adaptive refinement
- Fractal noise computed per quadtree node for procedural detail
- Wavelet compression: 70 GB raw data -> 14 GB processed
- Logarithmic depth buffer for planetary-scale Z precision
- Quadrilateralized spherical cube projection

### 2.8 Algorithm Decision Matrix

| Criterion      | CDLOD                   | Chunked LOD            | Clipmaps              | GPU Tessellation          | Mesh Shaders          |
| -------------- | ----------------------- | ---------------------- | --------------------- | ------------------------- | --------------------- |
| Crack-free     | Morphing (excellent)    | Skirts (good)          | Blending (excellent)  | Edge matching (excellent) | Custom (varies)       |
| CPU cost       | Low (traverse)          | Low (traverse)         | Very low (shift)      | Very low                  | Very low              |
| GPU cost       | Low                     | Low                    | Low                   | Medium (tess stages)      | Medium                |
| Draw calls     | ~6L+5                   | ~nodes visible         | ~6L+5                 | ~patches visible          | ~meshlets             |
| Streaming      | Easy (quadtree = tiles) | Easy (chunks = tiles)  | Medium (ring updates) | Hard (no natural tiling)  | Medium                |
| Max detail     | Unlimited (add levels)  | Limited by chunk count | Limited by L levels   | Max 64x per patch         | Max 256 verts/meshlet |
| Spherical      | Natural (6 quadtrees)   | Natural (6 quadtrees)  | Requires adaptation   | Natural                   | Natural               |
| Hardware req   | SM 3.0+                 | SM 2.0+                | SM 3.0+               | SM 5.0+ (DX11+)           | DX12 Ultimate         |
| VT integration | Natural                 | Natural                | Natural               | Possible                  | Natural               |

---

## 3. Crack-Fixing and LOD Transitions

### 3.1 The T-Junction Problem

Anywhere two patches meet at different LOD levels, T-junctions create gaps. One side has a straight edge while the other has two edges with a vertex in the middle. The middle vertex rarely lies exactly on the straight edge, producing visible holes, shading discontinuities, and z-fighting.

### 3.2 Solution Taxonomy

| Technique                            | Complexity | Quality   | CPU Cost | GPU Cost            |
| ------------------------------------ | ---------- | --------- | -------- | ------------------- |
| **Vertex morphing** (CDLOD)          | Medium     | Excellent | Low      | Low (vertex shader) |
| **Skirts/flanges** (Chunked LOD)     | Low        | Good      | Very low | Low (extra tris)    |
| **Edge stitching strips**            | Medium     | Good      | Medium   | Low                 |
| **Tessellation edge matching**       | Medium     | Excellent | Low      | Medium (tess stage) |
| **Overlapping tiles** (No Man's Sky) | Low        | Good      | Very low | Low (overdraw)      |
| **Transition blending** (Clipmaps)   | Medium     | Excellent | Low      | Low (vertex shader) |
| **Dynamic Stitching Strips**         | Medium     | Good      | Medium   | Low                 |

### 3.3 Vertex Morphing (CDLOD)

Vertices smoothly morph between LOD levels based on distance. When a vertex reaches its LOD boundary, it has already morphed to exactly match the coarser level. No cracks ever appear because the transition is continuous. The morph parameter is the same for the entire node.

**Pros:** Zero visual artifacts, mathematically correct, no inter-node communication needed.
**Cons:** Requires vertex shader computation, slightly more complex LOD logic.

### 3.4 Skirts / Flanges (Chunked LOD)

Vertical strips of geometry extend below every edge of every chunk:

```pseudocode
skirtHeight = maxHeightError * 2  // conservative
for each edge_vertex V:
    V_below = V - surfaceNormal * skirtHeight
    emit triangle strip connecting V to V_below
```

For spherical terrain, skirts point toward the planet center, with maximum length = distance from terrain vertex to center minus minimum terrain radius.

**Pros:** Dead simple, no inter-chunk communication needed. Widely used in production engines.
**Cons:** Wastes triangles on invisible geometry, can be visible from extreme grazing angles.

### 3.5 Edge Vertex Snapping

Cracks are fixed by modifying triangle indices at edges so that higher-detail edge vertices snap to positions matching the lower-detail neighbor. Some vertices become unused. Requires knowing neighbor LOD at render time.

### 3.6 Overlapping Tiles

Add overlap between terrain tiles. Vertices, normals, and materials must be exactly the same for the overlapped region. Overlap can be applied to just 2 sides of a tile (like roof tiles). Reportedly used in No Man's Sky.

### 3.7 T-Junction Resolution with Degenerate Triangles

For each edge shared between a high-LOD and low-LOD chunk, insert degenerate triangles that weld the extra vertices of the high-LOD edge to the matching position on the low-LOD edge.

### 3.8 Tessellation Edge Matching

The tessellation control shader explicitly matches edge tessellation factors between adjacent patches:

1. For each patch, check 4 neighbors (N, S, E, W)
2. If a smaller patch borders a larger one, its shared edge gets a scale factor of 0.5
3. Clamp tessellation levels to powers of two
4. Use `fractional_even_spacing` in the TES to ensure symmetric vertex placement

```glsl
layout(quads, fractional_even_spacing, ccw) in;

void main() {
    vec2 uv = gl_TessCoord.xy;
    vec3 p = mix(
        mix(cp[0], cp[1], uv.x),
        mix(cp[3], cp[2], uv.x),
        uv.y
    );
    float height = texture(heightmap, computeHeightmapUV(p)).r;
    p = normalize(p) * (planetRadius + height);
    gl_Position = viewProj * vec4(p, 1.0);
}
```

### 3.9 Dynamic Stitching Strips (DSS)

A 2019 approach generates thin strips dynamically to bridge LOD gaps:

- Strips are generated between adjacent patches at different LODs
- Strip resolution matches the finer of the two patches
- Vertices interpolate between fine and coarse edge positions
- Provides smooth transitions with minimal overdraw

### 3.10 Transition Blending (Clipmap Approach)

A transition region at level boundaries blends between fine and coarse geometry:

```glsl
float alpha = smoothstep(levelExtent - transitionWidth, levelExtent, dist);
float height = mix(fineHeight, coarseHeight, alpha);
```

### 3.11 Logarithmic Depth Buffer

For planetary-scale rendering, Z-fighting is solved by a logarithmic depth buffer. Outerra's formula:

```glsl
// Vertex shader (OpenGL)
float logzbuf(vec4 xyzw, float invfarplanecoef) {
    return (log(1.0 + xyzw.w) * invfarplanecoef - 1.0) * xyzw.w;
}
// CPU: invfarplanecoef = 2.0 / log(farPlane + 1.0);
```

This handles near=0.1m to far=1e10m in a single pass. Floating-point precision becomes insufficient beyond ~100,000 meters from origin; mitigations include camera-relative rendering or double-precision CPU + single-precision GPU.

---

## 4. Virtual Texturing Architecture

### 4.1 Core Concept -- Software Virtual Texturing (SVT)

Virtual texturing decouples the _logical_ texture resolution from _physical_ GPU memory. The full planet texture is a "virtual" address space; only visible tiles are resident in a GPU-side cache. This is also known as Megatexture (id Software) or Sparse Virtual Textures.

### 4.2 Three-Pass Pipeline

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
     - Compress to BC7 (CPU ISPCTextureCompressor or GPU compute)
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

### 4.3 Indirection Texture (Page Table)

The indirection texture is a mipmapped texture where each pixel maps to one virtual tile:

```
indirection_texel = {
    physical_cache_x,   // tile column in cache atlas
    physical_cache_y,   // tile row in cache atlas
    mip_level_delta     // difference from requested to available mip
};
```

For a 32K virtual texture with 256px tiles: indirection texture = 128x64 pixels (at mip 0). Very small, always resident.

**Bit-packed page table encoding (32-bit integer):**

| Bits      | Content                                |
| --------- | -------------------------------------- |
| 0         | Residency flag (page present)          |
| 1--8      | Physical page X coordinate             |
| 9--16     | Physical page Y coordinate             |
| Remaining | LOD hints, eviction state, debug flags |

**Page table sizes for different virtual texture resolutions (128x128 page size):**

| Virtual Resolution | Pages Per Axis | Page Table Memory |
| ------------------ | -------------- | ----------------- |
| 8K x 8K            | 64             | 16 KB             |
| 16K x 16K          | 128            | 64 KB             |
| 32K x 32K          | 256            | 256 KB            |
| 64K x 64K          | 512            | 1 MB              |
| 128K x 128K        | 1024           | 4 MB              |

### 4.4 Mip Level Selection Shader

```glsl
float computeVirtualMip(vec2 uv, vec2 virtualSize) {
    vec2 dx = dFdx(uv) * virtualSize;
    vec2 dy = dFdy(uv) * virtualSize;
    float d = max(dot(dx, dx), dot(dy, dy));
    return floor(0.5 * log2(d));
}
```

### 4.5 Virtual-to-Physical Address Translation

```glsl
vec4 sampleVirtualTexture(vec2 virtualUV, sampler2D pageTable, sampler2D physicalAtlas) {
    float mip = computeVirtualMip(virtualUV, virtualTextureSize);

    // Scale UV to page grid at this mip level
    vec2 pageGridSize = max(virtualTextureSize / (pageSize * exp2(mip)), 1.0);
    vec2 pageIndex = floor(virtualUV * pageGridSize);
    vec2 inPageOffset = fract(virtualUV * pageGridSize);

    // Lookup page table for physical location
    vec4 pageEntry = texelFetch(pageTable, ivec2(pageIndex), int(mip));

    // Fall back to coarser mip if not resident
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

### 4.6 Feedback Buffer Rendering

```glsl
// Feedback pass fragment shader -- outputs page requests instead of color
void feedbackPass(vec2 virtualUV) {
    float mip = computeVirtualMip(virtualUV, virtualTextureSize);
    vec2 pageGridSize = max(virtualTextureSize / (pageSize * exp2(mip)), 1.0);
    vec2 pageIndex = floor(virtualUV * pageGridSize);
    gl_FragColor = vec4(pageIndex.x / 255.0, pageIndex.y / 255.0, mip / 15.0, 1.0);
}
```

**Feedback buffer sizing and strategies:**

- Render at 1/8 to 1/16 of screen resolution (e.g., 240x135 for 1080p)
- Stochastic jitter across frames catches pages missed by low-resolution sampling
- PlayerUnknown Productions uses 4x4 pixel block sampling with frame rotation
- Double/triple-buffer GPU-to-CPU readback to avoid pipeline stalls
- D3D12 **Sampler Feedback** provides hardware-level MinMip feedback maps that automatically record which mip levels were accessed, eliminating the need for a manual feedback pass (supported on AMD RDNA 2/3, NVIDIA Ampere+, Intel Alchemist+)

### 4.7 Physical Tile Cache (Page Cache)

A fixed-size texture atlas holding currently resident tiles:

| Atlas Resolution | Pages (256px + 4px border) | Memory (BC7) | Memory (RGBA8) |
| ---------------- | -------------------------- | ------------ | -------------- |
| 4096x4096        | 15x15 = 225                | ~64 MB       | ~256 MB        |
| 8192x8192        | 31x31 = 961                | ~256 MB      | ~1 GB          |

**LRU eviction strategy:**

1. Each page tracks last-used frame number
2. When atlas is full, evict least-recently-used page
3. Lowest mip pages are pinned (never evicted) to prevent sampling holes
4. 4 high-res pages can be replaced by 1 coarser page (75% savings) under memory pressure

### 4.8 Fallback Mip Chain Strategy

Always keep the lowest few mip levels fully resident (e.g., mip 7 = 256x128, trivially small). When a tile is not yet loaded, the shader falls back to the coarsest available mip via the `mip_level_delta` in the indirection texture. This prevents visual holes.

**Recommended fallback hierarchy:**

1. Mip 0 (1 tile per face = 6 tiles) always resident -- guaranteed coverage
2. Mip 1--2 (24--96 tiles) pre-loaded at startup -- fast initial view
3. Higher mips loaded on demand via feedback
4. If cache full, evict LRU tiles; shader falls back to lower mip seamlessly

### 4.9 Texture Atlas Layout Options

| Layout                       | Description                                                                                                                        | Trade-offs                                      |
| ---------------------------- | ---------------------------------------------------------------------------------------------------------------------------------- | ----------------------------------------------- |
| **2D Texture Atlas**         | One large texture, tiles packed in grid. 4px border per tile for bilinear filtering (256+8=264px per slot). Wastes ~6% on borders. | Simplest; works everywhere                      |
| **Texture Array**            | Each slice = one tile. Avoids border issues within a tile. Limited by maxImageArrayLayers (typically 2048).                        | Clean filtering; limited capacity               |
| **Texture Array of Atlases** | Each slice is an atlas page holding multiple tiles.                                                                                | Best balance of flexibility and hardware limits |

### 4.10 Cube Map Tile Hierarchy (Quadtree)

```
Level 0: 6 tiles (one per face)
Level 1: 24 tiles (4 per face)
Level 2: 96 tiles
...
Level N: 6 * 4^N tiles
```

For 32K equivalent resolution with 256px tiles: level ~7 (6 \* 4^7 = 98,304 tiles). Only a fraction resident at any time.

### 4.11 Sparse Residency -- Hardware Virtual Texturing

Vulkan `VK_IMAGE_CREATE_SPARSE_RESIDENCY_BIT` and DX12 Reserved Resources provide hardware-level virtual texturing:

- Create a large virtual image (e.g., 32768x32768)
- Bind memory only to pages that are needed via `vkQueueBindSparse()` or `UpdateTileMappings()`
- Memory can be rebound dynamically; pages aligned to hardware sparse block size (typically 64 KB = one 256x256 BC7 tile)
- Unbound pages can return zero or undefined data (implementation-dependent)
- DX12 Tier 2+ supports partial mip residency

**Caveat:** Sparse binding on NVIDIA can be "painfully slow" -- `vkQueueBindSparse` may stall. A 2025 HAL Science paper ("The Sad State of Hardware Virtual Textures") documents that sparse texture binding causes frame rate stuttering on some drivers. **Software virtual texturing (indirection texture approach) is preferred** for cross-platform determinism and avoiding driver-specific performance cliffs.

### 4.12 MegaTexture (id Tech 5/6) -- Historical Context

The pioneering virtual texturing system for RAGE (2011): a single unique texture covering the entire game world up to 128,000 x 128,000 pixels, stored on disk as ~20 GB. Subdivided into 128x128 or 256x256 pages, streamed via feedback pass, with a page table mapping virtual UVs to physical atlas locations. Abandoned in id Tech 7 due to high disk space requirements, texture pop-in, and pipeline complexity.

### 4.13 Runtime Compute Generation vs Pre-Baked

**Runtime compute generation:**

- Generate tiles as RGBA8 on GPU compute, compress to BC7 on CPU (ISPCTextureCompressor) or GPU compute shader (~0.25 ms for 256x256)
- Budget: 4--16 tiles per frame at 256x256 to keep frame time stable
- Priority queue ordered by screen-space error (most-visible tiles first)
- Cache generated tiles to disk (LZ4/zstd compressed) to avoid regeneration

**Pre-baked (offline):**

- Generate full mip chain during offline bake
- Store all mip levels in KTX2/DDS format
- Stream individual mip levels independently at load time (lower mips first)
- Total mip chain adds 33% of base level size

**Procedural mip levels (noise anti-aliasing):**

An alternative to downsampling -- generate each mip level directly with band-limited noise:

- At mip level N, reduce the number of noise octaves by N (octaves above the Nyquist frequency would alias)
- A 32K base with 12 octaves of FBM: mip 0 = 12 octaves, mip 1 = 11, ..., mip 11 = 1
- Theoretically correct (no aliasing), practical for virtual texturing where each tile has a specific mip level

---

## 5. Texture Compression

### 5.1 Format Overview

| Format       | bpp  | Block      | Channels          | Platform       | Best For                           |
| ------------ | ---- | ---------- | ----------------- | -------------- | ---------------------------------- |
| **BC1**      | 4    | 4x4 -> 8B  | RGB + 1-bit A     | PC/Console     | Low-quality albedo, cutouts        |
| **BC4**      | 4    | 4x4 -> 8B  | R (8 values)      | PC/Console     | Height, roughness (single channel) |
| **BC5**      | 8    | 4x4 -> 16B | RG (2x BC4)       | PC/Console     | Tangent-space normal maps (RG)     |
| **BC6H**     | 8    | 4x4 -> 16B | RGB HDR (float16) | PC/Console     | HDR environment maps               |
| **BC7**      | 8    | 4x4 -> 16B | RGBA (8-bit)      | PC/Console     | High-quality albedo RGBA           |
| **ASTC 4x4** | 8    | 4x4 -> 16B | 1--4 ch           | Mobile/Some PC | Universal high-quality             |
| **ASTC 6x6** | 3.56 | 6x6 -> 16B | 1--4 ch           | Mobile         | Balanced quality/size              |
| **ASTC 8x8** | 2    | 8x8 -> 16B | 1--4 ch           | Mobile         | Aggressive compression             |

All BCn and ASTC formats decompress in hardware at full texture fetch rate.

### 5.2 BC7 Deep Dive

- **Block size**: 4x4 texels, 128 bits (16 bytes) per block
- **Quality**: ~42+ dB PSNR; "no perceptible difference from uncompressed originals at normal viewing distances"
- **Compression ratio**: 4:1 vs RGBA8
- **8 modes** with complex endpoint selection; brute-force encoding is too slow for real-time
- **Platform**: DX11+, Vulkan, OpenGL 4.2+

### 5.3 BC7 RDO (Rate-Distortion Optimized) Compression

**bc7enc_rdo** by Rich Geldreich: rate-distortion optimized BC7 encoder that maximizes quality per bit. Produces the highest-quality BC7 output at the cost of longer encode times. Best suited for offline baking or disk-cached tiles.

### 5.4 ASTC Block Size Selection

ASTC is unique in offering variable block sizes (4x4 through 12x12), all producing 128-bit output:

| Block Size | bpp  | Quality vs BC7 (4x4)       | Use Case                         |
| ---------- | ---- | -------------------------- | -------------------------------- |
| 4x4        | 8.0  | Comparable (~42 dB)        | When quality matches BC7 needed  |
| 5x5        | 5.12 | Slightly lower             | Good middle ground               |
| 6x6        | 3.56 | Surpasses BC1 at lower bpp | Balanced quality/size on mobile  |
| 8x8        | 2.0  | Lower                      | Aggressive VRAM budgets          |
| 10x10      | 1.28 | Noticeably lower           | Extreme compression              |
| 12x12      | 0.89 | Lowest                     | Thumbnails, very distant terrain |

ASTC 6x6 at 3.56 bpp surpasses BC1 at 4 bpp by ~1.5 dB despite using 10% fewer bits.

### 5.5 Per-Map-Type Recommendations

| Map Type      | Recommended Format   | Rationale                                              |
| ------------- | -------------------- | ------------------------------------------------------ |
| Albedo (RGBA) | BC7 / ASTC 4x4       | Highest quality for color, handles alpha               |
| Height (R)    | BC4 / ASTC 4x4 (1ch) | 8 gradient values per block; excellent for smooth data |
| Normal (RG)   | BC5 / ASTC 4x4 (2ch) | Two independent channels, reconstruct Z in shader      |
| Roughness (R) | BC4 / ASTC 4x4 (1ch) | Single channel, 4 bpp sufficient                       |

### 5.6 GPU Real-Time Compression

For procedural textures generated at runtime, compression must happen on the GPU or via fast CPU SIMD.

**GPU encoders:**

| Encoder                           | Formats                        | Speed                            | Quality                         |
| --------------------------------- | ------------------------------ | -------------------------------- | ------------------------------- |
| **Betsy** (Godot)                 | BC1/3/4/5/6, ETC1/2, EAC       | ~12x vs CPU ETC2                 | Good (no BC7)                   |
| **NVIDIA Texture Tools 3**        | BC7 (CUDA)                     | Fast (not real-time)             | High                            |
| **ISPCTextureCompressor** (Intel) | BC7 (CPU ISPC/SIMD)            | Competitive with low-quality GPU | Very high                       |
| **bc7enc_rdo**                    | BC7 (CPU)                      | Slower                           | Maximum (RDO)                   |
| **Compressonator 4.2** (AMD)      | BCn (DirectX Compute / OpenCL) | 38% faster than v4.0             | High (+0.6 dB over v4.0)        |
| **Tellusim SDK**                  | BC1/7, ASTC                    | BC7: 1.0 ms (1024x512 on M1 Max) | ~44.85 dB (GPU) vs ~48.27 (CPU) |
| **bc7e-on-gpu** (Aras P.)         | BC7                            | Real-time compute shader         | Good                            |

GPU encoding is **28--105x faster** than CPU but with a 3--4 dB quality penalty (Tellusim benchmarks).

**Practical pipeline for procedural planets:**

1. Generate tile in compute shader (256x256 RGBA8)
2. Compress via GPU BC7 compute shader (~0.25 ms for 256x256)
3. Copy compressed tile (32 KB for BC4, 64 KB for BC7) to staging buffer
4. Stream to disk cache (LZ4/zstd) for future loads

### 5.7 Mipmap Generation for Procedural Textures

**Compute shader approach (recommended):**

The `nvpro_pyramid` library demonstrates a cache-aware compute shader generating all mip levels in fewer dispatches than the naive blit approach, using register shuffles within GPU subgroups. Performance on RTX 3090 for sRGBA8:

| Resolution  | Compute Shader | Blit Method | Speedup |
| ----------- | -------------- | ----------- | ------- |
| 4096 x 4096 | 113 us         | 161 us      | 1.43x   |
| 2048 x 2048 | 36 us          | 63 us       | 1.75x   |

Advantages: works on compute-only queues, eliminates per-level sync barriers, supports custom reduction kernels.

**Per-tile mipmap generation:** Each tile must generate its own mip chain. The border overlap region ensures correct filtering at tile edges. For a 256x256 tile: mip levels 0--8, adding 33% memory overhead.

---

## 6. Streaming Pipeline

### 6.1 Vulkan Transfer Queue Pattern

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
vkEndCommandBuffer(transferCmd);

// 3. Submit with timeline semaphore for sync
VkSubmitInfo submitInfo = { /* ... signal transferDoneSemaphore */ };
vkQueueSubmit(transferQueue, 1, &submitInfo, VK_NULL_HANDLE);
```

### 6.2 Key Transfer Queue Considerations

- **Double-buffer staging**: Ring buffer or pool of staging buffers to avoid stalls. Map persistently with `VK_MEMORY_PROPERTY_HOST_VISIBLE_BIT | VK_MEMORY_PROPERTY_HOST_COHERENT_BIT`.
- **Queue family ownership transfer**: Required when transfer and graphics queues are different families. Two barriers needed -- release on transfer queue, acquire on graphics queue.
- **Timeline semaphores**: Preferred over binary semaphores for multi-frame async tracking. Graphics queue waits on transfer semaphore value before using updated tiles.
- **Batch uploads**: Accumulate multiple tile copies into one command buffer submission to amortize overhead.
- **Budget**: Limit uploads to N tiles per frame. Monitor transfer queue completion via fence/timeline semaphore.
- **Alignment**: Respect hardware `optimalBufferCopyRowPitchAlignment` and `minImageTransferGranularity`.

### 6.3 Streaming Budget Calculations

**PCIe bandwidth:**

- PCIe 3.0 x16: ~12 GB/s
- PCIe 4.0 x16: ~25 GB/s theoretical, ~12--15 GB/s practical for texture uploads
- A single 32K x 32K RGBA8 (4 GB) would take ~0.27--0.33s at full PCIe 4.0 bandwidth

**Per-frame streaming budget (practical):**

- 4--16 tiles/frame at 256px BC7 = 0.5--2 MB/frame
- At 60fps: ~30--120 MB/s sustained transfer rate
- SSD read: ~3--7 GB/s (NVMe), negligible latency for tile loads

**Staging buffer design:**

- Allocate 16--32 MB fixed staging buffer in `DEVICE_LOCAL | HOST_VISIBLE` memory
- Use range allocator for sub-allocations within the pool
- When staging memory fills, submit current batch and reuse

**DX12 DirectStorage:** Bypasses CPU for disk-to-GPU transfers entirely on supported platforms.

### 6.4 Recommended Streaming Parameters

| Parameter            | Recommended Value                                  |
| -------------------- | -------------------------------------------------- |
| Tile size            | 256x256 px (+ 4px border = 260x260)                |
| Cache atlas          | 8192x8192 (961 tile slots per layer)               |
| Layers               | 3--4 (diffuse, normal, roughness, height)          |
| Cache VRAM           | ~780 MB (BC7) or ~3 GB (uncompressed RGBA8)        |
| Feedback resolution  | Screen / 16                                        |
| Feedback latency     | 2--3 frames                                        |
| Tiles uploaded/frame | 4--16                                              |
| Compression          | BC7 via GPU compute or ISPCTextureCompressor (CPU) |
| Staging buffer       | 16--32 MB ring buffer                              |
| Disk cache           | LZ4/zstd-compressed BC7 tiles in KTX2/DDS          |

### 6.5 Async Texture Upload Architecture Diagram

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
        | BC7 Compress      |          +----------------------+
        | (CPU or GPU)      |          |  Transfer Queue      |
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

### 6.6 Gaia Sky SVT Implementation (Reference)

Gaia Sky's virtual texture system uses a quadtree where:

- Root (level 0) covers the whole area
- Each level has 4x the tiles of the level above, each covering 4x less area
- All tiles are the same pixel resolution (e.g., 512x512)
- Aspect ratio n:1 supported (n root tiles stacking horizontally)
- Tile files stored as `tx_[col]_[row].ext` (JPG or PNG)
- With 31 quadtree levels, each tile covers ~1 cm on Earth's surface

---

## 7. Engine-Specific Implementations

### 7.1 Unreal Engine 5

**Landscape system:**

- LOD handled via texture mipmaps with `tex2Dlod` HLSL instruction
- Vertex shader interpolates between mip levels for smooth morphing
- Heightmaps up to 8192x8192 legitimately supported

**Runtime Virtual Texturing (RVT):**

- Generates texture pages on the GPU at runtime (ideal for procedural content)
- Feedback buffer identifies needed virtual pages
- Pages rendered into physical page cache
- Base Color stored as RGB compressed to BC1; YCoCg variant adds 25% memory for higher quality
- Nanite + RVT decouples geometry LOD from texture detail

**Streaming Virtual Texturing (SVT):**

- Replaces traditional texture streaming with page-based streaming from disk
- Reduces peak VRAM usage for large texture sets

### 7.2 Unity

**Terrain system:**

- `SparseTexture` class wraps DX12/Vulkan sparse resources
- SVT documentation describes feedback-driven page streaming

**Planetary Terrain (mathis-s):**

- Dynamic quadtree-based LOD for spherical terrain
- Six quads initially forming a cube, vertices distorted to sphere
- Segments replaced by 4 sub-segments when camera is close, merged when far

**Orbis (DOTS-based):**

- Quadtree LOD from space to ground level
- Burst-compiled Jobs for async background processing
- Each terrain chunk stored as standalone ECS entity
- Cache-optimized data structures with MeshData API
- Floating origin system for infinite worlds

### 7.3 SpaceEngine (Custom C++ Engine)

- 6 quadtrees per planet, max depth ~12 for Earth-sized bodies
- Base: 256x256 textures, 33x33 vertex grids per node
- "Virtual levels" where geometry resolution lags texture resolution by 8x
- Geometry at level 9 uses height/normal maps from level 12
- Max theoretical resolution: 1 terapixel per face (~9.5 m/pixel)
- Replaced pixel-size error metric with simpler `distance_to_node_edge / node_size`
- Eliminated ancestor node partial rendering (obsolete on modern GPUs)

### 7.4 Outerra (Custom C++ Engine)

- Chunked LOD + GPU tessellation hybrid
- Quadrilateralized spherical cube projection (variant of WGS84)
- Wavelet-compressed real elevation data (70 GB -> 14 GB)
- Fractal procedural detail to centimeter scale
- Logarithmic depth buffer (near=0.1 m to far=1e10 m)
- Terrain and grass tessellated adaptively (no fragment shader depth writes)

### 7.5 PlayerUnknown Productions (Modern VT Pipeline)

State-of-the-art implementation achieving **70--80% less texture memory on GPU**:

- Stochastic feedback: 1 pixel per 4x4 block with frame rotation
- DX12 Reserved Resources for format-agnostic partial residency
- DX12 Sampler Feedback for hardware-captured tile access patterns
- DirectStorage for CPU-bypassing disk-to-GPU transfer

### 7.6 id Software MegaTexture (Historical)

- id Tech 5/6 (RAGE, Wolfenstein): single unique texture up to 128K x 128K
- ~20 GB on-disk compressed texture data
- 128x128 or 256x256 pages, feedback-driven streaming
- Abandoned in id Tech 7 due to disk bloat and pop-in artifacts

---

## 8. References

### LOD Algorithms -- Core Papers

- Strugar, F. (2010). "Continuous Distance-Dependent Level of Detail for Rendering Heightmaps." [Paper](https://aggrobird.com/files/cdlod_latest.pdf) | [GitHub](https://github.com/fstrugar/CDLOD)
- Ulrich, T. (2002). "Rendering Massive Terrains Using Chunked Level of Detail Control." [Paper](https://tulrich.com/geekstuff/sig-notes.pdf)
- Losasso, F. & Hoppe, H. (2004). "Geometry Clipmaps: Terrain Rendering Using Nested Regular Grids." [Paper](https://hhoppe.com/geomclipmap.pdf) | [Project](https://hhoppe.com/proj/geomclipmap/)
- Asirvatham, A. & Hoppe, H. (2005). "Terrain Rendering Using GPU-Based Geometry Clipmaps." [GPU Gems 2, Ch.2](https://developer.nvidia.com/gpugems/gpugems2/part-i-geometric-complexity/chapter-2-terrain-rendering-using-gpu-based-geometry)
- Hoppe, H. (1997). "View-Dependent Refinement of Progressive Meshes." [Paper](https://hhoppe.com/svdlod.pdf)
- Cozzi, P. & Ring, K. (2011). "3D Engine Design for Virtual Globes." [Book site](https://virtualglobebook.com/)
- Rumpelnik, M. (2020). "Planetary Rendering with Mesh Shaders." [TU Wien Thesis](https://www.cg.tuwien.ac.at/research/publications/2020/rumpelnik_martin_2020_PRM/rumpelnik_martin_2020_PRM-Thesis.pdf)

### LOD Algorithms -- Implementations and Guides

- [CDLOD Terrain Implementation -- svnte.se](https://svnte.se/cdlod-terrain)
- [Tessellated Terrain with Dynamic LOD -- Victor Bush](https://victorbush.com/2015/01/tessellated-terrain/)
- [NVIDIA Adaptive Terrain Tessellation](https://docs.nvidia.com/gameworks/content/gameworkslibrary/graphicssamples/opengl_samples/terraintessellationsample.htm)
- [Planetary Scale LOD Terrain -- Leif Node](https://leifnode.com/2014/04/planetary-scale-lod-terrain-generation/)
- [terrain-sandbox (Android CDLOD)](https://github.com/sduenasg/terrain-sandbox)
- [Ellipsoidal Clipmaps -- Computers & Graphics 2015](https://www.sciencedirect.com/science/article/abs/pii/S0097849315000916)
- [Using Mesh Shaders for CLOD Terrain -- SIGGRAPH 2020](https://dl.acm.org/doi/10.1145/3388767.3407391)
- [Terrain-Adaptive LOD with GPU Tessellation -- ScienceDirect 2021](https://www.sciencedirect.com/science/article/pii/S1110016821000326)
- [Multilevel Terrain with Dynamic Stitching Strips -- MDPI 2019](https://www.mdpi.com/2220-9964/8/6/255)
- [Comparison of Spherical Cube Map Projections -- Dimitrijevic & Lambers](https://docslib.org/doc/860041/comparison-of-spherical-cube-map-projections-used-in-planet-sized-terrain-rendering)

### Virtual Texturing

- [Sparse Virtual Textures -- Toni Sagrista](https://tonisagrista.com/blog/2023/sparse-virtual-textures/)
- [How Virtual Textures Really Work -- shlom.dev](https://www.shlom.dev/articles/how-virtual-textures-really-work/)
- [SVT Implementation Notes -- Holger Dammertz](https://holger.dammertz.org/stuff/notes_VirtualTexturing.html)
- [SVT Implementation -- Nathan Gaer / Studio Pixl](https://studiopixl.com/2022-04-27/sparse-virtual-textures)
- [Virtual Texturing -- PlayerUnknown Productions](https://playerunknownproductions.net/news/virtual-texturing)
- [Adaptive VT in Far Cry 4 -- GDC](https://gdcvault.com/play/1021761/Adaptive-Virtual-Texture-Rendering-in)
- [NVIDIA GTC 2010: Massive Texture Data](https://www.nvidia.com/content/GTC-2010/pdfs/2152_GTC2010.pdf)
- [D3D12 Sampler Feedback Streaming -- Intel](https://github.com/GameTechDev/SamplerFeedbackStreaming)
- [Unity SVT Documentation](https://docs.unity3d.com/Manual/svt-how-it-works.html)
- [Gaia Sky SVT Docs](https://gaia.ari.uni-heidelberg.de/gaiasky/docs/master/Virtual-textures.html)
- [Gaia Sky SVT PR #695](https://codeberg.org/gaiasky/gaiasky/pulls/695)
- [Virtual Texture Tools (langurmonkey)](https://codeberg.org/langurmonkey/virtualtexture-tools)
- [Procedural VT with Unity Terrain](https://github.com/ACskyline/PVTUT)
- [bgfx SVT Example](https://github.com/bkaradzic/bgfx/blob/master/examples/40-svt/svt.cpp)
- [The Sad State of Hardware Virtual Textures -- HAL Science 2025](https://hal.science/hal-05138369/file/The_Sad_State_of_Hardware_Virtual_Textures.pdf)

### Texture Compression

- [BC7 Format Specification -- Microsoft](https://learn.microsoft.com/en-us/windows/win32/direct3d11/bc7-format)
- [Understanding BCn Texture Compression -- Nathan Reed](https://www.reedbeta.com/blog/understanding-bcn-texture-compression-formats/)
- [Texture Compression in 2020 -- Aras P.](https://aras-p.info/blog/2020/12/08/Texture-Compression-in-2020/)
- [NVIDIA ASTC Guide](https://developer.nvidia.com/astc-texture-compression-for-game-assets)
- [ASTC Format Overview -- ARM](https://github.com/ARM-software/astc-encoder/blob/main/Docs/FormatOverview.md)
- [Betsy GPU Compressor -- Godot](https://godotengine.org/article/betsy-gpu-texture-compressor/)
- [NVIDIA Texture Tools 3](https://developer.nvidia.com/gpu-accelerated-texture-compression)
- [ISPCTextureCompressor -- Intel](https://github.com/GameTechDev/ISPCTextureCompressor)
- [bc7enc_rdo -- Rich Geldreich](https://github.com/richgel999/bc7enc_rdo)
- [bc7e-on-gpu -- Aras P.](https://github.com/aras-p/bc7e-on-gpu)
- [Compressonator 4.2 -- AMD GPUOpen](https://gpuopen.com/learn/compressonator-4-2/)
- [GPU Texture Encoder -- Tellusim](https://tellusim.com/gpu-encoder/)
- [Compressed GPU Formats Review -- Maister](https://themaister.net/blog/2021/08/29/compressed-gpu-texture-formats-a-review-and-compute-shader-decoders-part-3-3/)

### Vulkan / DX12 Upload & Sparse Resources

- [NVIDIA vk_async_resources Sample](https://github.com/nvpro-samples/vk_async_resources)
- [Uploading Textures the Right Way -- Erfan Ahmadi](https://erfan-ahmadi.github.io/blog/Nabla/imageupload)
- [Vulkan Staging Buffer Tutorial](https://docs.vulkan.org/tutorial/latest/04_Vertex_buffers/02_Staging_buffer.html)
- [Vulkan Sparse Resources Spec](https://docs.vulkan.org/spec/latest/chapters/sparsemem.html)
- [Vulkan Sparse Binding Overview -- Sawicki](https://www.asawicki.info/news_1698_vulkan_sparse_binding_-_a_quick_overview)
- [Sascha Willems Sparse Residency Example](https://github.com/SaschaWillems/Vulkan/blob/master/examples/texturesparseresidency/texturesparseresidency.cpp)
- [GPU Memory Pools in D3D12 -- MJP](https://therealmjp.github.io/posts/gpu-memory-pool/)
- [wgpu Transfer Queue Support (issue)](https://github.com/gfx-rs/wgpu/issues/5576)
- [vk_compute_mipmaps -- NVIDIA](https://github.com/nvpro-samples/vk_compute_mipmaps)

### Procedural Generation

- [NVIDIA GPU Gems 3: Procedural Terrains](https://developer.nvidia.com/gpugems/gpugems3/part-i-geometry/chapter-1-generating-complex-procedural-terrains-using-gpu)
- [GPU Gems 2: Tile-Based Texture Mapping](https://developer.nvidia.com/gpugems/gpugems2/part-ii-shading-lighting-and-shadows/chapter-12-tile-based-texture-mapping)
- [Procedural Planetary Surfaces -- Toni Sagrista](https://tonisagrista.com/blog/2021/procedural-planetary-surfaces/)
- [Procedural Planet Rendering -- Jad Khoury](https://jadkhoury.github.io/terrain_blog.html)
- [Tileable Procedural Shaders](https://github.com/tuxalin/procedural-tileable-shaders)

### Engine Implementations

- [SpaceEngine Terrain Engine Upgrade #3](https://spaceengine.org/news/blog171120/)
- [Outerra Blog: Logarithmic Depth Buffer](https://outerra.blogspot.com/2009/08/logarithmic-z-buffer.html)
- [Outerra: A Seamless Planet Engine](https://www.gamedeveloper.com/business/-i-outerra-i-a-seamless-planet-rendering-engine)
- [UE Runtime Virtual Texturing](https://dev.epicgames.com/documentation/en-us/unreal-engine/runtime-virtual-texturing-in-unreal-engine)
- [Unity Planetary Terrain -- mathis-s](https://github.com/mathis-s/PlanetaryTerrain)
- [Orbis DOTS Terrain](https://orbis-terrains.netlify.app/)
- [Making Worlds: Of Spheres and Cubes -- Acko.net](https://acko.net/blog/making-worlds-1-of-spheres-and-cubes/)
- [Terrain LOD Papers Archive -- VTerrain.org](http://vterrain.org/LOD/Papers/)
- [Terrain LOD on Spherical Grids -- VTerrain.org](http://vterrain.org/LOD/spherical.html)
