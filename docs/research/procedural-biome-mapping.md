# Procedural Biome Mapping Techniques for Planet Generation

*Research date: 2026-03-28*

---

## Table of Contents

1. [Algorithms for Mapping Biomes from Climate Data](#1-algorithms-for-mapping-biomes-from-climate-data)
2. [Whittaker Diagram Implementation](#2-whittaker-diagram-implementation)
3. [Koppen Climate Classification Algorithms](#3-koppen-climate-classification-algorithms)
4. [Vegetation Distribution Models](#4-vegetation-distribution-models)
5. [Color/Bump Mapping per Biome](#5-colorbump-mapping-per-biome)
6. [Transition Zones Between Biomes](#6-transition-zones-between-biomes)
7. [References](#7-references)

---

## 1. Algorithms for Mapping Biomes from Climate Data

### 1.1 Core Inputs

Biome classification for procedural planets requires three primary inputs, each derived from noise functions or simulation:

| Parameter | Derivation | Typical Range |
|-----------|-----------|---------------|
| **Temperature** | Latitude + elevation lapse rate + noise | -30 to +40 C |
| **Moisture/Precipitation** | Rain shadow sim, Perlin noise, latitude curve | 0-4000 mm/yr |
| **Elevation** | Heightmap (plate tectonics or fractal noise) | -11000 to +9000 m |

### 1.2 Temperature from Latitude and Elevation

The most common formula computes temperature as a function of latitude distance from the equator, modified by an elevation lapse rate [2][8]:

```
pseudocode: compute_temperature(lat, elevation)

  base_temp = MAX_EQUATOR_TEMP                          // e.g. 30 C
  lat_factor = abs(lat) / 90.0                          // 0 at equator, 1 at poles
  temp = base_temp - (lat_factor * TEMP_RANGE)          // e.g. TEMP_RANGE = 60
  temp -= max(0, elevation - SEA_LEVEL) * LAPSE_RATE    // ~6.5 C per 1000m
  temp += perlin_noise(x, y) * NOISE_AMPLITUDE           // local variation
  return temp
```

The Azgaar Fantasy Map Generator uses a variant [3]:

```
temperature = coastal_temp - (cell_height - 0.2) * 20
```

This creates ~13 C differential across the full elevation range (0.2-0.8 normalized), with a configurable coastal baseline of 0-20 C.

### 1.3 Precipitation / Moisture

Precipitation is typically generated with its own noise layer, then modified by:

- **Latitude curve** -- wetter near equator and mid-latitudes, drier at poles and subtropics
- **Elevation penalty** -- less rainfall at higher altitudes
- **Rain shadow** -- moisture drops on the leeward side of mountains (requires wind direction simulation)

From Joe Duffy's climate simulation [2]:

> "Perlin noise was again used to generate a pseudo-random range of values across the map. Like temperature, a curve is applied relative to latitude, and elevation also affects precipitation."

### 1.4 Biome Lookup from Two Axes

The standard approach uses a 2D lookup table indexed by temperature (x-axis) and moisture (y-axis). The Red Blob Games implementation [5] demonstrates this:

```javascript
function biome(e, m) {
  // e = elevation [0,1], m = moisture [0,1]
  if (e < 0.1)  return OCEAN;
  if (e < 0.12) return BEACH;

  if (e > 0.8) {
    if (m < 0.1) return SCORCHED;
    if (m < 0.2) return BARE;
    if (m < 0.5) return TUNDRA;
    return SNOW;
  }

  if (e > 0.6) {
    if (m < 0.33) return TEMPERATE_DESERT;
    if (m < 0.66) return SHRUBLAND;
    return TAIGA;
  }

  if (e > 0.3) {
    if (m < 0.16) return TEMPERATE_DESERT;
    if (m < 0.50) return GRASSLAND;
    if (m < 0.83) return TEMPERATE_DECIDUOUS_FOREST;
    return TEMPERATE_RAIN_FOREST;
  }

  // low elevation
  if (m < 0.16) return SUBTROPICAL_DESERT;
  if (m < 0.33) return GRASSLAND;
  if (m < 0.66) return TROPICAL_SEASONAL_FOREST;
  return TROPICAL_RAIN_FOREST;
}
```

Both elevation and moisture use fractional Brownian motion noise [5]:

```
e = (1*noise(1*nx, 1*ny) + 0.5*noise(2*nx, 2*ny)
    + 0.25*noise(4*nx, 4*ny)) / (1 + 0.5 + 0.25)
```

Different random seeds must be used for the elevation and moisture noise layers to prevent correlation.

---

## 2. Whittaker Diagram Implementation

### 2.1 Classical Whittaker Diagram

The Whittaker biome diagram [1][3] classifies terrestrial biomes on two axes:

- **X-axis**: Mean annual temperature (C)
- **Y-axis**: Mean annual precipitation (mm)

The classical diagram defines ~9 biomes in a roughly triangular shape (hot+wet = tropical rainforest; cold+dry = tundra; hot+dry = desert).

### 2.2 Rectangular Matrix Approach

For programming, the triangular Whittaker diagram is inconvenient. The Azgaar approach [3] converts it to a rectangular lookup matrix:

```
pseudocode: Whittaker rectangular biome matrix

  // 22 biomes, indexed by temperature (0-99) and moisture (0-9)
  BIOME_MATRIX[100][10] = {
    // moisture:  0(dry) ... 9(wet)
    // temp 0:   [ICE,    ICE,    ICE,    ICE,    ICE,    ICE,    ICE,    ICE,    ICE,    ICE   ]
    // temp 10:  [TUNDRA, TUNDRA, TUNDRA, TUNDRA, TUNDRA, TUNDRA, TUNDRA, TUNDRA, TUNDRA, TUNDRA]
    // temp 20:  [COLD_DESERT, COLD_DESERT, STEPPE, STEPPE, BOREAL, BOREAL, BOREAL, TAIGA, TAIGA, TAIGA]
    // ...
    // temp 90:  [HOT_DESERT, HOT_DESERT, SAVANNA, SAVANNA, TROP_SEASONAL, TROP_SEASONAL, TROP_DRY, TROP_DRY, TROP_WET, TROP_RAINFOREST]
  };

  function get_biome(temperature, precipitation):
    // Normalize temperature to 0-99 index
    t_idx = clamp(floor(remap(temperature, MIN_T, MAX_T, 0, 99)), 0, 99)

    // Normalize precipitation to 0-9 index
    p_idx = clamp(floor(remap(precipitation, MIN_P, MAX_P, 0, 9)), 0, 9)

    return BIOME_MATRIX[t_idx][p_idx]
```

The Azgaar implementation uses a 100x10 matrix in a spreadsheet, with x-axis shift accounting for coastal temperature:

```
x_index = cell_height * 100 - (coastal_temperature - 12) / 0.2
```

This ensures snow appears at sub-zero temperatures and hot deserts at the highest values [3].

### 2.3 WorldEngine: Holdridge Life Zones

WorldEngine [6] uses the Holdridge life zones model (closely related to Whittaker) with a hierarchical decision tree. Temperature is checked first (polar -> alpine -> boreal -> cool -> warm -> subtropical -> tropical), then humidity within each zone (superarid -> perarid -> arid -> semiarid -> subhumid -> humid -> perhumid):

```python
# Simplified WorldEngine biome classification
def classify_biome(world, x, y):
    if world.is_ocean(x, y):
        return "ocean"

    if world.is_temperature_polar(x, y):
        if world.is_humidity_superarid(x, y):
            return "polar desert"
        elif world.is_humidity_perarid(x, y):
            return "polar desert"
        else:
            return "ice"

    elif world.is_temperature_alpine(x, y):
        if world.is_humidity_superarid(x, y):
            return "subpolar dry tundra"
        elif world.is_humidity_arid(x, y):
            return "subpolar moist tundra"
        elif world.is_humidity_semiarid(x, y):
            return "subpolar wet tundra"
        else:
            return "subpolar rain tundra"

    elif world.is_temperature_boreal(x, y):
        if world.is_humidity_superarid(x, y):
            return "boreal desert"
        elif world.is_humidity_arid(x, y):
            return "boreal dry scrub"
        elif world.is_humidity_semiarid(x, y):
            return "boreal moist forest"
        elif world.is_humidity_subhumid(x, y):
            return "boreal wet forest"
        else:
            return "boreal rain forest"

    elif world.is_temperature_warm(x, y):
        if world.is_humidity_superarid(x, y):
            return "warm temperate desert"
        # ... continues with ~7 humidity levels
        else:
            return "warm temperate rain forest"

    elif world.is_temperature_tropical(x, y):
        if world.is_humidity_superarid(x, y):
            return "tropical desert"
        # ... continues
        else:
            return "tropical rain forest"
```

WorldEngine defines ~40+ biome types using this scheme. Temperature and humidity thresholds are specified as percentiles of the total land area (default temperature thresholds: .126/.235/.406/.561/.634/.876) [6].

---

## 3. Koppen Climate Classification Algorithms

### 3.1 Decision Tree

The Koppen system [7] classifies climates using monthly temperature and precipitation data. It is the most widely used climate classification and is directly implementable as a decision tree.

```python
def koppen_classify(monthly_temps, monthly_precip):
    """
    monthly_temps: list of 12 average temperatures (C)
    monthly_precip: list of 12 average precipitation (mm)
    Returns: Koppen classification string (e.g. 'Cfa', 'BWh', 'Dfc')
    """
    t_avg = mean(monthly_temps)
    t_max = max(monthly_temps)
    t_min = min(monthly_temps)
    p_total = sum(monthly_precip)
    p_min = min(monthly_precip)
    p_max = max(monthly_precip)

    # Determine if precipitation is concentrated in summer or winter
    # (Assumes northern hemisphere; flip for southern)
    summer_precip = sum(monthly_precip[3:9])   # Apr-Sep
    winter_precip = sum(monthly_precip[:3]) + sum(monthly_precip[9:])
    pct_summer = summer_precip / p_total if p_total > 0 else 0

    # --- Group B: Arid ---
    # Threshold depends on precipitation seasonality
    if pct_summer >= 0.70:
        threshold = t_avg * 20 + 280
    elif pct_summer >= 0.30:
        threshold = t_avg * 20 + 140
    else:
        threshold = t_avg * 20

    if p_total < threshold:
        if p_total < threshold * 0.5:
            # BW - Desert
            return 'BWh' if t_avg >= 18 else 'BWk'
        else:
            # BS - Steppe
            return 'BSh' if t_avg >= 18 else 'BSk'

    # --- Group A: Tropical (all months >= 18 C) ---
    if t_min >= 18:
        if p_min >= 60:
            return 'Af'    # Tropical rainforest
        elif p_min >= 100 - (p_total / 25):
            return 'Am'    # Tropical monsoon
        else:
            return 'Aw'    # Tropical savanna

    # --- Group E: Polar (warmest month < 10 C) ---
    if t_max < 10:
        if t_max >= 0:
            return 'ET'    # Tundra
        else:
            return 'EF'    # Ice cap

    # --- Group C vs D ---
    # C: Coldest month > 0 C (or > -3 C in some schemes)
    # D: Coldest month <= 0 C
    is_group_c = (t_min > 0)  # use > -3 for modified Koppen

    # Second letter: precipitation pattern
    p_summer_min = min(monthly_precip[3:9])
    p_winter_min = min(list(monthly_precip[:3]) + list(monthly_precip[9:]))
    p_summer_max = max(monthly_precip[3:9])
    p_winter_max = max(list(monthly_precip[:3]) + list(monthly_precip[9:]))

    if p_summer_min < p_winter_min / 10:
        second = 's'   # Dry summer
    elif p_winter_min < p_summer_min / 10:
        second = 'w'   # Dry winter
    else:
        second = 'f'   # No dry season

    # Third letter: temperature
    months_above_10 = sum(1 for t in monthly_temps if t >= 10)
    if t_max >= 22:
        third = 'a'    # Hot summer
    elif months_above_10 >= 4:
        third = 'b'    # Warm summer
    elif months_above_10 >= 1:
        third = 'c'    # Cold summer
    else:
        third = 'd'    # Very cold winter (only Group D)

    group = 'C' if is_group_c else 'D'
    return group + second + third
```

### 3.2 Key Thresholds Summary

| Group | Primary Condition | Subtypes |
|-------|------------------|----------|
| **A** (Tropical) | All months >= 18 C | Af: p_min >= 60mm; Am: monsoon threshold; Aw: savanna |
| **B** (Arid) | P < threshold(T, seasonality) | BW: P < 50% threshold; BS: 50-100% |
| **C** (Temperate) | Coldest month 0-18 C, warmest >= 10 C | s/w/f + a/b/c |
| **D** (Continental) | Coldest month < 0 C, warmest >= 10 C | s/w/f + a/b/c/d |
| **E** (Polar) | Warmest month < 10 C | ET: 0-10 C; EF: < 0 C |

### 3.3 Simplification for Procedural Generation

For game/planet generation, the full 30-type Koppen system is often simplified. A practical approach uses annual averages only (not monthly data), mapping to ~12 biome types:

```
pseudocode: simplified_koppen(avg_temp, annual_precip)

  if avg_temp < -10:      return POLAR_ICE
  if avg_temp < 0:        return TUNDRA
  if avg_temp < 5:
    if annual_precip < 250:  return COLD_DESERT
    return TAIGA
  if avg_temp < 15:
    if annual_precip < 250:  return TEMPERATE_DESERT
    if annual_precip < 750:  return GRASSLAND_STEPPE
    return TEMPERATE_FOREST
  if avg_temp < 25:
    if annual_precip < 250:  return HOT_DESERT
    if annual_precip < 1000: return SAVANNA
    return SUBTROPICAL_FOREST
  // avg_temp >= 25
  if annual_precip < 250:   return HOT_DESERT
  if annual_precip < 1500:  return TROPICAL_SEASONAL
  return TROPICAL_RAINFOREST
```

### 3.4 Available Implementations

- **Java**: Koppen Climate Classifier on SourceForge [7]
- **Python**: WorldEngine (Holdridge-based, similar structure) [6]
- **R**: `ggbiome` package with Whittaker lookup [10]
- **MATLAB**: `KoppenGeiger.m` based on Beck et al. (2018) [7]

---

## 4. Vegetation Distribution Models

### 4.1 Poisson Disk Sampling

The standard algorithm for natural-looking vegetation placement [12]. It generates points with a guaranteed minimum distance between them, avoiding both clustering and regularity:

```
pseudocode: poisson_disk_vegetation(biome_map, terrain)

  // Per-biome configuration
  BIOME_VEG_CONFIG = {
    FOREST:           { min_dist: 5,  density: 0.8,  types: [OAK, BIRCH, PINE] },
    TROPICAL_FOREST:  { min_dist: 3,  density: 0.95, types: [PALM, KAPOK, FERN] },
    SAVANNA:          { min_dist: 15, density: 0.3,  types: [ACACIA, GRASS_TUFT] },
    DESERT:           { min_dist: 40, density: 0.05, types: [CACTUS, SCRUB] },
    TUNDRA:           { min_dist: 20, density: 0.15, types: [LICHEN, MOSS, LOW_SHRUB] },
    TAIGA:            { min_dist: 8,  density: 0.6,  types: [SPRUCE, FIR] },
  }

  result = []
  for each biome_type in BIOME_VEG_CONFIG:
    config = BIOME_VEG_CONFIG[biome_type]
    // Generate Poisson disk samples for this biome's region
    points = bridson_poisson_disk(
      bounds = biome_region_bounds(biome_type),
      min_distance = config.min_dist
    )
    for p in points:
      if biome_map.get(p) != biome_type:
        continue   // Point fell outside this biome's actual area
      if random() > config.density:
        continue   // Thin based on density
      if terrain.slope_at(p) > MAX_VEG_SLOPE:
        continue   // No vegetation on cliffs

      veg_type = weighted_random(config.types)
      scale = random_range(0.8, 1.2)
      rotation = random_range(0, 360)
      result.append(VegetationInstance(p, veg_type, scale, rotation))

  return result
```

### 4.2 Bridson's Algorithm (Fast Poisson Disk)

The O(n) algorithm for generating Poisson disk distributions [12]:

```
pseudocode: bridson_poisson_disk(bounds, min_dist, k=30)

  cell_size = min_dist / sqrt(2)
  grid = 2D array of cells, each initially empty
  active_list = []
  result = []

  // Seed with first random point
  p0 = random_point_in(bounds)
  insert(grid, p0)
  active_list.append(p0)
  result.append(p0)

  while active_list is not empty:
    idx = random_index(active_list)
    center = active_list[idx]
    found = false

    for i in 0..k:
      // Generate random point in annulus [min_dist, 2*min_dist]
      angle = random() * 2 * PI
      radius = min_dist + random() * min_dist
      candidate = center + (cos(angle)*radius, sin(angle)*radius)

      if not in_bounds(candidate, bounds):
        continue
      if any_neighbor_within(grid, candidate, min_dist):
        continue

      insert(grid, candidate)
      active_list.append(candidate)
      result.append(candidate)
      found = true
      break

    if not found:
      active_list.remove(idx)  // No room around this point

  return result
```

### 4.3 Variable-Density Placement

For biome-aware density variation, a greyscale density map modulates the minimum distance [12]:

```
variable_min_dist(x, y) = base_min_dist / density_map.sample(x, y)
```

This creates denser vegetation in forests and sparser placement in deserts, all from a single generation pass. The density map can be derived directly from the moisture/temperature values used for biome classification.

### 4.4 Noise-Based Vegetation Clustering

Unity-style implementation for vegetation clustering within biomes [11][13]:

```csharp
// C# Unity example
float noiseValue = Mathf.PerlinNoise(x * 0.1f, z * 0.1f);
if (noiseValue > biome.vegetationThreshold) {
    int prefabIdx = Random.Range(0, biome.vegetationPrefabs.Length);
    float y = terrain.SampleHeight(new Vector3(x, 0, z));
    Vector3 scale = Vector3.one * Random.Range(0.8f, 1.2f);
    Quaternion rot = Quaternion.Euler(0, Random.Range(0, 360), 0);
    Instantiate(biome.vegetationPrefabs[prefabIdx],
                new Vector3(x, y, z), rot).transform.localScale = scale;
}
```

---

## 5. Color/Bump Mapping per Biome

### 5.1 Per-Biome Material Definition

Each biome requires a material definition mapping biome type to visual properties:

```
pseudocode: BiomeMaterial struct

  struct BiomeMaterial {
    color_primary:    vec3     // Dominant albedo color
    color_secondary:  vec3     // Secondary/accent color
    roughness:        float    // PBR roughness [0,1]
    bump_strength:    float    // Normal/bump intensity
    texture_scale:    float    // UV tiling factor
    albedo_texture:   Texture2D
    normal_texture:   Texture2D
  }

  BIOME_MATERIALS = {
    OCEAN:              { color: (0.05, 0.15, 0.40), roughness: 0.1, bump: 0.3 },
    BEACH:              { color: (0.82, 0.75, 0.55), roughness: 0.8, bump: 0.2 },
    SUBTROPICAL_DESERT: { color: (0.90, 0.80, 0.50), roughness: 0.9, bump: 0.4 },
    GRASSLAND:          { color: (0.55, 0.70, 0.30), roughness: 0.7, bump: 0.5 },
    TEMPERATE_FOREST:   { color: (0.20, 0.50, 0.15), roughness: 0.6, bump: 0.7 },
    TROPICAL_FOREST:    { color: (0.10, 0.40, 0.10), roughness: 0.5, bump: 0.8 },
    TAIGA:              { color: (0.25, 0.40, 0.30), roughness: 0.6, bump: 0.6 },
    TUNDRA:             { color: (0.60, 0.65, 0.55), roughness: 0.7, bump: 0.3 },
    SNOW:               { color: (0.95, 0.95, 0.97), roughness: 0.3, bump: 0.1 },
    BARE_ROCK:          { color: (0.55, 0.50, 0.45), roughness: 0.9, bump: 0.9 },
  }
```

### 5.2 Gradient-Based Biome Coloring (Shader Graph)

The Unity procedural planet tutorial [9] uses RGB gradient channels to define biome zones:

- **Red channel**: Polar regions (sampled from y-position/latitude)
- **Green channel**: Forest/temperate biome
- **Blue channel**: Desert biome

Biome gradients are sampled using position multiplied by noise for irregular boundaries:

```
biome_uv = y_position * noise(x, z)  // Breaks up latitude bands
biome_weights = sample_gradient(biome_uv)
// biome_weights.r = polar, .g = forest, .b = desert
```

Per-biome textures are multiplied by their channel weight and summed:

```
final_color = polar_tex * biome_weights.r
            + forest_tex * biome_weights.g
            + desert_tex * biome_weights.b
```

Transition sharpness is controlled with SmoothStep nodes or gradient keys (e.g., "black at 17.5%, white at 22.5% = 5% blend zone") [9].

### 5.3 Triplanar Mapping for Planets

For spherical planets, UV mapping is problematic at the poles. Triplanar mapping solves this by projecting textures from three orthogonal axes and blending by surface normal [14]:

```glsl
// GLSL triplanar mapping for per-biome texturing
vec3 getTriPlanarBlend(vec3 worldNormal) {
    vec3 blending = abs(worldNormal);
    blending = normalize(max(blending, 0.00001));
    float b = blending.x + blending.y + blending.z;
    blending /= b;
    return blending;
}

vec4 triplanarSample(sampler2D tex, vec3 worldPos, vec3 worldNormal, float scale) {
    vec3 blend = getTriPlanarBlend(worldNormal);
    vec4 xaxis = texture2D(tex, worldPos.yz * scale);
    vec4 yaxis = texture2D(tex, worldPos.xz * scale);
    vec4 zaxis = texture2D(tex, worldPos.xy * scale);
    return xaxis * blend.x + yaxis * blend.y + zaxis * blend.z;
}

// Per-biome material application
vec4 getBiomeColor(vec3 worldPos, vec3 worldNormal, int biomeID, float scale) {
    sampler2D albedo = biomeAlbedoTextures[biomeID];
    sampler2D normal = biomeNormalTextures[biomeID];

    vec4 color = triplanarSample(albedo, worldPos, worldNormal, scale);
    vec3 bump  = triplanarSample(normal, worldPos, worldNormal, scale).xyz;
    bump = bump * 2.0 - 1.0;
    bump.xy *= biomeBumpStrength[biomeID];
    bump = normalize(bump);

    return vec4(color.rgb, 1.0);
}
```

### 5.4 Elevation-Based Color Ramps

A simpler approach for distant views maps elevation directly to color [4][5]:

```glsl
// Fragment shader: elevation-based biome coloring
uniform sampler1D biomeColorRamp;  // 1D texture: elevation -> color
uniform float waterLevel;

vec3 terrainColor(float elevation, float moisture) {
    if (elevation < waterLevel) {
        float depth = (waterLevel - elevation) / waterLevel;
        return mix(vec3(0.2, 0.4, 0.7), vec3(0.05, 0.1, 0.3), depth);
    }
    // Remap elevation above water to [0,1]
    float e = (elevation - waterLevel) / (1.0 - waterLevel);
    // Offset by moisture for biome variation
    float lookup = e * 0.8 + moisture * 0.2;
    return texture(biomeColorRamp, lookup).rgb;
}
```

---

## 6. Transition Zones Between Biomes

### 6.1 Weight-Based Biome Blending

The Unity approach [13] computes biome weights from climate distance, ensuring smooth transitions:

```csharp
// C# biome weight calculation
float GetBiomeWeight(BiomeData biome, float temperature, float humidity) {
    float tempMatch = Mathf.Abs(biome.temperature - temperature);
    float humMatch  = Mathf.Abs(biome.humidity - humidity);
    float distance  = tempMatch + humMatch;
    // Inverse distance weighting with sharpness control
    float match = 1.0f / (distance + 0.001f);
    match = Mathf.Pow(match, biomeBlendingFactor);  // Higher = sharper transitions
    return match;
}

// Normalize weights for all contributing biomes
float totalWeight = 0;
foreach (var biome in allBiomes)
    totalWeight += GetBiomeWeight(biome, temp, humid);
foreach (var biome in allBiomes)
    biome.normalizedWeight = GetBiomeWeight(biome, temp, humid) / totalWeight;
```

### 6.2 Fast Voronoi-Based Blending (NoisePosti.ng)

The KdotJPG approach [15] uses jittered hexagonal grids with normalized sparse convolution for artifact-free blending:

**Weight kernel:**
```
weight(dx, dy) = max(0, radius^2 - dx^2 - dy^2)^2
```

This produces a smooth circular falloff reaching exactly zero at the radius boundary.

**Normalized sparse convolution** ensures weights always sum to 1.0:

```
pseudocode: voronoi_biome_blend(x, y, radius)

  // 1. Find all biome data points within radius
  nearby_points = gather_nearby_points(x, y, radius)

  // 2. Compute raw weights
  total_weight = 0
  for each point p in nearby_points:
    dx = x - p.x
    dy = y - p.y
    dist_sq = dx*dx + dy*dy
    w = max(0, radius*radius - dist_sq)
    w = w * w                          // Square for smooth falloff
    p.weight = w
    total_weight += w

  // 3. Normalize
  inv_total = 1.0 / total_weight
  for each point p in nearby_points:
    p.weight *= inv_total

  // Result: {Forest: 0.6, Plains: 0.4} in transition zones
  return nearby_points  // Each with biome type + normalized weight
```

The output is a set of biome contributions like `{Forest: 0.6, Plains: 0.4}`, which directly drives material blending in shaders.

### 6.3 Multi-Layer Noise Interpolation (PSWG)

The Galaxies: Parzi's Star Wars mod [16] uses a "master noise" to determine biome interpolation:

```
pseudocode: multi_layer_terrain_blend(x, z, layers)

  master = simplex_noise(x * freq, z * freq)   // Single "master" scalar
  total_height = 0

  for i in 0..layers.length:
    // Weight function: triangle wave centered on this layer's position
    //   w = -|( (n-1)*master - i )| + 1
    n = layers.length
    w = max(0, -abs((n - 1) * master - i) + 1.0)

    layer_height = layers[i].generate_height(x, z)
    total_height += layer_height * w

  return total_height
```

This creates smooth, fluid transitions between biome heightmap generators without visible borders.

### 6.4 Noise-Perturbed Biome Boundaries

The simplest approach adds Perlin noise to the biome lookup coordinates, breaking up straight latitude/temperature lines [9]:

```glsl
// GLSL: noisy biome boundaries
uniform float boundaryNoiseScale;
uniform float boundaryNoiseAmplitude;

int getBiome(vec3 worldPos) {
    float temp = computeTemperature(worldPos);
    float moisture = computeMoisture(worldPos);

    // Perturb lookup coordinates with noise
    float n1 = snoise(worldPos * boundaryNoiseScale) * boundaryNoiseAmplitude;
    float n2 = snoise(worldPos * boundaryNoiseScale * 1.7 + 31.5) * boundaryNoiseAmplitude;
    temp += n1;
    moisture += n2;

    return biome_lookup(temp, moisture);
}
```

### 6.5 Shader: Complete Biome Blending

Combining the above techniques into a production shader:

```glsl
// GLSL: biome blending fragment shader
#define MAX_BIOMES 4  // Max contributing biomes per fragment

struct BiomeContribution {
    int   biomeID;
    float weight;
};

uniform sampler2DArray biomeAlbedoArray;  // Texture array: one layer per biome
uniform sampler2DArray biomeNormalArray;
uniform float biomeBumpScale[16];

// Input from CPU: per-vertex biome weights (computed via Voronoi blend)
varying vec4 vBiomeWeights;    // Up to 4 biome weights
varying vec4 vBiomeIndices;    // Corresponding biome IDs (as float)

vec4 blendBiomeMaterials(vec3 worldPos, vec3 worldNormal) {
    vec3 triBlend = getTriPlanarBlend(worldNormal);
    vec3 finalColor = vec3(0.0);
    vec3 finalNormal = vec3(0.0);

    for (int i = 0; i < MAX_BIOMES; i++) {
        float w = vBiomeWeights[i];
        if (w < 0.001) continue;

        float biomeLayer = vBiomeIndices[i];

        // Triplanar sample per biome
        vec3 colX = texture(biomeAlbedoArray, vec3(worldPos.yz, biomeLayer)).rgb;
        vec3 colY = texture(biomeAlbedoArray, vec3(worldPos.xz, biomeLayer)).rgb;
        vec3 colZ = texture(biomeAlbedoArray, vec3(worldPos.xy, biomeLayer)).rgb;
        vec3 biomeColor = colX * triBlend.x + colY * triBlend.y + colZ * triBlend.z;

        vec3 nrmX = texture(biomeNormalArray, vec3(worldPos.yz, biomeLayer)).rgb;
        vec3 nrmY = texture(biomeNormalArray, vec3(worldPos.xz, biomeLayer)).rgb;
        vec3 nrmZ = texture(biomeNormalArray, vec3(worldPos.xy, biomeLayer)).rgb;
        vec3 biomeNrm = nrmX * triBlend.x + nrmY * triBlend.y + nrmZ * triBlend.z;
        biomeNrm = biomeNrm * 2.0 - 1.0;

        finalColor  += biomeColor * w;
        finalNormal += biomeNrm * w;
    }

    finalNormal = normalize(finalNormal);
    return vec4(finalColor, 1.0);
}
```

### 6.6 Height-Based Blend (Texture Bombing)

For more natural transitions at the micro level, height/alpha maps per biome texture can drive blend priority [4]:

```glsl
// Height-based texture blending between two biomes
float heightBlend(float h1, float h2, float blendFactor) {
    float depth = 0.2;
    float ma = max(h1 + (1.0 - blendFactor), h2 + blendFactor) - depth;
    float w1 = max(h1 + (1.0 - blendFactor) - ma, 0.0);
    float w2 = max(h2 + blendFactor - ma, 0.0);
    return w1 / (w1 + w2);  // Returns blend weight for texture 1
}
```

This technique ensures that (for example) rocks poke through grass naturally at transition zones rather than producing a uniform gradient.

---

## 7. References

1. [Whittaker Diagram - Procedural Content Generation Wiki](http://pcg.wikidot.com/pcg-algorithm:whittaker-diagram) -- Overview of Whittaker diagram usage in procedural generation, including Dwarf Fortress and Minecraft applications.

2. [Climate Simulation for Procedural World Generation - Joe Duffy](https://www.joeduffy.games/climate-simulation-for-procedural-world-generation) -- Temperature and precipitation generation using Perlin noise with latitude curves and elevation effects.

3. [Biomes Generation and Rendering - Azgaar](https://azgaar.wordpress.com/2017/06/30/biomes-generation-and-rendering/) -- Rectangular 100x10 biome matrix implementation with 22 biomes, temperature/moisture indexing, and color smoothing.

4. [Terrain Shader Experiments - Red Blob Games](https://www.redblobgames.com/x/1730-terrain-shader-experiments/) -- Barycentric coordinate blending, power-based transitions, noise-biased boundaries, and distance field techniques.

5. [Making Maps with Noise - Red Blob Games](https://www.redblobgames.com/maps/terrain-from-noise/) -- Biome lookup table from elevation/moisture noise, with JavaScript implementation and fractional Brownian motion.

6. [WorldEngine - GitHub](https://github.com/Mindwerks/worldengine) -- Open-source world generator using Holdridge life zones model with ~40 biome types classified by temperature and humidity zones.

7. [Koppen Climate Classification - Wikipedia](https://en.wikipedia.org/wiki/K%C3%B6ppen_climate_classification) -- Complete decision tree with all threshold values for the 30-type Koppen classification system.

8. [Adventures in Procedural Terrain Generation - Sam Mills](https://medium.com/@henchman/adventures-in-procedural-terrain-generation-part-1-b64c29e2367a) -- Temperature from latitude/elevation formula, elevation-based coloring, and moisture-biome decision trees.

9. [Unity Shader Graph Procedural Planet Tutorial - Tim Coster](https://timcoster.com/2020/09/03/unity-shader-graph-procedural-planet-tutorial/) -- RGB gradient biome mapping, noise-perturbed boundaries, SmoothStep transitions, per-biome texture multiplication.

10. [ggbiome: Whittaker Biome Information for R - GitHub](https://github.com/guillembagaria/ggbiome) -- R package implementing Whittaker biome lookup from temperature and precipitation data.

11. [Procedural Generation Techniques for Biome Diversity - peerdh.com](https://peerdh.com/blogs/programming-insights/procedural-generation-techniques-for-biome-diversity-in-terrain-algorithms) -- Perlin/Simplex noise, Voronoi diagrams, linear interpolation blending, cellular automata biome spread.

12. [Poisson Disk Sampling - Dev.Mag](http://devmag.org.za/2009/05/03/poisson-disk-sampling/) -- Bridson's algorithm for O(n) Poisson disk generation, variable-density sampling with greyscale maps.

13. [Procedural World Generation with Biomes in Unity - Emre Safa Baltaci](https://medium.com/@mrrsff/procedural-world-generation-with-biomes-in-unity-a474e11ff0b7) -- Weight-based biome blending with power function sharpness control, inverse distance weighting, and LOD optimization.

14. [GLSL Triplanar Texture Mapping - GitHub Gist](https://gist.github.com/patriciogonzalezvivo/20263fe85d52705e4530) -- Complete GLSL vertex/fragment shader for triplanar projection with normal map support.

15. [Fast Biome Blending Without Squareness - NoisePosti.ng](https://noiseposti.ng/posts/2021-03-13-Fast-Biome-Blending-Without-Squareness.html) -- Voronoi-based biome blending using jittered hexagonal grids and normalized sparse convolution.

16. [Generating Complex Multi-Biome Procedural Terrain with Simplex Noise - PSWG](https://parzivail.com/procedural-terrain-generaion/) -- Master noise weight interpolation for multi-layer terrain blending, eliminates hard biome borders.

17. [Biome and Vegetation PCG - GitHub](https://github.com/GrandPiaf/Biome-and-Vegetation-PCG) -- Academic project combining Poisson disk sampling with per-biome vegetation configuration and biome merging.

18. [AutoBiomes: Procedural Generation of Multi-Biome Landscapes - Springer](https://link.springer.com/article/10.1007/s00371-020-01920-7) -- Academic paper on automated biome generation combining DEM data with simplified climate simulation.
