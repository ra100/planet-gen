# Tectonic Terrain from Plates: Hypsometry, Isostasy, and the Puzzle-Piece Problem

## Research Date: 2026-04-01

## Problem Statement

The current `plates.wgsl` generates continental terrain using a smoothstep dome (`plate_core`) driven by `boundary_dist`. When water level changes, entire plates pop above/below water as uniform puzzle pieces rather than revealing terrain gradually. The root cause: `boundary_dist` creates a monotonic dome shape per plate, so all points at similar distance from the boundary share similar height.

---

## 1. Earth's Hypsometric Curve: The Bimodal Target

Earth's elevation histogram has two distinct peaks:
- **Continental peak**: ~+20m above sea level (broad, ~-500m to +2000m)
- **Oceanic peak**: ~-4300m below sea level (narrower, ~-3000m to -6000m)

This bimodality comes from two different crust types with different densities:
- Continental crust: ~2.7 g/cm3, 25-70 km thick
- Oceanic crust: ~3.0 g/cm3, 5-10 km thick

**Key insight**: The continental peak is NOT at a single elevation. It is a broad distribution centered just above sea level, with significant spread. Mountains and basins create continuous variation within continents. The ocean floor similarly varies. Water level naturally falls in the gap between the two distributions.

**Algorithmic implication**: We need two overlapping Gaussian-like distributions of height, not two flat platforms. The continental distribution should be broad enough that water level changes gradually flood lowlands and reveal highlands.

---

## 2. Isostasy: The Physical Basis for Height

### Airy Isostasy Model

The Airy-Heiskanen model says elevation comes from crustal thickness:

```
elevation = thickness * (1 - rho_crust / rho_mantle)
```

For continental crust (rho_c=2.7, rho_m=3.3):
```
elevation = thickness * (1 - 2.7/3.3) = thickness * 0.182
```

A 35km thick continent stands ~6.4km above a 7km thick ocean floor.

**The key procedural insight**: Instead of assigning height directly from plate type, assign a **crustal thickness** field, then derive height through isostasy. Crustal thickness varies continuously within a plate (thick at orogens, thin at rifts, medium at cratons). This naturally produces varied terrain without puzzle pieces.

### Simplified Isostasy for WGSL

```
h_surface = (thickness - T_ref) * (rho_m - rho_c) / rho_m
```

Where `T_ref` is a reference thickness (the thickness that produces sea-level elevation). This is a simple multiply-add operation, GPU-friendly.

---

## 3. Algorithmic Approaches from Open-Source Projects

### 3a. PlaTec (C++ library)

PlaTec's approach:
1. Start with flat fractal heightmap
2. Split into plates randomly
3. Move plates each timestep
4. **Continental collision**: fold terrain upward (additive height at collision zones)
5. **Subduction**: oceanic plate slides under, slight elevation increase
6. Erosion smoothing between steps

**Height generation**: PlaTec accumulates height through iterative collision. Each collision step adds a small amount of height at the boundary. This avoids the dome problem because height is built up from the boundary inward over many iterations, creating natural falloff.

**Limitation**: Requires iterative simulation (hundreds of steps). Not suitable for single-pass GPU generation.

### 3b. Tectonics.js

Tectonics.js tracks per-cell properties:
- **Crust thickness** (not just "continental" boolean)
- **Crust density** (varies with age and type)
- **Elevation derived from thickness via isostasy**

Continental crust thickness varies from 25-70km. The model interpolates density and thickness from control points based on elevation, creating a feedback loop where thickness drives elevation and elevation constrains thickness.

**Key takeaway**: Replace the binary `plate_type` (0 or 1) with a continuous `crust_thickness` field. Continental plates have thickness 30-70 (varying via noise), oceanic plates have thickness 5-10 (varying via noise). Isostasy converts thickness to elevation.

### 3c. Cortial et al. "Procedural Tectonic Planets" (2019, CGF)

This paper captures the fundamental phenomena procedurally:
- Plate parameters include crust thickness, elevation, and geodetic movement
- Complex phenomena (subduction, collision) deform the lithosphere
- Large-scale model is amplified with procedural detail noise
- **Crust density and thickness are interpolated from control points**

### 3d. Nick McDonald's "Clustered Convection"

GPU-accelerated approach using:
- Dynamic point cloud as plate centroids
- Voronoi clustering on GPU
- Height computed from segment properties at every step
- ~130 lines for convection, ~600 for tectonics

**Relevant technique**: Height is computed per-pixel from the properties of the Voronoi segment it belongs to, not from a global dome function. Each segment carries its own height, and these are blended at boundaries.

### 3e. LeatherBee Games

Assigns each plate:
- Random age
- Average elevation (with noise variation)
- Continental shelf: edges set below sea level with linear slope inward
- +/- 300m noise disrupts coastlines

**Key technique**: Continental plates don't have uniform elevation. A noise field modulates the base elevation across the entire plate, so different regions of the same continent can be above or below water.

### 3f. Red Blob Games (Amit Patel)

- Flood-fill plate assignment (not pure Voronoi) for irregular shapes
- Elevation assigned at boundaries based on convergent/divergent motion
- Interior elevation smoothed inward from boundary values
- Each plate has its own noise-modulated base elevation

---

## 4. The Solution: Crustal Thickness Field + Isostasy

### Core Algorithm

Replace the current `plate_type -> dome height` pipeline with:

```
plate_type + noise -> crustal_thickness -> isostatic_height -> + boundary_features -> final_height
```

### Step-by-step WGSL Implementation

#### Step 1: Per-plate crustal thickness base

Each plate gets a base thickness from its type:
```wgsl
// Continental: 30-45 km base (will produce above-water terrain)
// Oceanic: 6-9 km base (will produce deep ocean floor)
let base_thickness = select(7.0, 38.0, is_continental);
```

#### Step 2: Intra-plate thickness variation (THIS SOLVES THE PUZZLE PIECE PROBLEM)

Apply multiple octaves of noise to vary thickness within each plate. This is the critical difference from the current approach: instead of `boundary_dist` controlling height monotonically, **noise controls thickness variation independently of distance from boundary**.

```wgsl
// Large-scale basins and uplands within the plate
let basin_noise = snoise(raw_pos * 3.0 + seed_offset(params.seed + 1000u));
let basin2 = snoise(raw_pos * 6.0 + seed_offset(params.seed + 1010u)) * 0.5;
let basin3 = snoise(raw_pos * 12.0 + seed_offset(params.seed + 1020u)) * 0.25;
let intra_plate_variation = (basin_noise + basin2 + basin3) / 1.75; // normalized ~-1..+1

// Continental plates have MORE internal variation (basins, plateaus, lowlands)
// Oceanic plates have LESS (mostly uniform abyssal plain)
let variation_amplitude = select(1.5, 12.0, is_continental); // km of thickness variation
let thickness = base_thickness + intra_plate_variation * variation_amplitude;
```

This means a continental plate with base 38km will range from ~26km to ~50km thickness. Through isostasy, that's a height range of roughly -2km to +2.2km — some parts underwater, some above. Water level changes reveal terrain gradually because the height varies continuously.

#### Step 3: Continental margin transition

Near plate boundaries, blend thickness between the two plates. Use `boundary_dist` only for this transition, not for the dome:

```wgsl
// boundary_dist near 0 = at boundary. Transition zone width.
let margin_width = 0.08; // normalized distance
let margin_blend = smoothstep(0.0, margin_width, boundary_dist);

// At the boundary, thickness transitions toward neighbor's type
let neighbor_thickness = select(7.0, 38.0, neighbor_continental);
// Near boundary: blend toward average of both plate thicknesses
// Deep interior: pure plate thickness
let blended_thickness = mix(
    mix(thickness, neighbor_thickness + intra_plate_variation * select(1.5, 12.0, neighbor_continental), 0.5),
    thickness,
    margin_blend
);
```

#### Step 4: Isostatic conversion to elevation

```wgsl
// Simplified Airy isostasy
let rho_crust = select(3.0, 2.7, is_continental); // g/cm3
let rho_mantle = 3.3;
let T_ref = 20.0; // reference thickness for sea level (km)

// Height in km, then normalize to our height units
let h_km = (blended_thickness - T_ref) * (rho_mantle - rho_crust) / rho_mantle;
var height = h_km / 10.0; // scale to ~-0.4..+0.4 range
```

#### Step 5: Boundary features (mountains, trenches, ridges)

Keep the existing boundary feature code (convergent mountains, divergent ridges, etc.) but ADD them on top of the isostatic base. These features represent additional crustal thickening/thinning at boundaries:

```wgsl
// Convergent: thicken crust (mountains) or thin (trench on subducting side)
// Divergent: thin crust (rift) or slightly thicken (mid-ocean ridge magma)
// These modify height ADDITIVELY, same as current code
height += mountain_height * b_influence * convergence * ridge_mask;
height -= trench_depth * b_influence * convergence;
// etc.
```

#### Step 6: Hypsometric shaping (optional refinement)

After computing raw height, apply a transfer function that shapes the distribution to match Earth's bimodal curve:

```wgsl
// Shape the height distribution to be bimodal
// Continental heights: compress around sea level (most land is low-lying)
// Ocean depths: compress around abyssal plain depth
if (height > 0.0) {
    // Power curve keeps most land near sea level, mountains are rare peaks
    height = pow(height, 1.3) * 1.2;
} else {
    // Ocean floor: most is deep, continental shelf is narrow band
    height = -pow(abs(height), 0.85) * 1.1;
}
```

---

## 5. Why This Fixes the Puzzle-Piece Problem

**Current approach**: `height ~ f(boundary_dist)` means all points at the same distance from a plate boundary have similar heights. Entire contour rings pop above water together.

**New approach**: `height ~ isostasy(base_thickness + NOISE)` means height varies across the plate independent of boundary distance. Points at the same boundary distance can have wildly different heights depending on local noise. Water level changes reveal scattered patches: a lowland here, a highland there, naturally.

### Visual comparison:

**Before (dome)**:
```
      ___plateau___
     /              \
    /                \  <- uniform height at same boundary_dist
___/                  \___
   ^boundary           ^boundary
```

**After (thickness + noise)**:
```
    peak    valley   ridge
     /\      v       /\
    /  \    / \     /  \    <- varied heights everywhere
___/    \__/   \___/    \___
   ^boundary               ^boundary
```

---

## 6. Additional Techniques for Realism

### 6a. Per-plate noise seed

Each plate should use a unique noise seed for its internal variation, preventing adjacent plates from having correlated basins:

```wgsl
let plate_seed = params.seed + info.nearest_idx * 137u;
let basin_noise = snoise(raw_pos * 3.0 + seed_offset(plate_seed + 1000u));
```

### 6b. Continental shelf profile

The transition from continental to oceanic crust should include a shelf:
- 0-200m depth: continental shelf (wide, gentle)
- 200-4000m: continental slope (steep)
- 4000m+: abyssal plain

This can be achieved by using a non-linear transfer function on the margin blend:

```wgsl
// Shelf break: fast transition from shallow to deep
let shelf_t = smoothstep(0.02, 0.06, boundary_dist);
let slope_t = smoothstep(0.06, 0.12, boundary_dist);
```

### 6c. Craton stability

The oldest parts of continents (cratons) tend to have stable, moderate elevation. Add a "stability" noise field that reduces extreme elevation in plate interiors:

```wgsl
let stability = smoothstep(0.1, 0.3, boundary_dist); // interior = stable
let dampened_variation = intra_plate_variation * mix(1.0, 0.6, stability);
```

### 6d. Oceanic crust age gradient

Real ocean floor gets deeper away from mid-ocean ridges (as crust cools and contracts). If divergent boundaries are known, ocean depth should increase with distance from them:

```wgsl
// Oceanic crust deepens with distance from ridge
let age_depth = boundary_dist * 0.5; // older = deeper
let oceanic_depth_correction = age_depth * (1.0 - f32(is_continental));
height -= oceanic_depth_correction;
```

---

## 7. Implementation Priority

1. **Phase 1 (High impact)**: Replace dome with thickness field + isostasy. Add per-plate noise variation to thickness. This alone fixes the puzzle-piece problem.

2. **Phase 2 (Polish)**: Add continental shelf/slope profile. Add per-plate unique noise seeds. Tune the hypsometric transfer function.

3. **Phase 3 (Refinement)**: Craton stability damping. Oceanic age-depth relationship. More sophisticated margin blending.

---

## 8. Parameter Mapping

Current `plates.wgsl` parameters and how they map to the new system:

| Current | New Role |
|---------|----------|
| `plate_type` (0/1) | Drives `base_thickness` (7 vs 38 km) |
| `boundary_dist` | Only used for margin blending, NOT for dome height |
| `boundary_type` | Same: drives convergent/divergent features |
| `mountain_scale` | Same: scales boundary feature amplitude |
| `boundary_width` | Same: sigma for boundary influence spread |
| `detail_scale` | Same: fBm detail on top |
| NEW: `thickness_variation` | Controls amplitude of intra-plate noise |

---

## Sources

- [Hypsometry - Wikipedia](https://en.wikipedia.org/wiki/Hypsometry)
- [Earth's hypsometry and what it tells us about global sea level](https://www.sciencedirect.com/science/article/pii/S0012821X2400503X)
- [Hypsographic Curve of Earth's Surface from ETOPO1](https://www.ncei.noaa.gov/sites/default/files/2023-01/Hypsographic%20Curve%20of%20Earth%E2%80%99s%20Surface%20from%20ETOPO1.pdf)
- [Isostasy - Wikipedia](https://en.wikipedia.org/wiki/Isostasy)
- [Airy Isostasy - Geosciences LibreTexts](https://geo.libretexts.org/Courses/University_of_California_Davis/GEL_056:_Introduction_to_Geophysics/Geophysics_is_everywhere_in_geology.../03:_Planetary_Geophysics/3.4:_Isostasy)
- [Isostasy: A lithospheric balancing act](https://www.geological-digressions.com/isostasy-a-lithospheric-balancing-act/)
- [Clustered Convection for Procedural Plate Tectonics - Nick McDonald](https://nickmcd.me/2020/12/03/clustered-convection-for-simulating-plate-tectonics/)
- [Simulating worlds on the GPU - davidar.io](https://davidar.io/post/sim-glsl)
- [Procedural Tectonic Planets (Cortial et al. 2019)](https://hal.science/hal-02136820/file/2019-Procedural-Tectonic-Planets.pdf)
- [PlaTec - plate-tectonics library](https://github.com/Mindwerks/plate-tectonics)
- [Tectonics.js](https://github.com/davidson16807/tectonics.js/)
- [Terrain Generation 4: Plates, Continents, Coasts - LeatherBee](https://leatherbee.org/index.php/2018/10/28/terrain-generation-4-plates-continents-coasts/)
- [Terrain Generation 5: Fault Features - LeatherBee](https://leatherbee.org/index.php/2018/11/19/terrain-generation-5-fault-features/)
- [Procedural map generation on a sphere - Red Blob Games](https://www.redblobgames.com/x/1843-planet-generation/)
- [Procedural World Gen with Plate Tectonics - Hackaday](https://hackaday.io/project/196161-arduino-minecraft/log/229957-procedural-world-gen-with-plate-tectonics)
- [Procedural planet heightmap generation - raguilar011095](https://github.com/raguilar011095/planet_heightmap_generation)
- [SimpleTectonics - weigert](https://github.com/weigert/SimpleTectonics)
- [Physically Based Terrain Generation (thesis)](https://core.ac.uk/download/pdf/38053567.pdf)
