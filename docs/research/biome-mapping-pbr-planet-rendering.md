# Biome Mapping, Map Generation Pipeline & PBR Planet Rendering

*Research date: 2026-03-28 (updated with web research, second pass)*

---

## Table of Contents

1. [Biome Mapping Algorithms](#1-biome-mapping-algorithms)
2. [Map Generation Pipeline](#2-map-generation-pipeline)
3. [PBR Planet Rendering](#3-pbr-planet-rendering)
4. [Memory Budgets & Performance Summary](#4-memory-budgets--performance-summary)
5. [Sources](#5-sources)

---

## 1. Biome Mapping Algorithms

### 1.1 Whittaker Diagram Implementation

The Whittaker diagram maps (temperature, precipitation) pairs to biome types. The standard implementation discretizes the original triangular diagram into a rectangular lookup table with ~20 biome types.

**Core lookup (pseudocode):**
```glsl
// GPU-friendly Whittaker lookup via texture
// Encode biome IDs into a 2D texture where:
//   U = normalize(temperature, -30, 40)   // 0..1
//   V = normalize(precipitation, 0, 4000) // 0..1
uniform sampler2D u_biome_lut;  // e.g. 64x64, R8 format

int getBiome(float temperature, float precipitation) {
    vec2 uv = vec2(
        (temperature + 30.0) / 70.0,
        precipitation / 4000.0
    );
    return int(texture(u_biome_lut, uv).r * 255.0);
}
```

**Biome table (simplified Whittaker, 9 primary zones):**

| Temp \ Precip | Very Dry (<250mm) | Dry (250-750) | Moderate (750-1500) | Wet (1500-3000) | Very Wet (>3000) |
|---|---|---|---|---|---|
| Hot (>24C) | Subtropical desert | Savanna | Tropical seasonal | Tropical rainforest | Tropical rainforest |
| Warm (12-24C) | Temperate desert | Grassland | Temperate deciduous | Temperate rainforest | Temperate rainforest |
| Cool (0-12C) | Cold desert | Steppe | Boreal forest | Boreal forest | Boreal forest |
| Cold (<0C) | Tundra | Tundra | Tundra | Tundra | Ice sheet |

**Temperature from latitude and elevation:**
```
T(lat, elev) = T_equator - |lat|/90 * T_range - max(0, elev) * lapse_rate + noise
```
Where: T_equator ~ 30C, T_range ~ 60C, lapse_rate ~ 6.5C/km.

This is the method used by Azgaar's Fantasy Map Generator [3], AutoBiomes [1], and countless game implementations including Dwarf Fortress and Minecraft.

### 1.2 Koppen Climate Classification

Koppen-Geiger classifies into 5 main classes (A-E) with 30 sub-types using monthly temperature and precipitation thresholds. Algorithmic implementation:

```python
def koppen_classify(t_monthly, p_monthly):
    """t_monthly: 12 monthly temps (C), p_monthly: 12 monthly precip (mm)"""
    t_ann = mean(t_monthly)
    p_ann = sum(p_monthly)
    t_hot = max(t_monthly)
    t_cold = min(t_monthly)
    p_dry = min(p_monthly)

    # Tropical (A): coldest month >= 18C
    if t_cold >= 18:
        if p_dry >= 60:           return 'Af'   # Tropical rainforest
        if p_dry >= 100 - p_ann/25: return 'Am' # Tropical monsoon
        return 'Aw'                              # Tropical savanna

    # Arid (B): P_ann < 10 * P_threshold
    threshold = 20 * t_ann + 280  # simplified
    if p_ann < threshold:
        if p_ann < threshold / 2: return 'BWh' if t_ann >= 18 else 'BWk'
        return 'BSh' if t_ann >= 18 else 'BSk'

    # Temperate (C): -3 < t_cold < 18
    if -3 < t_cold < 18:
        if p_dry < 40:  return 'Cfa'  # simplified
        return 'Cfb'

    # Continental (D): t_cold <= -3, t_hot > 10
    if t_cold <= -3 and t_hot > 10:
        return 'Dfa' if t_hot >= 22 else 'Dfb'

    # Polar (E): t_hot < 10
    if t_hot >= 0: return 'ET'  # Tundra
    return 'EF'                  # Ice cap
```

**Koppen vs Whittaker for procedural generation:** Koppen requires monthly data (12 temperature + 12 precipitation values per cell), making it significantly more expensive than Whittaker's single-pair lookup. Koppen is better suited for offline world-building tools; Whittaker is preferred for real-time GPU evaluation.

**Koppen shortcut for procedural generation (Frozen Fractal [B-NEW1]):** Approximate year-round weather from January and July values, assuming intermediate months follow a sine curve. This gives enough data to evaluate even complex Koppen rules like "at least four months averaging above 10 C" without storing 12 monthly values per cell.

### 1.3 SpaceEngine Climate Model Formulas [B-NEW2]

SpaceEngine implements a physics-based temperature model:

**Planetary equilibrium temperature:**
```
T_eq = ( L_star / (4*pi*d^2) * (1-A) / (sigma_SB * f) )^(1/4)
```
Where: L_star = stellar luminosity, d = distance, A = bond albedo, sigma_SB = Stefan-Boltzmann, f = 4 (uniform) or 2 (tidally locked).

**Latitudinal temperature:** Modified cosine law relative to subsolar point, with T_pole minimum preventing zero at poles.

**Daylight fraction:**
```
sunlit_frac = arccos(-tan(lat) * tan(lat_subsolar)) / pi
```

**Multi-star system:** T_final = (T1^4 + T2^4 + T3^4 + ...)^(1/4)

**Altitude:** Pressure decreases exponentially with scale height; temperature profiles derived from pressure-temperature lookup.

**Standard lapse rate (Earth reference, NASA [B-NEW3]):**
```
T(h) = T_surface + Gamma * h
```
Where Gamma = -6.5 C/km in troposphere (up to ~11km). At 2km: T = 15 - 6.5*2 = 2.0 C.

### 1.4 Moisture Transport and Rain Shadow [B-NEW4][B-NEW5]

Nick McDonald's procedural weather system uses an ODE-coupled grid:

**Wind model:** Random time-dependent global wind vector: `(wx, wy) = Perlin(t, t)`, modified by terrain slope (faster uphill, slower downhill).

**Moisture transport:**
1. Water bodies evaporate moisture proportional to temperature
2. Wind convects moisture to downwind cells using semi-Lagrangian advection
3. Cells diffuse temperature and humidity with neighbors (averaging pass)
4. Precipitation occurs when humidity exceeds threshold, modulated by temperature (warm humid air cooling triggers rain)

**Rain shadow effect:** Wind picks up moisture from ocean, deposits on windward mountain slopes, leeward side receives little precipitation.

**WorldEngine approach [B-NEW6]:** Uses plate tectonics + rain shadow simulation. Moisture is tracked as a scalar field advected by wind, with precipitation = f(local moisture, temperature). Produces realistic desert placement leeward of mountain ranges.

### 1.5 Climate Simulation Pipeline (Latitude/Altitude/Moisture)

The AutoBiomes system [1] implements a sequential pipeline:

1. **Temperature**: `T = T_base(latitude) - lapse_rate * elevation + noise`
2. **Wind**: Iterative relaxation rather than full fluid dynamics. Trade winds at tropics, westerlies at mid-latitudes, polar easterlies
3. **Moisture transport**: Most moisture transferred to the downwind cell; partial shares to adjacent cells. Orographic lift causes precipitation on windward slopes
4. **Precipitation**: Accumulated from moisture dump events. Rain shadow on leeward side
5. **Biome classification**: Whittaker lookup from computed T and P

Joe Duffy's climate simulation [15] uses a similar approach with Perlin-noise-based temperature/precipitation maps adjusted by latitude curves and altitude falloff.

### 1.4 Biome Transition Zones: Blending Algorithms

#### Scattered Biome Blending (KdotJPG) [4]

The state-of-the-art approach uses **jittered triangular grid + normalized sparse convolution**:

**Weight contribution per point:**
```glsl
float weight = max(0.0, radius2 - dx*dx - dy*dy);
weight = weight * weight;  // squared polynomial falloff
```

This polynomial falloff goes to zero smoothly at a finite radius (unlike Gaussian which never reaches zero).

**Normalization:**
```glsl
// For each coordinate, sum all biome weights then normalize
float total = 0.0;
for (int i = 0; i < num_points; i++) total += weights[i];
float inv_total = 1.0 / total;
for (int i = 0; i < num_points; i++) weights[i] *= inv_total;
// Result: weights always sum to 1.0
```

**Performance (nanoseconds per coordinate, from [4]):**

| Method | radius=24 | radius=48 |
|---|---|---|
| Full-resolution convolution | 4851 ns | 23136 ns |
| Scattered blending (recommended) | 196 ns | 665 ns |
| Convoluted grid | 98 ns | 397 ns |
| Lerped grid (has artifacts) | 22 ns | 40 ns |

Scattered blending is **25x faster** than full-resolution while avoiding grid artifacts. Single-biome chunk detection yields an additional 36% improvement.

#### Noise-Based Boundary Perturbation

```glsl
// Perturb biome boundary with domain-warped noise
float boundary_noise = fbm(pos * 0.01, 4); // 4 octaves
float blend_factor = smoothstep(-0.1, 0.1, biome_distance + boundary_noise * 0.05);
vec3 color = mix(biome_a_color, biome_b_color, blend_factor);
```

#### Voronoi-Based Biome Distribution

Voronoi cells with jittered centers provide organic biome shapes. Each cell center carries a biome ID; blending occurs in the transition zone between cells using distance-based weights.

---

## 2. Map Generation Pipeline

### 2.1 Height Maps: Multi-Octave Noise + Erosion

**GPU pipeline (from NVIDIA GPU Gems 3, Chapter 1 [6]):**

The pipeline uses DirectX geometry shaders with stream output. A pixel shader evaluates a density function at all corners of a 33x33x33 volume:

```glsl
// Multi-octave noise combination (from GPU Gems 3)
density += noiseVol1.Sample(sampler, ws * 4.03) * 0.25;
density += noiseVol2.Sample(sampler, ws * 1.96) * 0.50;
density += noiseVol3.Sample(sampler, ws * 1.01) * 1.00;
// 9 octaves total for rich detail
```

**Performance (GeForce 8800 era, from [6]):**

| Method | Blocks/Second |
|---|---|
| Single-pass geometry shader | 6.6 |
| Two-pass (marker + expand) | 144 |
| Five-pass (shared vertices) | 260 |

Modern GPU compute approach (2024+):
- Compute shader generates heightmap via ridged multi-fractal simplex noise
- Second pass: Sobel filter for normal map
- Third pass: Diffuse/specular texture from height + normals
- Tessellation shaders for dynamic LOD based on camera distance

**Fractal Brownian Motion (fBm) heightmap:**
```glsl
float fbm(vec3 p, int octaves) {
    float value = 0.0, amplitude = 1.0, frequency = 1.0;
    float total_amplitude = 0.0;
    for (int i = 0; i < octaves; i++) {
        value += amplitude * snoise(p * frequency);
        total_amplitude += amplitude;
        amplitude *= 0.5;    // persistence
        frequency *= 2.0;    // lacunarity
    }
    return value / total_amplitude;
}

// Ridged multifractal variant (sharper mountain ridges)
float ridged_mf(vec3 p, int octaves) {
    float value = 0.0, amplitude = 1.0, frequency = 1.0;
    float weight = 1.0;
    for (int i = 0; i < octaves; i++) {
        float signal = 1.0 - abs(snoise(p * frequency));
        signal = signal * signal * weight;
        weight = clamp(signal, 0.0, 1.0);
        value += signal * amplitude;
        amplitude *= 0.5;
        frequency *= 2.0;
    }
    return value;
}
```

### 2.2 Albedo/Color Maps: Biome-Based Coloring

**Slope-dependent texturing with triplanar mapping [10][11]:**

The texturing pipeline from Sapra Projects [10] achieves **36 texture lookups** per fragment (down from 168) by:
1. Packing 6 biome masks into 2 RGBA textures
2. Sorting by weight, sampling only the 4 most influential biomes
3. Eliminating pre-blend height lookups

Result: **17.9x faster** than naive approach, scales independently of biome count.

```glsl
// Altitude + slope biome coloring
vec3 biome_color(float height, float slope, float moisture, float temp) {
    int biome_id = getBiome(temp, moisture);  // Whittaker lookup

    // Each biome has flat + slope texture variants
    vec3 flat_color = sample_biome_texture(biome_id, FLAT, uv);
    vec3 slope_color = sample_biome_texture(biome_id, SLOPE, uv);

    // Blend based on terrain steepness
    float slope_factor = smoothstep(0.3, 0.7, slope);
    return mix(flat_color, slope_color, slope_factor);
}
```

**Stochastic sampling** removes tiling artifacts via random offset texture sampling using triangular grid distribution [10].

### 2.3 Roughness Maps: Derived from Terrain Features

```glsl
float compute_roughness(int biome_id, float slope, float height, float moisture) {
    // Base roughness per biome
    float roughness = biome_base_roughness[biome_id];
    // Water: very smooth (0.05-0.1)
    // Sand/desert: moderate (0.4-0.6)
    // Rock/mountain: rough (0.7-0.9)
    // Forest canopy: moderate-rough (0.5-0.7)
    // Snow: moderate (0.3-0.5)

    // Modify by slope (steeper = rougher due to exposed rock)
    roughness = mix(roughness, 0.85, smoothstep(0.4, 0.8, slope));

    // Wet surfaces are smoother
    roughness *= mix(1.0, 0.7, moisture);

    return clamp(roughness, 0.05, 1.0);
}
```

**ORM packing** for GPU efficiency (Red=AO, Green=Roughness, Blue=Metallic) reduces texture samples [9].

### 2.4 Normal Maps: Heightmap Derivatives on Sphere

**Sobel-filter normal from heightmap:**
```glsl
// Compute normal from heightmap using Sobel operator
vec3 compute_normal(sampler2D heightmap, vec2 uv, float texel_size, float strength) {
    float h_l = texture(heightmap, uv + vec2(-texel_size, 0)).r;
    float h_r = texture(heightmap, uv + vec2( texel_size, 0)).r;
    float h_d = texture(heightmap, uv + vec2(0, -texel_size)).r;
    float h_u = texture(heightmap, uv + vec2(0,  texel_size)).r;

    vec3 normal = normalize(vec3(
        (h_l - h_r) * strength,
        (h_d - h_u) * strength,
        1.0
    ));
    return normal * 0.5 + 0.5;  // pack to [0,1] for storage
}
```

**From GPU Gems 3 [6], gradient-based normal on density volume:**
```glsl
grad.x = density_vol.Sample(sampler, uvw + float3(d, 0, 0)) -
         density_vol.Sample(sampler, uvw + float3(-d, 0, 0));
grad.y = density_vol.Sample(sampler, uvw + float3(0, d, 0)) -
         density_vol.Sample(sampler, uvw + float3(0, -d, 0));
grad.z = density_vol.Sample(sampler, uvw + float3(0, 0, d)) -
         density_vol.Sample(sampler, uvw + float3(0, 0, -d));
output.wsNormal = -normalize(grad);
```

**Triplanar normal mapping on sphere (from Ben Golus [11]):**

Three methods for converting tangent-space normals to world-space on triplanar projections:

1. **UDN Blend** (cheapest, slight flattening past 45 degrees):
```glsl
tnormalX = half3(tnormalX.xy + worldNormal.zy, worldNormal.x);
tnormalY = half3(tnormalY.xy + worldNormal.xz, worldNormal.y);
tnormalZ = half3(tnormalZ.xy + worldNormal.xy, worldNormal.z);
```

2. **Whiteout Blend** (better accuracy, nearly same cost):
```glsl
tnormalX = half3(tnormalX.xy + worldNormal.zy,
                  abs(tnormalX.z) * worldNormal.x);
tnormalY = half3(tnormalY.xy + worldNormal.xz,
                  abs(tnormalY.z) * worldNormal.y);
tnormalZ = half3(tnormalZ.xy + worldNormal.xy,
                  abs(tnormalZ.z) * worldNormal.z);
```

3. **Reoriented Normal Mapping (RNM)** (closest to ground truth):
```glsl
float3 rnmBlendUnpacked(float3 n1, float3 n2) {
    n1 += float3(0, 0, 1);
    n2 *= float3(-1, -1, 1);
    return n1 * dot(n1, n2) / n1.z - n2;
}
```

UV swizzling for triplanar: X-plane uses `worldPos.zy`, Y-plane uses `worldPos.xz`, Z-plane uses `worldPos.xy`.

---

## 3. PBR Planet Rendering

### 3.1 Atmospheric Scattering: Bruneton's Model

**Reference implementation:** [Bruneton 2017 (improved)](https://ebruneton.github.io/precomputed_atmospheric_scattering/) [7]

#### LUT Texture Dimensions and Memory

| Texture | Dimensions | Format | Memory (FP16) | Memory (FP32) | Parameters |
|---|---|---|---|---|---|
| Transmittance | 256 x 64 | RGBA16F/32F | ~128 KB | ~256 KB | (r, mu) |
| Irradiance | 64 x 16 | RGBA16F/32F | ~8 KB | ~16 KB | (r, mu_s) |
| Scattering (3D) | 256 x 128 x 32 | RGBA16F/32F | ~8 MB | ~16 MB | (r, mu, mu_s, nu) |
| **Total** | | | **~8.1 MB** | **~16.3 MB** | |

The scattering texture is 4D but packed into a 3D texture by encoding `nu` into the width dimension.

#### Key GLSL functions (from [7]):

```glsl
// Transmittance computation
DimensionlessSpectrum ComputeTransmittanceToTopAtmosphereBoundary(
    IN(AtmosphereParameters) atmosphere, Length r, Number mu);

// Single scattering (Rayleigh + Mie)
void ComputeSingleScattering(
    IN(AtmosphereParameters) atmosphere,
    IN(TransmittanceTexture) transmittance_texture,
    Length r, Number mu, Number mu_s, Number nu,
    bool ray_r_mu_intersects_ground,
    OUT(IrradianceSpectrum) rayleigh,
    OUT(IrradianceSpectrum) mie);

// Multiple scattering density
RadianceDensitySpectrum ComputeScatteringDensity(
    IN(AtmosphereParameters) atmosphere, ...);
```

#### Single vs Multi-Scatter

- **Single scatter**: One bounce of light. Fast to compute. Handles sun disk, sunset colors.
- **Multi-scatter** (Bruneton): Iterative precomputation over N orders (typically 4). Adds ~2x precomputation time but captures sky brightening near horizon and blue-shift in shadow areas.
- **Precomputation time**: ~3 seconds on modern GPU for 4 scattering orders [7].
- **Runtime cost**: Constant-time texture lookups regardless of scattering order.

#### Simplified single-scatter (Sean O'Neil / GPU Gems 2 [8])

For lower-fidelity use cases:
```glsl
// Rayleigh phase function
float rayleighPhase(float cosTheta) {
    return 3.0 / (16.0 * PI) * (1.0 + cosTheta * cosTheta);
}

// Mie phase function (Henyey-Greenstein)
float miePhase(float cosTheta, float g) {
    float g2 = g * g;
    return 3.0 / (8.0 * PI) * ((1.0 - g2) * (1.0 + cosTheta * cosTheta))
           / ((2.0 + g2) * pow(1.0 + g2 - 2.0 * g * cosTheta, 1.5));
}
```

### 3.2 Ocean: FFT-Based Wave Simulation

**Based on Tessendorf [12], GPU implementation from Paleologue [13].**

#### Phillips Spectrum

```
P(k) = A * exp(-1/(kL)^2) / k^4 * |k_hat . w_hat|^2
```
Where: A = amplitude factor, L = V^2/g (largest possible wave from wind V), k = wave vector magnitude, w = wind direction.

Typical parameters: Wind speed V = 31 m/s, tile size L = 1000m, grid 256x256 or 512x512.

#### GPU Compute Pipeline

```
Stage 1: Phillips spectrum * Gaussian noise -> h0(k)     [CPU, once]
Stage 2: Time-dependent spectrum h(k,t)                   [Compute shader, per frame]
Stage 3: Inverse FFT -> displacement + gradient maps       [Compute shader, per frame]
Stage 4: Vertex displacement + shading                     [Vertex/Fragment shader]
```

**Per-frame cost at 512x512:** 262,144 complex multiplications for time-dependent spectrum, plus 3 IFFTs (height, dx displacement, dz displacement).

#### Displacement and Normal Maps

```glsl
// Height displacement
h(x, t) = IFFT{ h_tilde(k, t) }

// Horizontal displacement (choppy waves)
D(x, t) = IFFT{ -i * (k/|k|) * h_tilde(k, t) }

// Gradient for normal map
grad(x, t) = IFFT{ i * k * h_tilde(k, t) }
// Normal = normalize(vec3(-grad.x, 1.0, -grad.z))
```

#### Ocean Shading

```glsl
// Fresnel (Schlick approximation)
float fresnel = 0.02 + 0.98 * pow(1.0 - max(dot(N, V), 0.0), 5.0);

// Subsurface scattering approximation
vec3 sss = ocean_color * max(0.0, dot(L, -V)) * thickness;

// Foam from Jacobian (wave folding detection)
float jacobian = dDx.x * dDz.z - dDx.z * dDz.x;
float foam = smoothstep(0.0, -0.3, jacobian); // negative = folding
```

**Sphere mapping** uses tri-planar projection with tangent vectors [13]:
```
t1 = (-sin(phi), 0, cos(phi))
t2 = (cos(theta)*cos(phi), -sin(theta), cos(theta)*sin(phi))
```

#### Memory Budget

| Resource | Size | Format |
|---|---|---|
| Initial spectrum h0(k) | 512x512 | RG32F (complex) = 2 MB |
| Time spectrum h(k,t) | 512x512 | RG32F = 2 MB |
| Height map | 512x512 | R32F = 1 MB |
| Displacement XZ | 512x512 | RG32F = 2 MB |
| Normal/gradient map | 512x512 | RG32F = 2 MB |
| Foam map | 512x512 | R32F = 1 MB |
| **Total** | | **~10 MB** |

At 128x128 resolution (sufficient for real-time): ~0.6 MB total.

### 3.3 Clouds: Noise-Based Volumetric Ray Marching

**Based on Schneider/Guerrilla Games (Horizon: Zero Dawn) [14] and Grenier [16].**

#### Noise Textures

| Texture | Resolution | Contents | Memory |
|---|---|---|---|
| 3D shape noise | 128x128x128 | Perlin-Worley + 3 octaves Worley (RGBA) | 32 MB (FP16) |
| 3D detail noise | 32x32x32 | 3 octaves Worley (RGB) | 96 KB (FP16) |
| 2D weather map | 512x512 | Coverage, cloud type, wetness (RGB) | 1.5 MB |
| 2D curl noise | 128x128 | 2D curl distortion (RG) | 128 KB |
| **Total** | | | **~34 MB** |

#### Density Function

```glsl
float cloudDensity(vec3 pos) {
    // Weather map lookup
    vec2 weather_uv = pos.xz / WEATHER_SCALE;
    vec3 weather = texture(u_weather, weather_uv).rgb;
    float coverage = weather.r;
    float cloud_type = weather.g;

    // Altitude fraction within cloud layer
    float height_frac = (pos.y - CLOUD_MIN) / (CLOUD_MAX - CLOUD_MIN);

    // Altitude-dependent shape (cumulus vs stratus)
    float altitude_mask = smoothstep(0.0, 0.1, height_frac)
                        * smoothstep(1.0, 0.6, height_frac);

    // 3D shape noise (low frequency)
    vec4 shape_noise = texture(u_shape_noise, pos * SHAPE_SCALE);
    float shape_fbm = shape_noise.g * 0.625
                    + shape_noise.b * 0.25
                    + shape_noise.a * 0.125;
    float base_cloud = remap(shape_noise.r, shape_fbm - 1.0, 1.0, 0.0, 1.0);
    base_cloud *= altitude_mask * coverage;

    // 3D detail noise (high frequency, erodes edges)
    vec3 detail = texture(u_detail_noise, pos * DETAIL_SCALE).rgb;
    float detail_fbm = detail.r * 0.625 + detail.g * 0.25 + detail.b * 0.125;
    float density = remap(base_cloud, detail_fbm * 0.35, 1.0, 0.0, 1.0);

    return max(0.0, density);
}
```

#### Lighting: Beer-Powder Approximation

```glsl
// Beer's law (absorption)
float beer(float density) { return exp(-density); }

// Powder effect (bright edges when backlit)
float powder(float density, float cosTheta) {
    return 1.0 - exp(-density * 2.0);
}

// Combined Beer-Powder term
float light_energy = beer(optical_depth) * mix(1.0, powder(optical_depth, cos_theta), 0.5);
```

#### Temporal Reprojection

The key optimization from Horizon: Zero Dawn [14]:
- Ray march only **1/16th of pixels** per frame using 4x4 Bayer matrix pattern
- Remaining 15/16 pixels filled via reprojection from previous frames
- Motion vectors generated from weighted absorption position tracking
- 8-value 1D Halton sequence for animated sub-pixel offset
- Full frame reconstructed over 16 frames
- **Speedup: ~6x** (from [16])
- Ghosting handled by neighborhood clamping

**Performance budget (Horizon: Zero Dawn):** ~2ms GPU time target [14].

#### Ray Marching Structure

```glsl
vec4 raymarchClouds(vec3 rayOrigin, vec3 rayDir) {
    // Intersect cloud layer shell
    vec2 t = intersectSphere(rayOrigin, rayDir, CLOUD_MIN, CLOUD_MAX);

    float transmittance = 1.0;
    vec3 scattering = vec3(0.0);
    float step_size = (t.y - t.x) / float(NUM_STEPS); // 64-128 steps

    for (int i = 0; i < NUM_STEPS; i++) {
        vec3 pos = rayOrigin + rayDir * (t.x + step_size * float(i));
        float density = cloudDensity(pos);

        if (density > 0.001) {
            // Light marching toward sun (6 steps)
            float light_density = 0.0;
            for (int j = 0; j < 6; j++) {
                vec3 light_pos = pos + sunDir * float(j) * LIGHT_STEP;
                light_density += cloudDensity(light_pos) * LIGHT_STEP;
            }

            float light_transmittance = beer(light_density);
            vec3 ambient = mix(CLOUD_BOTTOM_COLOR, CLOUD_TOP_COLOR,
                              (pos.y - CLOUD_MIN) / (CLOUD_MAX - CLOUD_MIN));

            scattering += (light_transmittance * sunColor + ambient)
                        * density * transmittance * step_size;
            transmittance *= beer(density * step_size);
        }

        if (transmittance < 0.01) break; // early exit
    }

    return vec4(scattering, transmittance);
}
```

---

## 4. Memory Budgets & Performance Summary

### Total GPU Memory Budget (per planet)

| System | Low Quality | Medium | High Quality |
|---|---|---|---|
| Atmosphere LUTs | 0.5 MB (FP16) | 4 MB | 8 MB (FP16) / 16 MB (FP32) |
| Heightmap (cubemap) | 6x512x512 R16 = 3 MB | 6x1024x1024 = 12 MB | 6x2048x2048 = 48 MB |
| Albedo (cubemap) | 6x512x512 RGBA8 = 6 MB | 6x1024x1024 = 24 MB | 6x2048x2048 = 96 MB |
| Normal map | 3 MB | 12 MB | 48 MB |
| Roughness map | 1.5 MB (R8) | 6 MB | 24 MB |
| Ocean FFT | 0.6 MB (128^2) | 5 MB (256^2) | 10 MB (512^2) |
| Cloud noise textures | 8 MB (64^3) | 20 MB (96^3) | 34 MB (128^3) |
| Cloud weather map | 0.5 MB | 1 MB | 1.5 MB |
| Biome LUT | <0.1 MB | <0.1 MB | <0.1 MB |
| **Total** | **~23 MB** | **~84 MB** | **~280 MB** |

### Per-Frame GPU Time Budget (targeting 60fps = 16.6ms total)

| System | Budget | Notes |
|---|---|---|
| Heightmap generation (if procedural) | 1-2 ms | Compute shader, LOD-dependent |
| Terrain rendering + texturing | 3-5 ms | Triplanar, 36 texture lookups |
| Atmosphere scattering | 0.5-1 ms | Precomputed LUT lookup only |
| Ocean FFT + rendering | 1-2 ms | 128x128 sufficient for real-time |
| Cloud ray marching | 2-3 ms | 1/16 pixels + temporal reprojection |
| Post-processing | 1-2 ms | Tone mapping, bloom |
| **Total** | **~8-15 ms** | Leaves headroom for UI, physics |

---

## 5. Sources

1. [AutoBiomes: Procedural Generation of Multi-Biome Landscapes (Springer, 2020)](https://link.springer.com/article/10.1007/s00371-020-01920-7)
2. [AutoBiomes PDF (CGI 2020)](https://cgvr.cs.uni-bremen.de/papers/cgi20/AutoBiomes.pdf)
3. [Azgaar: Biomes Generation and Rendering](https://azgaar.wordpress.com/2017/06/30/biomes-generation-and-rendering/)
4. [KdotJPG: Fast Biome Blending Without Squareness](https://noiseposti.ng/posts/2021-03-13-Fast-Biome-Blending-Without-Squareness.html)
5. [KdotJPG: Simple Biome Blending (GitHub)](https://github.com/KdotJPG/Simple-Biome-Blending)
6. [NVIDIA GPU Gems 3, Ch.1: Generating Complex Procedural Terrains Using the GPU](https://developer.nvidia.com/gpugems/gpugems3/part-i-geometry/chapter-1-generating-complex-procedural-terrains-using-gpu)
7. [Bruneton: Precomputed Atmospheric Scattering (New Implementation)](https://ebruneton.github.io/precomputed_atmospheric_scattering/)
8. [NVIDIA GPU Gems 2, Ch.16: Accurate Atmospheric Scattering](https://developer.nvidia.com/gpugems/gpugems2/part-ii-shading-lighting-and-shadows/chapter-16-accurate-atmospheric-scattering)
9. [PBR Texture Maps Explained (AITextured)](https://aitextured.com/articles/pbr_texture_maps_explained_albedo_normal_roughness_metallic_orm.html)
10. [Sapra Projects: Texturing a Procedural World](https://ensapra.com/2023/06/texturing-the-world)
11. [Ben Golus: Normal Mapping for a Triplanar Shader](https://bgolus.medium.com/normal-mapping-for-a-triplanar-shader-10bf39dca05a)
12. [Tessendorf FFT Ocean (Godot 4 implementation)](https://github.com/tessarakkt/godot4-oceanfft)
13. [Paleologue: Ocean Simulation with FFT and WebGPU](https://barthpaleologue.github.io/Blog/posts/ocean-simulation-webgpu/)
14. [Guerrilla Games: Real-Time Volumetric Cloudscapes of Horizon Zero Dawn](https://www.guerrilla-games.com/read/the-real-time-volumetric-cloudscapes-of-horizon-zero-dawn)
15. [Joe Duffy: Climate Simulation for Procedural World Generation](https://www.joeduffy.games/climate-simulation-for-procedural-world-generation)
16. [Grenier: Volumetric Clouds](https://www.jpgrenier.org/clouds.html)
17. [PCG Wiki: Whittaker Diagram](http://pcg.wikidot.com/pcg-algorithm:whittaker-diagram)
18. [Procedural Planet Rendering (Jad Khoury)](https://jadkhoury.github.io/terrain_blog.html)
19. [Bruneton Atmospheric Scattering GLSL Functions](https://github.com/ebruneton/precomputed_atmospheric_scattering/blob/master/atmosphere/functions.glsl)
20. [AMD GPUOpen: Procedural Generation with Work Graphs](https://gpuopen.com/learn/work_graphs_mesh_nodes/work_graphs_mesh_nodes-procedural_generation/)
21. [Toft & Bowles: Optimisations for Real-Time Volumetric Cloudscapes (2016)](https://arxiv.org/pdf/1609.05344)
22. [Red Blob Games: Making Maps with Noise](https://www.redblobgames.com/maps/terrain-from-noise/)
23. [Procedural Biome Generation (Whiax/Python)](https://whiax.itch.io/pixplorer/devlog/1125883/very-fast-procedural-biome-generation-in-python)
24. [Ubisoft: Making Waves in Ocean Surface Rendering](https://www.ubisoft.com/en-us/studio/laforge/news/5WHMK3tLGMGsqhxmWls1Jw/making-waves-in-ocean-surface-rendering-using-tiling-and-blending)
25. [Bruneton Unity Port (Scrawk)](https://github.com/Scrawk/Brunetons-Improved-Atmospheric-Scattering)
26. [WebTide: FFT Ocean WebGPU (GitHub)](https://github.com/BarthPaleologue/WebTide)
27. [Koppen-Geiger at 1km Resolution (Nature Scientific Data)](https://www.nature.com/articles/sdata2018214)
28. [Tenjix: Climate-Based Biomes](https://tenjix.de/projects/climate-based-biomes/)
29. [Scratchapixel: Simulating the Colors of the Sky](https://www.scratchapixel.com/lessons/procedural-generation-virtual-worlds/simulating-sky/simulating-colors-of-the-sky.html)
30. [Meteoros: Real-time Cloudscape Rendering in Vulkan (GitHub)](https://github.com/AmanSachan1/Meteoros)
