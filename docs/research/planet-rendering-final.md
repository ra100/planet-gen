# Physically Accurate Planet Rendering: Consolidated Reference

**Editor's Note:** This document consolidates the best content from two independent research efforts (Coder 4/Opus and Coder 6/GLM-5-Turbo). Opus provided the primary structure, mathematical rigor, and production-quality GLSL code. GLM-5-Turbo contributed the Rayleigh depolarization correction (King factor), the Pearl-Bracey effective transmittance approximation, methane absorption data for ice giants, and several additional references. Errors found in both documents have been corrected.

---

## Table of Contents

1. [Mathematical Foundation](#1-mathematical-foundation)
2. [Atmospheric Scattering](#2-atmospheric-scattering)
3. [Atmospheric Layers & Composition](#3-atmospheric-layers--composition)
4. [Aerosols & Particulates](#4-aerosols--particulates)
5. [Cloud Rendering](#5-cloud-rendering)
6. [Surface Rendering](#6-surface-rendering)
7. [Limb Effects](#7-limb-effects)
8. [Light Transport](#8-light-transport)
9. [Implementation Approaches](#9-implementation-approaches)
10. [Gas Giant & Exotic Planet Considerations](#10-gas-giant--exotic-planet-considerations)
11. [Open-Source Implementations & Resources](#11-open-source-implementations--resources)
12. [References](#12-references)

---

## 1. Mathematical Foundation

### 1.1 The Volume Rendering Integral

All atmospheric and planetary rendering derives from the **radiative transfer equation (RTE)** for participating media. For a ray traveling from point **A** to point **B**:

```
L(A, ω) = L(B, ω) · T(A, B) + ∫[A→B] T(A, P) · J(P, ω) ds
```

Where:
- **L(B, ω)** is the radiance at point B in direction ω
- **T(A, B)** is the **transmittance** between A and B
- **J(P, ω)** is the **source function** at point P (in-scattered light)
- **ds** is the differential path length element

### 1.2 Transmittance and Optical Depth

```
T(A, B) = exp(-τ(A, B))
τ(A, B) = ∫[A→B] β_e(P) ds
β_e = β_s + β_a
```

For Earth's atmosphere, molecular absorption is negligible for Rayleigh (β_a ≈ 0), but ozone introduces significant absorption in specific bands.

### 1.3 The Source Function (Single Scattering)

Only direct sunlight is considered:

```
J_single(P, ω) = β_s(P) · p(ω, ω_sun) · I_sun · T(P, P_sun)
```

### 1.4 Phase Functions

#### 1.4.1 Rayleigh Phase Function

```
P_R(cos θ) = (3 / 16π) · (1 + cos²θ)
```

Symmetric (equal forward/backward scattering). Normalized over the sphere.

#### 1.4.2 Henyey-Greenstein (HG) Phase Function

```
P_HG(cos θ, g) = (1 / 4π) · (1 - g²) / (1 + g² - 2g·cos θ)^(3/2)
```

- **g = 0**: isotropic, **g ≈ 0.76**: Earth aerosols (forward scattering)

**Importance sampling** (analytically invertible):

```glsl
float sampleHG(float g, float xi) {
    if (abs(g) < 1e-3)
        return 1.0 - 2.0 * xi;
    float sqrTerm = (1.0 - g * g) / (1.0 + g - 2.0 * g * xi);
    return -1.0 / (2.0 * g) * (1.0 + g * g - sqrTerm * sqrTerm);
}
```

#### 1.4.3 Double Henyey-Greenstein

```
P_DHG(cos θ) = α · P_HG(cos θ, g₁) + (1 - α) · P_HG(cos θ, g₂)
```

Typical: g₁ = 0.8, g₂ = -0.5, α = 0.7.

#### 1.4.4 Cornette-Shanks Phase Function

More physically plausible than HG — includes the (1 + cos²θ) factor:

```
P_CS(cos θ, g) = (3 / 8π) · (1 - g²)(1 + cos²θ) / ((2 + g²)(1 + g² - 2g·cos θ)^(3/2))
```

#### 1.4.5 Draine Phase Function (NVIDIA 2023)

A blend of HG and Draine's function matches ~95% of true Mie scattering:

```
P_Draine(cos θ, g, α) = (1/4π) · (1 - g²) / (1 + g² - 2g·cos θ)^(3/2)
                         · (1 + α·cos²θ) / (1 + α(1 + 2g²)/3)
```

Analytically invertible — practical for GPU path tracing.

### 1.5 Density Profiles

#### Exponential Profile (standard atmosphere)

```
ρ(h) = ρ₀ · exp(-h / H)
```

Earth: H_R = 8 km (Rayleigh), H_M = 1.2 km (Mie).

#### Tent (Triangular) Profile (ozone layer)

```
ρ_O₃(h) = max(0, 1 - |h - h₀| / w)
```

Ozone: h₀ = 25 km, w = 15 km.

#### Custom Profiles (Bruneton 2017)

```cpp
struct DensityProfileLayer {
    double width, exp_term, exp_scale, linear_term, constant_term;
};
// Density = exp_term * exp(exp_scale * h) + linear_term * h + constant_term
```

#### Gaussian Profile (volcanic stratospheric aerosol)

```
ρ(h) = ρ₀ · exp(-((h - 20) / 5)²)
```

Peak at 20 km after major eruptions.

---

## 2. Atmospheric Scattering

### 2.1 Rayleigh Scattering

Particles much smaller than wavelength (N₂, O₂, ~0.1 nm vs. 380–780 nm). Strong λ⁻⁴ wavelength dependence.

**Cross-section with King correction (depolarization factor):**

```
σ_R(λ) = (8π³(n² - 1)²) / (3N²λ⁴) · ((6 + 3δ)/(6 - 7δ))
```

- **n** ≈ 1.000293, **N** ≈ 2.547 × 10²⁵ m⁻³, **δ** ≈ 0.035 (depolarization factor for air)

The King correction factor (6 + 3δ)/(6 - 7δ) ≈ 1.049 accounts for molecular anisotropy and increases the cross-section by ~5%.

**Precomputed sea-level coefficients** (Bruneton):

| Wavelength | β_R(0) |
|-----------|--------|
| 440 nm (Blue) | 33.1 × 10⁻⁶ m⁻¹ |
| 550 nm (Green) | 13.5 × 10⁻⁶ m⁻¹ |
| 680 nm (Red) | 5.8 × 10⁻⁶ m⁻¹ |

**RGB vector: β_R(0) = (5.802, 13.558, 33.1) × 10⁻⁶ m⁻¹** (at 680, 550, 440 nm)

```glsl
vec3 betaRayleigh(float h) {
    const vec3 betaR0 = vec3(5.802e-6, 13.558e-6, 33.1e-6);
    const float HR = 8000.0;
    return betaR0 * exp(-h / HR);
}
```

### 2.2 Mie Scattering

Aerosols (0.1–10 μm). Approximately wavelength-independent. Strongly forward-scattering.

```
β_M(h) = β_M(0) · exp(-h / H_M)
β_M(0) ≈ 21.0 × 10⁻⁶ m⁻¹,  H_M = 1.2 km
```

Unlike Rayleigh, Mie has significant absorption:

```
β_M_extinction = β_M_scattering / 0.9 ≈ 1.11 × β_M_scattering
```

Single-scattering albedo ω₀ ≈ 0.9 for typical aerosols.

```glsl
float betaMieScattering(float h) {
    return 21.0e-6 * exp(-h / 1200.0);
}
float betaMieExtinction(float h) {
    return betaMieScattering(h) / 0.9;
}
```

### 2.3 Single Scattering

```
L_single(A, ω) = ∫[A→B] [β_R(P)·P_R(cos θ) + β_M(P)·P_M(cos θ)]
                         · I_sun · T(P, P_sun) · T(A, P) ds
```

### 2.4 Multiple Scattering

Essential for accurate sky color, twilight, thick atmospheres, and shadow-side fill light.

**Approaches:**

1. **Constant ambient** (Nishita 1993): Crude.
2. **Iterative accumulation** (Bruneton & Neyret 2008): Precompute successive orders in 4D LUTs.
3. **Isotropic approximation** (Hillaire 2020): Closed-form geometric series `L_ms ≈ L₂ / (1 - f_ms)`.
4. **Empirical scaling**: Quick hack (1.2–2.0× single scatter).
5. **Powder effect** (Hillaire 2020): Ground-atmosphere bouncing term `L_powder = R_g · c · (1 - T(c,s)) · (1 - T(c,eye))`.

### 2.5 Optical Depth Calculation

```glsl
float opticalDepth(vec3 rayOrigin, vec3 rayDir, float rayLength, int numSamples) {
    float ds = rayLength / float(numSamples);
    float tau = 0.0;
    for (int i = 0; i < numSamples; i++) {
        float t = (float(i) + 0.5) * ds;
        float h = length(rayOrigin + rayDir * t) - planetRadius;
        tau += betaExtinction(h) * ds;
    }
    return tau;
}
```

### 2.6 Ray Marching

```glsl
vec2 raySphereIntersect(vec3 ro, vec3 rd, float radius) {
    float b = dot(ro, rd);
    float c = dot(ro, ro) - radius * radius;
    float discriminant = b * b - c;
    if (discriminant < 0.0) return vec2(-1.0);
    float sqrtDisc = sqrt(discriminant);
    return vec2(-b - sqrtDisc, -b + sqrtDisc);
}
```

**Pseudocode:**
```
for each pixel:
    1. Cast ray, intersect atmosphere sphere and planet sphere
    2. Determine entry A and exit B
    3. March A→B in N steps:
        a. Compute altitude h, density ρ(h)
        b. Shadow ray toward sun, compute T(P, Sun)
        c. Accumulate in-scattered light × T(A, P)
    4. Return accumulated color + attenuated background
```

**Step counts:** Outer: 16–64 (real-time), 256+ (offline). Inner: 8–32 (real-time), 64+ (offline).

---

## 3. Atmospheric Layers & Composition

### 3.1 Earth's Atmospheric Layers

| Layer | Altitude | Rendering Relevance |
|-------|----------|---------------------|
| **Troposphere** | 0–12 km | Bulk of scattering, clouds, aerosols, weather |
| **Stratosphere** | 12–50 km | Ozone layer, twilight colors |
| **Mesosphere** | 50–80 km | Noctilucent clouds, minimal scattering |
| **Thermosphere** | 80–700 km | Aurora, negligible for rendering |

Typically only 0–60 km is modeled (density at 60 km is ~10⁻⁴ of sea level).

### 3.2 The Ozone Layer

Concentrated at 15–35 km, peaking at ~25 km. **Absorbs in the Chappuis band (400–650 nm)**, preferentially absorbing red/green:

| Wavelength | σ_O₃ |
|-----------|-------|
| 680 nm (red) | 0.650 × 10⁻²⁵ m² |
| 550 nm (green) | 0.085 × 10⁻²⁵ m² |
| 440 nm (blue) | ~0 |

```
β_e_total(h) = β_R(h) + β_M_extinction(h) + β_O₃_absorption(h)
```

Without ozone, twilight is too orange/red. Ozone creates the deep blue "blue hour."

### 3.3 Planet Atmosphere Parameters

#### Earth-Like

| Parameter | Value |
|-----------|-------|
| Planet radius | 6,360 km |
| Atmosphere height | 60 km |
| Rayleigh H | 8 km |
| Mie H | 1.2 km |
| β_R(0) | (5.802, 13.558, 33.1) × 10⁻⁶ m⁻¹ |
| β_M(0) | 21.0 × 10⁻⁶ m⁻¹ |
| Mie g | 0.76 |
| Ground albedo | 0.1–0.4 |

#### Mars-Like (thin CO₂, dust-dominated)

| Parameter | Value |
|-----------|-------|
| Planet radius | 3,390 km |
| Rayleigh H | ~11 km |
| Mie H | Variable (dust storms) |
| Mie g | 0.85–0.90 (fine dust) |
| Dominant scattering | Mie >> Rayleigh |
| Sky color | Butterscotch day; **blue at sunset** |

Mars blue sunsets occur because fine dust forward-scatters red light away from the observer's line of sight, leaving Rayleigh-scattered blue light visible near the sun.

#### Jupiter-Like (gas giant)

| Parameter | Value |
|-----------|-------|
| Planet radius | 69,911 km |
| Scale height | ~27 km |
| Composition | H₂, He, NH₃, CH₄, H₂O |
| Layers | NH₃ ice (~0.5 bar), NH₄SH (~2 bar), H₂O (~5 bar) |

#### Ice Giant (Neptune/Uranus) — Methane Absorption

| Gas | Absorption Bands | Visual Effect |
|-----|-----------------|---------------|
| CH₄ | Strong red/NIR (~600–900 nm) | Blue-green color (red absorbed with path length) |

To render: apply wavelength-dependent absorption that preferentially attenuates red with increasing atmospheric path length.

#### Thin/No Atmosphere (Moon, Mercury)

No scattering. Direct surface BRDF only. Sharp terminator.

---

## 4. Aerosols & Particulates

### 4.1 Types

| Type | Size | Scattering | ω₀ |
|------|------|-----------|-----|
| Fine aerosol | 0.01–0.1 μm | Moderate forward | ~0.97 |
| Coarse dust | 0.1–10 μm | Strong forward, ~λ-independent | 0.7–0.95 |
| Water droplets | 5–15 μm | Very strong forward | ~1.0 |
| Black carbon | 0.01–1 μm | Strong absorption | 0.2–0.4 |

### 4.2 Height Profiles

- **Boundary layer** (0–2 km): H ≈ 1–2 km, exponential
- **Elevated layer** (2–6 km): tent or Gaussian
- **Stratospheric** (15–25 km, volcanic): Gaussian `ρ₀·exp(-((h-20)/5)²)`

### 4.3 Haze Levels

- **Clear**: β_M(0) ≈ 2 × 10⁻⁵ m⁻¹
- **Hazy**: β_M(0) ≈ 5 × 10⁻⁵ m⁻¹
- **Heavy**: β_M(0) ≈ 2 × 10⁻⁴ m⁻¹

---

## 5. Cloud Rendering

### 5.1 Volumetric Cloud Density (Schneider 2015/2017)

State-of-the-art for real-time clouds:

1. **Base shape**: Low-freq Perlin-Worley noise (128³)
2. **Detail erosion**: High-freq Worley noise (32³)
3. **Weather map**: 2D coverage/type control
4. **Height gradient**: Cloud type shaping

```glsl
float sampleCloudDensity(vec3 pos) {
    float height_fraction = getHeightFraction(pos);

    vec4 lowFreqNoise = texture(cloudBaseNoise, pos * baseScale);
    float baseCloud = remap(lowFreqNoise.r, lowFreqNoise.g * 0.625
                           + lowFreqNoise.b * 0.25
                           + lowFreqNoise.a * 0.125 - 1.0, 1.0, 0.0, 1.0);

    float density_height = getDensityHeightGradient(height_fraction, cloudType);
    baseCloud *= density_height;

    float coverage = texture(weatherMap, pos.xz * weatherScale).r;
    baseCloud = remap(baseCloud, 1.0 - coverage, 1.0, 0.0, 1.0) * coverage;

    vec3 detailNoise = texture(cloudDetailNoise, pos * detailScale).rgb;
    float detailFBM = detailNoise.r * 0.625 + detailNoise.g * 0.25 + detailNoise.b * 0.125;
    baseCloud = remap(baseCloud, detailFBM * 0.35, 1.0, 0.0, 1.0);

    return max(baseCloud, 0.0);
}
```

### 5.2 Cloud Types

| Type | Altitude | Optical Thickness | Notes |
|------|----------|-------------------|-------|
| Cirrus | 6–12 km | Low (τ ≈ 0.1–3) | Ice crystals, forward-scatter g≈0.85 |
| Stratus | 0–2 km | Moderate (τ ≈ 5–20) | Nearly opaque |
| Cumulus | 2–6 km | High (τ ≈ 10–50) | Volumetric |
| Cumulonimbus | 2–15 km | Very high (τ ≈ 50–200) | Full volumetric, heavy multi-scatter |

### 5.3 Light Transport Through Clouds

#### Beer-Lambert Law

```
T = exp(-σ_t · d)
```

#### Powder Effect (Dark Edges)

```
powder(d, cos θ) = 1.0 - exp(-2.0 · σ_t · d)
E = 2.0 · exp(-σ_t · d) · (1.0 - exp(-2.0 · σ_t · d))
```

#### Pearl-Bracey Effective Transmittance (thick clouds)

Smooth transition from Beer-Lambert to diffusion regime:

```
T_eff(τ, g) ≈ (1 + g·τ)^(-1/g)
```

#### Multi-Scattering Octave Approximation

```glsl
vec3 cloudLighting(vec3 pos, vec3 lightDir, float density) {
    float lightOpticalDepth = computeLightOpticalDepth(pos, lightDir);
    float a = 1.0, b = 1.0, c = 1.0;
    float phaseVal = hgPhase(dot(viewDir, lightDir), 0.6);
    vec3 luminance = vec3(0.0);

    for (int i = 0; i < 8; i++) {
        luminance += b * exp(-lightOpticalDepth * a) * phaseVal;
        a *= 0.5; b *= 0.5; c *= 0.5;
        phaseVal = mix(1.0 / (4.0 * PI), phaseVal, c);
    }
    return luminance * sunColor;
}
```

#### Silver Lining Effect

Bright cloud edges when sunlit from behind — natural result of HG forward scattering with high g (~0.85) combined with multi-scattering.

### 5.4 Cloud Shadows

March from surface toward sun through cloud layer. Accumulate τ_cloud; apply `shadow = exp(-τ_cloud)`. For performance, precompute a 2D cloud shadow map from the sun's perspective.

---

## 6. Surface Rendering

### 6.1 BRDF Models

**Lambertian:** `f = ρ / π` — sufficient for most terrain.

**Oren-Nayar:** For rough surfaces (regolith, dust):
```
f_ON = (ρ/π) · (A + B · max(0, cos(φ_i - φ_o)) · sin α · tan β)
```

**Cook-Torrance (microfacet):** For water/ice:
```
f_CT = D · F · G / (4 · (n·ω_i)(n·ω_o))
```

**Hapke model:** Planetary science standard. Accounts for opposition surge, macro-roughness, regolith multiple scattering.

### 6.2 Ocean Specular (Sun Glint)

```glsl
vec3 oceanReflection(vec3 N, vec3 V, vec3 L, float roughness) {
    vec3 H = normalize(L + V);
    float NdotH = max(dot(N, H), 0.0);
    float NdotL = max(dot(N, L), 0.0);

    float alpha2 = roughness * roughness;
    float denom = NdotH * NdotH * (alpha2 - 1.0) + 1.0;
    float D = alpha2 / (PI * denom * denom);
    float F = 0.02 + 0.98 * pow(1.0 - max(dot(V, H), 0.0), 5.0);

    return vec3(D * F * NdotL);
}
```

Ocean roughness depends on wind speed. Ocean albedo: ~0.03 (calm, nadir) to ~0.1 (rough, oblique) without specular.

### 6.3 Night-Side Rendering

**City lights:** NASA "Black Marble" dataset, blended as sun dips below horizon, blocked by clouds.

```glsl
float nightBlend = smoothstep(-0.1, -0.3, dot(N, sunDir));
vec3 nightColor = texture(nightLightMap, uv).rgb * nightBlend * (1.0 - cloudCoverage);
```

**Thermal emission:** `L_thermal = ε · σ_SB · T⁴ / π` — visible above ~700K as dull red glow.

**Atmospheric night glow:** Airglow and light pollution produce faint luminance visible from space on the night side.

### 6.4 Terrain Textures

1. **Albedo map** — base color
2. **Normal map** — surface detail
3. **Specular map** — water/ice vs. land
4. **Cloud map** — cloud layer opacity
5. **Night map** — artificial/thermal emission
6. **Elevation map** — for parallax/displacement

Biome albedos: forest ~0.1–0.15, desert ~0.3–0.4, snow ~0.8–0.9, ocean ~0.06.

---

## 7. Limb Effects

### 7.1 Limb Darkening

Surface darkens near the edge: Lambertian falloff `I(μ) = I₀ · μ` where μ = cos(emission angle).

### 7.2 Atmospheric Limb Brightening

Atmosphere appears brightest at the limb because tangent viewing rays traverse maximum atmosphere thickness:

```
L_tangent ≈ √(2 · R_p · H)
```

For Earth: √(2 × 6360 × 8) ≈ 319 km (vs. ~60 km vertical). ~5× longer path.

### 7.3 Chapman Function

Air mass as function of zenith angle χ for spherical atmosphere:

```
Ch(h, χ) = ∫[0→∞] exp(-(r(s) - R_p) / H) ds
```

Approximates to H/cos(χ) for small angles but the full function accounts for curvature near the horizon.

### 7.4 Limb Rendering

```glsl
vec3 computeLimbGlow(vec3 rayOrigin, vec3 rayDir) {
    vec2 atmoHit = raySphereIntersect(rayOrigin, rayDir, atmosphereRadius);
    vec2 planetHit = raySphereIntersect(rayOrigin, rayDir, planetRadius);
    if (atmoHit.x < 0.0) return vec3(0.0);

    float tStart = max(atmoHit.x, 0.0);
    float tEnd = (planetHit.x > 0.0) ? planetHit.x : atmoHit.y;
    return rayMarchScattering(rayOrigin + rayDir * tStart, rayDir, tEnd - tStart);
}
```

---

## 8. Light Transport

### 8.1 Sun as Directional Light

Solar irradiance: ~1361 W/m². All rays parallel. Sun-side optical depth is a 2D function τ(h, θ_sun) — perfect for a 2D LUT.

Sun color after atmosphere: ~5778K blackbody filtered to roughly (1.0, 0.95, 0.8) at midday, (1.0, 0.5, 0.2) near sunset.

### 8.2 Phase Angle Effects

- **α ≈ 0°** (opposition): Full illumination, slight brightening from backscatter
- **α ≈ 90°** (quarter): Half disk, good for atmospheric effects
- **α ≈ 180°** (conjunction): Backlit, forward scattering halo visible

### 8.3 Shadow Calculations

#### Eclipse Shadows

```glsl
float eclipseShadow(vec3 surfacePos, vec3 sunPos, vec3 moonPos, float moonRadius) {
    vec3 toSun = normalize(sunPos - surfacePos);
    vec3 toMoon = normalize(moonPos - surfacePos);
    float sunAngR = atan(sunRadius / length(sunPos - surfacePos));
    float moonAngR = atan(moonRadius / length(moonPos - surfacePos));
    float separation = acos(dot(toSun, toMoon));

    if (separation > sunAngR + moonAngR) return 1.0;
    if (separation < moonAngR - sunAngR) return 0.0;
    float overlap = smoothstep(sunAngR + moonAngR,
                               abs(moonAngR - sunAngR), separation);
    return 1.0 - overlap;
}
```

### 8.4 Multiple Light Sources (Binary Stars)

```
L_total = Σᵢ L_scatter,i + L_surface · Πᵢ Tᵢ
```

Each star contributes independently with its own direction, spectral irradiance, and transmittance.

---

## 9. Implementation Approaches

### 9.1 Key Papers (Chronological)

| Paper | Year | Contribution |
|-------|------|-------------|
| **Nishita et al.** | 1993 | First atmosphere-from-space method, ray marching |
| **Preetham, Shirley, Smits** | 1999 | Analytical sky model, ground-level only |
| **Hoffman & Preetham** | 2002 | First GPU implementation (simplified) |
| **O'Neil** | 2005 | GPU Gems 2, single scattering, Shader Model 2.0 |
| **Bruneton & Neyret** | 2008 | Precomputed multi-scattering, 4D LUTs |
| **Bouthors et al.** | 2008 | Interactive multi-scattering in clouds |
| **Schneider** | 2015/2017 | Real-time volumetric clouds (Horizon Zero Dawn) |
| **Hillaire** | 2015 | Multi-scattering clouds in Frostbite |
| **Hillaire** | 2020 | Scalable atmosphere, UE5 SkyAtmosphere |
| **Elek et al.** | 2020 | Non-Earth atmospheres (Mars, Titan) |
| **Peters et al. (NVIDIA)** | 2023 | Approximate Mie scattering function |

### 9.2 LUT Strategies (Hillaire 2020)

| LUT | Dims | Resolution | Purpose |
|-----|------|-----------|---------|
| Transmittance | (h, μ_sun) | 256×64 | exp(-τ) from h to TOA |
| Multi-Scattering | (h, μ_sun) | 32×32 | Isotropic MS approximation |
| Sky-View | (view_zenith, azimuth) | 192×108 | Final sky radiance |
| Aerial Perspective | (screen UV, depth) | 32³ | Scene object atmosphere |

All updated per-frame (cheap enough).

#### Transmittance LUT Parameterization

```glsl
vec2 transmittanceLUTParams(float h, float mu) {
    float rho = sqrt(max((planetRadius + h) * (planetRadius + h)
                        - planetRadius * planetRadius, 0.0));
    float rhoH = sqrt(atmosphereRadius * atmosphereRadius
                     - planetRadius * planetRadius);
    float v = rho / rhoH;

    float H = sqrt(atmosphereRadius * atmosphereRadius
                  - planetRadius * planetRadius);
    float disc = (planetRadius + h) * (planetRadius + h) * (mu * mu - 1.0)
               + atmosphereRadius * atmosphereRadius;
    float d = max(sqrt(max(disc, 0.0)) - (planetRadius + h) * mu, 0.0);
    float dMin = atmosphereRadius - planetRadius - h;
    float dMax = rho + H;
    float u = (d - dMin) / (dMax - dMin);
    return vec2(u, v);
}
```

### 9.3 GPU Considerations

- **Float32** for optical depth and transmittance LUTs; Float16 elsewhere
- **Early termination** when transmittance < 0.001
- **Compute shaders** preferred for LUT generation
- **Decouple from screen resolution** — LUTs at fixed resolution
- **Temporal reprojection** for aerial perspective

### 9.4 Real-Time vs. Offline

| | Real-Time | Offline |
|--|-----------|---------|
| Method | Precomputed LUTs | Monte Carlo path tracing |
| Samples | 16–64 | 256–1024+ |
| Multi-scatter | Approximation | Hundreds of bounces |
| Spectral | RGB | Full spectral |
| Time | ~0.5–2 ms/frame | Hours/frame |

---

## 10. Gas Giant & Exotic Planet Considerations

### 10.1 Band Structure

Alternating **zones** (bright, high-pressure upwellings) and **belts** (dark, low-pressure). Render with latitude-dependent cloud properties and advected flow noise.

```glsl
struct JupiterBand {
    float latitudeCenter, width, cloudAltitude, windSpeed, turbulence;
    vec3 cloudAlbedo;
};
```

### 10.2 Metallic Hydrogen

Below ~100 GPa, hydrogen becomes metallic. Not visible externally but generates magnetic fields. Relevant for cross-section visualizations.

### 10.3 Hot Jupiter Thermal Emission

Equilibrium temperatures 1000–4000 K. Tidally locked → day/night temperature gradient.

```glsl
vec3 thermalEmission(float temperature) {
    vec3 color = blackbodyToRGB(temperature);
    float intensity = stefanBoltzmann(temperature);
    return color * intensity;
}

float temperatureMap(vec3 N, vec3 substellarPoint) {
    float cosAngle = dot(N, substellarPoint);
    return mix(1200.0, 2500.0,
               max(cosAngle, 0.0) * 0.7 + 0.3);  // heat redistribution
}
```

### 10.4 Ring Shadows and Ring Scattering

```glsl
float ringShadow(vec3 surfacePoint, vec3 sunDir,
                 float ringInner, float ringOuter) {
    float t = -surfacePoint.y / sunDir.y;
    if (t < 0.0) return 1.0;
    vec3 ringPoint = surfacePoint + sunDir * t;
    float r = length(ringPoint.xz);
    if (r < ringInner || r > ringOuter) return 1.0;
    return exp(-ringOpticalDepthProfile(r));
}

vec4 renderRing(vec3 rayOrigin, vec3 rayDir, vec3 sunDir) {
    float t = intersectRingPlane(rayOrigin, rayDir);
    vec3 P = rayOrigin + rayDir * t;
    float r = length(P.xz);
    if (r < ringInner || r > ringOuter) return vec4(0.0);

    float tau = ringOpticalDepthProfile(r);
    float transmittance = exp(-tau);
    float cosTheta = dot(rayDir, sunDir);
    float phase = ringPhaseFunction(cosTheta);
    vec3 albedo = texture(ringAlbedoProfile,
        vec2((r - ringInner) / (ringOuter - ringInner), 0.0)).rgb;

    return vec4(sunColor * (1.0 - transmittance) * phase * albedo, 1.0 - transmittance);
}
```

Rings exhibit **opposition surge** (brightening at zero phase angle) and strong forward/backward scattering.

### 10.5 Other Exotics

- **Tidally locked**: Permanent day/night hemispheres, possible atmospheric collapse on night side
- **Sub-Neptune/Hycean**: Thick H₂/He over water, deep blue from H₂ Rayleigh
- **Lava worlds**: Molten surface as emissive blackbody, thin/no atmosphere

---

## 11. Open-Source Implementations

| Project | Technique | URL |
|---------|-----------|-----|
| **Bruneton 2017** | Precomputed Atmospheric Scattering | https://github.com/ebruneton/precomputed_atmospheric_scattering |
| **Hillaire UE Sky** | Scalable Atmosphere (UE4/5) | https://github.com/sebh/UnrealEngineSkyAtmosphere |
| **Scratchapixel** | Nishita tutorial | https://www.scratchapixel.com/lessons/procedural-generation-virtual-worlds/simulating-sky/ |
| **GPU Gems 2 Ch16** | O'Neil GPU scattering | https://developer.nvidia.com/gpugems/gpugems2/part-ii-shading-lighting-and-shadows/chapter-16 |
| **PBRT v4** | Phase functions, volume rendering | https://pbr-book.org |

**Game engines:** UE5 SkyAtmosphere, Unity HDRP Physically Based Sky, Godot community shaders.

---

## 12. References

1. Nishita et al. (1993). "Display of the Earth Taking into Account Atmospheric Scattering." *SIGGRAPH*. [PDF](http://nishitalab.org/user/nis/cdrom/sig93_nis.pdf)
2. Preetham, Shirley, Smits (1999). "A Practical Analytic Model for Daylight." *SIGGRAPH*.
3. Hoffman & Preetham (2002). "Rendering Outdoor Light Scattering in Real Time." *GPU Gems*.
4. O'Neil (2005). "Accurate Atmospheric Scattering." *GPU Gems 2, Ch. 16*. [Link](https://developer.nvidia.com/gpugems/gpugems2/part-ii-shading-lighting-and-shadows/chapter-16-accurate-atmospheric-scattering)
5. Bruneton & Neyret (2008). "Precomputed Atmospheric Scattering." *CGF/Eurographics*. DOI: 10.1111/j.1467-8659.2008.01245.x
6. Bouthors et al. (2008). "Interactive Multiple Anisotropic Scattering in Clouds." *SIGGRAPH*.
7. Schneider (2015). "Real-time Volumetric Cloudscapes of Horizon Zero Dawn." *SIGGRAPH Advances*.
8. Schneider & Vos (2017). "Nubis: Authoring Real-Time Volumetric Cloudscapes." *SIGGRAPH*. [Link](https://www.guerrilla-games.com/read/nubis-authoring-real-time-volumetric-cloudscapes-with-the-decima-engine)
9. Bruneton (2017). "Evaluation of 8 Clear Sky Models." *IEEE TVCG*. DOI: 10.1109/TVCG.2016.2622272
10. Hillaire (2015). "Physically Based Sky, Atmosphere and Cloud Rendering in Frostbite." *SIGGRAPH*.
11. Hillaire (2020). "A Scalable and Production Ready Sky and Atmosphere Rendering Technique." *EGSR*. [PDF](https://sebh.github.io/publications/egsr2020.pdf)
12. Elek et al. (2020). "Interactive Visualization of Atmospheric Effects for Celestial Bodies." *IEEE TVCG*. DOI: 10.1109/TVCG.2020.3030333
13. Wilkie et al. (2021). "Physically Based Real-Time Rendering of Atmospheres using Mie Theory." *Eurographics*. DOI: 10.1111/cgf.15010
14. Peters et al. (2023). "An Approximate Mie Scattering Function." *SIGGRAPH*. [PDF](https://research.nvidia.com/labs/rtr/approximate-mie/publications/approximate-mie.pdf)
15. Riley & McGuire (2018). "Rendering GDC 2018: Multi-Layer Space Battles." Valve. [PDF](https://media.steampowered.com/apps/valve/2018/ValveRenderingGDC2018.pdf)
16. Wrenninge (2015). "Production Volume Rendering." *SIGGRAPH Course*.
17. Pharr, Jakob, Humphreys (2023). *Physically Based Rendering* (4th ed.). https://pbr-book.org
18. Henyey & Greenstein (1941). "Diffuse radiation in the galaxy." *ApJ*.
19. Cornette & Shanks (1992). "Physically reasonable analytic expression for the single-scattering phase function." *Applied Optics*.

---

## Appendix A: Implementation Checklist

### Level 1: Basic Single Scattering (1–2 days)
- [ ] Ray-sphere intersection (planet + atmosphere)
- [ ] Exponential density profile
- [ ] Rayleigh phase function
- [ ] Optical depth via ray marching (16 samples)
- [ ] Single-scattering integral (16×8)
- [ ] Sky dome from ground, add Mie + HG

### Level 2: Precomputed Transmittance (1 day)
- [ ] 2D Transmittance LUT (h, μ)
- [ ] Replace inner march with LUT lookups

### Level 3: Full Production Atmosphere (1–2 weeks)
- [ ] Hillaire multi-scattering LUT
- [ ] Sky-View LUT + Aerial Perspective LUT
- [ ] Ozone absorption
- [ ] Ground to space seamless

### Level 4: Advanced Effects
- [ ] Volumetric clouds (Schneider)
- [ ] Cloud shadows
- [ ] Surface BRDF + ocean specular
- [ ] Night-side city lights
- [ ] Ring rendering
- [ ] Multiple light sources

### Level 5: Non-Earth Planets
- [ ] Parameterizable atmosphere
- [ ] Custom density profiles
- [ ] Gas giant bands
- [ ] Thermal emission

---

## Appendix B: Complete Single-Scattering GLSL Shader

```glsl
#version 450

const float PI = 3.14159265359;
const float planetRadius = 6360000.0;
const float atmosphereRadius = 6420000.0;
const float HR = 8000.0;
const float HM = 1200.0;
const vec3 betaR = vec3(5.802e-6, 13.558e-6, 33.1e-6);
const float betaM = 21.0e-6;
const float g = 0.76;
const int VIEW_SAMPLES = 32;
const int LIGHT_SAMPLES = 8;

float phaseRayleigh(float cosTheta) {
    return 3.0 / (16.0 * PI) * (1.0 + cosTheta * cosTheta);
}

float phaseMie(float cosTheta, float g) {
    float g2 = g * g;
    float denom = 1.0 + g2 - 2.0 * g * cosTheta;
    return (1.0 / (4.0 * PI)) * (1.0 - g2) / (denom * sqrt(denom));
}

vec2 raySphereIntersect(vec3 origin, vec3 dir, float radius) {
    float a = dot(dir, dir);
    float b = 2.0 * dot(origin, dir);
    float c = dot(origin, origin) - radius * radius;
    float disc = b * b - 4.0 * a * c;
    if (disc < 0.0) return vec2(-1.0);
    disc = sqrt(disc);
    return vec2(-b - disc, -b + disc) / (2.0 * a);
}

vec3 computeAtmosphere(vec3 rayOrigin, vec3 rayDir, vec3 sunDir) {
    vec2 atmoHit = raySphereIntersect(rayOrigin, rayDir, atmosphereRadius);
    if (atmoHit.y < 0.0) return vec3(0.0);

    vec2 planetHit = raySphereIntersect(rayOrigin, rayDir, planetRadius);
    float tMax = (planetHit.x > 0.0) ? planetHit.x : atmoHit.y;
    float tMin = max(atmoHit.x, 0.0);

    float ds = (tMax - tMin) / float(VIEW_SAMPLES);
    float cosTheta = dot(rayDir, sunDir);

    vec3 totalRayleigh = vec3(0.0);
    vec3 totalMie = vec3(0.0);
    float opticalDepthR = 0.0;
    float opticalDepthM = 0.0;

    for (int i = 0; i < VIEW_SAMPLES; i++) {
        float t = tMin + (float(i) + 0.5) * ds;
        vec3 P = rayOrigin + rayDir * t;
        float h = length(P) - planetRadius;

        float densityR = exp(-h / HR) * ds;
        float densityM = exp(-h / HM) * ds;
        opticalDepthR += densityR;
        opticalDepthM += densityM;

        vec2 sunHit = raySphereIntersect(P, sunDir, atmosphereRadius);
        float dsLight = sunHit.y / float(LIGHT_SAMPLES);
        float opticalDepthLightR = 0.0;
        float opticalDepthLightM = 0.0;

        bool blocked = false;
        for (int j = 0; j < LIGHT_SAMPLES; j++) {
            float tLight = (float(j) + 0.5) * dsLight;
            vec3 PLight = P + sunDir * tLight;
            float hLight = length(PLight) - planetRadius;
            if (hLight < 0.0) { blocked = true; break; }
            opticalDepthLightR += exp(-hLight / HR) * dsLight;
            opticalDepthLightM += exp(-hLight / HM) * dsLight;
        }

        if (!blocked) {
            vec3 tau = betaR * (opticalDepthR + opticalDepthLightR)
                     + vec3(betaM * 1.1) * (opticalDepthM + opticalDepthLightM);
            vec3 attenuation = exp(-tau);
            totalRayleigh += densityR * attenuation;
            totalMie += densityM * attenuation;
        }
    }

    float pR = phaseRayleigh(cosTheta);
    float pM = phaseMie(cosTheta, g);
    vec3 sunIntensity = vec3(20.0);
    return sunIntensity * (totalRayleigh * betaR * pR + totalMie * vec3(betaM) * pM);
}
```
