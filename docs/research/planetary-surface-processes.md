# Planetary Surface Processes and Terrain Generation

**Research date:** 2026-03-27

This document covers the major geological processes that shape planetary surfaces, with quantitative data suitable for procedural terrain generation. Each section includes specific rates, distributions, and physical parameters with citations.

---

## Table of Contents

1. [Impact Cratering](#1-impact-cratering)
2. [Volcanism](#2-volcanism)
3. [Erosion](#3-erosion)
4. [Sedimentation and Deposition](#4-sedimentation-and-deposition)
5. [Tectonic Features](#5-tectonic-features)
6. [Weathering](#6-weathering)
7. [Sources](#7-sources)

---

## 1. Impact Cratering

### 1.1 Size-Frequency Distribution (SFD)

Impact crater populations follow a **power-law size-frequency distribution**. The cumulative number of craters N larger than diameter D per unit area follows:

```
N(>D) = k * D^(-b)
```

where b is the cumulative SFD slope. The earliest numerical study (Young, 1940) estimated b ~= 2.5. Three major "standard" production distributions are used in planetary science: (a) a simple -2 power law, (b) the **Neukum Production Function** (NPF), and (c) Hartmann's incremental system.

The Neukum Production Function (Neukum 1983, Neukum et al. 2001) is an 11th-degree polynomial fit to the cumulative SFD, revealing **multi-slope structure**:

- For crater radii r < ~2 km: steep slope, cumulative exponent b ~= 3
- For 2 km < r < ~30 km: **shallow region** (the "hump")
- For larger craters and basins: steep again
- For D < 250 m: cumulative slope of -3.82
- For D between 10--23 m: cumulative slope of -3.0

Two distinct impactor populations have been identified. **Population 1** (dominant during heavy bombardment) had a complex multi-sloped SFD similar to the asteroid belt. **Population 2** (dominant since ~3.7 Ga) produces craters following a differential **-3 single-slope power law**.

[Ivanov et al.: Size-Frequency Distribution](https://www2.boulder.swri.edu/~bottke/Reprints/Ivanov-etal_2002_AstIII_Craters.pdf) (2002);
[Slopes of Lunar Crater Size-Frequency Distributions at Copernican-Aged Craters](https://agupubs.onlinelibrary.wiley.com/doi/full/10.1029/2023JE007816) (2023);
[PSI: Power Law Distributions](https://www.psi.edu/research/mgs/powerlaw.html)

### 1.2 Late Heavy Bombardment (LHB)

The Late Heavy Bombardment (also called the lunar cataclysm) occurred approximately **4.1 to 3.8 billion years ago** (some models extend it to 4.2--3.5 Ga). During this period, the inner solar system experienced an elevated impact flux, possibly triggered by giant planet migration (the Nice model).

**Extrapolated cratering on Earth during the LHB:**
- **~22,000+ craters** with diameter > 20 km
- **~40 impact basins** with diameter ~1,000 km
- **Several basins** with diameter ~5,000 km

Impact velocities during this period were elevated: the current asteroid belt average is ~5 km/s, but during the LHB velocities reached ~10 km/s. Melt volume increases **100--1,000x** when velocity increases from 5 to 10 km/s.

The bombardment rate decreased gradually from ~3.8 Ga until ~3.0 Ga, implying an **extended tail**. Between 3.0 and 1.0 Ga, bombardment may have been characterized by long periods (>600 Myr) of relative quiescence broken by shorter episodes (~200 Myr) of elevated flux.

[Late Heavy Bombardment -- Wikipedia](https://en.wikipedia.org/wiki/Late_Heavy_Bombardment);
[Cataclysm No More: New Views on Timing and Delivery of Lunar Impactors](https://pmc.ncbi.nlm.nih.gov/articles/PMC5602003/) (2017);
[Ages of Large Lunar Impact Craters and Implications for Bombardment](https://www.sciencedirect.com/science/article/abs/pii/S0019103513001322) (2013)

### 1.3 Saturation Equilibrium

On ancient surfaces with low erosion, crater density eventually reaches **saturation equilibrium**: for each new crater formed, approximately one crater of similar size is destroyed. Observed equilibrium densities are:

- **5--10% of geometric saturation** (the theoretical maximum circle-packing density)
- Relative density (R) values between **0.1 and 0.3**
- Saturation is **size-dependent**: small craters on old surfaces reach saturation at ~1% geometric saturation, while large craters persist longer due to their resistance to destruction

The crater diameter at which saturation occurs depends on surface age -- younger surfaces only saturate at small diameters, while older surfaces may saturate at progressively larger sizes.

[Saturation and Equilibrium Conditions for Impact Cratering on the Lunar Surface](https://agupubs.onlinelibrary.wiley.com/doi/abs/10.1029/RS005i002p00273) (Gault, 1970);
[Cratering Saturation and Equilibrium: A New Model](https://www.sciencedirect.com/science/article/abs/pii/S0019103509003194) (2009);
[The Equilibrium Size-Frequency Distribution of Small Craters](https://www.sciencedirect.com/science/article/pii/S0019103517308370) (2018)

---

## 2. Volcanism

### 2.1 Magma Generation Mechanisms

Three primary mechanisms generate magma in planetary interiors:

1. **Decompression melting** -- As mantle rock rises (at divergent boundaries or hotspots), reduced pressure lowers the solidus temperature, causing partial melting. This is the dominant mechanism at mid-ocean ridges.
2. **Flux melting (subduction)** -- Water released from subducting oceanic crust lowers the melting point of overlying mantle wedge, generating hydrous magmas typical of arc volcanism.
3. **Hotspot / mantle plume volcanism** -- Deep mantle plumes deliver anomalously hot material to the base of the lithosphere, producing localized volcanism (e.g., Hawaii, Yellowstone). Hotspot melt production rate during plume-mid-ocean-ridge interaction is **~7x higher** than after the interaction, because thinner lithospheric coverage allows greater decompression melting.

[An Introduction to Geology: Igneous Processes and Volcanoes](https://opengeology.org/textbook/4-igneous-processes-and-volcanoes/);
[Plate Tectonics and Volcanic Activity -- National Geographic](https://education.nationalgeographic.org/resource/plate-tectonics-volcanic-activity/)

### 2.2 Eruption Rates on Earth

**Global magma production:** ~**20--25 km^3/year** currently; averaged over the last 180 Myr: **26--34 km^3/year**.

| Setting | Output Rate |
|---|---|
| **Mid-ocean ridges (total)** | ~19 km^3/year of new oceanic crust (2.7 km^2/year of seafloor, ~7 km thick) |
| **Mid-ocean ridges (erupted lava)** | ~3 km^3/year |
| **Continental volcanic systems** | ~1 km^3/year |
| **Hawaii hotspot** | ~0.21 km^3/year (210 km^3/ka) |
| **Kilauea (current)** | ~0.1 km^3/year |
| **Yellowstone hotspot** | ~0.002 km^3/year (2 km^3/ka) |
| **Society Islands hotspot** | ~0.036 km^3/year |
| **Subduction arcs (Lesser Antilles)** | ~0.003 km^3/year |
| **Mount Etna** | ~0.0016 km^3/year |
| **Laki-Grimsvotn eruption (1783--85)** | 7.25 km^3/year (peak) |

The unit 1 Armstrong Unit = 1 km^3/year (32 m^3/s) is used in volcanology.

[Magma Supply Rate -- Wikipedia](https://en.wikipedia.org/wiki/Magma_supply_rate);
[Mid-ocean Ridge -- Wikipedia](https://en.wikipedia.org/wiki/Mid-ocean_ridge);
[Rates of Magma Emplacement and Volcanic Output (Crisp, 1984)](https://www.sciencedirect.com/science/article/abs/pii/0377027384900398);
[Global Rates of Subaerial Volcanism on Earth (Frontiers, 2022)](https://www.frontiersin.org/journals/earth-science/articles/10.3389/feart.2022.922160/full)

### 2.3 Volcanism on Other Bodies

- **Mars**: Olympus Mons is 550 km across and 21 km high, ~100x the volume of Mauna Loa. The Tharsis region covers ~25% of the Martian surface, averaging 7--10 km above datum. Martian eruptions are infrequent but enormous in scale. The most recent known activity in the Cerberus Fossae region occurred ~53,000 years ago. Martian volcanoes grew **~1,000x slower** than Earth volcanoes.
- **Venus**: Over 80,000 volcanic features detected by radar. Recent evidence (2020+) suggests Venus is currently volcanically active, though quantitative output rates remain uncertain.
- **Io**: The most volcanically active body in the solar system, with ~400 active volcanoes driven by tidal heating from Jupiter. Io surpasses Earth in volcanic vitality.

[Volcanism on Mars -- Wikipedia](https://en.wikipedia.org/wiki/Volcanism_on_Mars);
[Volcanism on Venus -- Wikipedia](https://en.wikipedia.org/wiki/Volcanism_on_Venus);
[Shaping the Planets: Volcanism (LPI)](https://www.lpi.usra.edu/education/explore/shaping_the_planets/volcanism/)

---

## 3. Erosion

### 3.1 Glacial Erosion

Glacial erosion is the most powerful terrestrial erosion agent. The log-mean glacial erosion rate is **0.51 mm/year**, nearly an order of magnitude greater than fluvial erosion (0.067 mm/year).

| Glacier Type | Erosion Rate (mm/year) |
|---|---|
| **Alpine tidewater glaciers** | 2.2 (log-average) |
| **Alpine glaciers** | 0.58 |
| **Continental glaciers** | 0.26 |
| **Noncontinental high-latitude glaciers** | 0.24 |
| **Temperate glaciers (range)** | 0.1 -- 10+ |
| **Polar glaciers (minimum)** | ~0.01 |

99% of the world's glaciers erode between **0.02 and 2.68 mm/year**. Glaciers modify terrain through plucking, abrasion, and meltwater erosion, creating U-shaped valleys, cirques, aretes, and fjords.

[Around 99% of World's Glaciers Erode Between 0.02 and 2.68 mm/year (Down To Earth)](https://www.downtoearth.org.in/climate-change/around-99-per-cent-of-worlds-glaciers-erode-between-002-and-268-mm-per-year-study);
[Erosion Rates -- AntarcticGlaciers.org](https://www.antarcticglaciers.org/glacial-geology/dating-glacial-sediments-2/cryospheric-geomorphology-dating-glacial-landforms/cosmogenic-nuclide-dating-cryospheric-geomorphology/erosion-rates/);
[Limits to Timescale Dependence in Erosion Rates (PMC)](https://pmc.ncbi.nlm.nih.gov/articles/PMC11661439/) (2024)

### 3.2 Fluvial (Water) Erosion

Fluvial erosion has a characteristic log-mean rate of **0.067 mm/year**, significantly slower than glacial erosion but acting over much larger areas. Rivers erode through hydraulic action, abrasion, attrition, and solution.

In high-relief mountain belts like the Himalayas, erosion rates reach **2--12 mm/year**, among the highest measured anywhere on Earth. This extreme rate approximately balances tectonic uplift.

[Geology of the Himalayas -- Wikipedia](https://en.wikipedia.org/wiki/Geology_of_the_Himalayas);
[Erosion -- Wikipedia](https://en.wikipedia.org/wiki/Erosion)

### 3.3 Wind (Aeolian) Erosion

Wind erosion operates through **deflation** (removal of loose particles) and **abrasion** (sandblasting). Nonfluvial subaerial erosion (including wind) has a characteristic log-mean rate of only **0.00032 mm/year** -- orders of magnitude slower than glacial or fluvial erosion.

Wind erosion is most significant in arid and semi-arid regions where vegetation is sparse. On Mars, wind is the dominant active erosion agent today.

[Erosion -- National Geographic Education](https://education.nationalgeographic.org/resource/erosion/);
[Erosion -- NPS](https://www.nps.gov/subjects/erosion/erosion.htm)

### 3.4 Comparative Rock Outcrop Erosion Rates

Erosion rates vary strongly by rock type and climate:

| Factor | Erosion Rate |
|---|---|
| **Sedimentary outcrops** | 20 +/- 2.0 mm/kyr |
| **Metamorphic outcrops** | 11 +/- 1.4 mm/kyr |
| **Igneous outcrops** | 8.7 +/- 1.0 mm/kyr |
| **Temperate climates** | 25 +/- 2.5 mm/kyr |
| **Polar climates** | 3.9 +/- 0.39 mm/kyr |
| **Carbonate sedimentary rocks** | 5 mm/kyr |
| **Biotite-rich crystalline rocks** | 1 mm/kyr |
| **Homogeneous crystalline rocks** | 0.2 mm/kyr |

[Erosion Rates -- AntarcticGlaciers.org](https://www.antarcticglaciers.org/glacial-geology/dating-glacial-sediments-2/cryospheric-geomorphology-dating-glacial-landforms/cosmogenic-nuclide-dating-cryospheric-geomorphology/erosion-rates/)

---

## 4. Sedimentation and Deposition

### 4.1 Ocean Sedimentation

Marine sedimentation rates span several orders of magnitude depending on setting and sediment type:

| Environment | Sedimentation Rate |
|---|---|
| **Abyssal clay (deep ocean)** | < 5 m/Myr (~0.005 mm/year) |
| **Pelagic biogenic sediments** | up to 200 m/Myr (~0.2 mm/year) |
| **Continental shelf (typical)** | 0.1 -- 1 cm/year (1 -- 10 mm/year) |
| **Near major river outlets** | 3 -- 8 cm/year (30 -- 80 mm/year) |
| **Active delta lobes** | 10 -- 20 mm/year |

The Amazon River alone contributes over **1.2 billion tonnes** of sediment annually. Coarse terrigenous sediment dominates near continental margins, while fine pelagic clays and biogenic ooze dominate the deep ocean floor.

[Deep-Sea Sedimentation (EBSCO)](https://www.ebsco.com/research-starters/oceanography/deep-sea-sedimentation);
[12.6 Sediment Distribution -- Introduction to Oceanography](https://rwu.pressbooks.pub/webboceanography/chapter/12-6-sediment-distribution/);
[Marine Sediment -- Wikipedia](https://en.wikipedia.org/wiki/Marine_sediment)

### 4.2 River Deltas

River deltas form when flow velocity decreases upon entering standing water, reducing the capacity to transport sediment. Coarser sediments (gravel, sand) settle near the delta front as bars and foreset beds, while finer particles (silt, clay) travel farther offshore.

Sediment delivery is strongly seasonal, with the largest loads transported during flood flow. Rivers draining glaciated volcanic headwaters with wide, shallow continental shelves produce the highest sedimentation rates.

[River Delta -- Wikipedia](https://en.wikipedia.org/wiki/River_delta);
[Modeling River Delta Formation (PMC)](https://pmc.ncbi.nlm.nih.gov/articles/PMC2040410/) (2007)

### 4.3 Sand Dunes

Sand dune migration rates vary by dune size and wind regime:

| Dune Type / Location | Migration Rate (m/year) |
|---|---|
| **Kumtag Desert (average)** | 12.86 |
| **Kumtag (small-medium dunes)** | 13.84 |
| **Kumtag (large dunes)** | 11.27 |
| **Bodele Depression (median)** | 15.83 |
| **Namibia Sperrgebiet (average)** | 7 -- 32 |
| **Namibia (small, fast dunes)** | up to 83 |
| **Namibia (large dunes)** | ~9 |

Barchan dunes move proportionally to wind velocity and inversely proportionally to their height. Sediment moves along the stoss (windward) side to the crest; when critical overload is reached, it avalanches down the lee slope.

[Analysis of Sand Dune Migration on Kumtag Desert (MDPI, 2024)](https://www.mdpi.com/2073-445X/14/11/2169);
[Racing Dunes in Namibia (NASA Earth Observatory)](https://earthobservatory.nasa.gov/images/150808/racing-dunes-in-namibia);
[Quantifying Dune Migration in the Central Sahara](https://www.sciencedirect.com/science/article/abs/pii/S0341816223007774) (2023)

### 4.4 Glacial Moraines

Moraines are deposits of sediment carried and deposited by glaciers, found at glacier termini (terminal moraines), alongside glaciers (lateral moraines), between merging glaciers (medial moraines), and beneath glaciers (ground moraines). Glaciers carry an extremely wide range of sediment sizes, from fine clay to house-sized boulders (erratics), deposited unsorted as **till**.

[Fluvioglacial Landform -- Wikipedia](https://en.wikipedia.org/wiki/Fluvioglacial_landform);
[Sedimentary Environments (Columbia University)](https://www.columbia.edu/~vjd1/sed_env.htm)

---

## 5. Tectonic Features

### 5.1 Mountain Ranges (Orogeny)

Mountain building occurs primarily at **convergent plate boundaries** through crustal thickening, thrust faulting, and folding.

**Himalayas** -- the type example of continent-continent collision orogeny:
- India-Eurasia collision began ~**55--65 Ma**
- Total crustal shortening: ~**2,500 km**
- Current convergence rate: ~**17 mm/year** (historically 40--50 mm/year)
- India's pre-collision northward velocity: **18--19.5 cm/year**, dropping to **4.5 cm/year** at collision
- Peak uplift rate: ~**10 mm/year** at Nanga Parbat
- Mount Everest: **8,848 m** -- built in ~50 Myr
- Erosion rates in the Himalayas: **2--12 mm/year**, roughly balancing uplift

[Geology of the Himalayas -- Wikipedia](https://en.wikipedia.org/wiki/Geology_of_the_Himalayas);
[The Himalayas (USGS: This Dynamic Earth)](https://pubs.usgs.gov/gip/dynamic/himalaya.html);
[Continental/Continental: The Himalayas (Geological Society)](https://www.geolsoc.org.uk/Plate-Tectonics/Chap3-Plate-Margins/Convergent/Continental-Collision.html)

### 5.2 Rift Valleys

Rift valleys form at **divergent boundaries** where tectonic plates pull apart, thinning the lithosphere and creating elongated depressions bounded by normal faults.

**East African Rift System** -- the primary active continental rift:
- Overall extension rate: **8--9 mm/year** (Nubian-Somali plate separation)
- Northern sector: **5--16 mm/year**
- South Turkana Basin: **3.5--5.8 mm/year**
- Maximum divergence (including Davie Ridge): ~**7 mm/year**
- The rift was preceded by enormous continental flood basalt eruptions that uplifted the Ethiopian, Somali, and East African plateaus

For comparison, the Afar triple junction where seafloor spreading has begun shows rates of **12--20 mm/year** (average 15.75 mm/year), approaching mid-ocean ridge spreading rates.

[East African Rift -- Wikipedia](https://en.wikipedia.org/wiki/East_African_Rift);
[Rift Valley -- National Geographic Education](https://education.nationalgeographic.org/resource/rift-valley/);
[Accelerated Rifting in Response to Regional Climate Change in the East African Rift System (Nature, 2025)](https://www.nature.com/articles/s41598-025-23264-9)

### 5.3 Mid-Ocean Ridges

The global mid-ocean ridge system is the **longest mountain range on Earth** at ~65,000 km (total oceanic ridge system: ~80,000 km). Ridges rise ~2,000 m above the surrounding ocean basin floor.

**Spreading rates:**

| Ridge | Spreading Rate (full rate) |
|---|---|
| **North Atlantic (slow)** | ~25 mm/year |
| **Pacific (fast)** | 80 -- 145 mm/year |
| **East Pacific Rise (Miocene max)** | >200 mm/year |
| **Ultraslow ridges** | <20 mm/year |
| **Global range** | 10 -- 200 mm/year |

New seafloor forms at ~2.7 km^2/year, producing ~19 km^3/year of new oceanic crust (~7 km thick). Two mechanisms drive spreading: **slab pull** (dominant -- the weight of subducting lithosphere drags the plate) and **ridge push** (gravitational sliding off the elevated ridge).

Slow-spreading ridges feature a central rift valley **10--20 km wide** with relief up to **1,000 m**. Fast-spreading ridges lack a rift valley and instead have a smooth axial high.

[Mid-Ocean Ridge -- Wikipedia](https://en.wikipedia.org/wiki/Mid-ocean_ridge);
[What is a Mid-Ocean Ridge? (NOAA)](https://oceanexplorer.noaa.gov/ocean-fact/mid-ocean-ridge/)

---

## 6. Weathering

### 6.1 Chemical Weathering

Chemical weathering alters the internal mineral structure through hydrolysis, oxidation, carbonation, and dissolution. Key quantitative data:

- **Soil formation** requires **100--1,000 years** for initial development
- **Lichen-covered granite** weathers **3--4x faster** than bare rock
- Plant roots elevate CO2 to **30% of all soil gases**, greatly accelerating carbonation
- **Chemical Index of Alteration (CIA)**: unweathered rock = 47, fully weathered = 100
- Chemical weathering dominates in **warm, humid climates** (tropical regions)
- Basaltic oceanic crust becomes less dense at a rate of ~**15% per 100 Myr**

[Weathering -- Wikipedia](https://en.wikipedia.org/wiki/Weathering);
[Mechanical/Chemical Weathering and Soil Formation (EIU)](https://ux1.eiu.edu/~jpstimac/1300/weathering.html);
[Controls on Weathering Processes and Rates -- LibreTexts](https://geo.libretexts.org/Courses/Chabot_College/Introduction_to_Physical_Geology_(Shulman)/10:_Weathering_Sediment_and_Soil/10.04:_Controls_on_Weathering_Processes_and_Rates)

### 6.2 Physical (Mechanical) Weathering

Physical weathering breaks rock without changing its chemical composition. The primary mechanisms:

**Frost wedging / ice segregation:**
- Water expands **9.2%** upon freezing
- Theoretical maximum pressure: >**200 MPa** (29,000 psi)
- Realistic upper limit: ~**14 MPa** (2,000 psi) -- still far exceeds granite tensile strength of ~**4 MPa** (580 psi)
- **Ice segregation** (growth of ice lenses by migration of unfrozen water) produces pressures **up to 10x greater** than simple frost wedging
- Most effective temperature range: **-4 to -15 C** (25 to 5 F)
- Physical weathering dominates in **cold, dry climates** (polar and alpine regions)

**Pressure release (exfoliation):**
- Differential stress in buttressed rock can reach **35 MPa** (5,100 psi)
- When overlying rock is removed (by erosion), underlying rock expands and fractures in sheets

**Thermal cycling:**
- Repeated heating and cooling causes differential expansion/contraction
- Most effective in deserts with large diurnal temperature swings
- Works synergistically with chemical weathering

[Weathering -- Wikipedia](https://en.wikipedia.org/wiki/Weathering);
[Physical Weathering -- LibreTexts](https://geo.libretexts.org/Courses/Chabot_College/Introduction_to_Physical_Geology_(Shulman)/10:_Weathering_Sediment_and_Soil/10.02:_Physical_Weathering);
[Frost Wedging: Causes and Process (Vaia)](https://www.vaia.com/en-us/explanations/environmental-science/geology/frost-wedging/)

---

## 7. Sources

All sources cited inline above, collected here for reference:

1. [Ivanov et al.: Size-Frequency Distribution (2002)](https://www2.boulder.swri.edu/~bottke/Reprints/Ivanov-etal_2002_AstIII_Craters.pdf)
2. [Slopes of Lunar Crater SFDs at Copernican-Aged Craters (Oetting et al., 2023)](https://agupubs.onlinelibrary.wiley.com/doi/full/10.1029/2023JE007816)
3. [Late Heavy Bombardment -- Wikipedia](https://en.wikipedia.org/wiki/Late_Heavy_Bombardment)
4. [Cataclysm No More: New Views on Timing and Delivery of Lunar Impactors (PMC, 2017)](https://pmc.ncbi.nlm.nih.gov/articles/PMC5602003/)
5. [Ages of Large Lunar Impact Craters (Kirchoff et al., Icarus, 2013)](https://www.sciencedirect.com/science/article/abs/pii/S0019103513001322)
6. [Saturation and Equilibrium Conditions for Impact Cratering (Gault, 1970)](https://agupubs.onlinelibrary.wiley.com/doi/abs/10.1029/RS005i002p00273)
7. [Cratering Saturation and Equilibrium: A New Model (2009)](https://www.sciencedirect.com/science/article/abs/pii/S0019103509003194)
8. [The Equilibrium SFD of Small Craters (2018)](https://www.sciencedirect.com/science/article/pii/S0019103517308370)
9. [Magma Supply Rate -- Wikipedia](https://en.wikipedia.org/wiki/Magma_supply_rate)
10. [Mid-Ocean Ridge -- Wikipedia](https://en.wikipedia.org/wiki/Mid-ocean_ridge)
11. [Rates of Magma Emplacement and Volcanic Output (Crisp, 1984)](https://www.sciencedirect.com/science/article/abs/pii/0377027384900398)
12. [Global Rates of Subaerial Volcanism on Earth (Frontiers, 2022)](https://www.frontiersin.org/journals/earth-science/articles/10.3389/feart.2022.922160/full)
13. [Volcanism on Mars -- Wikipedia](https://en.wikipedia.org/wiki/Volcanism_on_Mars)
14. [Volcanism on Venus -- Wikipedia](https://en.wikipedia.org/wiki/Volcanism_on_Venus)
15. [Erosion Rates -- AntarcticGlaciers.org](https://www.antarcticglaciers.org/glacial-geology/dating-glacial-sediments-2/cryospheric-geomorphology-dating-glacial-landforms/cosmogenic-nuclide-dating-cryospheric-geomorphology/erosion-rates/)
16. [99% of World's Glaciers Erode 0.02--2.68 mm/year (Down To Earth)](https://www.downtoearth.org.in/climate-change/around-99-per-cent-of-worlds-glaciers-erode-between-002-and-268-mm-per-year-study)
17. [Limits to Timescale Dependence in Erosion Rates (PMC, 2024)](https://pmc.ncbi.nlm.nih.gov/articles/PMC11661439/)
18. [Geology of the Himalayas -- Wikipedia](https://en.wikipedia.org/wiki/Geology_of_the_Himalayas)
19. [East African Rift -- Wikipedia](https://en.wikipedia.org/wiki/East_African_Rift)
20. [The Himalayas (USGS: This Dynamic Earth)](https://pubs.usgs.gov/gip/dynamic/himalaya.html)
21. [Deep-Sea Sedimentation (EBSCO)](https://www.ebsco.com/research-starters/oceanography/deep-sea-sedimentation)
22. [Racing Dunes in Namibia (NASA Earth Observatory)](https://earthobservatory.nasa.gov/images/150808/racing-dunes-in-namibia)
23. [Weathering -- Wikipedia](https://en.wikipedia.org/wiki/Weathering)
24. [Accelerated Rifting in the East African Rift System (Nature, 2025)](https://www.nature.com/articles/s41598-025-23264-9)
25. [What is a Mid-Ocean Ridge? (NOAA)](https://oceanexplorer.noaa.gov/ocean-fact/mid-ocean-ridge/)
