# Physically Accurate Planet Rendering: A Technical Implementation Guide

**Researcher:** Coder 6 (GLM-5-Turbo)  
**Date:** 2026-03-26

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
11. [References](#11-references)

---

## 1. Mathematical Foundation

### 1.1 Rendering Equation for Participating Media

The fundamental equation governing light transport through participating media (atmospheres, clouds) extends the surface rendering equation to account for volume interactions. For a point **x** with direction **ω**:

**L(x, ω) = L_{sun}(x, ω) + ∫_S f_r(x, ω_i, ω) L(x, ω_i) (n·ω_i) dω_i + ∫_Ω L_s(x, ω', ω) dω'**

Where:
- **L(x, ω)** is the radiance at point x in direction ω
- **L_{sun}(x, ω)** is the direct sun light (zero unless ω points to sun)
- The second integral is surface reflection
- **L_s(x, ω', ω)** is the in-scattered radiance from direction ω' into ω

### 1.2 Volume Rendering Integral

For a ray through participating media from point **a** to point **b**:

**L(a, ω) = T(a, b) L(b, ω) + ∫_a^b T(a, t) σ_s(t) ∫_Ω p(t, ω_i, ω) L(t, ω_i) dω_i dt**

Where:
- **T(a, b)** is the transmittance from a to b
- **σ_s(t)** is the scattering coefficient at point t
- **p(t, ω_i, ω)** is the scattering phase function
- The outer integral is along the ray path
- The inner integral is over all incoming directions (scattering integral)

### 1.3 Transmittance

Transmittance along a ray from point **a** to point **b**:

**T(a, b) = exp(-∫_a^b σ_t(s) ds)**

Where **σ_t = σ_s + σ_a** is the extinction coefficient (scattering + absorption).

### 1.4 Optical Depth

The optical depth (or optical thickness) **τ** is defined as:

**τ(a, b) = ∫_a^b σ_t(s) ds**

So transmittance simplifies to: **T(a, b) = exp(-τ(a, b))**

### 1.5 Phase Functions

#### Rayleigh Phase Function

For scattering by particles much smaller than the wavelength (gas molecules):

**p_R(θ) = 3 / (16π) · (1 + cos²θ)**

Where θ is the scattering angle (angle between incoming and outgoing directions).

#### Mie Phase Function (Henyey-Greenstein)

For scattering by particles comparable to or larger than the wavelength (aerosols, dust):

**p_M(θ) = (1 - g²) / (4π(1 + g² - 2g·cosθ)^(3/2))**

Where **g** is the asymmetry parameter:
- **g = 0**: isotropic scattering
- **g → 1**: strong forward scattering
- **g → -1**: strong backward scattering
- Typical Earth atmosphere: g ≈ 0.76

#### Double Henyey-Greenstein

To create a more realistic forward-scattering lobe with a wider distribution:

**p_DHG(θ) = α · p_HG(θ, g_1) + (1 - α) · p_HG(θ, g_2)**

Where α blends two HG functions with different asymmetry parameters. Common values: g₁ ≈ 0.8, g₂ ≈ -0.5, α ≈ 0.65.

#### Cornette-Shanks Phase Function

A more accurate approximation that avoids the HG singularity at θ = 0:

**p_CS(θ) = (3 / 8π) · (1 - g²)(1 + cos²θ) / (2 + g²) / (1 + g² - 2g·cosθ)^(3/2)**

### 1.6 Density Profiles

#### Exponential (Barometric) Profile

For well-mixed atmospheric gases:

**ρ(h) = ρ_0 · exp(-h / H)**

Where:
- **ρ_0** is the density at sea level (or reference altitude)
- **H** is the scale height
- **h** is the altitude above the reference

Earth parameters:
- Rayleigh scale height: H_R ≈ 8,500 m
- Mie scale height: H_M ≈ 1,200 m

#### Tent Function (Piecewise Linear)

Used in the Bruneton/Neyret model for ozone and aerosol density:

**ρ(h) = max(0, 1 - |h - h_0| / w)**

Where h₀ is the layer center and w is the half-width. This allows modeling the ozone layer peak around 20-30 km.

---

## 2. Atmospheric Scattering

### 2.1 Rayleigh Scattering

Rayleigh scattering is caused by particles much smaller than the wavelength of light (N₂, O₂ molecules). The key characteristic is its wavelength dependence.

#### Cross-Section

The Rayleigh scattering cross-section for a single molecule:

**σ_R(λ) = (8π³(n² - 1)²) / (3N²λ⁴) · ((6 + 3δ)/(6 - 7δ))**

Where:
- **n** is the refractive index of air (~1.000293 at sea level)
- **N** is the number density of molecules (~2.547 × 10²⁵ m⁻³ at sea level)
- **λ** is wavelength
- **δ** is the depolarization factor (~0.035 for air, accounts for anisotropy)

The λ⁻⁴ dependence causes blue light to scatter ~5.5× more than red light.

#### Practical Rayleigh Coefficients

For implementation, Bruneton provides these at sea level:

| Wavelength (nm) | σ_s (m⁻¹) |
|-----------------|-----------|
| 440 | 1.185 × 10⁻⁵ |
| 550 | 6.444 × 10⁻⁶ |
| 680 | 3.054 × 10⁻⁶ |

Or using the simplified formula from Preetham et al. (1999):

**β_R(λ) = (8π³(n² - 1)²) / (3Nλ⁴) · (6 + 3δ)/(6 - 7δ)**

In shader code, a common approach is to parameterize by wavelength as:

**β_R(λ) ∝ 1/λ⁴**

And store three coefficients (R, G, B channels) for computation.

#### Scale Height

The Rayleigh scattering coefficient at altitude h:

**β_R(h, λ) = β_R(0, λ) · exp(-h / H_R)**

With H_R ≈ 8,500 m for Earth.

### 2.2 Mie Scattering

Mie scattering is caused by aerosols (dust, pollen, water droplets, pollution). It is far less wavelength-dependent than Rayleigh.

#### Scattering Coefficient

**β_M(λ) = β_M^s · exp(-h / H_M)**

Where β_M^s is the Mie scattering coefficient at sea level (typically around 2.1 × 10⁻⁵ m⁻¹ for Earth), and H_M ≈ 1,200 m.

#### Phase Function

The Henyey-Greenstein phase function (see §1.5) with g ≈ 0.76 for Earth's aerosols creates strong forward scattering.

#### Combined Scattering

The combined phase function is:

**p(θ, λ) = (β_R(λ) · p_R(θ) + β_M(λ) · p_M(θ)) / (β_R(λ) + β_M(λ))**

### 2.3 Single vs Multiple Scattering

#### Single Scattering (SS)

Light scatters exactly once between the sun and the camera. This is the dominant visual contribution and is relatively inexpensive to compute. It accounts for the blue sky color and the basic sunset gradient.

For a camera ray through the atmosphere, single scattering at point **x**:

**L_{SS}(x, ω) = σ_s(x) · p(θ) · T(x_a, x) · T(x, x_s) · E_{sun} / (4π)**

Where x_a is the camera position, x_s is the point where the sun ray enters the atmosphere.

#### Multiple Scattering (MS)

Light undergoes two or more scattering events. This is critical for:
- Brightening of the horizon (aurora-like glow)
- Color bleeding (the sky near the horizon appears white rather than blue)
- Twilight colors after sunset
- Cloud illumination from below

**Approximation strategies:**

1. **Order-2 scattering**: Compute one additional bounce. Expensive but more accurate.
2. **LUT-based**: Precompute multiple scattering into lookup tables (Bruneton approach).
3. **Spherical harmonics**: Decompose the scattering integral into SH coefficients (Hillaire approach).
4. **Empirical scaling**: Multiply single scattering by a factor (quick hack, e.g., 1.2-2.0×).
5. **Powder effect** (Hillaire): Add a ground-albedo-dependent term that approximates the brightening from multiple bounces between ground and atmosphere: **L_{powder} = R_g · c · (1 - T(c,s)) · (1 - T(c,eye))**, where R_g is ground albedo and c is a tuning constant.

### 2.4 Optical Depth & Transmittance Calculations

For a ray from height **h** with zenith angle **θ** (measured from zenith), the optical depth through an atmosphere with exponential density profile and planet radius **R**:

For rays that don't intersect the planet surface:

**τ(h, θ) = H · ρ_0 · (σ_s + σ_a) / cos(θ_z) · [exp(-h / H) - exp(-(√(h² + 2hR·cos(θ_z) + R²) - R) / H)]**

Where θ_z is the zenith angle of the ray.

**Implementation**: Rather than computing this analytically in shaders, the standard approach is to precompute a **transmittance LUT** parameterized by (r, μ) where r is the distance from planet center and μ = cos(zenith angle of ray).

The transmittance function:

**T(r, μ) = exp(-τ(r, μ))**

This LUT is typically 256×64 or similar dimensions and is reused across all scattering calculations.

### 2.5 Ray Marching Through the Atmosphere

When direct computation isn't possible (e.g., for inhomogeneous atmospheres, cloud volumes, or procedural density), ray marching is used.

#### Algorithm

```
L = 0
transmittance = 1.0
for t in ray_steps:
    x = ray_origin + t * ray_direction
    density = sample_density(x)
    if (density > 0):
        // Sample light direction
        light_transmittance = compute_transmittance(x, sun_direction)
        scattering = density * phase_function(cos_scatter_angle)
        L += transmittance * scattering * light_transmittance * sun_irradiance
        transmittance *= exp(-density * step_size)
return L
```

#### Step Size Strategies

1. **Fixed step**: Simple, but wastes samples in low-density regions and under-samples high-density regions.
2. **Distance-based**: Smaller steps near the viewer, larger far away. **step = base_step + t * growth_factor**.
3. **Density-adaptive**: More steps where density is higher. Can use the derivative of the density function or a binary search refinement.
4. **Importance sampling**: Distribute steps proportional to the scattering contribution. Often based on the transmittance profile.

#### Typical Step Counts

- Real-time ground view: 8-16 steps for sky, 32-64 for clouds
- Space view (full planet): 16-32 steps
- Offline rendering: 256-1024+ steps

---

## 3. Atmospheric Layers & Composition

### 3.1 Earth Atmospheric Layers

| Layer | Altitude Range | Temperature Trend | Rendering Impact |
|-------|---------------|-------------------|-----------------|
| **Troposphere** | 0-12 km | Decreasing (6.5°C/km) | Weather, clouds, most aerosols, bulk of Rayleigh scattering |
| **Stratosphere** | 12-50 km | Increasing (ozone heating) | Ozone absorption (UV), jet streams, important for sunset colors |
| **Mesosphere** | 50-80 km | Decreasing | Meteor burns, noctilucent clouds, minimal scattering contribution |
| **Thermosphere** | 80-700+ km | Increasing (UV absorption) | Aurora, ionosphere; negligible for visible rendering |

For rendering, the troposphere and stratosphere are the most important. Above ~50 km, the atmosphere is thin enough to be negligible for most visual effects.

### 3.2 Ozone Layer

The ozone layer, centered around 20-30 km, absorbs strongly in the ultraviolet (Hartley band, <300 nm) and visible Chappuis band (450-750 nm, peak ~600 nm).

**Effect on rendering:** The Chappuis band absorption subtly modifies the blue-yellow balance, particularly at twilight when light passes through much more atmosphere. This creates the slightly warmer tones of the setting sun that pure Rayleigh/Mie scattering doesn't fully explain.

**Modeling approach (Bruneton):**
- Use a tent function density profile centered at ~25 km with half-width ~15 km
- Absorption cross-section at 550 nm: ~5 × 10⁻²⁵ m² (per molecule)
- Scale height: ~6-8 km

**In the Bruneton model**, ozone is included as an absorbing-only medium with density profile:

**ρ_{O₃}(h) = ρ_{O₃}^0 · max(0, 1 - |h - 25000| / 15000)**

### 3.3 Parameterizing Atmospheres for Different Planets

#### Earth-Like (N₂/O₂ atmosphere)

| Parameter | Value |
|-----------|-------|
| Planet radius | 6,371 km |
| Atmosphere height | ~100 km (effective ~60 km) |
| Rayleigh scale height | 8,500 m |
| Mie scale height | 1,200 m |
| Rayleigh β (680 nm) | 3.054 × 10⁻⁶ m⁻¹ |
| Mie β | 2.1 × 10⁻⁵ m⁻¹ |
| Mie g | 0.76 |
| Ground albedo | 0.1-0.4 |

#### Mars-Like (CO₂ atmosphere, thin)

| Parameter | Value |
|-----------|-------|
| Planet radius | 3,389 km |
| Surface pressure | ~0.6 kPa (~1/170 of Earth) |
| Rayleigh scale height | ~11,100 m |
| Much thinner atmosphere — Rayleigh scattering ~1% of Earth's |
| Strong dust scattering (Mie) with g ≈ 0.85-0.90 |
| Absorption by CO₂ and iron oxide dust |
| Surface color: reddish-orange (iron oxide) |

**Rendering note:** Mars skies appear butterscotch/tawny due to dust scattering dominating over Rayleigh. The low atmospheric density means limb effects are subtle.

#### Gas Giant (Jupiter-like)

| Parameter | Value |
|-----------|-------|
| Planet radius | 69,911 km |
| Atmosphere: H₂/He |
| Scale height | ~27 km (much larger than terrestrial) |
| Multiple cloud layers (NH₃, NH₄SH, H₂O) at different pressures |
| Strong banding (zonal winds) affects cloud distribution |
| Ammonia absorption bands |

#### Thin Atmosphere (Moon, Mercury)

| Parameter | Value |
|-----------|-------|
| Essentially no atmosphere |
| Only exosphere particles |
| No scattering to speak of |
| Surface rendering only |

---

## 4. Aerosols & Particulates

### 4.1 Types and Properties

| Type | Typical Size | Scattering Type | Key Effect |
|------|-------------|-----------------|------------|
| Dust (Saharan, Martian) | 0.1-10 μm | Mie | Red-brown haze, reduced visibility |
| Sea salt | 0.1-10 μm | Mie | Coastal haze |
| Sulfate aerosols | 0.1-1 μm | Mie | Volcanic haze, bluish-white |
| Smoke/soot | 0.01-1 μm | Mie + absorption | Brown/gray haze |
| Water vapor | molecule | Modified Rayleigh | Humidity haze |

### 4.2 Absorption vs Scattering Coefficients

The extinction coefficient splits into scattering and absorption:

**σ_t = σ_s + σ_a**

**Single scattering albedo: ω = σ_s / σ_t**

- Pure scattering (Rayleigh molecules): ω ≈ 1.0
- Dust: ω ≈ 0.92-0.98
- Smoke: ω ≈ 0.4-0.8 (significant absorption)
- Cloud droplets: ω ≈ 0.999 (nearly pure scattering)

In the rendering equation, only **σ_s** contributes to in-scattered light, while both contribute to extinction (transmittance reduction).

### 4.3 Height-Dependent Density Profiles

Different aerosol types have different vertical distributions:

#### Boundary Layer Dust (0-2 km)
**ρ(h) = ρ_0 · exp(-h / H_{dust})** with H_{dust} ≈ 1-2 km

#### Elevated Aerosol Layer (2-6 km)
Modeled with a tent function or Gaussian centered at the layer height.

#### Stratospheric Aerosol (15-25 km)
After volcanic eruptions (e.g., Pinatubo), sulfate aerosols form a persistent layer:

**ρ(h) = ρ_0 · exp(-((h - 20) / 5)²)** (Gaussian, peak at 20 km)

---

## 5. Cloud Rendering

### 5.1 Volumetric Cloud Modeling

#### Approaches

1. **Procedural noise-based** (e.g., Perlin/Worley FBM): Generate density fields from layered noise. Common in real-time (Horizon Zero Dawn, UE5).
2. **Pre-computed 3D textures**: Store voxelized cloud data from simulation or procedural generation.
3. **Hybrid**: 2D cloud textures with volumetric effects via analytical density profiles.

#### Density Generation (Real-Time)

A common approach combines multiple noise octaves:

**density(x) = detail_noise(x) · base_shape(x) - coverage_threshold**

Where:
- **base_shape**: Low-frequency Worley noise (large-scale cloud structure)
- **detail_noise**: Higher-frequency Perlin-Worley hybrid (edges and detail)
- **coverage_threshold**: Density below this is treated as clear sky

### 5.2 Cloud Layer Types

| Cloud Type | Altitude | Optical Properties | Rendering Notes |
|-----------|----------|-------------------|----------------|
| Cirrus/Cirrostratus | 5-13 km | Thin, ice crystals, forward-scattering (g ≈ 0.85) | Semi-transparent, wispy |
| Altostratus | 2-7 km | Moderate, mixed phase | Diffuse lighting |
| Stratus/Stratocumulus | 0-2 km | Thick, water droplets, g ≈ 0.85 | Nearly opaque, flat |
| Cumulus/Cumulonimbus | 0.5-16 km | Very thick, towering, self-shadowed | Complex volumetric |

### 5.3 Light Transport Through Clouds

#### Beer-Lambert Law (Single Scattering Approximation)

For a light ray traversing a cloud of optical depth τ:

**I = I_0 · exp(-τ)**

Where **τ = ∫ β_e(s) ds** along the ray path.

This works for thin clouds but fails for thick clouds where multiple scattering dominates.

#### Henyey-Greenstein Powder Effect

For multiple scattering within clouds, Hillaire (2016) uses the powder effect approximation:

**L_{MS} = (1 - T_{before}) · (1 - T_{after}) · albedo · phase_powder**

Where T_{before} and T_{after} are transmittances before and after the scattering point.

#### Silver Lining Effect

The bright edges of clouds when the sun is behind them are caused by strong forward scattering. The HG phase function with high g (~0.85) naturally produces this when combined with multiple scattering.

#### Energy Conservation & Darkening

Thick clouds appear dark on the unlit side. The **Pearl-Bracey effective transmittance** approximation can be used:

**T_{eff}(τ, g) ≈ (1 + g·τ)^{-1/g}**

This provides a smooth transition from Beer-Lambert at low τ to the diffusion regime at high τ.

### 5.4 Cloud Shadows

Cloud shadows are computed by tracing a ray from the surface point toward the sun through the cloud volume. If the optical depth along this ray exceeds a threshold, the surface point is in shadow.

**Implementation:**
1. Compute shadow ray from surface to sun.
2. March through cloud layer (typically 4-8 steps for performance).
3. Accumulate optical depth; if τ > shadow_threshold, apply shadow.
4. Optional: Soft shadow by modulating shadow intensity with τ.

---

## 6. Surface Rendering

### 6.1 BRDFs for Planetary Surfaces

#### Lambertian

The simplest and most common model for planetary surfaces:

**f_r(ω_i, ω_o) = ρ / π**

Where ρ is the surface albedo. Sufficient for most terrain when viewed from space.

#### Oren-Nayar

For rough, diffuse surfaces (better for regolith, soil):

**f_r(ω_i, ω_o) = (ρ / π) · (A + B · max(0, cos(φ_i - φ_o)) · sin(α) · tan(β))**

Where A and B are functions of surface roughness σ, and α, β are functions of the zenith angles.

#### Microfacet Models (Cook-Torrance, GGX)

For icy surfaces, water, or metallic regolith:

**f_r = D(h) · F(ω_o, h) · G(ω_i, ω_o, h) / (4 · (n·ω_i) · (n·ω_o))**

### 6.2 Specular Reflection for Oceans

Oceans create a distinct specular highlight (sun glint). For a rough ocean surface:

**L_{spec} = E_{sun} · F_R(ω_o, h) · D_{GGX}(h) · G_{GGX}(ω_i, ω_o) · (n·ω_i) / (n·ω_o)**

Where:
- **F_R** is the Fresnel reflectance (use Schlick approximation for real-time)
- Ocean roughness depends on wind speed
- **Normal mapping** or FFT-based wave simulation for surface normals

**From space:** Ocean specular reflection is visible as a distinct bright spot (glint). Earth's ocean albedo varies from ~0.03 (calm, nadir) to ~0.1 (rough, oblique) without specular.

### 6.3 Night-Side Rendering

#### City Lights
- Point light sources or emissive texture maps on the night side
- Only visible on cloud-free regions
- Typically textured from satellite imagery (DMSP/OLS or VIIRS data)

#### Thermal/Infrared Emission
For exoplanets or scientific visualization, the night side emits thermal radiation:

**L_{thermal} = ε · σ_{SB} · T^4 / π**

Where T is surface temperature and ε is emissivity (~0.9-0.95 for most surfaces).

#### Atmospheric Night Glow
Airglow and human-created light pollution produce a faint luminance on the night side atmosphere, visible from space.

### 6.4 Terrain Coloring & Albedo Maps

Surface albedo varies by:
- **Biome type**: forest (~0.1-0.15), desert (~0.3-0.4), snow (~0.8-0.9), ocean (~0.06)
- **Elevation**: Higher elevations tend toward snow/rock
- **Latitude**: Polar ice caps

For procedural planets, albedo can be generated from a combination of latitude-based biome assignment, elevation-based noise, and moisture maps.

---

## 7. Limb Effects

### 7.1 Limb Darkening/Brightening

**Limb darkening** occurs on the planet's surface disk: the edges appear darker than the center when the surface is directly illuminated. This is a geometric effect — the angle between the surface normal and the view direction increases toward the limb, reducing the apparent reflectance for Lambertian surfaces.

For a Lambertian surface, the intensity at the limb follows:

**I(μ) = I_0 · μ** (where μ = cos(emission angle))

**Limb brightening** can occur in the atmosphere at the limb because the path length through the atmosphere is much longer when viewing tangentially (see §7.3).

### 7.2 Atmospheric Glow at the Limb

When viewing a planet from space, the atmosphere creates a thin bright ring (limb glow) due to the greatly increased path length through scattering media when the line of sight is nearly tangent to the planet.

**Path length at limb:**
For a ray at altitude h grazing the planet (viewing angle tangent to surface):

**l ≈ √(2π R h)** (first-order approximation)

This can be hundreds of km even for a thin atmosphere, creating significant scattering.

### 7.3 Atmosphere Thickness vs Viewing Angle

The effective atmosphere thickness a viewer sees depends on the viewing angle **α** from the surface normal:

**l(α) = H / cos(α)** (for a plane-parallel approximation)

For the spherical case (camera at infinity, ray passes at altitude h above a planet of radius R):

**l ≈ √((R+h)² - R²) · ρ(h) / ρ(0)**

This is why the limb appears brighter — the geometric path through the scattering medium increases dramatically.

---

## 8. Light Transport

### 8.1 Sun as Directional Light

The sun subtends ~0.53° from Earth, so it's modeled as a directional light. For rendering:

**E_{sun} = E_{TOA} · T_{atm}(camera_pos, sun_dir)**

Where:
- **E_{TOA}** is total solar irradiance at top of atmosphere (~1361 W/m² for Earth)
- **T_{atm}** is atmospheric transmittance along the sun-camera path

The sun color is approximately a 5778 K blackbody, but atmospheric transmittance modifies this. A simplified sun color after atmospheric filtering: roughly (1.0, 0.95, 0.8) at midday, shifting toward (1.0, 0.5, 0.2) near sunset.

### 8.2 Phase Angle Effects

The phase angle **α** is the angle Sun-Planet-Observer. Key effects:

- **α ≈ 0° (full phase/opposition)**: Planet fully illuminated (like full moon)
- **α ≈ 90° (quarter phase)**: Half illuminated
- **α ≈ 180° (new phase/conjunction)**: Backlit — only atmosphere visible

Atmospheric backscattering makes planets slightly brighter near opposition (opposition surge / Seeliger effect).

### 8.3 Shadow Calculations

#### Self-Shadowing

Terrain self-shadows: computed by testing whether a shadow ray from the surface point toward the sun intersects the terrain. For procedural planets, can be done analytically or with shadow maps.

#### Eclipse Shadows

When a moon passes between the sun and a planet:
- **Umbra**: Complete shadow (no direct sun). Can compute geometrically as a cone.
- **Penumbra**: Partial shadow. Sun is partially occluded, creating a gradual shadow edge.

**Implementation:** Test the moon's position relative to the sun-planet line. The penumbra angle is: **θ_p = arctan((R_sun - R_moon) / d_sun)**.

### 8.4 Multiple Light Sources

For exoplanet scenarios with binary stars:

**L_total = Σ_i T_{atm}(x, ω_i) · E_{sun,i} · p(θ_i)**

Each star contributes independently (linear light addition). Need to handle different star temperatures and positions.

---

## 9. Implementation Approaches

### 9.1 Real-Time: Precomputed LUT Method (Bruneton & Neyret 2008)

The foundational real-time approach. Precomputes scattering into several lookup tables.

#### Transmittance LUT: T(r, μ)
- **r**: distance from planet center
- **μ**: cos(angle between zenith and ray direction)
- **Resolution**: 256 × 64 (or 128 × 32 for lower quality)
- **Precomputation**: Numerical integration of the exponential density profile along the ray

#### Inscattering LUT: S(r, μ, μ_s, ν)
- **r**: camera altitude
- **μ**: cos(zenith angle of view ray)
- **μ_s**: cos(zenith angle of sun)
- **ν**: cos(azimuth angle between view and sun)
- **Resolution**: 128 × 32 × 32 × 8 (packed into a 3D or 2D texture atlas)
- **Contains**: Precomputed single + multiple scattering, with ground reflection

#### Irradiance LUT: E(r, μ_s)
- **r**: distance from planet center
- **μ_s**: cos(zenith angle of sun)
- **Resolution**: 64 × 32
- **Contains**: Precomputed diffuse ground irradiance

**Reference implementation**: https://ebruneton.github.io/precomputed_atmospheric_scattering/ (2017 update)

**Memory**: ~2-4 MB total for all LUTs. Precomputation takes ~10 seconds on GPU.

### 9.2 Real-Time: Hillaire 2020 Method

Eric Hillaire's approach, used in Unreal Engine 5's Sky Atmosphere component.

#### Key Innovations

1. **Spherical harmonics for multiple scattering**: Rather than a 4D LUT, decomposes the multiple scattering into spherical harmonic coefficients, reducing storage and enabling better quality.

2. **Separate single/multiple scattering**: Single scattering computed analytically per-pixel; multiple scattering from LUTs.

3. **Powder effect**: Simple analytical approximation for ground-atmosphere multiple bouncing.

4. **Transmittance LUT**: Similar to Bruneton's, parameterized by (r, μ).

5. **Multi-scattering LUT**: 2D texture parameterized by (r, μ_s), containing pre-integrated multiple scattering using SH.

**Paper**: "Real-Time Atmospheric Scattering" (EGSR 2020 / SIGGRAPH 2020 Advances)  
**Reference**: https://sebh.github.io/publications/  
**UE5 Source**: Available in Unreal Engine 5 under Engine/Plugins/SkyAtmosphere/

### 9.3 Real-Time: Preetham et al. (1999)

The classic analytical sky model. Not physically-based from space view, but works well for ground-level sky rendering.

**Key equations:**
- Sky color as function of zenith angle and sun position
- Analytical formulae for turbidity, zenith luminance, chromaticity
- Very fast (fully analytical, no LUTs)

**Limitations**: Ground-view only, no view from space, no multiple scattering, single-scattering Rayleigh+Mie approximation.

**Paper**: "A Practical Analytical Model for Daylight" (SIGGRAPH 1999)

### 9.4 Offline / Path-Traced Approaches

For film-quality or scientific accuracy:

1. **Monte Carlo path tracing** through participating media: Sample scattering events along rays using Woodcock tracking or exponential transmittance sampling.
2. **Mitsuba renderer**: Supports volumetric path tracing with physically-based atmospheres.
3. **Arnold, RenderMan, V-Ray**: All support volumetric atmospheric scattering.
4. **OSPRay**: Open-source, supports volume rendering.

**Key advantage**: No precomputation artifacts, handles heterogeneous media naturally.
**Key disadvantage**: Minutes to hours per frame.

### 9.5 GPU Shader Considerations

#### Performance Tips

1. **Early-out**: Skip atmosphere rendering for pixels that don't intersect the atmosphere (space background).
2. **Low-res atmosphere**: Render atmosphere at half or quarter resolution, upscale.
3. **Aerial perspective as post-process**: For ground-view, compute atmosphere only in screen-space.
4. **Temporal reprojection**: Reuse previous frame's scattering with temporal filtering.
5. **Half precision**: FP16 is sufficient for most atmosphere LUT lookups.

#### Coordinate Systems

Use a coordinate system centered on the planet:
- Camera position in world space (relative to planet center)
- Convert to (r, μ) for LUT lookups where:
  - r = length(camera_position)
  - μ = dot(normalize(camera_position), ray_direction)

### 9.6 Precomputation Strategies Summary

| LUT | Dimensions | Resolution | Purpose |
|-----|-----------|------------|---------|
| Transmittance | (r, μ) | 256×64 | How much light passes through atmosphere along a ray |
| Single Scattering | (r, μ, μ_s, ν) | 128×32×32×8 | In-scattered light, single bounce |
| Multi-Scattering | (r, μ_s) | 64×32 | Integrated multiple scattering (SH-based) |
| Irradiance | (r, μ_s) | 64×32 | Ground illumination from sky light |

---

## 10. Gas Giant & Exotic Planet Considerations

### 10.1 Band Structure in Gas Giant Atmospheres

Jupiter and Saturn exhibit alternating bands (zones and belts) caused by differential rotation and convective cells:

- **Zones**: Rising air, high-altitude ammonia clouds, bright
- **Belts**: Sinking air, lower clouds, darker, warmer

**Rendering approach:**
- Texture the cloud layer with latitude-dependent band patterns
- Different cloud densities and colors per band
- Bands have slightly different altitudes, creating shadow effects at boundaries
- Turbulent eddies (vortices) at band interfaces

### 10.2 Atmospheric Chemistry

Gas giant atmospheres have wavelength-dependent absorption bands:

| Gas | Absorption Bands | Visual Effect |
|-----|-----------------|---------------|
| CH₄ (methane) | Strong in red/NIR (~600-900 nm) | Imparts blue-green color (Uranus, Neptune) |
| NH₃ (ammonia) | NIR, some visible | Affects cloud reflectance |
| H₂/He | Very weak absorption | Minimal visual effect |
| H₂O | Various bands | Affects deeper cloud layers |

**Jupiter**: NH₃ clouds at ~0.5 bar level, with deeper NH₄SH clouds at ~2 bar and H₂O clouds at ~5 bar.

**Neptune/Uranus**: Strong methane absorption of red light creates the blue color. To render this, apply a wavelength-dependent absorption that preferentially attenuates red wavelengths with increasing path length.

### 10.3 Metallic Hydrogen Layers

Below ~1 Mbar pressure in Jupiter, hydrogen transitions to a metallic state. This is below the visible cloud layers and doesn't directly affect rendering, but could be relevant for cutaway visualizations.

### 10.4 Hot Jupiter Thermal Emission

Hot Jupiters (close-in gas giants) have equilibrium temperatures of 1000-4000 K. They emit significant thermal radiation:

- **Day side**: Glows visibly in red/infrared; some emit enough visible light to be detected
- **Night side**: Still very hot (1500-2500 K for very close orbits)
- **Phase curve**: Brightness varies with phase angle due to uneven heat distribution

**Rendering**: Add a thermal emission term to the surface/upper atmosphere:

**L_thermal = ε · B_λ(T)** where B_λ is the Planck function and T varies across the surface.

### 10.5 Ring Shadows and Ring Scattering

#### Ring Shadows on Planets

Rings cast shadows on the planet surface. For a ring system with inner radius r₁, outer radius r₂, and inclination i:

1. Compute the ring's shadow projection onto the planet surface
2. The shadow is a band of latitude on the planet (for aligned rings)
3. Optical depth of the ring determines shadow opacity (B ring τ ≈ 0.5-2.0, C ring τ ≈ 0.1)

#### Ring Rendering

- **Translucent**: Rings are semi-transparent, requiring alpha blending over the planet
- **Multiple scattering within rings**: Can be approximated with a modified Beer-Lambert law
- **Ring particle phase function**: Strong forward and backward scattering peaks
- **Oppposition effect**: Rings brighten significantly at opposition (zero phase angle)

**Implementation:**
- Render rings as a flat disk with particle density varying with radius
- Apply ring shadow texture to the planet
- Handle planet-occluded portions of rings (front vs back)

---

## 11. References

### Key Papers

1. **Nishita, T., Sirai, T., Tadamura, K., & Nakamae, E. (1993).** "Display of the Earth Taking into Account Atmospheric Scattering." *SIGGRAPH '93*.  
   https://doi.org/10.1145/166117.166151 — Pioneering work on atmospheric scattering from space views.

2. **Preetham, A. J., Shirley, P., & Smits, B. (1999).** "A Practical Analytical Model for Daylight." *SIGGRAPH '99*.  
   https://doi.org/10.1145/311535.311545 — Classic analytical sky model for ground views.

3. **Riley, K., & McGuire, M. (2018).** "Rendering GDC 2018: Multi-Layer Space Battles."  
   https://media.steampowered.com/apps/valve/2018/ValveRenderingGDC2018.pdf — Practical multi-layer planet rendering.

4. **Bruneton, E., & Neyret, F. (2008).** "Precomputed Atmospheric Scattering." *Computer Graphics Forum (Eurographics)*.  
   https://doi.org/10.1111/j.1467-8659.2008.01245.x — Foundational LUT-based real-time approach.  
   **Code & Demo**: https://ebruneton.github.io/precomputed_atmospheric_scattering/

5. **Hillaire, S. (2020).** "Real-Time Atmospheric Scattering in Screen Space." *EGSR / SIGGRAPH 2020 Advances*.  
   https://sebh.github.io/publications/ — Spherical harmonics approach, used in UE5.

6. **Hillaire, S. (2015).** "Physically Based Sky, Atmosphere and Cloud Rendering in Frostbite." *SIGGRAPH 2015*.  
   — Early real-time multi-scattering cloud rendering.

7. **Bouthors, A., et al. (2008).** "Interactive Multiple Anisotropic Scattering in Clouds." *SIGGRAPH '08*.  
   — Multiple scattering in volumetric clouds.

8. **Wrenninge, M. (2015).** "Production Volume Rendering." *Siggraph Course*.  
   — Comprehensive volumetric rendering for film.

9. **Kettig, T. (2024).** "Planet Rendering" blog series.  
   https://www.trist.am/blog/2024/atmosphere-rendering/ — Excellent modern overview.

### Open-Source Implementations

1. **Bruneton's Atmospheric Scattering** (C++/OpenGL): https://github.com/ebruneton/precomputed_atmospheric_scattering
2. **SkyAtmosphere in UE5**: Part of Unreal Engine 5 source (Engine/Plugins/SkyAtmosphere/)
3. **Three.js Atmosphere Shader**: Various examples on ShaderToy (search "atmospheric scattering")
4. **Atmospheric Scattering in Shadertoy**: https://www.shadertoy.com/results?query=atmosphere
5. **Sebastian Lague's Planet Generation** (Unity): YouTube series + GitHub — procedural planet rendering
6. **Gaia Sky** (Java/JOGL): Open-source planetarium software with atmospheric scattering

### Books

1. **Pharr, M., Jakob, W., & Humphreys, G.** "Physically Based Rendering: From Theory to Implementation" (4th ed., 2023). — Chapters on volume rendering and participating media.
2. **Dutré, P., et al.** "Advanced Global Illumination" (2nd ed., 2006). — Mathematical foundations.

---

*Document compiled from established academic literature, open-source implementations, and industry practices. Researcher: Coder 6 (GLM-5-Turbo), 2026-03-26.*
