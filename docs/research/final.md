# Procedural Planet Generation — Complete Research Reference

_Compiled 2026-03-28 from 100+ sources across planetary science and computer graphics_

---

## Executive Summary

This document provides the complete reference material for building a GPU-accelerated procedural planet generator that produces physically plausible planets with biomes, atmosphere, oceans, ice, and up to 32K textures.

**Key findings:**
1. **Planet composition follows deterministic rules** based on distance from star, mass, and metallicity — enabling principled procedural generation rather than arbitrary noise
2. **Cube-to-sphere quadtree LOD** is the proven approach (SpaceEngine, Outerra) for planetary-scale rendering
3. **32K textures are feasible** in ~3 minutes on modern GPUs using tiled generation with BCn compression
4. **Atmospheric scattering** (Bruneton 2017) is a solved problem with open-source implementations
5. **Biome mapping** from Whittaker/Köppen diagrams provides realistic surface variety from just temperature + precipitation inputs

---

# PART I: PLANETARY SCIENCE REFERENCE

## 1. Planet Formation & Composition Rules

### 1.1 Frost Line — The Master Divider

The frost line determines whether volatiles condense into solids, fundamentally splitting planet types:

| Volatile | Condensation T | Distance (AU) | Effect |
|----------|---------------|----------------|--------|
| Silicates | ~1300-1500 K | 0.1-0.4 AU | Rock-forming |
| Iron/Nickel | ~1400 K | 0.1-0.3 AU | Metal cores |
| **H₂O (water)** | **150-170 K** | **2.7-3.2 AU** | **THE key boundary** |
| NH₃ (ammonia) | ~80 K | ~9 AU | Ice giant component |
| CO₂ | ~70 K | ~10 AU | Dry ice |
| CH₄ (methane) | ~30-31 K | ~30 AU | Outer ice component |
| CO | ~20-25 K | ~30-50 AU | Kuiper belt |
| N₂ | ~20-22 K | ~30-50+ AU | Outer volatiles |

Beyond the water frost line, solid surface density jumps **4×** (from ~7 to ~30 g/cm² in MMSN). This is why gas giants form beyond ~3 AU.

**Implementation:** Use frost line as the primary input for determining planet type and composition.

### 1.2 Planet Types by Distance

| Zone | Distance | Planet Type | Composition | Core | Atmosphere |
|------|----------|-------------|-------------|------|------------|
| Hot inner | < 0.5 AU | Hot rocky | Fe/Ni core + silicates | 30-70% mass | Thin/none (stripped by stellar wind) |
| Inner rocky | 0.5-1.5 AU | Terrestrial | Silicates + metals | 25-35% | Secondary (outgassed), N₂/CO₂/O₂ |
| **Frost line** | **~3 AU** | **Transition** | **Rock + ice available** | | |
| Outer rocky-icy | 3-10 AU | Ice/rock cores | Rock + water ice | Ice mantle | Primary capture begins |
| Gas giant | 5-30 AU | Gas giant | H₂/He envelope (80-87%) | 5-20 M⊕ rock/ice | H₂/He, NH₃, CH₄ clouds |
| Far outer | 30-50 AU | Ice giant | Ices 60-80% (H₂O, NH₃, CH₄) | Rock + ice | H₂/He (10-20%), CH₄-rich |
| Beyond | >50 AU | Kuiper belt | CO, N₂, CH₄ ices | Small bodies | Thin/surface volatiles |

**Earth bulk composition:** Fe 32.1%, O 30.1%, Si 15.1%, Mg 13.9%, S 2.9%, Ni 1.8%, Ca 1.1%, Al 1.1%

### 1.3 Key Formation Equations

**MMSN surface density:** Σ(r) = 1700 × (r/1 AU)^(-3/2) g/cm²

**Disk temperature:** T(r) = 280 × (r/1 AU)^(-1/2) K

**Scale height:** H/r ≈ 0.033 × (r/1 AU)^(1/4)

**Critical core mass:** M_crit ≈ 10 M⊕ (for gas accretion onset)

**Isolation mass (inner):** M_iso ≈ 0.11 M⊕ at 1 AU

**Isolation mass (beyond ice line):** M_iso ≈ 5-10 M⊕ at 5 AU

---

## 2. Geological Feature Reference

### 2.1 Tectonic Regimes

| Regime | Condition | Surface Feature | Example |
|--------|-----------|-----------------|---------|
| Plate tectonics | Rayleigh # > 10⁶, wet interior | Ridges, subduction, mountains | Earth |
| Stagnant lid | Rayleigh # < 10⁶ or dry | Volcanic provinces, no subduction | Mars, Venus |
| Episodic | Intermediate | Periodic resurfacing | Venus (hypothesized) |
| Ice tectonics | Ice shell over ocean | Fractures, ridges on ice | Europa |

**Rayleigh number:** Ra = αgΔTD³/(νκ)
- Earth mantle: Ra ~ 10⁸ (vigorous convection → plate tectonics)
- Critical Ra for convection onset: ~10³

**Implementation rule:** Planet mass > 0.5 M⊕ with water → likely plate tectonics. Small/dry planets → stagnant lid.

### 2.2 Volcanism Types

| Type | Profile | Height | Width | Eruption Style |
|------|---------|--------|-------|----------------|
| Shield | Low slope (<5°) | 5-10 km | 100+ km | Effusive basalt |
| Stratovolcano | Steep (30°) | 2-5 km | 10-30 km | Explosive |
| Caldera | Depression | -1 to -5 km | 5-50 km | Collapse after eruption |
| Flood basalt | Flat flows | 0.5-2 km | 100s km | Massive effusive |
| Cryovolcano | Dome/cone | 1-5 km | 10-50 km | Ice slush (icy moons) |

**Olympus Mons (Mars):** 21.9 km height, 600 km diameter — shield volcano on stagnant lid planet (no plate motion → volcano stays over hotspot indefinitely)

### 2.3 Impact Cratering

**Crater diameter vs impactor:** D_crater ≈ 20 × D_impactor (gravity regime)

**Depth/diameter ratio:** d/D ≈ 0.2 (simple craters), ~0.05 (complex craters > 3-5 km)

**Size-frequency distribution (production function):**
N(>D) ∝ D^(-b), where b ≈ 2.0-2.5

**Ejecta blanket:** Extends ~1-2 crater radii, blanket thickness ∝ r^(-3)

**Degradation:** Craters fade with geological activity. Heavily cratered = old/inactive surface.

**Implementation:** Place craters via Poisson disk sampling, scaled by surface age. Young surfaces → few craters, old → saturated.

### 2.4 Erosion Rates

| Process | Rate (mm/yr) | Signature |
|---------|-------------|-----------|
| Fluvial (rivers) | 0.01-10 | V-shaped valleys, meanders |
| Glacial | 0.001-100 | U-shaped valleys, moraines |
| Aeolian (wind) | 0.001-1 | Dunes, yardangs |
| Chemical weathering | 0.001-0.1 | Rounded terrain, soil formation |

**Stream power law:** E = K × A^m × S^n (E=erosion, A=drainage area, S=slope, K=erodibility)

---

## 3. Atmosphere Reference Tables

### 3.1 Composition by Planet Type

| Component | Earth-like | Venus-like | Mars-like | Gas Giant | Ice Giant |
|-----------|-----------|------------|-----------|-----------|-----------|
| N₂ | 78% | 3.5% | 95.3% | trace | trace |
| O₂ | 21% | 0% | 0.13% | 0% | 0% |
| CO₂ | 0.04% | 96.5% | 1.9% | trace | trace |
| H₂O | 0-4% | 30 ppm | 210 ppm | trace | trace |
| Ar | 0.93% | 70 ppm | 1.6% | 0% | 0% |
| H₂ | trace | trace | trace | 89.8% | 82.5% |
| He | 5 ppm | trace | trace | 10.2% | 15.2% |
| CH₄ | 1.8 ppm | 0 | 0 | 0.3% | 2.3% |
| NH₃ | trace | 0 | 0 | 0.026% | trace |
| H₂S | 0 | 0 | 0 | 0.007% | trace |
| Cloud type | H₂O | H₂SO₄ | Dust/CO₂ | NH₃, NH₄SH | CH₄ |

### 3.2 Temperature & Pressure

| Parameter | Earth | Venus | Mars | Jupiter | Neptune |
|-----------|-------|-------|------|---------|---------|
| Surface P (atm) | 1.0 | 92 | 0.006 | >1000 | >1000 |
| Surface T (K) | 288 | 737 | 210 | 165 (1 bar) | 72 (1 bar) |
| Lapse rate (K/km) | 6.5 | 7.7 | 4.5 | 2.0 | ~1.0 |
| Scale height (km) | 8.5 | 15.9 | 11.1 | 27 | 20-22 |
| Tilt (degrees) | 23.4 | 177.4 | 25.2 | 3.1 | 28.3 |

### 3.3 Atmospheric Circulation

**Hadley cell width:** Y_H ≈ (5 × R_T × a/3)^(1/2) where R_T = thermal Rossby number

**Rhines scale:** L_β = √(U/β) where β = df/dy (Coriolis gradient)

| Rotation Rate | Circulation Pattern | Climate Zones |
|---------------|-------------------|---------------|
| Slow (Venus) | Single Hadley cell pole-to-pole | Broad, gradual zones |
| Earth-like | 3 cells per hemisphere | 6 climate bands |
| Fast | Many narrow bands, strong jets | Many small zones (Jupiter) |

**Implementation:** Rotation rate determines number of circulation bands → affects biome distribution patterns.

---

## 4. Oceans & Ice Parameters

### 4.1 Ocean Formation Conditions

- **Source:** Outgassing from mantle (primary) + cometary delivery (minor)
- **Minimum conditions:** Surface T > 273 K AND partial pressure of water vapor allows liquid
- **Habitable zone (Earth-like):** 0.95-1.67 AU for Sun-like star

### 4.2 Ocean Composition (Earth)

| Ion | Concentration (g/kg) |
|-----|---------------------|
| Cl⁻ | 19.35 |
| Na⁺ | 10.76 |
| SO₄²⁻ | 2.71 |
| Mg²⁺ | 1.29 |
| Ca²⁺ | 0.41 |
| K⁺ | 0.39 |
| Salinity | ~35 g/kg (3.5%) |
| pH | ~8.1 |

### 4.3 Ice Distribution

| Ice Type | Volume (10⁶ km³) | Albedo | Notes |
|----------|-------------------|--------|-------|
| Antarctica | 26.5 | 0.80-0.87 | Largest land ice |
| Greenland | 2.9 | 0.75-0.85 | Second largest |
| Sea ice (Arctic) | seasonal | 0.50-0.70 | Seasonally varies |
| Subsurface (Europa) | ~100 km deep | N/A | Under ice shell |

**Tidal heating:** E_tidal = (21/2) × (k₂/Q) × (GM_p²/R⁵) × (M_s/M_p)² × (R/a)⁵ × e² × n
- Europa: ~0.1-1 W/m² — enough for subsurface ocean

**Axial tilt effect on ice:** Higher tilt → more extreme seasons → different ice distribution. Earth at 23.4° has ~15% permanent ice cover. At 54° tilt, ice caps migrate to equator during summer.

### 4.4 Milankovitch Cycles

| Parameter | Period (kyr) | Effect |
|-----------|-------------|--------|
| Obliquity | 41 | Tilt 22.1-24.5°, affects season severity |
| Eccentricity | 100, 405 | Orbit shape, insolation variation |
| Precession | 19, 23 | Season timing relative to distance |

**Implementation:** Use obliquity oscillation for ice age cycles in dynamic simulations.

---

## 5. Rotational & Orbital Effects

### 5.1 Axial Tilt Effects on Biomes

| Tilt | Climate Pattern | Biome Distribution |
|------|----------------|-------------------|
| 0° | No seasons, permanent equatorial heat | Equatorial desert, polar ice |
| 10-20° | Mild seasons | Slight latitudinal bands |
| **23.4° (Earth)** | **Distinct seasons** | **6 biome bands** |
| 40-54° | Extreme seasons | Ice at tropics in winter |
| 90° | Maximal seasons | Pole faces star directly in summer |
| >90° (retrograde) | Same as 180°-tilt | Venus-like (177.4° ≈ 2.6° prograde) |

### 5.2 Rotation Rate Effects

**Oblateness:** f = ω²R³/(2GM)
- Earth: f ≈ 1/298 (barely visible)
- Fast rotator (6h day): f ≈ 1/15 (noticeable ellipsoid)
- Jupiter (10h): f ≈ 1/15

**Coriolis → circulation bands:**
- Slow rotation: few wide bands, weak weather
- Fast rotation: many narrow bands, strong jet streams, intense storms

**Tidal locking timescale:** τ_lock ∝ a⁶ (scales with 6th power of distance!)

**Tidally locked planets (close orbit):**
- Permanent dayside: hot, possibly dry
- Permanent nightside: cold, ice-covered
- Terminator zone: potentially habitable ring
- Thick atmosphere can redistribute heat

### 5.3 Implementation Rules

```
if tidal_locking:
    temperature_map = dayside_heating * cos(angle_to_substellar) + redistribution_factor
    ice_distribution = nightside + high_latitude
else:
    temperature_map = latitude_based + seasonal_variation * sin(tilt)
    ice_distribution = latitude > ice_line_latitude
```

---

## 6. Surface Properties Database

### 6.1 Albedo Values by Material

| Surface Material | Albedo (visible) | Notes |
|-----------------|------------------|-------|
| Fresh snow | 0.80-0.90 | Highest common surface |
| Old/melting snow | 0.50-0.70 | Lower as it ages |
| Ice (glacier) | 0.30-0.60 | Depends on density/air bubbles |
| Sea ice | 0.50-0.70 | With snow cover higher |
| Ocean water | 0.06 | Very dark, sun glint exceptions |
| Sand (desert) | 0.30-0.40 | Sahara ~0.35 |
| Grassland | 0.20-0.25 | |
| Deciduous forest | 0.15-0.20 | |
| Coniferous forest | 0.10-0.15 | Darker |
| Tundra | 0.20-0.25 | |
| Wetland | 0.10-0.15 | |
| Bare rock | 0.10-0.30 | Granite higher, basalt lower |
| Basalt (dark) | 0.05-0.10 | Very dark volcanic rock |
| Limestone | 0.30-0.50 | Light colored |
| Red sandstone | 0.20-0.30 | |
| Urban/asphalt | 0.05-0.20 | |

**Planetary Bond albedos:** Venus 0.76, Earth 0.306, Mars 0.25, Jupiter 0.503, Neptune 0.290

### 6.2 Roughness Values by Material

| Material | PBR Roughness | Notes |
|----------|-------------|-------|
| Calm water | 0.0-0.05 | Near-mirror reflection |
| Ice surface | 0.05-0.15 | Smooth, some scattering |
| Wet sand | 0.3-0.5 | |
| Snow | 0.3-0.5 | Diffuse but not rough |
| Dry sand | 0.8-1.0 | Very rough |
| Grass/moss | 0.4-0.6 | |
| Tree bark | 0.7-0.9 | |
| Exposed rock | 0.6-0.9 | Varies with weathering |
| Fresh lava | 0.7-0.95 | Very rough, some glass |
| Regolith | 0.8-1.0 | Moon/Mars surface |

### 6.3 Spectral Colors (RGB Approximations)

| Material | R | G | B | Notes |
|----------|---|---|---|-------|
| Ocean deep | 10 | 30 | 80 | Depth-dependent |
| Ocean shallow | 20 | 80 | 140 | Over sand |
| Beach sand | 194 | 178 | 128 | Warm yellow |
| Desert sand | 210 | 180 | 140 | Tan |
| Grass | 86 | 130 | 50 | |
| Dense forest | 34 | 80 | 20 | |
| Snow | 240 | 245 | 255 | Slightly blue |
| Basalt | 50 | 45 | 40 | Dark volcanic |
| Granite | 160 | 155 | 150 | Light gray |
| Red sandstone | 180 | 100 | 60 | |
| Limestone | 210 | 200 | 180 | Cream |
| Mars regolith | 180 | 100 | 60 | Iron oxide red |
| Moon regolith | 130 | 125 | 120 | Gray |
| Lava (hot) | 200 | 60 | 10 | Molten glow |

---

## 7. Terrain Generation Parameters

### 7.1 Real Planet Power Spectra

| Body | Spectral exponent β | Fractal dimension D | Notes |
|------|-------------------|-------------------|-------|
| Earth | 2.0 | 2.5 | Well-characterized |
| Mars | 2.38 | 2.31 | Rougher at small scales |
| Venus | 1.47 | 2.76 | Smoother overall |
| Moon | 2.5 | 2.25 | Heavily cratered |
| Typical noise (p=0.5) | 2.0 | 2.5 | fBm with persistence 0.5 |

**D = (7-β)/2** (relationship between spectral exponent and fractal dimension)
**Hurst exponent:** H = (β-1)/2

### 7.2 Hypsometric Curves

| Body | Median Elevation | Range | Shape |
|------|-----------------|-------|-------|
| Earth | -2.5 km (bimodal) | -11 km to +8.8 km | Bimodal (ocean/continent) |
| Mars | +1.0 km | -8 km to +21.9 km | Unimodal with Tharsis bulge |
| Venus | +0.5 km | -2 km to +11 km | Unimodal, 60% within ±0.5 km |

**Earth's bimodal distribution** is key: ~70% ocean floor at ~-4 km, ~30% continents at ~+0.8 km. This is unique in the solar system and driven by plate tectonics + water.

**Implementation:** Use plate tectonics flag to decide unimodal vs bimodal elevation distribution.

### 7.3 Recommended Terrain Pipeline

```
1. Continental placement
   - Low-frequency Voronoi cells for continental boundaries
   - Or large-scale fBm with frequency ≈ 2-4

2. Base terrain
   - fBm with 8-12 octaves, lacunarity=2.0, persistence=0.5
   - Spectral exponent β ≈ 2.0 (Earth-like)

3. Mountain ridges
   - Ridge noise along plate boundaries (if plate tectonics)
   - Or isolated ridges from domain-warped noise

4. Hydraulic erosion
   - 20-50 GPU compute iterations
   - Creates realistic valleys, river networks

5. Coastal erosion
   - Smooth terrain near sea level

6. Crater placement
   - Poisson disk sampling
   - Size-frequency: N(>D) ∝ D^(-2)
   - Crater depth: d = 0.2 × D (simple), 0.05 × D (complex)
   - Ejecta blanket: ±1 radius

7. Volcanic features
   - Shield volcanoes at hotspots (low-frequency noise peaks)
   - Stratovolcanoes at plate boundaries

8. Ice carving (if applicable)
   - U-shaped valley modification above snow line
   - Fjord-like features near coasts
```

---

## 8. Equations & Formulas Quick Reference

### Formation
- MMSN surface density: Σ = 1700(r/AU)^(-3/2) g/cm²
- Disk temperature: T = 280(r/AU)^(-1/2) K
- Scale height: H/r = 0.033(r/AU)^(1/4)
- Critical core mass: M_crit ≈ 10 M⊕
- Toomre Q = c_sκ/(πGΣ), Q<1 → unstable

### Geology
- Rayleigh number: Ra = αgΔTD³/(νκ)
- Stream power: E = KA^mS^n
- Crater d/D: 0.2 (simple), 0.05 (complex)

### Atmosphere
- Lapse rate: Γ = -dT/dz (dry: 9.8 K/km, moist Earth avg: 6.5 K/km)
- Scale height: H = kT/(μg) (Earth: 8.5 km)
- Oblateness: f = ω²R³/(2GM)

### Tidal Locking
- τ_lock ∝ a⁶ × (Q/k₂) × (M_p/M_s)² × R³

### Scattering (CG)
- Rayleigh: σ ∝ λ⁻⁴ (blue sky)
- Mie: forward-peaked, Henyey-Greenstein phase function
- Bruneton LUTs: transmittance (256×64), scattering (256×128×32)

---

# PART II: TECHNICAL IMPLEMENTATION REFERENCE

## 9. GPU Architecture

### 9.1 Recommended Pipeline Architecture

```
┌──────────── PREVIEW MODE ─────────────┐
│ 256×256 noise → instant planet preview │
│ Show biome distribution, rough shape   │
│ User adjusts parameters iteratively    │
└──────────────────┬────────────────────┘
                   │ User approves params
                   ▼
┌──────────── GENERATION MODE ──────────┐
│ Async compute queue                   │
│ Generate tiles in batches             │
│ 6 faces × 64×64 tiles = 24,576 tiles  │
│ Each tile: 512² pixels                │
│ ~7ms per tile on RTX 3080-class GPU   │
│ Total: ~3 minutes for full 32K planet │
│ Write compressed tiles to disk        │
└──────────────────┬────────────────────┘
                   │ Complete
                   ▼
┌──────────── RUNTIME MODE ─────────────┐
│ Virtual texturing for streaming       │
│ Quadtree LOD for mesh (chunked LOD)   │
│ Only stream visible tiles             │
│ Atmospheric scattering (precomputed)  │
│ FFT ocean animation                   │
│ Day/night cycle                       │
└───────────────────────────────────────┘
```

### 9.2 Sphere Representation

**Recommended: Cube sphere with quadtree LOD**

Each of 6 cube faces → quadtree → chunks:

```glsl
// Cube to sphere (normalized cube map)
vec3 cubeToSphere(vec3 p) {
    return normalize(p);
}

// Sphere to cube (for UV lookup)
// GPU hardware cubemap sampling handles this natively
```

**Advantages:**
- Natural quadtree LOD per face
- Simple UV mapping
- GPU cubemap hardware support
- No pole pinching (unlike lat/lon)

**Distortion:** ~33% area variation at cube corners (acceptable for most applications)

### 9.3 Tile-Based 32K Generation

**Tiling scheme:** 32,768 / 512 = 64 tiles per axis per face

**Per-tile generation (compute shader):**
```glsl
// 1. Generate height
float height = fbm(position, 8); // 8 octaves

// 2. Erosion (iterated in separate passes)
// [hydraulic erosion: 20-50 iterations]

// 3. Compute normals from height
float3 normal = computeNormal(heightmap, x, y);

// 4. Determine biome
float temperature = baseTemp - lapseRate * height + tempNoise;
float moisture = moistureNoise + oceanProximity - rainShadow;
int biome = whittakerLookup(temperature, moisture);

// 5. Generate albedo from biome
float3 albedo = biomeColor(biome) + colorVariationNoise;

// 6. Generate roughness from biome
float roughness = biomeRoughness(biome) + roughnessNoise;

// 7. Output all maps
outHeight = height;
outNormal = normal;
outAlbedo = albedo;
outRoughness = roughness;
```

### 9.4 Memory Budget

| Map | Resolution | Format | Size |
|-----|-----------|--------|------|
| Albedo × 6 faces | 32768² | BC7 | 3.0 GB |
| Height × 6 faces | 32768² | R16 | 4.0 GB |
| Normal × 6 faces | 32768² | BC5 | 1.5 GB |
| Roughness × 6 faces | 32768² | BC4 | 0.75 GB |
| **Total (compressed)** | | | **~9.3 GB** |

On disk: same (compressed). In GPU memory at runtime: only visible tiles via virtual texturing (~256-512 MB).

---

## 10. Atmospheric Rendering

### 10.1 Bruneton Method (Recommended)

**Precompute (one-time, ~1s on GPU):**
1. Transmittance table (256×64 2D)
2. Scattering table (256×128×32 3D)
3. Irradiance table (64×16 2D)
4. Optional: multiple scattering (additional 3D)

**Parameters per planet type:**
```glsl
// Earth-like
vec3 rayleigh_beta = vec3(5.5e-6, 13.0e-6, 22.4e-6); // per meter
float mie_beta = 21e-6;
float mie_g = 0.758; // asymmetry

// Mars-like (thinner, dusty)
vec3 rayleigh_beta = vec3(1.0e-6, 2.5e-6, 4.5e-6); // ~1/5 Earth
float mie_beta = 50e-6; // much dustier
float mie_g = 0.85; // more forward-scattering

// Gas giant (thick, methane-rich)
vec3 rayleigh_beta = vec3(15e-6, 12e-6, 5e-6); // reversed: methane absorbs red
float mie_beta = 30e-6;
float mie_g = 0.80;
```

**Runtime:** 2-4 texture lookups per pixel → <0.5ms for full screen

**Source:** [Bruneton 2017 implementation](https://ebruneton.github.io/precomputed_atmospheric_scattering/)

### 10.2 Alternative: Hillaire (Epic Games, 2020)

- No high-dimensional LUTs
- Analytical multiple scattering approximation
- Better for mobile/lower-end
- Used in Unreal Engine HDRP

---

## 11. Ocean Rendering

### 11.1 Tessendorf FFT Waves

**Pipeline:**
1. Generate Phillips wave spectrum in frequency domain
2. Animate with time-dependent phase
3. GPU IFFT (256² or 512²) → height displacement
4. Compute normal maps from displacement gradients
5. Render with Fresnel reflection + SSS

**Phillips spectrum:** P(k) = A × exp(-1/(kL)²) / k⁴ × |k̂·ŵ|²  
where L = V²/g (wind speed → dominant wavelength)

**FFT compute cost:** ~0.5ms for 256² on modern GPU

### 11.2 Ocean Appearance

- **Fresnel effect:** Water reflective at grazing angles, transparent from above
- **Subsurface scattering:** Wrapped diffuse for translucent green-blue
- **Foam:** Generated at wave crests (height > threshold)
- **Depth coloring:** Exponential absorption → deeper = darker blue

---

## 12. Cloud Rendering

### 12.1 Procedural Cloud Maps

| Cloud Type | Noise | Altitude | Coverage |
|-----------|-------|----------|----------|
| Cirrus | Thin high-frequency | 6-12 km | Sparse |
| Stratus | Flat layered | 2-4 km | 50-80% |
| Cumulus | Worley/Voronoi | 2-8 km | 20-40% |
| Cumulonimbus | Tall Worley | 2-16 km | Small % |
| Cyclones | Domain-warped spiral | Multiple | Storm systems |

**Implementation:** Cloud layer as separate noise-based map, animated over time, rendered as translucent shell around planet.

---

## 13. Biome Mapping Implementation

### 13.1 Complete Whittaker Table

```
            | 0-10 | 10-25 | 25-50 | 50-100 | 100-150 | 150-200 | 200-300 | 300+ cm/yr
------------|------|-------|-------|--------|---------|---------|---------|----------
> 24°C      | DESERT | DESERT | THORN_SAVANNA | DRY_SAVANNA | WET_SAVANNA | TROPICAL_DF | TROPICAL_RF | TROPICAL_RF
20-24°C     | DESERT | DESERT | THORN_SAVANNA | SAVANNA | TROPICAL_DF | TROPICAL_RF | TROPICAL_RF | TROPICAL_RF
15-20°C     | DESERT | SEMIARID | GRASSLAND | WOODLAND | TEMPERATE_DF | TEMPERATE_RF | TEMPERATE_RF | TEMPERATE_RF
10-15°C     | DESERT | STEPPE | GRASSLAND | BOREAL | BOREAL | BOREAL | TEMPERATE_RF | TEMPERATE_RF
5-10°C      | TUNDRA | TUNDRA | TAIGA | TAIGA | TAIGA | TAIGA | TAIGA | TAIGA
0-5°C       | TUNDRA | TUNDRA | TUNDRA | TAIGA | TAIGA | TAIGA | TAIGA | TAIGA
< 0°C       | ICE | ICE | ICE | TUNDRA | TUNDRA | TUNDRA | TUNDRA | TUNDRA
```

### 13.2 Temperature & Moisture Generation

```glsl
// Temperature: latitude + elevation + variation
float baseTemp = 30.0 - abs(latitude) * 60.0; // 30°C equator, -30° poles
float temp = baseTemp - 6.5 * elevation_km + 5.0 * tempNoise; // lapse rate + noise

// Moisture: noise + ocean proximity + rain shadow
float baseMoisture = moistureNoise * 100.0; // 0-100 cm/yr base
float moisture = baseMoisture + oceanBonus(oceanDistance);
moisture *= rainShadowFactor(windDirection, mountains);
moisture = clamp(moisture, 0, 400);

int biome = whittakerLookup(temp, moisture);
```

### 13.3 Biome Color Palettes

Each biome has a base color ± noise variation (±10-15% per channel):

| Biome | Base RGB | Roughness | Height Offset |
|-------|----------|-----------|--------------|
| Desert | (210, 180, 140) | 0.85 | -20 to +50m |
| Grassland | (86, 130, 50) | 0.55 | ±10m |
| Savanna | (160, 150, 60) | 0.65 | ±20m |
| Tropical rainforest | (34, 80, 20) | 0.50 | ±30m |
| Temperate forest | (50, 100, 30) | 0.55 | ±50m |
| Boreal forest | (40, 70, 35) | 0.60 | ±30m |
| Tundra | (140, 160, 130) | 0.50 | ±10m |
| Taiga | (60, 80, 45) | 0.55 | ±20m |
| Ice/snow | (240, 245, 255) | 0.15 | ±5m |
| Beach/coast | (194, 178, 128) | 0.70 | Sea level |
| Mountains (rock) | (128, 128, 128) | 0.80 | > 2000m |
| Volcanic | (50, 45, 40) | 0.90 | Variable |

---

## 14. Existing Systems — Lessons Learned

| System | Key Takeaway | Apply to Our System |
|--------|-------------|-------------------|
| SpaceEngine | Quadtree LOD + multi-octave noise proven at scale | Use same architecture |
| Elite Dangerous | Physics-based generation from first principles | Use frost line + composition rules |
| No Man's Sky | Seed-based determinism + biome coloring | Seed everything; biome LUT coloring |
| Gaia Sky | GPU generation is fast enough for real-time | Generate on GPU, not CPU |
| Outerra | Virtual texturing scales to planet size | Use VT for runtime |

---

## 15. Recommended Tech Stack

| Component | Technology | Rationale |
|-----------|-----------|-----------|
| Graphics API | Vulkan or WebGPU | Compute shaders, modern features |
| Noise generation | Compute shaders (GLSL/HLSL/WGSL) | Parallel, fast |
| LOD system | Quadtree chunked LOD (6 cube faces) | Proven, simple, GPU-friendly |
| Textures | BCn compressed, tiled, virtual texturing | Memory efficient |
| Atmosphere | Bruneton 2017 precomputed scattering | Gold standard, open source |
| Ocean | Tessendorf FFT waves | Industry standard |
| Erosion | Virtual pipe model on GPU | Fast, realistic |
| File format | KTX2 or custom tiled | GPU-native, mipmapped |
| Preview | 256² per face, single compute dispatch | <10ms preview |

---

## 16. Open Questions & Future Research

1. **Plate tectonics simulation:** How to generate realistic plate boundaries procedurally? (Voronoi-based continent plates show promise)
2. **River networks:** Can hydraulic erosion alone generate convincing rivers, or do we need explicit drainage basin computation?
3. **Vegetation/placement:** Beyond biome coloring — how to place individual trees/rocks/objects on a 32K planet
4. **Ring systems:** Not covered — need research for gas giant rings
5. **Multi-star systems:** Binary/trinary star lighting
6. **Seasonal variation:** Dynamic snow/vegetation changes over time
7. **Interior rendering:** Cave systems (heightmaps can't do overhangs)

---

## Source Index

### Planetary Science
- Hayashi 1981 — MMSN model
- Pollack et al. 1996 — Giant planet formation phases
- Kokubo & Ida 1996/1998 — Runaway/oligarchic growth
- Ormel 2024 — Planet formation review (arXiv:2410.14430)
- Pontoppidan et al. — Frost lines (PPVI chapter)

### Computer Graphics
- Bruneton & Neyret 2008/2017 — Precomputed atmospheric scattering
- Hillaire 2020 — Scalable atmospheric scattering (EGSR)
- Tessendorf 2004 — Ocean simulation
- Mei et al. 2007 — GPU hydraulic erosion
- Michelic 2018 — Real-time procedural planets (CESCG)
- Willmott 2007 — Creating spherical worlds (Spore/Maxis)
- Tulrich — Chunked LOD

### Online Resources
- [Gaia Sky blog](https://tonisagrista.com/blog/2021/procedural-planetary-surfaces/)
- [acko.net Making Worlds](https://acko.net/blog/making-worlds-1-of-spheres-and-cubes/)
- [SpaceEngine documentation](https://spaceengine.org/)
- [NVIDIA GPU Gems](https://developer.nvidia.com/gpugems/)
- [80.lv GPU-driven terrain](https://80.lv/articles/gpu-driven-quadtree-terrain-mesh-rendering-inspired-by-far-cry-5)

---

_Next step: Design the application architecture based on this reference, then implement the GPU compute pipeline._