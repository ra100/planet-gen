# PBR Materials Pipeline: Deep-Dive Consolidated Research

> Consolidated from four source research documents, 2026-03-27 to 2026-03-28.
> Focuses on unique deep-dive content not covered in final.md (which already has basic albedo/roughness tables, spectral RGB approximations, biome color palettes, and basic cube-to-sphere overview).

---

## Table of Contents

1. [Executive Summary](#1-executive-summary)
2. [Sphere Parameterization Deep Dive](#2-sphere-parameterization-deep-dive)
3. [PBR Map Generation Pipeline](#3-pbr-map-generation-pipeline)
4. [Material Properties by Composition](#4-material-properties-by-composition)
5. [Biome Transition Blending](#5-biome-transition-blending)
6. [Weathering Simulation](#6-weathering-simulation)
7. [Code Examples](#7-code-examples)
8. [References](#8-references)

---

## 1. Executive Summary

This document consolidates deep-dive research on the PBR materials pipeline for procedural planet rendering. It covers three interrelated domains:

- **Sphere parameterization**: Detailed distortion analysis comparing cube-to-sphere variants, Fibonacci lattice, octahedral mapping, HEALPix, and icosphere grids -- with quantitative area ratios and GPU cost trade-offs.
- **PBR map generation**: Weathering-driven roughness, curvature-based roughness derivation, metallic maps from mineral composition, parallax occlusion mapping vs tessellation, and height-based material blending.
- **Material science**: Rock-type albedo by geological classification (igneous, metamorphic, sedimentary), mineral-specific weathering rates and color shifts, space weathering on airless bodies, and regolith optical effects.

The goal is to provide implementation-ready detail for Planet Gen's future 8K+ export pipeline and physically-based terrain coloring.

---

## 2. Sphere Parameterization Deep Dive

### 2.1 Cube-to-Sphere Projection Variants

#### 2.1.1 Naive Normalization (Gnomonic)

```glsl
vec3 cubeToSphere(vec3 p) {
    return normalize(p);
}
```

- Max/min area ratio: **~3.5-5.2:1** (worst among common methods)
- Face-center vertices unmoved; corner vertices move from `sqrt(3)` to `1.0`
- Neither equal-area nor conformal

#### 2.1.2 Analytic Mapping

For a cube-face point `p = (x, y, z)`:

```
sphere_point = p * sqrt(1 - (p_yx^2 + p_zz^2) / 2 + (p_yx^2 * p_zz^2) / 3)
```

```glsl
vec3 cubeToSphereAnalytic(vec3 p) {
    vec3 p2 = p * p;
    return p * sqrt(1.0 - (p2.yxx + p2.zzy) / 2.0 + (p2.yxx * p2.zzy) / 3.0);
}
```

- Max/min area ratio: **~1.57-1.8:1** (significant improvement)
- Points pulled toward edge midpoints instead of corners

#### 2.1.3 Tangent-Adjusted Mapping

Pre-warps UV coordinates using `tan(w * pi/4)` for uniform angular spacing:

```glsl
vec3 cubeToSphereTangent(vec3 faceNormal, vec2 uv, vec3 right, vec3 up) {
    vec2 warped = tan(uv * 0.7853981633974483); // pi/4
    vec3 p = faceNormal + warped.x * right + warped.y * up;
    return normalize(p);
}

// Inverse: sphere to cube face UV
vec2 sphereToCubeTangent(vec2 uv) {
    return (4.0 / 3.14159265) * atan(uv);
}
```

- Max/min area ratio: **~1.414:1 (sqrt(2))**
- Used by Google S2 geometry library
- Very fast: single `tan` or `atan` per axis

#### 2.1.4 Equal-Area (Inverse Lambert Azimuthal)

Two-step projection achieving perfectly equal area:

```glsl
vec3 cubeToSphereEqualArea(vec3 faceNormal, vec2 uv, vec3 right, vec3 up) {
    // Step 1: Map to curved square
    float u2 = uv.x * uv.x;
    float v2 = uv.y * uv.y;
    float u_prime = uv.x * sqrt(1.0 - v2 / 3.0);
    float v_prime = uv.y * sqrt(1.0 - u2 / 3.0);

    // Step 2: Inverse Lambert azimuthal
    float r2 = u_prime * u_prime + v_prime * v_prime;
    float sz = 1.0 - r2;
    float scale = sqrt(2.0 - r2);
    float sx = u_prime * scale;
    float sy = v_prime * scale;

    return sx * right + sy * up + sz * faceNormal;
}
```

- **Area distortion: 1.0:1 (perfectly equal-area)**
- Angular distortion present but bounded at face corners

#### 2.1.5 Quadrilateralized Spherical Cube (QSC / COBE)

Used by COBE satellite mission. Properties:

- Equal-area projection
- Limited angular distortion (~22 degrees max at face edges)
- 6 faces with `6 * 2^(2N)` bins at depth N
- Average error 4.7 arcsec, RMS 6.6 arcsec, peak 24 arcsec
- Implemented in PROJ library (`+proj=qsc`)

#### 2.1.6 Comparison Table

| Method               | Max/Min Area Ratio | Conformal? | Equal-Area? | GPU Cost             | Best For                 |
| -------------------- | ------------------ | ---------- | ----------- | -------------------- | ------------------------ |
| Naive (gnomonic)     | ~3.5-5.2:1         | No         | No          | Cheapest (normalize) | Prototyping              |
| Analytic             | ~1.57-1.8:1        | No         | No          | Low (1 sqrt)         | General purpose          |
| Tangent-adjusted     | ~1.414:1           | No         | No          | Low (1 tan/atan)     | Real-time rendering      |
| Equal-area (Lambert) | 1.0:1              | No         | Yes         | Medium (2 sqrt)      | Scientific visualization |
| QSC (COBE)           | 1.0:1              | No         | Yes         | Medium               | Data storage, CMB maps   |

### 2.2 HEALPix (Hierarchical Equal Area isoLatitude Pixelization)

Three key properties: (1) all pixels at given resolution cover same area, (2) pixel centers lie on isolatitude rings, (3) recursive quadtree subdivision from 12 base pixels.

**Resolution formulas:**

```
N_pix = 12 * N_side^2
Omega_pix = pi / (3 * N_side^2)        [steradians per pixel]
theta_pix = sqrt(pi / (3 * N_side^2))  [angular resolution]
```

| N_side | N_pix     | Pixel Area (sr) | Angular Resolution |
| ------ | --------- | --------------- | ------------------ |
| 1      | 12        | 1.0472          | ~58.6 deg          |
| 8      | 768       | 0.01636         | ~7.3 deg           |
| 64     | 49,152    | 2.557e-4        | ~0.92 deg          |
| 512    | 3,145,728 | 3.996e-6        | ~0.11 deg          |

**Projection construction:**

- Equatorial region: Lambert cylindrical equal-area projection
- Polar caps: Collignon pseudo-cylindrical equal-area projection
- Transition at `theta = arccos(2/3)` (~41.8 deg from pole)

**Benefits for noise sampling:** Equal area guarantees each noise sample covers the same solid angle -- no polar oversampling. Isolatitude property enables row-by-row processing. No singularities at poles. NESTED ordering maps well to GPU spatial locality.

**Limitations:** Pixel boundaries are curves (harder to rasterize), no native triangle mesh, less game engine support, more complex indexing math.

### 2.3 Icosphere / Geodesic Grids

**Vertex count after n subdivisions:** `V(n) = 10 * 4^n + 2`
**Face count:** `F(n) = 20 * 4^n`

| Subdivisions | Vertices | Faces  |
| ------------ | -------- | ------ |
| 0            | 12       | 20     |
| 3            | 642      | 1,280  |
| 5            | 10,242   | 20,480 |
| 6            | 40,962   | 81,920 |

**Subdivision classes (Goldberg polyhedra dual):**

- Class I (m,0): `T = m^2` -- axial, uniform edge division
- Class II (m,m): `T = 3m^2` -- triacon subdivision
- Class III (m,n): `T = m^2 + mn + n^2` -- general, potentially chiral

**Uniformity:** Max-to-min edge length ratio converges to ~1.17:1 (vs infinity at UV sphere poles). 12 vertices retain icosahedral 5-fold symmetry. The dual hex grid (Goldberg polyhedron) has exactly 12 pentagons per Euler's formula.

**Key GPU consideration:** Icosphere patches are triangular -- this complicates texture tiling vs quad-based cube faces. Triangle-based LOD requires T-junction elimination at subdivision boundaries.

### 2.4 Octahedral Mapping

Maps the unit sphere to an octahedron, unfolded into a single square:

```glsl
// Encoding (3D to 2D)
vec2 octEncode(vec3 n) {
    n /= (abs(n.x) + abs(n.y) + abs(n.z));
    if (n.z < 0.0) {
        n.xy = (1.0 - abs(n.yx)) * sign(n.xy);
    }
    return n.xy * 0.5 + 0.5;
}

// Decoding (2D to 3D)
vec3 octDecode(vec2 f) {
    f = f * 2.0 - 1.0;
    vec3 n = vec3(f.x, f.y, 1.0 - abs(f.x) - abs(f.y));
    float t = max(-n.z, 0.0);
    n.xy -= sign(n.xy) * t;
    return normalize(n);
}
```

| Property             | Cubemap                    | Octahedral                 |
| -------------------- | -------------------------- | -------------------------- |
| Encoding cost        | 10 VALU ops                | 19 VALU ops                |
| Neighbor sampling    | Complex (face transitions) | Simple 2D offsets (8 VALU) |
| Texture storage      | 6 square faces             | 1 square                   |
| GPU hardware support | Native `textureCube`       | Standard 2D texture        |

Max error: 0.04 deg with 24-bit precision. Near-uniform across most of sphere, with discontinuity along fold edges of lower hemisphere.

### 2.5 Fibonacci Sphere

Distributes points via golden-angle spiral -- excellent point uniformity with no UV mapping.

```
For i = 0 to N-1:
    theta_i = 2*pi*i / phi          (golden angle increments)
    phi_i   = acos(1 - 2*(i + eps) / (N - 1 + 2*eps))
```

Where `phi = (1 + sqrt(5)) / 2` and `eps ~= 0.36` for offset lattice.

**Uniformity metric:** Normalized minimum nearest-neighbor distance `delta* ~= 3.35` (vs 3.09 for canonical, ~8.3% improvement).

**Limitations:** No regular grid structure; requires Delaunay triangulation for mesh; no natural LOD hierarchy; hard to texture.

### 2.6 3D Noise vs 2D Noise for Sphere Texturing

| Method                  | Seamless?  | Dimensions | GPU Cost | Distortion        | Best For                    |
| ----------------------- | ---------- | ---------- | -------- | ----------------- | --------------------------- |
| 3D noise at xyz         | Yes        | 3D         | Low      | None              | Direct vertex displacement  |
| 4D torus trick          | Yes        | 4D         | Medium   | None              | Equirect texture generation |
| Lat/lon + 2D noise      | No (poles) | 2D         | Lowest   | Severe at poles   | Quick prototypes only       |
| Cube face 2D + blending | Mostly     | 2D         | Low      | At face edges     | Cubemap terrain             |
| Triplanar               | Yes        | 3x 2D      | 3x cost  | Blend transitions | Detail texturing            |

**4D torus trick for seamless sphere noise:** Map 2D texture coords to 4D noise via torus embedding -- both axes loop without distortion because a 4D torus (product of two circles) embeds isometrically:

```glsl
float seamlessSphereNoise(vec2 uv, float scale) {
    float theta = uv.x * 6.2831853;  // 2*pi
    float phi   = uv.y * 3.1415926;  // pi
    vec4 noisePos = vec4(
        cos(theta) * scale,
        sin(theta) * scale,
        cos(phi) * scale,
        sin(phi) * scale
    );
    return snoise(noisePos);  // 4D simplex noise
}
```

### 2.7 Cube Sphere Seam Fixes

**Overlap borders:** Extend each face texture by a few pixels beyond the boundary; blend in a narrow strip.

**3D noise bypass:** Sample 3D noise at Cartesian sphere coordinates -- inherently seamless, works for any sphere representation:

```glsl
float seamlessNoise(vec3 spherePos) {
    return snoise(spherePos * noiseScale);
}
```

**Pre-distort UV grid:** Apply inverse distortion to cube face grid to counteract normalization distortion.

### 2.8 Recommendations by Use Case

- **Real-time games:** Cube sphere with tangent-adjusted mapping. Best distortion-to-simplicity ratio (1.414:1). Native GPU cubemap support. Quadtree LOD per face. Google S2 validates at production scale.
- **Scientific visualization:** HEALPix or equal-area cube sphere. Perfect area preservation for physical simulation.
- **Maximum uniformity:** Icosphere (geodesic grid). 1.17:1 max edge length ratio. Consider Goldberg dual hex grid for simulation.
- **Procedural noise terrain:** Any sphere representation + 3D simplex noise. Decouples noise from parameterization entirely.

---

## 3. PBR Map Generation Pipeline

### 3.1 Roughness from Surface Curvature

Roughness maps for planets can be derived procedurally from terrain geometry:

```glsl
float terrainRoughness(float height, float slope, float curvature,
                       float biomeRoughness, vec2 pos) {
    float roughness = biomeRoughness;           // base from biome LUT
    roughness += slope * 0.2;                    // steep = more exposed rock
    roughness += curvature * 0.1;                // ridges = more weathered
    roughness += noise(pos * 128.0) * 0.05;     // micro-detail variation
    roughness -= ao * 0.1;                       // crevices = wet = smoother
    return clamp(roughness, 0.02, 1.0);
}
```

**Factors modulating roughness:**

- **Height:** Lower elevation (wet areas) -> smoother; higher -> rougher
- **Slope:** Steep slopes (exposed rock) -> medium roughness
- **Curvature:** Convex ridges -> more weathered (rougher); concave valleys -> sediment accumulation (smoother)
- **Ambient occlusion:** Dark crevices act as proxy for moisture -> smoother
- **Erosion data:** Stream-power-law erosion output directly informs roughness

### 3.2 Roughness by Biome with Slope/Moisture Modifiers

```glsl
float compute_roughness(int biome_id, float slope, float height, float moisture) {
    float roughness = biome_base_roughness[biome_id];
    // Water: 0.05-0.1 | Sand/desert: 0.4-0.6 | Rock: 0.7-0.9
    // Forest canopy: 0.5-0.7 | Snow: 0.3-0.5

    roughness = mix(roughness, 0.85, smoothstep(0.4, 0.8, slope));  // steeper = rougher
    roughness *= mix(1.0, 0.7, moisture);  // wet = smoother
    return clamp(roughness, 0.05, 1.0);
}
```

**ORM packing** for GPU efficiency: Red=AO, Green=Roughness, Blue=Metallic -- reduces texture samples.

### 3.3 Metallic Map from Mineral Composition

Metallic values in PBR are binary for most natural materials (dielectric = 0.0, metal = 1.0), but planet surfaces contain mineral blends that can produce intermediate metallic hints:

**Approach: crustal composition -> metallic signal**

| Mineral/Material                | Metallic Value | Notes                                  |
| ------------------------------- | -------------- | -------------------------------------- |
| Quartz, feldspar, calcite       | 0.0            | Pure dielectric                        |
| Wet surfaces (any)              | 0.0            | Water is dielectric                    |
| Iron oxide coatings (hematite)  | 0.0-0.05       | Very slight metallic sheen             |
| Magnetite-rich basalt           | 0.05-0.15      | Titaniferous magnetite (0.5-2% TiO2)   |
| Desert varnish (Mn/Fe oxides)   | 0.05-0.10      | Thin coating, subtle sheen             |
| Native metal deposits (rare)    | 0.8-1.0        | Exposed ore veins, extremely localized |
| Fresh volcanic glass (obsidian) | 0.02-0.05      | Specular but non-metallic              |

**Implementation strategy:**

```glsl
float computeMetallic(float ironContent, float magnetiteRatio, float weatheringAge) {
    // Most terrain is dielectric
    float metallic = 0.0;
    // Fresh mafic rock with high magnetite can have slight metallic character
    metallic += magnetiteRatio * 0.15;
    // Weathering oxidizes metallic minerals, reducing metallic signal
    metallic *= (1.0 - weatheringAge * 0.8);
    return clamp(metallic, 0.0, 0.2);  // cap at 0.2 -- natural surfaces are almost never metallic
}
```

For planet-scale rendering, metallic is effectively 0.0 everywhere except for very localized ore deposit features. The primary visual distinction comes from albedo and roughness.

### 3.4 Parallax Occlusion Mapping vs Tessellation

#### Basic Parallax Mapping

Single-sample UV offset based on view direction:

```glsl
vec2 parallaxMapping(vec2 texCoords, vec3 viewDir) {
    float height = texture(depthMap, texCoords).r;
    vec2 p = viewDir.xy / viewDir.z * (height * heightScale);
    return texCoords - p;
}
```

Minimal overhead, breaks at steep angles.

#### Steep Parallax Mapping

Layer-based ray marching through depth range:

```glsl
vec2 steepParallaxMapping(vec2 texCoords, vec3 viewDir) {
    const float numLayers = 10.0;
    float layerDepth = 1.0 / numLayers;
    float currentLayerDepth = 0.0;
    vec2 P = viewDir.xy * heightScale;
    vec2 deltaTexCoords = P / numLayers;

    vec2 currentTexCoords = texCoords;
    float currentDepthMapValue = texture(depthMap, currentTexCoords).r;

    while (currentLayerDepth < currentDepthMapValue) {
        currentTexCoords -= deltaTexCoords;
        currentDepthMapValue = texture(depthMap, currentTexCoords).r;
        currentLayerDepth += layerDepth;
    }
    return currentTexCoords;
}
```

#### Parallax Occlusion Mapping (POM)

Refines steep parallax with linear interpolation between bracketing layers:

```glsl
// After steep parallax finds intersection...
vec2 prevTexCoords = currentTexCoords + deltaTexCoords;
float afterDepth  = currentDepthMapValue - currentLayerDepth;
float beforeDepth = texture(depthMap, prevTexCoords).r
                    - currentLayerDepth + layerDepth;
float weight = afterDepth / (afterDepth - beforeDepth);
vec2 finalTexCoords = prevTexCoords * weight
                    + currentTexCoords * (1.0 - weight);
```

#### Performance Comparison

| Technique               | Samples/pixel  | Silhouettes | Depth Correct | Best For                   |
| ----------------------- | -------------- | ----------- | ------------- | -------------------------- |
| Basic parallax          | 1              | No          | No            | Subtle bumps               |
| Steep parallax          | 8-32           | No          | No            | Medium detail              |
| POM                     | 8-32           | No          | No            | High detail, flat surfaces |
| Tessellation + displace | N/A (geometry) | Yes         | Yes           | Close-up terrain           |

#### Recommendations for Planets

- **Far view:** Normal maps only (no parallax needed)
- **Medium view:** POM for terrain detail layers (rock faces, cliffs)
- **Close view:** Tessellation + displacement for geometry-correct rendering
- Use LOD transitions to blend techniques by camera distance
- POM artifacts are worst at grazing angles; discard fragments where displaced UVs exceed [0,1]

### 3.5 Ambient Occlusion from Heightmaps

#### HBAO (Horizon-Based AO)

Cast rays in N directions along height field, march M steps to find maximum horizon angle:

```glsl
float hbaoFromHeightmap(sampler2D heightMap, vec2 uv, vec2 texelSize) {
    float centerH = texture(heightMap, uv).r;
    float ao = 0.0;
    const int NUM_DIRECTIONS = 8;
    const int NUM_STEPS = 6;
    float sampleRadius = 10.0;

    for (int d = 0; d < NUM_DIRECTIONS; d++) {
        float angle = float(d) * PI * 2.0 / float(NUM_DIRECTIONS);
        vec2 dir = vec2(cos(angle), sin(angle));

        float maxHorizon = -1.0;
        for (int s = 1; s <= NUM_STEPS; s++) {
            vec2 sampleUV = uv + dir * texelSize * float(s) * sampleRadius / float(NUM_STEPS);
            float sampleH = texture(heightMap, sampleUV).r;
            float dh = sampleH - centerH;
            float dist = float(s) * sampleRadius / float(NUM_STEPS);
            float horizon = dh / dist;
            maxHorizon = max(maxHorizon, horizon);
        }
        ao += clamp(maxHorizon, 0.0, 1.0);
    }
    return 1.0 - ao / float(NUM_DIRECTIONS);
}
```

#### Line-Sweep AO

O(N) per scanline (amortized) -- efficient for offline baking. Sweep in 8-16 directions maintaining running maximum horizon angle.

#### Compute Shader Strategy

Both HBAO and blur passes benefit from group-shared memory:

```glsl
layout(local_size_x = 16, local_size_y = 16) in;
shared float sharedHeights[18][18];  // 16x16 + 1-texel border

void main() {
    ivec2 gid = ivec2(gl_GlobalInvocationID.xy);
    ivec2 lid = ivec2(gl_LocalInvocationID.xy);
    sharedHeights[lid.y + 1][lid.x + 1] = imageLoad(heightMap, gid).r;
    // Load border texels...
    barrier();
    // Compute HBAO from shared memory instead of texture loads
}
```

### 3.6 Normal Map Generation

#### Central Differences (simplest)

```glsl
vec3 heightToNormal(sampler2D heightMap, vec2 uv, vec2 texelSize, float strength) {
    float hL = texture(heightMap, uv - vec2(texelSize.x, 0.0)).r;
    float hR = texture(heightMap, uv + vec2(texelSize.x, 0.0)).r;
    float hD = texture(heightMap, uv - vec2(0.0, texelSize.y)).r;
    float hU = texture(heightMap, uv + vec2(0.0, texelSize.y)).r;
    float dX = (hR - hL) * strength;
    float dY = (hU - hD) * strength;
    return normalize(vec3(-dX, -dY, 1.0));
}
```

#### Sobel 3x3 (better noise rejection)

```
Gx: [-1 0 1; -2 0 2; -1 0 1]    Gy: [-1 -2 -1; 0 0 0; 1 2 1]
```

#### Scharr 3x3 (best rotational symmetry)

```
Gx: [-3 0 3; -10 0 10; -3 0 3]   Gy: [-3 -10 -3; 0 0 0; 3 10 3]
```

Scharr preferred for spherical heightmaps due to rotational invariance.

#### Analytical Derivatives (no heightmap needed)

When noise is procedural, compute gradients directly via chain rule -- resolution-independent, no texel artifacts, costs ~2x a single noise evaluation.

### 3.7 Splat Map Generation

Procedurally generate material weights from terrain properties:

```glsl
vec4 generateSplatWeights(float height, float slope, float curvature,
                          float moisture, float noise) {
    vec4 weights = vec4(0.0);
    // R = grass: low slope, medium height, high moisture
    weights.r = smoothstep(0.3, 0.7, moisture)
              * (1.0 - smoothstep(0.5, 0.8, slope))
              * smoothstep(0.05, 0.2, height);
    // G = rock: steep slope or high altitude
    weights.g = smoothstep(0.4, 0.7, slope)
              + smoothstep(0.6, 0.8, height) * 0.5;
    // B = sand: low height, low moisture
    weights.b = (1.0 - smoothstep(0.0, 0.15, height))
              * (1.0 - smoothstep(0.2, 0.5, moisture));
    // A = snow: high altitude, low slope
    weights.a = smoothstep(0.7, 0.85, height)
              * (1.0 - smoothstep(0.5, 0.8, slope));

    weights /= max(dot(weights, vec4(1.0)), 0.001);  // normalize
    return weights;
}
```

---

## 4. Material Properties by Composition

### 4.1 Igneous Rock Classification and Albedo

| Category     | SiO2 (wt%) | Key Minerals                                  | Color                     | Albedo    |
| ------------ | ---------- | --------------------------------------------- | ------------------------- | --------- |
| Felsic       | 65-75      | Quartz, K-feldspar, Na-plagioclase, muscovite | Light (white, pink, grey) | 0.20-0.35 |
| Intermediate | 52-63      | Plagioclase, hornblende, biotite, pyroxene    | Medium grey               | 0.15-0.25 |
| Mafic        | 45-52      | Ca-plagioclase, pyroxene, olivine             | Dark                      | 0.06-0.12 |
| Ultramafic   | <=40       | Olivine, pyroxene                             | Very dark green-black     | 0.05-0.10 |

**Key physical principle:** Iron and magnesium content = darkness. Fe2+/Mg2+-bearing mafic minerals strongly absorb visible light. Silica and aluminum content = lightness -- quartz (SiO2) has high visible transmittance, feldspar is white to pink.

#### Basalt Detail

- SiO2: 45-52%, FeO: 5-14%, MgO: 5-12%, TiO2: 0.5-2.0%
- Absorbs roughly evenly across visible spectrum -> near-black
- Most common rock on Earth's surface (oceanic crust)
- Albedo: ~0.06-0.12 (Icelandic basalts measured ~0.11)

#### Granite Detail

- SiO2: 65-75%, quartz 20-60%, alkali feldspar, biotite, muscovite
- Major component of continental crust
- Albedo: ~0.20-0.35 (lighter varieties approach 0.35)

#### Andesite Detail

- SiO2: 52-63%, plagioclase dominant
- Characteristic of subduction zones and volcanic arcs
- Albedo: ~0.15-0.25

### 4.2 Metamorphic Rock Albedo

Metamorphic facies produce distinct surface appearances:

| Rock          | Protolith | Albedo    | Notes                                  |
| ------------- | --------- | --------- | -------------------------------------- |
| Marble        | Limestone | 0.40-0.60 | Highest reflectance among common rocks |
| Slate         | Shale     | 0.08-0.15 | Dark grey to black                     |
| Quartzite     | Sandstone | 0.25-0.40 | White to pale                          |
| Schist/gneiss | Various   | 0.10-0.25 | Varies with mica/feldspar content      |

**Pressure-temperature facies progression (Barrovian):**
Zeolite (200-300C) -> Greenschist (300-500C) -> Amphibolite (500-750C) -> Granulite (700-1000C)

**Subduction facies:**
Zeolite -> Blueschist (200-500C, 6-18 kbar) -> Eclogite (500-800C+, >12 kbar)

### 4.3 Sedimentary Rock Albedo

#### Clastic

| Rock           | Grain Size      | Albedo    |
| -------------- | --------------- | --------- |
| Conglomerate   | >2 mm           | 0.10-0.20 |
| Sandstone      | 0.0625-2 mm     | 0.20-0.40 |
| Siltstone      | 0.004-0.0625 mm | 0.15-0.25 |
| Shale/mudstone | <0.004 mm       | 0.05-0.15 |

#### Chemical

| Rock      | Composition             | Albedo    |
| --------- | ----------------------- | --------- |
| Limestone | CaCO3                   | 0.10-0.35 |
| Dolomite  | CaMg(CO3)2              | 0.20-0.35 |
| Rock salt | NaCl                    | 0.40-0.50 |
| Gypsum    | CaSO4-2H2O              | 0.35-0.55 |
| Chert     | SiO2 (microcrystalline) | 0.10-0.25 |

#### Biogenic

| Rock      | Origin                 | Albedo    |
| --------- | ---------------------- | --------- |
| Chalk     | Coccolithophore shells | 0.40-0.55 |
| Coal      | Plant material         | 0.03-0.05 |
| Diatomite | Diatom frustules       | 0.30-0.45 |

### 4.4 Grain Size Effect on Albedo

Crushed/powdered rock is generally lighter than intact surfaces because scattering increases with grain boundaries. A planet covered in fine-grained regolith will have somewhat higher albedo than polished bedrock of the same composition.

---

## 5. Biome Transition Blending

### 5.1 Scattered Biome Blending (KdotJPG)

State-of-the-art approach using jittered triangular grid + normalized sparse convolution:

```glsl
// Weight contribution per point
float weight = max(0.0, radius2 - dx*dx - dy*dy);
weight = weight * weight;  // squared polynomial falloff (reaches zero at finite radius)

// Normalization: weights always sum to 1.0
float total = 0.0;
for (int i = 0; i < num_points; i++) total += weights[i];
float inv_total = 1.0 / total;
for (int i = 0; i < num_points; i++) weights[i] *= inv_total;
```

**Performance (ns per coordinate):**

| Method                      | radius=24 | radius=48 |
| --------------------------- | --------- | --------- |
| Full-resolution convolution | 4,851 ns  | 23,136 ns |
| Scattered blending          | 196 ns    | 665 ns    |
| Convoluted grid             | 98 ns     | 397 ns    |
| Lerped grid (has artifacts) | 22 ns     | 40 ns     |

Scattered blending is **25x faster** than full-resolution while avoiding grid artifacts. Single-biome chunk detection yields an additional 36% improvement.

### 5.2 Noise-Based Boundary Perturbation

```glsl
float boundary_noise = fbm(pos * 0.01, 4);
float blend_factor = smoothstep(-0.1, 0.1, biome_distance + boundary_noise * 0.05);
vec3 color = mix(biome_a_color, biome_b_color, blend_factor);
```

### 5.3 Height-Based Blending (Advanced)

Simple linear blending produces unrealistic mud-like transitions. Height-based blending uses material depth maps:

**Simple linear** (poor quality):

```glsl
vec3 color = tex1.rgb * a1 + tex2.rgb * a2;
```

**Depth-filtered blend** (best quality):

```glsl
vec3 heightBlend(vec4 tex1, float a1, vec4 tex2, float a2) {
    float depth = 0.2;
    float ma = max(tex1.a + a1, tex2.a + a2) - depth;
    float b1 = max(tex1.a + a1 - ma, 0.0);
    float b2 = max(tex2.a + a2 - ma, 0.0);
    return (tex1.rgb * b1 + tex2.rgb * b2) / (b1 + b2);
}
```

This simulates realistic material settling: sand fills cracks in stone, grass grows on flat areas between rocks.

### 5.4 Slope-Dependent Blending with Triplanar

```glsl
vec3 biome_color(float height, float slope, float moisture, float temp) {
    int biome_id = getBiome(temp, moisture);  // Whittaker lookup
    vec3 flat_color = sample_biome_texture(biome_id, FLAT, uv);
    vec3 slope_color = sample_biome_texture(biome_id, SLOPE, uv);
    float slope_factor = smoothstep(0.3, 0.7, slope);
    return mix(flat_color, slope_color, slope_factor);
}
```

**Optimization (Sapra Projects):** Pack 6 biome masks into 2 RGBA textures, sort by weight, sample only 4 most influential biomes. Result: 36 texture lookups (down from 168), **17.9x faster**, scales independently of biome count.

---

## 6. Weathering Simulation

### 6.1 Iron Oxidation and Reddening

The most prominent weathering color change. Fe2+-bearing minerals (olivine, pyroxene, magnetite) oxidize to Fe3+:

- **Hematite** (Fe2O3): Deep red; dominant pigment in red soils/sandstones
- **Goethite** (FeOOH): Yellow-brown; common in temperate weathering
- **Ferrihydrite** (Fe5O8H-nH2O): Reddish-brown, poorly crystalline; 2025 research confirmed as dominant iron phase in Martian dust (formed in presence of cool water)

**Effect on albedo:**

| Surface State                | Albedo    | Color            |
| ---------------------------- | --------- | ---------------- |
| Fresh basalt                 | 0.06-0.10 | Dark grey-black  |
| Weathered basalt (Fe oxides) | 0.10-0.18 | Red-brown        |
| Heavily oxidized (Mars-like) | 0.15-0.25 | Rust-red         |
| Fresh granite                | 0.25-0.35 | Light grey-pink  |
| Desert varnish on granite    | 0.15-0.25 | Dark brown-black |

Iron oxidation **modestly increases** albedo at red/NIR wavelengths while **decreasing** at blue/UV, producing characteristic reddening.

### 6.2 Mineral-Specific Weathering Rates

| Weathering Process                   | Affected Minerals            | Albedo Change     | Color Shift      |
| ------------------------------------ | ---------------------------- | ----------------- | ---------------- |
| Iron oxidation                       | Olivine, pyroxene, magnetite | +0.04 to +0.15    | Red-brown        |
| Desert varnish (Mn/Fe oxide coating) | Light-colored rocks          | -0.10 to -0.20    | Dark brown-black |
| Feldspar decomposition -> clay       | K-feldspar, plagioclase      | Slight lightening | Pale (kaolinite) |
| Biological crusts (cyanobacteria)    | Desert surfaces              | -0.02 to -0.10    | Darkening        |
| Sulfate/carbonate crusts             | Evaporite environments       | Lightening        | White-yellow     |

### 6.3 Space Weathering on Airless Bodies

On bodies without atmospheres (Moon, Mercury, asteroids), surface modification occurs through micrometeorite bombardment, solar wind irradiation, cosmic rays, and thermal cycling.

**Optical effects from two particle populations:**

- **Nanophase metallic iron (npFe0):** 1-15 nm particles on grain rims. Cause spectral reddening (red-sloped continuum) and weakening of mineral absorption bands. Formed by impacts.
- **Britt-Pieters particles:** 40 nm to ~2 um particles dispersed in soil matrix. Cause overall darkening without significant reddening.

**Net effect on albedo:**

| Body                | Fresh Albedo | Mature Regolith | Change                |
| ------------------- | ------------ | --------------- | --------------------- |
| Moon (highlands)    | 0.20-0.25    | 0.10-0.15       | Darkening             |
| Moon (maria/basalt) | 0.10-0.15    | 0.06-0.08       | Darkening             |
| S-type asteroids    | 0.20-0.30    | 0.10-0.20       | Darkening + reddening |
| Mercury             | --           | 0.07-0.10       | Heavily weathered     |

**Timescales:** Mature lunar soils accumulate ~10^6 to 10^7 years exposure. Fresh crater rays fade as space weathering darkens exposed material over ~1 Gyr.

### 6.4 Procedural Weathering Implementation

```glsl
// Weathering pipeline for albedo modification
vec3 applyWeathering(vec3 baseAlbedo, float ironContent, float weatheringAge,
                     float moistureExposure, float isAirless) {
    vec3 result = baseAlbedo;

    if (isAirless > 0.5) {
        // Space weathering: overall darkening + reddening
        float darkening = weatheringAge * 0.3;
        float reddening = weatheringAge * 0.1;
        result *= (1.0 - darkening);
        result.r += reddening * ironContent;
    } else {
        // Atmospheric weathering: iron oxidation
        float oxidation = ironContent * weatheringAge * moistureExposure;
        result.r += oxidation * 0.15;  // red increase
        result.g += oxidation * 0.05;  // slight brown
        result.b -= oxidation * 0.03;  // blue decrease
    }

    return clamp(result, vec3(0.02), vec3(0.95));
}
```

### 6.5 Albedo Generation Pipeline (Complete)

**Step 1 -- Base albedo by material:**

```
albedo_base(x,y) = lookup_table[material(x,y)]
```

**Step 2 -- Apply weathering modifier:**

```
albedo_weathered = albedo_base * (1 + weathering_factor * delta_albedo)
```

**Step 3 -- Vegetation overlay:**

```
albedo_veg = lerp(albedo_weathered, albedo_vegetation_type, vegetation_coverage)
```

**Step 4 -- Ice/snow:**

```
albedo_final = lerp(albedo_veg, albedo_snow, snow_coverage)
```

**Step 5 -- Spectral variation (per-channel):**

- Basalt: R=0.10, G=0.09, B=0.08 (slight red excess from Fe)
- Granite: R=0.30, G=0.28, B=0.25 (warm grey)
- Vegetation: R=0.10, G=0.20, B=0.05 (green peak, red-edge)
- Desert sand: R=0.45, G=0.35, B=0.25 (iron oxide red-yellow)

---

## 7. Code Examples

### 7.1 Triplanar Mapping (Seamless Sphere Texturing)

```glsl
vec3 triplanarSample(sampler2D tex, vec3 worldPos, vec3 worldNormal, float scale) {
    vec2 uvX = worldPos.zy * scale;
    vec2 uvY = worldPos.xz * scale;
    vec2 uvZ = worldPos.xy * scale;

    // Fix UV mirroring on opposite faces
    if (worldNormal.x < 0.0) uvX.x = -uvX.x;
    if (worldNormal.y < 0.0) uvY.x = -uvY.x;
    if (worldNormal.z < 0.0) uvZ.x = -uvZ.x;

    vec3 blend = abs(worldNormal);
    blend = pow(blend, vec3(4.0));      // sharpen transitions
    blend /= dot(blend, vec3(1.0));     // normalize

    vec3 colX = texture(tex, uvX).rgb;
    vec3 colY = texture(tex, uvY).rgb;
    vec3 colZ = texture(tex, uvZ).rgb;

    return colX * blend.x + colY * blend.y + colZ * blend.z;
}
```

### 7.2 Triplanar Normal Mapping (Three Methods)

**UDN Blend** (cheapest, slight flattening past 45 deg):

```glsl
tnormalX = vec3(tnormalX.xy + worldNormal.zy, worldNormal.x);
tnormalY = vec3(tnormalY.xy + worldNormal.xz, worldNormal.y);
tnormalZ = vec3(tnormalZ.xy + worldNormal.xy, worldNormal.z);
```

**Whiteout Blend** (better accuracy, nearly same cost):

```glsl
tnormalX = vec3(tnormalX.xy + worldNormal.zy, abs(tnormalX.z) * worldNormal.x);
tnormalY = vec3(tnormalY.xy + worldNormal.xz, abs(tnormalY.z) * worldNormal.y);
tnormalZ = vec3(tnormalZ.xy + worldNormal.xy, abs(tnormalZ.z) * worldNormal.z);
```

**Reoriented Normal Mapping (RNM)** (highest quality):

```glsl
vec3 rnmBlendUnpacked(vec3 n1, vec3 n2) {
    n1 += vec3(0.0, 0.0, 1.0);
    n2 *= vec3(-1.0, -1.0, 1.0);
    return n1 * dot(n1, n2) / n1.z - n2;
}
```

Swizzle back to world space after blending:

```glsl
tnormalX = tnormalX.zyx;  // X projection
tnormalY = tnormalY.xzy;  // Y projection
tnormalZ = tnormalZ.xyz;  // Z projection (identity)
```

### 7.3 Height-Weighted Triplanar Blending

```glsl
blend *= lerp(vec3(1.0), vec3(heightX, heightY, heightZ), blendHeightStrength);
blend /= dot(blend, vec3(1.0));
```

Elevates prominent surface features naturally (rocks poke through soil at projection transitions).

### 7.4 Anti-Tiling for Detail Textures

```glsl
vec3 antiTileTriplanar(sampler2D tex, vec3 pos, vec3 normal, float scale) {
    vec3 col1 = triplanarSample(tex, pos, normal, scale);
    vec3 col2 = triplanarSample(tex, pos + vec3(17.3), normal, scale * 0.37);
    float blend = noise(pos * 0.01);
    return mix(col1, col2, blend * 0.3);
}
```

Additional techniques: hash-based tile rotation/flip, multi-frequency blending at 2-3 scales, noise-based UV distortion.

### 7.5 Sobel Normal Map (Compute Shader)

```glsl
layout(local_size_x = 16, local_size_y = 16) in;
layout(r32f, binding = 0) readonly uniform image2D heightMap;
layout(rgba8, binding = 1) writeonly uniform image2D normalMap;
uniform float strength;

void main() {
    ivec2 pos = ivec2(gl_GlobalInvocationID.xy);
    float tl = imageLoad(heightMap, pos + ivec2(-1, -1)).r;
    float tc = imageLoad(heightMap, pos + ivec2( 0, -1)).r;
    float tr = imageLoad(heightMap, pos + ivec2( 1, -1)).r;
    float ml = imageLoad(heightMap, pos + ivec2(-1,  0)).r;
    float mr = imageLoad(heightMap, pos + ivec2( 1,  0)).r;
    float bl = imageLoad(heightMap, pos + ivec2(-1,  1)).r;
    float bc = imageLoad(heightMap, pos + ivec2( 0,  1)).r;
    float br = imageLoad(heightMap, pos + ivec2( 1,  1)).r;

    float Gx = (tr + 2.0 * mr + br) - (tl + 2.0 * ml + bl);
    float Gy = (bl + 2.0 * bc + br) - (tl + 2.0 * tc + tr);
    vec3 normal = normalize(vec3(-Gx * strength, -Gy * strength, 1.0));
    imageStore(normalMap, pos, vec4(normal * 0.5 + 0.5, 1.0));
}
```

### 7.6 GPU Memory Budget (Per Planet)

| System               | Low Quality            | Medium              | High Quality        |
| -------------------- | ---------------------- | ------------------- | ------------------- |
| Heightmap (cubemap)  | 6x512x512 R16 = 3 MB   | 6x1024x1024 = 12 MB | 6x2048x2048 = 48 MB |
| Albedo (cubemap)     | 6x512x512 RGBA8 = 6 MB | 6x1024x1024 = 24 MB | 6x2048x2048 = 96 MB |
| Normal map           | 3 MB                   | 12 MB               | 48 MB               |
| Roughness map        | 1.5 MB (R8)            | 6 MB                | 24 MB               |
| Ocean FFT            | 0.6 MB (128^2)         | 5 MB (256^2)        | 10 MB (512^2)       |
| Cloud noise textures | 8 MB (64^3)            | 20 MB (96^3)        | 34 MB (128^3)       |
| Atmosphere LUTs      | 0.5 MB (FP16)          | 4 MB                | 8-16 MB             |
| **Total**            | **~23 MB**             | **~84 MB**          | **~280 MB**         |

---

## 8. References

### Sphere Parameterization

- [Catlike Coding: Cube Sphere](https://catlikecoding.com/unity/tutorials/procedural-meshes/cube-sphere/)
- [Cube-to-sphere Projections for Procedural Texturing (JCGT 2018)](https://www.jcgt.org/published/0007/02/01/paper.pdf)
- [Making Worlds 1 -- Acko.net](https://acko.net/blog/making-worlds-1-of-spheres-and-cubes/)
- [Red Blob Games: Square Tiling of Sphere](https://www.redblobgames.com/x/1938-square-tiling-of-sphere/)
- [Dimitrijevic & Lambers: Spherical Cube Map Projections Comparison](https://www.researchgate.net/publication/304777429)
- [HEALPix Official Site](https://healpix.sourceforge.io/)
- [Gorski 2005: HEALPix Framework](https://ui.adsabs.harvard.edu/abs/2005ApJ...622..759G/abstract)
- [QSC -- PROJ Documentation](https://proj.org/en/stable/operations/projections/qsc.html)
- [Octahedral Normal Vector Encoding (Narkowicz)](https://knarkowicz.wordpress.com/2014/04/16/octahedron-normal-vector-encoding/)
- [AMD GPUOpen: Fetching from Cubes and Octahedrons](https://gpuopen.com/learn/fetching-from-cubes-and-octahedrons/)
- [Fibonacci Lattice (Extreme Learning)](https://extremelearning.com.au/how-to-evenly-distribute-points-on-a-sphere-more-effectively-than-the-canonical-fibonacci-lattice/)

### PBR Map Generation

- [LearnOpenGL: Parallax Mapping](https://learnopengl.com/Advanced-Lighting/Parallax-Mapping)
- [Advanced Terrain Texture Splatting (Game Developer)](https://www.gamedeveloper.com/programming/advanced-terrain-texture-splatting)
- [Andersson: Terrain Rendering in Frostbite (SIGGRAPH 2007)](https://www.ea.com/frostbite/news/terrain-rendering-in-frostbite-using-procedural-shader-splatting)
- [Ben Golus: Normal Mapping for Triplanar Shader](https://bgolus.medium.com/normal-mapping-for-a-triplanar-shader-10bf39dca05a)
- [Catlike Coding: Triplanar Mapping](https://catlikecoding.com/unity/tutorials/advanced-rendering/triplanar-mapping/)
- [Sapra Projects: Texturing a Procedural World](https://ensapra.com/2023/06/texturing-the-world)
- [Nvidia HBAO (Bavoil)](https://developer.download.nvidia.com/assets/gamedev/files/sdk/11/SSAO11.pdf)
- [Intel XeGTAO](https://github.com/GameTechDev/XeGTAO)
- [KdotJPG: Fast Biome Blending Without Squareness](https://noiseposti.ng/posts/2021-03-13-Fast-Biome-Blending-Without-Squareness.html)

### Materials and Weathering

- [Classification of Igneous Rocks (Geosciences LibreTexts)](https://geo.libretexts.org/Bookshelves/Geology/Book:_An_Introduction_to_Geology/04/4.01)
- [Metamorphic facies (Wikipedia)](https://en.wikipedia.org/wiki/Metamorphic_facies)
- [Sedimentary Rocks (Geosciences LibreTexts)](https://geo.libretexts.org/Bookshelves/Geology/Book:_An_Introduction_to_Geology/05/5.03)
- [Rock Albedo and Thermal Conditions (ResearchGate)](https://www.researchgate.net/publication/227663439)
- [USGS: Spectral Reflectance of Selected Rocks](https://pubs.usgs.gov/publication/70010345)
- [Space Weathering on Airless Bodies (Pieters 2016)](https://agupubs.onlinelibrary.wiley.com/doi/10.1002/2016JE005128)
- [Ferrihydrite in Martian Dust (Nature 2025)](https://www.nature.com/articles/s41467-025-56970-z)

### Noise and Terrain

- [Jadkhoury: Procedural Planet Rendering](https://jadkhoury.github.io/terrain_blog.html)
- [de Carpentier: Scape Procedural Basics](https://www.decarpentier.nl/scape-procedural-basics)
- [Quilez: Domain Warping](https://iquilezles.org/articles/warp/)
- [GPU Gems 2, Ch. 26: Improved Perlin Noise](https://developer.nvidia.com/gpugems/gpugems2/part-iii-high-quality-rendering/chapter-26-implementing-improved-perlin-noise)
- [GPU Gems 3, Ch. 1: Complex Procedural Terrains](https://developer.nvidia.com/gpugems/gpugems3/part-i-geometry/chapter-1-generating-complex-procedural-terrains-using-gpu)
- [SpaceEngine: Terrain Engine Upgrade](https://spaceengine.org/news/blog171230/)
- [Gustavson: WebGL Noise Library](https://stegu.github.io/webgl-noise/webdemo/)
