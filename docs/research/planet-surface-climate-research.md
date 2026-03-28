# Climate/Biome Simulation & Surface Property Generation for Procedural Planets

**Research Date:** 2026-03-27
**Scope:** Climate simulation, biome classification, albedo mapping, roughness generation, and heightmap-derived surface properties for procedural planet rendering.

---

## Table of Contents

1. [Climate & Biome Simulation](#1-climate--biome-simulation)
2. [Albedo Map Generation](#2-albedo-map-generation)
3. [Roughness Map Generation](#3-roughness-map-generation)
4. [Heightmap to Surface Properties](#4-heightmap-to-surface-properties)
5. [Source Catalog](#5-source-catalog)

---

## 1. Climate & Biome Simulation

### 1.1 Latitude-Based Temperature Models

The foundational approach to planetary temperature distribution applies **Lambert's cosine law**: the intensity of stellar radiation on a surface is proportional to the cosine of the angle of incidence.

**Basic model:**

```
T(lat) = T_equator - DeltaT * (1 - cos(lat))
```

Where `T_equator` is the equatorial temperature (~25-30 C for Earth-like), `DeltaT` is the equator-to-pole temperature difference (~50-60 C for Earth), and `lat` is the latitude in radians.

**SpaceEngine's more physically rigorous model** (Source [1]) starts from the planetary equilibrium temperature:

```
T_eq = ( L_star / (4 * pi * d^2) * (1 - A) / (sigma_SB * f) )^(1/4)
```

Where:
- `L_star` = stellar luminosity (W)
- `d` = star-planet distance (m)
- `A` = bond albedo (dimensionless, ~0.30 for Earth)
- `sigma_SB` = Stefan-Boltzmann constant (5.67e-8 W/m^2/K^4)
- `f` = redistribution factor (4 for uniform distribution, 2 for tidally locked)

SpaceEngine then applies a latitudinal dependence using Lambert's cosine law adjusted for axial tilt (obliquity), with a minimum temperature fraction `T_pole` preventing poles from reaching absolute zero.

**Daylight fraction** at a given latitude and subsolar latitude:

```
fraction = arccos(-tan(phi) * tan(phi_SS)) / pi
```

Where `phi` = latitude, `phi_SS` = subsolar latitude (varies with season/obliquity).

For **multi-star systems**, temperatures combine as:

```
T_final = ( T_star1^4 + T_star2^4 + ... )^(1/4)
```

**Practical simplification** (Source [3]): Use Perlin noise combined with a latitude-based curve, ensuring climate is hottest near the equator and colder at the poles, with altitude adjustments layered on top.

### 1.2 Altitude-Based Temperature Lapse Rate

The **International Standard Atmosphere** (ISA), defined by ICAO, specifies a tropospheric lapse rate of **6.50 C/km** from sea level to 11 km (Source [2]).

```
T(altitude) = T_sea_level - 6.5 * (altitude_km)
```

Key parameters:
| Parameter | Value |
|-----------|-------|
| Sea-level temperature (ISA) | 15.0 C |
| Tropospheric lapse rate | 6.50 C/km |
| Tropopause altitude | ~11,000 m |
| Dry adiabatic lapse rate | 9.8 C/km |
| Moist adiabatic lapse rate | ~5.0 C/km (varies with moisture) |

The observed 6.5 C/km is less than the dry adiabatic rate (9.8 C/km) due to:
- Latent heat release during condensation
- Radiative heat transfer from the surface
- Vertical mixing processes

For procedural generation, the combined temperature model is:

```
T(lat, alt) = T_equator - DeltaT * (1 - cos(lat)) - 6.5 * alt_km
```

### 1.3 Precipitation Models

#### Distance from Ocean
Precipitation generally decreases with distance from moisture sources (oceans, large lakes). A simple exponential decay model:

```
P(dist) = P_coast * exp(-dist / lambda)
```

Where `lambda` is a characteristic decay distance (~500-1000 km depending on wind patterns).

#### Orographic Precipitation and Rain Shadow

When air masses encounter mountains, they are forced upward, cool adiabatically, and release moisture. The **AutoBiomes** system (Source [4]) models precipitation as a two-step process simulating rain shadows through temperature differences between target and source regions.

**Nick McDonald's procedural weather system** (Source [5]) implements this as an ODE system where:
- Wind speed increases when pointing uphill, decreases downhill
- Temperature decreases as air is pushed uphill, increases downhill
- Rain forms when temperature and humidity exceed thresholds
- Rain shadows emerge naturally as moisture is depleted on windward slopes

Implementation approach:
1. Define a prevailing wind direction (or simulate wind from pressure differences)
2. For each cell along wind direction, track moisture content
3. When terrain elevation increases: cool air, check condensation threshold, precipitate excess moisture
4. On leeward side: warm descending air, resulting in low humidity and precipitation

#### Simplified Precipitation Rules (Source [6])
- High precipitation: where winds blow onto mountains (windward)
- Low precipitation: rain shadow (leeward of mountains)
- Onshore winds cause high precipitation near coasts
- Interior continental regions tend to be drier
- Near equator: convergence zone creates high rainfall (ITCZ)

### 1.4 Koppen Climate Classification

The Koppen system (Source [7]) classifies climate into 5 main groups using monthly temperature and precipitation data, typically averaged over 30+ years. Here is the complete algorithmic classification:

#### Group A: Tropical (coldest month >= 18 C)
- **Af** (Rainforest): driest month >= 60 mm
- **Am** (Monsoon): driest month < 60 mm but >= 100 - (annual_precip / 25) mm
- **Aw/As** (Savanna): driest month < 60 mm AND < 100 - (annual_precip / 25) mm

#### Group B: Arid (annual precipitation below threshold)
Threshold calculation:
```
threshold = T_annual_mean_C * 20
if >= 70% of precip falls in high-sun half:  threshold += 280
elif 30-70% in high-sun half:                threshold += 140
else:                                        threshold += 0
```
- **BW** (Desert): annual precip < 50% of threshold
- **BS** (Steppe): annual precip 50-100% of threshold
- Temperature modifier: **h** (hot) if mean annual temp > 18 C; **k** (cold) if <= 18 C

#### Group C: Temperate (coldest month 0 C to 18 C, warmest month > 10 C)
Precipitation patterns:
- **w** (dry winter): driest winter month < 1/10 of wettest summer month
- **s** (dry summer): driest summer month < 30 mm AND < 1/3 of wettest winter month
- **f** (no dry season): neither w nor s
Temperature subcategories:
- **a**: warmest month > 22 C
- **b**: all months < 22 C but >= 4 months > 10 C
- **c**: 1-3 months > 10 C

#### Group D: Continental (coldest month < 0 C, warmest month > 10 C)
Same precipitation (w, s, f) and temperature (a, b, c) patterns as Group C, plus:
- **d**: coldest month < -38 C

#### Group E: Polar (warmest month < 10 C)
- **ET** (Tundra): warmest month 0-10 C
- **EF** (Ice Cap): all months < 0 C

### 1.5 Holdridge Life Zones

The Holdridge system (Source [8]) classifies land into 37+ life zones using three variables on logarithmic scales:

**Biotemperature** (mean annual temperature with values < 0 C and > 30 C set to 0):

```
T_bio = mean( max(0, min(30, T_monthly)) ) for all 12 months
```

**Potential Evapotranspiration (PET):**

```
PET = 58.93 * T_bio  (mm/year)
```

**Aridity Index:**

```
AI = PET / MAP  (where MAP = mean annual precipitation)
```

Classification thresholds:
- AI < 0.2: hyperarid/arid
- AI < 0.5: dry (semi-arid)
- AI ~ 1.0: subhumid
- AI > 1.0: humid to superhumid

**Latitudinal belts** (by biotemperature):
| Belt | Biotemperature Range |
|------|---------------------|
| Polar | 0-1.5 C |
| Subpolar | 1.5-3 C |
| Boreal | 3-6 C |
| Cool Temperate | 6-12 C |
| Warm Temperate | 12-18 C |
| Subtropical | 18-24 C |
| Tropical | > 24 C |

The system maps onto a triangular diagram with hexagonal life zone boundaries.

### 1.6 Ocean Currents (Simplified)

The **Great Ocean Conveyor Belt** (thermohaline circulation) distributes heat globally (Source [6]):

- Cold water flows from poles toward equator (deep currents)
- Warm water flows from equator toward poles (surface currents)
- In the Northern Hemisphere: warm currents on western continental margins, cold currents on eastern margins
- Southern Hemisphere: reversed pattern

**Effects on climate:**
- Warm currents increase local temperature and precipitation (e.g., Gulf Stream warming NW Europe by ~5-10 C)
- Cold currents decrease local temperature and precipitation (e.g., Benguela Current creating Namib Desert)
- Currents north of equator rotate clockwise; south rotate counterclockwise (Coriolis effect)

**Simplified implementation for procedural generation:**
1. Define major gyre circulation patterns based on continent placement
2. Mark coastal cells as "warm current" or "cold current" influenced
3. Adjust temperature by +/- 5-15 C and precipitation multiplier by 0.5-2.0 based on current type

### 1.7 Biome-to-Albedo Mapping Table

Combining data from NASA, Wikipedia, and climate science sources (Sources [9], [10], [11]):

| Biome / Surface Type | Albedo Range | Typical Value |
|----------------------|-------------|---------------|
| Deep ocean | 0.04-0.08 | 0.06 |
| Tropical rainforest | 0.10-0.15 | 0.13 |
| Conifer forest (summer) | 0.08-0.15 | 0.12 |
| Deciduous forest | 0.15-0.18 | 0.17 |
| Temperate grassland | 0.20-0.28 | 0.25 |
| Shrubland / savanna | 0.15-0.25 | 0.20 |
| Bare soil | 0.15-0.25 | 0.17 |
| Cropland | 0.15-0.25 | 0.20 |
| Tundra | 0.15-0.25 | 0.20 |
| Desert sand | 0.30-0.45 | 0.40 |
| New concrete | 0.50-0.60 | 0.55 |
| Ocean ice | 0.50-0.70 | 0.60 |
| Melting snow | 0.30-0.50 | 0.40 |
| Dirty snow | 0.15-0.25 | 0.20 |
| Fresh snow | 0.75-0.90 | 0.80 |
| Earth average | - | 0.30 |

---

## 2. Albedo Map Generation

### 2.1 Albedo by Biome (Scientific References)

**NASA MODIS data** (Source [9]) confirms that the largest temporal and spatial variations of surface albedo are caused by snow cover. Albedo measurements are obtained using the MODIS instruments on Terra and Aqua satellites, and the CERES instrument.

Key reference values from the Wikipedia Albedo article (Source [10]):
- Fresh asphalt: 0.04
- Open ocean: 0.06
- Worn asphalt: 0.12
- Conifer forest (summer): 0.08-0.15
- Deciduous forest: 0.15-0.18
- Bare soil: 0.17
- Green grass: 0.25
- Desert sand: 0.40
- Ocean ice: 0.50-0.70
- Fresh snow: 0.80

### 2.2 Albedo from Mineral Composition

Rock and soil mineral composition strongly affects surface reflectance (Sources [12], [13]):

| Mineral / Rock Type | Albedo Range | Visual Color |
|--------------------|-------------|--------------|
| Basalt (fresh) | 0.05-0.12 | Dark gray/black |
| Basalt (weathered) | 0.10-0.15 | Dark gray |
| Iron oxide (hematite) | 0.15-0.25 | Red/rust |
| Iron oxide (goethite) | 0.20-0.30 | Yellow-brown |
| Granite | 0.15-0.35 | Gray/pink |
| Limestone | 0.30-0.50 | Buff-white |
| Quartz sand | 0.35-0.45 | White/light |
| Chalk | 0.50-0.65 | White |

Iron oxide spectral signatures:
- Goethite (FeOOH): absorption at 430-460 nm
- Hematite (Fe2O3): absorption at 535-585 nm
- Higher iron oxide content -> lower overall reflectance due to opaque minerals
- Basalt atoms of iron in pyroxenes and olivines show absorption at ~0.950 um (near-infrared)

**For Mars-like planets:** The bright ochre regions contain abundant ferric iron (Fe3+) oxides (rust), with albedo ~0.15-0.25, while dark regions with ferrous iron (Fe2+) in mafic minerals like pyroxene show lower albedo (~0.05-0.15).

### 2.3 Snow/Ice Line

The **snow line** is the lower topographic limit of permanent snow cover (Source [14]):

| Latitude | Approximate Snow Line Altitude |
|----------|-------------------------------|
| Equator (0 deg) | ~4,500 m (15,000 ft) |
| Tropics (20-23 deg) | ~5,000-5,700 m (highest in Himalayas) |
| Mid-latitudes (45 deg) | ~2,500-3,000 m (Alps) |
| Sub-polar (60 deg) | ~1,000-1,500 m |
| Polar (75+ deg) | ~0 m (sea level) |

**Factors modifying snow line position:**
- Windward slopes and sun-facing slopes: snow line up to 1 km higher
- Coastal locations: lower snow line due to more winter snowfall
- Interior continental: higher snow line due to less snowfall
- Summer temperatures and total snowfall are the primary determinants

**Simplified formula for procedural generation:**

```
snow_line_alt(lat) = 5500 - 5500 * (|lat| / 90)^1.5
```

This approximates the non-linear decrease from tropical to polar regions.

### 2.4 Ocean Color Variation

Ocean color depends on depth, turbidity, and biological content (Sources [15], [16]):

| Water Type | Visual Color | Albedo/Reflectance | Cause |
|------------|-------------|-------------------|-------|
| Deep open ocean | Dark blue | 0.04-0.06 | Rayleigh scattering of water; short wavelengths penetrate deepest |
| Clear shallow (<10m) | Turquoise/cyan | 0.10-0.20 | Bottom reflection through clear water |
| Coastal (moderate turbidity) | Green | 0.05-0.10 | Phytoplankton chlorophyll + sediment |
| Turbid estuary | Brown/yellow-green | 0.08-0.15 | High sediment concentration |
| Coral reef shallow | Variable | 0.10-0.25 | High bottom albedo from sand/coral |

Key depth thresholds:
- In clear water: bottom visible to ~40-50 m
- In turbid water: bottom visible to ~5-8 m
- When TSM (total suspended matter) > 50 g/m^3: albedo increases enough to substantially reduce net shortwave heat flux

For procedural rendering, depth-based color can be modeled as:

```
ocean_albedo = deep_albedo + (shallow_albedo - deep_albedo) * exp(-depth / attenuation_depth)
```

Where `attenuation_depth` ~ 5-15 m depending on water clarity.

### 2.5 Seasonal Albedo Variation

The seasonal albedo cycle follows snow cover and vegetation phenology (Source [17]):

- **Peak albedo:** Winter (snow cover)
- **Minimum albedo:** Summer (full vegetation, no snow)
- Snow cover is the dominant factor for seasonal albedo changes
- In mountainous areas: snow dominates albedo variation
- In lowland/vegetated areas: vegetation phenology dominates

Seasonal range examples:
| Region | Winter Albedo | Summer Albedo |
|--------|--------------|---------------|
| Boreal forest (snow-covered) | 0.40-0.60 | 0.08-0.15 |
| Temperate grassland | 0.25-0.35 | 0.20-0.25 |
| Deciduous forest | 0.20-0.30 (bare + snow) | 0.15-0.18 |
| Tundra | 0.60-0.80 (snow) | 0.15-0.20 |
| Desert | 0.35-0.40 | 0.35-0.40 (minimal variation) |

---

## 3. Roughness Map Generation

### 3.1 PBR Roughness Conventions

In physically-based rendering (Sources [18], [19]), roughness is defined on a 0-1 scale:

- **0.0** = perfectly smooth/mirror surface
- **1.0** = fully diffuse/rough surface

Roughness maps are grayscale images: black = smooth, white = rough.

**Microfacet theory** (Source [19]): Every surface is modeled as composed of tiny mirror-like facets (microfacets). Roughness determines how aligned these facets are:
- Low roughness: facets aligned -> coherent specular reflection
- High roughness: facets randomly oriented -> scattered, diffuse reflection

The **Trowbridge-Reitz GGX** normal distribution function:

```
D(h) = alpha^2 / (pi * ((n.h)^2 * (alpha^2 - 1) + 1)^2)
```

Where `alpha` = roughness parameter (0 to 1), `h` = half-vector, `n` = surface normal.

The full **Cook-Torrance specular BRDF**:

```
f_r = k_d * (c / pi) + k_s * (D * F * G) / (4 * (w_o . n) * (w_i . n))
```

Where D = normal distribution (roughness-dependent), F = Fresnel, G = geometry/shadowing.

### 3.2 Roughness from Heightmap Gradient

Roughness can be derived from local terrain slope magnitude:

```
roughness = clamp(gradient_magnitude / max_gradient, 0, 1)
```

Where `gradient_magnitude = sqrt(dz/dx^2 + dz/dy^2)` computed via Sobel or finite difference operators.

### 3.3 Roughness by Surface Type

Approximate PBR roughness values for natural materials (Sources [18], [20]):

| Surface Type | Roughness Range | Typical Value |
|-------------|----------------|---------------|
| Calm water | 0.01-0.05 | 0.03 |
| Wet rock | 0.15-0.30 | 0.20 |
| Ice (glacier) | 0.20-0.40 | 0.30 |
| Ice (sea) | 0.30-0.45 | 0.35 |
| Wet sand | 0.15-0.30 | 0.20 |
| Dry sand (fine) | 0.40-0.60 | 0.50 |
| Dark/wet soil | 0.30-0.50 | 0.40 |
| Light/dry soil | 0.50-0.70 | 0.60 |
| Smooth rock | 0.30-0.50 | 0.40 |
| Rough rock | 0.60-0.85 | 0.75 |
| Conifer forest canopy | 0.60-0.80 | 0.70 |
| Deciduous forest canopy | 0.50-0.70 | 0.60 |
| Grassland | 0.50-0.70 | 0.60 |
| Dry sand (coarse) | 0.70-0.95 | 0.85 |
| Fresh snow | 0.50-0.70 | 0.60 |
| Rubber | ~1.0 | 1.0 |

### 3.4 Geological Interpretation: Terrain Age

Geomorphic cycle theory (William M. Davis, Source [21]) describes landscape evolution through stages:

| Stage | Terrain Character | Roughness Implication |
|-------|------------------|----------------------|
| Youth | Steep slopes, deep valleys, sharp ridges, active erosion | High roughness (0.7-1.0) |
| Maturity | Moderate slopes, rounded hills, wider valleys | Medium roughness (0.4-0.7) |
| Old Age | Gentle slopes, peneplains, low relief | Low roughness (0.2-0.4) |

In practice:
- Young mountains (Himalayas, Andes): steep slopes, severe weather, high erosion rates -> rough terrain
- Old mountains (Appalachians, Scottish Highlands): rounded, subdued topography -> smooth terrain
- Stable warm/moist climates favor diffusive landform evolution -> smooth surfaces even on relatively steep hillslopes

### 3.5 Roughness at Different Scales

Surface roughness is inherently a **multiscale property** (Sources [22], [23]):

**Macro-roughness (waviness):**
- Measured at scales of meters to kilometers
- Captured by heightmap geometry and normal maps
- Represents major terrain features: ridges, valleys, cliffs

**Micro-roughness (texture):**
- Measured at scales of millimeters to centimeters
- Captured by PBR roughness maps
- Represents surface polish, grain, weathering, biological crusts

In PBR workflow:
- Normal maps handle macro-scale surface detail (bumps, dents, cracks)
- Roughness maps handle micro-scale detail (polish, wear, grime)
- The distinction matters because different phenomena dominate at different scales

**Scale-dependent roughness** can be computed using moving windows of different sizes (3x3 to 11x11 cells) on the heightmap, yielding different roughness values at each scale. This is expected since longer wavelengths exist on rough surfaces larger than the scan window.

---

## 4. Heightmap to Surface Properties

### 4.1 Slope, Aspect, and Curvature from Heightmap

Primary topographic attributes are calculated from directional derivatives of the elevation surface (Sources [24], [25]).

#### Finite Difference Method (Horn's 3rd-order)

For a 3x3 window centered on pixel `e`:

```
a b c
d e f
g h i
```

**Slope (gradient magnitude):**

```
dz/dx = ((c + 2f + i) - (a + 2d + g)) / (8 * cellsize)
dz/dy = ((g + 2h + i) - (a + 2b + c)) / (8 * cellsize)
slope = sqrt((dz/dx)^2 + (dz/dy)^2)
slope_degrees = atan(slope) * 180 / pi
```

**Aspect (downslope direction):**

```
aspect = atan2(dz/dy, -dz/dx) * 180 / pi
if aspect < 0: aspect += 360
```

**Curvature (second derivatives):**

```
profile_curvature = -d^2z/ds^2  (in direction of maximum slope)
plan_curvature = -d^2z/dn^2    (perpendicular to slope direction)
```

Where second derivatives are computed from the same 3x3 window using:

```
d2z/dx2 = ((a + 2*b + c) + (g + 2*h + i) - 2*(d + 2*e + f)) / (8 * cellsize^2)
```

(and similarly for dy2 and dxdy mixed partial).

### 4.2 Normal Map Generation from Heightmap

Two main approaches (Source [26]):

#### Sobel-based method (preferred for quality)

The Sobel operator kernels:

```
Gx:                Gy:
[-1  0  +1]       [-1  -2  -1]
[-2  0  +2]       [ 0   0   0]
[-1  0  +1]       [+1  +2  +1]
```

Normalization factor: 1/8.

From gradient to normal:

```
normal.x = -Gx
normal.y = -Gy
normal.z = 1.0 / strength
normal = normalize(normal)
```

Where `strength` controls the apparent height of features.

#### GPU derivative method (fast, lower quality)

```glsl
// GLSL
float h = texture(heightmap, uv).r;
float dhdx = dFdx(h);
float dhdy = dFdy(h);
vec3 normal = normalize(vec3(-dhdx, -dhdy, 1.0));
```

#### Cross-product method

```
vec3 tangent = vec3(2.0 * cellsize, 0.0, h_right - h_left);
vec3 bitangent = vec3(0.0, 2.0 * cellsize, h_up - h_down);
vec3 normal = normalize(cross(tangent, bitangent));
```

### 4.3 Roughness from Local Height Variance (RMS Height)

The two most common parameters for surface roughness characterization are **RMS height** and **correlation length** (Sources [22], [27]).

**RMS Height** (root mean square of elevation deviations):

```
sigma_h = sqrt( (1/N) * sum( (z_i - z_mean)^2 ) )
```

Computed over a local window (typically 3x3 to 11x11 cells).

**Standard deviation of slope** (another roughness metric):

```
sigma_slope = sqrt( (1/N) * sum( (slope_i - slope_mean)^2 ) )
```

**Roughness indices commonly used:**
1. RMSH (Root Mean Square Height): standard deviation of residual elevation
2. Standard deviation of slope
3. Standard deviation of curvature
4. Vector ruggedness measure (VRM)
5. Terrain ruggedness index (TRI)

**Practical computation for PBR roughness from heightmap:**

```python
def compute_roughness(heightmap, window_size=5):
    # Local mean
    kernel = np.ones((window_size, window_size)) / (window_size**2)
    local_mean = convolve2d(heightmap, kernel, mode='same')

    # Local variance
    local_var = convolve2d(heightmap**2, kernel, mode='same') - local_mean**2

    # RMS height
    rms_height = np.sqrt(np.maximum(local_var, 0))

    # Normalize to 0-1 range for PBR
    roughness = np.clip(rms_height / rms_height.max(), 0, 1)
    return roughness
```

### 4.4 Flow Accumulation and D8 Algorithm

The **D8 algorithm** (Source [28]) is the most commonly used method for hydrological analysis of DEMs:

**Step 1: Flow Direction**
For each cell, determine the steepest descent among 8 neighbors:

```
slope_to_neighbor = (z_center - z_neighbor) / distance
```

Where `distance` = `cellsize` for cardinal neighbors, `cellsize * sqrt(2)` for diagonal neighbors. The direction with the maximum positive slope is the flow direction.

**Step 2: Flow Accumulation**
Count the number of upstream cells that drain through each cell:

```
for each cell in topological order (highest to lowest):
    downstream_cell = flow_direction[cell]
    accumulation[downstream_cell] += accumulation[cell] + 1
```

**Step 3: Drainage Network Extraction**
Apply a flow accumulation threshold (FAT):

```
channel[cell] = (accumulation[cell] >= threshold)
```

Typical FAT values range from 100 to 10,000 cells depending on DEM resolution and desired drainage density.

**Pit filling** is a necessary preprocessing step: flat areas and pits (local minima) must be filled or breached to ensure continuous flow paths.

**Popularity:** The D8 algorithm's simplicity makes it the most widely used method, providing reasonable representation for convergent flow and maintaining consistency between flow patterns, contributing area, and subcatchment delineation.

### 4.5 Hypsometric Curve Matching

The **hypsometric curve** measures the relative cumulative area above a given relative elevation threshold within a drainage basin (Source [29]).

**Hypsometric integral:**

```
HI = (z_mean - z_min) / (z_max - z_min)
```

Where `z_mean`, `z_min`, `z_max` are the mean, minimum, and maximum elevations of the basin.

**Interpretation:**
| HI Value | Stage | Terrain Character |
|----------|-------|-------------------|
| > 0.6 | Youth | Convex curve; much area at high elevations |
| 0.35-0.6 | Maturity | S-shaped curve; balanced distribution |
| < 0.35 | Old Age | Concave curve; most area at low elevations |

**For Earth-like realism:** The global hypsometric curve shows ~29% land distribution, with a bimodal distribution (continental shelves and ocean floors). Procedural terrain systems can validate realism by comparing generated hypsometric curves against Earth's (Source [29]).

**Application to procedural generation:**
1. Generate initial heightmap
2. Compute hypsometric curve
3. Compare against target curve (Earth's or desired planet type)
4. Apply histogram equalization or iterative erosion to match
5. Validate that the generated terrain's elevation distribution matches the target

---

## 5. Source Catalog

### [1] SpaceEngine Climate Model
- **Title:** "The Climate Model: Behind The Scenes"
- **URL:** https://spaceengine.org/articles/the-climate-model/
- **Year:** 2023
- **Summary:** Detailed description of SpaceEngine's climate simulation including planetary equilibrium temperature, Lambert's cosine law for latitude dependence, longitudinal thermal transport using radiative and advective timescales, multi-star system support, and real atmospheric temperature-pressure profile data. Provides exact equations for temperature distribution.

### [2] Lapse Rate - Wikipedia / International Standard Atmosphere
- **Title:** "Lapse rate" / "International Standard Atmosphere"
- **URLs:** https://en.wikipedia.org/wiki/Lapse_rate / https://en.wikipedia.org/wiki/International_Standard_Atmosphere
- **Year:** Ongoing
- **Summary:** Standard tropospheric lapse rate of 6.50 C/km defined by ICAO (1919). Explains the difference between dry adiabatic (9.8 C/km) and observed environmental lapse rates. The ISA defines sea-level temperature as 15 C.

### [3] Climate Simulation for Procedural World Generation (Joe Duffy)
- **Title:** "Climate Simulation for Procedural World Generation"
- **URL:** https://www.joeduffy.games/climate-simulation-for-procedural-world-generation
- **Year:** ~2023
- **Summary:** Practical approach using Perlin noise combined with latitude curves for temperature and precipitation. Uses biome lookup table indexed by temperature and rainfall. Implements Poisson Disc Sampling for river generation with altitude-based flow.

### [4] AutoBiomes (Springer)
- **Title:** "AutoBiomes: procedural generation of multi-biome landscapes"
- **URL:** https://link.springer.com/article/10.1007/s00371-020-01920-7
- **Year:** 2020
- **Summary:** Academic paper on efficient multi-biome terrain generation combining procedural techniques with simplified climate simulation. Models precipitation as a two-step process that produces rain shadow effects.

### [5] Procedural Weather Patterns (Nick McDonald)
- **Title:** "Procedural Weather Patterns"
- **URL:** https://nickmcd.me/2018/07/10/procedural-weather-patterns/
- **Year:** 2018
- **Summary:** Implements weather as a coupled ODE system on a grid. Wind interacts with terrain (faster uphill, slower downhill), temperature and humidity diffuse between cells, rain forms above temperature/humidity thresholds. Naturally produces rain shadows and orographic precipitation.

### [6] Geoff's Climate Cookbook (Worldbuilding Workshop)
- **Title:** "Working Out Climates Using Geoff's Climate Cookbook"
- **URL:** https://worldbuildingworkshop.com/2015/11/27/climate/
- **Year:** 2015
- **Summary:** Practical guide to climate placement for worldbuilding. Covers ocean current effects (warm vs cold currents), wind/pressure patterns, precipitation rules, rain shadow effects, and latitude-based temperature zones. Includes rules for the Great Ocean Conveyor Belt.

### [7] Koppen Climate Classification - Wikipedia
- **Title:** "Koppen climate classification"
- **URL:** https://en.wikipedia.org/wiki/K%C3%B6ppen_climate_classification
- **Year:** Ongoing
- **Summary:** Complete algorithmic description of the Koppen-Geiger system with all threshold values. Defines Groups A through E with precipitation and temperature subtypes. Includes the arid threshold formula and all numerical boundaries.

### [8] Holdridge Life Zones - Wikipedia
- **Title:** "Holdridge life zones"
- **URL:** https://en.wikipedia.org/wiki/Holdridge_life_zones
- **Year:** Ongoing (original system: Holdridge, 1947/1967)
- **Summary:** Classification using biotemperature, precipitation, and PET ratio on logarithmic scales. PET = 58.93 * biotemperature. Defines 37+ life zones from polar desert to tropical rain forest. Uses triangular diagram with hexagonal boundaries.

### [9] NASA Albedo Data
- **Title:** "Albedo Values" / "Measuring Earth's Albedo"
- **URLs:** https://mynasadata.larc.nasa.gov/basic-page/albedo-values / https://earthobservatory.nasa.gov/images/84499/measuring-earths-albedo
- **Year:** Ongoing
- **Summary:** NASA reference for albedo values. Forests ~0.15, fresh snow ~0.90, Earth planetary albedo ~0.31. Data from MODIS and CERES instruments on Terra and Aqua satellites.

### [10] Albedo - Wikipedia
- **Title:** "Albedo"
- **URL:** https://en.wikipedia.org/wiki/Albedo
- **Year:** Ongoing
- **Summary:** Comprehensive table of albedo values: fresh asphalt (0.04), open ocean (0.06), conifer forest (0.08-0.15), deciduous forest (0.15-0.18), bare soil (0.17), green grass (0.25), desert sand (0.40), ocean ice (0.50-0.70), fresh snow (0.80). Earth's Bond albedo = 0.294.

### [11] UCAR Albedo and Climate
- **Title:** "Albedo and Climate"
- **URL:** https://scied.ucar.edu/learning-zone/how-climate-works/albedo-and-climate
- **Year:** Ongoing
- **Summary:** Educational resource from the University Corporation for Atmospheric Research. Confirms forest albedo ~15%, fresh snow ~90%, Earth planetary albedo ~31%.

### [12] Reflectance Spectra of Analog Basalts
- **Title:** "Reflectance spectra of analog basalts; implications for remote sensing of lunar geology"
- **URL:** https://www.sciencedirect.com/science/article/abs/pii/S0032063309001792
- **Year:** 2009
- **Summary:** Basalt reflectance from 7% (hand specimen) to 35% (crushed <250um). Iron in pyroxenes/olivines shows absorption at ~0.950 um. Covers 350-2500 nm spectral range.

### [13] Martian Surface - Wikipedia
- **Title:** "Martian surface"
- **URL:** https://en.wikipedia.org/wiki/Martian_surface
- **Year:** Ongoing
- **Summary:** Bright ochre areas: ferric iron (Fe3+) oxides with higher albedo. Dark areas: ferrous iron (Fe2+) in mafic minerals (pyroxene) with lower albedo. Demonstrates iron oxide control over planetary surface albedo.

### [14] Snow Line - Wikipedia / Britannica
- **Title:** "Snow line"
- **URLs:** https://en.wikipedia.org/wiki/Snow_line / https://www.britannica.com/science/snow-line-topography
- **Year:** Ongoing
- **Summary:** Snow line at equator ~4,500 m; Himalayas ~5,700 m; Alps ~3,000 m; falls to sea level at poles. Modified by windward/leeward exposure, coastal proximity, and seasonal factors.

### [15] Ocean Color Remote Sensing (IntechOpen)
- **Title:** "Challenges and New Advances in Ocean Color Remote Sensing of Coastal Waters"
- **URL:** https://www.intechopen.com/chapters/45249
- **Year:** ~2014
- **Summary:** Covers bottom albedo effects in shallow waters, adjacency effects, and absorbing aerosols. Open ocean color driven by phytoplankton; coastal waters influenced by dissolved/particulate matter.

### [16] Shallow Water Bathymetry from Ocean Color
- **Title:** "Diffuse reflectance of oceanic shallow waters: Influence of water depth and bottom albedo"
- **URL:** https://aslopubs.onlinelibrary.wiley.com/doi/abs/10.4319/lo.1994.39.7.1689
- **Year:** 1994 (Maritorena)
- **Summary:** Classic paper on how bottom albedo affects water color in shallow environments. Optical depth varies with wavelength; turbid water optically shallow at 5-8 m, clear water at 40-50 m.

### [17] Seasonal Albedo and Vegetation Phenology (Springer)
- **Title:** "Parameterization of snow-free land surface albedo as a function of vegetation phenology based on MODIS data"
- **URL:** https://link.springer.com/article/10.1007/s00704-008-0003-y
- **Year:** 2008
- **Summary:** Surface albedo follows seasonal cycle with winter peak and summer minimum. Snow cover dominates albedo variation in mountains; vegetation phenology dominates in lowlands.

### [18] PBR Guide Part 2 (Adobe/Allegorithmic)
- **Title:** "The PBR Guide - Part 2"
- **URL:** https://substance3d.adobe.com/tutorials/courses/the-pbr-guide-part-2
- **Year:** ~2018 (updated)
- **Summary:** Definitive industry reference for PBR roughness. Roughness 0 = mirror, 1 = fully diffuse. Roughness maps are grayscale. Covers metallic/roughness workflow and material reference values.

### [19] LearnOpenGL PBR Theory
- **Title:** "Theory - PBR"
- **URL:** https://learnopengl.com/PBR/Theory
- **Year:** ~2020
- **Summary:** Comprehensive PBR theory including microfacet model, Cook-Torrance BRDF, GGX normal distribution function, Fresnel equations, and geometry function. Explains how roughness parameter controls microfacet alignment.

### [20] Physically Based Database
- **Title:** "Physically Based - The PBR values database"
- **URL:** https://physicallybased.info
- **Year:** Ongoing
- **Summary:** Database of physically based material values for CG artists. Provides IOR, reflectance, and spectral data for materials including sand, ice, snow, water, grass, marble, brick, and more.

### [21] Geomorphic Cycle / Landscape Evolution
- **Title:** "Geomorphic cycle" / "Geology and Physical Processes - Mountains"
- **URLs:** https://www.britannica.com/science/geomorphic-cycle / https://www.nps.gov/subjects/mountains/geology.htm
- **Year:** Ongoing
- **Summary:** Davis's geomorphic cycle theory: youth (steep, rough) -> maturity (moderate) -> old age (smooth, peneplain). Young mountains have high erosion rates and rough terrain; old stable landscapes become smooth through diffusive processes.

### [22] Terrain Roughness from Elevation Maps (CMU)
- **Title:** "Terrain Roughness Measurement from Elevation Maps"
- **URL:** https://www.ri.cmu.edu/pub_files/pub3/hoffman_regis_1989_1/hoffman_regis_1989_1.pdf
- **Year:** 1989 (Hoffman & Krotkov)
- **Summary:** Foundational paper on computing terrain roughness from DEMs using amplitude, slope, and correlation analysis. Establishes RMS height and correlation length as primary roughness parameters.

### [23] Scale-Dependent Roughness Parameters
- **Title:** "Scale-dependent roughness parameters for topography analysis"
- **URL:** https://www.sciencedirect.com/science/article/pii/S2666523921001367
- **Year:** 2021
- **Summary:** Roughness is scale-dependent; measurements at different window sizes yield different values. Scale-Dependent Roughness Parameters (SDRP) analysis yields slope, curvature, and higher-order derivatives at many scales.

### [24] WhiteboxTools Geomorphometric Analysis
- **Title:** "Geomorphometric analysis - WhiteboxTools User Manual"
- **URL:** https://www.whiteboxgeo.com/manual/wbt_book/available_tools/geomorphometric_analysis.html
- **Year:** Ongoing
- **Summary:** Uses Horn's (1981) 3rd-order finite difference method for slope. Computes planform/profile curvature from second derivatives. Provides tools for aspect, hillshade, and other terrain metrics.

### [25] Sobel Operator - Wikipedia
- **Title:** "Sobel operator"
- **URL:** https://en.wikipedia.org/wiki/Sobel_operator
- **Year:** Ongoing
- **Summary:** Defines the 3x3 Gx and Gy kernels. Gradient magnitude G = sqrt(Gx^2 + Gy^2), direction theta = atan2(Gy, Gx). Normalization factor 1/8 for accurate derivative estimation.

### [26] Normal Map from Heightmap (GameDev.net / GPU forums)
- **Title:** "Create a normal map from Heightmap in a pixel shader?"
- **URL:** https://www.gamedev.net/forums/topic/428776-create-a-normal-map-from-heightmap-in-a-pixel-shader/
- **Year:** ~2006-2015
- **Summary:** Multiple GPU-based approaches: Sobel sampling, central differences, cross-product method, and GLSL dFdx/dFdy derivatives. Sobel produces higher quality; GPU derivatives are faster but lower quality.

### [27] Comparisons of Roughness Indices (arXiv)
- **Title:** "Comparisons of five indices for estimating local terrain roughness"
- **URL:** https://arxiv.org/pdf/2301.02350
- **Year:** 2023
- **Summary:** Compares RMSH, standard deviation of slope, standard deviation of curvature, vector ruggedness measure, and terrain ruggedness index. Local roughness computed via non-overlapping moving windows (3x3 to 11x11).

### [28] D8 Flow Accumulation
- **Title:** "A fast and simple algorithm for calculating flow accumulation matrices from raster digital elevation"
- **URL:** https://link.springer.com/article/10.1007/s11707-018-0725-9
- **Year:** 2018
- **Summary:** D8 algorithm assigns flow to steepest descent among 8 neighbors. Flow accumulation counts upstream contributing cells. Channel extraction via threshold. Most widely used due to simplicity and reasonable accuracy.

### [29] Hypsometric Curve in Terrain Generation
- **Title:** "Terrain descriptors for landscape synthesis, analysis and simulation"
- **URL:** https://onlinelibrary.wiley.com/doi/10.1111/cgf.70080
- **Year:** 2025 (Argudo et al.)
- **Summary:** Uses hypsometric curves and integrals as terrain descriptors for procedural generation validation. HI > 0.6 = youthful terrain, 0.35-0.6 = mature, < 0.35 = old age. Earth's 29% land distribution used as validation target.

### [30] Procedural Terrain Methods Review (Springer)
- **Title:** "Methods for Procedural Terrain Generation: A Review"
- **URL:** https://link.springer.com/chapter/10.1007/978-3-030-21077-9_6
- **Year:** 2019
- **Summary:** Survey of procedural terrain methods including noise-based, simulation-based (erosion, tectonics), and example-based approaches. Discusses realism metrics and validation techniques including hypsometric analysis.

### [31] Digital Terrain Analysis (Wilson & Gallant)
- **Title:** "Digital Terrain Analysis" (Chapter 1)
- **URL:** https://johnwilson.usc.edu/wp-content/uploads/2016/05/2000-Wilson-Gallant-Terrain-Anaylsis-Chapter-1.pdf
- **Year:** 2000
- **Summary:** Comprehensive reference for computing slope, aspect, plan/profile curvature, flow-path length, and contributing area from DEMs. Covers both finite difference and bivariate interpolation approaches.

### [32] Koppen-Geiger World Map
- **Title:** "World Map of the Koppen-Geiger climate classification"
- **URL:** https://koeppen-geiger.vu-wien.ac.at/pdf/Paper_2006.pdf
- **Year:** 2006
- **Summary:** Updated global Koppen-Geiger map at high resolution. Provides the standard reference implementation of the classification system with all numerical thresholds.

---

*Total distinct sources: 32*
