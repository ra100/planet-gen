# Planetary Internal Structure and Geodynamics

*Deep research report -- compiled 2026-03-27*

---

## 1. Layered Structure of Rocky Planets (Earth as Reference)

Earth's interior is divided into concentric shells whose boundaries are defined by seismic discontinuities. The **Preliminary Reference Earth Model (PREM)** provides the standard density, pressure, and velocity profiles. [Internal structure of Earth -- Wikipedia](https://en.wikipedia.org/wiki/Internal_structure_of_Earth) | [PREM density profile (Dziewonski & Anderson, 1981)](https://www.researchgate.net/figure/Density-profile-of-the-Earth-according-to-the-PREM-model-19-Different-colors_fig4_323944696)

| Layer | Depth range (km) | Density (g/cm^3) | Temperature | Key boundary |
|---|---|---|---|---|
| **Continental crust** | 0--30 (avg), up to 70 | ~2.7 | Surface--200 C | -- |
| **Oceanic crust** | 0--5 to 10 | ~3.0 | Surface--200 C | -- |
| **Upper mantle** | Moho (~7--70) to 410 | 3.2--3.4 | 200--900 C | Mohorovicic discontinuity |
| **Transition zone** | 410--660 | 3.7--4.0 | 900--1600 C | 410-km olivine-->wadsleyite |
| **Lower mantle** | 660--2890 | 4.4--5.6 | 1600--~4000 C | 660-km spinel-->perovskite+magnesiowustite |
| **D'' layer** | ~2700--2890 | ~5.5--5.7 | ~3500--4000 C | -- |
| **Outer core (liquid)** | 2890--5150 | 9.9--12.2 | 4000--5000 C | Gutenberg discontinuity (2890 km) |
| **Inner core (solid)** | 5150--6371 | 12.6--13.0 | ~5000--6000 C | Lehmann discontinuity (5150 km) |

- Average Earth density: **5.515 g/cm^3** (bulk)
- Total Earth mass: ~6 x 10^24 kg
- Pressure at core-mantle boundary: **~136 GPa**; at Earth's center: **~360 GPa**
- Mantle viscosity range: **10^21 to 10^24 Pa-s**
- Core composition: predominantly iron-nickel alloy with ~10% light elements (S, O, Si, H)

[Inside the Earth -- USGS](https://pubs.usgs.gov/gip/dynamic/inside.html) | [Earth's Interior -- Purdue University](https://web.ics.purdue.edu/~braile/edumod/earthint/earthint.htm)

---

## 2. Convection Regimes: Stagnant Lid, Mobile Lid, and Episodic

Three primary convection/tectonic regimes have been identified in numerical modeling and in the solar system:

### 2.1 Stagnant Lid (default mode)

The surface forms a single, rigid, immobile shell. Convection occurs only beneath this lid. **This is the most common tectonic regime in the solar system** -- it operates on Mercury, the Moon, Mars, Venus (presently), and most rocky/icy moons.

- The cold upper lithosphere is too viscous to participate in underlying mantle flow.
- Yield strength of the lid is high enough that convective stresses cannot cause brittle failure.
- Requires **viscosity contrast > 10^4** (four orders of magnitude) between surface and deep interior.

[Lid tectonics -- Wikipedia](https://en.wikipedia.org/wiki/Lid_tectonics) | [Stagnant lid tectonics: Perspectives from silicate planets (Stern et al., 2018)](https://www.sciencedirect.com/science/article/pii/S1674987117301135)

### 2.2 Mobile Lid (Plate Tectonics)

Multiple cold surface plates move continuously, with creation at ridges and recycling at subduction zones. **Earth is the only body in the solar system known to operate in this regime.**

- Requires weak, localized shear zones (plate boundaries) in the lithosphere.
- Lithospheric strength must be **less than** convective driving stresses.
- Subduction is the descending component; mid-ocean ridge spreading is the ascending component.

[Plate tectonics -- Wikipedia](https://en.wikipedia.org/wiki/Plate_tectonics) | [Mantle convection -- Wikipedia](https://en.wikipedia.org/wiki/Mantle_convection)

### 2.3 Episodic Lid

An intermediate regime in which the lid is mostly stagnant but periodically mobilizes in catastrophic overturns. **Venus is the leading candidate**, with evidence for a global resurfacing event ~500 Ma ago followed by quiescence.

- Occurs at **intermediate lithospheric yield stress** -- too strong for continuous mobile lid, too weak for permanent stagnant lid.
- First systematically described by **Moresi & Solomatov (1998)**.
- Venusian surface shows ~1000 impact craters distributed nearly randomly, consistent with a single catastrophic resurfacing rather than continuous recycling.

[Geodynamics of Venus -- Wikipedia](https://en.wikipedia.org/wiki/Geodynamics_of_Venus) | [Moresi & Solomatov cited in: Subduction initiation from a stagnant lid (Springer, 2016)](https://link.springer.com/article/10.1186/s40645-016-0103-8)

### 2.4 Additional Regimes

Recent work (Nature Communications, 2025) distinguishes **six** quantitative regimes: mobile lid, stagnant lid, sluggish lid, plutonic-squishy lid, episodic lid, and transitional. [Dissecting the puzzle of tectonic lid regimes -- Nature Communications (2025)](https://www.nature.com/articles/s41467-025-65943-1)

---

## 3. What Determines the Tectonic Regime?

The key condition is the ratio of **lithospheric yield stress** to **convective driving stress**:

> If lithospheric strength > convective stresses --> stagnant lid
> If lithospheric strength < convective stresses --> mobile lid (plate tectonics)
> Intermediate --> episodic

### 3.1 Controlling Parameters

| Factor | Effect on tectonics | Notes |
|---|---|---|
| **Planet size/mass** | Conflicting predictions. O'Neill & Lenardic (2007): 1.1 M_Earth planet has reduced driving stresses, favoring stagnant lid. Valencia et al. (2007): larger planets have higher mantle velocities, favoring plate tectonics. | "The influence of size may be small to the point of irrelevance compared to the presence of surface water." |
| **Surface temperature** | At 273 K: plate tectonics possible. At 759 K: only stagnant lid. Liquid water enables damage processes that weaken the lithosphere. | [Geodynamics of terrestrial exoplanets -- Wikipedia](https://en.wikipedia.org/wiki/Geodynamics_of_terrestrial_exoplanets) |
| **Water content** | Critical enabler. High pore fluid content lowers friction coefficient below critical value for sustained subduction. Wet rheology alone is not sufficient. | Water lubricates faults and enables grain-size reduction. |
| **Mantle viscosity** | Low reference viscosity (high Ra) favors plate tectonics. High viscosity favors stagnant lid. | Viscosity contrast > 10^4 between surface and interior required for stagnant lid. |
| **Initial/mantle temperature** | Hot interior with large internal heating rate may favor stagnant lid. Example: planet at initial CMB temp of 6100 K --> stagnant; 8100 K --> plate tectonics eventually. | [Noack & Breuer (2014) via Wikipedia](https://en.wikipedia.org/wiki/Geodynamics_of_terrestrial_exoplanets) |
| **Yield stress** | Low yield stress --> mobile lid; high --> stagnant lid; intermediate --> episodic. | Fundamental parameter in Moresi-Solomatov framework. |

[Plate tectonics on super-Earths (van Heck, 2011)](http://jupiter.ethz.ch/~pjt/papers/vanHeck2011PEPI_SuperEarths.pdf) | [The conditions for plate tectonics on super-Earths (ScienceDirect)](https://www.sciencedirect.com/science/article/abs/pii/S0012821X12001513) | [The dependence of planetary tectonics on mantle thermal state (Royal Society, 2018)](https://royalsocietypublishing.org/doi/10.1098/rsta.2017.0409)

---

## 4. Heat Sources in Planetary Interiors

Earth's total internal heat loss at the surface: **47 +/- 2 TW** (average flux ~91.6 mW/m^2). This comes from two roughly equal contributions. [Earth's internal heat budget -- Wikipedia](https://en.wikipedia.org/wiki/Earth%27s_internal_heat_budget)

### 4.1 Radiogenic Heat (~20 TW, ~40-50% of total)

Four isotopes dominate (>99.5% of radiogenic heat):

| Isotope | Half-life (Gyr) | Heat release (uW/kg isotope) | Mean mantle concentration (ppb) | Heat release (pW/kg mantle) | Estimated global contribution |
|---|---|---|---|---|---|
| **U-238** | 4.47 | 94.6 | 30.8 | 2.91 | ~8 TW |
| **U-235** | 0.704 | 569 | 0.22 | 0.125 | ~0.3 TW |
| **Th-232** | 14.0 | 26.4 | 124 | 3.27 | ~8 TW |
| **K-40** | 1.25 | 29.2 | 36.9 | 1.08 | ~4 TW |

- Combined geoneutrino-constrained estimate (KamLAND + Borexino): **~20 TW total radiogenic power** (~16 TW from Th+U alone).
- Minor contributors: Rb-87, Sm-147 (~0.5% combined).

[What Keeps the Earth Cooking? -- Berkeley Lab (2011)](https://newscenter.lbl.gov/2011/07/17/kamland-geoneutrinos/) | [Quantifying Earth's radiogenic heat budget (ScienceDirect, 2022)](https://www.sciencedirect.com/science/article/am/pii/S0012821X2200320X) | [Earth Still Retains Much of Its Original Heat -- Science/AAAS](https://www.science.org/content/article/earth-still-retains-much-its-original-heat)

### 4.2 Primordial Heat (~12--30 TW, ~50-60% of total)

Residual heat from:
- Gravitational potential energy released during **accretion** and **core-mantle differentiation**
- Energy from the **giant Moon-forming impact** (~4.5 Ga)
- Core formation released ~10^31 J of gravitational energy

The primordial contribution is estimated at **12--30 TW**, constituting roughly half of Earth's heat budget. [Earth's internal heat budget -- Wikipedia](https://en.wikipedia.org/wiki/Earth%27s_internal_heat_budget)

### 4.3 Tidal Heating

For a synchronously rotating satellite with eccentric orbit:

```
E_tidal = -Im(k_2) * (21/2) * (G * M_h^2 * R^5 * n * e^2) / a^6
```

Where:
- `Im(k_2)` = imaginary part of the second-order Love number (dissipation efficiency)
- `M_h` = host body mass
- `R` = satellite radius
- `n` = mean orbital motion
- `e` = orbital eccentricity
- `a` = semi-major axis

**Key dependences:** Tidal heating scales as R^5, M_h^2, e^2, and **a^(-6)** (extremely sensitive to orbital distance).

| Body | Surface heat flux | Total power | Notes |
|---|---|---|---|
| **Io** | 2--3 W/m^2 | ~10^14 W (~100 TW) | Most volcanically active body in solar system |
| **Europa** | ~0.19 W/m^2 | ~10^12 W | Maintains subsurface ocean |
| **Enceladus** | up to ~16 GW (tiger stripes) | ~5--16 GW | Powers cryovolcanic jets |
| **Earth** (from Moon) | negligible | ~0.1 TW | Minor compared to radiogenic+primordial |

[Tidal heating -- Wikipedia](https://en.wikipedia.org/wiki/Tidal_heating) | [Tidal Heating of Jupiter's and Saturn's Moons -- Stanford](http://large.stanford.edu/courses/2007/ph210/pavlichin2/) | [Tidal heating of Io -- Wikipedia](https://en.wikipedia.org/wiki/Tidal_heating_of_Io)

---

## 5. Cooling Rates as a Function of Planet Size

### 5.1 The Surface-Area-to-Volume Argument

For a sphere of radius R:

```
Surface area = 4*pi*R^2
Volume       = (4/3)*pi*R^3
SA/V ratio   = 3/R
```

Heat is stored in the volume but escapes through the surface. A larger planet has a **smaller SA/V ratio**, meaning it retains heat longer. The naive cooling timescale scales as:

```
t_cool ~ Volume / Surface area ~ R / 3
```

Thus, **doubling a planet's radius roughly doubles the conductive cooling timescale**. [Planetary cooling: The surface area to volume ratio (University of Victoria)](https://web.uvic.ca/~jwillis/teaching/astr201/maths.5.planetary_cooling.pdf) | [Explore Mars -- LPI/USRA](https://www.lpi.usra.edu/education/explore/mars/inside_mars/cooling-planets/)

### 5.2 Convective Cooling Complicates the Picture

Real cooling is not purely conductive. The convective heat flux is parameterized as:

```
q_conv = a' * T^(1+beta) / eta(T)^beta
```

where `beta` is the tectonic cooling efficiency exponent (0 to ~0.33), and `eta(T)` is temperature-dependent viscosity. The Nusselt-Rayleigh scaling gives:

```
Nu = a * Ra^beta
```

For `beta = 0.3`, a planet with vigorous plate tectonics cools much faster than one in stagnant-lid mode. Seales & Lenardic (2021) showed that **a 5 M_Earth planet with beta=0.2 can reach the same temperature as a planet an order of magnitude less massive after 10 Gyr**, because the simple SA/V argument breaks down when tectonic regime varies.

[A Note on Planet Size and Cooling Rate -- Seales & Lenardic (2021)](https://ar5iv.labs.arxiv.org/html/2102.01077) | [ScienceDirect version](https://www.sciencedirect.com/science/article/abs/pii/S0019103521002323)

### 5.3 Comparative Cooling

| Body | Radius (km) | Mass (M_Earth) | SA/V (km^-1) | Current thermal state |
|---|---|---|---|---|
| **Moon** | 1,737 | 0.012 | 1.73 x 10^-3 | Largely cooled; bulk volcanism ceased ~1 Ga (possibly sporadic activity to ~120 Ma) |
| **Mars** | 3,390 | 0.107 | 8.85 x 10^-4 | Mostly cooled; dynamo ceased ~3.9 Ga; possible recent volcanism (Olympus Mons, <200 Ma flows) |
| **Earth** | 6,371 | 1.0 | 4.71 x 10^-4 | Fully active; plate tectonics, active dynamo |
| **Venus** | 6,052 | 0.815 | 4.96 x 10^-4 | Hot interior, no plate tectonics; episodic resurfacing ~500 Ma |

---

## 6. Mantle Convection Models

### 6.1 The Boussinesq Approximation

The standard simplification for mantle convection modeling assumes:
- Density variations are negligible everywhere **except** in the buoyancy term of the momentum equation.
- The fluid is treated as incompressible: `div(v) = 0`
- Density depends linearly on temperature: `rho = rho_0 * (1 - alpha * (T - T_0))`

where `alpha` is the thermal expansion coefficient. This yields the nondimensional conservation equations (mass, momentum, energy) that depend on a single key parameter: the **Rayleigh number**.

[Mantle convection -- Wikipedia](https://en.wikipedia.org/wiki/Mantle_convection) | [Derivation of the Rayleigh-Benard equations for modeling convection in the Earth's mantle (UC Davis)](https://www.math.ucdavis.edu/~egp/PUBLICATIONS/PREPRINTS/2015/IC-EGP_SHORT_2015.pdf)

### 6.2 Rayleigh Number

The Rayleigh number measures the ratio of buoyancy-driven forces to viscous and thermal diffusive resistance:

```
Ra = (rho * g * alpha * DeltaT * d^3) / (kappa * eta)
```

where:
- `rho` = density (~3300 kg/m^3 for mantle)
- `g` = gravitational acceleration (~10 m/s^2)
- `alpha` = thermal expansion coefficient (~2 x 10^-5 K^-1)
- `DeltaT` = temperature difference across the layer (~2500 K for whole mantle)
- `d` = layer thickness (~2890 km = 2.89 x 10^6 m for whole mantle)
- `kappa` = thermal diffusivity (~10^-6 m^2/s)
- `eta` = kinematic viscosity (~10^17 m^2/s, corresponding to dynamic viscosity ~3 x 10^20 Pa-s)

**For internally heated convection:**
```
Ra_H = (g * rho_0^2 * beta * H * D^5) / (eta * alpha * k)
```

where `H` is the volumetric heating rate.

### Critical Rayleigh numbers:
- **Plane layer (Rayleigh-Benard):** Ra_c ~ **1,708** (free-free) to **1,100** (free-rigid)
- **Spherical shell:** Ra_c ~ **660**

### Earth's mantle Ra:
- Estimated at **10^6 to 10^8** -- roughly **10,000 times critical**
- This indicates vigorous, chaotic convection

[Rayleigh number -- Wikipedia](https://en.wikipedia.org/wiki/Rayleigh_number) | [Rayleigh Number -- ScienceDirect Topics](https://www.sciencedirect.com/topics/earth-and-planetary-sciences/rayleigh-number)

### 6.3 Nusselt Number and Heat Transfer Scaling

The Nusselt number is the ratio of total (convective + conductive) heat transfer to purely conductive heat transfer:

```
Nu = q_total / q_conductive
```

The scaling relationship between Nu and Ra:

```
Nu = a * Ra^beta
```

| Boundary condition | Exponent beta | Reference |
|---|---|---|
| Free-slip surfaces | ~1/3 (0.33) | Classical boundary layer theory |
| Rigid surfaces | ~1/5 (0.20) | Turcotte & Schubert |
| Basally heated spherical shell (numerical) | 0.294 +/- 0.004 | [Wolstencroft et al. (2009)](https://www.sciencedirect.com/science/article/abs/pii/S0031920109001216) |
| Internally heated (converted) | 0.337 +/- 0.009 | Wolstencroft et al. (2009) |
| Hard turbulence regime | ~2/7 (0.286) | Experimental |

**Practical significance:** Using beta = 0.29 instead of 1/3, an Ra of 10^9 gives a surface heat flux **~32% lower** than the classical 1/3 scaling would predict.

[Nusselt-Rayleigh scaling for spherical shell Earth mantle (Wolstencroft et al., 2009)](https://ui.adsabs.harvard.edu/abs/2009PEPI..176..132W/abstract) | [Scaling Laws in Rayleigh-Benard Convection (AGU, 2019)](https://agupubs.onlinelibrary.wiley.com/doi/full/10.1029/2019EA000583)

### 6.4 Whole-Mantle vs. Layered Convection

Two competing models:
- **Whole-mantle convection:** Material circulates from surface to core-mantle boundary. Supported by seismic tomography showing subducted slabs penetrating the 660-km discontinuity.
- **Layered convection:** The 660-km phase transition acts as a barrier. Upper and lower mantle convect separately.

Current consensus: predominantly **whole-mantle convection**, with the 660-km boundary causing temporary impediment but not permanent layering. The endothermic phase transition (spinel to perovskite) has a Clapeyron slope of about **-2 to -4 MPa/K**, which can slow but usually not permanently block slab penetration.

### 6.5 Convective Parameters for Earth's Mantle

- Convective velocities at surface (plate speeds): **1--10 cm/yr**
- Shallow convection cycle timescale: **~50 Myr**
- Deep convection cycle timescale: **~200 Myr**
- Typical mantle stresses: **3--30 MPa**
- Strain rates: **10^-14 to 10^-16 /s**
- Homologous temperature (T/T_melt): **0.65--0.75** for most of the mantle
- Primary upper mantle mineral: olivine (Mg,Fe)_2SiO_4

[Mantle convection -- Wikipedia](https://en.wikipedia.org/wiki/Mantle_convection)

---

## 7. When Does a Planet Become Geologically Dead?

A planet becomes "geologically dead" when its interior cools below the threshold needed to drive convection, volcanism, and magnetic dynamo generation. The primary factors are: **(1) initial heat content (scales with mass), (2) SA/V ratio, (3) radiogenic element budget, (4) tectonic regime efficiency**.

### 7.1 The Moon

- **Radius:** 1,737 km; **Mass:** 0.012 M_Earth
- Dynamo ceased: probably by **~3.5 Ga** (possibly as late as ~1 Ga in a weakened state)
- Major volcanism (mare basalts): **3.9--3.1 Ga**
- Traditionally considered geologically dead by **~1 Ga**
- Recent surprise: Chang'e 5 samples revealed basalts as young as **~2 Ga** [Two-billion-year-old volcanism on the Moon -- Nature (2021)](https://www.nature.com/articles/s41586-021-04100-2), and some irregular mare patches may be **~120 Ma** old [Recent volcanic eruptions on the Moon -- Science](https://www.science.org/content/article/recent-volcanic-eruptions-moon)
- No current magnetic field; no current volcanism

### 7.2 Mars

- **Radius:** 3,390 km; **Mass:** 0.107 M_Earth (11% of Earth's)
- Dynamo active: from formation to **~3.9 Ga** (possibly later; previously estimated at 4.1 Ga) [Revisiting timeline that pinpoints when Mars lost its dynamo -- Harvard Gazette (2023)](https://news.harvard.edu/gazette/story/2023/07/revisiting-timeline-that-pinpoints-when-mars-lost-its-dynamo/)
- Core: mostly or entirely liquid; may have a very small solid inner core. Composition differs from Earth (more sulfur). [Mars core study (Hsieh et al., 2024)](https://www.jsg.utexas.edu/lin/files/HsiehMarsDynamoFeSSciAd2024.pdf)
- Volcanism: Olympus Mons lava flows possibly as young as **~200 Ma**; Elysium Planitia flows possibly **~50 Ma**
- Loss of magnetic field --> atmospheric stripping by solar wind --> loss of surface liquid water
- **Not fully dead**, but approaching geological dormancy

### 7.3 Earth

- **Radius:** 6,371 km; **Mass:** 1.0 M_Earth
- Active plate tectonics, vigorous mantle convection (Ra ~ 10^7)
- Active magnetic dynamo (powered by inner core solidification + compositional convection)
- Inner core began solidifying: **~1--1.5 Ga** (possibly as recently as ~0.5 Ga based on some models)
- Surface heat flow: **47 TW**; will remain geologically active for **billions of years** more
- Expected to remain active until Sun enters red giant phase (~5 Gyr hence), though the dynamo may weaken as the core fully solidifies

### 7.4 Summary Comparison

| Property | Moon | Mars | Earth |
|---|---|---|---|
| Mass (M_Earth) | 0.012 | 0.107 | 1.0 |
| SA/V ratio (relative) | 3.7x | 1.9x | 1.0x |
| Dynamo duration | ~1 Gyr? | ~0.5--0.8 Gyr | >4.5 Gyr (ongoing) |
| Last volcanism | ~2 Ga (bulk); ~120 Ma (minor?) | ~50--200 Ma (localized) | Present |
| Current tectonic mode | Dead (stagnant lid) | Nearly dead (stagnant lid) | Active mobile lid |
| Current magnetic field | None | Crustal remnants only | Active dipole |

The fundamental scaling: **Mars had ~11% of Earth's mass and cooled to dynamo death in ~0.5--0.8 Gyr. The Moon had ~1.2% of Earth's mass and lost its dynamo even earlier. Earth, 10x more massive than Mars, retains its dynamo after 4.5 Gyr.**

[Planetary size and cooling rate -- Phys.org (2024)](https://phys.org/news/2024-01-planetary-size-cooling-mars-died.html) | [Mars Was Always Destined to Die -- TIME](https://time.com/6100276/mars-water-loss/) | [The Ages of Mars -- ESA](https://sci.esa.int/web/mars-express/-/55481-the-ages-of-mars)

---

## Sources

1. [Internal structure of Earth -- Wikipedia](https://en.wikipedia.org/wiki/Internal_structure_of_Earth)
2. [Earth's internal heat budget -- Wikipedia](https://en.wikipedia.org/wiki/Earth%27s_internal_heat_budget)
3. [Mantle convection -- Wikipedia](https://en.wikipedia.org/wiki/Mantle_convection)
4. [Plate tectonics -- Wikipedia](https://en.wikipedia.org/wiki/Plate_tectonics)
5. [Lid tectonics -- Wikipedia](https://en.wikipedia.org/wiki/Lid_tectonics)
6. [Rayleigh number -- Wikipedia](https://en.wikipedia.org/wiki/Rayleigh_number)
7. [Tidal heating -- Wikipedia](https://en.wikipedia.org/wiki/Tidal_heating)
8. [Geodynamics of terrestrial exoplanets -- Wikipedia](https://en.wikipedia.org/wiki/Geodynamics_of_terrestrial_exoplanets)
9. [Geodynamics of Venus -- Wikipedia](https://en.wikipedia.org/wiki/Geodynamics_of_Venus)
10. [Rayleigh Number -- ScienceDirect Topics](https://www.sciencedirect.com/topics/earth-and-planetary-sciences/rayleigh-number)
11. [Nusselt-Rayleigh number scaling for spherical shell Earth mantle -- Wolstencroft et al. (2009)](https://ui.adsabs.harvard.edu/abs/2009PEPI..176..132W/abstract)
12. [A Note on Planet Size and Cooling Rate -- Seales & Lenardic (2021)](https://ar5iv.labs.arxiv.org/html/2102.01077)
13. [Dissecting the puzzle of tectonic lid regimes -- Nature Communications (2025)](https://www.nature.com/articles/s41467-025-65943-1)
14. [What Keeps the Earth Cooking? -- Berkeley Lab (2011)](https://newscenter.lbl.gov/2011/07/17/kamland-geoneutrinos/)
15. [Revisiting timeline for Mars dynamo -- Harvard Gazette (2023)](https://news.harvard.edu/gazette/story/2023/07/revisiting-timeline-that-pinpoints-when-mars-lost-its-dynamo/)
16. [Two-billion-year-old volcanism on the Moon -- Nature (2021)](https://www.nature.com/articles/s41586-021-04100-2)
17. [Planetary size and cooling rate may explain why Mars died -- Phys.org (2024)](https://phys.org/news/2024-01-planetary-size-cooling-mars-died.html)
18. [Planetary cooling: The surface area to volume ratio -- University of Victoria](https://web.uvic.ca/~jwillis/teaching/astr201/maths.5.planetary_cooling.pdf)
19. [Inside the Earth -- USGS](https://pubs.usgs.gov/gip/dynamic/inside.html)
20. [Tidal Heating of Jupiter's and Saturn's Moons -- Stanford](http://large.stanford.edu/courses/2007/ph210/pavlichin2/)
21. [The dependence of planetary tectonics on mantle thermal state -- Royal Society (2018)](https://royalsocietypublishing.org/doi/10.1098/rsta.2017.0409)
22. [Stagnant lid tectonics -- Stern et al. (2018)](https://www.sciencedirect.com/science/article/pii/S1674987117301135)
