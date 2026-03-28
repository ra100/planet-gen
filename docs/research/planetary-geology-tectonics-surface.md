# Planetary Geology, Tectonics, and Surface Processes

**Deep Research Report**
**Date: 2026-03-28**

---

## 1. Plate Tectonics vs Stagnant Lid

### 1.1 Conditions for Plate Tectonics

Plate tectonics requires that convective stresses in the mantle exceed the yield strength of the lithosphere. The key controlling parameters are:

**Planet mass range**: Research shows mixed results. Larger, cooler planets may favor plate tectonics due to higher Rayleigh numbers, but for standard initial temperature estimates of super-Earths, surface mobilization is *less* likely than on Earth. Warm initial conditions reverse this conclusion. The commonly cited range of 0.5--5 M_Earth is a rough guideline; initial thermal state has a first-order influence ([Noack & Breuer 2014, ScienceDirect](https://www.sciencedirect.com/science/article/abs/pii/S003206331300161X); [O'Neill et al. 2007, ScienceDirect](https://www.sciencedirect.com/science/article/abs/pii/S0012821X07003457)).

**Surface water requirement**: Water is critical for plate tectonics. It percolates into oceanic lithosphere through fractures, reacts with minerals to form hydrous phases (serpentine), lowers lithospheric strength, and lubricates subduction zones. Without surface water, serpentinization cannot weaken plates sufficiently for subduction to initiate ([Water & fracture zones, Oxford Academic](https://academic.oup.com/gji/article/204/3/1405/676315); [Subduction, Wikipedia](https://en.wikipedia.org/wiki/Subduction)).

**Mantle viscosity thresholds**: Upper mantle viscosity estimates range from 10^19 to 10^24 Pa-s depending on depth, temperature, and composition. For plate tectonics, damage must reduce the viscosity of lithospheric shear zones to a value comparable to the underlying mantle viscosity (~10^21 Pa-s for Earth's upper mantle) ([PNAS, mantle viscosity inversions](https://www.pnas.org/doi/10.1073/pnas.2318706121); [Royal Society](https://royalsocietypublishing.org/doi/10.1098/rsta.2017.0409)).

### 1.2 Stagnant Lid Convection

In the stagnant lid regime, the surface is locked as a single immobile plate with no subduction. This is the dominant tectonic mode for most rocky bodies in the Solar System:

| Body  | Tectonic Mode | Heat Flow (mW/m^2) | Notes |
|-------|---------------|---------------------|-------|
| Earth | Mobile lid (plate tectonics) | ~87 (average 86--95) | Bimodal crust |
| Venus | Stagnant lid (episodic?) | ~31 (10--40 range) | Possible past resurfacing event |
| Mars  | Stagnant lid | ~19 (14--25 range) | Archetype of stagnant lid |
| Moon  | Stagnant lid | ~12--18 | Apollo 15/17 measurements |

Heat transport efficiency in the stagnant lid regime is extremely low compared to plate tectonics. Without plate tectonics, Earth's mantle temperature would be 700--1500 K higher for the same surface heat flux ([Reese et al. 1998, ADS](https://ui.adsabs.harvard.edu/abs/1998JGR...10313643R/abstract); [Lid tectonics, Wikipedia](https://en.wikipedia.org/wiki/Lid_tectonics)).

Heat flow sources:
- Earth: [Present-day heat flow model of Mars, Nature](https://www.nature.com/articles/srep45629)
- Venus: [Venus lithosphere strength, Nature](https://www.nature.com/articles/s43247-026-03278-5)
- Mars: [PMC](https://pmc.ncbi.nlm.nih.gov/articles/PMC5377363/)
- Moon: [Apollo 15 heat flow, Springer](https://link.springer.com/article/10.1007/BF00562006)

### 1.3 Rayleigh Number for Mantle Convection

The Rayleigh number determines whether convection occurs and its vigor.

**Thermal Rayleigh number (bottom-heated)**:

```
Ra_T = (rho * g * alpha * DeltaT * D^3) / (kappa * eta)
```

Where:
- rho = mantle density (~4000 kg/m^3)
- g = gravitational acceleration (9.8 m/s^2 for Earth)
- alpha = thermal expansion coefficient (~2 x 10^-5 K^-1)
- DeltaT = superadiabatic temperature difference across mantle (~2500 K)
- D = mantle depth (2890 km = 2.89 x 10^6 m)
- kappa = thermal diffusivity (~10^-6 m^2/s)
- eta = dynamic viscosity (~10^21 Pa-s)

**Internal heating Rayleigh number**:

```
Ra_H = (g * rho^2 * beta * H * D^5) / (eta * alpha_thermal * k)
```

Where H = radiogenic heat production per unit mass, k = thermal conductivity.

**Key values**:
- Critical Ra for onset of convection: ~10^3 (exact value depends on geometry and boundary conditions)
- Earth's mantle Ra: ~10^8 (about one hundred million), indicating vigorous, chaotic convection
- At Ra ~ 10^6, orderly convection cells become disrupted
- The critical Ra would be attained for a temperature difference of only 0.025 K across Earth's mantle

([Rayleigh number, Wikipedia](https://en.wikipedia.org/wiki/Rayleigh_number); [Interactive Earth](https://ian-r-rose.github.io/interactive_earth/explanation.html); [ScienceDirect Rayleigh number overview](https://www.sciencedirect.com/topics/earth-and-planetary-sciences/rayleigh-number))

**Nusselt-Rayleigh scaling** (heat transfer efficiency):

```
Nu ~ Ra^beta
```

Where beta ~ 0.3 for isoviscous convection, relating convective heat transport (Nu) to vigor of convection (Ra).

---

## 2. Volcanism Types

### 2.1 Shield Volcanoes (Hawaiian-type)

**Composition**: Predominantly basaltic (mafic), SiO2 ~45--52 wt%
**Lava viscosity**: 10--100 Pa-s (10^1--10^2 Pa-s)
**Eruption temperature**: 1100--1200 degC
**Slope angles**: 2--3 deg near base, steepening to ~10 deg; average 5--9.4 deg

**Dimensions (Earth examples)**:

| Volcano | Height | Diameter | Volume |
|---------|--------|----------|--------|
| Mauna Loa | 4,169 m above sea level (9 km from seafloor) | >100 km base | ~80,000 km^3 |
| Michoacan-Guanajuato average | 340 m | 4,100 m | 1.7 km^3 |
| California/Oregon shields | 500--600 m | 5--6 km | -- |

Height-to-width ratio: approximately 1:20.
Typical lava flow thickness: <1 m. About 58% of Kilauea's lava is delivered via lava tubes.

([Shield volcano, Wikipedia](https://en.wikipedia.org/wiki/Shield_volcano); [Morphometry of terrestrial shield volcanoes, ScienceDirect](https://www.sciencedirect.com/science/article/abs/pii/S0169555X17305263))

### 2.2 Stratovolcanoes (Composite Volcanoes)

**Composition**: Andesitic to rhyolitic (intermediate to felsic), SiO2 ~60--70 wt%
**Lava viscosity by type**:
- Andesite: ~3,500 Pa-s at 1200 degC
- Dacite-rhyolite (hot, 1200 degC): ~10^5 Pa-s
- Rhyolite (cool, 800 degC): ~10^8 Pa-s

**Eruption temperature**: 850--1100 degC (andesite)
**Slope angles**: 30--35 deg (steep-sided due to high viscosity)
**Lava flow thickness**: 50--500 m
**Lava flow length**: Typically only a few km

([Stratovolcano, Wikipedia](https://en.wikipedia.org/wiki/Stratovolcano); [Andesitic to Rhyolitic Lava, SDSU](https://volcanoes.sdsu.edu/andesiterhyolite_lava.html); [Lava viscosity, Wikipedia Magma](https://en.wikipedia.org/wiki/Magma))

### 2.3 Flood Basalts (Large Igneous Provinces)

| Province | Volume | Area | Age |
|----------|--------|------|-----|
| Deccan Traps (India) | ~1,000,000 km^3 | ~500,000 km^2 | ~66 Ma |
| Siberian Traps (Russia) | 1,000,000--4,000,000 km^3 | ~5,000,000 km^2 | ~250 Ma |
| Columbia River Basalts (USA) | ~175,000 km^3 | ~160,000 km^2 | ~17--6 Ma |

Deccan Traps: >2 km total thickness of solidified flood basalt layers.

([Deccan Traps, Wikipedia](https://en.wikipedia.org/wiki/Deccan_Traps); [Siberian Traps, Wikipedia](https://en.wikipedia.org/wiki/Siberian_Traps); [Flood Basalts, Oregon State](https://volcano.oregonstate.edu/flood-basalts))

### 2.4 Cryovolcanism

**Definition**: Eruption of volatile materials (water, ammonia, methane, nitrogen) instead of silicate magma. Erupted material = "cryolava" from subsurface "cryomagma" reservoirs.

**Known/suspected cryovolcanic bodies**:
- Enceladus (Saturn): Active water-ice geysers from south polar region
- Triton (Neptune): Active nitrogen geysers
- Europa (Jupiter): Suspected water-ice volcanism
- Pluto: Large-scale cryovolcanic terrain, multiple domes several km high, total volume >10^4 km^3
- Titan (Saturn): Suspected methane/ammonia volcanism

([Cryovolcano, Wikipedia](https://en.wikipedia.org/wiki/Cryovolcano); [Pluto cryovolcanism, Nature Communications](https://www.nature.com/articles/s41467-022-29056-3))

### 2.5 Olympus Mons (Mars)

The largest known volcano in the Solar System:

| Parameter | Value |
|-----------|-------|
| Height above datum | 21.287 km (MOLA) |
| Local relief above plains | 21.9--26 km |
| Diameter | 600 km (370 mi) |
| Surface area | ~300,000 km^2 (size of Italy) |
| Caldera complex | 60 x 80 km, up to 3.2 km deep, 6 nested calderas |
| Escarpment height | Up to 8 km |
| Average flank slope | 5% (~2.9 deg) |
| Summit atmospheric pressure | 72 Pa (12% of Mars surface average of 600 Pa) |
| Lithosphere thickness | ~70 km beneath the edifice |
| Youngest lava flows | ~2 Ma |
| Last eruption | ~25 Ma |
| Caldera ages | 350--150 Ma |
| Estimated magma chamber depth | ~32 km below caldera floor |

Formed by basaltic shield volcanism in a single-plate (stagnant lid) regime, allowing the edifice to grow over a stationary hotspot for billions of years without plate motion redistributing volcanism.

([Olympus Mons, Wikipedia](https://en.wikipedia.org/wiki/Olympus_Mons); [Olympus Mons, Britannica](https://www.britannica.com/place/Olympus-Mons))

---

## 3. Impact Cratering

### 3.1 Crater Scaling Laws (Pi-Scaling)

The Holsapple-Schmidt pi-scaling framework uses dimensionless groups:

**Pi-groups**:
- pi_D = D_crater / d_projectile * (rho_t / rho_p)^(1/3) -- scaled crater diameter
- pi_2 = (g * d_projectile) / v_i^2 -- gravity-scaled size (ratio of gravitational to inertial stresses)
- pi_3 = Y / (rho_t * v_i^2) -- strength-scaled size
- pi_4 = rho_t / rho_p -- density ratio

Where D_crater = transient crater diameter, d_projectile = projectile diameter, rho_t = target density, rho_p = projectile density, g = surface gravity, v_i = impact velocity, Y = target strength.

**Scaling in two regimes**:

*Gravity regime* (pi_2 >> pi_3, large craters):
```
pi_D = C_D * pi_2^(-mu_g)
```

*Strength regime* (pi_3 >> pi_2, small craters):
```
pi_D = C_D * pi_3^(-mu_s)
```

**Exponent mu**:
- mu = 1/3 if crater size scales with momentum
- mu = 2/3 if crater size scales with energy (point-source limit)
- Typical values: mu ~ 0.41 (sand), mu ~ 0.55 (rock)

**Simplified energy scaling**:
- Gravity regime: D proportional to E^(1/4) (quarter-root scaling)
- Strength regime: D proportional to E^(1/3) (cube-root scaling)

([LPI Theory PDF](https://www.lpi.usra.edu/lunar/tools/lunarcratercalc/theory.pdf); [Holsapple & Schmidt, ResearchGate](https://www.researchgate.net/publication/240484956_On_the_Scaling_of_Crater_Dimensions_2_Impact_Processes); [Prieur et al. 2017, Wiley](https://agupubs.onlinelibrary.wiley.com/doi/full/10.1002/2017je005283))

### 3.2 Simple vs Complex Crater Transition

The transition diameter D_t scales inversely with surface gravity:

```
D_t ~ constant / g
```

| Body | Gravity (m/s^2) | Transition Diameter |
|------|-----------------|---------------------|
| Earth (sediment) | 9.8 | ~2 km |
| Earth (crystalline rock) | 9.8 | ~4 km |
| Mars | 3.7 | ~3--8 km (avg ~5--8 km) |
| Moon (mare) | 1.6 | ~14 km |
| Moon (highland) | 1.6 | ~17 km |

Transition from transitional to fully complex morphology on the Moon: ~24 km (mare), ~28 km (highland).

([CosmoQuest](https://cosmoquest.org/x/2017/12/cq-science-post-6-simple-to-complex/); [Robbins & Hynek 2012, Wiley](https://agupubs.onlinelibrary.wiley.com/doi/full/10.1029/2011JE003967); [Kruger et al. 2018, Wiley](https://agupubs.onlinelibrary.wiley.com/doi/full/10.1029/2018JE005545))

### 3.3 Depth/Diameter Ratios

| Crater Type | d/D Ratio | Characteristics |
|-------------|-----------|-----------------|
| Simple | ~1:5 (d/D ~ 0.2) | Bowl-shaped, parabolic cross-section |
| Complex (small) | ~1:10 (d/D ~ 0.1) | Central peak, flat floor, terraced walls |
| Complex (large) | ~1:20 (d/D ~ 0.05) | Shallower with increasing size |
| Peak-ring basins | < 1:20 | Ring of peaks replaces central peak |

Below ~10 km on the Moon, d/D follows a power law that decreases with increasing crater size.

**Example**: Barringer Crater (Earth) -- 1.19 km diameter, ~170 m apparent depth, ~300 m true depth (d/D ~ 0.14--0.25).

([Crater Explorer](https://craterexplorer.ca/crater-classification/); [LPI Education](https://www.lpi.usra.edu/education/explore/shaping_the_planets/impact-cratering/); [Melosh Chapter 6, UChicago](https://geosci.uchicago.edu/~kite/doc/Melosh_ch_6.pdf))

### 3.4 Size-Frequency Distribution

The cumulative size-frequency distribution follows a power law:

```
N(>D) = k * D^(-b)
```

Where N(>D) = cumulative number of craters larger than diameter D per unit area.

**Exponent b values**:
- b ~ 2 for intermediate crater sizes (commonly used reference slope)
- b ~ 2.4 measured at Apollo 11 site for D = 2--40 m: N(x) = 22000 * x^(-2.4) craters/km^2
- b varies from ~1 to ~4 depending on diameter range
- The Neukum production function uses an 11th-degree polynomial in log(D), valid for D = 10 m to 300 km

The power law is not a single slope but changes across diameter ranges, requiring piecewise or polynomial fits.

([Power-law scaling, Progress in Physics](https://www.progress-in-physics.com/2016/PP-44-04.PDF); [Ivanov et al. 2002, SwRI](https://www2.boulder.swri.edu/~bottke/Reprints/Ivanov-etal_2002_AstIII_Craters.pdf); [Robbins 2018, Wiley](https://onlinelibrary.wiley.com/doi/full/10.1111/maps.12990))

### 3.5 Peak Ring and Multi-Ring Basin Thresholds

| Morphology | Onset Diameter (Moon) | Onset Diameter (Mercury) |
|------------|----------------------|--------------------------|
| Central peak | Simple-complex transition | -- |
| Peak-ring basin | Largest onset in inner Solar System | 126 km (+33/-26) |
| Multi-ring basin | >~300 km | -- |

Progression with increasing size: central-peak crater -> peak-ring crater -> multi-ring basin.

The onset of peak-ring morphology depends on both gravitational acceleration and mean impact velocity, relating to the depth of melting relative to the transient cavity depth.

([Baker et al. 2012, Wiley](https://agupubs.onlinelibrary.wiley.com/doi/full/10.1029/2011je004021); [Peak ring, Wikipedia](https://en.wikipedia.org/wiki/Peak_ring); [Multi-ringed basin, Wikipedia](https://en.wikipedia.org/wiki/Multi-ringed_basin); [Complex crater, Wikipedia](https://en.wikipedia.org/wiki/Complex_crater))

---

## 4. Erosion Processes

### 4.1 Fluvial Erosion: Stream Power Law

```
E = K * A^m * S^n
```

Where:
- E = erosion rate (m/yr or mm/kyr)
- K = erodibility coefficient (depends on lithology, climate, sediment flux; units vary with m and n)
- A = upstream drainage area (m^2) -- proxy for discharge
- S = local channel gradient (dimensionless)
- m, n = positive exponents

**Exponent constraints**:
- Concavity index: theta = m/n ~ 0.45--0.5 (Hack 1957)
- m ~ 0.3--0.5 (commonly ~0.5)
- n ~ 1--2 (commonly ~1, but observations suggest n > 1 for threshold-controlled incision)
- The ratio m/n ~ 0.5 is well-constrained from equilibrium river profiles

K varies by orders of magnitude: from ~10^-8 to 10^-3 depending on rock type and climate.

The equation derives from conservation of water mass and momentum combined with channel hydraulic geometry and basin hydrology relationships.

([Stream power law, Wikipedia](https://en.wikipedia.org/wiki/Stream_power_law); [Whipple & Tucker 1999, UChicago](https://sseh.uchicago.edu/doc/Whipple_and_Tucker_1999.pdf); [Global 10Be analysis, ScienceDirect](https://www.sciencedirect.com/science/article/abs/pii/S0169555X16303907))

### 4.2 Glacial Erosion Rates

Glacial erosion rates span seven orders of magnitude, strongly dependent on thermal regime, sliding velocity, and lithology:

| Setting | Erosion Rate (mm/yr) |
|---------|---------------------|
| Polar glaciers / cold-based ice | 0.001--0.01 |
| Thin temperate plateau glaciers on crystalline bedrock | ~0.01 |
| Temperate valley glaciers on resistant crystalline bedrock (Norway) | ~0.1 |
| Small temperate glaciers, diverse bedrock (Swiss Alps) | ~1.0 |
| Large fast-moving temperate valley glaciers (SE Alaska) | 10--100 |

Key controls: basal sliding velocity, subglacial hydrology, basal thermal regime, precipitation, and lithology. Erosion rates vary by up to a factor of 100 for a given sliding velocity.

([Hallet et al. 1996, ScienceDirect](https://www.sciencedirect.com/science/article/pii/0921818195000216); [Herman et al. 2015, Nature Geoscience](https://www.nature.com/articles/s41561-025-01747-8); [Empirical basis for glacial erosion models, Nature Comms](https://www.nature.com/articles/s41467-020-14583-8))

### 4.3 Aeolian Erosion: Saltation Threshold

**Bagnold's threshold friction velocity**:

```
u*_t = A * sqrt((rho_s / rho_a - 1) * g * d)
```

Where:
- u*_t = threshold friction velocity (m/s)
- A = empirical coefficient (~0.1; specifically 0.118 for particle friction Reynolds number Re*_p > 10)
- rho_s = sediment grain density (~2650 kg/m^3 for quartz)
- rho_a = air density (1.225 kg/m^3 on Earth; ~0.02 kg/m^3 on Mars)
- g = gravitational acceleration
- d = grain diameter

**Key values**:
- Earth: At wind speeds ~30 m/s, upper limit on transported quartz grain diameter ~0.5 mm
- Earth: Impact/fluid threshold ratio ~0.82
- Mars: Impact threshold is approximately one order of magnitude below the fluid threshold (due to low atmospheric density)
- Mars: Saltation, once initiated, is sustained at much lower wind speeds than required to start it

Saltation threshold depends on soil moisture, clay content, vegetation, armoring, and cementation.

([Bagnold 1941; Physics of Aeolian sand transport, HAL](https://hal.science/hal-01115982/document); [Saltation threshold Earth Mars Venus, ResearchGate](https://www.researchgate.net/publication/229747215_Saltation_threshold_on_Earth_Mars_and_Venus); [Lower-than-expected threshold on Mars, PNAS](https://www.pnas.org/content/118/5/e2012386118))

### 4.4 Chemical Weathering: Arrhenius Temperature Dependence

Chemical weathering rate follows an Arrhenius-type equation:

```
W = W_0 * exp(-E_a / (R * T))
```

Where:
- W = weathering rate
- W_0 = pre-exponential factor
- E_a = apparent activation energy (kJ/mol)
- R = gas constant (8.314 J/(mol*K))
- T = temperature (K)

**Activation energies for silicate dissolution**:

| Mineral | E_a (kJ/mol) | Notes |
|---------|-------------|-------|
| Generic silicates | ~60 | 6x rate increase from 5 to 25 degC |
| Orthoclase | ~36 | Near-neutral pH |
| Plagioclase | ~107 | Near-neutral pH |
| Global average (humid sites) | 56 +/- 8 | Humidity index > 0.55 |

**Practical effect**: An Arrhenius-predicted 3.5--9x increase in plagioclase dissolution rate as temperature rises from 3.4 to 22 degC.

**Chemical denudation rates**: 0.2--5 mm per 1000 years in alpine environments (higher in carbonates than crystalline rocks).

This temperature dependence forms the basis of the silicate weathering thermostat, a key negative feedback stabilizing Earth's climate over geological timescales.

([Science, silicate weathering thermostat](https://www.science.org/doi/10.1126/science.add2922); [PMC, global temperature control](https://pmc.ncbi.nlm.nih.gov/articles/PMC8980099/); [ScienceDirect, weathering rate overview](https://www.sciencedirect.com/topics/earth-and-planetary-sciences/weathering-rate))

### 4.5 Landscape Evolution Timescales

Landscape evolution timescales can be estimated from erosion rates:

- **Fluvial**: At typical bedrock incision rates of 0.01--1 mm/yr, a 1 km deep valley requires 10^6--10^8 years
- **Glacial**: Alpine glaciers at 1 mm/yr erode 1 km in ~10^6 years; fast Alaskan glaciers at 10--100 mm/yr in 10^4--10^5 years
- **Aeolian**: Generally the slowest; desert deflation rates ~0.001--0.01 mm/yr
- **Chemical weathering**: 0.0002--0.005 mm/yr, contributing over 10^7--10^9 year timescales

([Limits to timescale dependence, Science Advances](https://www.science.org/doi/10.1126/sciadv.adr2009))

---

## 5. Hypsometric Curves

### 5.1 Earth: Bimodal Distribution

Earth's hypsometric curve is uniquely bimodal among Solar System bodies, reflecting two distinct crustal types:

**Two elevation modes**:
- Continental platform: mode at ~+100 m (mean ~+840 m above sea level)
- Deep-sea floor: mode at ~-4700 m (mean ~-3800 m below sea level)

**Key statistics**:
- 29% of surface above sea level
- 85% of surface falls within two narrow elevation bands: (+2000 m to -500 m) and (-3000 m to -6000 m)
- 95% of surface is above -6 km

**Physical basis**: The bimodal distribution arises from the density contrast between continental crust (rho ~ 2700 kg/m^3, thickness 30--70 km, mean ~35 km) and oceanic crust (rho ~ 3000 kg/m^3, thickness ~5--7 km). This bimodality is intrinsically linked to plate tectonics.

([Hypsometry, Wikipedia](https://en.wikipedia.org/wiki/Hypsometry); [ETOPO1 Hypsographic Curve, NCEI](https://www.ncei.noaa.gov/sites/g/files/anmtlf171/files/2023-01/Hypsographic%20Curve%20of%20Earth%E2%80%99s%20Surface%20from%20ETOPO1.pdf); [Rowley 2013, Journal of Geology](https://pubs.geoscienceworld.org/ucp/the-journal-of-geology/article/121/5/445/622554/Sea-Level-Earth-s-Dominant-Elevation-Implications))

### 5.2 Mars: Bimodal with Dichotomy

Mars also has a bimodal hypsometric distribution, reflecting the hemispheric dichotomy:

- **Southern Highlands**: 5--6 km higher elevation, 30 km thicker crust
- **Northern Lowlands**: ~42% of surface, relatively flat (slopes typically <0.5 deg)
- Elevation difference between hemispheres: 1--3 km (up to 5--6 km locally)
- When elevations are referenced to the center of figure rather than the geoid, the bimodal distribution becomes unimodal

The dichotomy may have originated from a giant impact, degree-1 mantle convection, or a hybrid of both.

([Martian dichotomy, Wikipedia](https://en.wikipedia.org/wiki/Martian_dichotomy); [Hypsometric curve of Mars, Springer](https://link.springer.com/article/10.1007/BF00898431); [MOLA topography statistics, Wiley](https://agupubs.onlinelibrary.wiley.com/doi/abs/10.1029/2000JE001403))

### 5.3 Venus: Unimodal, Narrow Distribution

Venus has a distinctly unimodal hypsometric distribution:

- **51% of surface** within 500 m of the median radius (6,052 km)
- **80% of surface** within 1 km of the median radius (Magellan data)
- Only one dominant surface rock type implied
- Average crustal thickness: 10--20 km for lowlands/plains (>75% of surface)
- Plateau (tessera) crust: ~20--30 km thick (<15% of surface)

**Comparison of crustal thickness contrasts**:
- Earth: continental/oceanic = 40/5 km = 35 km difference (ratio 8:1)
- Venus: plateaus/lowlands = ~30/15 km = 15 km difference (ratio 2:1)

The narrow unimodal distribution is consistent with the absence of plate tectonics and the lack of compositionally distinct oceanic/continental crust.

([Rosenblatt et al. 1994, Wiley](https://agupubs.onlinelibrary.wiley.com/doi/abs/10.1029/94GL00419); [Crustal formation on Venus, Springer](https://link.springer.com/article/10.1007/BF00142388); [Geology of Venus, Wikipedia](https://en.wikipedia.org/wiki/Geology_of_Venus))

### 5.4 Mathematical Models for Hypsometry

**Cumulative hypsometric curve**: H(a) gives the fraction of surface area above elevation a.

For a planet with two crustal types (like Earth), the distribution can be modeled as a bimodal Gaussian:

```
f(z) = (f_c / sigma_c) * phi((z - mu_c) / sigma_c) + (f_o / sigma_o) * phi((z - mu_o) / sigma_o)
```

Where:
- f_c, f_o = fractional areas of continent/ocean (~0.29, ~0.71 for Earth)
- mu_c, mu_o = mean elevations of each mode
- sigma_c, sigma_o = standard deviations of each mode
- phi = standard normal distribution function

The hypsometric integral (HI) = area under the normalized hypsometric curve, ranges from 0 (fully eroded) to 1 (uneroded). Earth's global HI ~ 0.45.

Statistical moments (mean, variance, skewness, kurtosis) of the hypsometric curve and its density function provide quantitative descriptors of planetary topography shape.

([Statistical moments of hypsometric curve, Springer](https://link.springer.com/article/10.1007/BF01033300); [Earth's hypsometry and sea level, ScienceDirect](https://www.sciencedirect.com/science/article/pii/S0012821X2400503X))

---

## Summary Table: Key Numerical Parameters

| Parameter | Value | Source |
|-----------|-------|--------|
| Earth heat flow | 86--95 mW/m^2 | Global average |
| Mars heat flow | 14--25 mW/m^2 (avg 19) | Model estimates |
| Venus heat flow | ~31 mW/m^2 | Stagnant lid models |
| Moon heat flow | 12--18 mW/m^2 | Apollo measurements |
| Earth Ra | ~10^8 | Vigorous convection |
| Critical Ra | ~10^3 | Convection onset |
| Olympus Mons height | 21.287 km | MOLA |
| Olympus Mons diameter | 600 km | -- |
| Deccan Traps volume | ~10^6 km^3 | -- |
| Basalt viscosity | 10--100 Pa-s | At ~1200 degC |
| Rhyolite viscosity | 10^5--10^8 Pa-s | 800--1200 degC |
| Crater d/D (simple) | ~0.2 | Bowl-shaped |
| Crater d/D (complex) | 0.05--0.1 | Flat-floored |
| Stream power m/n ratio | ~0.45--0.5 | Concavity index |
| Glacial erosion range | 0.001--100 mm/yr | Polar to temperate |
| Silicate weathering E_a | 36--107 kJ/mol | Mineral-dependent |
| Bagnold coefficient A | 0.118 | For Re*_p > 10 |
| Earth continental mode | ~+100 m | Hypsometric peak |
| Earth ocean floor mode | ~-4700 m | Hypsometric peak |
