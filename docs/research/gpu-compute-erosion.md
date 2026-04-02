# GPU Compute: Noise, Erosion & Shader Optimization -- Consolidated Deep Dive

_Consolidated research from gpu-noise-erosion-compute.md, researcher-b.md, procedural-terrain-generation-final.md, and planet-topography-generation.md. Focuses on unique deep-dive content not covered in the parent final.md overview._

_Research date: 2026-03-28 (consolidated 2026-04-02)_

---

## Executive Summary

This document consolidates GPU-specific implementation details for procedural planet generation: noise algorithm internals with GLSL/WGSL code, hash function design for GPU rendering, hydraulic and thermal erosion simulation on compute shaders, race condition strategies, and compute optimization techniques. It complements `procedural-terrain-generation-final.md` (which covers the 8-step terrain pipeline, power spectra, hypsometric curves, basic fBm parameters, and stream power law) by diving deeper into the GPU implementation layer.

**Key findings:**

- Simplex noise achieves O(n^2) vs Perlin's O(n\*2^n) complexity in n dimensions, with zero texture memory via permutation polynomials (Gustavson & McEwan 2012)
- GPU erosion race conditions can be safely ignored: single float buffer with extra iterations is faster than correctness mechanisms (Paris)
- 8-octave simplex fBm on 4096x4096 takes ~0.8 ms on RTX 3060-class hardware; 500-step hydraulic erosion on 1024x1024 takes ~1-2.5 seconds
- WGSL ports of all major noise algorithms exist and perform within 5-15% of native Vulkan

---

## Table of Contents

1. [Noise Algorithms: Deep Comparison](#1-noise-algorithms-deep-comparison)
2. [Hash Functions for GPU Noise](#2-hash-functions-for-gpu-noise)
3. [Noise Filtering & Artifact Reduction](#3-noise-filtering--artifact-reduction)
4. [3D Noise vs 2D Noise Trade-offs](#4-3d-noise-vs-2d-noise-trade-offs)
5. [Hydraulic Erosion Simulation](#5-hydraulic-erosion-simulation)
6. [Thermal Erosion Simulation](#6-thermal-erosion-simulation)
7. [Particle-Based Erosion on GPU](#7-particle-based-erosion-on-gpu)
8. [Compute Shader Optimization](#8-compute-shader-optimization)
9. [Code Examples](#9-code-examples)
10. [Benchmarks & Performance](#10-benchmarks--performance)
11. [References](#11-references)

---

## 1. Noise Algorithms: Deep Comparison

### 1.1 Simplex vs Perlin vs Worley -- Complexity & Isotropy

| Property              | Perlin (Improved 2002)                                              | Simplex (2001)                                              | Worley / Cellular (1996)                         |
| --------------------- | ------------------------------------------------------------------- | ----------------------------------------------------------- | ------------------------------------------------ |
| Grid type             | Hypercubic lattice                                                  | Simplex lattice (triangles/tetrahedra)                      | Jittered point grid                              |
| Samples per eval (2D) | 4 corners                                                           | 3 corners                                                   | 9 cells (3x3) or 4 cells (2x2 optimized)         |
| Samples per eval (3D) | 8 corners                                                           | 4 corners                                                   | 27 cells (3x3x3) or 8 cells (2x2x2 opt.)         |
| Complexity (n-dim)    | **O(n \* 2^n)**                                                     | **O(n^2)**                                                  | O(k) where k = search neighborhood size          |
| Directional artifacts | Axis-aligned bias (improved version reduces but does not eliminate) | Visually isotropic, no directional bias                     | None (point-based)                               |
| Gradient computation  | Analytical (from dot products)                                      | Analytical, cheaply computed nearly everywhere              | Numerical (finite differences on distance field) |
| Output range (2D)     | ~[-0.7, 0.7] practical                                              | [-1, 1] normalized                                          | [0, max_dist] (F1), varies (F2-F1)               |
| Patent status         | None                                                                | Expired January 2022 (US6867776B2)                          | None                                             |
| Best variant          | Improved Perlin (quintic fade `6t^5 - 15t^4 + 10t^3`)               | OpenSimplex2(F) for general fBm, OpenSimplex2(S) for ridged | 2x2 optimized (BrianSharpe)                      |

**Simplex advantage in higher dimensions:** In 3D, simplex evaluates 4 gradient samples vs Perlin's 8, and in 4D it evaluates 5 vs 16. This makes simplex the clear winner for 3D noise on sphere surfaces.

**OpenSimplex2 variants:**

| Variant         | Character                                 | Best For                                             |
| --------------- | ----------------------------------------- | ---------------------------------------------------- |
| OpenSimplex2(F) | Looks most like Simplex, comparable speed | General fBm, standard terrain                        |
| OpenSimplex2(S) | Smoother profile                          | Ridged noise (abs value thresholding handles better) |

OpenSimplex2 uses a rotated body-centered-cubic grid in 3D (offset union of two rotated cubic grids), improving probability symmetry in gradient vector tables and eliminating diagonal banding artifacts beyond 3D. Available in Java, C#, C, GLSL, HLSL under CC0 license.

### 1.2 Gabor Noise -- Anisotropic Terrain Features

Gabor noise (Lagae et al. 2009) uses sparse convolution with Gabor kernels for precise spectral and directional control:

**Mathematical formulation:**

```
g(x) = K * exp(-pi * a^2 * |x|^2) * cos(2*pi * F0 . x + phi)
```

Where: K = magnitude, a = bandwidth (Gaussian envelope width), F0 = frequency vector (direction + principal frequency), phi = phase offset. Gabor noise sums Poisson-distributed impulses convolved with these kernels.

**Terrain-relevant parameters:**

- `sector_angle`: Dominant direction (0 = horizontal ridges, PI/4 = diagonal dunes)
- `sector_width`: 0 = perfectly aligned features, PI = isotropic
- `bandwidth`: Low = smooth broad features, high = sharp detail
- `Jacobian matrix`: Proper filtering when mapped onto curved surfaces (critical for planet rendering)

**Performance:** 5-20x slower than simplex for equivalent resolution. Cost scales with `density * 9_cells * avg_impulses_per_cell`. Higher register pressure (~20-30 registers per thread). Best used selectively for specific layers (sand dunes, river erosion patterns, geological strata), not as the base heightmap.

### 1.3 Value Noise

The simplest coherent noise: random scalar values at lattice points, interpolated (bilinear, bicubic, or hermite). Faster than gradient noise but produces blocky appearance due to axis-aligned interpolation. Frequency content clusters around lattice alignment. Not recommended for terrain base layers.

### 1.4 Worley / Cellular Noise -- Distance Metrics

Beyond basic F1/F2, the distance metric dramatically changes the output character:

| Metric       | Formula                                     | Cell Shape                   |
| ------------ | ------------------------------------------- | ---------------------------- |
| Euclidean    | `sqrt(dx*dx + dy*dy)`                       | Smooth organic cells         |
| Manhattan    | `abs(dx) + abs(dy)`                         | Diamond-shaped, jagged       |
| Chebyshev    | `max(abs(dx), abs(dy))`                     | Square cells                 |
| Minkowski(p) | `pow(pow(abs(dx),p) + pow(abs(dy),p), 1/p)` | Interpolates between metrics |

**Combination patterns for terrain:**

- **F1**: Voronoi cell interiors (volcanic craters)
- **F2**: Rounded cell shapes
- **F2 - F1**: Cell edges / cracks (dried mud, lava cracks, tectonic plate edges)
- **F1 \* F2**: Organic patterns

**Optimized 2x2 approach (BrianSharpe/Gustavson):** Reduces jitter from +/-0.5 to +/-0.25 (2D) or +/-0.1666 (3D), applies cubic weighting to push points toward extremes. Result: 2.25x fewer distance calculations in 2D, 3.375x fewer in 3D.

---

## 2. Hash Functions for GPU Noise

### 2.1 Permutation Polynomial (Gustavson & McEwan 2012)

The definitive GPU noise implementation eliminates all texture lookups using purely computational hashing:

```glsl
vec3 permute(vec3 x) {
    return mod(((x * 34.0) + 1.0) * x, 289.0);
}
```

**Key properties:**

- `(34x^2 + x) mod 289` is a permutation polynomial over Z/289Z
- Computed purely in ALU -- zero texture lookups, zero shared memory
- Replaces the traditional 256-entry permutation table used in CPU implementations
- Pairs with cross-polytope gradient mapping: maps points onto N-dimensional octahedron surfaces to select gradients computationally
- Uses rank-ordering via pairwise component comparisons (warp-friendly, no branching)

### 2.2 Jarzynski & Olano (2020) -- Hash Function Quality

Modern GPU hash functions for procedural generation must balance speed vs statistical quality. The key paper (Jarzynski & Olano, "Hash Functions for GPU Rendering," JCGT 2020) evaluates multiple approaches:

**Recommended GPU hash functions:**

| Function              | Speed     | Quality            | Notes                                    |
| --------------------- | --------- | ------------------ | ---------------------------------------- |
| PCG Hash              | Fast      | Excellent          | Based on permuted congruential generator |
| xxHash (GPU port)     | Fast      | Excellent          | High avalanche, passes BigCrush          |
| SplitMix64            | Moderate  | Excellent          | From Java's SplittableRandom             |
| Wang Hash             | Very fast | Good               | Simple integer hash, minor bias          |
| `(34x^2 + x) mod 289` | Very fast | Adequate for noise | Gustavson's permutation polynomial       |

**Key insight:** For noise generation, hash quality need only be "good enough" -- visible artifacts matter more than passing statistical test suites. The permutation polynomial approach is sufficient for terrain noise. For applications needing true randomness (particle spawning, Monte Carlo), prefer PCG or xxHash.

### 2.3 FAST32 Hash (BrianSharpe GPU-Noise-Lib)

Used in optimized Worley noise implementations:

```glsl
void FAST32_hash_2D(vec2 gridcell, out vec4 hash_x, out vec4 hash_y) {
    // Maps 2D integer coordinates to 4 pseudo-random (x,y) pairs
    // Uses a chain of multiply-add-fract operations
    // Designed for minimal ALU with acceptable distribution
    const vec2 OFFSET = vec2(26.0, 161.0);
    const float DOMAIN = 71.0;
    const vec3 SOMELARGEFLOATS = vec3(951.135664, 642.949883, 803.202459);
    vec4 P = vec4(gridcell.xy, gridcell.xy + 1.0);
    P = P - floor(P * (1.0 / DOMAIN)) * DOMAIN;
    P += OFFSET.xyxy;
    P *= P;
    P = P.xzxz * P.yyww;
    hash_x = fract(P * (1.0 / SOMELARGEFLOATS.x));
    hash_y = fract(P * (1.0 / SOMELARGEFLOATS.y));
}
```

---

## 3. Noise Filtering & Artifact Reduction

### 3.1 Slightly Off-Integer Lacunarity

Using lacunarity values like **2.01 or 1.99** instead of exactly 2.0 prevents overlapping noise peaks across octaves, eliminating unrealistic grid-aligned patterns. This is a zero-cost optimization that significantly improves visual quality.

### 3.2 Octave Rotation (GPU Gems 3, Ch.1)

Rotating the domain between octaves breaks axis alignment artifacts:

```glsl
const mat3 octaveRotation = mat3(
     0.00,  0.80,  0.60,
    -0.80,  0.36, -0.48,
    -0.60, -0.48,  0.64
);

float fbm_rotated(vec3 x, int numOctaves) {
    float G = 0.5;
    float f = 1.0;
    float a = 1.0;
    float t = 0.0;
    for (int i = 0; i < numOctaves; i++) {
        t += a * noise(f * x);
        f *= 2.0;
        a *= G;
        x = octaveRotation * x;  // rotate domain each octave
    }
    return t;
}
```

### 3.3 Improved Perlin Fade Function

The original Perlin hermite `3t^2 - 2t^3` has non-zero second derivatives at lattice boundaries, producing visible creases. The improved quintic `6t^5 - 15t^4 + 10t^3` has zero first **and** second derivatives at t=0 and t=1, eliminating these artifacts. Always use the improved version.

### 3.4 IQ Derivative-Based Erosion Look

Quilez's technique uses noise derivatives to suppress detail on steep slopes, creating an erosion-like appearance without actual simulation:

```glsl
float iqTurbulence(vec2 p, int octaves) {
    float sum = 0.5;
    float freq = 1.0, amp = 1.0;
    vec2 dsum = vec2(0.0);
    for (int i = 0; i < octaves; i++) {
        vec3 n = perlinNoisePseudoDeriv(p * freq, float(i) / 256.0);
        dsum += n.yz;                              // accumulate derivatives
        sum += amp * n.x / (1.0 + dot(dsum, dsum)); // suppress on slopes
        freq *= 2.0;
        amp *= 0.5;
    }
    return sum;
}
```

**Key insight:** The `1 / (1 + dot(dsum, dsum))` term attenuates higher octaves where accumulated slope is large. Flat areas get full fractal detail while steep slopes appear smooth -- mimicking real erosion patterns at zero simulation cost.

### 3.5 Analytical Derivatives for Normal Computation

De Carpentier's approach computes normals analytically during fBm evaluation, avoiding costly finite-difference sampling (which requires 2-4 extra noise evaluations per pixel):

```glsl
float height = 0.0;
vec3 normal = vec3(0.0);
for (int i = 0; i < octaves; i++) {
    vec3 n = perlinNoiseDerivatives(p * freq, seed);
    height += amp * n.x;
    normal += amp * freq * vec3(-n.y, 1.0, -n.z);
    freq *= lacunarity;
    amp *= gain;
}
normal = normalize(normal);
```

For ridged/billowed variants, multiply derivatives by `sign(n.x)` to account for the `abs()` operation.

### 3.6 Elevation Redistribution

Raw noise produces uniform elevation distributions. To create realistic terrain with flat lowlands and sharp peaks:

```glsl
elevation = pow(raw_elevation, exponent);
```

Higher exponents push mid-elevations down, creating broad valleys with occasional sharp peaks. This models the bimodal character of real planetary hypsometry.

---

## 4. 3D Noise vs 2D Noise Trade-offs

### 4.1 The Sphere Mapping Problem

Standard 2D noise on (theta, phi) creates seams at phi=2\*pi and pole pinching. Three solutions exist:

| Method                                 | Pros                                              | Cons                           |
| -------------------------------------- | ------------------------------------------------- | ------------------------------ |
| 3D noise on unit sphere `noise(x,y,z)` | Naturally seamless, simple                        | 3D noise is more expensive     |
| Cube-to-sphere (6 faces)               | Standard 2D noise, easy LOD, GPU cubemap hardware | 33% area distortion at corners |
| Blended cube faces                     | Can use optimized 2D noise                        | Complex blending at edges      |

### 4.2 Cost Comparison

3D simplex evaluates 4 gradient samples per point (vs 3 for 2D). For 8-octave fBm at 4096x4096:

- 2D simplex fBm: ~0.8 ms (RTX 3060 class)
- 3D simplex fBm: ~1.3 ms (RTX 3060 class, ~1.6x slower)

The 60% overhead of 3D noise is generally acceptable since it completely eliminates seam issues. For the cube-to-sphere approach, 2D noise is used per face but edge continuity must be handled explicitly.

### 4.3 Planetary Scale Octave Budget

Quilez notes that with **24 octaves** of fBm, you can create terrain spanning the entire Earth with detail down to 2 meters. Practical LOD strategy:

| Octaves | Feature Scale       | Strategy                           |
| ------- | ------------------- | ---------------------------------- |
| 1-8     | Continental (~1 km) | Pre-bake to cubemap texture        |
| 9-16    | Regional (~1 m)     | Compute on demand per visible tile |
| 17-24   | Local (~2 m)        | Compute only for close-up camera   |

### 4.4 Texture-Based Octave Caching

For extreme performance, pre-bake low octaves into textures:

```glsl
float fbm_hybrid(vec3 x, int numOctaves) {
    float t = 0.0;
    float a = 1.0;
    float f = 1.0;

    // Low octaves from pre-baked 3D texture (faster)
    for (int i = 0; i < 3; i++) {
        t += a * noiseVol.Sample(TrilinearRepeat, x * f).x;
        f *= 2.03;  // slightly off for variation
        a *= 0.5;
    }

    // High octaves computed (more precise at high freq)
    for (int i = 3; i < numOctaves; i++) {
        t += a * snoise(f * x);
        f *= 1.97;
        a *= 0.5;
    }
    return t;
}
```

Pre-baked noise volumes: 3-4 textures at 128^3 or 256^3 = 8-64 MB each.

---

## 5. Hydraulic Erosion Simulation

### 5.1 Grid-Based Virtual Pipe Model (Mei et al. 2007, Jako 2011)

The standard GPU erosion approach stores per-cell state in multiple buffers:

| Buffer    | Format  | Contents                      | Size (4096^2) |
| --------- | ------- | ----------------------------- | ------------- |
| Terrain   | R32F    | Bedrock + sediment height     | 64 MB         |
| Water     | R32F    | Water column height           | 64 MB         |
| Sediment  | R32F    | Suspended sediment amount     | 64 MB         |
| Flux      | RGBA32F | Outflow flux (L, R, T, B)     | 256 MB        |
| Velocity  | RG32F   | Water velocity (vx, vy)       | 128 MB        |
| Hardness  | R32F    | Terrain resistance (optional) | 64 MB         |
| **Total** |         |                               | **~640 MB**   |

**Algorithm: 5 compute passes per timestep.**

**Pass 1 -- Water increment (rain/sources):**

```hlsl
[numthreads(16, 16, 1)]
void CSWaterSources(uint3 id : SV_DispatchThreadID) {
    float water = WaterTex[id.xy];
    water += dt * rainRate;
    WaterTex[id.xy] = water;
}
```

**Pass 2 -- Flow simulation (virtual pipe model):**

```hlsl
[numthreads(16, 16, 1)]
void CSFlowSimulation(uint3 id : SV_DispatchThreadID) {
    float h_center = TerrainTex[id.xy] + WaterTex[id.xy];
    float h_left   = TerrainTex[id.xy - uint2(1,0)] + WaterTex[id.xy - uint2(1,0)];
    float h_right  = TerrainTex[id.xy + uint2(1,0)] + WaterTex[id.xy + uint2(1,0)];
    float h_top    = TerrainTex[id.xy - uint2(0,1)] + WaterTex[id.xy - uint2(0,1)];
    float h_bottom = TerrainTex[id.xy + uint2(0,1)] + WaterTex[id.xy + uint2(0,1)];

    float4 flux = FluxTex[id.xy];
    float A = cellSize * cellSize;
    float g = 9.81;

    flux.x = max(0, flux.x + dt * A * g * (h_center - h_left)   / cellSize);
    flux.y = max(0, flux.y + dt * A * g * (h_center - h_right)  / cellSize);
    flux.z = max(0, flux.z + dt * A * g * (h_center - h_top)    / cellSize);
    flux.w = max(0, flux.w + dt * A * g * (h_center - h_bottom) / cellSize);

    // Prevent draining more water than available
    float totalFlux = flux.x + flux.y + flux.z + flux.w;
    float waterVol = WaterTex[id.xy] * cellSize * cellSize;
    float k = min(1.0, waterVol / (totalFlux * dt + 1e-6));
    flux *= k;

    FluxTex[id.xy] = flux;
}
```

**Pass 3 -- Apply flow (update water level + derive velocity):**

```hlsl
[numthreads(16, 16, 1)]
void CSApplyFlow(uint3 id : SV_DispatchThreadID) {
    float inflow = FluxTex[id.xy - uint2(1,0)].y
                 + FluxTex[id.xy + uint2(1,0)].x
                 + FluxTex[id.xy - uint2(0,1)].w
                 + FluxTex[id.xy + uint2(0,1)].z;
    float outflow = dot(FluxTex[id.xy], float4(1,1,1,1));

    float dV = (inflow - outflow) * dt;
    WaterTex[id.xy] += dV / (cellSize * cellSize);

    float vx = (FluxTex[id.xy - uint2(1,0)].y - FluxTex[id.xy].x
              + FluxTex[id.xy].y - FluxTex[id.xy + uint2(1,0)].x) * 0.5;
    float vy = (FluxTex[id.xy - uint2(0,1)].w - FluxTex[id.xy].z
              + FluxTex[id.xy].w - FluxTex[id.xy + uint2(0,1)].z) * 0.5;
    VelocityTex[id.xy] = float2(vx, vy) / (cellSize * max(WaterTex[id.xy], 0.001));
}
```

**Pass 4 -- Erosion and deposition:**

```hlsl
[numthreads(16, 16, 1)]
void CSErosionDeposition(uint3 id : SV_DispatchThreadID) {
    float2 vel = VelocityTex[id.xy];
    float speed = length(vel);

    // Local slope
    float dhdx = (TerrainTex[id.xy + uint2(1,0)] - TerrainTex[id.xy - uint2(1,0)]) * 0.5;
    float dhdy = (TerrainTex[id.xy + uint2(0,1)] - TerrainTex[id.xy - uint2(0,1)]) * 0.5;
    float sinAlpha = length(float2(dhdx, dhdy));

    // Sediment transport capacity
    float C = Kc * max(sinAlpha, 0.01) * speed;

    float sediment = SedimentTex[id.xy];
    if (sediment < C) {
        float erosion = Ks * (C - sediment);
        erosion = min(erosion, Lmax);
        TerrainTex[id.xy] -= erosion;
        SedimentTex[id.xy] += erosion;
    } else {
        float deposition = Kd * (sediment - C);
        TerrainTex[id.xy] += deposition;
        SedimentTex[id.xy] -= deposition;
    }
}
```

**Pass 5 -- Sediment transport (semi-Lagrangian advection) + evaporation:**

```hlsl
[numthreads(16, 16, 1)]
void CSSedimentTransport(uint3 id : SV_DispatchThreadID) {
    float2 vel = VelocityTex[id.xy];
    float2 srcPos = float2(id.xy) - vel * dt / cellSize;
    SedimentTex[id.xy] = SedimentTexPrev.SampleLevel(LinearClamp, srcPos / gridSize, 0);
    WaterTex[id.xy] *= (1.0 - Ke * dt);
}
```

**Erosion parameters (Jako 2011):**

| Parameter         | Symbol | Range      | Effect                            |
| ----------------- | ------ | ---------- | --------------------------------- |
| Sediment capacity | Kc     | 0.01-1.0   | How much sediment water can carry |
| Suspension rate   | Ks     | 0.001-0.01 | How fast terrain dissolves        |
| Deposition rate   | Kd     | 0.001-0.01 | How fast sediment settles         |
| Evaporation rate  | Ke     | 0.001-0.01 | Water volume loss per step        |
| Max erosion depth | Lmax   | 0.001-0.1  | Prevents over-erosion             |

### 5.2 Multi-Layer Erosion (Stava et al. 2008)

Extends the pipe model with:

- Terrain composed of **multiple material layers** with different erosion resistance
- Two coupled erosion modes: **dissolution** (slow water) and **force-based** (fast water)
- Bank collapse / slippage when undercutting occurs
- GPU implementation at 20+ fps on 2048x1024 grids

### 5.3 Transport-Limited vs Detachment-Limited Models

Two fundamentally different erosion regimes:

| Property         | Detachment-Limited                         | Transport-Limited             |
| ---------------- | ------------------------------------------ | ----------------------------- | --- | -------------------------------- |
| Limiting factor  | Ability to detach rock                     | Ability to transport sediment |
| PDE type         | Hyperbolic (advection)                     | Parabolic (diffusion)         |
| Information flow | Upstream only                              | Both directions               |
| Landscape type   | Steep, tectonically active; bedrock rivers | Low-gradient; alluvial rivers |
| Equation         | `dz/dt = U - K*A^m*                        | nabla z                       | ^n` | `dz/dt = -nabla . (K_t*A^m*S^n)` |

---

## 6. Thermal Erosion Simulation

### 6.1 Algorithm

Material moves downhill when slope exceeds the **talus angle** (angle of repose):

```hlsl
[numthreads(16, 16, 1)]
void CSThermalErosion(uint3 id : SV_DispatchThreadID) {
    float h = TerrainTex[id.xy];
    float4 dh = float4(
        h - TerrainTex[id.xy - uint2(1,0)],
        h - TerrainTex[id.xy + uint2(1,0)],
        h - TerrainTex[id.xy - uint2(0,1)],
        h - TerrainTex[id.xy + uint2(0,1)]
    );

    float talusAngle = Kt_alpha_scale * hardness + Kt_alpha_bias;
    float4 excess = max(dh - talusAngle * cellSize, 0.0);
    float totalExcess = dot(excess, float4(1,1,1,1));

    if (totalExcess > 0.0) {
        float transfer = Kt * min(totalExcess, h * 0.5);
        float4 weights = excess / totalExcess;
        TerrainTex[id.xy] -= transfer;
        // Distribute to neighbors proportionally
        // (requires atomic adds or separate scatter pass)
    }
}
```

### 6.2 Typical Talus Angles

| Material    | Talus Angle    | tan(angle) |
| ----------- | -------------- | ---------- |
| Sand        | ~33 degrees    | ~0.65      |
| Gravel      | ~35-40 degrees | ~0.70-0.84 |
| Rock debris | ~40-45 degrees | ~0.84-1.00 |

### 6.3 Diffusion-Based Hillslope Model

Linear diffusion models soil creep, frost shattering, rain splash, and bioturbation:

```
dz/dt = kappa * nabla^2(z)
```

**Published diffusivity values:**

| Setting                       | kappa [m^2/yr]   |
| ----------------------------- | ---------------- |
| Sierra Nevada, California     | 1.8              |
| Generic temperate hillslopes  | 0.036 +/- 0.0055 |
| Active mountain ranges (rock) | ~10              |

**Nonlinear transport law** (for steep slopes approaching failure):

```
q_s = -kappa * nabla(z) / [1 - (|nabla z| / S_c)^2]
```

where S_c is the critical slope (tan(S_c) ~ 0.6-0.75). This diverges as slope approaches S_c, producing sharp cliff faces.

---

## 7. Particle-Based Erosion on GPU

### 7.1 Complete Algorithm (Nick McDonald)

```cpp
struct Particle {
    glm::vec2 pos;
    glm::vec2 speed = glm::vec2(0.0);
    float volume = 1.0;
    float sediment = 0.0;
};

void erode(HeightMap& map, int numParticles) {
    for (int i = 0; i < numParticles; i++) {
        Particle drop;
        drop.pos = randomPosition();

        while (drop.volume > minVolume) {
            glm::ivec2 ipos = drop.pos;

            // 1. Surface normal drives acceleration
            glm::vec3 n = surfaceNormal(ipos.x, ipos.y);
            drop.speed += dt * glm::vec2(n.x, n.z) / (drop.volume * density);
            drop.pos += dt * drop.speed;
            drop.speed *= (1.0 - dt * friction);

            if (outOfBounds(drop.pos)) break;

            // 2. Equilibrium sediment concentration
            float c_eq = drop.volume * glm::length(drop.speed) *
                (map[ipos] - map[drop.pos]);
            if (c_eq < 0.0) c_eq = 0.0;

            // 3. Erode or deposit
            float cdiff = c_eq - drop.sediment;
            drop.sediment += dt * depositionRate * cdiff;
            map[ipos] -= dt * drop.volume * depositionRate * cdiff;

            // 4. Evaporate
            drop.volume *= (1.0 - dt * evapRate);
        }
    }
}
```

### 7.2 GPU Compute Version

```hlsl
struct Droplet {
    float2 pos;
    float2 vel;
    float  volume;
    float  sediment;
    float  speed;
};

[numthreads(256, 1, 1)]
void CSDropletErosion(uint3 id : SV_DispatchThreadID) {
    Droplet drop;
    drop.pos = RandomPosition(id.x);
    drop.vel = float2(0, 0);
    drop.volume = 1.0;
    drop.sediment = 0.0;

    for (int step = 0; step < maxLifetime; step++) {
        float2 gradient = SampleGradient(drop.pos);
        drop.vel = drop.vel * (1.0 - friction) - gradient * gravity;
        drop.speed = length(drop.vel);
        if (drop.speed < minSpeed) break;

        float2 newPos = drop.pos + normalize(drop.vel) * stepSize;
        float heightDiff = SampleHeight(drop.pos) - SampleHeight(newPos);
        float capacity = max(-heightDiff, minSlope) * drop.speed * drop.volume * Kc;

        if (drop.sediment > capacity || heightDiff > 0) {
            float deposit = (heightDiff > 0)
                ? min(heightDiff, drop.sediment)
                : (drop.sediment - capacity) * Kd;
            AtomicDeposit(drop.pos, deposit);
            drop.sediment -= deposit;
        } else {
            float erosion = min((capacity - drop.sediment) * Ks, -heightDiff);
            AtomicErode(drop.pos, erosion);
            drop.sediment += erosion;
        }

        drop.pos = newPos;
        drop.volume *= (1.0 - evapRate);
    }
}
```

### 7.3 GPU Parallelization Challenges

- **Atomic operations** for height modification create contention at popular cells (convergence zones)
- **Divergent loop lengths** per thread (some drops die early, wasting SIMD lanes)
- **Random memory access** pattern causes poor cache coherency
- Sebastian Lague's GPU version achieved ~40x speedup over CPU: 400K droplets on 1000x1000 in ~0.5 seconds

### 7.4 Practical Parameters (Sebastian Lague)

| Parameter                | Value          |
| ------------------------ | -------------- |
| Droplets                 | 50,000-200,000 |
| Max lifetime             | 30-64 steps    |
| Erosion rate             | 0.3            |
| Deposition rate          | 0.3            |
| Evaporation rate         | 0.01           |
| Sediment capacity factor | 4.0            |
| Min slope                | 0.01           |
| Inertia                  | 0.05           |

---

## 8. Compute Shader Optimization

### 8.1 Race Condition Strategies for GPU Erosion (Axel Paris)

Paris tested three strategies for the core problem of multiple threads reading/writing the same grid cell:

**Strategy 1: Single Integer Buffer with atomicAdd**

```glsl
layout(binding = 0, std430) coherent buffer HeightData {
    int heightBuffer[];  // heights as fixed-point integers
};
atomicAdd(heightBuffer[neighborIdx], -transferAmount);
atomicAdd(heightBuffer[currentIdx], transferAmount);
```

Pros: Guaranteed correctness. Cons: Limited to large-scale erosion (>1m precision); int representation loses sub-meter detail.

**Strategy 2: Double Buffer (Float + Integer)**

```glsl
layout(binding = 0, std430) readonly buffer HeightIn { float inData[]; };
layout(binding = 1, std430) writeonly buffer HeightOut { float outData[]; };
// Swap buffers after each step
```

Pros: Deterministic, race-free. Cons: **4-5x slower** due to conversion overhead and double memory.

**Strategy 3: Single Float Buffer (Ignore Races)**

```glsl
layout(binding = 0, std430) coherent buffer HeightData {
    float floatingHeightBuffer[];
};
// Read neighbors, compute transfer, write directly
// Race conditions exist but convergence compensates
```

Pros: Fastest, no conversion overhead. Cons: Non-deterministic; needs more iterations.

**Paris's conclusion:** "The single floating point buffer is the most efficient one." Compensating for race-condition errors by increasing iteration count (e.g., 500 to 700) is cheaper than correctness mechanisms. Result is visually equivalent.

**Reversed Read Pattern (Gather vs Scatter):** Instead of each cell writing to neighbors (scatter), each cell reads from neighbors and writes only to itself (gather). Eliminates races entirely but is **4-5x slower** in benchmarks.

### 8.2 Work Group Sizing

| Workload Type                | Recommended Size     | Rationale                                            |
| ---------------------------- | -------------------- | ---------------------------------------------------- |
| Noise generation (ALU-bound) | 8x8 or 16x16         | Matches 64/256 thread warps, no shared memory needed |
| Erosion (neighbor access)    | 8x8 with 1-cell halo | Shared memory for neighbor reads                     |
| Particle droplets            | 256x1x1              | 1D dispatch, each thread = one droplet               |

### 8.3 Memory & Register Pressure

| Noise Type      | Registers per Thread | Shared Memory       | Notes                                         |
| --------------- | -------------------- | ------------------- | --------------------------------------------- |
| 2D Simplex      | ~8-12 vec4           | None                | Purely computational, each thread independent |
| 3D Simplex      | ~12-16 vec4          | None                | More temporaries for 3D skewing               |
| 2D Worley (2x2) | ~8-10 vec4           | None                | Distance tracking for F1+F2                   |
| Gabor           | ~20-30 registers     | None                | Loop over Poisson impulses                    |
| Grid erosion    | ~10 vec4             | Optional 10x10 tile | Neighbor reads benefit from caching           |

### 8.4 Warp-Friendly Design Principles

For noise generation:

- No branching in core evaluation (`(x0.x > x0.y)` for simplex corner selection compiles to a predicated move)
- No shared memory needed -- each thread fully independent
- Memory-write coalescing: output to R32F texture with linear addressing

For erosion:

- All 5 shallow-water passes are independent per cell -- perfect GPU parallelism
- Semi-Lagrangian advection (Pass 5) requires linear texture sampling from arbitrary positions
- GPU->CPU readback can constitute up to **50% of total wall time** -- avoid readback during iteration loop

### 8.5 WebGPU / WGSL Performance

WebGPU compute shaders achieve **near-native performance** (typically within 5-15% of Vulkan/Metal) for ALU-bound workloads like noise generation. The main overhead is JavaScript-to-GPU command submission latency, not shader execution. For Rust + wgpu (native), this overhead is negligible.

---

## 9. Code Examples

### 9.1 WGSL 2D Simplex Noise (Gustavson Port)

```wgsl
fn permute3(x: vec3f) -> vec3f {
    return (((x * 34.0) + 1.0) * x) % vec3f(289.0);
}

fn snoise2(v: vec2f) -> f32 {
    let C = vec4f(0.211324865405187, 0.366025403784439,
                  -0.577350269189626, 0.024390243902439);
    var i  = floor(v + dot(v, C.yy));
    let x0 = v - i + dot(i, C.xx);

    var i1: vec2f;
    if (x0.x > x0.y) { i1 = vec2f(1.0, 0.0); }
    else              { i1 = vec2f(0.0, 1.0); }

    var x12 = x0.xyxy + C.xxzz;
    x12 = vec4f(x12.xy - i1, x12.zw);

    i = i % vec2f(289.0);
    let p = permute3(permute3(i.y + vec3f(0.0, i1.y, 1.0))
                             + i.x + vec3f(0.0, i1.x, 1.0));

    var m = max(vec3f(0.5) - vec3f(
        dot(x0, x0), dot(x12.xy, x12.xy), dot(x12.zw, x12.zw)), vec3f(0.0));
    m = m * m;
    m = m * m;

    let x_  = 2.0 * fract(p * C.www) - 1.0;
    let h   = abs(x_) - 0.5;
    let a0  = x_ - floor(x_ + 0.5);
    m = m * (1.79284291400159 - 0.85373472095314 * (a0 * a0 + h * h));

    let g = vec3f(a0.x * x0.x + h.x * x0.y,
                  a0.y * x12.x + h.y * x12.y,
                  a0.z * x12.z + h.z * x12.w);

    return 130.0 * dot(m, g);
}
```

### 9.2 GLSL 2D Simplex Noise (Gustavson & McEwan 2012)

```glsl
vec3 permute(vec3 x) {
    return mod(((x * 34.0) + 1.0) * x, 289.0);
}

float snoise(vec2 P) {
    const vec2 C = vec2(0.211324865405187,  // (3-sqrt(3))/6
                        0.366025403784439);  // (sqrt(3)-1)/2
    vec2 i  = floor(P + dot(P, C.yy));
    vec2 x0 = P - i + dot(i, C.xx);

    vec2 i1 = (x0.x > x0.y) ? vec2(1.0, 0.0) : vec2(0.0, 1.0);
    vec4 x12 = x0.xyxy + vec4(C.xx, C.xx - 1.0);
    x12.xy -= i1;

    i = mod(i, 289.0);
    vec3 p = permute(permute(i.y + vec3(0.0, i1.y, 1.0))
                             + i.x + vec3(0.0, i1.x, 1.0));

    vec3 m = max(0.5 - vec3(dot(x0,x0), dot(x12.xy,x12.xy),
                             dot(x12.zw,x12.zw)), 0.0);
    m = m * m; m = m * m;

    vec3 x = 2.0 * fract(p * vec3(1.0/41.0)) - 1.0;
    vec3 h = abs(x) - 0.5;
    vec3 a0 = x - floor(x + 0.5);
    m *= 1.79284291400159 - 0.85373472095314 * (a0*a0 + h*h);

    vec3 g;
    g.x  = a0.x * x0.x + h.x * x0.y;
    g.yz = a0.yz * x12.xz + h.yz * x12.yw;

    return 130.0 * dot(m, g);
}
```

### 9.3 GLSL Optimized Worley 2D (BrianSharpe 2x2)

```glsl
float Cellular2D(vec2 P) {
    vec2 Pi = floor(P);
    vec2 Pf = P - Pi;

    vec4 hash_x, hash_y;
    FAST32_hash_2D(Pi, hash_x, hash_y);

    const float JITTER_WINDOW = 0.25;
    hash_x = Cellular_weight_samples(hash_x) * JITTER_WINDOW
           + vec4(0.0, 1.0, 0.0, 1.0);
    hash_y = Cellular_weight_samples(hash_y) * JITTER_WINDOW
           + vec4(0.0, 0.0, 1.0, 1.0);

    vec4 dx = hash_x - Pf.xxxx;
    vec4 dy = hash_y - Pf.yyyy;
    vec4 d = dx*dx + dy*dy;

    d.xy = min(d.xy, d.zw);
    return min(d.x, d.y) * (1.0 / 1.125);
}
```

### 9.4 GLSL Worley F1/F2 with Multiple Distance Metrics

```glsl
vec2 worley2D(vec2 P, float jitter, bool manhattanDistance) {
    vec2 result = vec2(1e20);
    for (int y = -1; y <= 1; y++) {
        for (int x = -1; x <= 1; x++) {
            vec2 cell = floor(P) + vec2(x, y);
            vec2 point = cell + hash22(cell) * jitter;
            vec2 diff = point - P;

            float dist;
            if (manhattanDistance)
                dist = abs(diff.x) + abs(diff.y);
            else
                dist = dot(diff, diff);

            if (dist < result.x) {
                result.y = result.x;
                result.x = dist;
            } else if (dist < result.y) {
                result.y = dist;
            }
        }
    }
    return sqrt(result);
}
```

### 9.5 GLSL Gabor Noise (from victor-shepardson)

```glsl
struct gnoise_params {
    float bandwidth;
    float density;
    float sigma;
    int   octaves;
    float sector_angle;
    float sector_width;
    mat2  jacobian;
};

float gnoise(vec2 uv, gnoise_params p) {
    vec2 cell = floor(uv);
    float result = 0.0;

    for (int dy = -1; dy <= 1; dy++) {
        for (int dx = -1; dx <= 1; dx++) {
            vec2 neighbor = cell + vec2(dx, dy);
            int n_impulses = poisson(neighbor, p.density);

            for (int i = 0; i < n_impulses; i++) {
                vec2 pos = neighbor + rand2(neighbor, i);
                float freq = rand_annular_sector(p);
                float phase = rand(neighbor, i) * 2.0 * PI;

                vec2 diff = uv - pos;
                mat2 sigma_fg = compute_filter_covariance(
                    p.bandwidth, p.sigma, p.jacobian);
                float envelope = exp(-PI * dot(diff, sigma_fg * diff));
                float oscillation = cos(2.0*PI*dot(freq_vec, diff) + phase);
                result += envelope * oscillation;
            }
        }
    }
    return result / sqrt(p.density);
}
```

### 9.6 GLSL Ridged Multifractal (Musgrave)

```glsl
float ridgedMultifractal(vec3 p, float H, float lacunarity,
                         int octaves, float offset) {
    float result = 0.0;
    float weight = 1.0;
    float freq = 1.0;

    for (int i = 0; i < octaves; i++) {
        float signal = offset - abs(snoise(p * freq));
        signal *= signal;        // sharpen ridges
        signal *= weight;        // weight by previous octave
        weight = clamp(signal * 2.0, 0.0, 1.0);

        result += signal * pow(freq, -H);
        freq *= lacunarity;
    }
    return result;
}
```

Recommended defaults: H=1.0, offset=1.0, lacunarity=2.0, gain=2.0.

### 9.7 GLSL Optimized fBm (Inigo Quilez)

```glsl
float fbm(vec3 x, float H, int numOctaves) {
    float G = exp2(-H);   // gain = 2^(-H), computed once
    float f = 1.0;
    float a = 1.0;
    float t = 0.0;
    for (int i = 0; i < numOctaves; i++) {
        t += a * noise(f * x);
        f *= 2.0;   // lacunarity (use 2.01 to reduce alignment)
        a *= G;      // gain
    }
    return t;
}
```

Eliminates `pow()` calls using iterative multiplication.

### 9.8 Domain Warping (Two-Level)

```glsl
float pattern(vec2 p) {
    vec2 q = vec2(fbm(p + vec2(0.0, 0.0), 0.5, 8),
                  fbm(p + vec2(5.2, 1.3), 0.5, 8));
    vec2 r = vec2(fbm(p + 4.0*q + vec2(1.7, 9.2), 0.5, 8),
                  fbm(p + 4.0*q + vec2(8.3, 2.8), 0.5, 8));
    return fbm(p + 4.0 * r, 0.5, 8);
}
// Cost: 5x base fBm (40 octave evaluations for 8-octave fBm)
```

Offset constants (0.0, 5.2, 1.3, 1.7, 9.2, 8.3, 2.8) must be different to produce independent noise patterns. The factor 4.0 controls warping strength (range 1.0-8.0).

---

## 10. Benchmarks & Performance

### 10.1 Noise Generation (4096x4096, single evaluation)

| Noise Type                   | GTX 580 (2012) | Est. RTX 3060 (2022) | Est. RTX 4090 (2023) | Source                   |
| ---------------------------- | -------------- | -------------------- | -------------------- | ------------------------ |
| 2D Simplex (1 octave)        | 3.6 ms         | ~0.1 ms              | ~0.05 ms             | Gustavson 2012           |
| 3D Simplex (1 octave)        | 6.9 ms         | ~0.2 ms              | ~0.1 ms              | Gustavson 2012           |
| 2D Simplex fBm (8 oct)       | ~29 ms         | ~0.8 ms              | ~0.4 ms              | Linear extrapolation     |
| 2D Simplex fBm (16 oct)      | ~58 ms         | ~1.6 ms              | ~0.8 ms              | Linear extrapolation     |
| 2D Worley (3x3, F1+F2)       | N/A            | ~2-5 ms              | ~1-2 ms              | ALU cost estimate        |
| 2D Gabor (density=4)         | N/A            | ~20 ms               | ~10 ms               | 5-20x simplex estimate   |
| Domain warp (2-level, 8 oct) | N/A            | ~7 ms                | ~3.5 ms              | 5x base fBm              |
| Diamond-Square               | N/A            | ~212 ms              | ~100 ms              | VertexFragment benchmark |

**Extrapolation basis:** RTX 3060 ~20 TFLOPS FP32 vs GTX 580 ~1.6 TFLOPS = ~12.5x theoretical, practical ~10x. RTX 4090 ~82 TFLOPS = ~50x theoretical, practical ~30-40x.

### 10.2 Gustavson Simplex Benchmarks (Msamples/sec)

| GPU     | 2D Simplex | 3D Simplex | 4D Simplex | 2D Classic | 3D Classic |
| ------- | ---------- | ---------- | ---------- | ---------- | ---------- |
| GTX 580 | 4,676      | 2,415      | 1,429      | 3,724      | 1,256      |
| HD 5870 | 4,980      | 3,062      | 2,006      | 5,134      | 2,081      |
| GTX 260 | 1,487      | 784        | 426        | 1,108      | 405        |

**Improved version** (GTX 580): 13,863 Msamples/sec for 2D -- a 1.78x speedup over the basic version.

### 10.3 Multi-Octave fBm (4096x4096, Est. RTX 3060)

| Octaves | Simplex-based fBm | Perlin-based fBm |
| ------- | ----------------- | ---------------- |
| 1       | ~0.1 ms           | ~0.15 ms         |
| 4       | ~0.4 ms           | ~0.6 ms          |
| 8       | ~0.8 ms           | ~1.2 ms          |
| 12      | ~1.2 ms           | ~1.8 ms          |
| 16      | ~1.6 ms           | ~2.4 ms          |

Scales linearly with octave count (purely ALU-bound loop).

### 10.4 GPU vs CPU Diamond-Square

| Resolution | Single-thread C# | Multi-thread C# | GPU (256 threads) | GPU Speedup |
| ---------- | ---------------- | --------------- | ----------------- | ----------- |
| 512x512    | 38.3 ms          | 26.7 ms         | 2.1 ms            | 18x         |
| 1024x1024  | 161.6 ms         | 67.9 ms         | 9.3 ms            | 17x         |
| 2048x2048  | 740.2 ms         | 224.4 ms        | 41.7 ms           | 18x         |
| 4096x4096  | 2,987.3 ms       | 823.9 ms        | 211.6 ms          | 14x         |
| 8192x8192  | 13,042.1 ms      | 3,121.5 ms      | 778.2 ms          | 17x         |

Note: Diamond-Square has inherent sequential dependencies (each subdivision depends on the previous), making it harder to parallelize than noise. Pure noise functions show even larger GPU advantages since every texel is fully independent.

### 10.5 Erosion Simulation

| Method                                | Resolution | Steps     | Time (RTX 3060 class) |
| ------------------------------------- | ---------- | --------- | --------------------- |
| Grid hydraulic (shallow water)        | 1024x1024  | 500       | ~1-2.5 sec            |
| Grid hydraulic (shallow water)        | 2048x2048  | 50        | ~100 ms               |
| Grid hydraulic (shallow water)        | 4096x4096  | 500       | ~15-40 sec            |
| Grid thermal (single float, races OK) | 1024x1024  | 1000      | ~2-5 sec              |
| Grid thermal (dual buffer, no races)  | 1024x1024  | 1000      | ~8-25 sec             |
| Particle droplet (GPU)                | 400K drops | 100 steps | ~0.5 sec              |
| Particle droplet (CPU)                | 200K drops | ~50 steps | ~10-20 sec            |

### 10.6 Memory Budget (4096x4096 grid)

| Component                                                            | Buffers | Total Size  |
| -------------------------------------------------------------------- | ------- | ----------- |
| Erosion buffers (terrain, water, sediment, flux, velocity, hardness) | 6       | ~640 MB     |
| Noise output (single channel)                                        | 1       | 64 MB       |
| Domain warp intermediates (RG32F x2)                                 | 2       | 256 MB      |
| **Total noise pipeline**                                             |         | **~320 MB** |
| **Total erosion pipeline**                                           |         | **~640 MB** |

### 10.7 Recommended Pipeline Summary

| Stage                        | Technique                 | Resolution | Est. Time (RTX 3060) | Memory      |
| ---------------------------- | ------------------------- | ---------- | -------------------- | ----------- |
| Base heightmap               | Simplex fBm (12 oct)      | 4096x4096  | ~1.5 ms              | 64 MB       |
| Continental warp             | 2-level domain warping    | 4096x4096  | ~7 ms                | 200 MB      |
| Crater/cell features         | Worley F1+F2 (3x3)        | 4096x4096  | ~3 ms                | 128 MB      |
| Directional features         | Gabor noise (selective)   | 2048x2048  | ~20 ms               | 16 MB       |
| Hydraulic erosion            | Shallow water (500 steps) | 2048x2048  | ~5 sec               | 150 MB      |
| Thermal erosion              | Grid-based (200 steps)    | 2048x2048  | ~1 sec               | 40 MB       |
| **Total offline generation** |                           |            | **~7 sec**           | **~600 MB** |

---

## 11. References

### Noise Algorithms

- Gustavson & McEwan, "Efficient computational noise in GLSL" (2012): https://ar5iv.labs.arxiv.org/html/1204.1461
- GPU Gems 2 Ch.26, Implementing Improved Perlin Noise: https://developer.nvidia.com/gpugems/gpugems2/part-iii-high-quality-rendering/chapter-26-implementing-improved-perlin-noise
- GPU Gems Ch.5, Improved Perlin Noise: https://developer.nvidia.com/gpugems/gpugems/part-i-natural-effects/chapter-5-implementing-improved-perlin-noise
- Perlin, "Improving Noise," SIGGRAPH 2002: https://mrl.cs.nyu.edu/~perlin/paper445.pdf
- Gustavson, "Simplex Noise Demystified" (2005): https://cgvr.cs.uni-bremen.de/teaching/cg_literatur/simplexnoise.pdf
- KdotJPG, OpenSimplex2 (CC0): https://github.com/KdotJPG/OpenSimplex2
- Worley, "A Cellular Texture Basis Function," SIGGRAPH 1996: https://dl.acm.org/doi/10.1145/237170.237267
- BrianSharpe GPU-Noise-Lib: https://github.com/BrianSharpe/GPU-Noise-Lib
- BrianSharpe optimized cellular: https://briansharpe.wordpress.com/2011/12/01/optimized-artifact-free-gpu-cellular-noise/
- Gustavson cellular GLSL notes: https://itn-web.it.liu.se/~stegu76/GLSL-cellular/GLSL-cellular-notes.pdf
- Lagae et al., "Procedural Noise using Sparse Gabor Convolution" (SIGGRAPH 2009): https://dl.acm.org/doi/10.1145/1531326.1531360
- Gabor Noise by Example (2012): https://dl.acm.org/doi/10.1145/2185520.2185569
- Survey of Procedural Noise Functions (CGF 2010): https://www.cs.umd.edu/~zwicker/publications/SurveyProceduralNoise-CGF10.pdf
- GLSL noise algorithms collection: https://gist.github.com/patriciogonzalezvivo/670c22f3966e662d2f83

### Hash Functions

- Jarzynski & Olano, "Hash Functions for GPU Rendering," JCGT 2020
- BrianSharpe FAST32 hash: https://github.com/BrianSharpe/GPU-Noise-Lib

### Erosion Simulation

- Mei et al., "Fast Hydraulic Erosion Simulation and Visualization on GPU" (PG 2007): https://inria.hal.science/inria-00402079/document
- Jako, "Fast Hydraulic and Thermal Erosion on the GPU" (CESCG 2011): https://old.cescg.org/CESCG-2011/papers/TUBudapest-Jako-Balazs.pdf
- Stava et al., "Interactive Terrain Modeling Using Hydraulic Erosion" (SCA 2008): https://cgg.mff.cuni.cz/~jaroslav/papers/2008-sca-erosim/2008-sca-erosiom-fin.pdf
- Paris, "Terrain Erosion on the GPU": https://aparis69.github.io/public_html/posts/terrain_erosion.html
- Paris, "Terrain Erosion on the GPU #2": https://aparis69.github.io/public_html/posts/terrain_erosion_2.html
- McDonald, "Simple Particle-Based Hydraulic Erosion": https://nickmcd.me/2020/04/10/simple-particle-based-hydraulic-erosion/
- Lague, Hydraulic Erosion tool: https://sebastian.itch.io/hydraulic-erosion
- bshishov/UnityTerrainErosionGPU: https://github.com/bshishov/UnityTerrainErosionGPU
- lisyarus WebGPU shallow water: https://github.com/lisyarus/webgpu-shallow-water
- Frozen Fractal, "Around The World: Hydraulic Erosion": https://frozenfractal.com/blog/2025/6/6/around-the-world-23-hydraulic-erosion/
- Cordonnier et al., "Large Scale Terrain from Tectonic Uplift and Fluvial Erosion" (EG/CGF 2016): https://inria.hal.science/hal-01262376
- Fernandes & Dietrich, "Hillslope evolution by diffusive processes" (WRR 1997): https://agupubs.onlinelibrary.wiley.com/doi/pdf/10.1029/97WR00534

### Fractal Terrain & fBm

- Quilez, "fBm": https://iquilezles.org/articles/fbm/
- Quilez, "Domain Warping": https://iquilezles.org/articles/warp/
- Quilez, "Value noise derivatives": https://iquilezles.org/articles/morenoise/
- Book of Shaders Ch.13: https://thebookofshaders.com/13/
- GPU Gems 3 Ch.1 (procedural terrain): https://developer.nvidia.com/gpugems/gpugems3/part-i-geometry/chapter-1-generating-complex-procedural-terrains-using-gpu
- De Carpentier, Scape procedural basics: https://www.decarpentier.nl/scape-procedural-basics
- Musgrave, "Procedural Fractal Terrains" (course notes): https://www.classes.cs.uchicago.edu/archive/2015/fall/23700-1/final-project/MusgraveTerrain00.pdf
- Red Blob Games, terrain from noise: https://www.redblobgames.com/maps/terrain-from-noise/

### WebGPU / WGSL

- Kosmos (Rust+WebGPU terrain): https://github.com/kaylendog/kosmos
- wgsl-fns utility library: https://github.com/koole/wgsl-fns
- WGSL specification: https://www.w3.org/TR/WGSL/

### GPU Performance & Benchmarks

- GPU Diamond-Square benchmarks: https://www.vertexfragment.com/ramblings/diamond-square/
- Gaia Sky procedural surfaces: https://tonisagrista.com/blog/2021/procedural-planetary-surfaces/
- AMD GPUOpen work graphs + mesh nodes: https://gpuopen.com/learn/work_graphs_mesh_nodes/work_graphs_mesh_nodes-procedural_generation/

---

_Confidence: HIGH for algorithm descriptions and code examples (sourced from published papers and reference implementations). MEDIUM for performance estimates on modern GPUs (extrapolated from older benchmarks). LOW for Gabor noise GPU timings (sparse benchmarks available)._
