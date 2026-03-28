# PCO Habitable Planet Atmosphere Presets
## Physical Celestial Objects — Blender Add-on Reference

Last updated: 2026-03-27  
Consolidated from 2-researcher + critic pipeline.

---

## Table of Contents

1. [PCO Layer Physics Reference](#1-pco-layer-physics-reference)
2. [Habitability & Planet Size Guide](#2-habitability--planet-size-guide)
3. [Atmosphere Height (PCO Radius Parameter)](#3-atmosphere-height-pco-radius-parameter)
4. [Scattering Coefficient Reference](#4-scattering-coefficient-reference)
5. [Aurora & Airglow Reference](#5-aurora--airglow-reference)
6. [Complete Planet Presets (9 Types)](#6-complete-planet-presets)
7. [Star-Type Sky Color Adjustment](#7-star-type-sky-color-adjustment)
8. [Quick-Start Cheat Sheet](#8-quick-start-cheat-sheet)
9. [References](#9-references)

---

## 1. PCO Layer Physics Reference

PCO gives you 6 atmosphere layers per planet. Each layer maps to a real atmospheric phenomenon:

### Layer 1: Rayleigh Troposphere
The main atmosphere. Molecular (Rayleigh) scattering gives the sky its color. Short wavelengths scatter more (λ⁻⁴), so blue light dominates the overhead sky. At sunrise/sunset, light passes through more atmosphere → blue is scattered away, red/orange remains.

- **Scattering Coefficient RGB** controls sky color (higher B = bluer sky)
- **Density Exponent** controls how quickly atmosphere thins with altitude
- Should be the densest layer (highest Density value)

### Layer 2: Mie Aerosol/Haze
Particulate scattering from dust, pollution, sea salt, etc. Unlike Rayleigh, Mie scatters more uniformly across wavelengths (forward-scattering dominant for particles ~wavelength size). This is what makes hazy/dusty skies look whitish.

- **Scattering Coefficient RGB** should be warm/neutral (dust = orange, water = white)
- **Start/End Level** should be near surface (0.0 → 0.10–0.20)
- **Density** typically 0.2–0.5 (lower than main atmosphere)

### Layer 3: Ozone Layer
Stratospheric ozone absorbs UV (Hartley band) and visible green (Chappuis band, 440–700 nm peak). The green absorption is what makes sunsets/limbs reddish — green gets eaten, red and blue pass through.

- **Absorption = 1.0, Scattering = 0.0** (this layer absorbs, doesn't scatter)
- **Scattering Coefficient RGB** encodes absorption: higher values in absorbing wavelengths
- Sits at 15–35 km altitude (~0.10–0.23 of atmosphere height)
- **Center of Density** = 0.5 (peaks in middle of its range)

### Layer 4: Clouds
Water/ice clouds scatter all wavelengths roughly equally → white. Clouds are Mie scatterers with very high single-scatter albedo (~0.99).

- **Scattering Coefficient = (1.0, 1.0, 1.0)** — pure white
- **Absorption = 0.03–0.06** (very low — clouds are reflective, not dark)
- **Center of Density** = 0.3–0.5 (clouds peak in middle-lower troposphere)
- **Density** = 1.5–3.0 (much denser than clear atmosphere)
- **Layer Density** = Value (constant coverage) or Texture (cloud map)

### Layer 5: Aurora
Emission layer from charged particles hitting atmospheric gases along magnetic field lines. Uses PCO's Emission parameter (layer glows in Scattering Coefficient color). Set **Layer Density = Aurora** for procedural aurora texture.

- **Emission** = 0.8–4.0 (higher = brighter glow)
- **Scattering/Absorption = 0** (it emits light, doesn't scatter/absorb)
- High altitude: Start Level 0.65, End Level 1.0
- **Layer Density = Aurora** (procedural mode)

### Layer 6: Airglow
Very faint photochemical emission in the mesosphere/thermosphere (85–300 km). Caused by recombination of atoms dissociated by UV during the day. Barely visible from surface (1/10 of dark sky brightness).

- **Emission** = 0.05–0.15 (very subtle)
- **Scattering/Absorption = 0**
- **Layer Density = Value** (uniform, very thin)
- High altitude, near-transparent

### Parameter Ranges

| Parameter | Valid Range | Notes |
|-----------|-------------|-------|
| Visible | on/off | Toggle |
| Start Level | 0.0 – 1.0 | 0 = planet surface, 1 = atmosphere max |
| End Level | 0.0 – 1.0 | Must be > Start Level |
| Center of Density | 0.0 – 1.0 | 0 = starts at Start Level |
| Density Exponent | 0.5 – 8.0 | Higher = more concentrated near peak |
| Scattering | 0.0 – 1.0 | 0 = no scattering, 1 = full |
| Absorption | 0.0 – 1.0 | Usually matches Scattering |
| Emission | 0.0 – 10.0+ | 0 = no glow, higher = brighter |
| Scattering Coefficient | RGB 0.0–1.0 | Color of scattered/absorbed/emitted light |
| Density | 0.0 – 5.0+ | Multiplier for layer density |
| Noise Jitter | 0.0 – 1.0 | Smoothing |

---

## 2. Habitability & Planet Size Guide

### Surface Gravity Formula

```
g = G × M / R²
G = 6.674 × 10⁻¹¹ m³ kg⁻¹ s⁻²
```

For a rocky planet: `M = (4/3)πR³ρ` → `g = (4/3)πGρR`

### Comfortable Gravity Range for Humans/Humanoids

| Gravity | Comfort | Notes |
|---------|---------|-------|
| < 0.5g | Problematic | Bone density loss, muscle atrophy |
| 0.5–0.7g | Manageable | Long-term adaptation needed |
| **0.7–1.3g** | **Comfortable** | **Sweet spot for humanoid life** |
| 1.3–1.5g | Tolerable | Joint strain, fatigue |
| > 1.5g | Difficult | Serious health impacts |

### Planet Radius by Density & Gravity

| Density (kg/m³) | 0.5g | 0.7g | 1.0g | 1.3g | 1.5g |
|-----------------|------|------|------|------|------|
| **4000** (low-density rocky) | 4,400 | 6,160 | 8,800 | 11,440 | 13,200 |
| **4500** (Mars-like) | 3,910 | 5,475 | 7,820 | 10,165 | 11,730 |
| **5000** (Mercury-like) | 3,520 | 4,928 | 7,040 | 9,152 | 10,560 |
| **5515** (Earth) | 3,185 | 4,460 | 6,371 | 8,282 | 9,556 |

### PCO Radius Values (in meters)

| Planet Type | Radius (m) | Gravity | Density |
|-------------|-----------|---------|---------|
| Earth-analog | 6,371,000 | 1.0g | 5515 |
| Super-Earth | 8,500,000 | 1.78g | 5515 |
| Terraformed Mars | 3,389,000 | 0.38g | 3930 |
| High-O₂ Carboniferous | 6,500,000 | 1.04g | 5515 |
| Ocean World | 6,000,000 | 0.87g | 5300 |
| K-dwarf orbiter | 6,371,000 | 1.0g | 5515 |
| M-dwarf orbiter | 5,500,000 | 0.71g | 5515 |
| High-pressure N₂/CO₂ | 7,000,000 | 1.20g | 5515 |
| Cold NH₃/N₂ world | 6,200,000 | 0.93g | 5300 |

---

## 3. Atmosphere Height (PCO Radius Parameter)

### Scale Height Formula

```
H = kT / (mg)
k = 1.381 × 10⁻²³ J/K  (Boltzmann constant)
m = mean molecular mass (kg)
g = surface gravity (m/s²)
T = surface temperature (K)
```

Atmosphere pressure drops by factor e (2.718) per scale height.

### Visible Atmosphere Radius for PCO

The visible atmosphere (what you see as the limb glow) extends roughly 8–12 scale heights above the surface. Beyond that, density is negligible.

```
PCO Atmosphere Radius ≈ Planet Radius + (10 × Scale Height)
```

| Planet Type | T (K) | Mean m (amu) | g (m/s²) | H (km) | 10H (km) | Atmosphere Radius (m) |
|-------------|-------|--------------|----------|--------|----------|----------------------|
| Earth-analog | 288 | 28.97 | 9.81 | 8.4 | 84 | **6,455,000** |
| Super-Earth | 290 | 28.97 | 17.4 | 4.7 | 47 | **8,547,000** |
| Terraformed Mars | 220 | 35.0 | 3.72 | 13.9 | 139 | **3,528,000** |
| High-O₂ | 295 | 29.8 | 10.2 | 8.0 | 80 | **6,580,000** |
| Ocean World | 300 | 28.0 | 8.6 | 10.1 | 101 | **6,101,000** |
| K-dwarf orbiter | 280 | 28.97 | 9.81 | 8.2 | 82 | **6,453,000** |
| M-dwarf orbiter | 250 | 28.97 | 7.0 | 9.5 | 95 | **5,595,000** |
| High-pressure | 310 | 32.0 | 12.0 | 6.2 | 62 | **7,062,000** |
| Cold NH₃/N₂ | 243 | 28.0 | 9.2 | 7.5 | 75 | **6,275,000** |

---

## 4. Scattering Coefficient Reference

### 4.1 Rayleigh Scattering RGB (λ⁻⁴ normalized)

Rayleigh scattering intensity ∝ λ⁻⁴. Using R=650nm, G=550nm, B=450nm:

| Composition | R (650nm) | G (550nm) | B (450nm) | Sky Appearance |
|-------------|-----------|-----------|-----------|----------------|
| **N₂/O₂ (Earth)** | 0.23 | 0.45 | 1.00 | Blue |
| **CO₂-rich** | 0.26 | 0.48 | 1.00 | Blue, slightly warmer |
| **O₂-rich (35%)** | 0.20 | 0.40 | 1.00 | Deeper blue/violet |
| **H₂O vapor** | 0.35 | 0.55 | 1.00 | Paler, whiter blue |
| **NH₃/N₂** | 0.40 | 0.30 | 1.00 | Purplish |
| **N₂/CO₂ mix** | 0.25 | 0.47 | 1.00 | Standard blue |

### 4.2 Mie Aerosol Scattering RGB

Aerosols scatter more uniformly (forward-biased but less wavelength-dependent):

| Aerosol Type | RGB | Notes |
|-------------|-----|-------|
| **Desert dust** | (0.95, 0.80, 0.55) | Orange-brown haze |
| **Sea salt / maritime** | (0.85, 0.88, 0.90) | White-grey, clean |
| **Volcanic SO₂** | (0.90, 0.85, 0.60) | Yellowish haze |
| **Soot / pollution** | (0.60, 0.55, 0.50) | Grey-brown |
| **Biogenic (forest)** | (0.70, 0.80, 0.85) | Blue-grey haze |
| **NH₃ ice crystals** | (0.75, 0.65, 0.90) | Purple haze |
| **High-pressure haze** | (0.92, 0.90, 0.88) | Pale grey, nearly white |

### 4.3 Ozone Chappuis Band Absorption

Ozone absorbs primarily green light (550–650 nm). In PCO, encode as Scattering Coefficient where this layer *absorbs*:

| Parameter | Value | Notes |
|-----------|-------|-------|
| Scattering | 0.0 | Ozone absorbs, not scatters |
| Absorption | 1.0 | Full absorption |
| Scatter RGB | (0.80, 0.30, 0.90) | Absorbs green most, passes red/blue |

### 4.4 Cloud Scattering

Clouds scatter all wavelengths → white. Very low absorption.

| Parameter | Value |
|-----------|-------|
| Scattering | 1.0 |
| Absorption | 0.04 |
| Scatter RGB | (1.0, 1.0, 1.0) |

---

## 5. Aurora & Airglow Reference

### 5.1 Aurora Emission Lines

| Gas | Altitude (km) | Wavelength (nm) | Color | PCO Scatter RGB | Emission Strength |
|-----|--------------|-----------------|-------|-----------------|-------------------|
| O (atomic) | 100–150 | 557.7 | Green | (0.20, 1.00, 0.20) | 1.0–2.0 |
| O (atomic) | 200+ | 630.0 | Red | (1.00, 0.20, 0.10) | 0.8–1.5 |
| N₂ | 80–120 | 427.8 / 391.4 | Blue-purple | (0.30, 0.10, 1.00) | 0.8–1.5 |
| N₂⁺ | 90–130 | 427.8 | Blue | (0.15, 0.15, 1.00) | 0.6–1.2 |
| CO₂⁺ | 80–100 | 289.0 / 337.0 | UV/Purple | (0.60, 0.00, 1.00) | 0.5–1.0 |
| H (proton) | 110+ | 656.3 (Hα) | Red-pink | (1.00, 0.30, 0.40) | 0.5–0.8 |

**Multi-color aurora (Earth-normal):** Use combined RGB (0.25, 0.80, 0.40), Emission 1.0–2.0

### 5.2 Aurora Strength by World Type

| World Type | Emission Value | Reason |
|------------|---------------|--------|
| Earth-analog | 1.0–2.0 | Moderate field + solar wind |
| M-dwarf orbiter | 2.0–4.0 | Strong stellar wind, intense aurora |
| Super-Earth | 2.0–3.0 | Likely strong magnetic field |
| Terraformed Mars | 0.3–0.5 | Weak/no intrinsic field, thin atmo |
| High-pressure world | 0.8–1.5 | Denser atmo = more particles to excite |

### 5.3 Airglow

| Emission Source | Altitude (km) | Wavelength | Color | PCO RGB | Emission |
|----------------|--------------|------------|-------|---------|----------|
| OI (green line) | 95–100 | 557.7 nm | Green | (0.15, 1.00, 0.20) | 0.08–0.15 |
| Na D-line | 85–95 | 589 nm | Yellow-orange | (1.00, 0.85, 0.10) | 0.03–0.08 |
| OI (red line) | 150–300 | 630.0 nm | Red | (1.00, 0.15, 0.05) | 0.02–0.05 |
| OH Meinel | 80–105 | 1.5–4.0 µm | IR (invisible) | N/A | N/A |

**Recommended PCO airglow layer:**  
RGB = (0.15, 1.00, 0.20) — OI green dominant  
Emission = 0.08–0.15  
Very thin, barely visible from surface

---

## 6. Complete Planet Presets

### 6.1 Earth-Analog (N₂/O₂, G-dwarf)

**Planet Radius:** 6,371,000 m  
**Atmosphere Radius:** 6,455,000 m  
**Sky color:** Blue  
**Notes:** Baseline reference. 78% N₂, 21% O₂, 1% Ar.

| Parameter | L1: Rayleigh | L2: Aerosol | L3: Ozone | L4: Clouds | L5: Aurora | L6: Airglow |
|-----------|-------------|-------------|-----------|------------|------------|-------------|
| Visible | true | true | true | true | true | true |
| Start Level | 0.00 | 0.00 | 0.10 | 0.04 | 0.65 | 0.55 |
| End Level | 0.60 | 0.15 | 0.23 | 0.35 | 1.00 | 0.95 |
| Center of Density | 0.0 | 0.0 | 0.5 | 0.35 | 0.5 | 0.5 |
| Density Exponent | 3.0 | 4.0 | 5.0 | 4.5 | 2.0 | 2.0 |
| Scattering | 1.0 | 1.0 | 0.0 | 1.0 | 0.0 | 0.0 |
| Absorption | 0.0 | 0.10 | 1.0 | 0.04 | 0.0 | 0.0 |
| Emission | 0.0 | 0.0 | 0.0 | 0.0 | 1.5 | 0.10 |
| Scatter RGB | (0.23,0.45,1.00) | (0.85,0.88,0.90) | (0.80,0.30,0.90) | (1.00,1.00,1.00) | (0.25,0.80,0.40) | (0.15,1.00,0.20) |
| Density | 1.0 | 0.25 | 0.15 | 2.0 | 0.5 | 0.15 |
| Layer Density | Value | Value | Value | Value | Aurora | Value |

---

### 6.2 Super-Earth (Denser, Higher Gravity)

**Planet Radius:** 8,500,000 m  
**Atmosphere Radius:** 8,547,000 m  
**Sky color:** Deep blue  
**Notes:** ~1.5 atm, 1.78g surface gravity. More compressed atmosphere (smaller H). N₂/O₂ dominated.

| Parameter | L1: Rayleigh | L2: Aerosol | L3: Ozone | L4: Clouds | L5: Aurora | L6: Airglow |
|-----------|-------------|-------------|-----------|------------|------------|-------------|
| Visible | true | true | true | true | true | true |
| Start Level | 0.00 | 0.00 | 0.12 | 0.04 | 0.70 | 0.60 |
| End Level | 0.65 | 0.12 | 0.25 | 0.30 | 1.00 | 0.95 |
| Center of Density | 0.0 | 0.0 | 0.5 | 0.35 | 0.5 | 0.5 |
| Density Exponent | 4.0 | 5.0 | 6.0 | 5.0 | 2.0 | 2.0 |
| Scattering | 1.0 | 1.0 | 0.0 | 1.0 | 0.0 | 0.0 |
| Absorption | 0.0 | 0.10 | 1.0 | 0.04 | 0.0 | 0.0 |
| Emission | 0.0 | 0.0 | 0.0 | 0.0 | 2.5 | 0.12 |
| Scatter RGB | (0.23,0.45,1.00) | (0.85,0.85,0.82) | (0.80,0.30,0.90) | (1.00,1.00,1.00) | (0.25,0.80,0.40) | (0.15,1.00,0.20) |
| Density | 1.5 | 0.20 | 0.20 | 2.5 | 0.4 | 0.12 |
| Layer Density | Value | Value | Value | Value | Aurora | Value |

---

### 6.3 Terraformed Mars-Analog

**Planet Radius:** 3,389,000 m  
**Atmosphere Radius:** 3,528,000 m  
**Sky color:** Pale blue-white, slightly pink  
**Notes:** ~0.4 atm. CO₂ 40%, N₂ 45%, O₂ 15%. Large scale height (low gravity) but thin overall. Weak magnetic field → faint/absent aurora. Dust storms common.

| Parameter | L1: Rayleigh | L2: Aerosol | L3: Ozone | L4: Clouds | L5: Aurora | L6: Airglow |
|-----------|-------------|-------------|-----------|------------|------------|-------------|
| Visible | true | true | true | true | true | true |
| Start Level | 0.00 | 0.00 | 0.05 | 0.02 | 0.60 | 0.50 |
| End Level | 0.55 | 0.20 | 0.15 | 0.30 | 1.00 | 0.90 |
| Center of Density | 0.0 | 0.0 | 0.5 | 0.30 | 0.5 | 0.5 |
| Density Exponent | 2.5 | 3.0 | 4.0 | 3.5 | 2.0 | 2.0 |
| Scattering | 1.0 | 1.0 | 0.0 | 1.0 | 0.0 | 0.0 |
| Absorption | 0.0 | 0.15 | 1.0 | 0.05 | 0.0 | 0.0 |
| Emission | 0.0 | 0.0 | 0.0 | 0.0 | 0.4 | 0.05 |
| Scatter RGB | (0.26,0.48,1.00) | (0.95,0.80,0.55) | (0.80,0.30,0.90) | (1.00,0.98,0.95) | (0.30,0.80,0.40) | (0.15,1.00,0.20) |
| Density | 0.4 | 0.40 | 0.05 | 0.8 | 0.2 | 0.08 |
| Layer Density | Value | Value | Value | Value | Aurora | Value |

---

### 6.4 High-O₂ Carboniferous World

**Planet Radius:** 6,500,000 m  
**Atmosphere Radius:** 6,580,000 m  
**Sky color:** Vivid deep blue, almost violet at zenith  
**Notes:** 35% O₂, 63% N₂, 2% Ar/CO₂. Richer oxygen → more scattering → deeper blue sky. Giant insects era. More ozone (more O₂ = more O₃ production). Lush vegetation → biogenic haze.

| Parameter | L1: Rayleigh | L2: Aerosol | L3: Ozone | L4: Clouds | L5: Aurora | L6: Airglow |
|-----------|-------------|-------------|-----------|------------|------------|-------------|
| Visible | true | true | true | true | true | true |
| Start Level | 0.00 | 0.00 | 0.10 | 0.04 | 0.65 | 0.55 |
| End Level | 0.60 | 0.15 | 0.25 | 0.40 | 1.00 | 0.95 |
| Center of Density | 0.0 | 0.0 | 0.5 | 0.35 | 0.5 | 0.5 |
| Density Exponent | 3.0 | 3.5 | 5.0 | 4.0 | 2.0 | 2.0 |
| Scattering | 1.0 | 1.0 | 0.0 | 1.0 | 0.0 | 0.0 |
| Absorption | 0.0 | 0.10 | 1.0 | 0.04 | 0.0 | 0.0 |
| Emission | 0.0 | 0.0 | 0.0 | 0.0 | 1.5 | 0.12 |
| Scatter RGB | (0.20,0.40,1.00) | (0.70,0.80,0.85) | (0.80,0.30,0.90) | (1.00,1.00,1.00) | (0.25,0.80,0.40) | (0.15,1.00,0.20) |
| Density | 1.2 | 0.30 | 0.25 | 2.5 | 0.5 | 0.15 |
| Layer Density | Value | Value | Value | Value | Aurora | Value |

---

### 6.5 Ocean World

**Planet Radius:** 6,000,000 m  
**Atmosphere Radius:** 6,101,000 m  
**Sky color:** Pale blue, hazy near horizon  
**Notes:** 100% ocean surface. Very high humidity → thick cloud deck, persistent haze. Evaporation drives strong convection. N₂ dominant with significant H₂O vapor.

| Parameter | L1: Rayleigh | L2: Aerosol | L3: Ozone | L4: Clouds | L5: Aurora | L6: Airglow |
|-----------|-------------|-------------|-----------|------------|------------|-------------|
| Visible | true | true | true | true | true | true |
| Start Level | 0.00 | 0.00 | 0.08 | 0.03 | 0.65 | 0.55 |
| End Level | 0.55 | 0.18 | 0.20 | 0.50 | 1.00 | 0.95 |
| Center of Density | 0.0 | 0.0 | 0.5 | 0.40 | 0.5 | 0.5 |
| Density Exponent | 2.5 | 3.5 | 5.0 | 3.0 | 2.0 | 2.0 |
| Scattering | 1.0 | 1.0 | 0.0 | 1.0 | 0.0 | 0.0 |
| Absorption | 0.0 | 0.08 | 1.0 | 0.04 | 0.0 | 0.0 |
| Emission | 0.0 | 0.0 | 0.0 | 0.0 | 1.2 | 0.10 |
| Scatter RGB | (0.35,0.55,1.00) | (0.85,0.88,0.90) | (0.80,0.30,0.90) | (1.00,1.00,1.00) | (0.25,0.80,0.40) | (0.15,1.00,0.20) |
| Density | 1.2 | 0.40 | 0.12 | 3.5 | 0.4 | 0.12 |
| Layer Density | Value | Value | Value | Value | Aurora | Value |

---

### 6.6 K-Dwarf Orbiter (Orange Sun)

**Planet Radius:** 6,371,000 m  
**Atmosphere Radius:** 6,453,000 m  
**Sky color:** Warm yellow-orange overhead, vivid red-orange at sunset  
**Notes:** K-type star emits more red/orange light. Rayleigh still scatters blue most, but there's less blue to scatter → sky appears yellow-orange. N₂/O₂ atmosphere similar to Earth.

**Rayleigh correction:** Multiply base by (1.8, 1.2, 0.85) → renormalize → (0.46, 0.49, 0.55) × boost → **(0.55, 0.65, 1.00)** normalized

| Parameter | L1: Rayleigh | L2: Aerosol | L3: Ozone | L4: Clouds | L5: Aurora | L6: Airglow |
|-----------|-------------|-------------|-----------|------------|------------|-------------|
| Visible | true | true | true | true | true | true |
| Start Level | 0.00 | 0.00 | 0.10 | 0.04 | 0.65 | 0.55 |
| End Level | 0.60 | 0.15 | 0.23 | 0.35 | 1.00 | 0.95 |
| Center of Density | 0.0 | 0.0 | 0.5 | 0.35 | 0.5 | 0.5 |
| Density Exponent | 3.0 | 4.0 | 5.0 | 4.5 | 2.0 | 2.0 |
| Scattering | 1.0 | 1.0 | 0.0 | 1.0 | 0.0 | 0.0 |
| Absorption | 0.0 | 0.10 | 1.0 | 0.04 | 0.0 | 0.0 |
| Emission | 0.0 | 0.0 | 0.0 | 0.0 | 1.5 | 0.10 |
| Scatter RGB | (0.55,0.65,1.00) | (0.90,0.85,0.70) | (0.80,0.30,0.90) | (1.00,0.98,0.92) | (0.30,0.70,0.40) | (0.20,0.80,0.15) |
| Density | 1.0 | 0.30 | 0.15 | 2.0 | 0.5 | 0.15 |
| Layer Density | Value | Value | Value | Value | Aurora | Value |

---

### 6.7 M-Dwarf Orbiter (Red Dwarf)

**Planet Radius:** 5,500,000 m  
**Atmosphere Radius:** 5,595,000 m  
**Sky color:** Deep orange-red, almost Martian at zenith  
**Notes:** M-type star is very red (T~3000–3800K). Very little blue/green light to scatter. Atmosphere likely thin (tidally locked, strong stellar wind can strip atmo). Intense auroras from stellar wind.

**Rayleigh correction:** Multiply base by (2.5, 1.0, 0.5) → renormalize → **(0.80, 0.50, 1.00)** — red-dominant

| Parameter | L1: Rayleigh | L2: Aerosol | L3: Ozone | L4: Clouds | L5: Aurora | L6: Airglow |
|-----------|-------------|-------------|-----------|------------|------------|-------------|
| Visible | true | true | true | true | true | true |
| Start Level | 0.00 | 0.00 | 0.08 | 0.05 | 0.60 | 0.50 |
| End Level | 0.55 | 0.15 | 0.20 | 0.45 | 1.00 | 0.90 |
| Center of Density | 0.0 | 0.0 | 0.5 | 0.35 | 0.5 | 0.5 |
| Density Exponent | 3.0 | 3.5 | 5.0 | 3.5 | 2.0 | 2.0 |
| Scattering | 1.0 | 1.0 | 0.0 | 1.0 | 0.0 | 0.0 |
| Absorption | 0.0 | 0.12 | 1.0 | 0.05 | 0.0 | 0.0 |
| Emission | 0.0 | 0.0 | 0.0 | 0.0 | 3.5 | 0.08 |
| Scatter RGB | (0.80,0.50,1.00) | (0.85,0.75,0.60) | (0.80,0.30,0.90) | (1.00,0.95,0.90) | (0.40,0.30,1.00) | (0.15,1.00,0.20) |
| Density | 0.6 | 0.25 | 0.10 | 2.5 | 0.6 | 0.10 |
| Layer Density | Value | Value | Value | Value | Aurora | Value |

**Tidally locked note:** For permanent dayside, increase Ambient Light slightly. For permanent nightside, aurora may dominate the visible atmosphere.

---

### 6.8 High-Pressure N₂/CO₂ World

**Planet Radius:** 7,000,000 m  
**Atmosphere Radius:** 7,062,000 m  
**Sky color:** Pale grey-white (high optical depth whitens sky)  
**Notes:** 2.5 atm, 60% N₂, 35% CO₂, 5% Ar. Warm (~310K). Dense atmosphere → Rayleigh optical depth > 1 at zenith → sky appears white/pale (multiple scattering randomizes direction). Dense haze layer.

| Parameter | L1: Rayleigh | L2: Aerosol | L3: Ozone | L4: Clouds | L5: Aurora | L6: Airglow |
|-----------|-------------|-------------|-----------|------------|------------|-------------|
| Visible | true | true | true | true | true | true |
| Start Level | 0.00 | 0.00 | 0.10 | 0.03 | 0.65 | 0.55 |
| End Level | 0.65 | 0.20 | 0.23 | 0.35 | 1.00 | 0.95 |
| Center of Density | 0.0 | 0.0 | 0.5 | 0.35 | 0.5 | 0.5 |
| Density Exponent | 3.5 | 4.5 | 5.0 | 4.5 | 2.0 | 2.0 |
| Scattering | 1.0 | 1.0 | 0.0 | 1.0 | 0.0 | 0.0 |
| Absorption | 0.0 | 0.10 | 1.0 | 0.04 | 0.0 | 0.0 |
| Emission | 0.0 | 0.0 | 0.0 | 0.0 | 1.5 | 0.10 |
| Scatter RGB | (0.60,0.70,1.00) | (0.92,0.90,0.88) | (0.80,0.30,0.90) | (1.00,1.00,1.00) | (0.25,0.80,0.40) | (0.15,1.00,0.20) |
| Density | 2.0 | 0.50 | 0.10 | 2.5 | 0.5 | 0.12 |
| Layer Density | Value | Value | Value | Value | Aurora | Value |

**Sky whitening:** The Rayleigh RGB is shifted toward white (0.60, 0.70, 1.00) because at high optical depth, multiple scattering makes the sky appear paler. The higher Density (2.0) also contributes.

---

### 6.9 Cold NH₃/N₂ World

**Planet Radius:** 6,200,000 m  
**Atmosphere Radius:** 6,275,000 m  
**Sky color:** Pale blue with purple tinge near horizon  
**Notes:** T = 243K (-30°C). N₂/NH₃ atmosphere. Possible ammonia-water biochemistry. NH₃ ice crystals create purplish haze. Very cold → less thermal turbulence → smoother atmosphere.

| Parameter | L1: Rayleigh | L2: Aerosol | L3: Ozone | L4: Clouds | L5: Aurora | L6: Airglow |
|-----------|-------------|-------------|-----------|------------|------------|-------------|
| Visible | true | true | true | false | true | true |
| Start Level | 0.00 | 0.00 | 0.10 | 0.03 | 0.65 | 0.55 |
| End Level | 0.55 | 0.18 | 0.22 | 0.40 | 1.00 | 0.90 |
| Center of Density | 0.0 | 0.0 | 0.5 | 0.35 | 0.5 | 0.5 |
| Density Exponent | 3.0 | 3.5 | 5.0 | 3.5 | 2.0 | 2.0 |
| Scattering | 1.0 | 1.0 | 0.0 | 1.0 | 0.0 | 0.0 |
| Absorption | 0.0 | 0.08 | 1.0 | 0.04 | 0.0 | 0.0 |
| Emission | 0.0 | 0.0 | 0.0 | 0.0 | 1.0 | 0.08 |
| Scatter RGB | (0.40,0.30,1.00) | (0.75,0.65,0.90) | (0.80,0.30,0.90) | (0.95,0.90,1.00) | (0.40,0.20,0.80) | (0.15,1.00,0.20) |
| Density | 0.8 | 0.35 | 0.08 | 2.0 | 0.4 | 0.10 |
| Layer Density | Value | Value | Value | Value | Aurora | Value |

**Note:** No real ozone layer in NH₃/N₂ atmosphere. Layer 3 (Ozone) is set visible=false for physical accuracy, but you can enable it as an "absorption layer" if you want a visible stratosphere for visual effect. Clouds here may include NH₃ ice → slight purple tint.

---

## 7. Star-Type Sky Color Adjustment

### How Star Temperature Affects Sky Color

Rayleigh scattering always favors blue (λ⁻⁴). But if the star emits less blue light, the sky has less blue to scatter → shifts warmer.

### Quick Adjustment Method

Take Earth Rayleigh RGB (0.23, 0.45, 1.00) and multiply by star SED correction factors, then normalize so max channel = 1.0:

| Star Type | T_eff (K) | R factor | G factor | B factor | Corrected RGB (normalized) | Sky Color |
|-----------|-----------|----------|----------|----------|---------------------------|-----------|
| **F5V** | 6500 | 0.85 | 0.90 | 1.10 | (0.20, 0.41, 1.00) | Deeper blue |
| **G2V (Sun)** | 5778 | 1.00 | 1.00 | 1.00 | (0.23, 0.45, 1.00) | Blue |
| **K5V** | 4350 | 1.80 | 1.20 | 0.85 | (0.55, 0.65, 1.00) | Yellow-orange |
| **M5V** | 3200 | 2.50 | 1.00 | 0.50 | (0.80, 0.50, 1.00) | Orange-red |
| **M8V** | 2500 | 3.50 | 0.80 | 0.30 | (0.95, 0.38, 1.00) | Deep red-orange |

### Aurora Color Shift

Aurora colors don't change much with star type (emission lines are set by atmospheric gas), but:
- M-dwarf: stronger aurora (higher Emission), possibly more CO₂⁺ purple
- K-dwarf: similar to Earth
- F-dwarf: similar to Earth

---

## 8. Quick-Start Cheat Sheet

### Earth Preset — Copy/Paste Ready

Set **Planet Radius:** `6371000`  
Set **Atmosphere Radius:** `6455000`

**Layer 1 (Rayleigh Troposphere):**
Start Level: 0.00 | End Level: 0.60 | Center of Density: 0.0 | Density Exponent: 3.0 | Scattering: 1.0 | Absorption: 0.0 | Emission: 0.0 | Scatter RGB: (0.23, 0.45, 1.0) | Density: 1.0

**Layer 2 (Aerosol/Haze):**
Start Level: 0.00 | End Level: 0.15 | Center of Density: 0.0 | Density Exponent: 4.0 | Scattering: 1.0 | Absorption: 0.1 | Emission: 0.0 | Scatter RGB: (0.85, 0.88, 0.90) | Density: 0.25

**Layer 3 (Ozone):**
Start Level: 0.10 | End Level: 0.23 | Center of Density: 0.5 | Density Exponent: 5.0 | Scattering: 0.0 | Absorption: 1.0 | Emission: 0.0 | Scatter RGB: (0.80, 0.30, 0.90) | Density: 0.15

**Layer 4 (Clouds):**
Start Level: 0.04 | End Level: 0.35 | Center of Density: 0.35 | Density Exponent: 4.5 | Scattering: 1.0 | Absorption: 0.04 | Emission: 0.0 | Scatter RGB: (1.0, 1.0, 1.0) | Density: 2.0

**Layer 5 (Aurora):**
Start Level: 0.65 | End Level: 1.0 | Center of Density: 0.5 | Density Exponent: 2.0 | Scattering: 0.0 | Absorption: 0.0 | Emission: 1.5 | Scatter RGB: (0.25, 0.80, 0.40) | Density: 0.5 | **Layer Density: Aurora**

**Layer 6 (Airglow):**
Start Level: 0.55 | End Level: 0.95 | Center of Density: 0.5 | Density Exponent: 2.0 | Scattering: 0.0 | Absorption: 0.0 | Emission: 0.10 | Scatter RGB: (0.15, 1.0, 0.20) | Density: 0.15

### Top 5 Mistakes

| # | Mistake | Fix |
|---|---------|-----|
| 1 | Cloud Absorption too high (0.5+) | Keep at 0.03–0.06 (clouds are reflective, not dark) |
| 2 | All layers same Density Exponent | Use 3–5 for surface layers, 2 for high layers |
| 3 | Aurora with Layer Density = Value | Must use **Aurora** mode for procedural texture |
| 4 | Sky appears white, not blue | Lower Density on Layer 1, or reduce Density Exponent |
| 5 | Atmosphere Radius too small | Should be Planet Radius + 80–150 km minimum |

### How to Adjust

| Want to... | Change this... | How |
|------------|---------------|-----|
| Make sky more blue | Layer 1 Scatter RGB | Increase B, decrease R |
| Make sky less blue (hazy) | Layer 2 Density | Increase to 0.4–0.8 |
| Make sky orange/red (sunset) | Star-type correction | Multiply R×1.5–2.0, renormalize |
| Make clouds thicker | Layer 4 Density | Increase to 3.0–4.0 |
| Make clouds more broken | Layer 4 Layer Density | Switch from Value → Texture (use cloud map) |
| Brighter aurora | Layer 5 Emission | Increase to 3.0–5.0 |
| Aurora only at poles | Layer 5 Layer Density | Use Aurora mode (built-in polar projection) |
| Visible airglow | Layer 6 Emission | Increase to 0.2–0.3 |
| Thicker atmosphere overall | Atmosphere Radius | Increase by 50,000–100,000 m |
| Thinner atmosphere | Atmosphere Radius | Decrease toward planet radius |

---

## 9. References

1. Liou, K.N. (2002). *An Introduction to Atmospheric Radiation*, 2nd Ed. Academic Press.
2. Bohren, C.F. & Huffman, D.R. (1983). *Absorption and Scattering of Light by Small Particles*. Wiley.
3. Bucholtz, A. (1995). "Rayleigh-scattering calculations for the terrestrial atmosphere." *Applied Optics*, 34(15), 2765–2773.
4. Vallance Jones, A. (1974). *Aurora*. D. Reidel Publishing.
5. Chamberlain, J.W. (1961). *Physics of the Aurora and Airglow*. Academic Press.
6. Valencia, D. et al. (2006). "Super-Earths in the Habitable Zone." *Icarus*, 181(2), 568–573.
7. Kopparapu, R.K. et al. (2013). "Habitable Zones Around Main-Sequence Stars." *ApJ*, 765, 131.
8. Seager, S. (2010). *Exoplanet Atmospheres*. Princeton University Press.
9. Pierrehumbert, R.T. (2010). *Principles of Planetary Climate*. Cambridge University Press.
10. Grender, J. et al. (1988). "Atomic oxygen green and red line emission." *JGR*, 93(A3), 1949–1955.

---

*Document generated for use with Physical Celestial Objects (PCO) Blender add-on.*  
*All parameter values are starting points — tune visually in Rendered viewport.*
