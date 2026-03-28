# Procedural Planet Generation — Technical & CG Research

_Research compiled 2026-03-28 via web search, web fetch, and source analysis_

---

## 1. GPU Noise Generation

### 1.1 Noise Algorithms for Terrain

| Algorithm | Type | GPU Suitability | Best For |
|-----------|------|-----------------|----------|
| Perlin | Gradient | Excellent (GPU Gems Ch.5) | General terrain |
| Simplex | Gradient | Excellent | Natural terrain, fewer artifacts than Perlin |
| OpenSimplex2 | Gradient | Excellent | Avoids directional artifacts of Simplex |
| Worley/Voronoi | Cellular | Good | Crack patterns, continental boundaries, cell structures |
| Gabor | Band-pass | Moderate | Anisotropic features, detailed textures |
| Value | Value | Good | Fast but lower quality |

**Source:** [NVIDIA GPU Gems Ch.5 — Improved Perlin Noise](https://developer.nvidia.com/gpugems/gpugems/part-i-natural-effects/chapter-5-implementing-improved-perlin-noise), [Reddit: Fast terrain generation on GPU via compute](https://www.reddit.com/r/proceduralgeneration/comments/tisdxi/fast_terrain_generation_on_the_gpu_via_compute/)

### 1.2 Fractal Brownian Motion (fBm)

Multi-octave noise is the foundation of procedural terrain. Each octave adds higher frequency, lower amplitude detail:

```glsl
// fBm pseudocode
const int N_OCTAVES = 8;
float frequency = 2.5;
float amplitude = 0.5;
float lacunarity = 2.0;  // frequency multiplier per octave
float persistence = 0.5;  // amplitude multiplier per octave

float n = 0.0;
for (int i = 0; i < N_OCTAVES; i++) {
    n += amplitude * noise(frequency * position);
    frequency *= lacunarity;
    amplitude *= persistence;
}
```

**Key parameters:**
- **Lacunarity** (2.0 typical): controls frequency gap between octaves
- **Persistence** (0.5 typical): controls roughness. Lower = smoother terrain
- **Octaves** (5-12): more octaves = finer detail but more GPU cost
- Spectral exponent β = -log₂(persistence). Earth terrain: β ≈ 2.0

**Source:** [Gaia Sky — Procedural Planetary Surfaces](https://tonisagrista.com/blog/2021/procedural-planetary-surfaces/)

### 1.3 Advanced Noise Techniques

**Domain Warping:** Feed noise output back as input offset for organic, flowing patterns:
```glsl
vec2 q = vec2(fbm(position + vec2(0.0, 0.0)), fbm(position + vec2(5.2, 1.3)));
vec2 r = vec2(fbm(position + 4.0*q + vec2(1.7, 9.2)), fbm(position + 4.0*q + vec2(8.3, 2.8)));
float f = fbm(position + 4.0*r);
```

**Ridge Noise:** |noise| inverted for sharp ridgelines:
```glsl
float ridge = 1.0 - abs(noise(p));
ridge = ridge * ridge; // sharpen
```

**Thermal Weathering:** Erode steep slopes by distributing height to neighbors. GPU-friendly as a cellular automaton pass.

**Source:** [Gaia Sky procedural surfaces](https://tonisagrista.com/blog/2021/procedural-planetary-surfaces/), [acko.net Making Worlds](https://acko.net/blog/making-worlds-1-of-spheres-and-cubes/)

---

## 2. GPU Hydraulic Erosion Simulation

### 2.1 Virtual Pipe Model (Mei/Xiao)

The standard GPU erosion approach stores per-cell water height, sediment, and flow in 4 textures:

1. **Water Increment** — add rain/springs to water height map
2. **Flow Simulation** — virtual pipes between cells, calculate outflow based on height difference:
   ```
   Δh = (h_left + w_left) - (h_center + w_center)
   flow = max(0, flow_prev + Δt * A * g * Δh / pipe_length)
   ```
3. **Erosion/Deposition** — based on water velocity and carrying capacity:
   ```
   capacity = Kc * |velocity| * (maxSediment - currentSediment)
   if capacity > sediment: erode, else deposit
   ```
4. **Sediment Transport** — advect sediment with water velocity
5. **Evaporation** — reduce water by Ke fraction per step

All 5 steps are independent per cell → perfect GPU parallelism.

**Performance:** 2048×2048 heightmap, 50 iterations: ~100ms on RTX 3080 (compute shader)

**Sources:**
- [Fast Hydraulic Erosion Simulation and Visualization on GPU (Mei et al.)](https://www.researchgate.net/publication/4295561_Fast_Hydraulic_Erosion_Simulation_and_Visualization_on_GPU)
- [Interactive Terrain Modeling Using Hydraulic Erosion (Stava et al., Purdue)](https://www.cs.purdue.edu/cgvlab/www/resources/papers/Stava-2008-Interactive_Terrain_Modeling_Using_Hydraulic_Erosion.pdf)
- [CESCG 2011 — Hydraulic Erosion on GPU (Jako)](https://old.cescg.org/CESCG-2011/papers/TUBudapest-Jako-Balazs.pdf)
- [Unity GPU Erosion (GitHub)](https://github.com/bshishov/UnityTerrainErosionGPU)

### 2.2 Thermal Erosion

Simpler than hydraulic — just talus angle slope threshold:
```
if (slope > talus_angle) move_material_to_lower_neighbor
```
Single compute dispatch per iteration. Usually 10-50 iterations sufficient.

---

## 3. Sphere Parameterization & Projection

### 3.1 Cube-to-Sphere Mappings

| Method | Formula | Area Distortion | Use Case |
|--------|---------|-----------------|----------|
| Normalized Cube | `p_sphere = normalize(p_cube)` | ±33% | Simple, fast, most common |
| Tangent Space | Adjusted normalization | ±22% | Better distribution |
| HEALPix | Equal-area, ring/nest scheme | 0% | Scientific, astronomy |
| Cool 80-style | Polynomial correction | ~±10% | Better than normalized |

**Normalized cube map (most common for planet rendering):**
```glsl
vec3 cubeToSphere(vec3 p) {
    return normalize(p); // GPU native cubemap sampling
}
```

**Pros:** Trivial to compute, GPU cubemap hardware support, quadtree LOD per face  
**Cons:** 33% area distortion at corners, non-uniform texel density

**Source:** [Making Worlds 1: Of Spheres and Cubes (acko.net)](https://acko.net/blog/making-worlds-1-of-spheres-and-cubes/), [Grivicki & Vrto G&C 2005](http://adsabs.harvard.edu/cgi-bin/nph-bib_query?bibcode=2005ApJ...622..759G)

### 3.2 Icosphere

Subdivide icosahedron → project vertices to sphere. More uniform triangle distribution but:
- No natural quadtree LOD
- Harder to UV map
- More complex neighbor queries

### 3.3 Seamless Noise on Sphere

**Problem:** Standard noise on (θ, φ) creates seam at φ=2π and pole pinching.

**Solutions:**
1. **3D noise sampled on sphere surface:** `noise(x,y,z)` where `(x,y,z)` is sphere point — naturally seamless
2. **Cube map noise:** Generate noise on each cube face with continuity at edges
3. **Blended cube faces:** Overlap faces and blend

3D noise sampling is simplest and avoids all seam issues at cost of one extra noise dimension.

**Source:** [Three.js procedural planet mesh generator](https://discourse.threejs.org/t/procedural-planet-mesh-generator-gpgpu/69389), [Medium: GPGPU Procedural Planet Meshes](https://medium.com/fractions/gpgpu-on-the-web-procedural-planet-meshes-0601b044c818)

---

## 4. LOD Systems for Planetary Scale

### 4.1 Quadtree Chunked LOD

The standard approach for planet LOD:

- Start with cube sphere (6 faces)
- Each face = root of a quadtree
- Subdivide nodes near camera → higher detail chunks
- Merge nodes far from camera → lower detail

**Chunk properties:**
- Each chunk is a fixed-size grid (e.g., 33×33 or 65×65 vertices)
- All chunks same vertex count → uniform GPU draw cost
- Chunks are independently paged in/out of GPU memory
- "Skirts" hide seams between LOD levels

**LOD selection metric:**
```
screen_space_error = (geometric_error / distance_to_camera) * viewport_height
if screen_space_error > threshold: subdivide
```

**Source:** [Tulrich Chunked LOD (SIG notes)](http://tulrich.com/geekstuff/sig-notes.pdf), [acko.net Making Worlds](https://acko.net/blog/making-worlds-1-of-spheres-and-cubes/)

### 4.2 Geometry Clipmaps

Alternative: concentric rings of regular grids centered on viewer. GPU-friendly as uniform-sized vertex buffers. Better cache coherence. Used in some terrain engines.

### 4.3 GPU-Driven LOD (Modern)

Recent approach (Far Cry 5 inspired): GPU manages entire quadtree — culling, subdivision, and meshlet generation all on GPU. CPU only feeds camera position.

**Source:** [80.lv: GPU-Driven Quadtree Terrain Mesh Rendering](https://80.lv/articles/gpu-driven-quadtree-terrain-mesh-rendering-inspired-by-far-cry-5)

---

## 5. 32K Texture Generation

### 5.1 Memory Budget

| Map Type | Resolution | Format | Size (bytes) |
|----------|-----------|--------|-------------|
| Albedo (RGB) | 32768² | BC7 (4bpp) | 512 MB |
| Height (R32) | 32768² | R32_FLOAT | 4 GB |
| Normal (RG) | 32768² | BC5 (2bpp) | 256 MB |
| Roughness (R) | 32768² | BC4 (1bpp) | 128 MB |
| **Total** | | | **~4.9 GB** |

Full uncompressed: ~12 GB for RGBA32F. Compressed formats essential.

**Height map is the bottleneck** — R32F at 32K = 4 GB. Solutions:
1. Use R16 (2 GB, ~1cm precision at Earth scale — sufficient)
2. Tile generation — generate in tiles, never hold full texture in memory
3. Virtual texturing for streaming

### 5.2 Tiled/Streaming Generation

**Architecture:**
1. Divide 32K×32K face into tiles (e.g., 512×512 pixels → 64×64 tiles per face)
2. Generate each tile independently on GPU compute
3. Tiles written to disk as generated
4. Virtual texturing system streams visible tiles to GPU at runtime

**Tile generation pipeline:**
```
For each face (6):
  For each tile (64×64):
    1. Generate height via fBm + erosion
    2. Derive normal map from height
    3. Determine biome from height + moisture + temperature
    4. Generate albedo from biome palette + noise variation
    5. Generate roughness from material properties
    6. Encode + compress (BC7 on GPU via compute)
    7. Write tile to disk
```

**Compute cost estimate (RTX 3080):**
- 8 octaves noise per pixel: ~0.5ms per 512² tile
- 50 iterations erosion: ~2ms per 512² tile
- Full pipeline per tile: ~5-10ms
- Total 6 faces × 4096 tiles = 24,576 tiles × 7ms ≈ **~3 minutes** for full 32K planet

### 5.3 Virtual Texturing (MegaTextures)

**Runtime system:**
- Full texture stored on disk in tiled mipmapped format
- GPU requests tiles via feedback buffer
- Only visible tiles at correct mip level loaded into cache texture
- Cache: typically 128MB-512MB GPU memory
- Async compute for tile streaming

**Source:** [SIGGRAPH 2008 Advanced Virtual Texture Topics](https://advances.realtimerendering.com/s2008/SIGGRAPH%202008%20-%20Advanced%20virtual%20texture%20topics.pdf), [UE Virtual Textures Guide](https://dev.epicgames.com/community/learning/tutorials/58vb/unreal-engine-guide-to-virtual-textures-for-noobs)

### 5.4 Texture Compression Formats

| Format | BPP | Channels | Quality | GPU Decode |
|--------|-----|----------|---------|-----------|
| BC7 | 4 | RGBA | Excellent (high quality) | Hardware |
| BC5 | 2 | RG (normals) | Good | Hardware |
| BC4 | 1 | R (height/roughness) | Good | Hardware |
| ASTC | variable | RGBA | Excellent (configurable) | Hardware (mobile/modern) |
| ETC2 | 4 | RGBA | Good | Hardware |

**Note:** BCn formats have hardware decode on desktop GPUs. ASTC for cross-platform.

---

## 6. Biome Mapping Algorithms

### 6.1 Whittaker Diagram Implementation

The Whittaker biome diagram classifies biomes by **mean annual temperature (MAT)** and **mean annual precipitation (MAP)**. Implementation as a 2D lookup table:

```
Input: temperature (0-1), precipitation (0-1)
Output: biome ID

// Simplified Whittaker lookup
biome_table[temp_bin][precip_bin] = {
    //  very dry    dry        moderate   wet        very wet
    /* hot   */ { DESERT,    DESERT,    SAVANNA,   TROPICAL,  TROPICAL  },
    /* warm  */ { DESERT,    GRASSLAND, WOODLAND,  TEMPERATE, TEMP_RAIN },
    /* cool  */ { STEPPE,    STEPPE,    BOREAL,     BOREAL,    BOREAL    },
    /* cold  */ { TUNDRA,    TUNDRA,    TUNDRA,     TUNDRA,    TUNDRA    },
}
```

**Climate inputs needed:**
1. **Temperature** = base_latitude_temp - lapse_rate × elevation + noise_variation
2. **Precipitation** = base_noise + ocean_proximity_bonus - rain_shadow(mountains)
3. **Elevation** (from heightmap) — affects both T and P

**Key formulas:**
- Temperature lapse rate: ~6.5°C/km (Earth)
- Rain shadow: leeward side of mountains gets less rain
- Ocean proximity: coastal areas wetter, temperature moderated

### 6.2 Köppen Climate Classification

More detailed than Whittaker — uses monthly temperature and precipitation:
- **A (Tropical):** coldest month > 18°C
- **B (Arid):** precipitation below threshold
- **C (Temperate):** coldest month 0-18°C, warmest > 10°C
- **D (Continental):** coldest < 0°C, warmest > 10°C
- **E (Polar):** warmest < 10°C

Sub-classified by precipitation pattern (s/w/f) and temperature (a/b/c/d/h/k).

**Source:** [Worldbuilding Pasta: Beyond Köppen-Geiger](https://worldbuildingpasta.blogspot.com/2024/12/beyond-koppen-geiger-climate.html), [AutoBiomes paper (CGI 2020)](https://cgvr.cs.uni-bremen.de/papers/cgi20/AutoBiomes.pdf)

### 6.3 Biome Transition Zones

Smooth transitions between biomes using:
1. **Perlin noise blending:** Use noise to create irregular borders
2. **Gradient blending:** Interpolate biome properties at boundaries
3. **Ecotone width:** 10-50km typical transition zones

**Source:** [PCG Wiki: Whittaker Diagram](http://pcg.wikidot.com/pcg-algorithm:whittaker-diagram), [Reddit: Biome generation methods](https://www.reddit.com/r/proceduralgeneration/comments/7natln/resources_or_methods_for_biome_generation/)

---

## 7. Map Types & Generation Pipeline

### 7.1 Height Maps

**Generation pipeline:**
1. Base terrain: multi-octave noise (fBm), optionally warped
2. Continental placement: low-frequency noise or Voronoi cells
3. Ridge noise for mountain ranges
4. Hydraulic erosion pass (GPU compute)
5. Thermal erosion pass
6. Optional: crater placement (distance fields + rim uplift)

**Height map to normal map (Sobel filter on GPU):**
```glsl
// Compute normals from heightmap in compute shader
float hL = heightAt(x-1, y);
float hR = heightAt(x+1, y);
float hD = heightAt(x, y-1);
float hU = heightAt(x, y+1);
vec3 normal = normalize(vec3(hL - hR, 2.0 * texelSize, hD - hU));
```

### 7.2 Albedo Maps

Generated from biome classification + material library:

| Material | RGB (approx) | Notes |
|----------|-------------|-------|
| Ocean water | (10, 30, 80) | Deep, varies with depth |
| Sand/beach | (194, 178, 128) | Warm yellow |
| Grassland | (86, 130, 50) | Varies with moisture |
| Forest | (34, 80, 20) | Dark green |
| Snow/ice | (240, 245, 255) | Slightly blue |
| Rock/bare | (128, 128, 128) | Varies with geology |
| Desert sand | (210, 180, 140) | Tan |
| Volcanic | (50, 40, 35) | Dark basalt |
| Tundra | (140, 160, 130) | Muted green-gray |

Per-pixel variation via noise added to base biome color.

### 7.3 Roughness Maps

Derived from surface material:

| Material | Roughness | Notes |
|----------|-----------|-------|
| Water | 0.0-0.05 | Mirror-smooth when calm |
| Ice | 0.05-0.15 | Smooth, slightly rough |
| Sand (dry) | 0.8-1.0 | Very rough |
| Sand (wet) | 0.3-0.5 | Smoother |
| Rock | 0.6-0.9 | Depends on weathering |
| Snow | 0.3-0.5 | Somewhat diffuse |
| Vegetation | 0.4-0.7 | Variable |
| Lava (fresh) | 0.7-0.95 | Very rough, glassy patches |

### 7.4 Cloud Maps

Procedural cloud layers:
- **Stratiform:** Flat, layered noise at high altitude
- **Cumuliform:** Worley/Voronoi noise for fluffy, towering shapes
- **Cyclonic:** Spiral patterns via domain warping
- Coverage controlled by noise threshold

**Source:** [Gaia Sky planetary surfaces](https://tonisagrista.com/blog/2021/procedural-planetary-surfaces/)

---

## 8. Atmospheric Scattering

### 8.1 Bruneton & Neyret Method (2008/2017)

The gold standard for real-time atmospheric scattering. Precomputes scattering into LUTs:

**Lookup tables:**
- **Transmittance** (256×64 2D texture): how much light reaches each altitude
- **Single scattering** (256×128×32 3D texture): scattered light per altitude, sun angle, viewing angle
- **Irradiance** (64×16 2D texture): ground irradiance
- **Optional:** Multiple scattering (4D, packed as 3D)

**Physical parameters:**
- Rayleigh scattering: β_R = (5.5e-6, 13.0e-6, 22.4e-6) m⁻¹ (wavelength-dependent, blue sky)
- Mie scattering: β_M ≈ 21e-6 m⁻¹ (wavelength-independent, haze)
- Mie asymmetry factor: g ≈ 0.758 (forward-peaked)
- Ozone absorption: included in 2017 version
- Custom density profiles for different planet types

**Precomputation:** ~1 second on GPU for Earth-like atmosphere

**Runtime cost:** 2-4 texture lookups per pixel in fragment shader

**Source:** [Bruneton 2017 Implementation](https://ebruneton.github.io/precomputed_atmospheric_scattering/), [GPU Gems 2 Ch.16](https://developer.nvidia.com/gpugems/gpugems2/part-ii-shading-lighting-and-shadows/chapter-16-accurate-atmospheric-scattering), [GitHub: Scrawk Implementation](https://github.com/Scrawk/Brunetons-Atmospheric-Scatter)

### 8.2 Hillaire Method (Epic Games, 2020)

Improved approach for production use:
- No high-dimensional LUTs needed
- New analytical approximation for multiple scattering
- Scalable from mobile to high-end
- Used in Unreal Engine

**Source:** [Hillaire 2020 (EGSR)](https://sebh.github.io/publications/egsr2020.pdf), [cpp-rendering.io sky & atmosphere](https://cpp-rendering.io/sky-and-atmosphere-rendering/)

### 8.3 Elek Method (2009, CESCG)

Simpler single-scattering model. Good for fast previews. Less accurate at sunset/sunrise.

**Source:** [Elek 2009 — Rendering Parametrizable Planetary Atmospheres](https://old.cescg.org/CESCG-2009/papers/PragueCUNI-Elek-Oskar09.pdf)

---

## 9. Ocean Rendering

### 9.1 Tessendorf FFT Waves

Industry standard for realistic ocean waves:

1. Generate wave spectrum (Phillips or JONSWAP) in frequency domain
2. Add time-dependent phase animation
3. IFFT to get height displacement in spatial domain
4. Generate normal maps from displacement gradients

**FFT resolution:** 256×256 or 512×512 typical. GPU FFT (compute shader) runs in <1ms.

**Phillips spectrum:**
```
P(k) = A × exp(-1/(kL)²) / k⁴ × |k̂·ŵ|²
where L = V²/g (wind-dependent wavelength)
```

**Source:** [Tessendorf 2004 course notes](https://jtessen.people.clemson.edu/reports/papers_files/coursenotes2004.pdf), [Ubisoft: Ocean Surface Rendering using Tiling and Blending](https://www.ubisoft.com/en-us/studio/laforge/news/5WHMK3tLGMGsqhxmWls1Jw/making-waves-in-ocean-surface-rendering-using-tiling-and-blending)

### 9.2 Ocean SSS

Subsurface scattering for water gives the translucent green/blue look:
- Approximate with wrapped lighting: `NdotL = (dot(N,L) + wrap) / (1 + wrap)`
- Or use BSSRDF approximation for more accuracy
- Depth-dependent color attenuation: exponential falloff with distance through water

**Source:** [Ocean rendering part 1 (rtryan98)](https://rtryan98.github.io/2025/10/04/ocean-rendering-part-1.html), [WebGPU ocean simulation](https://barthpaleologue.github.io/Blog/posts/ocean-simulation-webgpu/)

---

## 10. Existing Systems Analysis

### 10.1 SpaceEngine

- **Quadtree-based terrain engine** with 6 cube faces
- **Multi-octave Perlin noise** for heightmaps
- Color maps from elevation + splatting shader + detail textures
- Resolution down to ~1mm per pixel
- LOD: quadtree subdivision with landscape tiles
- Texture arrays for atmosphere, clouds, lava, lights
- Moving generation from CPU to GPU (Vulkan + compute shaders)
- Deterministic from seed

**Source:** [SpaceEngine blog](https://spaceengine.org/news/blog100531/), [SpaceEngine user manual](https://spaceengine.org/manual/user-manual/)

### 10.2 Outerra

- Planet-scale terrain from real DEM data + procedural fill
- Chunked LOD with quadtree
- Level from satellite imagery down to ground detail
- Atmospheric scattering (Bruneton-based)

### 10.3 No Man's Sky

- Single seed → deterministic universe of 18 quintillion planets
- Fractal-based terrain generation
- On-the-fly generation — more detail as player approaches
- Algorithms updated between game versions (planets can change)
- Biome-driven coloring

**Source:** [No Man's Sky Wiki: Procedural Generation](https://nomanssky.fandom.com/wiki/Procedural_generation), [Rambus: Algorithms of NMS](https://www.rambus.com/blogs/the-algorithms-of-no-mans-sky-2/)

### 10.4 Elite Dangerous — Stellar Forge

- 1:1 scale Milky Way, 400 billion star systems
- Physics-based: models collisions, tidal forces, gravity
- Real astronomical catalog data as anchors
- Galactic "boxels" — cubic sectors with shared properties (age, metallicity)
- Deterministic from galactic coordinates as seed
- Predicted TRAPPIST-1 before official discovery

**Source:** [Elite Dangerous: Stellar Forge wiki](https://elite-dangerous.fandom.com/wiki/Stellar_Forge), [Frontier Forums](https://forums.frontier.co.uk/threads/myth-busting-on-stellar-forge-and-the-generation-of-everything-from-stars-to-rocks.517029/)

### 10.5 Gaia Sky (Open Source)

- Open source planet visualization
- GPU-accelerated procedural generation (since v3.6.3)
- Simplex noise with fBm
- Humidity + elevation → biome coloring via lookup table
- Temperature layer integration
- Voronoi and Curl noise available
- Terrace features supported

**Source:** [Gaia Sky procedural surfaces blog](https://tonisagrista.com/blog/2021/procedural-planetary-surfaces/), [Gaia Sky supercharging post](https://tonisagrista.com/blog/2024/supercharging-planetary-surfaces)

### 10.6 Other Notable Projects

- **PQN (Procedural Quadtree Noise)** — Unity planetary plugin
- **Michelic 2018 (CESCG)** — real-time procedural planets paper [PDF](https://cescg.org/wp-content/uploads/2018/04/Michelic-Real-Time-Rendering-of-Procedurally-Generated-Planets-2.pdf)
- **Andrew Willmott (Maxis/Spore)** — Creating Spherical Worlds [PDF](http://www.cs.cmu.edu/~ajw/s2007/0251-SphericalWorlds.pdf)
- **proceduralplanets.wordpress.com** — Detailed Unity planet generation blog

---

## 11. PBR Rendering Pipeline Integration

### 11.1 Map → Shader Flow

```
Height Map → [compute] → Normal Map
                                ↓
Albedo Map ────────────→ Fragment Shader ← Roughness Map
                                ↓
                  Atmospheric Scattering (Bruneton)
                                ↓
                  Ocean (FFT + SSS + Fresnel)
                                ↓
                  Clouds (volume / billboard)
                                ↓
                  Final HDR Output → Tone Map → Present
```

### 11.2 Day/Night Cycle

- Sun direction computed from orbital parameters + planet rotation
- Shadow mapping or ray-marching for terrain shadows
- Night side: city lights (noise-based placement in habitable biomes), lava glow
- Twilight zone: extended atmospheric scattering (Bruneton handles this)

---

## 12. Performance & Architecture

### 12.1 Compute Time Estimates (RTX 3080-class GPU)

| Operation | Resolution | Time |
|-----------|-----------|------|
| 8-octave fBm noise | 512² | ~0.5ms |
| 50-iteration erosion | 512² | ~2ms |
| Full tile pipeline (height+normals+biome+albedo+roughness) | 512² | ~5-10ms |
| Atmospheric LUT precompute | standard sizes | ~1s (one-time) |
| FFT ocean displacement | 256² | ~0.5ms |
| Full 32K planet (6 faces × 4096 tiles) | 32768² per face | ~3 min |

### 12.2 Pipeline Architecture

```
┌─────────────────────────────────────────────┐
│              PREVIEW MODE                    │
│  Low-res noise (256²) → instant preview     │
│  Show biome distribution, rough shape        │
└─────────────────┬───────────────────────────┘
                  │ User approves
                  ▼
┌─────────────────────────────────────────────┐
│           GENERATION MODE                    │
│  Async compute queue                        │
│  Generate tiles in batches (64-256 per frame)│
│  Progress bar: X/24576 tiles                 │
│  Write to disk as completed                  │
└─────────────────┬───────────────────────────┘
                  │ Generation complete
                  ▼
┌─────────────────────────────────────────────┐
│            RUNTIME MODE                      │
│  Virtual texturing system                    │
│  Quadtree LOD for mesh                       │
│  Stream visible tiles only                   │
│  Atmospheric scattering (precomputed LUTs)   │
└─────────────────────────────────────────────┘
```

### 12.3 File Formats for Large Textures

| Format | Pros | Cons | Use Case |
|--------|------|------|----------|
| EXR | Float precision, tiles, mipmaps | Large files | Height maps |
| TIFF | Tiled, many codecs | Variable support | General |
| DDS/BCn | GPU-native, compressed | Lossy | Albedo, normals, roughness |
| Custom tiled | Optimized for streaming | Proprietary | Virtual texturing |
| KTX2 | Modern, compressed, mipmapped | Newer format | Cross-platform |

### 12.4 Multi-GPU Considerations

- Split tile generation across GPUs (embarrassingly parallel)
- One GPU for generation, one for rendering
- Or use async compute on single GPU (generate tiles during rendering gaps)

---

## 13. Key References & Source URLs

### Academic Papers
- [Bruneton & Neyret 2008/2017 — Precomputed Atmospheric Scattering](https://ebruneton.github.io/precomputed_atmospheric_scattering/)
- [Hillaire 2020 — Scalable Atmospheric Scattering (EGSR)](https://sebh.github.io/publications/egsr2020.pdf)
- [Tessendorf 2004 — Simulating Ocean Water](https://jtessen.people.clemson.edu/reports/papers_files/coursenotes2004.pdf)
- [Mei et al. — Fast Hydraulic Erosion on GPU](https://www.researchgate.net/publication/4295561_Fast_Hydraulic_Erosion_Simulation_and_Visualization_on_GPU)
- [Michelic 2018 — Real-Time Procedural Planets](https://cescg.org/wp-content/uploads/2018/04/Michelic-Real-Time-Rendering-of-Procedurally-Generated-Planets-2.pdf)

### GPU Gems & Technical
- [GPU Gems Ch.5 — Improved Perlin Noise](https://developer.nvidia.com/gpugems/gpugems/part-i-natural-effects/chapter-5-implementing-improved-perlin-noise)
- [GPU Gems 2 Ch.16 — Accurate Atmospheric Scattering](https://developer.nvidia.com/gpugems/gpugems2/part-ii-shading-lighting-and-shadows/chapter-16-accurate-atmospheric-scattering)
- [SIGGRAPH 2008 — Advanced Virtual Texture Topics](https://advances.realtimerendering.com/s2008/SIGGRAPH%202008%20-%20Advanced%20virtual%20texture%20topics.pdf)

### Open Source & Blogs
- [Gaia Sky — Procedural Planetary Surfaces](https://tonisagrista.com/blog/2021/procedural-planetary-surfaces/)
- [acko.net — Making Worlds: Spheres and Cubes](https://acko.net/blog/making-worlds-1-of-spheres-and-cubes/)
- [GitHub: Scrawk Bruneton Atmosphere (Unity)](https://github.com/Scrawk/Brunetons-Atmospheric-Scatter)
- [GitHub: Unity GPU Terrain Erosion](https://github.com/bshishov/UnityTerrainErosionGPU)
- [SpaceEngine blog](https://spaceengine.org/news/blog100531/)
- [80.lv: GPU-Driven Quadtree Terrain](https://80.lv/articles/gpu-driven-quadtree-terrain-mesh-rendering-inspired-by-far-cry-5)
- [PCG Wiki: Whittaker Diagram](http://pcg.wikidot.com/pcg-algorithm:whittaker-diagram)

### Existing Systems
- [Elite Dangerous: Stellar Forge](https://elite-dangerous.fandom.com/wiki/Stellar_Forge)
- [No Man's Sky: Procedural Generation](https://nomanssky.fandom.com/wiki/Procedural_generation)
- [Spore: Creating Spherical Worlds (Willmott)](http://www.cs.cmu.edu/~ajw/s2007/0251-SphericalWorlds.pdf)

---

_Confidence: HIGH for established techniques (noise, LOD, Bruneton). MEDIUM for performance estimates (varies by hardware). LOW for specific implementation details of closed-source systems._
