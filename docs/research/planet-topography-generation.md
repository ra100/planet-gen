# Planetary Topography Generation & Heightmap Techniques

**Research date:** 2026-03-27

---

## Table of Contents

1. [Earth Topography Statistical Properties](#1-earth-topography-statistical-properties)
2. [Power Spectral Density & Fractal Properties of Terrain](#2-power-spectral-density--fractal-properties-of-terrain)
3. [Physical Controls on Elevation](#3-physical-controls-on-elevation)
4. [Heightmap Generation Algorithms](#4-heightmap-generation-algorithms)
5. [Multi-Octave Noise for Multi-Scale Terrain](#5-multi-octave-noise-for-multi-scale-terrain)
6. [Fractal Terrain: Mandelbrot, Musgrave & Multifractals](#6-fractal-terrain-mandelbrot-musgrave--multifractals)
7. [Hydraulic & Diffusion-Based Erosion Models](#7-hydraulic--diffusion-based-erosion-models)
8. [Hydrology-Based Terrain & River Networks](#8-hydrology-based-terrain--river-networks)
9. [Reference Elevation Ranges](#9-reference-elevation-ranges)
10. [Surface Roughness in Planetary Context](#10-surface-roughness-in-planetary-context)
11. [Roughness Map Generation from Heightmaps](#11-roughness-map-generation-from-heightmaps)
12. [Hapke Photometric Model for Roughness](#12-hapke-photometric-model-for-roughness)
13. [Sources](#13-sources)

---

## 1. Earth Topography Statistical Properties

### 1.1 The Hypsometric Curve

Earth's elevation distribution is **bimodal**, a unique feature among solar system bodies caused by the density difference between continental and oceanic crust [1][2][3].

**Two frequency maxima:**
- **+100 m** -- mean level of lowland continental areas
- **-4700 m** -- mean level of the deep-sea floor

This bimodality arises from two distinct crustal compositions [3]:
- **Continental (sialic) crust:** granitic to gabbroic, specific gravity ~2.7
- **Oceanic (simatic) crust:** peridotitic/basaltic, specific gravity ~3.3

**Key statistics:**
| Parameter | Value |
|---|---|
| Land surface fraction | ~29% |
| Ocean surface fraction | ~71% |
| Mean continental elevation | +840 m (+2756 ft) [4] |
| Mean ocean depth | -3688 m (-12,100 ft) [4] |
| Highest point (Mt. Everest) | +8849 m |
| Lowest point (Challenger Deep) | -10,994 m |
| Total relief | ~19.8 km |

Approximately 85% of Earth's surface falls into two narrow elevation bands: (a) 2000 m above sea level to 500 m below sea level, and (b) 3000-6000 m below sea level [1].

**Strahler's hypsometric integral** for a drainage basin is given by [1]:

```
y = [ (d - x)/x * a/(d - a) ]^z
```

where *a*, *d*, and *z* are fitting parameters describing the shape of the cumulative elevation curve.

**Contrast with other planets:** Mars, Moon, and Venus all have **unimodal** elevation distributions, lacking the bimodal signature of two crustal types and plate tectonics [1].

### 1.2 ETOPO Global Relief Model

The ETOPO1 dataset from NOAA/NCEI provides the most comprehensive global elevation data at 1 arc-minute resolution, used to construct the global hypsographic curve [5].

---

## 2. Power Spectral Density & Fractal Properties of Terrain

### 2.1 Spectral Scaling

Topographic surfaces exhibit **self-affine** (not self-similar) fractal behavior. The power spectral density (PSD) of terrain follows a power law [6][7]:

```
S(f) = C / f^beta
```

where:
- `S(f)` = spectral density (power) at spatial frequency *f*
- `beta` = spectral exponent
- `C` = constant

### 2.2 Hurst Exponent and Fractal Dimension

The **Hurst exponent** *H* characterizes the persistence/roughness of the surface [6][7][8]:

**For 1D profiles (topographic cross-sections):**
```
beta = 2H + 1
D_profile = 2 - H
```

**For 2D surfaces:**
```
beta = 2H + 2
D_surface = 3 - H
```

where *D* is the fractal (Hausdorff) dimension.

**Typical values for Earth terrain:**
| Parameter | Range | Notes |
|---|---|---|
| Hurst exponent *H* | 0.4 - 0.8 | Most natural terrain [8] |
| Spectral exponent *beta* (2D) | 2.8 - 3.6 | Derived from H |
| Fractal dimension *D* (2D surface) | 2.2 - 2.6 | Higher = rougher |
| Fractal dimension *D* (3D volume) | Between 2 and 3 | All fractal surfaces [9] |

For example, coastal terrain DEMs show beta ~ 2.8, implying H ~ 0.4 (relatively rough). Smoother continental interiors trend toward H ~ 0.7-0.8 [8].

**Key relationship:** Higher *H* means smoother terrain (more spatial correlation); lower *H* means rougher, more erratic terrain. *H* = 0.5 corresponds to standard Brownian motion [7].

### 2.3 Self-Affine vs Self-Similar

Topographic profiles are **self-affine**: they scale differently in horizontal and vertical directions. This is an important distinction because not all fractal dimension estimation methods are valid for self-affine signals. Spectral analysis gives the least biased and lowest variance estimates [6].

---

## 3. Physical Controls on Elevation

### 3.1 Isostasy

Isostasy is the gravitational equilibrium between Earth's crust (lithosphere) and mantle, such that the crust "floats" at an elevation determined by its thickness and density [10][11].

**Airy isostasy** (variable crustal thickness):
```
h = t_c * (rho_m - rho_c) / rho_m
```
where:
- `h` = surface elevation above datum
- `t_c` = crustal thickness
- `rho_c` = crustal density (~2700 kg/m^3 continental, ~2900 kg/m^3 oceanic)
- `rho_m` = mantle density (~3300 kg/m^3)

**Pratt isostasy** (variable crustal density):
Assumes a constant depth of compensation; elevation varies with density.

### 3.2 Dynamic Topography

Dynamic topography is the component of surface elevation that **cannot** be explained by classical isostatic models. It arises from mantle convection currents exerting vertical stresses on the lithosphere [10][12].

- Typical magnitude: **a few hundred meters** (up to ~1-2 km in extreme cases)
- Important near subduction zones, mantle plumes, and mid-ocean ridges
- Hard to separate from isostatic effects due to uncertainties in crustal/lithospheric thickness

### 3.3 Whole Lithosphere Isostasy (WLI)

Modern understanding recognizes that topography depends on **lithospheric buoyancy** -- both crustal and mantle lithosphere components. Variations in lithosphere thickness exert first-order control on continental elevations. Plausible regional density variations are sufficient to account for global elevations without invoking dynamic topography greater than a few hundred meters [12].

---

## 4. Heightmap Generation Algorithms

### 4.1 Diamond-Square Algorithm

A classic midpoint displacement algorithm that produces fractal terrain on a grid of size `(2^n + 1) x (2^n + 1)` [13][14].

**Algorithm:**
1. Initialize four corner values
2. **Diamond step:** For each square, set its center = average of 4 corners + random offset
3. **Square step:** For each diamond, set its center = average of 4 adjacent points + random offset
4. Reduce random range by factor `2^(-H)` each iteration (where H in [0,1])
5. Repeat until grid is filled

**Roughness control:** The scaling factor `2^(-H)` per iteration directly controls the Hurst exponent:
- H near 0 = very rough terrain
- H near 1 = very smooth terrain

**Characteristics:**
- Fast: O(N) for N grid points
- Constrained to `(2^n + 1)` dimensions
- Produces visible axis-aligned "creases" (directional artifacts) [14]
- Edge points have 3 neighbors instead of 4; can wrap toroidally for seamless tiling [13]

### 4.2 Perlin Noise

Gradient noise function developed by Ken Perlin (1982). Produces smooth, continuous noise suitable for terrain generation [15][16].

**Algorithm steps:**
1. Define a grid of random unit-length gradient vectors
2. For point P, find the containing grid cell
3. Compute offset vectors from each of the `2^n` cell corners to P
4. Compute dot products: `dot(gradient_i, offset_i)` for each corner *i*
5. Interpolate using a **fade/ease function**

**Fade function** (Perlin 2002 improved version) [15]:
```
fade(t) = 6t^5 - 15t^4 + 10t^3
```
This has zero first **and** second derivatives at t=0 and t=1, eliminating visible grid artifacts.

**Computational complexity:** O(2^n) per evaluation in *n* dimensions [15].

### 4.3 Simplex Noise

Improved noise function by Ken Perlin (2001) using a simplex grid (triangles in 2D, tetrahedra in 3D) instead of a square/cubic grid [17].

**Advantages over classic Perlin noise:**
- **Fewer artifacts:** Visually isotropic (no directional artifacts)
- **Lower complexity:** O(n^2) in *n* dimensions vs O(n * 2^n) for Perlin
- **Better scaling:** Particularly advantageous in 3D+ dimensions
- **Continuous gradient:** Well-defined and cheaply computed nearly everywhere
- **Hardware-friendly:** Simpler to implement in shaders

In practice, Perlin noise may still be faster for 2D specifically, but Simplex dominates for 3D and higher [17].

---

## 5. Multi-Octave Noise for Multi-Scale Terrain

Real terrain has structure at many scales simultaneously -- from continental shelves (~1000 km) down to individual rocks (~1 m). Multi-octave noise (also called fractal noise or fBm) achieves this by summing noise at different frequencies [18][19][20].

### 5.1 The fBm Summation Formula

```
elevation(x, y) = SUM_{i=0}^{octaves-1} [ amplitude_i * noise(frequency_i * x, frequency_i * y) ]
```

where:
```
frequency_i = lacunarity^i * base_frequency
amplitude_i = persistence^i * base_amplitude
```

**Standard parameters:**
| Parameter | Typical Value | Effect |
|---|---|---|
| Octaves | 4 - 8 | Number of noise layers |
| Lacunarity | 2.0 | Frequency multiplier per octave |
| Persistence (gain) | 0.5 | Amplitude multiplier per octave |
| Base frequency | Depends on world scale | Sets largest feature size |

**Concrete example** [19]:
```
elevation = (1.0  * noise(1*x, 1*y)      // continental shapes
           + 0.5  * noise(2*x, 2*y)      // large mountain ranges
           + 0.25 * noise(4*x, 4*y)      // hills and valleys
           + 0.125 * noise(8*x, 8*y))    // small-scale detail
           / (1.0 + 0.5 + 0.25 + 0.125)  // normalize
```

### 5.2 Scale Interpretation

| Octave | Approx. Feature Size | Terrain Feature |
|---|---|---|
| 1 | 1000+ km | Continental landmasses, ocean basins |
| 2 | ~500 km | Mountain ranges, large plateaus |
| 3 | ~250 km | Individual mountain groups |
| 4 | ~125 km | Valleys, large hills |
| 5 | ~60 km | Hills, ridges |
| 6 | ~30 km | Small hills, cliffs |
| 7-8 | <15 km | Surface texture, boulders |

### 5.3 Elevation Redistribution

Raw noise produces uniform elevation distributions. To create more realistic terrain with flat lowlands and sharp peaks [19]:
```
elevation = pow(raw_elevation, exponent)
```
Higher exponents push mid-elevations down, creating broad valleys with occasional sharp peaks.

---

## 6. Fractal Terrain: Mandelbrot, Musgrave & Multifractals

### 6.1 Historical Foundation

Mandelbrot provided the earliest representations of fractal terrain by comparing the self-similarity of mountainous terrain to Brownian motion. Later work by Mandelbrot and F. Kenton Musgrave showed increasingly realistic terrain using **fractional Brownian motion (fBm)** in 3D space combined with Perlin noise and **multifractals** [9][21].

### 6.2 Musgrave's Terrain Functions

Musgrave formalized several key procedural terrain models, all parameterized by *H* (fractal increment), *lacunarity*, and *octaves* [22][23]:

#### Standard fBm
```
fBm(point) = SUM_{i=0}^{octaves-1} noise(point * lacunarity^i) * lacunarity^(-i*H)
```
- Spectral weights: `w_i = frequency_i^(-H)`
- H=1.0: smooth terrain; H->0: approaches white noise
- Self-similar across all scales

#### Ridged Multifractal
Creates sharp ridges and mountain peaks [22][23]:
```
signal = offset - abs(noise(point))    // invert absolute value -> ridges
signal = signal^2                       // sharpen the ridges
weight = signal * gain                  // prior octave modulates next
result += signal * spectral_weight[i]
```
**Recommended defaults:** H=1.0, offset=1.0, gain=2.0

The key insight is that the absolute value of noise creates sharp creases (ridges), and squaring amplifies them. The gain parameter causes large features to modulate small features, producing steep mountains with smooth valleys.

#### Hybrid Multifractal
Combines fBm (low altitudes) with multifractal behavior (high altitudes) [22]:
**Recommended defaults:** H=0.25, offset=0.7

#### Hetero Terrain
Heterogeneous terrain where local altitude modulates subsequent octave amplitudes [22]:
- Higher areas get more detail (more erosion-like features)
- Lower areas remain smooth (sediment-filled basins)

### 6.3 Multifractal Advantage

Standard fBm is self-similar at all scales -- every part of the terrain has the same roughness. Real terrain is **multifractal**: mountain peaks are rougher than valley floors. Multifractal models capture this by making the fractal parameters spatially varying [21].

---

## 7. Hydraulic & Diffusion-Based Erosion Models

### 7.1 Stream Power Law (Detachment-Limited Erosion)

The most widely used fluvial erosion model in geomorphology [24][25][26]:

```
E = K * A^m * S^n
```

where:
- `E` = erosion rate [m/yr]
- `K` = erosion coefficient (erodibility, depends on lithology) [m^(1-2m)/yr]
- `A` = upstream drainage area [m^2] (proxy for water discharge)
- `S` = local slope (= |dz/dx|)
- `m` = area exponent, typically **0.4 - 0.5**
- `n` = slope exponent, typically **1.0 - 1.2**

**Concavity index:** theta = m/n, typically ~0.4-0.5 for natural rivers.

The full landscape evolution equation under detachment-limited erosion with tectonic uplift:
```
dz/dt = U - K * A^m * |nabla z|^n
```
where `U` = uplift rate [m/yr]. At steady state, `dz/dt = 0` and the landscape is in equilibrium [25][26].

**Mathematical character:** This is a **hyperbolic (advection-type)** PDE -- information propagates **upstream** only [24].

### 7.2 Transport-Limited Erosion

An alternative model where erosion rate is limited by the flow's ability to transport sediment, not its ability to detach rock [24]:

```
dz/dt = -nabla . q_s
q_s = K_t * A^m * S^n
```

where `q_s` is sediment flux per unit width [m^2/yr].

**Mathematical character:** Parabolic (diffusion-type) PDE -- information propagates in **both** directions [24].

**When to use which:**
- **Detachment-limited:** Steep, tectonically active landscapes; bedrock rivers
- **Transport-limited:** Low-gradient landscapes; alluvial rivers; decaying topography

### 7.3 Diffusion-Based Erosion (Thermal Weathering / Soil Creep)

Models hillslope smoothing processes (frost shattering, rain splash, bioturbation, soil creep) as a diffusion process [27][28]:

**Linear diffusion:**
```
dz/dt = kappa * nabla^2(z)
```

where `kappa` is the **hillslope diffusivity** [m^2/yr].

**Published diffusivity values:**

| Setting | kappa [m^2/yr] | Source |
|---|---|---|
| Sierra Nevada, California | 1.8 | [28] |
| Generic temperate hillslopes | 3.6 +/- 0.55 (x10^-2 m^2/yr = 360 cm^2/yr) | [28] |
| Active mountain ranges (rock) | ~10 | [28] |

**Nonlinear transport law** (for steep slopes approaching failure) [28]:
```
q_s = -kappa * nabla(z) / [1 - (|nabla z| / S_c)^2]
```
where `S_c` is the critical slope (angle of repose), typically ~30-37 degrees (tan(S_c) ~ 0.6-0.75).

This diverges as slope approaches `S_c`, producing sharp cliff faces.

### 7.4 Particle-Based Hydraulic Erosion

A practical simulation approach for terrain generation [29][30]:

**Algorithm (per water droplet):**
1. Spawn droplet at random position with zero sediment
2. Compute surface normal at current position
3. Update velocity using gravity component along slope (with friction)
4. Move droplet by velocity vector
5. Compute sediment capacity: `capacity ~ slope * velocity * water_volume`
6. If carrying < capacity: erode terrain, add sediment to droplet
7. If carrying > capacity: deposit excess sediment
8. Apply evaporation to water volume
9. Terminate if velocity ~ 0 or max iterations reached

**Practical parameters** [29]:
- Droplet count: 35,000-50,000 (100,000 produces unrealistic results)
- Post-processing: Gaussian blur on heightmap to smooth artifacts

**Three approaches compared** [30]:
1. **Simulation-based:** O(N^3) -- physically realistic but slow
2. **GAN-based:** Train on real elevation data (e.g., USGS NED); fast inference but little control
3. **River-network-first:** O(N^2 log N) -- efficient, good drainage structure

---

## 8. Hydrology-Based Terrain & River Networks

### 8.1 Rivers-First Approach

Instead of generating terrain then eroding it, generate river networks first, then sculpt terrain to match [31][32][33]:

1. Create land/ocean mask (e.g., from fBm + thresholding)
2. Generate river mouth points on coastlines
3. Grow river graphs **upstream** from ocean mouths using Poisson disc sampling + Delaunay triangulation
4. Assign elevations inversely proportional to drainage area along edges
5. Interpolate terrain between rivers using triangulation
6. Apply thermal erosion pass for final detail

**Constraint:** Rivers only merge downstream (no bifurcation). The **Strahler number** classifies stream order and determines river width [31].

### 8.2 Cordonnier et al. (2016): Tectonic Uplift + Stream Power

The first computer graphics method combining uplift and hydraulic erosion at large scale [25]:

Given a user-painted **uplift map**, the method:
1. Generates a stream graph embedding elevation + stream flow
2. Applies the **stream power equation** for erosion
3. Converts the graph to a DEM by blending landform feature kernels

This gives high-level control over dendritic river networks, watersheds, and mountain ridges at low computational cost.

### 8.3 Procedural Drainage Basins (2022)

A more recent approach generates terrain by first placing water bodies, then "growing" terrain outward from them, producing natural-looking landscapes with proper drainage in under 30 seconds [33].

---

## 9. Reference Elevation Ranges

### 9.1 Earth

| Feature | Elevation | Notes |
|---|---|---|
| Challenger Deep | -10,994 m | Mariana Trench |
| Mean ocean floor | -3,688 m | ETOPO1 |
| Sea level | 0 m | Reference datum |
| Mean land elevation | +840 m | ~29% of surface |
| Mt. Everest | +8,849 m | Highest point |
| **Total relief** | **~19.8 km** | |

### 9.2 Mars

| Feature | Elevation | Notes |
|---|---|---|
| Hellas Basin floor | -8,200 m | Deepest point; giant impact crater [34] |
| Mean datum (areoid) | 0 m | Defined by mean atmospheric pressure |
| Olympus Mons summit | +21,229 m | Tallest volcano in solar system [34][35] |
| **Total relief** | **~29.4 km** | Nearly 1.5x Earth's range |

MOLA (Mars Orbiter Laser Altimeter) produced the most precise topographic map of any planet including Earth, with ~700 million laser footprints at ~1 m radial accuracy [34].

Mars has a **unimodal** elevation distribution (no plate tectonics), but shows a strong **hemispheric dichotomy**: the northern lowlands are ~5 km lower than the southern highlands [34].

### 9.3 Other Bodies

| Body | Approx. Total Relief | Notes |
|---|---|---|
| Moon | ~19.8 km | South Pole-Aitken basin to highlands |
| Venus | ~13 km | Maxwell Montes to Diana Chasma |
| Mercury | ~10 km | Impact craters dominate |

---

## 10. Surface Roughness in Planetary Context

### 10.1 Definitions

**RMS height (sigma_h):** Root mean square of surface height deviations from the mean:
```
sigma_h = sqrt( (1/N) * SUM (z_i - z_mean)^2 )
```

**RMS slope (sigma_s):** Root mean square of surface gradients:
```
sigma_s = sqrt( (1/N) * SUM (dz/dx)^2 )
```

**Correlation length (l_c):** The horizontal distance over which the autocorrelation function of the surface decays to 1/e (or 0.2, depending on convention) [36][37]:
- Small `l_c` = surface dominated by high-frequency roughness
- Large `l_c` = smooth, long-wavelength features

### 10.2 Roughness vs Geological Surface Type

Roughness depends strongly on scale (surfaces are rougher at smaller scales) and on geological process [36][37]:

| Surface Type | RMS Height | Correlation Length | Notes |
|---|---|---|---|
| Bare agricultural soil | ~1.8 cm | ~17.8 cm | Moderate roughness [37] |
| Plowed field | ~3-4 cm | ~10-20 cm | Higher roughness |
| Desert pavement | ~0.5-1 cm | ~5-10 cm | Wind-smoothed |
| Lava flows (aa) | ~10-50 cm | ~1-5 m | Very rough |
| Lava flows (pahoehoe) | ~1-5 cm | ~0.5-2 m | Smoother |
| Fault/fracture surfaces | Scale-dependent | Scale-dependent | Rougher at smaller scales [37] |

### 10.3 RMS Slope vs RMS Height

RMS slope and RMS height capture **different** aspects of roughness:
- **RMS height** describes vertical amplitude of roughness
- **RMS slope** describes the angular steepness of surface facets
- They are related through correlation length: `sigma_s ~ sigma_h / l_c`
- For remote sensing, RMS slope is often more relevant as it directly affects scattering geometry

---

## 11. Roughness Map Generation from Heightmaps

Three standard methods for deriving roughness maps from DEMs [38]:

### 11.1 Standard Deviation of Elevation

Apply a moving window (e.g., 3x3 to 41x41 pixels) and compute the standard deviation of elevation values within the window. This captures the **amplitude** of local relief variation.

```
roughness(x,y) = std_dev( z[x-w..x+w, y-w..y+w] )
```

### 11.2 Slope-Based Roughness

1. Compute slope magnitude at each pixel:
   ```
   slope = sqrt( (dz/dx)^2 + (dz/dy)^2 )
   ```
2. Compute standard deviation of slope in a moving window:
   ```
   roughness(x,y) = std_dev( slope[x-w..x+w, y-w..y+w] )
   ```

This captures variability of terrain steepness rather than raw elevation.

### 11.3 Curvature-Based Roughness

Profile curvature (second derivative of elevation) identifies breaks-in-slope:
```
curvature = d^2z/dx^2 + d^2z/dy^2    // Laplacian
roughness(x,y) = std_dev( curvature[window] )
```

Higher-order derivatives are more sensitive to small-scale roughness features.

### 11.4 Practical Tools

Standard GIS tools implement these: GDAL (`gdaldem`), SAGA GIS, WhiteboxTools, and xDEM all provide slope, curvature, and roughness computation from DEMs [38].

---

## 12. Hapke Photometric Model for Roughness

### 12.1 Overview

The Hapke model is a semi-empirical radiative transfer model widely used in planetology to describe how planetary surfaces reflect light as a function of viewing geometry and surface properties [39][40][41].

### 12.2 Bidirectional Reflectance Equation

The full Hapke (2002) reflectance formula [39]:

```
R(i,e,alpha) = (w / 4pi) * [ mu_0e / (mu_0e + mu_e) ]
               * [ P(alpha) * (1 + B(alpha)) + M(mu_0e, mu_e) ]
               * S(i, e, alpha, theta_bar)
```

**Parameters:**

| Symbol | Name | Range | Meaning |
|---|---|---|---|
| w | Single scattering albedo | [0, 1] | Fraction of light scattered vs absorbed per grain |
| b | Asymmetry parameter | [0, 1] | Forward/backward scattering ratio |
| c | Scattering coefficient | [0, 1] | Relative lobe strengths |
| B_0 | Opposition effect amplitude | [0, 1] | Shadow-hiding brightness surge at zero phase |
| h | Opposition effect width | [0, 1] | Angular width of opposition surge |
| theta_bar | Macroscopic roughness | [0, 90 deg] | **Mean slope angle of surface facets** |

### 12.3 Macroscopic Roughness Parameter (theta_bar)

The roughness parameter models the surface as a collection of facets whose slopes follow a **Gaussian distribution** with mean slope angle theta_bar [39][41]:

```
P(theta) ~ exp(-tan^2(theta) / (pi * tan^2(theta_bar)))
```

The correction factor `S(i, e, alpha, theta_bar)` modifies the effective incidence and emission angles to account for:
- Tilted facets changing local illumination geometry
- **Shadowing** between facets at large phase angles
- Mutual obscuration

**Key functions:**
- `H(w, x) = (1 + 2x) / (1 + 2*sqrt(1-w))` -- Ambartsumian-Chandrasekhar H-function (approximation)
- `M(mu_0e, mu_e) = H(w, mu_0e) * H(w, mu_e) - 1` -- multiple scattering term
- `B(alpha) = B_0 / [1 + (1/h)*tan(alpha/2)]` -- opposition effect

### 12.4 Measured Roughness Values

| Surface | theta_bar | Source |
|---|---|---|
| Lunar mare (average) | 16 +/- 4 deg | [40] |
| Fra Mauro regolith | 25 +/- 1 deg | [40] |
| Lunar surface (nadir obs.) | ~20 deg | [40] |
| Lunar surface (multi-angle) | 20-35 deg | [40] |

### 12.5 Limitations

The physical meaning of theta_bar remains debated [40][41]:
- The scale at which roughness is measured is not clearly defined
- theta_bar is not strictly equal to the true mean slope angle of the surface
- The model sometimes requires **unphysical parameters** for icy surfaces
- Having 6 free parameters makes the model difficult to constrain from observations alone

---

## 13. Sources

1. [Hypsometry - Wikipedia](https://en.wikipedia.org/wiki/Hypsometry)
2. [Hypsometric Curve - SERC Carleton](https://serc.carleton.edu/mathyouneed/hypsometric/index.html)
3. [Hypsometric Curve | Britannica](https://www.britannica.com/science/hypsometric-curve)
4. [Ocean | Britannica - Major Subdivisions](https://www.britannica.com/science/ocean/Major-subdivisions-of-the-oceans)
5. [ETOPO Global Relief Model - NCEI/NOAA](https://www.ncei.noaa.gov/products/etopo-global-relief-model)
6. [Relationship between Fractal Dimension and Spectral Scaling Decay Rate - MDPI Symmetry 2016](https://www.mdpi.com/2073-8994/8/7/66)
7. [Four Methods to Estimate the Fractal Dimension from Self-Affine Signals - PMC 2012](https://pmc.ncbi.nlm.nih.gov/articles/PMC3459993/)
8. [Fractal landscape - Grokipedia](https://grokipedia.com/page/Fractal_landscape)
9. [Fractal landscape - Wikipedia](https://en.wikipedia.org/wiki/Fractal_landscape)
10. [Isostasy - Wikipedia](https://en.wikipedia.org/wiki/Isostasy)
11. [Isostasy | Britannica](https://www.britannica.com/science/isostasy-geology)
12. [Global Whole Lithosphere Isostasy - Lamb 2020, G-Cubed](https://agupubs.onlinelibrary.wiley.com/doi/full/10.1029/2020GC009150)
13. [Diamond-square algorithm - Wikipedia](https://en.wikipedia.org/wiki/Diamond-square_algorithm)
14. [Procedurally Generating Terrain - Archer, MICS 2011](https://micsymposium.org/mics_2011_proceedings/mics2011_submission_30.pdf)
15. [Perlin noise - Wikipedia](https://en.wikipedia.org/wiki/Perlin_noise)
16. [Understanding Perlin Noise - Adrian Biagioli 2014](https://adrianb.io/2014/08/09/perlinnoise.html)
17. [Simplex noise - Wikipedia](https://en.wikipedia.org/wiki/Simplex_noise)
18. [Perlin Noise for Procedural Terrain Generation - JDH Wilkins](https://www.jdhwilkins.com/mountains-cliffs-and-caves-a-comprehensive-guide-to-using-perlin-noise-for-procedural-generation)
19. [Making maps with noise - Red Blob Games](https://www.redblobgames.com/maps/terrain-from-noise/)
20. [Procedural Terrain Generation with Noise Functions - Cesium 2017](https://cesium.com/blog/2017/11/17/procedural-terrain-generation-with-noise-functions/)
21. [Procedural Terrain Generation with Fractional Brownian Motion - Game Developer](https://www.gamedeveloper.com/programming/sponsored-feature-procedural-terrain-generation-with-fractional-brownian-motion)
22. [Procedural Fractal Terrains - F. K. Musgrave (UChicago archive)](https://www.classes.cs.uchicago.edu/archive/2015/fall/23700-1/final-project/MusgraveTerrain00.pdf)
23. [musgrave.c - Purdue (Ebert texture code)](https://engineering.purdue.edu/~ebertd/texture/1stEdition/musgrave/musgrave.c)
24. [Transport-limited fluvial erosion - Earth Surface Dynamics 2020](https://esurf.copernicus.org/articles/8/841/2020/)
25. [Large Scale Terrain Generation from Tectonic Uplift and Fluvial Erosion - Cordonnier et al. 2016, EG/CGF](https://inria.hal.science/hal-01262376)
26. [Terrain Generation Using Procedural Models Based on Hydrology - Genevaux et al. 2013, SIGGRAPH](https://hal.science/hal-01339224/file/siggraph2013.pdf)
27. [Fast Hydraulic and Thermal Erosion on the GPU - Jako 2011, CESCG](https://old.cescg.org/CESCG-2011/papers/TUBudapest-Jako-Balazs.pdf)
28. [Hillslope evolution by diffusive processes - Fernandes & Dietrich 1997, Water Resources Research](https://agupubs.onlinelibrary.wiley.com/doi/pdf/10.1029/97WR00534)
29. [Simulating hydraulic erosion - Job Talle](https://jobtalle.com/simulating_hydraulic_erosion.html)
30. [Three Ways of Generating Terrain with Erosion Features - GitHub (dandrino)](https://github.com/dandrino/terrain-erosion-3-ways)
31. [Procedural river drainage basins - Red Blob Games](https://www.redblobgames.com/x/1723-procedural-river-growing/)
32. [RiverLand: Efficient Procedural Modeling of Terrains - Springer 2009](https://link.springer.com/chapter/10.1007/978-3-642-10331-5_44)
33. [Procedural Generation of Landscapes with Water Bodies Using Artificial Drainage Basins - CGI 2022](https://cgvr.cs.uni-bremen.de/papers/cgi22/CGI22.pdf)
34. [Mars: Topography - NOAA Science On a Sphere](https://sos.noaa.gov/catalog/datasets/mars-topography/)
35. [Olympus Mons - Wikipedia](https://en.wikipedia.org/wiki/Olympus_Mons)
36. [The roughness of natural terrain: A planetary and remote sensing perspective - ResearchGate](https://www.researchgate.net/publication/234039430_The_roughness_of_natural_terrain_A_planetary_and_remote_sensing_perspective)
37. [Surface roughness characterization using REA analysis - Nature Scientific Reports 2024](https://www.nature.com/articles/s41598-024-52329-4)
38. [Terrain Topology Algorithms - GitHub (Scrawk)](https://github.com/Scrawk/Terrain-Topology-Algorithms)
39. [Hapke's model - PlanetGLLiM documentation](https://xllim.gitlabpages.inria.fr/planet-gllim/rst/scientific_doc/photometric_models/hapke.html)
40. [A test of the Hapke photometric model - Shepard 2007, JGR Planets](https://agupubs.onlinelibrary.wiley.com/doi/full/10.1029/2005JE002625)
41. [Hapke parameters - Wikipedia](https://en.wikipedia.org/wiki/Hapke_parameters)
42. [Physically-based analytical erosion for fast terrain generation - INRIA 2024](http://www-sop.inria.fr/reves/Basilic/2024/TGSC24/Analytical_Terrains_EG.pdf)
43. [Noise for terrains - Learn Procedural Generation](https://aparis69.github.io/LearnProceduralGeneration/terrain/procedural/noise_for_terrains/)
44. [The Book of Shaders: Fractal Brownian Motion](https://thebookofshaders.com/13/)
45. [Fractal Brownian Islands - MIT CSAIL](https://groups.csail.mit.edu/mac/users/rauch/islands/)
