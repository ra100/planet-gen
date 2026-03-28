# Procedural Map Generation Techniques for Planet Rendering

> Deep research report -- 28 March 2026
> 15 sources consulted; shader pseudocode included

---

## Table of Contents

1. [Height Maps](#1-height-maps)
2. [Albedo / Color Maps](#2-albedo--color-maps)
3. [Roughness Maps](#3-roughness-maps)
4. [Normal Maps](#4-normal-maps)
5. [Cloud Maps](#5-cloud-maps)
6. [Atmospheric Scattering](#6-atmospheric-scattering)
7. [Ocean Maps](#7-ocean-maps)
8. [Shader Pseudocode](#8-shader-pseudocode)
9. [Performance Summary](#9-performance-summary)
10. [References](#references)

---

## 1. Height Maps

### 1.1 Multi-Octave Noise (Fractional Brownian Motion)

fBm sums multiple octaves of coherent noise (typically Perlin or Simplex) at increasing frequency and decreasing amplitude [1][5][6]:

```
h(p) = SUM_i  a_i * noise(p * f_i)
where  a_i = persistence^i,  f_i = lacunarity^i
```

- **Lacunarity** (typically 2.0): frequency multiplier per octave.
- **Persistence** (typically 0.5): amplitude multiplier per octave.
- **Octaves**: 6-12 for planetary terrain; more octaves add fine detail at higher cost.

fBm alone produces rounded, "blobby" terrain that lacks ridge structures and valleys [6].

### 1.2 Ridge Noise (Ridged Multifractal)

Ridge noise modifies Perlin noise by taking `1 - abs(noise(p))`, creating sharp ridges at zero crossings [2][6]. The ridged multifractal feeds back each octave's output as a weight for the next, so rough areas (ridges) accumulate more detail while flat areas (valleys) stay smooth:

```
signal = offset - abs(noise(p * f_i))
signal^2 * weight
weight = clamp(signal * gain, 0, 1)
```

This was introduced by Musgrave [2] and remains the standard technique for mountain-range generation. The resulting terrain has convincing peaks, saddle points, and valley floors.

### 1.3 Domain Warping

Domain warping, popularized by Inigo Quilez [3], distorts the input coordinates of an fBm with another fBm before evaluation. The basic form is `f(p + h(p))` where h is itself a noise function:

```glsl
vec2 q = vec2( fbm(p + vec2(0.0, 0.0)),
               fbm(p + vec2(5.2, 1.3)) );
float result = fbm(p + 4.0 * q);
```

Multi-level warping applies two stages for more organic distortion [3]:

```glsl
vec2 q = vec2( fbm(p + vec2(0.0,0.0)), fbm(p + vec2(5.2,1.3)) );
vec2 r = vec2( fbm(p + 4.0*q + vec2(1.7,9.2)),
               fbm(p + 4.0*q + vec2(8.3,2.8)) );
float result = fbm(p + 4.0 * r);
```

This costs 3-7 noise evaluations per sample (depending on fractal depth in the warping functions) but produces terrain that resembles tectonic deformation [4].

### 1.4 Hydraulic Erosion Post-Processing

Hydraulic erosion transforms noise-generated height maps into terrain with realistic river valleys, alluvial fans, and sediment deposits. Two main approaches exist:

**Particle-based (droplet) erosion** [7][8]: Simulates individual raindrops that pick up and deposit sediment as they flow downhill. The algorithm per droplet:

1. Place droplet at a random location on the height map.
2. Compute the gradient via bilinear interpolation of surrounding height nodes.
3. Move the droplet along the gradient direction.
4. Compute sediment carry capacity: `C = speed * water * slope * capacityFactor`.
5. If carried sediment > C, deposit the excess. If < C, erode from the terrain.
6. Apply evaporation to reduce the droplet's water volume.
7. Repeat for a fixed lifetime (typically 30-60 steps).

Sebastian Lague's open-source Unity implementation [7] processes 200k-2M droplets and is widely used as a reference.

**Grid-based (shallow water) erosion** [9]: Solves the shallow-water equations on the full grid, computing velocity fields that drive erosion, transport, and deposition in parallel. This maps well to GPU compute shaders.

| Method | Grid Size | Performance | Hardware |
|--------|-----------|-------------|----------|
| GPU grid-based [9] | 1024x1024 | ~2 ms/cycle | GTX 1050 |
| GPU grid-based [9] | 4096x4096 | memory-limited | GTX 1050 (2 GB) |
| CPU droplet [7] | 512x512 | ~seconds for 200k drops | Modern CPU |

Key optimization: switching from array-of-structs to struct-of-arrays on GPU yields a **5-10x speedup** due to coalesced memory access [9].

---

## 2. Albedo / Color Maps

### 2.1 Biome-Based Coloring Pipelines

The dominant approach for procedural planet coloring uses biome classification based on terrain properties [10][11]:

1. **Classify each texel** by height (elevation zones) and slope (cliff detection).
2. **Add latitude** to distinguish polar, temperate, and tropical zones.
3. **Sample a biome gradient texture** where the X-axis encodes moisture/temperature and the Y-axis encodes elevation.
4. **Blend textures** per biome using smooth-step transitions.

A typical pipeline (from Tim Coster's Unity tutorial [10]):
- Separate RGB channels of a gradient control biome weights (R = polar, G = forest, B = desert).
- Noise is multiplied with the Y-position before gradient lookup to create irregular biome boundaries.
- Each biome has up to 5 detail textures blended by the biome weight channel.

### 2.2 Spectral Data to RGB Conversion

For physically-based planetary albedo [10][12]:

- Real surface spectral reflectance data (e.g., USGS spectral library) can be converted to sRGB via CIE color matching functions.
- The conversion integrates spectral power against CIE XYZ matching functions, then transforms XYZ to sRGB.
- In practice, artists use reference albedo values: fresh snow ~0.8-0.9, vegetation ~0.1-0.2, ocean ~0.06, desert sand ~0.3-0.4.
- PBR albedo values must stay in linear color space; sRGB conversion is: `Linear = (sRGB/255)^2.2` [12].

### 2.3 Slope and Height-Based Blending

Jadkhoury's planet renderer [5] uses normal vectors to determine surface angle, then blends textures:
- Steep slopes > 45 degrees: rock texture.
- High altitudes + shallow slopes: snow.
- Low altitudes: grass/soil.
- Smooth interpolation between zones using `smoothstep()`.

---

## 3. Roughness Maps

### 3.1 Surface Material Properties

Roughness in PBR defines microsurface scattering: 0.0 = mirror smooth, 1.0 = fully diffuse [12]. Typical planetary roughness values:

| Surface Type | Roughness |
|-------------|-----------|
| Calm water | 0.02-0.05 |
| Wet rock | 0.15-0.25 |
| Dry rock | 0.4-0.6 |
| Sand | 0.7-0.85 |
| Snow (fresh) | 0.8-0.95 |
| Vegetation | 0.6-0.8 |

### 3.2 Procedural Roughness Generation

Roughness maps for planets can be derived from terrain properties rather than authored manually:

1. **Height-based**: Lower elevation (wet areas) = smoother; higher elevation = rougher.
2. **Slope-based**: Steep slopes (exposed rock) have medium roughness; flat areas vary by biome.
3. **Curvature-based**: Convex regions (ridges) are more weathered (rougher); concave regions (valleys) accumulate sediment (smoother) [13].
4. **Erosion-derived**: Davidar's GPU planet simulation [13] applies stream-power-law erosion, and the resulting sediment distribution directly informs surface roughness.

A practical shader approach:
```
roughness = base_roughness_for_biome
roughness += slope_factor * 0.2
roughness += curvature * 0.1
roughness += noise(p * high_freq) * 0.05  // micro-detail
roughness = clamp(roughness, 0.02, 1.0)
```

### 3.3 Weathering Effects Simulation

Weathering can be approximated by:
- **Ambient occlusion** as a proxy for moisture accumulation (dark crevices = wet = smoother).
- **Exposure maps** (computed from terrain normals vs. sky hemisphere): exposed surfaces are weathered rougher.
- **Age-based**: simulation timestep modulates roughness over geological time in full planet simulations [13].

---

## 4. Normal Maps

### 4.1 Derived from Height Maps

Two standard methods convert a height map to a normal map:

**Central differences** (fast, simple):
```
dh/dx = (h[x+1, y] - h[x-1, y]) / 2
dh/dy = (h[x, y+1] - h[x, y-1]) / 2
normal = normalize(vec3(-dh/dx, -dh/dy, 1.0))
```

**Sobel filter** (3x3, better noise rejection) [14]:
The Sobel operator applies weighted 3x3 kernels for horizontal and vertical gradients:
```
Gx = [[-1, 0, 1], [-2, 0, 2], [-1, 0, 1]]
Gy = [[-1, -2, -1], [0, 0, 0], [1, 2, 1]]
```
Convolve these with the height map, then:
```
normal = normalize(vec3(-Gx_result * strength, -Gy_result * strength, 1.0))
```

The Sobel approach provides smoother results by weighting diagonal neighbors, reducing staircase artifacts at the cost of slightly blurred edges [14].

**Scharr filter** (3x3, higher accuracy):
Uses weights `[[-3, 0, 3], [-10, 0, 10], [-3, 0, 3]]` for better rotational symmetry.

### 4.2 Fully Procedural Normals

Instead of generating a height map first, analytical derivatives of the noise function can be computed directly. For Perlin noise, the gradient is available analytically, avoiding the finite-difference approximation entirely. This:
- Eliminates the height-map resolution limit.
- Costs roughly 2x a single noise evaluation (for both value and gradient).
- Is resolution-independent, important for LOD transitions on planets [5].

Jadkhoury's planet renderer [5] computes normals procedurally in the fragment shader for terrain that is generated on the fly, avoiding texture storage for normal maps.

### 4.3 Practical Considerations

- **Tangent-space vs. object-space**: Tangent-space normals are standard for deformable geometry; object-space is simpler for static spherical planets.
- **Gaussian pre-blur**: Applying a Gaussian filter to the height map before Sobel differentiation reduces high-frequency noise artifacts [14].
- **Strength parameter**: Multiplying the X/Y gradient components by a strength factor (typically 1.0-10.0) controls the perceived bump intensity.

---

## 5. Cloud Maps

### 5.1 Procedural Cloud Layers

Planetary cloud maps are typically generated using a combination of Perlin and Worley noise projected onto a sphere [15][16].

**Worley (cellular) noise** generates blob-like patterns based on distance to nearest feature points. Inverting Worley noise creates rounded cloud-like shapes [16].

**Perlin-Worley hybrid**: Combining Perlin noise (smooth gradients) with Worley noise (cellular structure) produces convincing cloud density fields. Multiple octaves add detail at different scales [16]:
- Octave 1: large weather systems (thousands of km).
- Octave 2: cloud banks (hundreds of km).
- Octave 3-4: individual cloud features.

### 5.2 Weather Pattern Simulation

Wedekind's procedural global cloud cover [15] uses curl noise derived from 3D Worley noise to simulate atmospheric circulation:

1. Sample gradients from a 3D Worley noise potential field.
2. Project gradients onto the sphere surface (remove radial component).
3. Rotate the tangential gradient 90 degrees around the surface normal to get curl noise.
4. Apply latitude-dependent blending: `mix_factor = (1 + sin(2.5 * latitude)) / 2` to simulate prevailing wind bands (trade winds, westerlies).
5. Store the resulting warp field in a cubemap.
6. Advect the cloud density texture through the warp field over time.

This produces realistic banded cloud patterns (ITCZ, mid-latitude cyclones) without a full fluid simulation.

### 5.3 Worley Noise for Clouds

Worley noise implementation for clouds [16]:
1. Divide 3D space into a grid of cells.
2. Place one random feature point per cell.
3. For each sample point, find the distance to the N nearest feature points.
4. Cloud density = 1.0 - distance_to_nearest (inverted Worley).
5. Apply fBm layering with 3-4 octaves of Worley noise for multi-scale detail.

Compute shaders (HLSL/GLSL) are preferred for Worley noise generation due to the per-pixel search over neighboring cells [16].

---

## 6. Atmospheric Scattering

### 6.1 Rayleigh Scattering

Rayleigh scattering models small-molecule scattering (air) with strong wavelength dependence (proportional to 1/lambda^4) [17][18][19]:

```
beta_R(h, lambda) = (8 * pi^3 * (n^2 - 1)^2) / (3 * N * lambda^4) * exp(-h / H_R)
```

Where H_R is the Rayleigh scale height (~8 km for Earth). Precomputed sea-level coefficients for Earth [18][19]:
- Red (680nm): 5.8e-6 m^-1
- Green (550nm): 13.5e-6 m^-1
- Blue (440nm): 33.1e-6 m^-1

**Phase function** (symmetric):
```
P_R(mu) = (3 / 16*pi) * (1 + mu^2)
```

### 6.2 Mie Scattering

Mie scattering models aerosol particles with weak wavelength dependence [17][18]:

```
beta_M(h) = beta_M(0) * exp(-h / H_M)
```

Where H_M is the Mie scale height (~1.2 km for Earth). Sea-level coefficient: ~21e-6 m^-1. The extinction coefficient is ~1.1x the scattering coefficient [18].

**Phase function** (forward-scattering, Henyey-Greenstein):
```
P_M(mu) = (3 / 8*pi) * (1 - g^2)(1 + mu^2) / ((2 + g^2)(1 + g^2 - 2*g*mu)^(3/2))
```

Where g (anisotropy parameter) is typically 0.76 for Earth's atmosphere [18].

### 6.3 Single Scattering Model

The in-scattered radiance along a view ray from point P_camera to P_atmosphere [17][18]:

```
L(P_c, P_a) = integral[P_c to P_a] of:
    T(P_c, X) * SunIntensity * P(V,L) * T(X, P_sun) * beta_s(h) ds
```

Where T is the transmittance (optical depth):
```
T(A, B) = exp(- integral[A to B] beta_e(h) ds)
```

### 6.4 Precomputed Lookup Tables

Bruneton's method [19] precomputes scattering into textures parameterized by altitude and view angle:
- **2D transmittance LUT**: altitude vs. viewing angle, ~256x64 texels.
- **3D/4D inscattering LUT**: altitude, view zenith, sun zenith, view-sun azimuth.
- Approximately 50 samples per integral provide good accuracy.
- Supports both single and multiple scattering.
- Runtime cost: a few texture lookups per pixel.

O'Neil's GPU Gems 2 approach [17] avoids LUTs entirely by:
- Modeling optical depth with a fitted exponential function `exp(-4x)`.
- Reducing per-vertex computation to ~60 operations (5 samples x 2 scattering types x 3 channels x 2 ops).
- Applying phase functions per-pixel in the fragment shader.

HDR rendering with exposure correction (`1 - exp(-exposure * color)`) is essential for correct atmospheric appearance [17].

---

## 7. Ocean Maps

### 7.1 Sea Level Masking

The simplest ocean representation: threshold the height map at sea level. Any texel below the threshold is classified as ocean [5][13]:

```
is_ocean = (height < sea_level) ? 1.0 : 0.0
```

The ocean mask feeds into:
- Albedo: deep ocean blue (~0.02-0.06 reflectance) vs. shallow coastal turquoise.
- Roughness: near-zero roughness for calm water, higher for wind-driven waves.
- Specular: water has a fixed IOR of ~1.33 (F0 = 0.02 in PBR).

### 7.2 FFT Wave Simulation

Based on Tessendorf's foundational paper [20], FFT ocean simulation generates a wave height field from a statistical wave spectrum:

**Phillips spectrum**:
```
P(k) = A * exp(-1 / (k*L)^2) / k^4 * |k_hat . w_hat|^2
```
Where L = V^2/g (largest possible wave for wind speed V), and w_hat is wind direction.

**JONSWAP spectrum** extends Phillips with a peak enhancement factor, producing more realistic energy distributions for developing seas [20].

**Runtime pipeline** (per frame):
1. Multiply spectrum by time-dependent complex exponentials: `h(k,t) = h0(k)*exp(i*w*t) + conj(h0(-k))*exp(-i*w*t)`.
2. Apply 2D Inverse FFT to get height field (and optionally horizontal displacement for choppy waves).
3. Compute normal field via IFFT of `i*k*h(k,t)`.

The 2D IFFT is the main bottleneck. For a 512x512 grid, this requires 262,144 complex multiplications per frame [20]. The Stockham FFT formulation avoids the expensive bit-reversal step of Cooley-Tukey, mapping better to GPU compute shaders [20].

| Grid Size | FFT Cost | Typical FPS | Notes |
|-----------|----------|-------------|-------|
| 256x256 | ~65k complex mults | 60+ fps | Suitable for distant ocean |
| 512x512 | ~262k complex mults | 30-60 fps | Good balance of detail/perf |
| 1024x1024 | ~1M complex mults | GPU compute required | Film quality |

### 7.3 Subsurface Scattering for Water

Water is a participating medium. Light penetrating the surface scatters within the volume and exits at nearby points, producing the characteristic translucent appearance of wave crests [21][22]:

**Approximation techniques**:
1. **Depth-based color blending**: `water_color = mix(shallow_color, deep_color, depth_factor)` where depth_factor = `1 - exp(-absorption * depth)`.
2. **Fresnel reflectance**: `F = F0 + (1 - F0) * (1 - dot(N, V))^5` determines the reflection/refraction balance. At grazing angles, water reflects nearly 100%; at normal incidence, it transmits most light.
3. **Wave-crest SSS**: When the sun is behind a wave (backlit), light travels a short path through the thin water, emerging with a green/turquoise tint. This is approximated by: `sss = max(0, dot(L, -V)) * wave_thickness_factor * sss_color`.
4. **Beer's Law absorption**: `transmittance = exp(-absorption_coeff * path_length)` with wavelength-dependent absorption (red absorbed first, blue/green transmitted furthest).

---

## 8. Shader Pseudocode

### 8.1 fBm Noise with Domain Warping

```glsl
// Basic fBm
float fbm(vec2 p) {
    float value = 0.0;
    float amplitude = 0.5;
    float frequency = 1.0;
    for (int i = 0; i < NUM_OCTAVES; i++) {
        value += amplitude * noise(p * frequency);
        amplitude *= 0.5;    // persistence
        frequency *= 2.0;    // lacunarity
    }
    return value;
}

// Domain-warped fBm (two-level, after Quilez)
float warpedFbm(vec2 p) {
    // First warp layer
    vec2 q = vec2(
        fbm(p + vec2(0.0, 0.0)),
        fbm(p + vec2(5.2, 1.3))
    );
    // Second warp layer
    vec2 r = vec2(
        fbm(p + 4.0 * q + vec2(1.7, 9.2)),
        fbm(p + 4.0 * q + vec2(8.3, 2.8))
    );
    return fbm(p + 4.0 * r);
}

// Ridged multifractal variant
float ridgedFbm(vec2 p) {
    float value = 0.0;
    float amplitude = 0.5;
    float frequency = 1.0;
    float weight = 1.0;
    float offset = 1.0;
    for (int i = 0; i < NUM_OCTAVES; i++) {
        float signal = offset - abs(noise(p * frequency));
        signal *= signal;      // sharpen ridges
        signal *= weight;
        weight = clamp(signal * 2.0, 0.0, 1.0);  // feedback
        value += signal * amplitude;
        amplitude *= 0.5;
        frequency *= 2.0;
    }
    return value;
}
```

### 8.2 Normal Map Generation from Height Map

```glsl
// Method 1: Central differences (fast)
vec3 normalFromHeight_CD(sampler2D heightMap, vec2 uv, float texelSize, float strength) {
    float hL = texture(heightMap, uv - vec2(texelSize, 0.0)).r;
    float hR = texture(heightMap, uv + vec2(texelSize, 0.0)).r;
    float hD = texture(heightMap, uv - vec2(0.0, texelSize)).r;
    float hU = texture(heightMap, uv + vec2(0.0, texelSize)).r;

    vec3 normal;
    normal.x = (hL - hR) * strength;
    normal.y = (hD - hU) * strength;
    normal.z = 1.0;
    return normalize(normal);
}

// Method 2: Sobel filter (3x3, better quality)
vec3 normalFromHeight_Sobel(sampler2D heightMap, vec2 uv, float texelSize, float strength) {
    // Sample 3x3 neighborhood
    float h00 = texture(heightMap, uv + vec2(-1, -1) * texelSize).r;
    float h10 = texture(heightMap, uv + vec2( 0, -1) * texelSize).r;
    float h20 = texture(heightMap, uv + vec2( 1, -1) * texelSize).r;
    float h01 = texture(heightMap, uv + vec2(-1,  0) * texelSize).r;
    // h11 = center, not needed for Sobel
    float h21 = texture(heightMap, uv + vec2( 1,  0) * texelSize).r;
    float h02 = texture(heightMap, uv + vec2(-1,  1) * texelSize).r;
    float h12 = texture(heightMap, uv + vec2( 0,  1) * texelSize).r;
    float h22 = texture(heightMap, uv + vec2( 1,  1) * texelSize).r;

    // Sobel kernels
    float Gx = -h00 - 2.0*h01 - h02 + h20 + 2.0*h21 + h22;
    float Gy = -h00 - 2.0*h10 - h20 + h02 + 2.0*h12 + h22;

    vec3 normal;
    normal.x = -Gx * strength;
    normal.y = -Gy * strength;
    normal.z = 1.0;
    return normalize(normal);
}
```

### 8.3 Atmospheric Scattering (Single Scattering Model)

```glsl
// Based on Scratchapixel [18] and O'Neil [17]
struct AtmosphereParams {
    float planetRadius;     // 6371e3 m (Earth)
    float atmosphereRadius; // 6471e3 m (Earth, +100km)
    vec3  betaR;            // Rayleigh coefficients: vec3(5.8e-6, 13.5e-6, 33.1e-6)
    float betaM;            // Mie coefficient: 21e-6
    float HR;               // Rayleigh scale height: 8000.0 m
    float HM;               // Mie scale height: 1200.0 m
    float g;                // Mie anisotropy: 0.76
    float sunIntensity;     // 22.0
};

// Phase functions
float rayleighPhase(float cosTheta) {
    return 3.0 / (16.0 * PI) * (1.0 + cosTheta * cosTheta);
}

float miePhase(float cosTheta, float g) {
    float g2 = g * g;
    float num = 3.0 * (1.0 - g2) * (1.0 + cosTheta * cosTheta);
    float den = 8.0 * PI * (2.0 + g2) * pow(1.0 + g2 - 2.0*g*cosTheta, 1.5);
    return num / den;
}

// Main scattering computation
vec3 atmosphere(vec3 rayDir, vec3 rayOrigin, vec3 sunDir, AtmosphereParams atm) {
    // 1. Intersect ray with atmosphere sphere
    vec2 tAtm = raySphereIntersect(rayOrigin, rayDir, atm.atmosphereRadius);
    if (tAtm.x > tAtm.y) return vec3(0.0);  // no intersection

    float tMin = max(tAtm.x, 0.0);
    float tMax = tAtm.y;

    // 2. March along view ray
    int NUM_SAMPLES = 16;
    int NUM_LIGHT_SAMPLES = 8;
    float ds = (tMax - tMin) / float(NUM_SAMPLES);

    vec3 totalR = vec3(0.0);  // Rayleigh accumulator
    vec3 totalM = vec3(0.0);  // Mie accumulator
    float optDepthR = 0.0;
    float optDepthM = 0.0;

    for (int i = 0; i < NUM_SAMPLES; i++) {
        vec3 samplePos = rayOrigin + rayDir * (tMin + (float(i) + 0.5) * ds);
        float h = length(samplePos) - atm.planetRadius;

        // Local density
        float densityR = exp(-h / atm.HR) * ds;
        float densityM = exp(-h / atm.HM) * ds;
        optDepthR += densityR;
        optDepthM += densityM;

        // 3. March toward sun from sample point
        vec2 tSun = raySphereIntersect(samplePos, sunDir, atm.atmosphereRadius);
        float dsSun = tSun.y / float(NUM_LIGHT_SAMPLES);
        float optDepthLightR = 0.0;
        float optDepthLightM = 0.0;

        for (int j = 0; j < NUM_LIGHT_SAMPLES; j++) {
            vec3 lightSample = samplePos + sunDir * ((float(j) + 0.5) * dsSun);
            float hLight = length(lightSample) - atm.planetRadius;
            optDepthLightR += exp(-hLight / atm.HR) * dsSun;
            optDepthLightM += exp(-hLight / atm.HM) * dsSun;
        }

        // 4. Compute transmittance: camera->sample and sample->sun
        vec3 tau = atm.betaR * (optDepthR + optDepthLightR)
                 + vec3(atm.betaM * 1.1) * (optDepthM + optDepthLightM);
        vec3 attenuation = exp(-tau);

        totalR += densityR * attenuation;
        totalM += densityM * attenuation;
    }

    // 5. Apply phase functions and sun intensity
    float cosTheta = dot(rayDir, sunDir);
    vec3 color = atm.sunIntensity *
                 (rayleighPhase(cosTheta) * atm.betaR * totalR
                + miePhase(cosTheta, atm.g) * atm.betaM * totalM);

    // 6. HDR tone mapping
    color = 1.0 - exp(-1.0 * color);
    return color;
}
```

### 8.4 Simple Hydraulic Erosion

```glsl
// Droplet-based erosion (CPU pseudocode, adapted from Lague [7] / Beyer)
struct Droplet {
    vec2  pos;
    vec2  dir;
    float speed;
    float water;
    float sediment;
};

void erode(float[] heightMap, int mapSize) {
    const float INERTIA = 0.05;
    const float CAPACITY_FACTOR = 4.0;
    const float DEPOSIT_SPEED = 0.3;
    const float ERODE_SPEED = 0.3;
    const float EVAPORATE_RATE = 0.01;
    const float GRAVITY = 4.0;
    const float MIN_SLOPE = 0.01;
    const int   MAX_LIFETIME = 60;

    for (int drop = 0; drop < NUM_DROPS; drop++) {
        Droplet d;
        d.pos = randomPosition(mapSize);
        d.dir = vec2(0.0);
        d.speed = 1.0;
        d.water = 1.0;
        d.sediment = 0.0;

        for (int step = 0; step < MAX_LIFETIME; step++) {
            int nodeX = int(d.pos.x);
            int nodeY = int(d.pos.y);

            // 1. Compute gradient via bilinear interpolation
            vec2 gradient = calcGradient(heightMap, d.pos);
            float oldHeight = sampleHeight(heightMap, d.pos);

            // 2. Update direction (blend old direction with gradient)
            d.dir = d.dir * INERTIA - gradient * (1.0 - INERTIA);
            if (length(d.dir) < 0.0001) {
                // Random direction if on flat terrain
                d.dir = randomUnitVec2();
            }
            d.dir = normalize(d.dir);

            // 3. Move droplet
            d.pos += d.dir;

            // Check bounds
            if (outOfBounds(d.pos, mapSize)) break;

            float newHeight = sampleHeight(heightMap, d.pos);
            float heightDiff = newHeight - oldHeight;

            // 4. Compute sediment capacity
            float capacity = max(-heightDiff, MIN_SLOPE)
                           * d.speed * d.water * CAPACITY_FACTOR;

            // 5. Erode or deposit
            if (d.sediment > capacity || heightDiff > 0.0) {
                // Deposit sediment
                float depositAmt = (heightDiff > 0.0)
                    ? min(d.sediment, heightDiff)
                    : (d.sediment - capacity) * DEPOSIT_SPEED;
                d.sediment -= depositAmt;
                depositToMap(heightMap, d.pos, depositAmt);
            } else {
                // Erode terrain
                float erodeAmt = min((capacity - d.sediment) * ERODE_SPEED,
                                     -heightDiff);
                d.sediment += erodeAmt;
                erodeFromMap(heightMap, d.pos, erodeAmt);
            }

            // 6. Update speed and evaporate water
            d.speed = sqrt(max(d.speed * d.speed + heightDiff * GRAVITY, 0.0));
            d.water *= (1.0 - EVAPORATE_RATE);
        }
    }
}
```

---

## 9. Performance Summary

| Technique | Resolution/Scale | Cost per Frame | Hardware | Source |
|-----------|-----------------|----------------|----------|--------|
| fBm (8 octaves) | per vertex/pixel | ~8 noise evals | Any GPU | [1][6] |
| Domain warping (2-level) | per vertex/pixel | ~21 noise evals (7 fBm x 3 octaves) | Any GPU | [3] |
| Hydraulic erosion (GPU grid) | 1024x1024 | 2 ms/cycle | GTX 1050 | [9] |
| Hydraulic erosion (droplet, CPU) | 512x512, 200k drops | several seconds total | Modern CPU | [7] |
| Normal map (Sobel) | per texel | 8 texture fetches | Any GPU | [14] |
| Normal map (central diff) | per texel | 4 texture fetches | Any GPU | [14] |
| Atmospheric scattering (raymarched) | per pixel | 16x8 = 128 exp() calls | Mid-range GPU | [17][18] |
| Atmospheric scattering (LUT) | per pixel | 2-4 texture lookups | Any GPU | [19] |
| FFT ocean (512x512) | per frame | 262k complex mults | Compute shader | [20] |
| Cloud Worley noise (3 octaves) | per texel | ~27 cell searches (3^3 x 3 octaves) | Compute shader | [15][16] |
| Full planet sim (4.5 Gyr) | 1024x1024 | 60 fps (real-time) | Modern GPU | [13] |

---

## References

1. [Red Blob Games: Making maps with noise](https://www.redblobgames.com/maps/terrain-from-noise/) -- Amit Patel's comprehensive guide to terrain-from-noise, covering fBm, octaves, and redistribution.

2. [Musgrave: Procedural Fractal Terrains (UChicago mirror)](https://www.classes.cs.uchicago.edu/archive/2015/fall/23700-1/final-project/MusgraveTerrain00.pdf) -- F. Kenton Musgrave's foundational paper on multifractal terrain, ridged noise, and heterogeneous terrains.

3. [Inigo Quilez: Domain Warping](https://iquilezles.org/articles/warp/) -- Quilez's article and GLSL code for multi-level domain warping with fBm.

4. [3DWorldGen: Domain Warping Noise](http://3dworldgen.blogspot.com/2017/05/domain-warping-noise.html) -- Practical analysis of domain warping for 3D terrain generation.

5. [Jadkhoury: Procedural Planet Rendering](https://jadkhoury.github.io/terrain_blog.html) -- GPU-based planet terrain with hybrid multifractal, double-buffered height maps, and procedural texturing.

6. [Learn Procedural Generation: Noise for Terrains](https://aparis69.github.io/LearnProceduralGeneration/terrain/procedural/noise_for_terrains/) -- Comprehensive overview of fBm, multifractal, and ridge noise with formulas and code.

7. [Sebastian Lague: Hydraulic Erosion (GitHub)](https://github.com/SebLague/Hydraulic-Erosion) -- Open-source Unity implementation of droplet-based hydraulic erosion.

8. [Hydraulic Erosion -- Procedural Terrain Generation blog](https://filipalexjoel.wordpress.com/2019/06/21/hydraulic-erosion/) -- Detailed walkthrough of droplet erosion algorithm with parameter analysis.

9. [Fast Hydraulic Erosion on GPU (15-618 Final Project)](https://patiltanma.github.io/15618-FinalProject/) -- GPU erosion with struct-of-arrays optimization achieving 5-10x speedup; 2 ms/cycle at 1024x1024.

10. [Tim Coster: Unity Shader Graph Procedural Planet Tutorial](https://timcoster.com/2020/09/03/unity-shader-graph-procedural-planet-tutorial/) -- Biome-based RGB gradient coloring, noise layering, and texture blending for procedural planets.

11. [PBR Color Space Conversion and Albedo Chart (ArtStation)](https://www.artstation.com/blogs/shinsoj/Q9j6/pbr-color-space-conversion-and-albedo-chart) -- sRGB-to-linear conversion formulas and reference albedo values for PBR materials.

12. [Roughness maps and PBR fundamentals (danthree.studio)](https://www.danthree.studio/en/glossary/roughness-map) -- Definition and practical guidance for roughness in PBR pipelines.

13. [Davidar: Simulating Worlds on the GPU](https://davidar.io/post/sim-glsl) -- Full planet simulation in GLSL: tectonic plates, hydraulic/thermal erosion, roughness from sediment, 60 fps at 1024x1024.

14. [Normal Map with Sobel Sampling (ArtStation)](https://theobaudoin.artstation.com/blog/zXDqm/normal-map-with-sobel-sampling-in-ue5) -- Sobel-based normal map generation in UE5 with comparison to standard sampling.

15. [Wedekind: Procedural Generation of Global Cloud Cover](https://www.wedesoft.de/software/2023/03/20/procedural-global-cloud-cover/) -- Curl noise from Worley noise for global cloud patterns with cubemap storage and GLSL implementation.

16. [CloudNoiseGen (GitHub)](https://github.com/Fewes/CloudNoiseGen) -- GPU compute shader for generating periodic Perlin-Worley 3D noise textures for volumetric clouds.

17. [O'Neil: Accurate Atmospheric Scattering (GPU Gems 2, Ch. 16)](https://developer.nvidia.com/gpugems/gpugems2/part-ii-shading-lighting-and-shadows/chapter-16-accurate-atmospheric-scattering) -- Classic reference for real-time atmospheric scattering without lookup tables; ~60 ops per vertex.

18. [Scratchapixel: Simulating the Colors of the Sky](https://www.scratchapixel.com/lessons/procedural-generation-virtual-worlds/simulating-sky/simulating-colors-of-the-sky.html) -- Rayleigh/Mie equations, phase functions, single scattering integral, and C++ implementation.

19. [Bruneton: Precomputed Atmospheric Scattering](https://ebruneton.github.io/precomputed_atmospheric_scattering/) -- Modern precomputed LUT approach with GLSL shaders, CIE spectral conversion, and multiple scattering support.

20. [Barth Paleologue: Ocean Simulation with FFT and WebGPU](https://barthpaleologue.github.io/Blog/posts/ocean-simulation-webgpu/) -- Phillips/JONSWAP spectra, Stockham FFT on GPU, 512x512 real-time ocean with choppy waves.

21. [Tessendorf: Simulating Ocean Water (Clemson)](https://people.computing.clemson.edu/~jtessen/reports/papers_files/coursenotes2004.pdf) -- Foundational paper on FFT-based ocean wave simulation.

22. [Optically Realistic Water (GitHub)](https://github.com/muckSponge/Optically-Realistic-Water/blob/master/README.md) -- Fresnel reflectance, volumetric light scattering, and chromatic aberration for water rendering.
