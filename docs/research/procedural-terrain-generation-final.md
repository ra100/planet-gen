# Procedural Terrain Generation Algorithms -- Deep Research

**Date**: 2026-03-27
**Scope**: Noise functions, fractal methods, erosion simulation, mathematical foundations for planet surface generation

---

## 1. Noise Functions

### 1.1 Perlin Noise (Original 1983/1985)

**Original paper**: Ken Perlin, "An Image Synthesizer," Proceedings of the 12th Annual Conference on Computer Graphics and Interactive Techniques (SIGGRAPH '85), San Francisco, CA, pp. 287-296, ACM, 1985.

- **URL**: https://www.sci.utah.edu/~kpotter/Library/Papers/perlin:1985:IS/
- **Year**: 1985 (algorithm developed 1982-1983, published 1985)

Perlin developed the noise function in 1982 after working on Disney's *Tron*, frustrated by the "machine-like" look of CGI. The algorithm:

1. Define a grid of random **gradient vectors** at integer lattice points
2. For any input point, find the surrounding grid cell
3. Compute **dot products** between gradient vectors and offset vectors from corners to the point
4. **Interpolate** using a smooth hermite curve: `s(t) = 3t^2 - 2t^3`

**Algorithm** (2D):
```
function noise(x, y):
    // Determine grid cell
    x0, y0 = floor(x), floor(y)
    x1, y1 = x0+1, y0+1
    // Interpolation weights
    sx = x - x0; sy = y - y0
    // Dot products at four corners
    n00 = dot(gradient[x0,y0], (sx, sy))
    n10 = dot(gradient[x1,y0], (sx-1, sy))
    n01 = dot(gradient[x0,y1], (sx, sy-1))
    n11 = dot(gradient[x1,y1], (sx-1, sy-1))
    // Bilinear interpolation with smoothstep
    u = smoothstep(sx); v = smoothstep(sy)
    return lerp(lerp(n00,n10,u), lerp(n01,n11,u), v)
```

Output range: approximately [-1, 1] (unbounded in theory; practical range ~[-0.7, 0.7] in 2D).

### 1.2 Improved Perlin Noise (2002)

**Paper**: Ken Perlin, "Improving Noise," Proceedings of the 29th Annual Conference on Computer Graphics and Interactive Techniques (SIGGRAPH '02), pp. 681-682, ACM, 2002.

- **ACM DL**: https://dl.acm.org/doi/10.1145/566654.566636
- **Direct PDF**: https://mrl.cs.nyu.edu/~perlin/paper445.pdf
- **Reference implementation**: https://cs.nyu.edu/~perlin/noise/

Two key improvements:
1. **Replaced hermite interpolation** `3t^2 - 2t^3` with quintic `6t^5 - 15t^4 + 10t^3` -- eliminates second-order derivative discontinuity at lattice boundaries
2. **Optimized gradient set** -- uses 12 gradient vectors (edges of a cube) instead of random unit vectors, eliminating directional bias artifacts

The improved version is what most modern implementations use.

### 1.3 Simplex Noise (2001)

**Paper**: Ken Perlin, "Noise Hardware," Real-Time Shading course notes, SIGGRAPH 2001.

- **Wikipedia**: https://en.wikipedia.org/wiki/Simplex_noise
- **Patent**: US6867776B2 (filed 2002, granted 2005, expired January 2022)

Key advantages over classic Perlin noise:
- **Complexity**: O(n^2) in n dimensions vs. O(n * 2^n) for classic Perlin
- **Simplex grid**: Uses simplices (triangles in 2D, tetrahedra in 3D) instead of hypercubes
- **No directional artifacts**: Fewer visual biases
- **Analytically computable gradients**: Important for normal computation on terrain

The simplex grid is obtained by **skewing** the input space, transforming the regular simplex lattice into a form traversable with integer coordinates.

**Demystified reference**: Stefan Gustavson, "Simplex Noise Demystified," 2005.
- **PDF**: https://cgvr.cs.uni-bremen.de/teaching/cg_literatur/simplexnoise.pdf

Gustavson's paper provides a complete walkthrough of the algorithm from 1D through 4D with Java reference code.

### 1.4 OpenSimplex2

**Repository**: KdotJPG, "OpenSimplex2: Successors to OpenSimplex Noise, plus updated OpenSimplex."

- **URL**: https://github.com/KdotJPG/OpenSimplex2
- **License**: CC0 (public domain)

Created to avoid Perlin's patent (now expired). Two variants:

| Variant | Character | Best for |
|---------|-----------|----------|
| **OpenSimplex2(F)** | Looks most like Simplex, comparable speed | General fBm, standard terrain |
| **OpenSimplex2(S)** | Smoother, like 2014 OpenSimplex | Ridged noise (abs value thresholding) |

- 3D uses a **rotated body-centered-cubic grid** as the offset union of two rotated cubic grids
- Improved probability symmetry in gradient vector tables
- Eliminates diagonal banding artifacts present in legacy implementations beyond 3D
- Available in Java, C#, C, GLSL, HLSL

### 1.5 Value Noise

Value noise is the simplest coherent noise function:

1. Assign **random scalar values** at each lattice point (not gradient vectors)
2. **Interpolate** between these values (bilinear, bicubic, or hermite)

**Characteristics**:
- Faster to compute than gradient noise
- Produces a **blocky** appearance due to axis-aligned interpolation
- Lower visual quality -- frequency content tends to cluster around lattice alignment
- Output range: [0, 1] (or [-1, 1] if centered)

**Reference**: Scratchapixel, "Value Noise and Procedural Patterns: Part 1"
- **URL**: https://www.scratchapixel.com/lessons/procedural-generation-virtual-worlds/procedural-patterns-noise-part-1/creating-simple-1D-noise.html

### 1.6 Gradient Noise

Gradient noise is the generalized category encompassing Perlin noise and Simplex noise. The key difference from value noise:

- **Value noise**: Interpolates between random **values** at lattice points
- **Gradient noise**: Interpolates between **dot products** of random gradient vectors and offset vectors

This produces smoother, less blocky results with more energy in high frequencies and fewer alignment artifacts.

**Reference**: Wikipedia, "Gradient noise"
- **URL**: https://en.wikipedia.org/wiki/Gradient_noise

### 1.7 Worley / Voronoi / Cellular Noise (1996)

**Paper**: Steven Worley, "A Cellular Texture Basis Function," Proceedings of the 23rd Annual Conference on Computer Graphics and Interactive Techniques (SIGGRAPH '96), pp. 291-294, ACM, 1996.

- **ACM DL**: https://dl.acm.org/doi/10.1145/237170.237267
- **SIGGRAPH history**: https://history.siggraph.org/learning/a-cellular-texture-basis-function-by-worley/

**Algorithm**:
1. Scatter **feature points** randomly in space (using a seeded hash per cell)
2. For any input point, find the **distances to the nearest N feature points**
3. Combine distances to create different patterns:
   - **F1** (nearest): Voronoi cell interiors
   - **F2** (second nearest): Rounded cell shapes
   - **F2 - F1**: Cell edges / cracks
   - **F1 * F2**: Organic patterns

**Applications in terrain**: Craters, dried mud/lava cracks, flagstone patterns, mountain range boundaries, tectonic plate edges.

The function can be computed efficiently without precalculation or table storage by hashing cell coordinates to determine feature point positions.

---

## 2. Fractal Brownian Motion (fBm)

### Core Concept

fBm combines multiple **octaves** of noise at increasing frequencies and decreasing amplitudes to create self-similar detail across scales.

**Reference**: The Book of Shaders, "Fractal Brownian Motion" (Chapter 13)
- **URL**: https://thebookofshaders.com/13/
- **Author**: Patricio Gonzalez Vivo

**Reference**: Axel Paris, "Noise for Terrains -- Learn Procedural Generation"
- **URL**: https://aparis69.github.io/LearnProceduralGeneration/terrain/procedural/noise_for_terrains/

### Formula

```
fBm(p) = SUM(i=0 to octaves-1) [ amplitude_i * noise(p * frequency_i) ]

where:
  frequency_i = frequency_0 * lacunarity^i
  amplitude_i = amplitude_0 * gain^i
```

### Parameters

| Parameter | Definition | Typical Range | Default |
|-----------|-----------|---------------|---------|
| **Octaves** | Number of noise layers summed | 4-12 | 6-8 |
| **Lacunarity** | Frequency multiplier per octave | 1.5-3.0 | 2.0 |
| **Gain** (persistence) | Amplitude multiplier per octave | 0.25-0.75 | 0.5 |
| **Frequency** | Base frequency (initial scale) | 0.001-0.1 | Depends on world scale |
| **Amplitude** | Base amplitude | 0.5-1.0 | 1.0 |

### Pseudocode

```javascript
function fbm(x, y) {
    var value = 0.0;
    var amplitude = 1.0;
    var frequency = baseFrequency;

    for (var i = 0; i < octaves; i++) {
        value += amplitude * noise(x * frequency, y * frequency);
        frequency *= lacunarity;  // typically 2.0
        amplitude *= gain;        // typically 0.5
    }
    return value;
}
```

### Parameter Effects on Terrain

- **Low lacunarity (1.5)**: Overlapping frequency bands, smoother rolling hills
- **High lacunarity (3.0+)**: Distinct scale jumps, more varied texture
- **Low gain (0.25)**: First octave dominates, smooth terrain with minor detail
- **High gain (0.75)**: High-frequency detail nearly as strong as base, rough/jagged terrain
- **6-8 octaves**: Good balance of detail vs. computation for most terrain
- **1-2 octaves**: Broad shapes only (good for continent masks)

**Reference**: Game Developer, "Sponsored Feature: Procedural Terrain Generation With Fractional Brownian Motion"
- **URL**: https://www.gamedeveloper.com/programming/sponsored-feature-procedural-terrain-generation-with-fractional-brownian-motion

---

## 3. Domain Warping

### Core Technique

Domain warping distorts the input coordinates of a noise function using another noise function before evaluation, replacing `f(p)` with `f(g(p))`.

**Primary reference**: Inigo Quilez, "Domain Warping"
- **URL**: https://iquilezles.org/articles/warp/
- **Author**: Inigo Quilez (demoscene, Shadertoy creator)

### Formulas

**Single warp**:
```
f(p) = fbm(p + fbm(p))
```

**Double warp** (recommended for terrain):
```
q.x = fbm(p + vec2(0.0, 0.0))
q.y = fbm(p + vec2(5.2, 1.3))
f(p) = fbm(p + 4.0 * q)
```

**Triple warp** (maximum complexity):
```
q.x = fbm(p + vec2(0.0, 0.0))
q.y = fbm(p + vec2(5.2, 1.3))

r.x = fbm(p + 4.0*q + vec2(1.7, 9.2))
r.y = fbm(p + 4.0*q + vec2(8.3, 2.8))

f(p) = fbm(p + 4.0 * r)
```

### Key Parameters

- **Warp amplitude**: The `4.0` multiplier controls distortion intensity. Range 1.0-8.0.
- **Offset vectors**: Arbitrary constants (e.g., `(5.2, 1.3)`) ensure each fBm evaluation samples different regions, preventing correlation artifacts
- **Intermediate values** (`q`, `r`) can be used for coloring/biome selection

### Effects on Terrain

- Creates natural-looking **ridges, valleys, and organic flow patterns**
- Produces features resembling real erosion channels and geological folding
- The warped domain breaks up the regular structure of raw noise
- Time parameter can animate cloud/lava flow patterns

**Additional reference**: Mathias Isaksen, "Domain Warping: An Interactive Introduction"
- **URL**: https://st4yho.me/domain-warping-an-interactive-introduction/

---

## 4. Ridge Noise / Ridged Multifractal

### Basic Ridge Noise

Ridge noise creates sharp mountain-like features by taking the absolute value of noise and inverting it:

```
ridge(p) = 1.0 - abs(noise(p))
```

Or equivalently:
```
ridge(p) = offset - abs(noise(p))
```

where `offset` is typically 1.0. This creates sharp peaks where the noise crosses zero.

### Ridged Multifractal (Musgrave)

**Primary reference**: F. Kenton Musgrave, "Procedural Fractal Terrains," from *Texturing and Modeling: A Procedural Approach* (3rd ed.), Morgan Kaufmann, 2002.

- **Course notes PDF**: https://www.classes.cs.uchicago.edu/archive/2015/fall/23700-1/final-project/MusgraveTerrain00.pdf
- **Book**: https://books.google.com/books/about/Texturing_and_Modeling.html?id=fXp5UsEWNX8C

**Algorithm**:
```
function ridgedMultifractal(p, H, lacunarity, octaves, offset, gain):
    result = 0.0
    frequency = 1.0
    weight = 1.0

    for i in range(octaves):
        signal = offset - abs(noise(p * frequency))
        signal *= signal          // sharpen the ridges
        signal *= weight          // weight by previous octave
        weight = clamp(signal * gain, 0.0, 1.0)

        result += signal * frequency^(-H)
        frequency *= lacunarity

    return result
```

### Parameters

| Parameter | Definition | Typical Value |
|-----------|-----------|---------------|
| **H** | Hurst exponent (fractal roughness) | 0.5-1.0 (0.9 for sharp ridges) |
| **Lacunarity** | Frequency multiplier | 2.0-2.5 |
| **Octaves** | Number of layers | 6-8 |
| **Offset** | Ridge height baseline | 1.0 |
| **Gain** | Weight feedback factor | 2.0 |

### Musgrave's Terrain Types

Musgrave defined several fractal terrain models:

1. **Monofractal fBm**: Uniform fractal dimension everywhere. Good for generic terrain.
2. **Hybrid multifractal**: Space-varying fractal dimension. Low areas smooth, high areas rough. More realistic than uniform fBm.
3. **Ridged multifractal**: Sharp ridges at zero crossings. Excellent for mountain ranges.
4. **Hetero terrain**: Heterogeneous terrain with altitude-dependent roughness.

Key insight: **Multifractals** have space-varying fractal dimensions and more closely resemble real-world terrains than monofractals. Real terrain is rougher at high altitudes (exposed rock) and smoother at low altitudes (sediment deposition).

### GLSL Turbulence Variant

From The Book of Shaders:
```glsl
for (int i = 0; i < OCTAVES; i++) {
    value += amplitude * abs(snoise(st));
    st *= 2.0;
    amplitude *= 0.5;
}
```

**Reference**: OpenSimplex2 recommends the **S variant** for ridged noise applications, as its smoother profile handles absolute-value thresholding better.

---

## 5. Erosion Simulation

### 5.1 Hydraulic Erosion (Cell-Based / Pipe Model)

**Key paper**: Xing Mei, Philippe Decaudin, Bao-Gang Hu, "Fast Hydraulic Erosion Simulation and Visualization on GPU," 15th Pacific Conference on Computer Graphics and Applications (PG '07), Maui, Hawaii, October 2007.

- **HAL archive**: https://hal.science/inria-00402079
- **PDF**: https://inria.hal.science/inria-00402079/document
- **ResearchGate**: https://www.researchgate.net/publication/4295561_Fast_Hydraulic_Erosion_Simulation_and_Visualization_on_GPU

**Algorithm overview** (per timestep):
1. **Water increment**: Add water from rain/sources: `d(x,y) += dt * r(x,y)`
2. **Flow simulation** (pipe model): Calculate water flux through virtual pipes between neighboring cells based on hydrostatic pressure differences:
   ```
   flux_L = max(0, flux_L + dt * A * g * dh_L / l)
   ```
   where `A` = pipe cross-section, `g` = gravity, `dh` = height difference, `l` = pipe length
3. **Velocity field**: Derive velocity from flux differences
4. **Erosion/deposition**: Based on sediment capacity:
   ```
   sediment_capacity = Kc * sin(slope_angle) * |velocity|
   if (sediment < capacity):
       dissolve = Ks * (capacity - sediment)    // erosion
   else:
       deposit = Kd * (sediment - capacity)     // deposition
   ```
5. **Sediment transport**: Advect sediment using velocity field
6. **Evaporation**: `d(x,y) *= (1 - Ke * dt)`

**Parameters**: Kc (sediment capacity constant), Ks (dissolving constant), Kd (deposition constant), Ke (evaporation rate).

### 5.2 Hydraulic Erosion (Interactive, Multi-Layer)

**Paper**: Ondrej Stava, Bedrich Benes, et al., "Interactive Terrain Modeling Using Hydraulic Erosion," Proceedings of the 2008 ACM SIGGRAPH/Eurographics Symposium on Computer Animation (SCA '08).

- **PDF**: https://cgg.mff.cuni.cz/~jaroslav/papers/2008-sca-erosim/2008-sca-erosiom-fin.pdf
- **ACM DL**: https://dl.acm.org/doi/abs/10.5555/1632592.1632622

Features:
- Terrain composed of **multiple material layers** with different erosion resistance
- Couples two erosion modes: **dissolution** (slow water) and **force-based** (fast water)
- Includes bank collapse / slippage when undercutting occurs
- Based on 2D **shallow water equations**
- GPU implementation at 20+ fps on 2048x1024 grids

### 5.3 Particle-Based Hydraulic Erosion

**Reference**: Nick McDonald, "Simple Particle-Based Hydraulic Erosion," 2020.
- **URL**: https://nickmcd.me/2020/04/10/simple-particle-based-hydraulic-erosion/

**Reference**: Sebastian Lague, "Hydraulic Erosion" (Unity implementation).
- **Interactive demo**: https://sebastian.itch.io/hydraulic-erosion
- **GitHub**: https://github.com/SebLague/Hydraulic-Erosion

**Algorithm** (per droplet):
```
for each droplet:
    position = random_surface_point()
    velocity = (0, 0, 0)
    sediment = 0
    volume = initial_volume

    for step in range(max_lifetime):
        // 1. Calculate surface normal
        normal = surface_normal_at(position)

        // 2. Accelerate by slope
        acceleration = gravity_component * normal / mass
        velocity += acceleration * dt
        velocity *= (1 - friction * dt)

        // 3. Calculate sediment capacity
        c_eq = volume * speed * max(heightDifference, min_slope)

        // 4. Erode or deposit
        if sediment < c_eq:
            erode = erosion_rate * (c_eq - sediment)
            heightmap[pos] -= erode
            sediment += erode
        else:
            deposit = deposition_rate * (sediment - c_eq)
            heightmap[pos] += deposit
            sediment -= deposit

        // 5. Move and evaporate
        position += velocity * dt
        volume *= (1 - evaporation_rate * dt)

        if out_of_bounds or volume < min_volume:
            break
```

**Typical parameters** (Sebastian Lague implementation):
- Droplets: 50,000-200,000
- Max lifetime: 30-64 steps
- Erosion rate: 0.3
- Deposition rate: 0.3
- Evaporation rate: 0.01
- Sediment capacity factor: 4.0
- Min slope: 0.01
- Inertia: 0.05 (blends old direction with gradient)

### 5.4 Thermal Erosion

**Reference**: Axel Paris, "Terrain Erosion on the GPU"
- **URL**: https://aparis69.github.io/public_html/posts/terrain_erosion.html

**Reference**: Balazs Jako, "Fast Hydraulic and Thermal Erosion on the GPU," CESCG 2011.
- **PDF**: https://old.cescg.org/CESCG-2011/papers/TUBudapest-Jako-Balazs.pdf

**Core principle**: Material moves downhill when the slope exceeds the **talus angle** (angle of repose).

**Algorithm**:
```
for each cell (x, y):
    h = heightmap[x, y]
    max_diff = 0

    for each neighbor n:
        diff = h - heightmap[n]
        if diff > max_diff:
            max_diff = diff
            steepest = n

    if max_diff / cell_size > tan(talus_angle):
        amount = dt * (max_diff - tan(talus_angle) * cell_size) * 0.5
        heightmap[x,y] -= amount
        heightmap[steepest] += amount
```

**Typical talus angles**:
- Sand: ~33 degrees (tan = 0.65)
- Gravel: ~35-40 degrees
- Rock debris: ~40-45 degrees

Thermal erosion smooths sharp features and creates scree slopes at the base of cliffs. It is complementary to hydraulic erosion.

### 5.5 Coastal Erosion

**Paper**: "NEWTS1.0: Numerical model of coastal Erosion by Waves and Transgressive Scarps," Geoscientific Model Development, 2024.
- **URL**: https://gmd.copernicus.org/articles/17/3433/2024/

**Reference**: Teoh, "River and Coastal Action in Automatic Terrain Generation"
- **Semantic Scholar**: https://www.semanticscholar.org/paper/River-and-Coastal-Action-in-Automatic-Terrain-Teoh/316be57e56662a0113a5678eb29dd5b3b951694a

Coastal erosion simulation steps per timestep:
1. **Sea-level change**: Adjust water level
2. **Wave erosion**: Fetch-dependent erosion (longer fetch = bigger waves = more erosion)
3. **Uniform erosion**: Background erosion rate

For procedural terrain, a simplified approach models wave energy as proportional to fetch distance and applies erosion perpendicular to the coastline.

### 5.6 Snowball/Simplified Erosion

**Reference**: Job Talle, "Simulating Hydraulic Erosion"
- **URL**: https://jobtalle.com/simulating_hydraulic_erosion.html

A simplified "snowball" model:
- Drops 50,000 snowballs at random positions
- Each rolls downhill accumulating/depositing sediment
- Deposit rate: `sediment * depositRate * surfaceNormal.y`
- Erosion rate: `erosionRate * (1 - surfaceNormal.y) * min(1, i * iterationScale)`
- Post-process with Gaussian blur to smooth results

---

## 6. Diamond-Square Algorithm

**Original paper**: Alain Fournier, Don Fussell, Loren Carpenter, "Computer Rendering of Stochastic Models," Communications of the ACM, Vol. 25, No. 6, pp. 371-384, June 1982.

- **ACM DL**: https://dl.acm.org/doi/10.1145/358523.358553
- **ResearchGate**: https://www.researchgate.net/publication/220425247_Computer_Rendering_of_Stochastic_Models

**Reference**: Wikipedia, "Diamond-square algorithm"
- **URL**: https://en.wikipedia.org/wiki/Diamond-square_algorithm

### Requirements

- Grid size must be **(2^n + 1) x (2^n + 1)** (e.g., 129x129, 257x257, 513x513, 1025x1025)
- Four corner values must be pre-seeded

### Algorithm

```
function diamondSquare(grid, size, roughness):
    // Seed four corners
    grid[0][0] = random()
    grid[0][size-1] = random()
    grid[size-1][0] = random()
    grid[size-1][size-1] = random()

    step = size - 1
    scale = roughness

    while step > 1:
        half = step / 2

        // DIAMOND STEP
        for x in range(0, size-1, step):
            for y in range(0, size-1, step):
                avg = (grid[x][y] + grid[x+step][y] +
                       grid[x][y+step] + grid[x+step][y+step]) / 4
                grid[x+half][y+half] = avg + random(-scale, scale)

        // SQUARE STEP
        for x in range(0, size, half):
            for y in range((x+half) % step, size, step):
                avg = average of up-to-4 diamond neighbors
                grid[x][y] = avg + random(-scale, scale)

        step = half
        scale *= 0.5  // reduce randomness each iteration (persistence)
```

### Characteristics

- **Fast**: O(n^2) for n grid points
- **Produces creasing artifacts** along axis-aligned lines at coarser subdivision levels
- The **scale reduction factor** (0.5 default) controls roughness:
  - 0.3-0.4: Very smooth terrain
  - 0.5: Standard terrain
  - 0.6-0.8: Very rough/jagged terrain
- Related to fractional Brownian motion with H = -log2(scale_factor)
- **Improvement over** simple midpoint displacement (which uses only 2 source points per step instead of 4)
- Cannot easily tile or wrap; boundary conditions require special handling

---

## 7. Mathematical Foundations

### 7.1 Fractal Dimension of Real Terrain

**Reference**: Wikipedia, "Fractal landscape"
- **URL**: https://en.wikipedia.org/wiki/Fractal_landscape

**Reference**: Mandelbrot, *The Fractal Geometry of Nature*, W.H. Freeman, 1982.

Real terrain surfaces have a **Hausdorff dimension** between 2.0 (perfectly flat) and 3.0 (space-filling). For Earth:

- **Typical fractal dimension**: D = 2.1 to 2.5
- **Mountain ranges**: D ~ 2.2 to 2.4
- **Heavily eroded terrain**: D ~ 2.1 to 2.2
- **Volcanic terrain**: D ~ 2.3 to 2.5

**Important caveat**: Real terrain exhibits fractal behavior over only about **2 orders of magnitude** of scale (Richardson's finding on Britain's coastline). Attempts to calculate a single "overall" fractal dimension of real landscapes can produce nonsensical results. Multi-fractal models are more appropriate.

### 7.2 Hurst Exponent

**Reference**: Wikipedia, "Hurst exponent"
- **URL**: https://en.wikipedia.org/wiki/Hurst_exponent

**Reference**: ScienceDirect, "Fractal dimensions of terrain profiles," 1991.
- **URL**: https://www.sciencedirect.com/science/article/abs/pii/002248989190030A

The Hurst exponent H relates to fractal dimension D of a surface profile:

```
D = 2 - H     (for 1D profiles)
D = 3 - H     (for 2D surfaces)
```

**For Earth terrain**:
- **H = 0.5 to 0.8** (typical range for topographic profiles)
- **H = 0.5**: Brownian motion, very rough terrain
- **H = 0.7**: Moderate smoothness, good approximation for many landscapes
- **H = 0.8**: Smoother, more correlated terrain (gentle hills)
- **H = 1.0**: Perfectly smooth, differentiable surface

Higher H = smoother trend, less volatility, less roughness. H values for real terrain indicate **persistent** behavior (positive autocorrelation) -- high points tend to be near other high points.

### 7.3 Power Spectrum of Natural Terrain (1/f Noise)

**Reference**: Paul Bourke, "Noise, Perlin, 1/f Noise, Modelling Planets"
- **URL**: https://paulbourke.net/fractals/noise/

The power spectrum of natural terrain follows:

```
P(f) proportional to 1/f^beta
```

where:
- **beta = 0**: White noise (uncorrelated random)
- **beta = 1**: Pink noise / 1/f noise (common in nature)
- **beta = 2**: Brownian noise / red noise (random walk)
- **beta ~ 2.0-2.5**: Typical for Earth terrain elevation profiles

**Relationship to fractal dimension**:
```
D = (5 - beta) / 2       (for 2D surfaces)
```

So for beta = 2.0: D = 1.5 (1D profile dimension). For beta = 2.4: D = 1.3.

This means terrain generation should produce noise with spectral power falling off approximately as f^(-2) to f^(-2.5).

### 7.4 Spectral Synthesis

**Reference**: RISC JKU, "Fractal Landscapes via FFT"
- **URL**: https://www3.risc.jku.at/education/courses/ws2016/cas/landscape.html

**Reference**: Williams College, "Fractal Terrain Generation Methods"
- **URL**: https://web.williams.edu/Mathematics/sjmiller/public_html/hudson/Dickerson_Terrain.pdf

**Algorithm**:
1. Generate NxN grid of **Gaussian white noise** (zero mean, unit variance)
2. Apply **FFT** (Fast Fourier Transform) to get frequency domain
3. **Filter** by multiplying each coefficient by `1/f^(beta/2)` where f = sqrt(fx^2 + fy^2)
4. Apply **inverse FFT** to get terrain heightmap

**Advantages**:
- Exact control over spectral properties
- Naturally tiles (periodic boundary conditions)
- Fast: O(N^2 log N)

**Disadvantages**:
- Produces statistically homogeneous terrain (same roughness everywhere)
- Tiling can be visible
- No local control over features

### 7.5 Variogram Models

**Reference**: Springer Nature, "Flexible spectral methods for the generation of random fields with power-law semivariograms," Mathematical Geosciences.
- **URL**: https://link.springer.com/article/10.1007/BF02768904

The **variogram** (or semivariogram) gamma(h) measures spatial correlation as a function of lag distance h:

```
gamma(h) = (1/2) * E[(Z(x+h) - Z(x))^2]
```

For fractal terrain, the variogram follows a **power law**:
```
gamma(h) proportional to h^(2H)
```

where H is the Hurst exponent. Four spectral methods for generating fields with power-law variograms:
1. **Simple Fourier method**: Direct but can have incorrect variance
2. **Randomization method**: Good for 4-6 orders of magnitude
3. **Hybrid method**: Best for broad scale ranges
4. **Fourier-Wavelet method**: Good multi-scale properties

### 7.6 Statistical Self-Similarity

Real terrain exhibits **statistical self-similarity**: zooming into a portion of terrain produces features statistically similar to the whole. This is the mathematical basis for using fractal noise in terrain generation.

However, real terrain is **not perfectly self-similar**:
- Large scales: tectonic forces, plate boundaries (non-fractal)
- Medium scales: erosion-dominated (approximately fractal, H ~ 0.5-0.8)
- Small scales: material properties, vegetation (different statistics)

Multi-fractal approaches address this by using **altitude-dependent fractal dimension**: rough at peaks, smooth in valleys (matching erosion physics where sediment fills valleys and exposed rock is rough).

---

## 8. Diffusion-Limited Aggregation (DLA) for River Networks

### Original DLA Paper

**Paper**: T.A. Witten and L.M. Sander, "Diffusion-Limited Aggregation, a Kinetic Critical Phenomenon," Physical Review Letters, Vol. 47, No. 19, pp. 1400-1403, November 9, 1981.

- **APS**: https://link.aps.org/doi/10.1103/PhysRevLett.47.1400
- **ADS**: https://ui.adsabs.harvard.edu/abs/1981PhRvL..47.1400W
- **DOI**: 10.1103/PhysRevLett.47.1400

**Core model**: Particles undergo random walks and stick upon contact with an existing aggregate, producing dendritic (branching) fractal structures.

### Application to River Networks

**Paper**: "A diffusion-limited aggregation model for the evolution of drainage networks," Earth and Planetary Science Letters, 1993.
- **ScienceDirect**: https://www.sciencedirect.com/science/article/pii/0012821X9390145Y

**Paper**: "Growth diffusion-limited aggregation for basin fractal river network evolution model," AIP Advances, 2020.
- **URL**: https://pubs.aip.org/aip/adv/article/10/7/075317/240923/Growth-diffusion-limited-aggregation-for-basin

**Modified DLA for drainage networks**:
1. Start with a seed point (river mouth / ocean boundary)
2. Launch random walkers from random positions
3. Each walker performs a 2D random walk until it contacts the existing drainage network
4. The walker accretes to the network at the contact point
5. Repeat to grow the network

**Key results**:
- Produces drainage patterns **remarkably similar to actual river networks**
- **Bifurcation ratio** Rb = 3.98 (observed real-world: ~3.5-5.0)
- **Length ratio** Rr = 2.09
- **Network fractal dimension**: D = log(Rb) / log(Rr) = **1.87** (observed: ~1.80-1.85)
- Matches Horton's laws of stream ordering

### Practical Implementation for Terrain

For terrain generation, DLA-generated river networks can be used as:
1. **Constraint networks**: Generate river tree first, then carve terrain to match
2. **Erosion guides**: Use DLA pattern to direct particle-based erosion
3. **Voronoi-based**: Place DLA network on a Voronoi graph (as in Amit Patel's mapgen2)

**Reference**: Amit Patel / Red Blob Games, "Polygonal Map Generation for Games"
- **URL**: http://www-cs-students.stanford.edu/~amitp/game-programming/polygon-map-generation/

---

## 9. Noise Color Spectrum Reference

**Reference**: Red Blob Games, "Noise Functions and Map Generation"
- **URL**: https://www.redblobgames.com/articles/noise/introduction.html

| Noise Color | Amplitude ~ f^? | Beta | Character | Terrain Use |
|-------------|-----------------|------|-----------|-------------|
| Violet | f^(+1) | -2 | Very high-frequency dominant | N/A |
| Blue | f^(+0.5) | -1 | High-frequency dominant | Object placement |
| White | f^(0) | 0 | Equal all frequencies | Raw random |
| Pink | f^(-0.5) | 1 | Balanced | Moderate terrain |
| Red/Brown | f^(-1) | 2 | Low-frequency dominant | Terrain elevation |

For terrain: **red/Brownian noise** (beta ~ 2) is most natural because low frequencies (large features) dominate, matching how geological processes work.

---

## 10. Summary: Source Registry

| # | Source | Year | Topic |
|---|--------|------|-------|
| 1 | Perlin, "An Image Synthesizer," SIGGRAPH '85 | 1985 | Original Perlin noise |
| 2 | Perlin, "Improving Noise," SIGGRAPH '02 | 2002 | Improved Perlin noise |
| 3 | Perlin, "Noise Hardware," SIGGRAPH '01 | 2001 | Simplex noise |
| 4 | Gustavson, "Simplex Noise Demystified" | 2005 | Simplex explanation |
| 5 | KdotJPG, OpenSimplex2 (GitHub) | 2019+ | Patent-free noise |
| 6 | Worley, "A Cellular Texture Basis Function," SIGGRAPH '96 | 1996 | Cellular/Voronoi noise |
| 7 | Scratchapixel, "Value Noise and Procedural Patterns" | -- | Value noise tutorial |
| 8 | The Book of Shaders, Chapter 13 (Gonzalez Vivo) | -- | fBm tutorial |
| 9 | Paris, "Noise for Terrains" | -- | fBm parameters for terrain |
| 10 | Game Developer, "Procedural Terrain Generation with fBm" | -- | fBm terrain guide |
| 11 | Quilez, "Domain Warping" (iquilezles.org) | -- | Domain warping technique |
| 12 | Ebert, Musgrave, et al., *Texturing and Modeling* (3rd ed.) | 2002 | Ridged multifractal |
| 13 | Musgrave, "Procedural Fractal Terrains" (course notes) | 2000 | Terrain algorithms |
| 14 | Mei et al., "Fast Hydraulic Erosion on GPU," PG '07 | 2007 | Pipe-model erosion |
| 15 | Stava et al., "Interactive Terrain Modeling," SCA '08 | 2008 | Multi-layer erosion |
| 16 | McDonald, "Simple Particle-Based Hydraulic Erosion" | 2020 | Droplet erosion |
| 17 | Lague, Hydraulic Erosion (GitHub) | 2019 | Unity erosion impl. |
| 18 | Jako, "Fast Hydraulic and Thermal Erosion on GPU," CESCG '11 | 2011 | GPU thermal erosion |
| 19 | Talle, "Simulating Hydraulic Erosion" | -- | Snowball erosion |
| 20 | Fournier, Fussell, Carpenter, "Computer Rendering of Stochastic Models," CACM | 1982 | Diamond-square origin |
| 21 | Witten & Sander, "DLA, a Kinetic Critical Phenomenon," PRL | 1981 | DLA model |
| 22 | "DLA model for drainage networks," Earth & Planetary Sci. Lett. | 1993 | DLA river networks |
| 23 | "Growth DLA for basin fractal river networks," AIP Advances | 2020 | DLA river evolution |
| 24 | Mandelbrot, *The Fractal Geometry of Nature* | 1982 | Fractal foundations |
| 25 | Bourke, "Noise, Perlin, 1/f Noise, Modelling Planets" | -- | 1/f noise, spectral synthesis |
| 26 | Red Blob Games, "Noise Functions and Map Generation" | -- | Noise comparison |
| 27 | Patel, "Polygonal Map Generation for Games" | 2010 | Voronoi terrain |
| 28 | Paris, "Terrain Erosion on the GPU" | -- | GPU erosion impl. |

---

*Research compiled 2026-03-27. 28 distinct sources across all 8 requested topic areas.*
