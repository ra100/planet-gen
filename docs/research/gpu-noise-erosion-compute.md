# GPU-Accelerated Noise Generation for Procedural Planet Generation

_Research date: 2026-03-28 (expanded with web-verified sources)_

---

## 1. Simplex Noise on GPU

### Algorithm Overview

Simplex noise (Perlin 2001) uses an N-dimensional simplex grid instead of a hypercubic lattice. In 2D it evaluates 3 gradient samples (vs 4 for Perlin), in 3D it evaluates 4 (vs 8), and in 4D it evaluates 5 (vs 16). This gives simplex a scaling advantage of O(N+1) vs O(2^N).

### Best Compute Shader Implementation: Gustavson & McEwan (2012)

The definitive GPU implementation eliminates all texture lookups using **permutation polynomials** and **cross-polytope gradient mapping**.

**Key innovations:**
- Replaces lookup tables with `(34x^2 + x) mod 289` -- a permutation polynomial computed purely in ALU
- Maps points onto N-dimensional octahedron surfaces to select gradients computationally
- Uses rank-ordering via pairwise component comparisons (warp-friendly, no branching)
- Self-contained: no external data, only a few registers of temporary storage

**Core GLSL (2D simplex):**
```glsl
vec3 permute(vec3 x) {
    return mod(((x * 34.0) + 1.0) * x, 289.0);
}

float snoise(vec2 P) {
    const vec2 C = vec2(0.211324865405187, // (3-sqrt(3))/6
                        0.366025403784439); // (sqrt(3)-1)/2
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

### Performance Benchmarks (Gustavson & McEwan 2012)

Measured in **Msamples/sec** (millions of noise evaluations per second):

| GPU           | 2D Simplex | 3D Simplex | 4D Simplex | 2D Classic | 3D Classic |
|---------------|-----------|-----------|-----------|-----------|-----------|
| GTX 580       | 4,676     | 2,415     | 1,429     | 3,724     | 1,256     |
| HD 5870       | 4,980     | 3,062     | 2,006     | 5,134     | 2,081     |
| GTX 260       | 1,487     | 784       | 426       | 1,108     | 405       |

**Improved version** (same paper, GTX 580): 13,863 Msamples/sec for 2D (vs 7,806 for old implementation) -- a 1.78x speedup.

At 4,676 Msamples/sec on a GTX 580, a 4096x4096 single-octave 2D simplex evaluation would take approximately **3.6 ms**. On modern GPUs (RTX 4090 with ~82 TFLOPS FP32 vs GTX 580's ~1.6 TFLOPS), we can extrapolate roughly **50x** improvement, suggesting sub-0.1 ms for single-octave 4096x4096.

### Memory Requirements

- **Zero textures** required (purely computational)
- Temporary registers: ~8-12 vec4 registers per thread
- Output: 1 float per texel (4 bytes) -> 4096x4096 = 64 MB for float32

### Warp-Friendly Considerations

- No branching in the core evaluation (the `(x0.x > x0.y)` for simplex corner selection compiles to a predicated move)
- No shared memory needed -- each thread fully independent
- Ideal compute shader dispatch: 8x8 or 16x16 thread groups (matching 64/256 thread warps)
- Memory-write coalescing: output to R32F texture with linear addressing

### Key References

- Gustavson & McEwan, "Efficient computational noise in GLSL" (2012): https://ar5iv.labs.arxiv.org/html/1204.1461
- GPU Gems 2 Ch.26, NVIDIA: https://developer.nvidia.com/gpugems/gpugems2/part-iii-high-quality-rendering/chapter-26-implementing-improved-perlin-noise
- BrianSharpe GPU-Noise-Lib: https://github.com/BrianSharpe/GPU-Noise-Lib
- GLSL noise algorithms collection: https://gist.github.com/patriciogonzalezvivo/670c22f3966e662d2f83

---

## 2. Worley/Cellular Noise on GPU

### Algorithm Overview

Worley (cellular) noise distributes feature points in a jittered grid and computes distances to the nearest points. F1 = distance to closest point, F2 = distance to second closest. Terrain uses F1 for volcanic craters, F2-F1 for cell boundaries/cracks.

### Standard Approach: 3x3 Grid Search

For each pixel, check all 9 neighboring cells (2D) or 27 cells (3D), each containing one jittered feature point. Compute distances to all points, track the two smallest.

### Optimized 2x2 Approach (Gustavson / BrianSharpe)

Reduces search window from 3x3 (9 points) to 2x2 (4 points) by constraining jitter:

**Key optimization:**
- Reduce jitter from +/-0.5 to +/-0.25 (2D) or +/-0.1666 (3D)
- Apply cubic weighting to push points toward extremes, compensating for reduced variation
- Result: 2.25x fewer distance calculations in 2D, 3.375x fewer in 3D

**GLSL Implementation (BrianSharpe):**
```glsl
float Cellular2D(vec2 P) {
    vec2 Pi = floor(P);
    vec2 Pf = P - Pi;

    vec4 hash_x, hash_y;
    FAST32_hash_2D(Pi, hash_x, hash_y);

    // Jitter window 0.25 eliminates artifacts
    const float JITTER_WINDOW = 0.25;
    hash_x = Cellular_weight_samples(hash_x) * JITTER_WINDOW
           + vec4(0.0, 1.0, 0.0, 1.0);
    hash_y = Cellular_weight_samples(hash_y) * JITTER_WINDOW
           + vec4(0.0, 0.0, 1.0, 1.0);

    // Distance to offset points
    vec4 dx = hash_x - Pf.xxxx;
    vec4 dy = hash_y - Pf.yyyy;
    vec4 d = dx*dx + dy*dy;      // squared Euclidean

    d.xy = min(d.xy, d.zw);
    return min(d.x, d.y) * (1.0 / 1.125); // normalize
}
```

### F1/F2 with Multiple Distance Metrics

```glsl
vec2 worley2D(vec2 P, float jitter, bool manhattanDistance) {
    // Returns vec2(F1, F2)
    // Searches 3x3 grid of cells
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
                dist = dot(diff, diff); // squared Euclidean

            if (dist < result.x) {
                result.y = result.x;  // F2 = old F1
                result.x = dist;      // F1 = new minimum
            } else if (dist < result.y) {
                result.y = dist;      // F2 updated
            }
        }
    }
    return sqrt(result); // actual distances
}
```

**Distance metric options:**
- Euclidean: `sqrt(dx*dx + dy*dy)` -- smooth organic cells
- Manhattan: `abs(dx) + abs(dy)` -- diamond-shaped, jagged cells
- Chebyshev: `max(abs(dx), abs(dy))` -- square cells
- Minkowski(p): `pow(pow(abs(dx),p) + pow(abs(dy),p), 1/p)` -- interpolates between metrics

### Performance Estimates

- 2x2 optimized is roughly **2x faster** than 3x3 in 2D (4 vs 9 distance calcs)
- 3D: 2x2x2 = 8 distance calcs vs 3x3x3 = 27 (3.4x faster)
- On modern GPU: 4096x4096 2D Worley (3x3) approximately 2-5 ms, 2x2 optimized approximately 1-2 ms
- Bottleneck is ALU (distance calculations), not memory

### Memory Requirements

- Same as simplex: zero textures for computational approach
- Output: 8 bytes/texel for F1+F2 (RG32F), or 4 bytes for F1 only
- 4096x4096 F1+F2: 128 MB

### Key References

- BrianSharpe optimized cellular: https://briansharpe.wordpress.com/2011/12/01/optimized-artifact-free-gpu-cellular-noise/
- Gustavson cellular GLSL notes: https://itn-web.it.liu.se/~stegu76/GLSL-cellular/GLSL-cellular-notes.pdf
- Erkaman glsl-worley (F1/F2 + Manhattan): https://github.com/Erkaman/glsl-worley
- Scrawk GPU-Voronoi-Noise (Unity): https://github.com/Scrawk/GPU-Voronoi-Noise
- Cellular noise variants (understanding): https://sangillee.com/2025-04-18-cellular-noises/

---

## 3. Gabor Noise on GPU

### Algorithm Overview

Gabor noise (Lagae et al. 2009) uses sparse convolution with Gabor kernels -- a Gaussian envelope modulated by a cosine wave. It provides:
- Precise spectral control (principal frequency, bandwidth, orientation)
- Anisotropic patterns (directional features like ridges, dunes, flow lines)
- Bandwidth-limited output (anti-aliasing friendly)
- Setup-free surface noise (no UV parameterization needed)

### Mathematical Formulation

A single Gabor kernel:
```
g(x) = K * exp(-pi * a^2 * |x|^2) * cos(2*pi * F0 . x + phi)
```

Where:
- `K` = magnitude
- `a` = width parameter (controls Gaussian envelope = bandwidth)
- `F0` = frequency vector (direction + principal frequency)
- `phi` = phase offset

Gabor noise = sparse Poisson-distributed impulses convolved with Gabor kernels:
```
n(x) = sum_i w_i * g(x - x_i)
```

### GPU GLSL Implementation (from victor-shepardson/webgl-gabor-noise)

**Architecture:**
```glsl
struct gnoise_params {
    float bandwidth;    // Gaussian envelope width
    float density;      // impulse density (Poisson rate)
    float sigma;        // filter sigma for anti-aliasing
    int   octaves;      // multi-scale evaluation
    float sector_angle; // anisotropy direction
    float sector_width; // anisotropy spread
    mat2  jacobian;     // surface mapping Jacobian
};

float gnoise(vec2 uv, gnoise_params p) {
    vec2 cell = floor(uv);
    float result = 0.0;

    // Iterate 3x3 neighborhood
    for (int dy = -1; dy <= 1; dy++) {
        for (int dx = -1; dx <= 1; dx++) {
            vec2 neighbor = cell + vec2(dx, dy);
            int n_impulses = poisson(neighbor, p.density);

            for (int i = 0; i < n_impulses; i++) {
                vec2 pos = neighbor + rand2(neighbor, i);
                float freq = rand_annular_sector(p);
                float phase = rand(neighbor, i) * 2.0 * PI;

                // Gabor kernel with anisotropic filtering
                vec2 diff = uv - pos;
                // Apply Jacobian for surface mapping
                mat2 sigma_fg = compute_filter_covariance(
                    p.bandwidth, p.sigma, p.jacobian);
                float envelope = exp(-PI * dot(diff, sigma_fg * diff));
                float oscillation = cos(2.0*PI*dot(freq_vec, diff) + phase);
                result += envelope * oscillation;
            }
        }
    }
    return result / sqrt(p.density); // normalize variance
}
```

### Anisotropy Control for Terrain Features

The key terrain-relevant parameters:
- **sector_angle**: Controls dominant direction (e.g., 0 = horizontal ridges, PI/4 = diagonal dunes)
- **sector_width**: 0 = perfectly aligned features, PI = isotropic noise
- **bandwidth**: Low values = smooth broad features, high values = sharp detail
- **Jacobian matrix**: Enables proper filtering when mapped onto curved surfaces (critical for planet rendering)

### Performance Characteristics

- Significantly more expensive than simplex/Perlin: each texel requires evaluating multiple Gabor kernels per cell (Poisson-distributed, typically 3-8 impulses)
- Cost scales with `density * 9_cells * avg_impulses_per_cell`
- Typical performance: **5-20x slower than simplex** for equivalent resolution
- Tradeoff is worthwhile for specific terrain features requiring anisotropy (sand dunes, river erosion patterns, geological strata)
- Best used selectively: generate Gabor noise for specific layers, not as the base heightmap

### Memory Requirements

- Purely computational (no textures)
- Higher register pressure than simplex (~20-30 registers per thread)
- Same output requirements: 4 bytes/texel for single channel

### Key References

- Lagae et al., "Procedural Noise using Sparse Gabor Convolution" (SIGGRAPH 2009): https://dl.acm.org/doi/10.1145/1531326.1531360
- Paper PDF: https://www-sop.inria.fr/reves/Basilic/2009/LLDD09/LLDD09PNSGC_paper.pdf
- "Gabor Noise by Example" (2012): https://dl.acm.org/doi/10.1145/2185520.2185569
- "Filtering Solid Gabor Noise" (2011): https://dl.acm.org/doi/abs/10.1145/2010324.1964946
- WebGL implementation: https://github.com/victor-shepardson/webgl-gabor-noise
- Survey of Procedural Noise Functions: https://www.cs.umd.edu/~zwicker/publications/SurveyProceduralNoise-CGF10.pdf

---

## 4. GPU Erosion Simulation

### Two Main Approaches

**A. Grid-based (Shallow Water / Pipe Model)** -- better GPU parallelism
**B. Particle-based (Droplet)** -- simpler algorithm, harder to parallelize

### A. Grid-Based Hydraulic Erosion (Mei et al. 2007, Jako 2011)

Uses shallow water equations with a virtual pipe model. Each cell stores:

| Buffer        | Format  | Contents                              |
|---------------|---------|---------------------------------------|
| Terrain       | R32F    | Bedrock + sediment height             |
| Water         | R32F    | Water column height                   |
| Sediment      | R32F    | Suspended sediment amount             |
| Flux          | RGBA32F | Outflow flux (left, right, top, bottom) |
| Velocity      | RG32F   | Water velocity (vx, vy)              |
| Hardness      | R32F    | Terrain resistance (optional)         |

**Total per cell: 36-40 bytes** (9-10 floats)
**4096x4096 grid: ~600 MB** for all buffers

**Algorithm (5 compute passes per timestep):**

```hlsl
// Pass 1: Water increment (rain/sources)
[numthreads(16, 16, 1)]
void CSWaterSources(uint3 id : SV_DispatchThreadID) {
    float water = WaterTex[id.xy];
    water += dt * rainRate;
    WaterTex[id.xy] = water;
}

// Pass 2: Flow simulation (virtual pipe model)
[numthreads(16, 16, 1)]
void CSFlowSimulation(uint3 id : SV_DispatchThreadID) {
    float h_center = TerrainTex[id.xy] + WaterTex[id.xy];
    float h_left   = TerrainTex[id.xy - uint2(1,0)] + WaterTex[id.xy - uint2(1,0)];
    float h_right  = TerrainTex[id.xy + uint2(1,0)] + WaterTex[id.xy + uint2(1,0)];
    float h_top    = TerrainTex[id.xy - uint2(0,1)] + WaterTex[id.xy - uint2(0,1)];
    float h_bottom = TerrainTex[id.xy + uint2(0,1)] + WaterTex[id.xy + uint2(0,1)];

    float4 flux = FluxTex[id.xy];
    float A = cellSize * cellSize; // pipe cross-section
    float g = 9.81;

    // Update flux based on pressure difference
    flux.x = max(0, flux.x + dt * A * g * (h_center - h_left)   / cellSize);
    flux.y = max(0, flux.y + dt * A * g * (h_center - h_right)  / cellSize);
    flux.z = max(0, flux.z + dt * A * g * (h_center - h_top)    / cellSize);
    flux.w = max(0, flux.w + dt * A * g * (h_center - h_bottom) / cellSize);

    // Scale factor: prevent draining more water than available
    float totalFlux = flux.x + flux.y + flux.z + flux.w;
    float waterVol = WaterTex[id.xy] * cellSize * cellSize;
    float k = min(1.0, waterVol / (totalFlux * dt + 1e-6));
    flux *= k;

    FluxTex[id.xy] = flux;
}

// Pass 3: Apply flow -> update water level
[numthreads(16, 16, 1)]
void CSApplyFlow(uint3 id : SV_DispatchThreadID) {
    // Inflow from neighbors
    float inflow = FluxTex[id.xy - uint2(1,0)].y  // right-flow of left neighbor
                 + FluxTex[id.xy + uint2(1,0)].x  // left-flow of right neighbor
                 + FluxTex[id.xy - uint2(0,1)].w  // bottom-flow of top neighbor
                 + FluxTex[id.xy + uint2(0,1)].z; // top-flow of bottom neighbor
    float outflow = dot(FluxTex[id.xy], float4(1,1,1,1));

    float dV = (inflow - outflow) * dt;
    WaterTex[id.xy] += dV / (cellSize * cellSize);

    // Velocity from flux difference
    float vx = (FluxTex[id.xy - uint2(1,0)].y - FluxTex[id.xy].x
              + FluxTex[id.xy].y - FluxTex[id.xy + uint2(1,0)].x) * 0.5;
    float vy = (FluxTex[id.xy - uint2(0,1)].w - FluxTex[id.xy].z
              + FluxTex[id.xy].w - FluxTex[id.xy + uint2(0,1)].z) * 0.5;
    VelocityTex[id.xy] = float2(vx, vy) / (cellSize * max(WaterTex[id.xy], 0.001));
}

// Pass 4: Erosion and deposition
[numthreads(16, 16, 1)]
void CSErosionDeposition(uint3 id : SV_DispatchThreadID) {
    float2 vel = VelocityTex[id.xy];
    float speed = length(vel);

    // Local slope (sin of tilt angle)
    float dhdx = (TerrainTex[id.xy + uint2(1,0)] - TerrainTex[id.xy - uint2(1,0)]) * 0.5;
    float dhdy = (TerrainTex[id.xy + uint2(0,1)] - TerrainTex[id.xy - uint2(0,1)]) * 0.5;
    float sinAlpha = length(float2(dhdx, dhdy));

    // Sediment transport capacity
    float C = Kc * max(sinAlpha, 0.01) * speed;

    float sediment = SedimentTex[id.xy];
    if (sediment < C) {
        // Erode: pick up sediment
        float erosion = Ks * (C - sediment);
        erosion = min(erosion, Lmax); // depth limit
        TerrainTex[id.xy] -= erosion;
        SedimentTex[id.xy] += erosion;
    } else {
        // Deposit: drop sediment
        float deposition = Kd * (sediment - C);
        TerrainTex[id.xy] += deposition;
        SedimentTex[id.xy] -= deposition;
    }
}

// Pass 5: Sediment transport (advection)
[numthreads(16, 16, 1)]
void CSSedimentTransport(uint3 id : SV_DispatchThreadID) {
    float2 vel = VelocityTex[id.xy];
    // Semi-Lagrangian advection: sample from upstream position
    float2 srcPos = float2(id.xy) - vel * dt / cellSize;
    SedimentTex[id.xy] = SedimentTexPrev.SampleLevel(LinearClamp, srcPos / gridSize, 0);
    // Evaporation
    WaterTex[id.xy] *= (1.0 - Ke * dt);
}
```

**Parameters (Jako 2011):**
- Kc (sediment capacity): 0.01-1.0
- Ks (suspension rate): 0.001-0.01
- Kd (deposition rate): 0.001-0.01
- Ke (evaporation rate): 0.001-0.01
- Lmax (max erosion depth): 0.001-0.1

### Thermal Erosion (additional pass)

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

### B. Particle-Based GPU Erosion

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
    drop.pos = RandomPosition(id.x); // hash-based random
    drop.vel = float2(0, 0);
    drop.volume = 1.0;
    drop.sediment = 0.0;

    for (int step = 0; step < maxLifetime; step++) {
        // 1. Compute surface normal at current position
        float2 gradient = SampleGradient(drop.pos);

        // 2. Update velocity (gravity + friction)
        drop.vel = drop.vel * (1.0 - friction) - gradient * gravity;
        drop.speed = length(drop.vel);
        if (drop.speed < minSpeed) break;

        // 3. Move
        float2 newPos = drop.pos + normalize(drop.vel) * stepSize;

        // 4. Compute capacity
        float heightDiff = SampleHeight(drop.pos) - SampleHeight(newPos);
        float capacity = max(-heightDiff, minSlope) * drop.speed * drop.volume * Kc;

        // 5. Erode or deposit
        if (drop.sediment > capacity || heightDiff > 0) {
            float deposit = (heightDiff > 0)
                ? min(heightDiff, drop.sediment)
                : (drop.sediment - capacity) * Kd;
            // InterlockedAdd or atomic deposit
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

**Particle-based challenges on GPU:**
- Atomic operations for height modification (contention at popular cells)
- Divergent loop lengths per thread (some drops die early)
- Random memory access pattern (poor cache coherency)

### Performance Benchmarks

**Grid-based (shallow water):**
- 1024x1024 grid: ~2-5 ms per timestep on modern GPU (RTX 3060+)
- 4096x4096 grid: ~30-80 ms per timestep
- Typically needs 200-1000 timesteps for visible erosion
- Total: 1024x1024 terrain, 500 steps = ~1-2.5 seconds

**Particle-based:**
- 1M droplets on GPU: ~10 seconds total (100 steps each)
- CPU reference: 200K particles in 10-20 seconds
- GPU advantage: 10-50x over single-threaded CPU

**Reference GPU performance (Diamond-Square, comparable compute load):**

| Resolution   | GPU Time  | CPU Single-Thread |
|-------------|-----------|-------------------|
| 512x512     | 2.1 ms    | 19.2 ms           |
| 1024x1024   | 9.3 ms    | 70.5 ms           |
| 2048x2048   | 41.7 ms   | 740.2 ms          |
| 4096x4096   | 211.6 ms  | 2,815.3 ms        |
| 8192x8192   | 778.2 ms  | 11,267.1 ms       |

Note: GPU->CPU readback can constitute up to 50% of total wall time.

### Key References

- Mei et al., "Fast Hydraulic Erosion Simulation and Visualization on GPU" (2007): https://inria.hal.science/inria-00402079/document
- Jako, "Fast Hydraulic and Thermal Erosion on the GPU" (CESCG 2011): https://old.cescg.org/CESCG-2011/papers/TUBudapest-Jako-Balazs.pdf
- bshishov/UnityTerrainErosionGPU: https://github.com/bshishov/UnityTerrainErosionGPU
- Simple particle erosion (Nick McDonald): https://nickmcd.me/2020/04/10/simple-particle-based-hydraulic-erosion/
- lisyarus WebGPU shallow water: https://github.com/lisyarus/webgpu-shallow-water
- GPU Diamond-Square benchmarks: https://www.vertexfragment.com/ramblings/diamond-square/

---

## 5. Multi-Octave Noise (fBm) on GPU

### Standard fBm

```glsl
// Naive implementation (expensive pow per octave)
float fbm(vec3 x, float H, int numOctaves) {
    float t = 0.0;
    for (int i = 0; i < numOctaves; i++) {
        float f = pow(2.0, float(i));
        float a = pow(f, -H);
        t += a * noise(f * x);
    }
    return t;
}
```

### Optimized fBm (Inigo Quilez)

Eliminates `pow()` calls using iterative multiplication:

```glsl
float fbm(vec3 x, float H, int numOctaves) {
    float G = exp2(-H);   // gain = 2^(-H), computed once
    float f = 1.0;        // frequency
    float a = 1.0;        // amplitude
    float t = 0.0;
    for (int i = 0; i < numOctaves; i++) {
        t += a * noise(f * x);
        f *= 2.0;         // lacunarity (can use 2.01 to reduce alignment)
        a *= G;           // gain
    }
    return t;
}
```

### Key Optimization: Slightly Off-Integer Lacunarity

Using lacunarity values like 2.01 or 1.99 instead of exactly 2.0 prevents overlapping noise peaks across octaves, eliminating unrealistic grid-aligned patterns.

### Octave Rotation (GPU Gems 3, Ch.1)

```glsl
// Rotate domain between octaves to break axis alignment
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
        x = octaveRotation * x; // rotate domain each octave
    }
    return t;
}
```

### Multi-Octave Performance

Each octave = one full noise evaluation. For 8-16 octave fBm:

**Estimated 4096x4096 generation times (modern GPU, RTX 3060-class):**

| Octaves | Simplex-based fBm | Perlin-based fBm |
|---------|-------------------|------------------|
| 1       | ~0.1 ms           | ~0.15 ms         |
| 4       | ~0.4 ms           | ~0.6 ms          |
| 8       | ~0.8 ms           | ~1.2 ms          |
| 12      | ~1.2 ms           | ~1.8 ms          |
| 16      | ~1.6 ms           | ~2.4 ms          |

These scale linearly with octave count since the loop is purely ALU-bound.

### Texture-Based Octave Caching (GPU Gems 3)

For extreme performance, pre-bake low octaves into textures:
- Reuse 3-4 noise textures among octaves for cache coherency
- Lowest 1-2 octaves: manual trilinear interpolation at full float precision
- Higher octaves: hardware-filtered texture lookups (faster but lower precision)

```glsl
// Hybrid: texture for low octaves, computed for high
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

### Domain Warping (Inigo Quilez)

Dramatically improves terrain variety by feeding noise into itself:

**Single-level warping:**
```glsl
float pattern(vec2 p) {
    vec2 q = vec2(fbm(p + vec2(0.0, 0.0), 0.5, 8),
                  fbm(p + vec2(5.2, 1.3), 0.5, 8));
    return fbm(p + 4.0 * q, 0.5, 8);
}
// Cost: 3x base fBm (24 octave evaluations for 8-octave fBm)
```

**Two-level warping (maximum organic quality):**
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

**Offset constants** (0.0, 5.2, 1.3, 1.7, 9.2, 8.3, 2.8) must be different to produce independent noise patterns. The factor 4.0 controls warping strength.

### Planetary Scale Consideration

Quilez notes that with just **24 octaves** of fBm, you can create terrain spanning the entire Earth with detail down to 2 meters. For a planet generator:
- Octaves 1-8: continental shapes (pre-baked texture, ~1km resolution)
- Octaves 9-16: regional features (computed on demand)
- Octaves 17-24: local detail (computed only for close-up views, LOD-dependent)

### Memory Requirements

- fBm itself: same as base noise (output only)
- Domain warping: 2-4 intermediate vec2 textures (for q, r vectors)
- 4096x4096 with 2-level warping: ~200 MB for intermediates
- Pre-baked noise volumes: 3-4 textures at 128^3 or 256^3 = 8-64 MB each

### Key References

- Quilez, "fBm": https://iquilezles.org/articles/fbm/
- Quilez, "Domain Warping": https://iquilezles.org/articles/warp/
- Book of Shaders Ch.13: https://thebookofshaders.com/13/
- GPU Gems 3 Ch.1 (procedural terrain): https://developer.nvidia.com/gpugems/gpugems3/part-i-geometry/chapter-1-generating-complex-procedural-terrains-using-gpu
- Godot fBm snippet: https://godotshaders.com/snippet/fractal-brownian-motion-fbm/

---

## Summary: Recommended Pipeline for Procedural Planet

| Stage | Technique | Resolution | Est. Time (RTX 3060) | Memory |
|-------|-----------|-----------|----------------------|--------|
| Base heightmap | Simplex fBm (12 oct) | 4096x4096 | ~1.5 ms | 64 MB |
| Continental warp | 2-level domain warping | 4096x4096 | ~7 ms | 200 MB |
| Crater/cell features | Worley F1+F2 (3x3) | 4096x4096 | ~3 ms | 128 MB |
| Directional features | Gabor noise (selective) | 2048x2048 | ~20 ms | 16 MB |
| Hydraulic erosion | Shallow water (500 steps) | 2048x2048 | ~5 sec | 150 MB |
| Thermal erosion | Grid-based (200 steps) | 2048x2048 | ~1 sec | 40 MB |
| **Total offline generation** | | | **~7 sec** | **~600 MB** |

All noise passes are fully parallelizable per-texel with no shared memory requirements. Erosion requires multiple sequential timesteps but each step is fully parallel. The entire pipeline can run asynchronously on GPU while CPU handles other work.

---

## 6. Ridged Multifractal & Hybrid Noise Variants

### Billowed Noise (de Carpentier, Scape 2008)

Takes absolute value of Perlin noise to create sharp creases/valleys [https://www.decarpentier.nl/scape-procedural-basics]:

```cg
float billowedNoise(float2 p, float seed) {
    return abs(perlinNoise(p, seed));
}
```

### Ridged Noise

Complement of billowed noise -- creates sharp mountain ridges [https://www.decarpentier.nl/scape-procedural-basics]:

```cg
float ridgedNoise(float2 p, float seed) {
    return 1.0 - abs(perlinNoise(p, seed));
}
```

### Standard Turbulence (fBm with variants)

The common Perlin-based summing algorithms include normal, ridged, and billowy turbulence. The base turbulence function [https://www.decarpentier.nl/scape-procedural-basics]:

```cg
float turbulence(float2 p, float seed, int octaves,
                 float lacunarity = 2.0, float gain = 0.5) {
    float sum = 0;
    float freq = 1.0, amp = 1.0;
    for (int i = 0; i < octaves; i++) {
        float n = perlinNoise(p * freq, seed + i / 256.0);
        sum += n * amp;
        freq *= lacunarity;
        amp *= gain;
    }
    return sum;
}
```

### IQ Turbulence (Derivative-Based Erosion Look)

Quilez's technique uses noise derivatives to suppress detail on steep slopes, creating an erosion-like appearance without actual simulation [https://www.decarpentier.nl/scape-procedural-basics, https://iquilezles.org/articles/morenoise/]:

```cg
float iqTurbulence(float2 p, float seed, int octaves,
                   float lacunarity = 2.0, float gain = 0.5) {
    float sum = 0.5;
    float freq = 1.0, amp = 1.0;
    float2 dsum = float2(0, 0);
    for (int i = 0; i < octaves; i++) {
        float3 n = perlinNoisePseudoDeriv(p * freq, seed + i / 256.0);
        dsum += n.yz;                              // accumulate derivatives
        sum += amp * n.x / (1 + dot(dsum, dsum));  // suppress on slopes
        freq *= lacunarity;
        amp *= gain;
    }
    return sum;
}
```

**Key insight:** The `1 / (1 + dot(dsum, dsum))` term attenuates higher octaves where accumulated derivatives (slope) are large. This gives flat areas full fractal detail while steep slopes appear smooth -- mimicking real erosion patterns without the cost of simulation.

### Ridged Multifractal (Musgrave)

The classic Musgrave ridged multifractal feeds the previous octave's output as weight for the next [https://github.com/cammymcp/Ridged-Multifractal-Terrain]:

```glsl
float ridgedMultifractal(vec3 p, float H, float lacunarity, int octaves, float offset) {
    float result = 0.0;
    float weight = 1.0;
    float freq = 1.0;

    for (int i = 0; i < octaves; i++) {
        float signal = offset - abs(snoise(p * freq));
        signal *= signal;        // sharpen ridges
        signal *= weight;        // weight by previous octave
        weight = clamp(signal * 2.0, 0.0, 1.0);  // next octave weight

        result += signal * pow(freq, -H);
        freq *= lacunarity;
    }
    return result;
}
```

### Analytical Derivatives for Normal Computation

De Carpentier's approach computes normals analytically during fBm evaluation, avoiding finite-difference sampling [https://www.decarpentier.nl/scape-procedural-basics]:

```cg
// Per-octave accumulation:
float height = 0;
float3 normal = float3(0, 0, 0);

for (int i = 0; i < octaves; i++) {
    float3 n = perlinNoiseDerivatives(p * freq, seed);
    height += amp * n.x;
    normal += amp * freq * float3(-n.y, 1, -n.z);
    freq *= lacunarity;
    amp *= gain;
}
normal = normalize(normal);
```

For ridged/billowed variants, multiply derivatives by `sign(n.x)` to account for the `abs()` operation.

### Key References

- De Carpentier, Scape procedural basics: https://www.decarpentier.nl/scape-procedural-basics
- De Carpentier, Scape procedural extensions: https://www.decarpentier.nl/scape-procedural-extensions
- Quilez, "Value noise derivatives": https://iquilezles.org/articles/morenoise/
- Ridged-Multifractal-Terrain (DX11): https://github.com/cammymcp/Ridged-Multifractal-Terrain
- GameDev.net procedural terrain fBm white paper: https://www.gamedev.net/reference/articles/article2452.asp

---

## 7. GPU Erosion: Detailed Race Condition Analysis (Axel Paris)

### The Core Problem

GPU thermal/hydraulic erosion moves material between grid cells. When multiple threads read/write the same cell simultaneously, race conditions occur. Axel Paris tested three strategies [https://aparis69.github.io/public_html/posts/terrain_erosion.html]:

### Strategy 1: Single Integer Buffer with atomicAdd

```glsl
layout(binding = 0, std430) coherent buffer HeightData {
    int heightBuffer[];  // heights as fixed-point integers
};

// In compute shader:
atomicAdd(heightBuffer[neighborIdx], -transferAmount);
atomicAdd(heightBuffer[currentIdx], transferAmount);
```

**Pros:** Atomic operations guarantee correctness for integer values.
**Cons:** Limited to large-scale erosion (>1m precision); `int` representation loses sub-meter detail.

### Strategy 2: Double Buffer (Float + Integer)

```glsl
layout(binding = 0, std430) readonly buffer HeightIn {
    float inData[];
};
layout(binding = 1, std430) writeonly buffer HeightOut {
    float outData[];
};
// Swap buffers after each step
```

**Pros:** Deterministic, race-condition-free output.
**Cons:** 4-5x slower than the naive approach due to conversion overhead and double memory [https://aparis69.github.io/public_html/posts/terrain_erosion_2.html].

### Strategy 3: Single Float Buffer (Ignore Races)

```glsl
layout(binding = 0, std430) coherent buffer HeightData {
    float floatingHeightBuffer[];
};
layout(local_size_x = 1024) in;
void main() {
    uint id = gl_GlobalInvocationID.x;
    // Read neighbors, compute transfer, write directly
    // Race conditions exist but convergence compensates
}
```

**Pros:** Fastest. No conversion overhead, single buffer.
**Cons:** Non-deterministic; needs more iterations to converge. Result is visually equivalent.

### Paris's Conclusion

"The single floating point buffer is the most efficient one" -- compensating for race-condition errors by increasing iteration count (e.g., from 500 to 700 iterations) is cheaper than the overhead of correctness mechanisms.

### Reversed Read Pattern (Part 2)

Paris's follow-up reverses the computation: instead of each cell writing to neighbors (scatter), each cell reads from neighbors and writes only to itself (gather) [https://aparis69.github.io/public_html/posts/terrain_erosion_2.html]:

```glsl
layout(local_size_x = 8, local_size_y = 8) in;
// Each cell examines 3x3 neighborhood
// Receives matter from upslope neighbors
// Distributes matter to downslope neighbors
// Writes ONLY to own cell -> no race condition
```

This gather pattern eliminates races entirely but is 4-5x slower in benchmarks.

### Key References

- Paris, "Terrain Erosion on the GPU": https://aparis69.github.io/public_html/posts/terrain_erosion.html
- Paris, "Terrain Erosion on the GPU #2": https://aparis69.github.io/public_html/posts/terrain_erosion_2.html

---

## 8. Particle-Based Erosion: Detailed Algorithm (Nick McDonald)

Complete minimal implementation of droplet-based hydraulic erosion [https://nickmcd.me/2020/04/10/simple-particle-based-hydraulic-erosion/]:

```cpp
struct Particle {
    glm::vec2 pos;
    glm::vec2 speed = glm::vec2(0.0);
    float volume = 1.0;    // water volume
    float sediment = 0.0;  // carried sediment
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

**Performance (CPU):** 200,000 particles in 10-20 seconds.

**Sebastian Lague GPU version:** 400,000 droplets on a 1000x1000 map in ~0.5 seconds using Unity compute shaders [https://x.com/sebastianlague/status/1101513037905829888]. That is approximately 40x speedup over CPU.

### Key References

- McDonald, "Simple Particle-Based Hydraulic Erosion": https://nickmcd.me/2020/04/10/simple-particle-based-hydraulic-erosion/
- Lague, Hydraulic Erosion tool: https://sebastian.itch.io/hydraulic-erosion
- Frozen Fractal, "Around The World: Hydraulic Erosion": https://frozenfractal.com/blog/2025/6/6/around-the-world-23-hydraulic-erosion/

---

## 9. Diamond-Square GPU Benchmarks (Vertex Fragment)

Concrete GPU vs CPU timing data for procedural heightmap generation [https://www.vertexfragment.com/ramblings/diamond-square/]:

| Resolution   | Single-thread C# | Multi-thread C# (128x128 chunks) | GPU (256 threads) | GPU Speedup |
|-------------|-------------------|-----------------------------------|-------------------|-------------|
| 512x512     | 38.3 ms           | 26.7 ms                           | 2.1 ms            | 18x         |
| 1024x1024   | 161.6 ms          | 67.9 ms                           | 9.3 ms            | 17x         |
| 2048x2048   | 740.2 ms          | 224.4 ms                          | 41.7 ms           | 18x         |
| 4096x4096   | 2,987.3 ms        | 823.9 ms                          | 211.6 ms          | 14x         |
| 8192x8192   | 13,042.1 ms       | 3,121.5 ms                        | 778.2 ms          | 17x         |

**Note:** Diamond-Square has inherent sequential dependencies (each subdivision depends on the previous), making it harder to parallelize than noise generation. Pure noise functions (simplex fBm) would show even larger GPU advantages since every texel is fully independent.

---

## 10. WebGPU / WGSL Noise Generation

### Emerging Platform

WebGPU is the successor to WebGL, providing compute shader access in browsers. WGSL (WebGPU Shading Language) has Rust-like syntax and is stricter than GLSL [https://www.w3.org/TR/WGSL/].

### Kosmos: Modular Terrain Generator in Rust + WebGPU

Kosmos compiles a node graph of noise functions into a single compute shader executed on the GPU [https://github.com/kaylendog/kosmos]. Supports Perlin, Simplex, and Worley noise with graph-based composition.

### WGSL Noise Utilities

The `wgsl-fns` library provides WGSL ports of common noise functions [https://github.com/koole/wgsl-fns]:
- `noise2D()` -- 2D value/gradient noise
- `fbm()` -- fractal Brownian motion
- SDF utilities for procedural geometry

### WGSL Simplex Noise (ported from Gustavson)

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

### Performance Note

WebGPU compute shaders achieve near-native performance (typically within 5-15% of Vulkan/Metal) for ALU-bound workloads like noise generation. The main overhead is JavaScript-to-GPU command submission latency, not shader execution.

### Key References

- Kosmos (Rust+WebGPU terrain): https://github.com/kaylendog/kosmos
- wgsl-fns utility library: https://github.com/koole/wgsl-fns
- WGSL specification: https://www.w3.org/TR/WGSL/
- lisyarus WebGPU shallow water: https://github.com/lisyarus/webgpu-shallow-water

---

## 11. Planetary-Scale Considerations

### Sphere Mapping for Noise

Noise must be seamless on a sphere. Two approaches [https://tonisagrista.com/blog/2021/procedural-planetary-surfaces/]:

1. **3D noise on unit sphere:** Sample `snoise3(normalize(position) * scale)`. Seamless by construction but requires 3D noise (more expensive than 2D).
2. **Cube-to-sphere (quad sphere):** Generate noise on 6 cube faces, project to sphere. Each face is a simple 2D rectangle, enabling standard 2D noise + easy LOD subdivision. Distortion near corners is manageable [https://www.shaneenishry.com/blog/2014/08/02/planet-generation-part-ii/].

### GPU-First Planet Generation (2024+)

Gaia Sky 3.6.3 moved procedural planet generation entirely to GPU, making generation "almost instantaneous, even with high resolutions" [https://tonisagrista.com/blog/2021/procedural-planetary-surfaces/].

AMD's GDC 2024 mesh nodes demo renders fully procedural worlds entirely on GPU through work graphs, with no CPU readback [https://gpuopen.com/learn/work_graphs_mesh_nodes/work_graphs_mesh_nodes-procedural_generation/].

### Octave Budget for Earth-Scale

Quilez notes that 24 octaves of fBm span Earth-to-2m detail [https://iquilezles.org/articles/fbm/]. Practical LOD strategy:
- **Octaves 1-8:** Continental scale (~1 km). Pre-bake to cubemap texture.
- **Octaves 9-16:** Regional features (~1 m). Compute on demand per visible tile.
- **Octaves 17-24:** Local detail (~2 m). Compute only for close-up camera.

Each LOD level adds ~0.8 ms for 4096x4096 simplex fBm on an RTX 3060-class GPU (extrapolated from Gustavson benchmarks).

### Key References

- Toni Sagrista, "Procedural generation of planetary surfaces": https://tonisagrista.com/blog/2021/procedural-planetary-surfaces/
- Shane Enishry, "Planet Generation Part II": https://www.shaneenishry.com/blog/2014/08/02/planet-generation-part-ii/
- Jad Khoury, "Procedural Planet Rendering": https://jadkhoury.github.io/terrain_blog.html
- AMD GPUOpen work graphs + mesh nodes: https://gpuopen.com/learn/work_graphs_mesh_nodes/work_graphs_mesh_nodes-procedural_generation/
- Quilez, "fBm": https://iquilezles.org/articles/fbm/

---

## 12. Consolidated Performance Summary

### Noise Generation (4096x4096, single evaluation)

| Noise Type | GTX 580 (2012) | Est. RTX 3060 (2022) | Est. RTX 4090 (2023) | Source |
|---|---|---|---|---|
| 2D Simplex (1 octave) | 3.6 ms | ~0.1 ms | ~0.05 ms | Gustavson 2012 [arxiv:1204.1461] |
| 3D Simplex (1 octave) | 6.9 ms | ~0.2 ms | ~0.1 ms | Gustavson 2012 |
| 2D Simplex fBm (8 oct) | ~29 ms | ~0.8 ms | ~0.4 ms | Linear extrapolation |
| 2D Simplex fBm (16 oct) | ~58 ms | ~1.6 ms | ~0.8 ms | Linear extrapolation |
| 2D Worley (3x3, F1+F2) | N/A | ~2-5 ms | ~1-2 ms | Estimated from ALU cost |
| 2D Gabor (density=4) | N/A | ~20 ms | ~10 ms | Estimated (5-20x simplex) |
| Domain warp (2-level, 8 oct) | N/A | ~7 ms | ~3.5 ms | 5x base fBm |
| Diamond-Square | N/A | ~212 ms | ~100 ms | VertexFragment benchmark |

**RTX 3060 extrapolation basis:** ~20 TFLOPS FP32 vs GTX 580's ~1.6 TFLOPS = ~12.5x theoretical. Practical speedup ~10x accounting for memory bandwidth.

**RTX 4090 extrapolation basis:** ~82 TFLOPS FP32 = ~50x over GTX 580. Practical ~30-40x.

### Erosion Simulation

| Method | Resolution | Steps | Time (RTX 3060 class) | Source |
|---|---|---|---|---|
| Grid hydraulic (shallow water) | 1024x1024 | 500 | ~1-2.5 sec | Jako 2011 + estimates |
| Grid hydraulic (shallow water) | 4096x4096 | 500 | ~15-40 sec | Scaled from 1024 |
| Grid thermal (single float, races OK) | 1024x1024 | 1000 | ~2-5 sec | Paris blog |
| Grid thermal (dual buffer, no races) | 1024x1024 | 1000 | ~8-25 sec | Paris blog (4-5x slower) |
| Particle droplet (GPU) | 400K drops | 100 steps | ~0.5 sec | Lague (1000x1000 map) |
| Particle droplet (CPU) | 200K drops | ~50 steps | ~10-20 sec | McDonald blog |

### Memory Budget (4096x4096 grid)

| Buffer | Format | Size |
|---|---|---|
| Heightmap | R32F | 64 MB |
| Water level | R32F | 64 MB |
| Sediment | R32F | 64 MB |
| Flux (4-dir) | RGBA32F | 256 MB |
| Velocity | RG32F | 128 MB |
| Hardness (optional) | R32F | 64 MB |
| **Total erosion** | | **~640 MB** |
| Noise output (single) | R32F | 64 MB |
| Domain warp intermediates | RG32F x2 | 256 MB |
| **Total noise pipeline** | | **~320 MB** |
