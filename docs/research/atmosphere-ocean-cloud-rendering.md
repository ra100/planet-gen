# Atmospheric Scattering, Ocean Rendering, and Cloud Rendering for Procedural Planets

_Research date: 2026-03-28_

---

## 1. Atmospheric Scattering

### 1.1 Rayleigh and Mie Scattering Fundamentals

**Rayleigh scattering** is caused by small air molecules and scatters short wavelengths (blue) far more than long wavelengths (red). This produces blue skies during daytime and orange-red sunsets as blue light is scattered away along long atmospheric paths. [GPU Gems 2 Ch.16](https://developer.nvidia.com/gpugems/gpugems2/part-ii-shading-lighting-and-shadows/chapter-16-accurate-atmospheric-scattering)

**Mie scattering** is caused by larger aerosol particles (dust, pollution, water droplets) and scatters all wavelengths roughly equally, producing hazy/whitish skies and halos around light sources. [GPU Gems 2 Ch.16](https://developer.nvidia.com/gpugems/gpugems2/part-ii-shading-lighting-and-shadows/chapter-16-accurate-atmospheric-scattering)

**Phase functions:**

```glsl
// Rayleigh phase function
float phaseRayleigh(float cosTheta) {
    return 3.0 / (16.0 * PI) * (1.0 + cosTheta * cosTheta);
}

// Henyey-Greenstein phase function (used for Mie)
// g: asymmetry parameter (-1..1), typically ~0.76 for aerosols
float phaseHG(float cosTheta, float g) {
    float g2 = g * g;
    float denom = 1.0 + g2 - 2.0 * g * cosTheta;
    return (1.0 / (4.0 * PI)) * (1.0 - g2) / (denom * sqrt(denom));
}
```

**Optical depth** along a ray is the integral of density along the path. Density falls off exponentially with altitude:

```
density(h) = exp(-h / H0)
```

where H0 is the scale height (~8.5 km for Rayleigh, ~1.2 km for Mie). [GPU Gems 2 Ch.16](https://developer.nvidia.com/gpugems/gpugems2/part-ii-shading-lighting-and-shadows/chapter-16-accurate-atmospheric-scattering)

**Single scattering integral** (simplified):

```
L(camera, view_dir) = integral along ray {
    density(h) * phase(angle) * sunIrradiance
    * transmittance(sun -> sample) * transmittance(sample -> camera)
} ds
```

Numerically integrated by sampling discrete points along the camera ray and summing contributions.

### 1.2 Bruneton's Precomputed Atmospheric Scattering (2008/2017)

Bruneton and Neyret's method precomputes all scattering integrals into LUTs, enabling real-time multi-scattering with zero per-frame integration cost. [Bruneton Implementation](https://ebruneton.github.io/precomputed_atmospheric_scattering/) | [GitHub](https://github.com/ebruneton/precomputed_atmospheric_scattering)

**Parameterization** -- four variables:
| Parameter | Meaning |
|-----------|---------|
| `r` | Distance from planet center to sample point |
| `mu` (μ) | Cosine of view zenith angle |
| `mu_s` (μs) | Cosine of sun zenith angle |
| `nu` (ν) | Cosine of view-sun angle |

**LUT dimensions (from reference implementation):**

| Texture | Dimensions | Parameterization |
|---------|-----------|------------------|
| Transmittance | 256 x 64 (2D) | (r, μ) |
| Scattering (single + multi) | 256 x 128 x 32 (3D, packing 4D) | (r, μ, μs, ν) packed into 3D |
| Irradiance | 64 x 16 (2D) | (r, μs) |

The 4D scattering function is mapped into a 3D texture by packing the ν dimension into the width: `SCATTERING_TEXTURE_WIDTH = SCATTERING_TEXTURE_NU_SIZE * SCATTERING_TEXTURE_MU_S_SIZE`. Typical sizes: R=32, MU=128, MU_S=32, NU=8. [Bruneton definitions.h](https://ebruneton.github.io/precomputed_atmospheric_scattering/atmosphere/reference/definitions.h.html) | [DeepWiki](https://deepwiki.com/ebruneton/precomputed_atmospheric_scattering/2.2-atmospheric-scattering-functions)

**Key GLSL function signatures:**

```glsl
// Texture coordinate mapping
vec2 GetTransmittanceTextureUvFromRMu(float r, float mu);
vec4 GetScatteringTextureUvwzFromRMuMuSNu(float r, float mu, float mu_s, float nu);
vec2 GetIrradianceTextureUvFromRMuS(float r, float mu_s);

// Runtime lookups
DimensionlessSpectrum GetTransmittance(sampler2D transmittance_texture, float r, float mu);
RadianceSpectrum GetScattering(sampler3D scattering_texture, float r, float mu, float mu_s, float nu);
IrradianceSpectrum GetIrradiance(sampler2D irradiance_texture, float r, float mu_s);
```

UV mapping uses non-linear remapping to increase sampling rate near the horizon where visual accuracy matters most. [functions.glsl](https://github.com/ebruneton/precomputed_atmospheric_scattering/blob/master/atmosphere/functions.glsl)

**Multi-scattering:** The precomputation iteratively computes scattering orders 1..N (typically 4 orders), accumulating results. Each order takes the previous order's result as input.

### 1.3 Hillaire's Production-Ready Method (2020)

Sebastien Hillaire (Epic Games) introduced a scalable approach that eliminates the expensive 4D LUT entirely, replacing it with smaller per-frame LUTs. This is the technique used in Unreal Engine. [Hillaire 2020 Paper](https://onlinelibrary.wiley.com/doi/full/10.1111/cgf.14050) | [Slides](https://blog.selfshadow.com/publications/s2020-shading-course/hillaire/s2020_pbs_hillaire_slides.pdf)

**LUT pipeline (4 passes):**

| LUT | PC Size | Mobile Size | Steps | PC Cost | Mobile Cost |
|-----|---------|-------------|-------|---------|-------------|
| Transmittance | 256 x 64 | 256 x 64 | 40 | 0.01 ms | 0.53 ms |
| Multi-Scattering | 32 x 32 | 32 x 32 | 20 | 0.07 ms | 0.12 ms |
| Sky-View | 200 x 100 | 96 x 50 | 30/8 | 0.05 ms | 0.27 ms |
| Aerial Perspective | 32x32x32 | 32x32x16 | 30/8 | 0.04 ms | 0.11 ms |
| **On-screen render** | 1280x720 | -- | -- | **0.14 ms** | -- |
| **Total** | -- | -- | -- | **0.31 ms** | **~1.0 ms** |

[Hillaire 2020 via ReadKong](https://www.readkong.com/page/a-scalable-and-production-ready-sky-and-atmosphere-3211109)

**Key innovations:**
- **Multi-scattering LUT** approximates infinite scattering orders via geometric series: `F_ms = 1 / (1 - f_ms)` where `f_ms` is the fraction of light scattered per bounce, evaluated with 64 uniformly distributed directions.
- **Sky-View LUT** stores the distant sky parameterized by latitude/longitude for the current camera position. When viewed from space, it becomes less accurate (wastes texels on empty space), so the technique seamlessly falls back to per-pixel ray marching. [GameDev.net Planet Atmosphere](https://www.gamedev.net/blogs/entry/2276727-planet-rendering-atmosphere/)
- **Aerial Perspective LUT** stores in-scattering (RGB) + transmittance (A) as a view-frustum-aligned 3D texture with 32 depth slices over 32 km.

**Single scattering equation with multi-scattering approximation:**

```
L_scat(camera, x, v) = sigma_s(x) * sum_i [
    T(camera,x) * S(x,l_i) * p(v,l_i) + Psi_ms
] * E_i
```

Where `Psi_ms = L_2nd_order * F_ms`. [Hillaire 2020](https://onlinelibrary.wiley.com/doi/full/10.1111/cgf.14050)

### 1.4 Space vs Ground Camera Handling

For a procedural planet renderer, the camera can be anywhere from ground level to deep space. Key considerations:

- **From ground:** Sky-View LUT works well; standard parameterization covers the full hemisphere.
- **From space:** Much of the Sky-View LUT is wasted on black space; the technique switches to direct ray marching on screen pixels that intersect the atmosphere shell. [Hillaire 2020](https://onlinelibrary.wiley.com/doi/full/10.1111/cgf.14050)
- **Transition:** Needs smooth blending between LUT-based and ray-march modes at the atmosphere boundary.
- **Planet curvature:** Both Bruneton and Hillaire handle spherical geometry natively (parameterized by radius `r` from planet center).

---

## 2. Ocean Rendering

### 2.1 FFT-Based Ocean Waves (Tessendorf)

Jerry Tessendorf's 2001 paper "Simulating Ocean Water" is the foundation for FFT-based ocean simulation in real-time graphics. The method uses the inverse FFT to transform a frequency-domain wave spectrum into a spatial displacement field each frame. [Tessendorf via Barth Cave](https://barthpaleologue.github.io/Blog/posts/ocean-simulation-webgpu/) | [WSCG 2025](http://wscg.zcu.cz/WSCG2025/papers/C59.pdf)

**Pipeline:**

```
1. Initialize spectrum h0(k) from Phillips/JONSWAP spectrum
2. Each frame: compute h(k,t) = h0(k) * exp(i*w(k)*t) + conj(h0(-k)) * exp(-i*w(k)*t)
3. IFFT to get height field h(x,t)
4. IFFT of i*k*h(k,t) to get gradient (for normals)
5. IFFT of -i*(k/|k|)*h(k,t) to get horizontal displacement (choppy waves)
```

**Typical resolutions:** 256x256 to 512x512. A 512x512 FFT requires 262,144 complex multiplications per frame. [Barth Cave](https://barthpaleologue.github.io/Blog/posts/ocean-simulation-webgpu/)

**Phillips Spectrum:**

```
P(k) = A * exp(-1 / (k*L)^2) / k^4 * |dot(k_hat, wind_dir)|^2
```

where L = V^2/g (V = wind speed, g = gravity), A is amplitude constant. JONSWAP spectrum is a more accurate alternative that reduces tiling artifacts. [Barth Cave](https://barthpaleologue.github.io/Blog/posts/ocean-simulation-webgpu/)

**GPU compute shader approach:** The FFT is dispatched as a sequence of butterfly passes on the GPU. Modern implementations use compute shaders with shared memory for the butterfly operations. The entire ocean simulation (spectrum update + FFT + normal computation) typically costs 0.5-2 ms on modern GPUs. [Godot 4 OceanFFT](https://github.com/tessarakkt/godot4-oceanfft)

**Tiling artifacts:** A single FFT tile repeats visibly. Solutions include:
- Multiple cascades at different scales (near/medium/far)
- Ubisoft La Forge (HPG 2024): texture synthesis algorithms for aperiodic ocean surfaces [Ubisoft La Forge](https://www.ubisoft.com/en-us/studio/laforge/news/5WHMK3tLGMGsqhxmWls1Jw/making-waves-in-ocean-surface-rendering-using-tiling-and-blending)
- WSCG 2025: combining multiple simulations with different tile areas [WSCG 2025](http://wscg.zcu.cz/WSCG2025/papers/C59.pdf)

### 2.2 Gerstner Waves

Gerstner waves (trochoidal waves, 1802) are an analytical alternative to FFT. They move vertices laterally toward wave crests, producing sharper peaks and flatter troughs than sine waves. [GPU Gems Ch.1](https://developer.nvidia.com/gpugems/gpugems/part-i-natural-effects/chapter-1-effective-water-simulation-physical-models)

**Displacement equation:**

```glsl
// Per wave i, applied to vertex position P:
// D_i = direction of wave i (unit vec2)
// w_i = 2*PI / wavelength_i
// phi_i = speed_i * w_i (phase constant)
// A_i = amplitude
// Q_i = steepness (0 = sine, 1/(w_i*A_i) = max before looping)

vec3 gerstnerDisplacement(vec2 P, float t) {
    vec3 offset = vec3(0);
    for each wave i {
        float theta = dot(w_i * D_i, P) + phi_i * t;
        offset.x += Q_i * A_i * D_i.x * cos(theta);
        offset.z += Q_i * A_i * D_i.y * cos(theta);  // xz plane
        offset.y += A_i * sin(theta);                   // height
    }
    return offset;
}
```

**Normal from derivatives:**

```glsl
vec3 gerstnerNormal(vec2 P, float t) {
    vec3 N = vec3(0, 1, 0);
    for each wave i {
        float theta = dot(w_i * D_i, P) + phi_i * t;
        float WA = w_i * A_i;
        N.x -= D_i.x * WA * cos(theta);
        N.z -= D_i.y * WA * cos(theta);
        N.y -= Q_i * WA * sin(theta);
    }
    return normalize(N);
}
```

**Performance comparison (from BTH thesis):**
- Gerstner: ~3500 computations/sec at 1024x1024 (single compute shader)
- FFT: ~200 computations/sec at 1024x1024 (spectrum update + FFT passes)
- Gerstner is ~17x faster per dispatch but lacks high-frequency detail
- Typical approach: 4 Gerstner waves in vertex shader + 15 waves in pixel shader for detail [GPU Gems Ch.1](https://developer.nvidia.com/gpugems/gpugems/part-i-natural-effects/chapter-1-effective-water-simulation-physical-models)

**For a planet renderer:** Gerstner waves are excellent for large-scale ocean swell visible from space; FFT adds detail at closer range. The two can be combined.

### 2.3 Ocean PBR: Fresnel, Subsurface Scattering, Foam

**Fresnel -- Schlick's approximation:**

```glsl
// R0 for water (IOR ~1.33): R0 = ((1.33 - 1) / (1.33 + 1))^2 = 0.02
float fresnelSchlick(float cosTheta, float R0) {
    return R0 + (1.0 - R0) * pow(1.0 - cosTheta, 5.0);
}

// In water shader:
float NdotV = max(dot(normal, viewDir), 0.0);
float fresnel = fresnelSchlick(NdotV, 0.02);
vec3 color = mix(refractionColor, reflectionColor, fresnel);
```

At steep viewing angles (looking down), the water appears translucent. At grazing angles, it becomes highly reflective. [Khronos Forums](https://community.khronos.org/t/improving-water-shader/54388)

**Subsurface scattering (SSS) approximation:**

```glsl
// Fake SSS using wave normals and sun direction
float sss = pow(max(dot(viewDir, -sunDir + normal * sssDistortion), 0.0), sssPower)
           * sssScale * thickness;
vec3 sssColor = sss * waterAbsorptionColor * sunColor;
```

The wave normals create a view-dependent translucency effect, particularly visible in thin wave crests backlit by the sun. [Crest Ocean System](https://crest.readthedocs.io/en/4.18/user/water-appearance.html)

**Absorption/color:**

```glsl
// Beer-Lambert absorption through water depth
vec3 absorption = exp(-waterDepth * absorptionCoeff);
// absorptionCoeff ~= vec3(0.46, 0.09, 0.06) for clear ocean (red absorbed most)
vec3 deepColor = vec3(0.0, 0.03, 0.05);  // deep ocean color
vec3 shallowColor = vec3(0.0, 0.4, 0.3); // shallow turquoise
vec3 waterColor = mix(deepColor, shallowColor, absorption);
```

**Foam system (from Crest Ocean):**
Three independent layers:
1. **Wave-breaking whitecaps** -- generated from Jacobian of displacement (where horizontal compression exceeds threshold)
2. **Ambient surface foam** -- persistent surface texture
3. **Shoreline foam** -- generated where water depth is shallow; uses depth buffer

```glsl
// Jacobian-based foam detection (from FFT displacement)
float jacobian = 1.0 + dDx.x + dDy.y + dDx.x * dDy.y - dDx.y * dDy.x;
float foam = saturate(-jacobian);  // negative Jacobian = wave folding
```

[Crest Ocean System](https://crest.readthedocs.io/en/4.18/user/water-appearance.html)

---

## 3. Volumetric Cloud Rendering

### 3.1 Ray Marching Approach

The standard real-time volumetric cloud technique (pioneered by Andrew Schneider at Guerrilla Games for Horizon: Zero Dawn) uses ray marching through a cloud layer shell, sampling 3D noise textures for density. [Guerrilla Games SIGGRAPH 2015](https://advances.realtimerendering.com/s2015/The%20Real-time%20Volumetric%20Cloudscapes%20of%20Horizon%20-%20Zero%20Dawn%20-%20ARTR.pdf) | [Schneider VFX](https://www.schneidervfx.com/)

**Ray marching pseudocode:**

```glsl
vec4 raymarchClouds(vec3 rayOrigin, vec3 rayDir) {
    // Intersect ray with cloud shell (e.g., 1500m - 4000m altitude)
    vec2 tMinMax = intersectShell(rayOrigin, rayDir, cloudBottomRadius, cloudTopRadius);

    float transmittance = 1.0;
    vec3 scatteredLight = vec3(0.0);
    float t = tMinMax.x;

    // Apply Bayer 4x4 dither offset to reduce banding
    t += bayerOffset * stepSize;

    for (int i = 0; i < MAX_STEPS; i++) {  // 64-128 steps
        vec3 pos = rayOrigin + rayDir * t;
        float density = sampleCloudDensity(pos);

        if (density > 0.0) {
            // Light march toward sun (6 samples in cone)
            float lightOpticalDepth = lightMarch(pos, sunDir, 6);

            // Beer-Lambert attenuation
            float lightTransmittance = exp(-lightOpticalDepth);

            // Phase function (dual-lobe HG)
            float phase = mix(phaseHG(cosTheta, 0.8),
                            phaseHG(cosTheta, -0.5), 0.5);

            // Beer-Powder effect (reduces out-scattering at edges)
            float beerPowder = 2.0 * exp(-density) * (1.0 - exp(-2.0 * density));

            vec3 luminance = sunColor * lightTransmittance * phase * beerPowder;
            luminance += ambientColor;  // sky ambient

            float stepTransmittance = exp(-density * stepSize * extinction);
            scatteredLight += luminance * transmittance * (1.0 - stepTransmittance);
            transmittance *= stepTransmittance;

            if (transmittance < 0.01) break;  // early exit
        }
        t += stepSize;
        if (t > tMinMax.y) break;
    }
    return vec4(scatteredLight, 1.0 - transmittance);
}
```

### 3.2 Noise-Based Cloud Density

**3D noise textures (from Horizon: Zero Dawn):**

| Texture | Resolution | Channels | Content |
|---------|-----------|----------|---------|
| Base shape | 128x128x128 | RGBA | R: Perlin-Worley, GBA: Worley at increasing frequencies |
| Detail erosion | 32x32x32 | RGB | Worley at increasing frequencies |
| Curl noise (2D) | 128x128 | RGB | Non-divergent noise for turbulence distortion |
| Weather map (2D) | 512x512 (or baked) | RGB | R: coverage, G: cloud type, B: wetness |

[Guerrilla Games 2015](https://advances.realtimerendering.com/s2015/The%20Real-time%20Volumetric%20Cloudscapes%20of%20Horizon%20-%20Zero%20Dawn%20-%20ARTR.pdf) | [jpg's blog](https://www.jpgrenier.org/clouds.html)

**Cloud density function:**

```glsl
float sampleCloudDensity(vec3 pos) {
    // Height fraction within cloud layer (0..1)
    float heightFrac = getHeightFraction(pos);

    // Sample weather map (coverage, type, wetness)
    vec3 weather = texture(weatherMap, pos.xz * weatherScale).rgb;
    float coverage = weather.r;

    // Base shape from 3D Perlin-Worley
    vec4 baseNoise = texture(baseShapeNoise, pos * baseScale);
    float baseShape = remap(baseNoise.r, -(1.0 - baseNoise.g * 0.625
                            - baseNoise.b * 0.25 - baseNoise.a * 0.125), 1.0, 0.0, 1.0);

    // Apply height gradient based on cloud type
    float densityHeightGradient = getDensityForCloudType(heightFrac, weather.g);
    baseShape *= densityHeightGradient;

    // Apply coverage
    float baseDensity = remap(baseShape, 1.0 - coverage, 1.0, 0.0, 1.0);
    baseDensity *= coverage;

    // Detail erosion (only if base density > 0 -- "cheap vs expensive" optimization)
    if (baseDensity > 0.0) {
        vec3 detailNoise = texture(detailNoise3D, pos * detailScale).rgb;
        float detailFBM = detailNoise.r * 0.625 + detailNoise.g * 0.25 + detailNoise.b * 0.125;
        // Erode more at edges, less at base
        float detailMod = mix(detailFBM, 1.0 - detailFBM, clamp(heightFrac * 10.0, 0.0, 1.0));
        baseDensity = remap(baseDensity, detailMod * 0.35, 1.0, 0.0, 1.0);
    }
    return max(baseDensity, 0.0);
}
```

### 3.3 Lighting: Beer-Lambert and Henyey-Greenstein

**Beer-Lambert law** governs light attenuation through a participating medium:

```
transmittance = exp(-optical_depth)
optical_depth = integral of (density * extinction_coefficient) along ray
```

**Henyey-Greenstein phase function** describes the angular distribution of scattered light:

```glsl
// Single-lobe HG
float phaseHG(float cosTheta, float g) {
    float g2 = g * g;
    float denom = 1.0 + g2 - 2.0 * g * cosTheta;
    return (1.0 / (4.0 * PI)) * (1.0 - g2) / (denom * sqrt(denom));
}

// Dual-lobe (common for clouds): forward + back scatter
float phaseDualHG(float cosTheta, float g1, float g2, float blend) {
    return mix(phaseHG(cosTheta, g1), phaseHG(cosTheta, g2), blend);
}
// Typical: g1 = 0.8 (strong forward), g2 = -0.5 (weak backward), blend = 0.5
```

Parameters: g > 0 = forward scattering (silver lining effect), g < 0 = back scattering. For clouds, g ~ 0.8 is typical for the primary forward lobe. [Wikipedia HG](https://en.wikipedia.org/wiki/Henyey%E2%80%93Greenstein_phase_function) | [PBR Book Phase Functions](https://pbr-book.org/3ed-2018/Volume_Scattering/Phase_Functions) | [NVIDIA Approximate Mie](https://research.nvidia.com/labs/rtr/approximate-mie/publications/approximate-mie.pdf)

**Beer-Powder effect** (Schneider, Guerrilla):

```glsl
// Combines Beer attenuation with a "powder" term that prevents
// the interior of clouds from being too dark
float beerPowder(float density) {
    float beer = exp(-density);
    float powder = 1.0 - exp(-2.0 * density);
    return 2.0 * beer * powder;
}
```

This approximates the increased brightness at thin cloud edges where in-scattering exceeds out-scattering. [jpg's blog](https://www.jpgrenier.org/clouds.html)

### 3.4 Cloud Shadow Mapping

Two approaches for casting cloud shadows onto the ground:

**1. Ray-marched shadow (Guerrilla approach):**
- For each ground pixel, march a ray toward the sun through the cloud layer
- Sample cloud density along the ray; compute transmittance via Beer-Lambert
- Expensive but accurate; can be done at reduced resolution

**2. Beer Shadow Maps (BSM):**
- Render the cloud layer from the light's perspective into a shadow map
- Store optical depth instead of binary shadow
- A single `tex3D` lookup gets the volumetric shadow [UE4 Volumetric Clouds](https://docs.unrealengine.com/4.26/en-US/BuildingWorlds/LightingAndShadows/VolumetricClouds/)

**3. Cascaded deep opacity maps:**
- Similar to cascaded shadow maps but store opacity at multiple depths
- Covers different frustum splits with varying resolution [UHawk VR Blog](https://blog.uhawkvr.com/rendering/rendering-volumetric-clouds-using-signed-distance-fields/)

### 3.5 Temporal Reprojection and Performance

**The core performance problem:** Full-resolution ray marching with 64-128 steps + 6 light samples per step is far too expensive for real-time.

**Temporal reprojection (Guerrilla's approach):**
- Render clouds at 1/16th resolution per frame (4x4 checkerboard)
- Use a **4x4 Bayer matrix** pattern to offset ray starting positions, cycling through all 16 positions over 16 frames
- Reconstruct full resolution by reprojecting previous frames using motion vectors
- 1D Halton sequence of 8 values for additional temporal jittering
- Blend factor: ~75% of the 16th frame to integrate Halton samples
- Invalidated pixels (disocclusion) fall back to noisier single-frame samples

[jpg's blog](https://www.jpgrenier.org/clouds.html) | [Vertex Fragment upsampling](https://www.vertexfragment.com/ramblings/volumetric-cloud-upsampling/)

**Performance budgets:**

| Implementation | Target | Actual Cost | Notes |
|---------------|--------|-------------|-------|
| Horizon: Zero Dawn (PS4) | 2 ms | ~2 ms | 1/16 resolution, 64+6 samples |
| Shadertoy 60fps iGPU | 16.6 ms total | feasible | Aggressive LOD |
| Unreal Engine 5 (PC) | ~2-4 ms | configurable | Cascaded shadow maps optional |
| Half-resolution rendering | -- | ~50% saving | Bilateral upsampling needed |

[Guerrilla Games](https://www.guerrilla-games.com/read/the-real-time-volumetric-cloudscapes-of-horizon-zero-dawn) | [Shadertoy](https://www.shadertoy.com/view/DtBGR1)

---

## 4. Performance Summary and Integration Strategy

### 4.1 Frame Budget Breakdown (targeting 60fps = 16.6 ms)

| System | Typical Cost (PC) | Notes |
|--------|-------------------|-------|
| Atmosphere LUTs (Hillaire) | 0.17 ms precompute | Updated per frame; on-screen pass 0.14 ms |
| Atmosphere on-screen | 0.14 ms | Sky-View LUT sample + aerial perspective |
| Ocean FFT (512x512, 3 cascades) | 0.5-2.0 ms | Spectrum + FFT + normals |
| Ocean shading | 0.5-1.0 ms | Fresnel, SSS, foam, reflections |
| Cloud ray marching (1/16 res) | 1.5-2.0 ms | 64 steps + 6 light samples |
| Cloud temporal reproject | 0.1-0.3 ms | Motion vectors + blend |
| Cloud shadows | 0.2-0.5 ms | BSM or reduced-res ray march |
| **Total atmosphere+ocean+clouds** | **~3-6 ms** | Leaves ~10 ms for terrain, objects, post |

### 4.2 Resolution Tradeoffs

| Technique | Low Quality | Medium | High | Ultra |
|-----------|------------|--------|------|-------|
| Ocean FFT | 128x128 | 256x256 | 512x512 | 512x512 x3 cascades |
| Cloud ray march | 1/16 res, 32 steps | 1/16 res, 64 steps | 1/4 res, 64 steps | Full res, 128 steps |
| Atmosphere Sky-View | 96x50 | 128x64 | 200x100 | 200x100 |
| Atmosphere Aerial | 32x32x16 | 32x32x32 | 64x64x32 | 64x64x64 |

### 4.3 Key Implementation References

| Topic | Primary Reference | URL |
|-------|------------------|-----|
| Atmosphere (production) | Hillaire 2020 | [Paper](https://onlinelibrary.wiley.com/doi/full/10.1111/cgf.14050) |
| Atmosphere (precomputed) | Bruneton 2008/2017 | [GitHub](https://github.com/ebruneton/precomputed_atmospheric_scattering) |
| Atmosphere (simple) | GPU Gems 2, Ch.16 | [NVIDIA](https://developer.nvidia.com/gpugems/gpugems2/part-ii-shading-lighting-and-shadows/chapter-16-accurate-atmospheric-scattering) |
| Atmosphere GLSL (minimal) | glsl-atmosphere | [GitHub](https://github.com/wwwtyro/glsl-atmosphere) |
| Ocean FFT | Tessendorf 2001 | [WebGPU impl](https://barthpaleologue.github.io/Blog/posts/ocean-simulation-webgpu/) |
| Ocean Gerstner | GPU Gems Ch.1 | [NVIDIA](https://developer.nvidia.com/gpugems/gpugems/part-i-natural-effects/chapter-1-effective-water-simulation-physical-models) |
| Ocean (Crest, Unity) | Crest Ocean System | [Docs](https://crest.readthedocs.io/en/4.18/user/water-appearance.html) |
| Clouds | Schneider (Guerrilla) 2015 | [Slides PDF](https://advances.realtimerendering.com/s2015/The%20Real-time%20Volumetric%20Cloudscapes%20of%20Horizon%20-%20Zero%20Dawn%20-%20ARTR.pdf) |
| Clouds (detailed breakdown) | jpg's blog | [Blog](https://www.jpgrenier.org/clouds.html) |
| Clouds (resources list) | pixelsnafu | [GitHub Gist](https://gist.github.com/pixelsnafu/e3904c49cbd8ff52cb53d95ceda3980e) |
| Cloud phase function | Henyey-Greenstein | [Wikipedia](https://en.wikipedia.org/wiki/Henyey%E2%80%93Greenstein_phase_function) |
| Real-time atmosphere+clouds (Vulkan) | Sakmary 2023 | [CESCG Paper](https://cescg.org/wp-content/uploads/2023/04/Sakmary-Real-time-Rendering-of-Atmosphere-and-Clouds-in-Vulkan.pdf) |
| WebGPU atmosphere | webgpu-sky-atmosphere | [GitHub](https://github.com/JolifantoBambla/webgpu-sky-atmosphere) |
| Alan Zucconi tutorial series | Atmospheric scattering | [Tutorial](https://www.alanzucconi.com/2017/10/10/atmospheric-scattering-7/) |
