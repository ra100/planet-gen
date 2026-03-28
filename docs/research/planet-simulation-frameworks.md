# Planet Simulation Frameworks and Input Parameter Systems

**Research Date: 2026-03-27**

---

## Table of Contents

1. [Input Parameters / Seed System](#1-input-parameters--seed-system)
2. [Existing Simulation Frameworks](#2-existing-simulation-frameworks)
3. [Mathematical Models and Scaling Laws](#3-mathematical-models-and-scaling-laws)
4. [Sources](#4-sources)

---

## 1. Input Parameters / Seed System

### 1.1 What Initial Parameters Determine a Planet's Final State

A planet's observable properties (mass, radius, surface conditions, atmosphere) emerge from a chain of initial conditions spanning stellar chemistry, disk dynamics, and accretion history. The key insight from modern exoplanet science is that **stellar composition is the master control variable**: the host star's elemental abundances (especially Fe, Si, Mg, C, O) set the building blocks available for planet formation, while orbital distance determines which volatiles condense into solids ([Bitsch & Battistini 2020, A&A](https://www.aanda.org/articles/aa/full_html/2022/04/aa42738-21/aa42738-21.html)).

The principal initial conditions are:

| Parameter | Role | Effect on Final State |
|-----------|------|----------------------|
| **Stellar [Fe/H]** | Sets total metal budget | Higher metallicity -> more solid material -> easier giant planet formation |
| **Stellar Mg/Si ratio** | Controls silicate mineralogy | High Mg/Si -> forsterite (Mg2SiO4) dominated; low Mg/Si -> enstatite (MgSiO3) + quartz |
| **Stellar C/O ratio** | Controls volatile chemistry | Low C/O -> more water ice available beyond snow line |
| **Disk mass & lifetime** | Sets available material and accretion time | Determines maximum planet mass achievable |
| **Orbital distance** | Determines condensation temperature | Inside snow line: dry, Fe/Si-rich solids. Outside: water ice incorporated |
| **Formation timing** | When embryo reaches pebble isolation mass | Early formation -> gas accretion -> gas giant; late -> rocky/icy |

Planets forming inside the snow line (T > ~170 K) end up with higher Fe/O and Si/O ratios because most oxygen remains in the gas phase as H2O and CO. Beyond the snow line, water ice condenses efficiently, and planets can be composed of ~50% water around metal-poor stars vs ~6% around metal-rich stars ([Bitsch & Battistini 2020](https://www.aanda.org/articles/aa/full_html/2022/04/aa42738-21/aa42738-21.html); [Cabral et al. 2023, A&A](https://www.aanda.org/articles/aa/full_html/2023/10/aa46697-23/aa46697-23.html)).

### 1.2 Key Input Parameters

A **minimal physical parameter set** for specifying a terrestrial planet:

1. **Mass (M)** -- The most fundamental parameter. Determines internal pressure, gravity, and ability to retain atmosphere.
2. **Bulk composition ratios (Fe/Si, Mg/Si)** -- Control core size, mantle mineralogy, and density profile. Typically inferred from host star abundances ([Dorn et al. 2015, A&A](https://www.aanda.org/articles/aa/full_html/2015/05/aa24915-14/aa24915-14.html)).
3. **Water mass fraction** -- Determines presence/extent of hydrosphere and high-pressure ice layers. Strongly dependent on formation location relative to ice line.
4. **H/He envelope fraction** -- For sub-Neptunes; even 0.1-5% H2 by mass dramatically inflates radius ([Zeng et al. 2019, PNAS](https://lweb.cfa.harvard.edu/~lzeng/planetmodels.html)).
5. **Orbital distance / equilibrium temperature** -- Controls surface temperature, atmospheric escape rates, tidal effects.
6. **Stellar type and luminosity** -- Sets irradiation, habitable zone location, XUV flux for atmospheric erosion.
7. **Age** -- Determines cooling state; young planets are hotter and larger (thermal contraction over Gyr timescales).

For the Zeng et al. mass-radius models, the interior is parameterized as up to 3 layers: Fe core, MgSiO3 mantle, H2O/ice envelope, with optional H2/He atmosphere at specified temperature. The key free parameters are **core mass fraction (CMF)** and **water mass fraction (WMF)** ([Zeng et al. 2019](https://lweb.cfa.harvard.edu/~lzeng/planetmodels.html)).

### 1.3 Minimal Parameterization for Diverse Outputs

For **procedural/game-oriented** planet generation, the minimal seed-to-planet pipeline needs surprisingly few inputs to produce visually and physically diverse results:

**Physically-motivated minimal set (6 parameters):**
1. Mass (log-uniform from ~0.1 to ~1000 Earth masses)
2. Semi-major axis (log-uniform, ~0.01 to ~100 AU)
3. Stellar luminosity (determines equilibrium temperature)
4. Fe/Si ratio (0.5 to 2.0, from stellar abundances)
5. Water fraction (0 to 0.5, from formation location)
6. Age (0.1 to 10 Gyr)

From these six, one can derive: radius (from mass-radius relations), surface gravity, atmospheric retention, surface temperature, likely tectonic regime (stagnant lid vs. plate tectonics from thermal evolution models), ocean coverage, and atmospheric composition.

**For visual/terrain generation, add:**
7. Rotation rate (affects oblateness, weather patterns)
8. Obliquity (seasonal variation)
9. A noise seed (for deterministic procedural surface detail)

### 1.4 Random Seed to Deterministic Procedural Generation

The core principle: a single integer seed initializes a pseudorandom number generator (PRNG), and all subsequent "random" choices are drawn from this deterministic sequence. Given the same seed, the same planet is always produced ([Wikipedia: Procedural generation](https://en.wikipedia.org/wiki/Procedural_generation); [Rambus: Algorithms of No Man's Sky](https://www.rambus.com/blogs/the-algorithms-of-no-mans-sky-2/)).

**How it works in practice:**

1. **Seed -> PRNG state** -- A 64-bit seed initializes a hash function or PRNG (e.g., xxHash, PCG, Mersenne Twister).
2. **Hierarchical derivation** -- The master seed generates sub-seeds for each aspect: orbital parameters, bulk properties, terrain, atmosphere, biome placement. This is critical because it allows independent modification of subsystems.
3. **Noise-based terrain** -- Surface height is generated by evaluating coherent noise (Perlin, Simplex, or cellular) on a unit sphere. Parameters controlled by sub-seeds include: octave count, lacunarity, persistence/gain, fractal type (fBm, ridged multifractal, hybrid), maximum amplitude, and ocean level ([Gaia Sky docs](https://gaia.ari.uni-heidelberg.de/gaiasky/docs/3.4.2/Procedural-generation.html); [Toni Sagrista 2021](https://tonisagrista.com/blog/2021/procedural-planetary-surfaces/)).

**No Man's Sky** uses this approach at massive scale: the position of each star serves as its seed, and pseudorandom numbers generated from that position determine the entire planetary system. This enables 18.4 quintillion unique but deterministic planets ([Rambus: Algorithms of No Man's Sky](https://www.rambus.com/blogs/the-algorithms-of-no-mans-sky-2/)).

**Key implementation patterns:**
- Use **hash functions** (not sequential PRNGs) for spatially-indexed generation, so you can query any location without generating all preceding locations
- **Hierarchical seeds** prevent "butterfly effect" where changing one parameter changes everything downstream
- Noise functions should be evaluated on the **surface of a 3D sphere** (not 2D UV coordinates) to avoid polar distortion and seams

### 1.5 Inverse Problem: Working Backwards from Desired State

The inverse problem -- inferring interior composition from observed mass and radius -- is a **highly degenerate** problem. Different compositions can produce identical mass-radius pairs. This has been extensively studied in exoplanet science.

**Key findings from Dorn et al. (2015):**
- Mass and radius alone are sufficient to constrain **core size** but poorly constrain **mantle composition** ([Dorn et al. 2015](https://www.aanda.org/articles/aa/full_html/2015/05/aa24915-14/aa24915-14.html))
- Adding stellar Fe/Si and Mg/Si abundances dramatically reduces degeneracy
- Smaller, Earth-sized planets are better constrained than larger super-Earths at the same measurement precision
- The degeneracy is quantified using Shannon entropy: H_posterior/H_prior approaching 0 = well constrained, approaching 1 = unconstrained

**Modern ML-based inverse solvers:**
- **ExoMDN** (Baumeister et al. 2023): A mixture density network trained on 5.6 million synthetic planets (iron core + silicate mantle + water/ice layer + H/He atmosphere). Given mass, radius, and equilibrium temperature, it returns full posterior distributions of layer mass fractions in under 1 second. Open source at [github.com/philippbaumeister/ExoMDN](https://github.com/philippbaumeister/ExoMDN) ([Baumeister et al. 2023, A&A](https://www.aanda.org/articles/aa/full_html/2023/08/aa46216-23/aa46216-23.html))
- **Bayesian MCMC methods** provide rigorous uncertainty quantification but are computationally expensive (hours per planet) ([Dorn et al. 2015](https://arxiv.org/abs/1502.03605))

**For procedural generation (game/art context):**
The inverse problem is simpler: you specify the desired outcome (e.g., "Earth-like ocean world") and can directly set parameters to achieve it. The physical inverse problem only matters if you want the result to be *physically self-consistent* -- i.e., could this planet actually exist? In that case, you can use the Zeng et al. mass-radius curves or ExoMDN to validate that your chosen mass, radius, and composition are mutually compatible.

---

## 2. Existing Simulation Frameworks

### 2.1 N-Body Accretion Codes

#### REBOUND

| Property | Detail |
|----------|--------|
| **What it does** | Multi-purpose N-body integrator for gravitational dynamics. Simulates motion of particles under gravity -- planets, moons, ring particles, dust. |
| **Used for** | Planetary ring dynamics, planet formation, orbital mechanics, asteroid dynamics, exoplanet transit timing |
| **Integrators** | Leap-frog, Symplectic Epicycle Integrator (SEI), Wisdom-Holman (WHFast), IAS15 (high-accuracy adaptive) |
| **Key features** | Barnes-Hut tree gravity, plane-sweep collision detection, MPI + OpenMP parallelization, Python and C API |
| **Open source** | Yes, 100% open source (GPL) |
| **URL** | [github.com/hannorein/rebound](https://github.com/hannorein/rebound) |
| **Install** | `pip install rebound` |
| **Citation** | [Rein & Liu 2012, A&A 537, A128](https://www.aanda.org/articles/aa/full_html/2012/01/aa18085-11/aa18085-11.html) |

#### ChaNGa (Charm N-body GrAvity solver)

| Property | Detail |
|----------|--------|
| **What it does** | Collisionless and SPH N-body simulations with cosmological or isolated boundary conditions |
| **Used for** | Galaxy formation, cosmological simulations, protoplanetary disk modeling |
| **Key features** | Barnes-Hut tree gravity (derived from PKDGRAV), SPH hydrodynamics (from GASOLINE), Charm++ parallelization for extreme scalability |
| **Open source** | Yes, GPL v2 |
| **URL** | [github.com/N-BodyShop/changa](https://github.com/N-BodyShop/changa) |
| **Citation** | [Jetley et al. 2008, "Massively Parallel Cosmological Simulations with ChaNGa"](https://charm.cs.illinois.edu/newPapers/08-03/paper.pdf) |

#### PKDGRAV3

| Property | Detail |
|----------|--------|
| **What it does** | High-performance N-body code optimized for trillion-particle cosmological simulations |
| **Used for** | Cosmological structure formation, planetesimal dynamics (with collision modules like EDACM), gravitational dynamics |
| **Key features** | Fast Multipole Method (FMM), individual adaptive time steps, GPU acceleration, designed for supercomputer-scale runs |
| **Open source** | Yes (source available on GitHub) |
| **URL** | [github.com/wullm/pkdgrav3](https://github.com/wullm/pkdgrav3) |
| **Citation** | [Potter et al. 2017, Comp. Astrophys. & Cosmology](https://comp-astrophys-cosmol.springeropen.com/articles/10.1186/s40668-017-0021-1) |

### 2.2 Mantle Convection Codes

#### ASPECT (Advanced Solver for Problems in Earth's ConvecTion)

| Property | Detail |
|----------|--------|
| **What it does** | Parallel finite element code for thermal convection in planetary mantles. Adaptive mesh refinement, multigrid solvers. |
| **Used for** | Mantle convection, lithosphere deformation, inner core convection, two-phase flow, subduction modeling |
| **Key features** | Built on deal.II (FEM), Trilinos (linear algebra), p4est (parallel meshes). Modular plugin architecture. Hundreds-to-thousands of cores. |
| **Open source** | Yes, GPL v2+ |
| **URL** | [aspect.geodynamics.org](https://aspect.geodynamics.org/) ; [github.com/geodynamics/aspect](https://github.com/geodynamics/aspect) |
| **Citation** | [Heister et al. 2017, GJI 210(2):833](https://academic.oup.com/gji/article/210/2/833/3807083) |

#### CitcomS

| Property | Detail |
|----------|--------|
| **What it does** | Finite element code for compressible thermochemical convection in spherical shell geometry |
| **Used for** | Earth's mantle convection, thermal evolution, plume dynamics |
| **Key features** | Spherical geometry (full or regional), compressible formulation, MPI parallelization |
| **Open source** | Yes, GPL v2 |
| **URL** | [github.com/geodynamics/citcoms](https://github.com/geodynamics/citcoms) ; [geodynamics.org/resources/citcoms](https://geodynamics.org/resources/citcoms) |
| **Validation** | Benchmarked against ASPECT with results agreeing within 1% ([GMD 2023](https://gmd.copernicus.org/articles/16/3221/2023/)) |

#### TerraFERMA (Transparent Finite Element Rapid Model Assembler)

| Property | Detail |
|----------|--------|
| **What it does** | Multiphysics code for rapid, reproducible construction of coupled geodynamic models |
| **Used for** | Mantle convection, subduction zone modeling, magma transport, coupled thermo-mechanical problems |
| **Key features** | Built on FEniCS (problem description), PETSc (solvers), SPuD (options management). Emphasizes transparency and reproducibility. |
| **Open source** | Yes, LGPL v3+ |
| **URL** | [terraferma.github.io](https://terraferma.github.io/) ; [github.com/TerraFERMA/TerraFERMA](https://github.com/TerraFERMA/TerraFERMA) |
| **Citation** | [Wilson & Spiegelman 2017, G-Cubed 18:769-810](https://agupubs.onlinelibrary.wiley.com/doi/full/10.1002/2016GC006702) |

#### Underworld2

| Property | Detail |
|----------|--------|
| **What it does** | Particle-in-cell finite element code for 2D/3D geodynamics. Python-based interface. |
| **Used for** | Mantle convection, lithosphere dynamics, subduction, rift modeling, crustal deformation |
| **Key features** | Hybrid mesh+particle approach (accurate velocity on mesh, accurate material tracking on particles). Jupyter notebook workflow. UWGeodynamics high-level module for rapid prototyping. |
| **Open source** | Yes (BSD-like license) |
| **URL** | [underworldcode.org](https://www.underworldcode.org/intro-to-underworld/) ; [github.com/underworldcode/underworld2](https://github.com/underworldcode/underworld2) |
| **Citation** | [Mansour et al. 2020, JOSS](https://www.theoj.org/joss-papers/joss.01797/10.21105.joss.01797.pdf) |

### 2.3 Terrain Generation Tools

#### World Machine

| Property | Detail |
|----------|--------|
| **What it does** | Node-based procedural terrain generation with physically-based erosion simulation |
| **Used for** | Game development, VFX, visualization. Used by CBS Studios, BBC, Kojima Productions, Microsoft, NASA. |
| **Key features** | Fractal generators, thermal/hydraulic erosion simulation, snow/sediment deposition, node-based workflow, heightmap export |
| **Open source** | No (commercial, free Basic edition) |
| **URL** | [world-machine.com](https://www.world-machine.com/) |

#### Gaea (QuadSpinner)

| Property | Detail |
|----------|--------|
| **What it does** | Next-generation terrain design tool with directed erosion and multiple workflow modes |
| **Used for** | Game terrain, film VFX, visualization. Used by Lucasfilm Games, Larian Studios, etc. |
| **Key features** | Three workflow modes (Layers, Graph, Sculpt), directed erosion system, physically-based weathering. Created by former GeoGlyph (World Machine plugin) developer. |
| **Open source** | No (commercial, free Community edition) |
| **URL** | [quadspinner.com](https://quadspinner.com/) |

#### libnoise

| Property | Detail |
|----------|--------|
| **What it does** | C++ library for generating 3D coherent noise (Perlin noise, ridged multifractal, etc.) |
| **Used for** | Terrain heightmaps, procedural textures, planet surface generation |
| **Key features** | Modular noise pipeline (generators + modifiers + combiners), Perlin noise, ridged multifractal, Voronoi, turbulence |
| **Open source** | Yes (LGPL) |
| **URL** | [libnoise.sourceforge.net](https://libnoise.sourceforge.net/tutorials/tutorial5.html) ; [sourceforge.net/projects/libnoise](https://sourceforge.net/projects/libnoise/) |

#### FastNoiseLite

| Property | Detail |
|----------|--------|
| **What it does** | Fast, portable noise generation library supporting multiple noise types and fractal configurations |
| **Used for** | Terrain generation, procedural textures, any application needing coherent noise |
| **Key features** | Perlin, Simplex (OpenSimplex2), Cellular (Voronoi), Value, Value Cubic noise types. Domain warp support. Fractal options (fBm, ridged, ping-pong). Float and double precision. |
| **Languages** | C, C++, C#, Java, JavaScript, Rust, Go, HLSL, GLSL, Fortran, Zig, and more |
| **Open source** | Yes (MIT license) |
| **URL** | [github.com/Auburn/FastNoiseLite](https://github.com/Auburn/FastNoiseLite) |

### 2.4 Procedural Planet Tools (Interactive/Game)

#### SpaceEngine

| Property | Detail |
|----------|--------|
| **What it does** | Real-time universe simulator with procedural generation of stars, planets, galaxies, and terrain at all scales |
| **Used for** | Space exploration, education, visualization, astronomy outreach |
| **Key features** | Deterministic universe from a single master seed. Fractal noise-based terrain. Configurable per-planet parameters: mass, radius, atmospheric gases (H2, He, N2, CO2, H2O, CH4, etc.), cloud layers, ocean levels. Procedural generation fills gaps beyond real star catalogs. |
| **Planet parameters** | ParentBody, Mass or Radius, Orbit (SemiMajorAxis or Period). Optional: atmospheric composition, surface type, cloud parameters, NoClouds/NoOcean/NoAtmosphere flags. |
| **Open source** | No (commercial, available on [Steam](https://store.steampowered.com/app/314650/SpaceEngine/)) |
| **URL** | [spaceengine.org](https://spaceengine.org/) |
| **Reference** | [SpaceEngine Wiki: Procedural Generation](https://spaceengine.fandom.com/wiki/Procedural_Generation) |

#### Universe Sandbox

| Property | Detail |
|----------|--------|
| **What it does** | Physics-based space simulator with real-time gravity, climate, collision, and material interaction |
| **Used for** | Education, what-if scenarios (e.g., collide planets, move orbits), stellar evolution visualization |
| **Key features** | N-body Newtonian gravity, planet composition/surface simulation, atmospheric modeling, collision physics, Roche fragmentation, stellar evolution. Recently migrated physics to Unity DOTS framework. |
| **Open source** | No (commercial, available on [Steam](https://universesandbox.com/)) |
| **URL** | [universesandbox.com](https://universesandbox.com/) |
| **Reference** | [Universe Sandbox - Wikipedia](https://en.wikipedia.org/wiki/Universe_Sandbox) |

#### Pioneer Space Simulator

| Property | Detail |
|----------|--------|
| **What it does** | Space trading and combat simulator (Frontier: Elite 2 spiritual successor) with procedural star systems |
| **Used for** | Open-ended space gameplay: trading, piracy, exploration, missions |
| **Key features** | Millions of procedurally generated star systems, planetary landing, C++ with Lua scripting, OpenGL rendering |
| **Open source** | Yes (GPL) |
| **URL** | [pioneerspacesim.net](https://pioneerspacesim.net/) ; [github.com/pioneerspacesim/pioneer](https://github.com/pioneerspacesim/pioneer) |

### 2.5 Blender Planet Generation Tools

#### b3dplanetgen

| Property | Detail |
|----------|--------|
| **What it does** | Customizable procedural planet generator addon for Blender |
| **Open source** | Yes |
| **URL** | [github.com/cdcarswell/b3dplanetgen](https://github.com/cdcarswell/b3dplanetgen) |

#### Procedural Planet Generator (jumpingpuzzle)

| Property | Detail |
|----------|--------|
| **What it does** | Blender addon for random planet generation using node groups. Supports desert, ocean, ice, volcanic types with clouds, atmosphere, and rings. |
| **Features** | 26 pre-designed node groups, random generation mode, compatible with Blender 3.x-4.2 |
| **Open source** | Commercial (Gumroad) |
| **URL** | [jumpingpuzzle.gumroad.com](https://jumpingpuzzle.gumroad.com/l/qngyv) |

#### Planet and Space Objects Generator (Hlavka)

| Property | Detail |
|----------|--------|
| **What it does** | Blender addon for procedural generation of planets and other space objects |
| **Open source** | Yes |
| **URL** | [github.com/MarekHlavka/Planet-and-space-objects-generator](https://github.com/MarekHlavka/Planet-and-space-objects-generator) |

#### ANT Landscape (Built-in)

| Property | Detail |
|----------|--------|
| **What it does** | Blender's built-in terrain generator. Can generate on sphere primitives for planet-like objects. |
| **Features** | Multiple noise types, fractal options, erosion. Free, ships with Blender. |
| **URL** | [Blender Manual](https://artisticrender.com/how-to-use-blenders-free-terrain-generator-ant-landscape-add-on/) |

#### 11 Free Procedural Planet Shaders

| Property | Detail |
|----------|--------|
| **What it does** | Collection of shader node setups for various planet types in Blender |
| **Open source** | Yes (free) |
| **URL** | [ArtStation](https://www.artstation.com/marketplace/p/2lql/11-free-procedural-planet-shaders-blender) |

### 2.6 Academic Codes for Thermal Evolution

#### Li Zeng's Planet Interior Models

| Property | Detail |
|----------|--------|
| **What it does** | Suite of tools for computing exoplanet interior structure and mass-radius relations |
| **Models** | 3-layer (Fe + MgSiO3 + H2O) with optional H/He envelope. Covers 0.1-100 Earth masses. |
| **Tools** | Interactive CDF tool (Wolfram Mathematica), MATLAB codes (ExoterDE, ExoterDB), downloadable mass-radius tables |
| **Open source** | Yes (MATLAB code and data freely available) |
| **URL** | [lweb.cfa.harvard.edu/~lzeng/planetmodels.html](https://lweb.cfa.harvard.edu/~lzeng/planetmodels.html) |
| **Citation** | [Zeng et al. 2019, PNAS 116(20):9723](https://lweb.cfa.harvard.edu/~lzeng/planetmodels.html) ; [Zeng & Seager 2008, PASP 120:983](https://ui.adsabs.harvard.edu/abs/2008PASP..120..983Z) |

#### ExoMDN

| Property | Detail |
|----------|--------|
| **What it does** | Machine learning model (Mixture Density Network) for rapid exoplanet interior characterization |
| **Input** | Mass, radius, equilibrium temperature |
| **Output** | Full posterior distributions of layer mass fractions (iron core, silicate mantle, water/ice, H/He) and thicknesses |
| **Training data** | 5.6 million synthetic planets |
| **Performance** | Full inference in < 1 second on standard CPU |
| **Open source** | Yes (Python) |
| **URL** | [github.com/philippbaumeister/ExoMDN](https://github.com/philippbaumeister/ExoMDN) |
| **Citation** | [Baumeister et al. 2023, A&A 676, A106](https://www.aanda.org/articles/aa/full_html/2023/08/aa46216-23/aa46216-23.html) |

#### pyExoRaMa

| Property | Detail |
|----------|--------|
| **What it does** | Interactive Python tool for visualizing and manipulating exoplanet mass-radius data against theoretical composition curves |
| **Open source** | Yes |
| **URL** | [zenodo.org/records/5899601](https://zenodo.org/records/5899601) |

---

## 3. Mathematical Models and Scaling Laws

### 3.1 Thermal Evolution: Parameterized Convection

Thermal evolution of terrestrial planets is modeled using **parameterized convection**, which reduces the full 3D convection problem to a 1D energy balance equation. The key relationship is between the **Nusselt number** (Nu, dimensionless heat flux) and the **Rayleigh number** (Ra, convective vigor):

```
Nu = a * Ra^beta
```

where the exponent beta determines how efficiently convection removes heat.

**Key values of beta:**
- Classical boundary layer theory predicts beta = 1/3
- Numerical simulations in spherical shells find beta = 0.294 +/- 0.004 for basally heated convection ([Wolstencroft et al. 2009, PEPI](https://www.sciencedirect.com/science/article/pii/S0031920109001216))
- For internally heated systems, beta = 0.337 +/- 0.009
- The difference matters: at Ra = 10^9, beta = 0.29 gives ~32% lower surface heat flux than beta = 1/3

**The Rayleigh number** encodes the competition between buoyancy and viscous drag:

```
Ra = (rho * g * alpha * delta_T * d^3) / (kappa * eta)
```

where rho = density, g = gravity, alpha = thermal expansivity, delta_T = temperature contrast, d = mantle thickness, kappa = thermal diffusivity, eta = viscosity.

**Thermal evolution equation** (simplified 1D):

```
rho * Cp * V * (dT/dt) = -Q_surface + Q_core + Q_radiogenic
```

The surface heat flux Q_surface is parameterized through Nu(Ra), linking the cooling rate to the current thermal state. The cooling timescale depends on two separable timescales: a dynamic timescale (convective overturn time) and a thermal timescale (planetary cooling time) ([Stevenson et al. 1983](https://www.sciencedirect.com/science/article/abs/pii/0040195181902055)).

**Stagnant lid vs. plate tectonics regimes:**

Different tectonic modes produce dramatically different cooling histories. Stagnant lid planets (like Mars, likely most rocky exoplanets) cool slower because the rigid lid insulates the interior. Scaling laws differ between regimes:

- Plate tectonics: Nu ~ Ra^(1/3)
- Stagnant lid: Nu ~ Ra_i^(1/3) * exp(-theta/3), where theta is the Frank-Kamenetskii parameter for temperature-dependent viscosity

([Tosi et al. 2019, PEPI](https://www.sciencedirect.com/science/article/abs/pii/S0031920118301936); [Auerbach et al. 2025, JGR Planets](https://agupubs.onlinelibrary.wiley.com/doi/10.1029/2025JE009016))

### 3.2 Crustal Thickness from Rayleigh-Taylor Instability

When a planet differentiates (metals sink, silicates float), the resulting layered structure can be gravitationally unstable. A dense crust overlying a lighter mantle is subject to **Rayleigh-Taylor instability** (RTI), which limits maximum crustal thickness.

**Core formation via RTI:** In the proto-Earth, an undifferentiated solid core overlain by a metal-melt layer and silicate-melt layer was gravitationally unstable. RTI causes core formation on a timescale of ~10 hours if the metal-melt layer exceeds ~1 km thickness ([Sasaki & Nakazawa 1986, Icarus](https://www.sciencedirect.com/science/article/abs/pii/0019103587901035)).

**Crustal thickness limits on KBOs:** For Kuiper Belt Objects with a dense rock/ice undifferentiated crust resting on an icy mantle, RTI sets the maximum crust thickness. The instability growth rate depends on the density contrast, viscosity of the substrate, and layer thickness ([Rubin et al. 2014, Icarus](https://www.sciencedirect.com/science/article/abs/pii/S0019103514001821)).

**Lithospheric delamination:** In terrestrial planets, mechanically thickened lithosphere can become gravitationally unstable. The growth rate of RTI for non-Newtonian (power-law) viscosity differs from the classical Newtonian case, with instability developing faster for stress-dependent rheology ([Houseman & Molnar 1997, GJI](https://academic.oup.com/gji/article/128/1/125/652825)).

The characteristic RTI timescale is:

```
tau_RT ~ (eta / (delta_rho * g * h))
```

where eta is the viscosity of the less viscous layer, delta_rho is the density contrast, g is gravity, and h is the perturbation wavelength.

### 3.3 Mass-Radius Relations and Key Scaling Laws

#### Empirical Mass-Radius Relations

The observed exoplanet population follows a **broken power law** with three regimes ([Bashi et al. 2017, A&A](https://www.aanda.org/articles/aa/full_html/2017/08/aa29922-16/aa29922-16.html)):

| Regime | Mass Range | Relation | Planet Type |
|--------|-----------|----------|-------------|
| Small | M < ~4.4 Earth masses | R proportional to M^0.27 | Rocky/terrestrial |
| Intermediate | ~4.4 to ~127 Earth masses | R proportional to M^0.67 | Sub-Neptunes, Neptunes |
| Giant | > ~127 Earth masses | R proportional to M^(-0.06) | Gas giants (degenerate electron pressure) |

#### Theoretical Mass-Radius (Composition-Dependent)

For solid exoplanets, Seager et al. (2007) found a generic functional form that is **not** a simple power law:

```
log10(R/R_earth) = k1 + (1/3) * log10(M/M_earth) - k2 * (M/M_earth)^k3
```

valid up to ~20 Earth masses, where k1, k2, k3 depend on composition ([Seager et al. 2007, ApJ 669:1279](https://arxiv.org/abs/0707.2895)).

The Zeng et al. models provide specific curves for:
- Pure iron: smallest radius for given mass
- Earth-like (32.5% Fe + 67.5% MgSiO3): the "rocky" baseline
- Pure MgSiO3: larger than Earth-like
- 50% H2O worlds: significantly larger
- 100% H2O: the "water world" upper limit for non-gaseous planets
- H2/He envelopes (0.1-5%): dramatically inflated radii

([Zeng et al. 2019, PNAS](https://lweb.cfa.harvard.edu/~lzeng/planetmodels.html))

#### Revised Mass-Radius with Machine Learning

Marif et al. (2023) applied ML techniques to revisit mass-radius relationships, finding that random forest models can capture the complex, non-linear dependence of radius on mass, stellar irradiation, and age better than simple power laws ([Marif et al. 2023, MNRAS 525(3):3469](https://academic.oup.com/mnras/article/525/3/3469/7246902)).

Otegi et al. (2020) provide updated piecewise relations for planets below 120 Earth masses, identifying a density transition at ~5 Earth masses separating rocky from volatile-rich populations ([Otegi et al. 2020, A&A](https://www.aanda.org/articles/aa/full_html/2020/02/aa36482-19/aa36482-19.html)).

#### Cooling Timescales

Planet cooling is governed by the thermal timescale:

```
tau_cool ~ (rho * Cp * R^2) / k
```

For Earth-sized planets, this is of order ~10 Gyr (comparable to stellar main-sequence lifetimes). For super-Earths, the timescale increases roughly as R^2, meaning larger planets retain heat longer and remain geologically active for longer periods.

Giant planet contraction (Kelvin-Helmholtz cooling) follows:

```
L(t) ~ L_0 * (t/t_0)^(-n)
```

with n approximately 1.2-1.3 for standard models ([Linder et al. 2019, A&A](https://www.aanda.org/articles/aa/full_html/2019/03/aa33873-18/aa33873-18.html)).

---

## 4. Sources

### N-Body and Planet Formation
1. [Rein & Liu 2012 - "REBOUND: An open-source multi-purpose N-body code for collisional dynamics", A&A 537, A128](https://www.aanda.org/articles/aa/full_html/2012/01/aa18085-11/aa18085-11.html)
2. [REBOUND GitHub Repository](https://github.com/hannorein/rebound)
3. [Jetley et al. 2008 - "Massively Parallel Cosmological Simulations with ChaNGa"](https://charm.cs.illinois.edu/newPapers/08-03/paper.pdf)
4. [ChaNGa GitHub / N-Body Shop](https://github.com/N-BodyShop/changa)
5. [Potter et al. 2017 - "PKDGRAV3: beyond trillion particle cosmological simulations", Comp. Astrophys. & Cosmology](https://comp-astrophys-cosmol.springeropen.com/articles/10.1186/s40668-017-0021-1)

### Mantle Convection
6. [ASPECT - Advanced Solver for Problems in Earth's ConvecTion](https://aspect.geodynamics.org/)
7. [Heister et al. 2017 - "High accuracy mantle convection simulation through modern numerical methods", GJI 210(2):833](https://academic.oup.com/gji/article/210/2/833/3807083)
8. [CitcomS - CIG Geodynamics](https://geodynamics.org/resources/citcoms)
9. [Wilson & Spiegelman 2017 - "TerraFERMA: The Transparent Finite Element Rapid Model Assembler", G-Cubed 18:769-810](https://agupubs.onlinelibrary.wiley.com/doi/full/10.1002/2016GC006702)
10. [Mansour et al. 2020 - "Underworld2: Python Geodynamics Modelling for Desktop, HPC and Cloud", JOSS](https://www.theoj.org/joss-papers/joss.01797/10.21105.joss.01797.pdf)

### Mass-Radius Relations and Interior Models
11. [Dorn et al. 2015 - "Can we constrain the interior structure of rocky exoplanets from mass and radius measurements?", A&A 577, A83](https://www.aanda.org/articles/aa/full_html/2015/05/aa24915-14/aa24915-14.html)
12. [Seager et al. 2007 - "Mass-Radius Relationships for Solid Exoplanets", ApJ 669:1279](https://arxiv.org/abs/0707.2895)
13. [Zeng et al. 2019 - Planet Models (Harvard CfA)](https://lweb.cfa.harvard.edu/~lzeng/planetmodels.html)
14. [Baumeister et al. 2023 - "ExoMDN: Rapid characterization of exoplanet interior structures with mixture density networks", A&A 676, A106](https://www.aanda.org/articles/aa/full_html/2023/08/aa46216-23/aa46216-23.html)
15. [Bashi et al. 2017 - "Two empirical regimes of the planetary mass-radius relation", A&A](https://www.aanda.org/articles/aa/full_html/2017/08/aa29922-16/aa29922-16.html)
16. [Otegi et al. 2020 - "Revisited mass-radius relations for exoplanets below 120 Earth masses", A&A](https://www.aanda.org/articles/aa/full_html/2020/02/aa36482-19/aa36482-19.html)

### Thermal Evolution and Scaling Laws
17. [Wolstencroft et al. 2009 - "Nusselt-Rayleigh number scaling for spherical shell Earth mantle simulation", PEPI 176:132](https://www.sciencedirect.com/science/article/pii/S0031920109001216)
18. [Tosi et al. 2019 - "Scaling laws of convection for cooling planets in a stagnant lid regime", PEPI 286:138](https://www.sciencedirect.com/science/article/abs/pii/S0031920118301936)
19. [Auerbach et al. 2025 - "Thermal Evolution of Planetary Interiors With a Crystallizing Basal Magma Ocean", JGR Planets](https://agupubs.onlinelibrary.wiley.com/doi/10.1029/2025JE009016)
20. [Stevenson et al. 1981 - "Parameterized convection and the thermal evolution of the earth", Tectonophysics](https://www.sciencedirect.com/science/article/abs/pii/0040195181902055)

### Planet Formation Initial Conditions
21. [Bitsch & Battistini 2020 - "Forming Planets Around Stars With Non-Solar Elemental Composition", A&A](https://www.aanda.org/articles/aa/full_html/2022/04/aa42738-21/aa42738-21.html)
22. [Cabral et al. 2023 - "Planet formation throughout the Milky Way", A&A](https://www.aanda.org/articles/aa/full_html/2023/10/aa46697-23/aa46697-23.html)

### Rayleigh-Taylor Instability
23. [Sasaki & Nakazawa 1986 - "The Earth's core formation due to the Rayleigh-Taylor instability", Icarus](https://www.sciencedirect.com/science/article/abs/pii/0019103587901035)
24. [Rubin et al. 2014 - "The effect of Rayleigh-Taylor instabilities on the thickness of undifferentiated crust on KBOs", Icarus](https://www.sciencedirect.com/science/article/abs/pii/S0019103514001821)

### Procedural Generation
25. [Rambus - "The algorithms of No Man's Sky"](https://www.rambus.com/blogs/the-algorithms-of-no-mans-sky-2/)
26. [Gaia Sky - Procedural planetary surfaces documentation](https://gaia.ari.uni-heidelberg.de/gaiasky/docs/3.4.2/Procedural-generation.html)
27. [Sagrista 2021 - "Procedural generation of planetary surfaces"](https://tonisagrista.com/blog/2021/procedural-planetary-surfaces/)
28. [Zechmann & Hlavacs 2025 - "Comparative Analysis of Procedural Planet Generators", arXiv:2510.24764](https://arxiv.org/abs/2510.24764)
29. [SpaceEngine Manual - Creating a planet](https://spaceengine.org/manual/making-addons/creating-a-planet/)
30. [FastNoiseLite GitHub](https://github.com/Auburn/FastNoiseLite)

### Blender Tools
31. [b3dplanetgen - GitHub](https://github.com/cdcarswell/b3dplanetgen)
32. [Planet and Space Objects Generator - GitHub](https://github.com/MarekHlavka/Planet-and-space-objects-generator)
