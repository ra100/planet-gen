# Level-of-Detail (LOD) Systems for Planetary Rendering

_Deep research -- 2026-03-28_

---

## 1. Quadtree-Based Sphere LOD -- CDLOD (Filip Strugar)

### Overview

CDLOD (Continuous Distance-Dependent Level of Detail) was published by Filip Strugar in 2010 in the Journal of Graphics, GPU, and Game Tools. It uses a quadtree of regular grids with GPU-based rendering and smooth geomorphing. The key innovation is that the LOD function is uniform across the whole mesh and based on precise 3D distance between observer and terrain. [Source](https://aggrobird.com/files/cdlod_latest.pdf) [GitHub](https://github.com/fstrugar/CDLOD)

### Quadtree Structure

- Each node has four child nodes and stores **min/max height values** for its rectangular heightmap area.
- Each successive LOD level renders 4x as many triangles and contains 4x more nodes than its parent.
- The tree has constant depth, predetermined by memory and granularity requirements.
- **Every terrain node is rendered using the same square grid mesh** to save GPU memory. The vertex shader transforms this single mesh to fit each node's requirements (positioning, heightmapping, spacing, morphing). [Source](https://svnte.se/cdlod-terrain)

### LOD Selection Algorithm

Distance-based ranges determine which nodes to render. The ranges increase by powers of two:

```
lodRanges[i] = minLodDistance * pow(2, i)
```

Selection traverses from root downward, rejecting nodes outside their LOD distance range or camera frustum. Nodes spanning multiple LOD ranges have their children evaluated instead. [Source](https://svnte.se/cdlod-terrain)

### Morphing (Crack Elimination)

Vertices morph toward lower-detail positions near LOD boundaries. The morph value ranges from 0.0 (halfway between LOD distances) to 1.0 (at the higher LOD distance):

```glsl
// morphFactor: 0 = about to split, 1 = about to merge
float factor = (distance - lodRanges[level]) / (lodRanges[level+1] - lodRanges[level]);
float morphValue = clamp(factor / 0.5 - 1.0, 0.0, 1.0);
```

The morph parameter is the same for the whole node. Morphing occurs in the vertex shader on the vertical (height) coordinate only, eliminating geometry cracks **without any mesh stitching**. [Source](https://svnte.se/cdlod-terrain)

### Sphere/Planet Application

The planet is represented as a **cube composed of 6 separate CDLOD quadtrees**. The grid mesh is modified in the vertex shader to fit each node, and the cube is spherized via normalization of the vertex position:

```glsl
vec3 spherePos = normalize(cubePos) * planetRadius;
```

Heights are sampled from a heightmap texture per-vertex during rendering. [Source](https://svnte.se/cdlod-terrain) [Source](https://aggrobird.com/files/cdlod_latest.pdf)

### Advantages

- Better screen-triangle distribution than fixed LOD approaches.
- Clean transitions between levels -- no stitching required.
- Single grid mesh reused for all nodes (low GPU memory).
- Simple vertex shader implementation.

---

## 2. Chunked LOD for Planets

### Overview

Chunked LOD, originally presented by Thatcher Ulrich (2002), pre-generates terrain mesh chunks at multiple resolutions and selects chunks from a quadtree based on screen-space error. [Source](https://tulrich.com/geekstuff/sig-notes.pdf)

### Chunk Generation

The algorithm works from a heightmap, generating a quadtree of meshes:
- Root: very coarse representation of the complete terrain.
- Each node divides into 4 children of the same base patch size (e.g., 256x256) with correspondingly higher detail.
- Chunks can be precomputed and stored on disk, enabling streaming. [Source](https://www.gamedev.net/forums/topic/485584-chunked-lod-with-procedural-planets/)

### Crack-Fixing Techniques

When adjacent chunks have different LOD levels, cracks appear at boundaries. Multiple solutions exist:

**1. Skirts / Flanges**
A row of quads points inward under the terrain surface at chunk boundaries. At a LOD mismatch, a small skirt section may be visible but the surface appears closed with no holes. This is the simplest approach and used widely in production engines. [Source](https://community.khronos.org/t/chunked-lod-cracks/72110)

**2. Edge Vertex Snapping (Index Buffer Modification)**
Cracks are fixed by modifying triangle indices at edges so that higher-detail edge vertices snap to positions matching the lower-detail neighbor. Some vertices become unused. [Source](https://www.gamedev.net/forums/topic/713470-terrain-lod-and-cracks/)

**3. Overlapping Tiles**
Add overlap between terrain tiles. Vertices, normals, and materials must be exactly the same for the overlapped region. Overlap can be applied to just 2 sides of a tile (like roof tiles). Used in some planet renderers and reportedly in No Man's Sky. [Source](https://www.gamedev.net/forums/topic/485584-chunked-lod-with-procedural-planets/)

**4. T-Junction Resolution**
For each edge shared between a high-LOD and low-LOD chunk, insert degenerate triangles that weld the extra vertices of the high-LOD edge to the matching position on the low-LOD edge.

### Planet-Scale Considerations

- Floating-point precision becomes insufficient beyond ~100,000 meters from origin. Mitigations include camera-relative rendering or double-precision CPU + single-precision GPU. [Source](https://leifnode.com/2014/04/planetary-scale-lod-terrain-generation/)
- **Logarithmic depth buffer** solves Z-fighting across planetary scales. Outerra's formula:

```glsl
// Vertex shader (OpenGL)
float logzbuf(vec4 xyzw, float invfarplanecoef) {
    return (log(1.0 + xyzw.w) * invfarplanecoef - 1.0) * xyzw.w;
}
// CPU: invfarplanecoef = 2.0 / log(farPlane + 1.0);
```
[Source](https://outerra.blogspot.com/2009/08/logarithmic-z-buffer.html)

---

## 3. Clipmap-Based Approaches (Losasso/Hoppe)

### Original Geometry Clipmaps (2004)

Losasso and Hoppe (SIGGRAPH 2004) treat terrain as a 2D elevation image organized into a mipmap pyramid of L levels. Rather than storing the entire pyramid, the system caches **n x n sample windows** within each level -- "nested regular grids centered about the viewer." [Source](https://hhoppe.com/geomclipmap.pdf) [Source](https://hhoppe.com/proj/geomclipmap/)

### Grid Structure

- Grid size must be odd: n = 2^k - 1 (typically **n = 255**).
- At n = 255, triangles are approximately 5 pixels wide in a 1024x768 window.
- Each clipmap ring is broken into 12 m x m blocks (m = (n+1)/4, typically 64x64 vertices).
- Vertex coordinates stored as SHORT2 (4 bytes per vertex); height fetched from texture. [Source](https://developer.nvidia.com/gpugems/gpugems2/part-i-geometric-complexity/chapter-2-terrain-rendering-using-gpu-based-geometry)

### Level Transitions (Blending)

Smooth transitions use a blending parameter in the vertex shader:

```glsl
float alpha_x = clamp((abs(pos.x - viewer.x) - offset) / width, 0.0, 1.0);
float alpha_y = clamp((abs(pos.y - viewer.y) - offset) / width, 0.0, 1.0);
float alpha = max(alpha_x, alpha_y);
float z_blended = mix(z_fine, z_coarse, alpha);
```

[Source](https://developer.nvidia.com/gpugems/gpugems2/part-i-geometric-complexity/chapter-2-terrain-rendering-using-gpu-based-geometry)

### GPU Implementation

**Data packing**: Two elevation values packed into a single 32-bit float:
- Integer part: fine-level elevation z_f
- Fractional part: scaled difference (z_c - z_f) with range [-256, 256] mapped to [0, 1]

This enables blending with a **single texture lookup** instead of three.

**Update algorithm** (toroidal addressing as viewer moves):
1. **Upsampling**: 4-point interpolatory subdivision (16 texture lookups per sample)
2. **Residual addition**: From compressed data or synthesized noise
3. **Normal map computation**: Cross products of grid-aligned tangent vectors

[Source](https://developer.nvidia.com/gpugems/gpugems2/part-i-geometric-complexity/chapter-2-terrain-rendering-using-gpu-based-geometry)

### Performance Numbers (GeForce 6800 GT)

| Metric | Value |
|--------|-------|
| Frame rate (n=255, L=11) | **130 fps** |
| Triangle throughput | **60M tri/sec** |
| Frame rate (n=127) | **298 fps** |
| Draw calls per frame | ~71 (with culling, L=11) |
| View frustum culling gain | 2-3x for 90-degree FOV |
| US terrain (40GB raw) | **355 MB** compressed in memory |

**Update times for full 255x255 level:**

| Operation | Time |
|-----------|------|
| Upsampling | 1.0 ms |
| Decompression | 8.0 ms |
| Synthesis | ~0 ms |
| Normal computation | 0.6 ms |

[Source](https://developer.nvidia.com/gpugems/gpugems2/part-i-geometric-complexity/chapter-2-terrain-rendering-using-gpu-based-geometry)

### Spherical Adaptation -- Ellipsoidal Clipmaps

Ellipsoidal clipmaps extend geometry clipmaps to an ellipsoidal surface, dividing it into three partitions seamlessly stitched together. The ellipsoidal grid is generated on the fly in the vertex shader rather than preloaded. [Source](https://www.sciencedirect.com/science/article/abs/pii/S0097849315000916)

---

## 4. GPU Tessellation Shaders for Planetary Terrain

### Hardware Tessellation Pipeline

The DX11/OpenGL 4.x/Vulkan tessellation pipeline consists of:
1. **Tessellation Control Shader (TCS)**: Sets per-edge tessellation levels
2. **Fixed-function Tessellator**: Subdivides patches
3. **Tessellation Evaluation Shader (TES)**: Positions generated vertices

Maximum hardware tessellation level: **64** (OpenGL). [Source](https://victorbush.com/2015/01/tessellated-terrain/)

### LOD Calculation -- Camera Distance Method

```
1. Project both edge vertices into camera space
2. Average their distances from camera
3. Map distance to tessellation level using min/max bounds:
   tessLevel = mix(maxTess, minTess,
       clamp((avgDist - minDist) / (maxDist - minDist), 0, 1))
4. Set inner levels as average of outer levels
```

[Source](https://victorbush.com/2015/01/tessellated-terrain/)

### LOD Calculation -- Screen-Space Sphere Method

```
1. Fit a sphere around each patch edge
2. Project sphere into screen space
3. Calculate projected diameter in pixels
4. Compare to target triangle width:
   tessLevel = projectedDiameter / targetTriangleWidth
```

This handles edge perpendicularity correctly -- an edge physically close to the camera but perpendicular to the view gets low tessellation (small screen footprint). [Source](https://victorbush.com/2015/01/tessellated-terrain/)

### NVIDIA Adaptive Terrain Tessellation

NVIDIA's implementation determines tessellation level based on the projected screen size of a sphere fitted to each patch edge:

```glsl
// In tessellation control shader:
float screenSpaceTessFactor(vec4 p0, vec4 p1) {
    vec4 midPoint = 0.5 * (p0 + p1);
    float radius = distance(p0, p1) / 2.0;
    vec4 v0 = viewMatrix * midPoint;
    // Project sphere diameter to pixels
    return clamp(diameter * screenSize / (fov * v0.z), 1.0, maxTessLevel);
}
```

View frustum culling is also performed in the TCS to skip off-screen patches entirely. [Source](https://docs.nvidia.com/gameworks/content/gameworkslibrary/graphicssamples/opengl_samples/terraintessellationsample.htm)

### Planetary Application (Springer 2017)

A method of Earth terrain tessellation on the GPU constructs a polygonal terrain model using triangle patches of different LOD on graphics cards with programmable tessellation, specifically targeting space simulator applications. [Source](https://link.springer.com/article/10.1134/S0361768817040065)

### Mesh Shaders for Planetary Terrain (Rumpelnik, TU Wien 2020)

Rumpelnik's thesis proposes using NVIDIA Turing mesh shaders instead of the tessellation pipeline. Rectangular regions of cells around the viewer are submitted to the mesh shader geometry pipeline, enabling efficient LOD decisions on the GPU. The approach provides uniform terrain resolution in all directions and avoids popping/swimming artifacts common in quadtree or clipmap methods. [Source](https://www.cg.tuwien.ac.at/research/publications/2020/rumpelnik_martin_2020_PRM/)

Mesh shaders offer more flexibility than hardware tessellation (which is limited to fixed tessellation patterns) and better threading than geometry shaders. [Source](https://dl.acm.org/doi/fullHtml/10.1145/3388767.3407391)

---

## 5. View-Dependent Mesh Refinement and Screen-Space Error Metrics

### Screen-Space Error Formula

The fundamental LOD selection metric. Given a node with **geometric error** e (world-space maximum deviation from ground truth), the projected screen-space error rho in pixels is:

```
rho = (e * screenHeight) / (2 * d * tan(fov / 2))

where:
    e = geometric error of the LOD level (meters)
    d = distance from camera to node center (meters)
    fov = vertical field of view (radians)
    screenHeight = viewport height in pixels
```

If rho exceeds a threshold (e.g., 1-4 pixels), the node must be refined (subdivided). This is the standard metric used in Cesium, Google Earth, and virtual globe engines. [Source](https://virtualglobebook.com/3DEngineDesignForVirtualGlobesSection121.pdf) [Source](https://graemephi.github.io/posts/calculating-lod/)

### SpaceEngine's Error Metric

SpaceEngine replaced its pixel-size-based metric with a simpler distance-based calculation:

```
error = distance_to_node_edge / node_size
```

Calculated in "unwarped" coordinates (before cube-to-sphere projection), this handles nodes at varying positions and cube corners uniformly without separate level-specific formulas. [Source](https://spaceengine.org/news/blog171120/)

SpaceEngine uses 6 quadtrees (one per cube face), base resolution 256x256 textures with 33x33 vertex grids. Maximum theoretical resolution: **1 terapixel** per cube face (256 * 2^12 = 1,048,576 pixels), ~9.5 m/pixel at equator for Earth. [Source](https://spaceengine.org/news/blog171120/)

### Hoppe's View-Dependent Refinement

Hoppe (1997) introduced view-dependent refinement of progressive meshes using three criteria:
1. **View frustum**: Skip refinement for off-screen regions
2. **Surface orientation**: Reduce detail for back-facing regions
3. **Screen-space geometric error**: The projected error must be below threshold

Geomorphs (smooth vertex interpolation) eliminate popping: models can have 1-4 pixels of geometric error while remaining nearly imperceptible. [Source](https://hhoppe.com/svdlod.pdf)

### ROAM (Real-time Optimally Adapting Meshes)

Uses two priority queues to drive split and merge operations on bintree triangles (longest-edge bisection). Maintains continuous triangulations with triangle stripping, view frustum culling, and geomorphing in a single per-frame refinement pass. [Source](https://www.semanticscholar.org/paper/Terrain-Simplification-Simplified:-A-General-for-Lindstrom-Pascucci/05933aaea4b2f1e9235ec99ae5509698f07a97ec)

---

## 6. Hybrid Approaches Combining Multiple LOD Strategies

### CPU Quadtree + GPU Tessellation (Most Common Hybrid)

The most effective hybrid combines both:

1. **CPU stage**: Coarse-grained quadtree generates terrain patches, performs view frustum culling
2. **GPU stage**: Tessellation shaders refine patches using displacement mapping

During tessellation, three factors are considered:
- Distance to camera
- Screen-space projection error
- **Variance of height** (terrain roughness standard)

This leverages CPU for hierarchical spatial organization and culling, GPU for dynamic fine-grained detail. [Source](https://www.sciencedirect.com/science/article/pii/S1110016821000326)

### Performance Comparison: Quadtree vs Tessellation (Lindqvist 2023)

A bachelor thesis directly compared quadtree and tessellation LOD for planetary terrain:

- **Quadtree solution**: Uses 6 quadtrees to construct planetary mesh on CPU with higher detail closer to the viewer.
- **Tessellation solution**: GPU subdivides a basic low-resolution model to achieve higher detail.

Both implement adaptive LOD on a spherical shape, but differ in where the work happens (CPU vs GPU) and the granularity of control. [Source](https://www.diva-portal.org/smash/record.jsf?pid=diva2:1812239)

### Outerra's Approach

Outerra combines:
- **Chunked LOD** quadtree subdivision for coarse terrain management
- **GPU tessellation** for fine-grained adaptive refinement
- **Fractal noise** computed per quadtree node for procedural detail
- **Wavelet compression**: 70GB raw data -> 14GB processed dataset
- **Logarithmic depth buffer** for planetary-scale Z precision
- **Quadrilateralized spherical cube** projection (variant of WGS84)

The terrain and grass are tessellated adaptively so they don't need fragment shader depth writes (only objects do). [Source](https://outerra.blogspot.com/2009/08/logarithmic-z-buffer.html) [Source](https://www.gamedeveloper.com/business/-i-outerra-i-a-seamless-planet-rendering-engine)

### Virtual Texturing + LOD

Modern approaches combine geometric LOD with Runtime Virtual Texturing (RVT):
- Geometric LOD manages mesh complexity
- Virtual texturing manages texture resolution independently
- Allows decoupling of geometry and texture detail levels
- Unreal Engine 5 implements this via Nanite + RVT for landscape. [Source](https://dev.epicgames.com/documentation/en-us/unreal-engine/runtime-virtual-texturing-in-unreal-engine)

---

## 7. Engine-Specific Implementations

### Unreal Engine

**Landscape System**:
- LOD handled via texture mipmaps with `tex2Dlod` HLSL instruction
- Vertex shader interpolates between mip levels for smooth morphing
- Heightmaps up to 8192x8192 legitimately supported
- Runtime Virtual Texturing creates shading data on demand at the correct resolution
- Standard texture streaming system loads/unloads mipmaps as needed

[Source](https://docs.unrealengine.com/4.26/en-US/BuildingWorlds/Landscape) [Source](https://dev.epicgames.com/documentation/en-us/unreal-engine/runtime-virtual-texturing-in-unreal-engine)

### Unity

**Planetary Terrain (mathis-s)**:
- Dynamic quadtree-based LOD for spherical terrain
- Six quads initially forming a cube, vertices distorted to sphere
- Segments replaced by 4 sub-segments when camera is close, merged back when far
- [GitHub](https://github.com/mathis-s/PlanetaryTerrain)

**Orbis (DOTS-based)**:
- Quadtree LOD from space to ground level
- Burst-compiled Jobs for async background processing
- Each terrain chunk stored as standalone ECS entity
- Cache-optimized data structures with MeshData API
- Supports both heightmap and runtime procedural generation
- Floating origin system for infinite worlds
- [Source](https://orbis-terrains.netlify.app/)

**Quadtree LOD Planet with Jobs (OmerBilget)**:
- Uses Unity Jobs System for parallel mesh generation
- Cubemap heightmap textures for terrain data
- [GitHub](https://github.com/OmerBilget/Quadtree-LOD-Planet-Generation-Unity)

### SpaceEngine (Custom C++ Engine)

- 6 quadtrees per planet (cube-sphere), max depth ~12 for Earth-sized bodies
- Base: 256x256 textures, 33x33 vertex grids per node
- "Virtual levels" where geometry resolution lags texture resolution by 8x
- Geometry at level 9 uses height/normal maps from level 12
- Max theoretical resolution: 1 terapixel per face (~9.5 m/pixel)
- Eliminated ancestor node partial rendering (obsolete on modern GPUs)
- [Source](https://spaceengine.org/news/blog171120/)

### Outerra (Custom C++ Engine)

- Chunked LOD + GPU tessellation hybrid
- Quadrilateralized spherical cube projection
- Wavelet-compressed real elevation data (70GB -> 14GB)
- Fractal procedural detail to centimeter scale
- Logarithmic depth buffer (near=0.1m to far=1e10m)
- [Source](https://www.outerra.com/) [Blog](https://outerra.blogspot.com/)

### Custom/Research Implementations

**Leif Node (OpenGL)**:
- Dynamic quadtree traversed every frame
- LOD distances: 50m * 2^level (50, 100, 200, 400...)
- fBm with 3D simplex noise for height
- Single plane mesh with 4 index buffers per quadrant
- ~200 fps on GTX 480
- [Source](https://leifnode.com/2014/04/planetary-scale-lod-terrain-generation/)

**terrain-sandbox (Android/OpenGL ES 3.0)**:
- CDLOD on mobile
- [GitHub](https://github.com/sduenasg/terrain-sandbox)

---

## 8. Performance Summary

| System | Triangle Rate | Frame Rate | Memory | Notes |
|--------|-------------|------------|--------|-------|
| Geometry Clipmaps (Hoppe) | 60M tri/sec | 130 fps (n=255) | 355 MB (US terrain) | GeForce 6800 GT, 2005 |
| Geometry Clipmaps (Hoppe) | -- | 298 fps (n=127) | -- | Smaller grid |
| Leif Node quadtree | -- | ~200 fps | -- | GTX 480, 2014 |
| Diamond hierarchies | 70M tri/sec | 40+ Hz | -- | Out-of-core, 100M+ triangles |
| Memory-efficient tessellation | -- | 570 fps (UHD) | 28 KB per patch | 2.5M triangles, 2022 |
| SpaceEngine | -- | -- | -- | 1 terapixel/face theoretical |

### Typical Modern Budget (Real-time, 60fps target)

- **Terrain triangles per frame**: 500K - 2M (typical), up to 5-10M on high-end GPUs
- **Draw calls for terrain**: 50-200 per frame
- **Texture memory for terrain**: 256 MB - 1 GB (with virtual texturing)
- **Heightmap update time**: 1-10 ms per frame

---

## 9. Algorithm Decision Matrix

| Criterion | CDLOD | Chunked LOD | Clipmaps | GPU Tessellation | Hybrid |
|-----------|-------|-------------|----------|-----------------|--------|
| Implementation complexity | Medium | Low-Medium | High | Medium | High |
| CPU load | Medium | Medium-High | Low | Low | Medium |
| GPU load | Low-Medium | Low | Medium | High | Medium-High |
| Crack handling | Morphing (free) | Skirts/stitching | Blending (free) | Edge matching | Mixed |
| Streaming support | Good | Excellent | Good | Limited | Excellent |
| Procedural generation | Good | Good | Medium | Excellent | Excellent |
| Visual quality | High | Medium-High | High | Very High | Very High |
| Popping artifacts | None (geomorph) | Possible | None (blend) | None (continuous) | None |
| Suited for planets | Yes (6 faces) | Yes (6 faces) | Needs adaptation | Yes (with quadtree) | Yes |

---

## 10. Key References

- Strugar, F. (2010). "Continuous Distance-Dependent Level of Detail for Rendering Heightmaps." [Paper](https://aggrobird.com/files/cdlod_latest.pdf) | [GitHub](https://github.com/fstrugar/CDLOD)
- Ulrich, T. (2002). "Rendering Massive Terrains Using Chunked Level of Detail Control." [Paper](https://tulrich.com/geekstuff/sig-notes.pdf)
- Losasso, F. & Hoppe, H. (2004). "Geometry Clipmaps: Terrain Rendering Using Nested Regular Grids." [Paper](https://hhoppe.com/geomclipmap.pdf) | [Project](https://hhoppe.com/proj/geomclipmap/)
- Asirvatham, A. & Hoppe, H. (2005). "Terrain Rendering Using GPU-Based Geometry Clipmaps." [GPU Gems 2, Ch.2](https://developer.nvidia.com/gpugems/gpugems2/part-i-geometric-complexity/chapter-2-terrain-rendering-using-gpu-based-geometry)
- Hoppe, H. (1997). "View-Dependent Refinement of Progressive Meshes." [Paper](https://hhoppe.com/svdlod.pdf)
- Cozzi, P. & Ring, K. (2011). "3D Engine Design for Virtual Globes." [Book site](https://virtualglobebook.com/)
- Rumpelnik, M. (2020). "Planetary Rendering with Mesh Shaders." [TU Wien](https://www.cg.tuwien.ac.at/research/publications/2020/rumpelnik_martin_2020_PRM/)
- Outerra Blog. "Logarithmic Depth Buffer." [Post](https://outerra.blogspot.com/2009/08/logarithmic-z-buffer.html)
- SpaceEngine Blog. "Terrain Engine Upgrade #3." [Post](https://spaceengine.org/news/blog171120/)
- Victor Bush. "Tessellated Terrain Rendering with Dynamic LOD." [Post](https://victorbush.com/2015/01/tessellated-terrain/)
- Leif Node. "Planetary Scale LOD Terrain Generation." [Post](https://leifnode.com/2014/04/planetary-scale-lod-terrain-generation/)
- Orbis DOTS Terrain Rendering. [Site](https://orbis-terrains.netlify.app/)
- vterrain.org. "Terrain LOD Published Papers." [Index](http://vterrain.org/LOD/Papers/)
- vterrain.org. "Terrain LOD on Spherical Grids." [Index](http://vterrain.org/LOD/spherical.html)
