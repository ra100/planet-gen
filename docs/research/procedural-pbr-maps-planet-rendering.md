# Procedural PBR Map Generation for Planets

> Deep research report -- 28 March 2026
> 12 web searches, 25+ sources consulted; shader snippets included

---

## Table of Contents

1. [Heightmap Generation](#1-heightmap-generation)
2. [Normal Maps from Heightmaps on GPU](#2-normal-maps-from-heightmaps-on-gpu)
3. [Albedo/Color Map Generation](#3-albedocolor-map-generation)
4. [Roughness Map Generation](#4-roughness-map-generation)
5. [Ambient Occlusion from Heightmaps on GPU](#5-ambient-occlusion-from-heightmaps-on-gpu)
6. [Displacement vs Parallax Occlusion Mapping](#6-displacement-vs-parallax-occlusion-mapping)
7. [Splat Map Generation](#7-splat-map-generation)
8. [Detail Texture Tiling and Triplanar Projection](#8-detail-texture-tiling-and-triplanar-projection)
9. [References](#references)

---

## 1. Heightmap Generation

### 1.1 Fractional Brownian Motion (fBm)

fBm sums multiple octaves of coherent noise at increasing frequency and decreasing amplitude [1][2][3]:

```
h(p) = SUM_i  amplitude_i * noise(p * frequency_i)
where  amplitude_i = gain^i,  frequency_i = lacunarity^i
```

Key parameters:
- **Lacunarity**: frequency multiplier per octave (typically ~1.92 to prevent lattice-aligned artifacts; 2.0 is common) [3]
- **Gain** (persistence): amplitude decay per octave (typically 0.5)
- **Octaves**: `log(terrain_width) / log(lacunarity)` for complete detail coverage [3]

fBm alone produces rounded, "blobby" terrain lacking ridges and valleys [1][2].

GLSL pseudocode:
```glsl
float fbm(vec2 p) {
    float sum = 0.0;
    float amp = 1.0;
    float freq = 1.0;
    for (int i = 0; i < OCTAVES; i++) {
        sum += amp * noise(p * freq);
        freq *= lacunarity;
        amp *= gain;
    }
    return sum;
}
```

### 1.2 Ridged Multifractal

Introduced by Musgrave [4], ridged multifractal takes `1 - abs(noise(p))` to create sharp ridges at zero crossings. Critically, each octave's output weights the next octave, so rough areas accumulate detail while flat areas stay smooth:

```glsl
float ridgedMultifractal(vec2 p, float H, float lacunarity,
                         int octaves, float offset, float gain) {
    float sum = 0.0;
    float freq = 1.0;
    float amp = 1.0;
    float weight = 1.0;
    for (int i = 0; i < octaves; i++) {
        float signal = offset - abs(noise(p * freq));
        signal *= signal;       // sharpen ridges
        signal *= weight;       // weight by previous octave
        weight = clamp(signal * gain, 0.0, 1.0);
        sum += signal * amp;
        freq *= lacunarity;
        amp *= pow(lacunarity, -H);  // spectral weight
    }
    return sum;
}
```

### 1.3 Hybrid Multifractal (IQ Noise / Heterogeneous Terrain)

Modifies finer-detail octave amplitudes based on coarser octave output. The key insight is to accumulate pseudo-derivatives and suppress fine detail on slopes [3]:

```glsl
float iqNoise(vec2 p) {
    float sum = 0.0;
    float amp = 1.0;
    float freq = 1.0;
    vec2 dsum = vec2(0.0);
    for (int i = 0; i < OCTAVES; i++) {
        vec3 n = noiseWithDerivatives(p * freq);  // .x = value, .yz = gradient
        dsum += n.yz;
        sum += amp * n.x / (1.0 + dot(dsum, dsum));
        freq *= lacunarity;
        amp *= gain;
    }
    return sum;
}
```

This creates realistic variation: peaks and valleys differ structurally [3].

### 1.4 Combining Multiple Noise Functions

Jadkhoury's planet renderer [1] layers FBM and hybrid multifractal at multiple scales:

```glsl
float getTerrainHeight(vec2 pos) {
    vec2 p = pos * density;
    float b2 = fbm(p * 10.0) * 0.2;
    float h1 = hybridMultifractal(p / 8.0, H, lacunarity, octaves, offset, gain);
    float h2 = hybridMultifractal(p / 3.0, H, lacunarity, octaves, offset, gain / 2.0) * 2.0;
    float h3 = hybridMultifractal(p * 2.0, H, lacunarity, octaves, offset, gain) * 0.3;
    return b2 + h1 + h2 + h3 - 0.8;
}
```

### 1.5 Domain Warping

Popularized by Inigo Quilez [5], domain warping distorts input coordinates with another noise function before evaluation: `f(p) = fbm(p + h(p))`.

**Two-layer warping** (basic):
```glsl
float pattern(vec2 p) {
    vec2 q = vec2(fbm(p + vec2(0.0, 0.0)),
                  fbm(p + vec2(5.2, 1.3)));
    return fbm(p + 4.0 * q);
}
```

**Three-layer warping** (organic, tectonic-like deformation):
```glsl
float pattern(vec2 p, out vec2 q, out vec2 r) {
    q.x = fbm(p + vec2(0.0, 0.0));
    q.y = fbm(p + vec2(5.2, 1.3));
    r.x = fbm(p + 4.0 * q + vec2(1.7, 9.2));
    r.y = fbm(p + 4.0 * q + vec2(8.3, 2.8));
    return fbm(p + 4.0 * r);
}
```

The offset vectors (5.2, 1.3), (1.7, 9.2), (8.3, 2.8) ensure independent fBM samples. The scaling factor 4.0 controls warping intensity. Domain warping creates swirly, displaced terrain features not present in standard fBm [5][6].

### 1.6 Continental Shelf Profiles

For realistic ocean bathymetry, the heightmap must encode the continental shelf transition [7]:

- **Continental shelf**: 0-200 m depth, slope ~0.5 degrees, width ~80 km average
- **Shelf break**: transition at ~120 m depth
- **Continental slope**: 120 m to ~3000 m depth, gradient ~4 degrees (up to 20 degrees)
- **Abyssal plain**: ~3000-6000 m depth, nearly flat

Procedural approach:
```glsl
float continentalProfile(float distFromCoast, float shelfWidth) {
    float t = distFromCoast / shelfWidth;
    if (t < 1.0) {
        // Continental shelf: gentle slope
        return -200.0 * smoothstep(0.0, 1.0, t);
    } else {
        // Continental slope + abyssal plain
        float slopeT = (t - 1.0) / 3.0;
        return -200.0 - 2800.0 * smoothstep(0.0, 1.0, slopeT);
    }
}
```

### 1.7 Billowy and Ridged Turbulence Variants

Two simple modifications to base Perlin noise [3]:

**Billowy turbulence**: `abs(noise(p))` -- creates eroded terrain with sharp creases in valleys.

**Ridged turbulence**: `1.0 - abs(noise(p))` -- inverts creases into sharp ridge lines for mountainous effects.

---

## 2. Normal Maps from Heightmaps on GPU

### 2.1 Central Differences Method

The simplest and most common approach. Sample four neighbors and construct the normal from height gradients [8][9]:

```glsl
// Fragment or compute shader
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

Central differences do not favor positive or negative axes, which is important to ensure normals follow the surface without directional bias [8].

### 2.2 Sobel Filter (3x3)

The Sobel operator provides better noise rejection by weighting diagonal neighbors [9][10]:

```
Gx kernel:             Gy kernel:
[-1  0  1]             [-1 -2 -1]
[-2  0  2]             [ 0  0  0]
[-1  0  1]             [ 1  2  1]
```

GLSL compute shader implementation:
```glsl
layout(local_size_x = 16, local_size_y = 16) in;
layout(r32f, binding = 0) readonly uniform image2D heightMap;
layout(rgba8, binding = 1) writeonly uniform image2D normalMap;

uniform float strength;

void main() {
    ivec2 pos = ivec2(gl_GlobalInvocationID.xy);

    // Sample 3x3 neighborhood
    float tl = imageLoad(heightMap, pos + ivec2(-1, -1)).r;
    float tc = imageLoad(heightMap, pos + ivec2( 0, -1)).r;
    float tr = imageLoad(heightMap, pos + ivec2( 1, -1)).r;
    float ml = imageLoad(heightMap, pos + ivec2(-1,  0)).r;
    float mr = imageLoad(heightMap, pos + ivec2( 1,  0)).r;
    float bl = imageLoad(heightMap, pos + ivec2(-1,  1)).r;
    float bc = imageLoad(heightMap, pos + ivec2( 0,  1)).r;
    float br = imageLoad(heightMap, pos + ivec2( 1,  1)).r;

    // Sobel operator
    float Gx = (tr + 2.0 * mr + br) - (tl + 2.0 * ml + bl);
    float Gy = (bl + 2.0 * bc + br) - (tl + 2.0 * tc + tr);

    vec3 normal = normalize(vec3(-Gx * strength, -Gy * strength, 1.0));

    // Encode to [0,1] range for storage
    imageStore(normalMap, pos, vec4(normal * 0.5 + 0.5, 1.0));
}
```

### 2.3 Scharr Filter (3x3, Higher Accuracy)

Better rotational symmetry than Sobel [9]:

```
Gx kernel:             Gy kernel:
[-3   0   3]           [-3  -10  -3]
[-10  0  10]           [ 0    0   0]
[-3   0   3]           [ 3   10   3]
```

Use the same convolution pattern as Sobel but with these weights. Scharr is preferred when rotational invariance matters (e.g., spherical heightmaps).

### 2.4 5x5 Canny-Based Kernel

For smoother results at the cost of performance, a 5x5 kernel incorporating a Gaussian filter reduces noise sensitivity [9]. The 3x3 Sobel is faster but more noise-sensitive; 5x5 gives smoother normals.

### 2.5 Analytical Derivatives (No Heightmap Needed)

When noise is generated procedurally, analytical gradients can be computed directly, bypassing the heightmap entirely [1][3]:

```glsl
// Perlin noise with analytical derivatives
vec3 noiseWithGradient(vec2 p) {
    // Returns vec3(value, dValue/dx, dValue/dy)
    // Compute gradient alongside value using chain rule
    // ...
}

vec3 proceduralNormal(vec2 p) {
    vec3 n = vec3(0.0);
    float amp = 1.0;
    float freq = 1.0;
    for (int i = 0; i < OCTAVES; i++) {
        vec3 ng = noiseWithGradient(p * freq);
        n.xy += amp * freq * ng.yz;  // accumulate gradient
        amp *= gain;
        freq *= lacunarity;
    }
    return normalize(vec3(-n.x, -n.y, 1.0));
}
```

Benefits: resolution-independent, eliminates texel-grid artifacts, costs ~2x a single noise evaluation [1].

### 2.6 GPU Implementation Strategy

For compute shader normal generation:
1. Generate heightmap into a single-channel texture (R32F or R16F)
2. Dispatch compute shader with one thread per texel
3. Use `imageLoad()` for neighbor sampling (avoids texture filtering overhead)
4. Store result in RGBA8 texture (XYZ normal + spare channel for height or AO)
5. Use group-shared memory for the 3x3 neighborhood if memory bandwidth is a bottleneck

---

## 3. Albedo/Color Map Generation

### 3.1 Biome Classification Pipeline

The standard approach classifies each texel by terrain properties [11][12][13]:

1. **Height zones**: ocean, shore, lowland, highland, alpine, snow
2. **Slope detection**: steep slopes -> exposed rock regardless of elevation
3. **Latitude**: polar, temperate, tropical zones (via dot product with pole axis)
4. **Moisture/temperature**: humidity and temperature maps sampled alongside the heightmap determine biome [13]

```glsl
vec3 biomeColor(float height, float slope, float latitude, float moisture) {
    // Classify biome
    int biome = GRASSLAND;
    if (height < seaLevel) biome = OCEAN;
    else if (slope > 0.7) biome = ROCK;
    else if (height > snowLine) biome = SNOW;
    else if (moisture < 0.2) biome = DESERT;
    else if (moisture > 0.7 && abs(latitude) < 0.4) biome = TROPICAL_FOREST;
    else if (abs(latitude) > 0.7) biome = TUNDRA;

    // Look up biome gradient texture
    return texture(biomeGradient, vec2(moisture, height)).rgb;
}
```

### 3.2 Biome Gradient Textures

A 2D gradient texture where X = moisture/temperature and Y = elevation serves as a lookup table [12]. Each row corresponds to an elevation band, each column to a climate zone. The biome colors are artist-defined but constrained to physically plausible PBR albedo ranges:

| Surface         | Albedo (linear) |
|-----------------|-----------------|
| Deep ocean      | 0.02-0.06       |
| Shallow water   | 0.06-0.12       |
| Desert sand     | 0.30-0.40       |
| Vegetation      | 0.10-0.20       |
| Fresh snow      | 0.80-0.90       |
| Bare rock       | 0.15-0.30       |
| Tropical forest | 0.08-0.15       |

PBR albedo values must be in linear color space [12].

### 3.3 Slope and Height-Based Blending

Combine height and slope with smooth transitions [1][11]:

```glsl
vec3 terrainColor(float height, vec3 normal) {
    float slope = 1.0 - normal.y;  // 0 = flat, 1 = vertical
    float h = height;

    vec3 snow   = vec3(0.9, 0.9, 0.95);
    vec3 rock   = vec3(0.25, 0.22, 0.20);
    vec3 grass  = vec3(0.12, 0.18, 0.08);
    vec3 sand   = vec3(0.35, 0.32, 0.25);

    // Height-based base color
    vec3 color = mix(sand, grass, smoothstep(0.0, 0.1, h));
    color = mix(color, rock, smoothstep(0.4, 0.6, h));
    color = mix(color, snow, smoothstep(0.7, 0.85, h));

    // Slope override: steep = rock
    color = mix(color, rock, smoothstep(0.4, 0.7, slope));

    return color;
}
```

### 3.4 SpaceEngine Approach

SpaceEngine [14] uses biome configuration files that define permitted materials per planet type. The color of each texel on a surface patch is analyzed, and the system assigns suitable materials by matching the color to target values for each biome (e.g., green -> grass, yellow -> sand). Colors can be adjusted per biome, and both large-scale (satellite-view) and small-scale (terrain detail) textures are automatically re-colored.

### 3.5 Noise-Based Biome Boundary Irregularity

To avoid straight-line biome boundaries [12][13]:
```glsl
float biomeBoundary = height + noise(pos * 0.01) * 0.05;
// Use biomeBoundary instead of raw height for classification
```

Multiplying noise with the classification parameter before lookup creates irregular, natural-looking biome transitions.

---

## 4. Roughness Map Generation

### 4.1 Terrain Type to Roughness Mapping

Roughness defines microsurface scattering: 0.0 = mirror smooth, 1.0 = fully diffuse. Reference values for terrain types:

| Surface Type     | Roughness Range |
|-----------------|-----------------|
| Calm water       | 0.02-0.05       |
| Wet rock         | 0.15-0.25       |
| Dry rock         | 0.40-0.60       |
| Sand             | 0.70-0.85       |
| Fresh snow       | 0.80-0.95       |
| Vegetation/grass | 0.60-0.80       |
| Ice              | 0.05-0.15       |
| Mud/clay         | 0.50-0.70       |
| Volcanic rock    | 0.60-0.80       |

### 4.2 Procedural Roughness Derivation

Roughness maps for planets can be derived from terrain properties rather than authored manually:

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

Factors that modulate roughness:
- **Height**: lower elevation (wet areas) -> smoother; higher -> rougher
- **Slope**: steep slopes (exposed rock) -> medium roughness
- **Curvature**: convex ridges -> more weathered (rougher); concave valleys -> sediment accumulation (smoother)
- **Ambient occlusion**: dark crevices act as a proxy for moisture -> smoother
- **Erosion data**: stream-power-law erosion output directly informs roughness

### 4.3 Ocean/Land Roughness Masking

The ocean mask from the heightmap directly controls roughness [14]:
```glsl
float roughness = isOcean ? oceanRoughness : landRoughness;
// oceanRoughness: ~0.02 (calm) to ~0.3 (storm waves)
// Driven by wind speed parameter or animated noise
```

### 4.4 Blender Node-Based Approach

In tools like Blender, roughness maps are separated using ColorRamp nodes: different terrain types (ocean, land, mountain) are isolated via height thresholds, each mapped to a distinct roughness value, then combined [15].

---

## 5. Ambient Occlusion from Heightmaps on GPU

### 5.1 Simple Neighborhood Average Method

The simplest heightmap AO: if the average height of the local neighborhood exceeds the center pixel, it is occluded [16][17]:

```glsl
float heightmapAO(sampler2D heightMap, vec2 uv, float radius, int samples) {
    float centerH = texture(heightMap, uv).r;
    float occlusion = 0.0;
    for (int i = 0; i < samples; i++) {
        vec2 offset = randomDirection(i) * radius;
        float neighborH = texture(heightMap, uv + offset).r;
        occlusion += max(0.0, neighborH - centerH);
    }
    return 1.0 - clamp(occlusion / float(samples), 0.0, 1.0);
}
```

### 5.2 Horizon-Based Ambient Occlusion (HBAO)

HBAO, developed by Nvidia [18], treats the depth/height buffer as a height field and integrates local visibility per fragment by raymarching against it:

1. For each fragment, cast rays in N directions (typically 4-8) along the height field
2. For each direction, march M steps (typically 4-8) to find the maximum horizon angle
3. The horizon angle determines how much sky is visible from that point
4. Accumulate occlusion as (1 - visible_sky_fraction) across all directions

```glsl
float hbaoFromHeightmap(sampler2D heightMap, vec2 uv, vec2 texelSize) {
    float centerH = texture(heightMap, uv).r;
    float ao = 0.0;
    const int NUM_DIRECTIONS = 8;
    const int NUM_STEPS = 6;
    float sampleRadius = 10.0;  // in texels

    for (int d = 0; d < NUM_DIRECTIONS; d++) {
        float angle = float(d) * PI * 2.0 / float(NUM_DIRECTIONS);
        vec2 dir = vec2(cos(angle), sin(angle));

        float maxHorizon = -1.0;  // horizon angle (tangent)
        for (int s = 1; s <= NUM_STEPS; s++) {
            vec2 sampleUV = uv + dir * texelSize * float(s) * sampleRadius / float(NUM_STEPS);
            float sampleH = texture(heightMap, sampleUV).r;
            float dh = sampleH - centerH;
            float dist = float(s) * sampleRadius / float(NUM_STEPS);
            float horizon = dh / dist;  // tangent of elevation angle
            maxHorizon = max(maxHorizon, horizon);
        }
        ao += clamp(maxHorizon, 0.0, 1.0);
    }
    return 1.0 - ao / float(NUM_DIRECTIONS);
}
```

### 5.3 Line-Sweep AO for Heightfields

Sweep over the height field in several directions (e.g., 8-16) to compute a sky visibility factor [19]:
1. For each direction, iterate pixels in scanline order
2. Maintain a running maximum horizon angle
3. At each pixel, compare current height to the horizon
4. Visibility = acos(horizon_angle) / PI

This is O(N) per scanline (amortized), making it efficient for offline baking.

### 5.4 Compute Shader Implementation

Both HBAO and blur passes map naturally to compute shaders using group-shared memory [18]:

```glsl
layout(local_size_x = 16, local_size_y = 16) in;

shared float sharedHeights[18][18];  // 16x16 + 1-texel border

void main() {
    ivec2 gid = ivec2(gl_GlobalInvocationID.xy);
    ivec2 lid = ivec2(gl_LocalInvocationID.xy);

    // Load center + border into shared memory
    sharedHeights[lid.y + 1][lid.x + 1] = imageLoad(heightMap, gid).r;
    // Load border texels (elided for brevity)
    barrier();

    // Compute HBAO using sharedHeights instead of texture loads
    // ... (same algorithm as above, reading from shared memory)
}
```

### 5.5 Ground Truth AO (GTAO)

Intel's XeGTAO [20] separates the hemisphere into "slices" and takes linear samples while rotating around the surface normal. Implemented in compute shader passes: PrefilterDepths -> MainPass -> Denoise. Also computes bent normals.

### 5.6 Raymarching AO from Heightmap

Cast rays from each surface point toward the hemisphere and march against the heightmap [21]:

```glsl
float raymarchAO(sampler2D heightMap, vec2 uv, vec3 surfaceNormal) {
    float centerH = texture(heightMap, uv).r;
    float ao = 0.0;
    const int NUM_RAYS = 16;
    for (int i = 0; i < NUM_RAYS; i++) {
        vec3 rayDir = cosineWeightedHemisphere(surfaceNormal, i, NUM_RAYS);
        float occlusion = 0.0;
        for (int step = 1; step <= 8; step++) {
            vec2 samplePos = uv + rayDir.xy * float(step) * texelSize;
            float expectedH = centerH + rayDir.z * float(step) * heightScale;
            float actualH = texture(heightMap, samplePos).r;
            if (actualH > expectedH) {
                occlusion = 1.0;
                break;
            }
        }
        ao += occlusion;
    }
    return 1.0 - ao / float(NUM_RAYS);
}
```

---

## 6. Displacement vs Parallax Occlusion Mapping

### 6.1 True Displacement Mapping

Displaces actual geometry vertices based on a heightmap. Requires tessellation hardware [22]:

**Pros**:
- Correct silhouettes at all viewing angles
- Proper self-shadowing and inter-object occlusion
- Correct depth buffer values for post-processing (DoF, SSAO)
- Works with all lighting models

**Cons**:
- Requires tessellation (hull + domain shaders or mesh shaders)
- Performance scales with triangle count
- LOD management needed to avoid over-tessellation
- Not available on all hardware/APIs (e.g., WebGL lacks tessellation)

Typical tessellation approach [22]:
```glsl
// Domain shader: displace tessellated vertices
float height = texture(heightMap, uv).r;
vec3 displaced = position + normal * height * displacementScale;
```

### 6.2 Basic Parallax Mapping

Offsets texture coordinates based on view direction and height value [22]:

```glsl
vec2 parallaxMapping(vec2 texCoords, vec3 viewDir) {
    float height = texture(depthMap, texCoords).r;
    vec2 p = viewDir.xy / viewDir.z * (height * heightScale);
    return texCoords - p;
}
```

Division by `viewDir.z` automatically scales displacement by viewing angle. Single texture sample, minimal overhead, but breaks at steep angles [22].

### 6.3 Steep Parallax Mapping

Divides depth range into layers and iteratively marches through them [22]:

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

Optimization: dynamically adjust layer count by viewing angle (fewer samples looking straight down, more at grazing angles).

### 6.4 Parallax Occlusion Mapping (POM)

Refines steep parallax with linear interpolation between the two layers bracketing the intersection [22]:

```glsl
// After steep parallax loop finds intersection...
vec2 prevTexCoords = currentTexCoords + deltaTexCoords;

float afterDepth  = currentDepthMapValue - currentLayerDepth;
float beforeDepth = texture(depthMap, prevTexCoords).r
                    - currentLayerDepth + layerDepth;

float weight = afterDepth / (afterDepth - beforeDepth);
vec2 finalTexCoords = prevTexCoords * weight
                    + currentTexCoords * (1.0 - weight);
```

### 6.5 Performance Comparison

| Technique               | Samples/pixel | Silhouettes | Depth correct | Best for          |
|------------------------|---------------|-------------|---------------|-------------------|
| Basic parallax         | 1             | No          | No            | Subtle bumps      |
| Steep parallax         | 8-32          | No          | No            | Medium detail      |
| POM                    | 8-32          | No          | No            | High detail, flat  |
| Tessellation + displace| N/A (geometry)| Yes         | Yes           | Close-up terrain   |

### 6.6 Recommendation for Planets

For planetary terrain:
- **Far view**: Normal maps only (no parallax needed)
- **Medium view**: POM for terrain detail layers, especially rock and cliff faces
- **Close view**: Tessellation + displacement for geometry-correct rendering
- Use LOD transitions to blend between techniques based on camera distance

POM artifacts are most visible at grazing angles and surface edges. Discarding fragments where displaced UVs fall outside [0,1] mitigates edge artifacts [22].

---

## 7. Splat Map Generation

### 7.1 What is a Splat Map

A splat map (alpha map / weight map) is a texture that does not render directly but stores per-texel weights indicating which material should appear at each point on the terrain [23][24]. RGBA channels encode weights for up to 4 materials per splat texture. Multiple splat textures can be used for more materials.

### 7.2 Procedural Splat Map Generation

Generate splat weights from terrain properties rather than painting manually:

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

    // Normalize so weights sum to 1
    weights /= max(dot(weights, vec4(1.0)), 0.001);
    return weights;
}
```

### 7.3 SpaceEngine's Approach

SpaceEngine [14] generates splat maps by analyzing the surface color texture: the shader examines each pixel's color and slope, matches to known biome materials via a configuration file, selects the 4 most suitable materials, and outputs material IDs + weights as two splat textures. The rendering shader then uses these to blend materials.

### 7.4 Frostbite Procedural Shader Splatting

Andersson's SIGGRAPH 2007 presentation [25] introduced procedural shader splatting for Frostbite: compute terrain material assignment dynamically in shaders based on terrain topology, rather than storing static splat textures. Dynamic flow control skips texture fetches in areas fully covered by other materials for performance.

### 7.5 Height-Based Blending (Advanced)

Simple linear blending produces unrealistic mud-like transitions. Height-based blending uses a depth map stored in each material texture's alpha channel [23]:

**Simple linear blend** (poor quality):
```glsl
vec3 color = tex1.rgb * a1 + tex2.rgb * a2;
```

**Height-based selection** (hard transition):
```glsl
vec3 color = (tex1.a + a1 > tex2.a + a2) ? tex1.rgb : tex2.rgb;
```

**Depth-filtered blend** (best quality -- smooth, natural transitions):
```glsl
vec3 heightBlend(vec4 tex1, float a1, vec4 tex2, float a2) {
    float depth = 0.2;  // blend depth
    float ma = max(tex1.a + a1, tex2.a + a2) - depth;
    float b1 = max(tex1.a + a1 - ma, 0.0);
    float b2 = max(tex2.a + a2 - ma, 0.0);
    return (tex1.rgb * b1 + tex2.rgb * b2) / (b1 + b2);
}
```

This simulates realistic material settling (sand fills cracks in stone, grass grows on flat areas between rocks) [23].

### 7.6 Memory Optimization

Pack 4 alpha maps into one RGBA texture. With texture arrays, materials can reference a shared atlas, reducing draw calls and state changes.

---

## 8. Detail Texture Tiling and Triplanar Projection

### 8.1 The Tiling Problem on Spheres

Standard UV mapping on spheres creates severe distortion at poles and seam artifacts along the 0/360-degree meridian. 2D noise textures projected via equirectangular mapping show obvious stretching at high latitudes [1][26].

### 8.2 Triplanar Projection Fundamentals

Triplanar mapping samples a texture three times along X, Y, Z world axes and blends based on surface normal orientation [26][27]:

```glsl
vec3 triplanarSample(sampler2D tex, vec3 worldPos, vec3 worldNormal, float scale) {
    // UV coordinates from world position
    vec2 uvX = worldPos.zy * scale;  // X-facing plane
    vec2 uvY = worldPos.xz * scale;  // Y-facing plane
    vec2 uvZ = worldPos.xy * scale;  // Z-facing plane

    // Blend weights from absolute normal
    vec3 blend = abs(worldNormal);
    blend = pow(blend, vec3(4.0));      // sharpen transitions
    blend /= dot(blend, vec3(1.0));     // normalize to sum=1

    // Sample and blend
    vec3 colX = texture(tex, uvX).rgb;
    vec3 colY = texture(tex, uvY).rgb;
    vec3 colZ = texture(tex, uvZ).rgb;

    return colX * blend.x + colY * blend.y + colZ * blend.z;
}
```

### 8.3 Fixing UV Mirroring

Single projections create mirrored textures on opposite-facing surfaces. Fix by conditionally negating U coordinates [26][27]:

```glsl
if (worldNormal.x < 0.0) uvX.x = -uvX.x;
if (worldNormal.y < 0.0) uvY.x = -uvY.x;
if (worldNormal.z < 0.0) uvZ.x = -uvZ.x;
```

### 8.4 Triplanar Normal Mapping

The naive approach of using mesh tangents for triplanar produces incorrect normals. Three correct blending methods exist [26]:

**UDN Blend** (fast, slightly flattened at >45 degrees):
```glsl
tnormalX = vec3(tnormalX.xy + worldNormal.zy, worldNormal.x);
tnormalY = vec3(tnormalY.xy + worldNormal.xz, worldNormal.y);
tnormalZ = vec3(tnormalZ.xy + worldNormal.xy, worldNormal.z);
```

**Whiteout Blend** (better detail retention):
```glsl
tnormalX = vec3(tnormalX.xy + worldNormal.zy,
                abs(tnormalX.z) * worldNormal.x);
tnormalY = vec3(tnormalY.xy + worldNormal.xz,
                abs(tnormalY.z) * worldNormal.y);
tnormalZ = vec3(tnormalZ.xy + worldNormal.xy,
                abs(tnormalZ.z) * worldNormal.z);
```

**Reoriented Normal Mapping (RNM)** (highest quality, closest to ground truth):
```glsl
vec3 rnmBlendUnpacked(vec3 n1, vec3 n2) {
    n1 += vec3(0.0, 0.0, 1.0);
    n2 *= vec3(-1.0, -1.0, 1.0);
    return n1 * dot(n1, n2) / n1.z - n2;
}
```

Normal component swizzling must match UV swizzling [26]:
```glsl
// After blending, swizzle back to world space
tnormalX = tnormalX.zyx;  // X projection
tnormalY = tnormalY.xzy;  // Y projection
tnormalZ = tnormalZ.xyz;  // Z projection (identity)
```

### 8.5 Blend Sharpening

Two approaches to control blend transitions [26][27]:

**Offset method**: subtract a constant before normalization, concentrating influence to dominant faces.

**Power method** (preferred):
```glsl
vec3 blend = pow(abs(worldNormal), vec3(blendExponent));
blend /= dot(blend, vec3(1.0));
```

Higher exponent (4-8) creates sharper transitions; lower (1-2) creates softer blends.

### 8.6 Height-Weighted Triplanar Blending

Use height data from material textures (e.g., MOHS: Metallic-Occlusion-Height-Smoothness) to influence blend weights [27]:

```glsl
blend *= lerp(vec3(1.0), vec3(heightX, heightY, heightZ), blendHeightStrength);
blend /= dot(blend, vec3(1.0));
```

This elevates prominent surface features naturally -- e.g., rocks poke through soil at projection transitions.

### 8.7 Detail Texture Anti-Tiling

To reduce visible tiling repetition in detail textures:

1. **Offset projections**: shift the X mapping by 0.5 vertically to eliminate repetition between X and Z projections [27]
2. **Hash-based variation**: use a hash of the tile position to rotate or flip each tile instance
3. **Multi-frequency blending**: sample the same texture at 2-3 different scales and blend by distance
4. **Noise-based UV distortion**: slightly warp UVs with low-frequency noise to break up grid alignment

```glsl
vec3 antiTileTriplanar(sampler2D tex, vec3 pos, vec3 normal, float scale) {
    // Primary sample
    vec3 col1 = triplanarSample(tex, pos, normal, scale);
    // Secondary sample at different scale + offset
    vec3 col2 = triplanarSample(tex, pos + vec3(17.3), normal, scale * 0.37);
    // Blend based on distance or noise
    float blend = noise(pos * 0.01);
    return mix(col1, col2, blend * 0.3);
}
```

### 8.8 Triplanar on Spheres: Solving Pole Distortion

For 2D noise on spheres, triplanar variants of noise types (gradient, Voronoi, simplex) generate noise on the X, Y, Z planes separately and combine them, forming a kind of 3D noise that works perfectly on spheres without pole distortion [1][14].

Alternatively, use true 3D noise evaluated at the world-space position of each vertex/fragment on the sphere, which is inherently free of parameterization artifacts but costs more than 2D noise.

---

## References

1. Jadkhoury, "Procedural Planet Rendering" -- [jadkhoury.github.io/terrain_blog.html](https://jadkhoury.github.io/terrain_blog.html)
2. "The Book of Shaders: Fractal Brownian Motion" -- [thebookofshaders.com/13/](https://thebookofshaders.com/13/)
3. de Carpentier, "Scape: Procedural Basics" -- [decarpentier.nl/scape-procedural-basics](https://www.decarpentier.nl/scape-procedural-basics)
4. Musgrave, "Procedural Fractal Terrains" -- [classes.cs.uchicago.edu/.../MusgraveTerrain00.pdf](https://www.classes.cs.uchicago.edu/archive/2015/fall/23700-1/final-project/MusgraveTerrain00.pdf)
5. Quilez, "Domain Warping" -- [iquilezles.org/articles/warp/](https://iquilezles.org/articles/warp/)
6. 3DWorldGen, "Domain Warping Noise" -- [3dworldgen.blogspot.com/2017/05/domain-warping-noise.html](http://3dworldgen.blogspot.com/2017/05/domain-warping-noise.html)
7. "The Topography of the Sea Floor" (Geosciences LibreTexts) -- [geo.libretexts.org/.../19.01](https://geo.libretexts.org/Courses/Sierra_College/Physical_Geology_(Sierra_College_Edition)/19:_Geology_of_the_Oceans/19.01:_The_Topography_of_the_Sea_Floor)
8. GameDev.net, "Create a normal map from Heightmap in a pixel shader?" -- [gamedev.net/forums/topic/428776](https://www.gamedev.net/forums/topic/428776-create-a-normal-map-from-heightmap-in-a-pixel-shader/)
9. GameDev.net, "Calculate Normals from a displacement map" -- [gamedev.net/forums/topic/594457](https://gamedev.net/forums/topic/594457-calculate-normals-from-a-displacement-map/)
10. Unity Discussions, "Sobel operator - height to normal map on GPU" -- [forum.unity.com/threads/sobel-operator...33159](https://forum.unity.com/threads/sobel-operator-height-to-normal-map-on-gpu.33159/)
11. Coster, "Unity Shader Graph Procedural Planet Tutorial" -- [timcoster.com/2020/09/03/unity-shader-graph-procedural-planet-tutorial](https://timcoster.com/2020/09/03/unity-shader-graph-procedural-planet-tutorial/)
12. Baltaci, "Procedural World Generation with Biomes in Unity" -- [medium.com/@mrrsff/procedural-world-generation-with-biomes](https://medium.com/@mrrsff/procedural-world-generation-with-biomes-in-unity-a474e11ff0b7)
13. Red Blob Games, "Procedural map generation on a sphere" -- [redblobgames.com/x/1843-planet-generation](https://www.redblobgames.com/x/1843-planet-generation/)
14. SpaceEngine, "Terrain engine upgrade #4" -- [spaceengine.org/news/blog171230](https://spaceengine.org/news/blog171230/)
15. Polycount, "How do you create Roughness maps for PBR?" -- [polycount.com/discussion/134289](https://polycount.com/discussion/134289/how-do-you-guys-create-roughness-maps-for-pbr-any-examples-also-a-few-questions)
16. Amazon Lumberyard, "Heightmap Ambient Occlusion" -- [github.com/awsdocs/amazon-lumberyard-user-guide/.../mat-shaders-heightmap_ambient_occlusion.md](https://github.com/awsdocs/amazon-lumberyard-user-guide/blob/master/doc_source/mat-shaders-heightmap_ambient_occlusion.md)
17. Nvidia, "GPU Gems 3: Chapter 12 - High-Quality Ambient Occlusion" -- [developer.nvidia.com/gpugems/gpugems3/.../chapter-12](https://developer.nvidia.com/gpugems/gpugems3/part-ii-light-and-shadows/chapter-12-high-quality-ambient-occlusion)
18. Nvidia, "HBAO using Compute Shaders" (Bavoil) -- [developer.download.nvidia.com/.../SSAO11.pdf](https://developer.download.nvidia.com/assets/gamedev/files/sdk/11/SSAO11.pdf)
19. Naaji, "Line-Sweep Ambient Occlusion" -- [karim.naaji.fr/lsao.html](https://karim.naaji.fr/lsao.html)
20. Intel, "XeGTAO" -- [github.com/GameTechDev/XeGTAO](https://github.com/GameTechDev/XeGTAO)
21. GLSL HBAO Fragment Shader (Guido Schmidt) -- [gist.github.com/guidoschmidt/a84a2bc64a93833ecaf0b08835efbdee](https://gist.github.com/guidoschmidt/a84a2bc64a93833ecaf0b08835efbdee)
22. LearnOpenGL, "Parallax Mapping" -- [learnopengl.com/Advanced-Lighting/Parallax-Mapping](https://learnopengl.com/Advanced-Lighting/Parallax-Mapping)
23. "Advanced Terrain Texture Splatting" -- [gamedeveloper.com/programming/advanced-terrain-texture-splatting](https://www.gamedeveloper.com/programming/advanced-terrain-texture-splatting)
24. Wikipedia, "Texture splatting" -- [en.wikipedia.org/wiki/Texture_splatting](https://en.wikipedia.org/wiki/Texture_splatting)
25. Andersson, "Terrain Rendering in Frostbite Using Procedural Shader Splatting" (SIGGRAPH 2007) -- [ea.com/frostbite/news/terrain-rendering-in-frostbite-using-procedural-shader-splatting](https://www.ea.com/frostbite/news/terrain-rendering-in-frostbite-using-procedural-shader-splatting)
26. Golus, "Normal Mapping for a Triplanar Shader" -- [bgolus.medium.com/normal-mapping-for-a-triplanar-shader-10bf39dca05a](https://bgolus.medium.com/normal-mapping-for-a-triplanar-shader-10bf39dca05a)
27. Catlike Coding, "Triplanar Mapping" -- [catlikecoding.com/unity/tutorials/advanced-rendering/triplanar-mapping](https://catlikecoding.com/unity/tutorials/advanced-rendering/triplanar-mapping/)
