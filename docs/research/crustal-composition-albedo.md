# Crustal Composition, Mineralogy, and Planetary Albedo

**Research date:** 2026-03-27

---

## Table of Contents

1. [Igneous Rocks](#1-igneous-rocks)
2. [Metamorphic Rocks](#2-metamorphic-rocks)
3. [Sedimentary Rocks](#3-sedimentary-rocks)
4. [Crustal Composition and Albedo](#4-crustal-composition-and-albedo)
5. [Surface Weathering and Color Changes](#5-surface-weathering-and-color-changes)
6. [Regolith Formation on Airless Bodies](#6-regolith-formation-on-airless-bodies)
7. [Albedo Definitions and Equations](#7-albedo-definitions-and-equations)
8. [Albedo by Surface Type](#8-albedo-by-surface-type)
9. [Generating Albedo Maps from Surface Composition](#9-generating-albedo-maps-from-surface-composition)
10. [Vegetation Distribution and the Holdridge Life Zone System](#10-vegetation-distribution-and-the-holdridge-life-zone-system)
11. [Sources](#11-sources)

---

## 1. Igneous Rocks

Igneous rocks form from the cooling and solidification of magma or lava. They are classified by silica content into four groups: **felsic**, **intermediate**, **mafic**, and **ultramafic** [Classification of Igneous Rocks - Geosciences LibreTexts](https://geo.libretexts.org/Bookshelves/Geology/Book:_An_Introduction_to_Geology_(Johnson_Affolter_Inkenbrandt_and_Mosher)/04:_Igneous_Processes_and_Volcanoes/4.01:_Classification_of_Igneous_Rocks) (2023).

### 1.1 Basalt (Mafic)

- **SiO2 content:** 45-52 wt%
- **Key minerals:** Augite (pyroxene), plagioclase feldspar (Ca-rich), titaniferous magnetite, olivine
- **Composition:** 5-14 wt% FeO, 5-12 wt% MgO, ~10 wt% CaO, >14 wt% Al2O3, 0.5-2.0 wt% TiO2
- **Color:** Dark grey to black (high iron/magnesium content)
- **Where it forms:** Oceanic crust at mid-ocean ridges from upwelling mantle; also at hotspots (e.g., Hawaii, Iceland) and continental flood basalt provinces
- **Significance:** The most common rock on Earth's surface; crustal portions of oceanic tectonic plates are composed predominantly of basalt
- **Albedo:** ~0.06-0.12 (Icelandic basalts measured at ~0.11)

[Basalt - Wikipedia](https://en.wikipedia.org/wiki/Basalt) (2024); [Basalt: Composition, Properties, Types, Uses - Geology In](https://www.geologyin.com/2024/01/basalt-composition-properties-types-uses.html) (2024)

### 1.2 Granite (Felsic)

- **SiO2 content:** 65-75 wt%
- **Key minerals:** Quartz (20-60%), alkali feldspar, plagioclase feldspar (Na-rich), biotite, muscovite, hornblende
- **Color:** Light-colored (white, pink, grey) due to dominance of quartz and feldspar
- **Where it forms:** Continental crust; crystallizes slowly at depth in batholiths and plutons. Associated with continental collision zones and subduction-related magmatism
- **Significance:** Major component of continental crust; granites and granodiorites (collectively "granitoids") are found worldwide
- **Albedo:** ~0.2-0.35 (lighter varieties can approach 0.35; spectral reflectance significantly higher than basalt across visible wavelengths)

[6 Igneous Rocks and Silicate Minerals - Mineralogy (OpenGeology)](https://opengeology.org/Mineralogy/6-igneous-rocks-and-silicate-minerals-v2/) (2023); [Felsic - Wikipedia](https://en.wikipedia.org/wiki/Felsic) (2024)

### 1.3 Andesite (Intermediate)

- **SiO2 content:** 52-63 wt%
- **Key minerals:** Plagioclase feldspar (dominant), biotite, pyroxene, hornblende (amphibole)
- **Color:** Medium grey; intermediate between basalt and granite
- **Where it forms:** Subduction zones and volcanic arcs (e.g., Andes mountains, hence the name); island arcs
- **Significance:** Characteristic volcanic rock of convergent plate boundaries
- **Albedo:** ~0.15-0.25 (intermediate between basalt and granite)

[Classification of Igneous Rocks - Geosciences LibreTexts](https://geo.libretexts.org/Bookshelves/Geology/Book:_An_Introduction_to_Geology_(Johnson_Affolter_Inkenbrandt_and_Mosher)/04:_Igneous_Processes_and_Volcanoes/4.01:_Classification_of_Igneous_Rocks) (2023)

### 1.4 Ultramafic Rocks

- **SiO2 content:** <=40 wt%
- **Key minerals:** Olivine, pyroxene (almost no feldspar or quartz)
- **Color:** Very dark green to black
- **Where it forms:** Earth's upper mantle; rarely exposed at surface (ophiolites, kimberlite pipes)
- **Example:** Peridotite, dunite, komatiite
- **Albedo:** ~0.05-0.10 (very dark)

### Summary Table: Igneous Rock Classification

| Category | SiO2 (wt%) | Key Minerals | Intrusive | Extrusive | Color | Approx. Albedo |
|----------|-------------|--------------|-----------|-----------|-------|----------------|
| Felsic | 65-75 | Quartz, K-feldspar, Na-plagioclase, muscovite | Granite | Rhyolite | Light | 0.20-0.35 |
| Intermediate | 52-63 | Plagioclase, hornblende, biotite, pyroxene | Diorite | Andesite | Medium grey | 0.15-0.25 |
| Mafic | 45-52 | Ca-plagioclase, pyroxene, olivine | Gabbro | Basalt | Dark | 0.06-0.12 |
| Ultramafic | <=40 | Olivine, pyroxene | Peridotite | Komatiite | Very dark | 0.05-0.10 |

---

## 2. Metamorphic Rocks

Metamorphic rocks form when pre-existing rocks are subjected to elevated temperatures and/or pressures that cause mineralogical and textural changes without melting. The concept of **metamorphic facies** groups mineral assemblages that form under similar P-T conditions [Metamorphic facies - Wikipedia](https://en.wikipedia.org/wiki/Metamorphic_facies) (2024).

### 2.1 Pressure-Temperature Regimes by Facies

| Facies | Temperature (C) | Pressure (kbar) | Diagnostic Minerals | Tectonic Setting |
|--------|-----------------|------------------|---------------------|------------------|
| **Zeolite** | ~200-300 | <1.5 | Zeolites (wairakite, laumontite) | Shallow burial, geothermal |
| **Prehnite-Pumpellyite** | ~250-350 | 1-4 | Prehnite, pumpellyite | Slightly deeper burial |
| **Greenschist** | 300-500 | 2-8 | Chlorite, epidote, actinolite, albite | Regional metamorphism |
| **Amphibolite** | 500-750 | 4-10 | Hornblende, plagioclase, garnet | Continental collision |
| **Granulite** | 700-1000 | 6-12+ | Orthopyroxene, plagioclase, garnet | Deep continental crust |
| **Blueschist** | 200-500 | 6-18 | Glaucophane, lawsonite | Subduction zones (cold) |
| **Eclogite** | 500-800+ | >12 (often >27) | Omphacite, garnet | Deep subduction |

[Metamorphic rock - Facies, Pressure, Heat - Britannica](https://www.britannica.com/science/metamorphic-rock/Metamorphic-facies) (2024); [Metamorphic facies - Wikipedia](https://en.wikipedia.org/wiki/Metamorphic_facies) (2024); [Metamorphic facies and pressure-temperature conditions - Fiveable](https://fiveable.me/introduction-geology/unit-7/metamorphic-facies-pressure-temperature-conditions/study-guide/fUncLTmSER32Gl1p) (2024)

### 2.2 Facies Series

- **Barrovian (medium P/T):** Zeolite -> prehnite-pumpellyite -> greenschist -> amphibolite -> granulite. Typical of continental collision orogens (e.g., Himalayas).
- **Buchan (low P/T):** Similar to Barrovian but at lower pressures; associated with high heat flow regions.
- **Subduction (high P/T):** Zeolite -> prehnite-pumpellyite -> blueschist -> eclogite. Cold slabs carried to great depth.

[6 Metamorphic Rocks - An Introduction to Geology (OpenGeology)](https://opengeology.org/textbook/6-metamorphic-rocks/) (2023)

### 2.3 Effect on Surface Appearance

Metamorphic rocks inherit their color largely from protolith composition but alteration can shift it:
- **Marble** (from limestone): white to grey, albedo ~0.4-0.6 (highest reflectance among common rocks; average reflectance in visible 16-86%, highest for marble)
- **Slate** (from shale): dark grey to black, albedo ~0.08-0.15
- **Quartzite** (from sandstone): white to pale, albedo ~0.25-0.40
- **Schist/gneiss**: variable, typically 0.10-0.25 depending on mica and feldspar content

---

## 3. Sedimentary Rocks

Sedimentary rocks form from the accumulation and lithification of sediments at or near Earth's surface. Three main categories exist [5.3: Sedimentary Rocks - Geosciences LibreTexts](https://geo.libretexts.org/Bookshelves/Geology/Book:_An_Introduction_to_Geology_(Johnson_Affolter_Inkenbrandt_and_Mosher)/05:_Weathering_Erosion_and_Sedimentary_Rocks/5.03:_Sedimentary_Rocks) (2023).

### 3.1 Clastic Sedimentary Rocks

Formed from weathered fragments of pre-existing rocks transported as solid clasts.

| Rock | Grain Size | Depositional Environment | Albedo |
|------|-----------|-------------------------|--------|
| **Conglomerate** | >2 mm (gravel) | High-energy rivers, alluvial fans, beaches | 0.10-0.20 |
| **Sandstone** | 0.0625-2 mm | Rivers, beaches, deserts, shallow marine | 0.20-0.40 |
| **Siltstone** | 0.004-0.0625 mm | Floodplains, deltas, shallow marine | 0.15-0.25 |
| **Shale/Mudstone** | <0.004 mm | Deep marine, lakes, floodplains (low energy) | 0.05-0.15 |

### 3.2 Chemical Sedimentary Rocks

Precipitated from dissolved ions in solution.

| Rock | Composition | Depositional Environment | Albedo |
|------|------------|-------------------------|--------|
| **Limestone** | CaCO3 (calcite) | Warm shallow marine, reefs | 0.10-0.35 |
| **Dolomite** | CaMg(CO3)2 | Altered limestone, evaporitic | 0.20-0.35 |
| **Rock salt** | NaCl (halite) | Evaporite basins | 0.40-0.50 |
| **Gypsum** | CaSO4-2H2O | Evaporite basins, sabkha | 0.35-0.55 |
| **Chert** | SiO2 (microcrystalline) | Deep marine, nodules in limestone | 0.10-0.25 |

### 3.3 Biogenic (Biochemical) Sedimentary Rocks

Formed from shells, skeletons, and organic material of organisms.

| Rock | Origin | Depositional Environment | Albedo |
|------|--------|-------------------------|--------|
| **Chalk** | Coccolithophore shells | Deep marine pelagic | 0.40-0.55 |
| **Coal** | Plant material | Swamps, peat bogs | 0.03-0.05 |
| **Diatomite** | Diatom frustules | Lacustrine, marine | 0.30-0.45 |
| **Reef limestone** | Coral, shell fragments | Tropical shallow marine | 0.15-0.35 |

[Sedimentary Rocks - Tulane University](https://www2.tulane.edu/~sanelson/eens1110/sedrx.htm) (2023); [Sedimentary rock - Wikipedia](https://en.wikipedia.org/wiki/Sedimentary_rock) (2024)

---

## 4. Crustal Composition and Albedo

The albedo of bare rock surfaces is controlled primarily by **mineral composition**, specifically the ratio of dark (Fe/Mg-rich) to light (Si/Al-rich) minerals.

### 4.1 Key Relationships

**Iron and magnesium content = darkness.** Mafic minerals (pyroxene, olivine, amphibole) contain Fe2+ and Mg2+ and strongly absorb visible light, especially at shorter wavelengths. This gives mafic rocks their characteristically low albedo. Basalt absorbs roughly evenly across the visible spectrum, producing its near-black color [Reflectance Spectroscopy Tutorial](https://ser.im-ldi.com/SPECTRA/intro.html) (2023).

**Silica and aluminum content = lightness.** Felsic minerals (quartz, feldspar, muscovite) are translucent to white. Quartz (SiO2) has high visible-light transmittance, and feldspar is typically white to pink. These minerals dominate granite and rhyolite, giving them higher albedo [6 Igneous Rocks and Silicate Minerals - Mineralogy](https://opengeology.org/Mineralogy/6-igneous-rocks-and-silicate-minerals-v2/) (2023).

### 4.2 Albedo Reference Values by Rock Type

| Surface Material | Broadband Albedo | Primary Control |
|-----------------|------------------|-----------------|
| Fresh basalt | 0.06-0.12 | High Fe, Mg; pyroxene + olivine |
| Weathered basalt | 0.08-0.15 | Iron oxide coatings increase slightly |
| Andesite | 0.15-0.25 | Intermediate Fe/Mg content |
| Granite (fresh) | 0.20-0.35 | High quartz + feldspar content |
| Sandstone | 0.20-0.40 | Quartz-dominated; Fe staining lowers |
| Limestone | 0.10-0.35 | Calcite; variable with impurities |
| Desert sand | 0.30-0.45 | Quartz grains; iron-oxide coating |
| White sand (gypsum/quartz) | 0.45-0.60 | Pure mineral, minimal absorbers |
| Ice/snow | 0.60-0.90 | Crystalline H2O, minimal absorption in VIS |
| Coal | 0.03-0.05 | Carbon, maximum absorption |

[Rock albedo and monitoring of thermal conditions - ResearchGate](https://www.researchgate.net/publication/227663439_Rock_albedo_and_monitoring_of_thermal_conditions_in_respect_of_weathering_Some_expected_and_some_unexpected_results) (2006); [Spectral reflectance and photometric properties of selected rocks - USGS](https://pubs.usgs.gov/publication/70010345) (1967)

### 4.3 Grain Size Effect

Crushed or powdered rock is generally lighter than intact surfaces because scattering increases with the number of grain boundaries. This is relevant for regolith: a planet covered in fine-grained regolith of a given composition will have a somewhat higher albedo than one with polished bedrock of the same composition.

---

## 5. Surface Weathering and Color Changes

### 5.1 Iron Oxidation and Reddening

The most prominent weathering-driven color change is **iron oxidation**. When Fe2+-bearing minerals (olivine, pyroxene, magnetite) are exposed to oxygen and/or water, iron oxidizes to Fe3+, forming iron oxide/hydroxide minerals:

- **Hematite** (Fe2O3): Deep red; dominant pigment in red soils and red sandstones
- **Goethite** (FeOOH): Yellow-brown; common in temperate weathering
- **Ferrihydrite** (Fe5O8H-nH2O): Reddish-brown, poorly crystalline; recently identified as the dominant iron oxide phase in Martian dust

Mars's red color was long attributed to hematite, but 2025 research using orbital and laboratory spectra showed that **ferrihydrite** is the dominant iron-bearing phase in Martian dust. Ferrihydrite forms in the presence of cool water, indicating Mars rusted while liquid water was still present on the surface [Detection of ferrihydrite in Martian red dust - Nature Communications](https://www.nature.com/articles/s41467-025-56970-z) (2025); [Is Mars red because of iron corrosion? - Astronomy.com](https://www.astronomy.com/science/is-mars-red-because-of-iron-corrosion-if-so-what-process-caused-it-to-occur/) (2025).

### 5.2 Effect on Albedo

Iron oxidation **modestly increases** albedo at longer wavelengths (red/NIR) while **decreasing** it at shorter wavelengths (blue/UV), producing the characteristic reddening:

| Surface State | Albedo (broadband) | Color Shift |
|---------------|-------------------|-------------|
| Fresh basalt | ~0.06-0.10 | Dark grey-black |
| Weathered basalt (Fe oxides) | ~0.10-0.18 | Red-brown |
| Heavily oxidized (Mars-like) | ~0.15-0.25 | Rust-red |
| Fresh granite | ~0.25-0.35 | Light grey-pink |
| Desert varnish on granite | ~0.15-0.25 | Dark brown-black |

### 5.3 Desert Varnish

An important counter-example: in arid environments, **desert varnish** -- a thin coating of manganese and iron oxides deposited by bacteria -- actually **darkens** light-colored rocks, reducing albedo by 0.1-0.2 [Rock albedo and monitoring of thermal conditions - ResearchGate](https://www.researchgate.net/publication/227663439_Rock_albedo_and_monitoring_of_thermal_conditions_in_respect_of_weathering_Some_expected_and_some_unexpected_results) (2006).

### 5.4 Other Weathering Processes

- **Chemical decomposition of feldspar** -> clay minerals (kaolinite, montmorillonite): generally lightens surface slightly (clays are pale)
- **Biological crusts** (cyanobacteria, lichens): darken desert surfaces by 0.02-0.10
- **Sulfate/carbonate crusts**: Can lighten surfaces in evaporite environments

---

## 6. Regolith Formation on Airless Bodies

On bodies without atmospheres (Moon, Mercury, asteroids), surface modification occurs through **space weathering** rather than chemical or biological weathering [Space Weathering on Airless Bodies - PMC](https://pmc.ncbi.nlm.nih.gov/articles/PMC5975224/) (2016).

### 6.1 Processes

1. **Micrometeorite bombardment:** Impacts shatter, melt, and vaporize surface rock. Over billions of years, this comminution creates a fine-grained regolith blanket (lunar regolith averages 4-5 m thick on maria, 10-15 m on highlands).
2. **Solar wind irradiation:** Ions (primarily H+ and He2+) sputter atoms from grain surfaces, implant into crystal lattices, and amorphize surface layers.
3. **Cosmic ray bombardment:** Higher-energy particles cause lattice damage and nuclear reactions at greater depth.
4. **Thermal cycling:** Extreme day/night temperature swings cause thermal fatigue and microcracking (e.g., Moon: -173C to +127C).

### 6.2 Optical Effects

Space weathering produces two populations of opaque particles:

- **Nanophase metallic iron (npFe0):** 1-15 nm diameter particles that accumulate on/in grain rims. These cause **spectral reddening** (red-sloped continuum in visible/NIR) and **weakening of mineral absorption bands**. Formed primarily through impact processes.
- **Britt-Pieters (B-P) particles:** Larger opaque particles (40 nm to ~2 um) dispersed throughout the soil matrix. These cause **overall darkening** without significant reddening.

### 6.3 Timescales

- Mature lunar soils accumulate approximately 10^6 to 10^7 years of integrated surface exposure
- Smaller bodies with lower gravity retain and recycle less regolith, accumulating fewer space weathering effects
- Fresh crater rays (high albedo) fade and disappear as space weathering darkens the exposed material over ~1 Gyr

### 6.4 Net Effect on Albedo

| Body | Fresh Surface Albedo | Mature Regolith Albedo | Change |
|------|---------------------|----------------------|--------|
| Moon (highlands) | ~0.20-0.25 | ~0.10-0.15 | Darkening |
| Moon (maria/basalt) | ~0.10-0.15 | ~0.06-0.08 | Darkening |
| S-type asteroids | ~0.20-0.30 | ~0.10-0.20 | Darkening + reddening |
| Mercury | -- | ~0.07-0.10 | Heavily weathered |

The Moon's overall geometric albedo is ~0.12, and Mercury's is ~0.14 [Albedo - Wikipedia](https://en.wikipedia.org/wiki/Albedo) (2024).

[Space weathering on airless bodies - Pieters 2016 - JGR Planets](https://agupubs.onlinelibrary.wiley.com/doi/10.1002/2016JE005128) (2016); [Space weathering on airless planetary bodies: Clues from hapkeite - PNAS](https://www.pnas.org/doi/10.1073/pnas.0401565101) (2004)

---

## 7. Albedo Definitions and Equations

### 7.1 Bond Albedo (Spherical Albedo), A

The fraction of total incident electromagnetic power scattered back into space by an astronomical body, integrated over all wavelengths and all phase angles.

**Equation:**

```
A = p * q
```

Where:
- **p** = geometric albedo
- **q** = phase integral

**Phase integral:**

```
q = 2 * integral from 0 to pi of [ Phi(alpha) * sin(alpha) d(alpha) ]
```

where Phi(alpha) is the normalized phase function (brightness at phase angle alpha relative to alpha=0).

**Range:** Strictly 0 to 1.

**Physical significance:** Determines equilibrium temperature of a planet:

```
T_eq = T_sun * sqrt(R_sun / (2*d)) * (1 - A)^(1/4)
```

[Bond albedo - Wikipedia](https://en.wikipedia.org/wiki/Bond_albedo) (2024); [Spheres - Bond Albedo, Phase Integral and Geometrical Albedo - Physics LibreTexts](https://phys.libretexts.org/Bookshelves/Astronomy__Cosmology/Planetary_Photometry_(Tatum_and_Fairbairn)/02:_Albedo/2.09:_Spheres_-_Bond_Albedo,_Phase_Integral_and_Geometrical_Albedo) (2024)

### 7.2 Geometric Albedo, p

The ratio of a body's brightness at zero phase angle (full illumination, observer at light source) to that of a perfectly diffusing (Lambertian) disk of the same cross-section.

**Equation:**

```
p = I(0) / (a^2 * F)
```

Where:
- I(0) = intensity at zero phase angle
- a = radius of sphere
- F = incident solar flux

**Range:** 0 to infinity (theoretically >1 possible for specularly reflecting or backscattering surfaces; e.g., Enceladus has geometric albedo ~1.4 at some wavelengths).

**For analytical surfaces:**

| Scattering Law | Geometric Albedo (p) | Phase Integral (q) | Bond Albedo (A) |
|----------------|---------------------|--------------------|--------------------|
| Lambertian | 2*omega_0 / 3 | 3/2 | omega_0 |
| Lommel-Seeliger | omega_0 / 8 | (16/3)(1 - ln2) | (3/2)*omega_0*(1 - ln2) |

where omega_0 is the single-scattering albedo.

### 7.3 Single-Scattering Albedo, omega_0

The ratio of scattering coefficient to total extinction coefficient for a single interaction event.

**Equation:**

```
omega_0 = beta_s / beta_e = beta_s / (beta_s + beta_a)
```

Where:
- beta_s = scattering coefficient
- beta_a = absorption coefficient
- beta_e = extinction coefficient

**Range:** 0 (pure absorption) to 1 (pure scattering, no absorption).

**Physical significance:** Fundamental material property that determines how much light is scattered vs absorbed at the particle/grain level. It is the building block from which geometric and Bond albedo are computed via radiative transfer.

[Single-scattering albedo - Wikipedia](https://en.wikipedia.org/wiki/Single-scattering_albedo) (2024); [Single-scattering Albedo - Physics LibreTexts](https://phys.libretexts.org/Bookshelves/Astronomy__Cosmology/Planetary_Photometry_(Tatum_and_Fairbairn)/02:_Albedo/2.04:_Surfaces_-_Single-scattering_Albedo) (2024)

### 7.4 Planetary Albedo Values

| Body | Geometric Albedo | Bond Albedo |
|------|-----------------|-------------|
| Mercury | 0.142 | 0.088 |
| Venus | 0.689 | 0.76 |
| Earth | 0.434 | 0.306 |
| Moon | 0.12 | 0.067 |
| Mars | 0.170 | 0.25 |
| Jupiter | 0.538 | 0.503 |
| Saturn | 0.499 | 0.342 |
| Uranus | 0.488 | 0.300 |
| Neptune | 0.442 | 0.290 |
| Enceladus | ~1.4 (VIS) | 0.81 |
| Eris | 0.96 | 0.99 |

[Albedo - Wikipedia](https://en.wikipedia.org/wiki/Albedo) (2024)

---

## 8. Albedo by Surface Type

### 8.1 Comprehensive Reference Table

| Surface Type | Albedo | Notes |
|-------------|--------|-------|
| **Water/Ocean** | | |
| Open ocean (low sun) | 0.06 | Near-total absorption |
| Open ocean (high sun) | 0.03-0.10 | Angle-dependent (Fresnel) |
| **Vegetation** | | |
| Tropical rainforest | 0.07-0.13 | Dark green canopy |
| Conifer forest (summer) | 0.08-0.15 | Needleleaf, dark |
| Deciduous forest | 0.15-0.18 | Broadleaf, lighter |
| Grassland | 0.20-0.25 | |
| Green grass | 0.25 | |
| Cropland | 0.15-0.25 | Varies by crop type/stage |
| Tundra (summer) | 0.15-0.20 | Low vegetation |
| **Bare Surfaces** | | |
| Bare soil (average) | 0.17 | |
| Black organic soil | <0.10 | High carbon content |
| Dark volcanic soil | 0.05-0.10 | Basaltic, high Fe |
| Desert sand | 0.30-0.45 | Quartz + iron oxide coating |
| White sand (gypsum) | 0.50-0.60 | Pure light minerals |
| **Ice and Snow** | | |
| Fresh snow | 0.80-0.90 | Can reach 0.95 |
| Old/melting snow | 0.40-0.60 | Grain coarsening, impurities |
| Dirty snow | 0.20-0.40 | Soot, dust contamination |
| Sea ice | 0.50-0.70 | |
| Glacier ice | 0.30-0.40 | Compressed, fewer boundaries |
| Antarctic snow (average) | >0.80 | |
| **Built Surfaces** | | |
| Fresh asphalt | 0.04 | |
| Worn asphalt | 0.12 | |
| New concrete | 0.55 | |
| **Clouds** | | |
| Thin cirrus | 0.20-0.30 | |
| Stratocumulus | 0.60-0.70 | |
| Thick cumulonimbus | 0.70-0.90 | |

[Albedo - Wikipedia](https://en.wikipedia.org/wiki/Albedo) (2024); [Albedo and Climate - UCAR](https://scied.ucar.edu/learning-zone/how-climate-works/albedo-and-climate) (2023)

### 8.2 What Controls Planetary Albedo

A planet's total (Bond) albedo is determined by five factors, in rough order of impact:

1. **Cloud cover** (dominant for Venus: A=0.76; Earth clouds contribute ~0.15 of Earth's 0.30)
2. **Ice and snow extent** (polar caps, glaciation; albedo 0.6-0.9)
3. **Surface composition** (rock type, soil, ocean coverage; albedo 0.03-0.45)
4. **Vegetation cover** (forests darken surface relative to bare ground or snow; albedo 0.07-0.25)
5. **Atmospheric Rayleigh scattering** (backscatters ~0.06 of incident light for Earth-like atmosphere)

For an airless rocky body, only #3 matters. For a body with atmosphere but no ocean/life, #1, #3, and #5 dominate.

[ESA - Reflecting on Earth's albedo](https://www.esa.int/Applications/Observing_the_Earth/Reflecting_on_Earth_s_albedo) (2023); [Measuring Earth's Albedo - NASA Earth Observatory](https://earthobservatory.nasa.gov/images/84499/measuring-earths-albedo) (2014)

---

## 9. Generating Albedo Maps from Surface Composition

### 9.1 From Real Remote Sensing Data

Real-world albedo maps are derived from satellite observations using the **Bidirectional Reflectance Distribution Function (BRDF)** approach:

1. **Instrument:** MODIS (on Terra/Aqua) or VIIRS captures reflectance at multiple view and solar angles
2. **BRDF model:** A mathematical model (typically the Ross-Li kernel model) fits directional reflectance samples to estimate how the surface reflects in all directions
3. **Integration:** The BRDF is integrated over the hemisphere to produce:
   - **Directional-hemispherical reflectance** (black-sky albedo: direct illumination only)
   - **Bi-hemispherical reflectance** (white-sky albedo: fully diffuse illumination)
4. **Blue-sky albedo** = weighted combination based on actual diffuse/direct ratio

[Mapping Surface Broadband Albedo from Satellite Observations - MDPI Remote Sensing](https://www.mdpi.com/2072-4292/7/1/990) (2015); [HAMSTER: Hyperspectral Albedo Maps - AMT Copernicus](https://amt.copernicus.org/articles/17/6025/2024/) (2024)

### 9.2 For Procedural/Synthetic Planet Generation

To generate an albedo map from a known surface composition map:

**Step 1: Assign base albedo by material type**
```
albedo_base(x,y) = lookup_table[material(x,y)]
```
Using the reference values from Section 4.2 and Section 8.1.

**Step 2: Apply weathering modifier**
```
albedo_weathered = albedo_base * (1 + weathering_factor * delta_albedo)
```
Where weathering_factor (0-1) encodes surface age and exposure, and delta_albedo is the weathering direction (+/- depending on process: iron oxidation, desert varnish, etc.).

**Step 3: Add vegetation overlay**
```
albedo_veg = lerp(albedo_weathered, albedo_vegetation_type, vegetation_coverage)
```
Where vegetation_coverage (0-1) comes from the biome/climate model (Section 10).

**Step 4: Add ice/snow**
```
albedo_final = lerp(albedo_veg, albedo_snow, snow_coverage)
```

**Step 5: Apply spectral variation (optional)**
Different materials reflect differently across wavelengths. For RGB rendering, use three albedo channels:
- Basalt: R=0.10, G=0.09, B=0.08 (slight red excess from Fe)
- Granite: R=0.30, G=0.28, B=0.25 (warm grey)
- Vegetation: R=0.10, G=0.20, B=0.05 (green peak, red-edge effect)
- Desert sand: R=0.45, G=0.35, B=0.25 (red-yellow from iron oxides)
- Snow: R=0.90, G=0.90, B=0.90 (near-flat high reflectance)

---

## 10. Vegetation Distribution and the Holdridge Life Zone System

### 10.1 Overview

The **Holdridge life zone system** classifies terrestrial ecosystems based on three bioclimatic variables, using logarithmic scales [Holdridge life zones - Wikipedia](https://en.wikipedia.org/wiki/Holdridge_life_zones) (2024):

1. **Mean annual biotemperature** -- mean of all temperatures with values <0C and >30C set to 0 (since plants are dormant outside this range)
2. **Mean annual precipitation** (mm)
3. **Potential evapotranspiration (PET) ratio** = PET / precipitation

### 10.2 Latitudinal Regions and Altitudinal Belts

The system defines five latitudinal regions that correspond to altitudinal belts:

| Latitudinal Region | Approx. Latitude | Altitudinal Belt | Biotemperature (C) |
|--------------------|-----------------|-----------------|--------------------|
| Polar | >66.5 N/S | Nival | <1.5 |
| Subpolar | 58-66.5 | Alpine | 1.5-3 |
| Boreal | 46-58 | Subalpine | 3-6 |
| Cool Temperate | 34-46 | Montane | 6-12 |
| Warm Temperate | 23-34 | Lower Montane | 12-18 |
| Subtropical | 12-23 | Premontane | 18-24 |
| Tropical | 0-12 | Basal | >24 |

### 10.3 Selected Life Zones and Their Albedo Characteristics

The system defines **38 life zones**. Key ones for albedo mapping:

| Life Zone | Biotemperature (C) | Precipitation (mm/yr) | PET Ratio | Typical Albedo |
|-----------|--------------------|-----------------------|-----------|----------------|
| Polar desert | <1.5 | <125 | -- | 0.60-0.85 (ice/snow) |
| Tundra | 1.5-3 | 125-500 | <1 | 0.15-0.25 (summer) |
| Boreal wet forest | 3-6 | 1000-2000 | 0.25-0.5 | 0.08-0.15 |
| Boreal moist forest | 3-6 | 500-1000 | 0.5-1 | 0.09-0.15 |
| Cool temperate steppe | 6-12 | 250-500 | 2-4 | 0.20-0.30 |
| Cool temperate moist forest | 6-12 | 500-1000 | 0.5-1 | 0.12-0.18 |
| Warm temperate desert | 12-18 | <250 | >8 | 0.30-0.45 |
| Warm temperate moist forest | 12-18 | 500-1000 | 0.5-1 | 0.12-0.18 |
| Subtropical desert | 18-24 | <125 | >16 | 0.35-0.50 |
| Subtropical rain forest | 18-24 | >4000 | <0.125 | 0.08-0.13 |
| Tropical desert | >24 | <125 | >32 | 0.35-0.50 |
| Tropical dry forest | >24 | 500-1000 | 2-4 | 0.12-0.18 |
| Tropical moist forest | >24 | 1000-2000 | 0.5-1 | 0.10-0.15 |
| Tropical wet/rain forest | >24 | >2000 | <0.5 | 0.07-0.13 |

[Holdridge Life Zones: climate and vegetation types - Columbia IRI](https://iridl.ldeo.columbia.edu/SOURCES/.ECOSYSTEMS/.Holdridge/.dataset_documentation.html) (2023); [The Holdridge life zones of the conterminous United States - USFS](https://research.fs.usda.gov/treesearch/30306) (2008)

### 10.4 Altitude-Vegetation-Albedo Gradient

For any given latitude, increasing altitude mirrors increasing latitude:

```
Tropical basal (0-1000m)  -> Premontane (1000-2000m) -> Lower Montane (2000-3000m)
  -> Montane (3000-4000m) -> Subalpine (4000-4500m) -> Alpine (4500-5000m) -> Nival (>5000m)
```

The albedo progression going uphill:
- **0-1000m:** Dense tropical forest, albedo 0.08-0.13
- **1000-2000m:** Cloud forest, albedo 0.10-0.15
- **2000-3500m:** Montane grassland/forest, albedo 0.15-0.20
- **3500-4500m:** Paramo/puna grassland, albedo 0.18-0.25
- **4500-5000m:** Sparse vegetation / bare rock, albedo 0.15-0.30
- **>5000m:** Snow/ice, albedo 0.60-0.90

---

## 11. Sources

1. [Basalt - Wikipedia](https://en.wikipedia.org/wiki/Basalt) (2024)
2. [Classification of Igneous Rocks - Geosciences LibreTexts](https://geo.libretexts.org/Bookshelves/Geology/Book:_An_Introduction_to_Geology_(Johnson_Affolter_Inkenbrandt_and_Mosher)/04:_Igneous_Processes_and_Volcanoes/4.01:_Classification_of_Igneous_Rocks) (2023)
3. [6 Igneous Rocks and Silicate Minerals - Mineralogy (OpenGeology)](https://opengeology.org/Mineralogy/6-igneous-rocks-and-silicate-minerals-v2/) (2023)
4. [Metamorphic facies - Wikipedia](https://en.wikipedia.org/wiki/Metamorphic_facies) (2024)
5. [Metamorphic rock - Facies, Pressure, Heat - Britannica](https://www.britannica.com/science/metamorphic-rock/Metamorphic-facies) (2024)
6. [5.3: Sedimentary Rocks - Geosciences LibreTexts](https://geo.libretexts.org/Bookshelves/Geology/Book:_An_Introduction_to_Geology_(Johnson_Affolter_Inkenbrandt_and_Mosher)/05:_Weathering_Erosion_and_Sedimentary_Rocks/5.03:_Sedimentary_Rocks) (2023)
7. [Albedo - Wikipedia](https://en.wikipedia.org/wiki/Albedo) (2024)
8. [Bond albedo - Wikipedia](https://en.wikipedia.org/wiki/Bond_albedo) (2024)
9. [Geometric albedo - Wikipedia](https://en.wikipedia.org/wiki/Geometric_albedo) (2024)
10. [Single-scattering albedo - Wikipedia](https://en.wikipedia.org/wiki/Single-scattering_albedo) (2024)
11. [Spheres - Bond Albedo, Phase Integral and Geometrical Albedo - Physics LibreTexts](https://phys.libretexts.org/Bookshelves/Astronomy__Cosmology/Planetary_Photometry_(Tatum_and_Fairbairn)/02:_Albedo/2.09:_Spheres_-_Bond_Albedo,_Phase_Integral_and_Geometrical_Albedo) (2024)
12. [Space Weathering on Airless Bodies - PMC](https://pmc.ncbi.nlm.nih.gov/articles/PMC5975224/) (2016)
13. [Space weathering on airless bodies - Pieters 2016 - JGR Planets](https://agupubs.onlinelibrary.wiley.com/doi/10.1002/2016JE005128) (2016)
14. [Detection of ferrihydrite in Martian red dust - Nature Communications](https://www.nature.com/articles/s41467-025-56970-z) (2025)
15. [Rock albedo and monitoring of thermal conditions - ResearchGate](https://www.researchgate.net/publication/227663439_Rock_albedo_and_monitoring_of_thermal_conditions_in_respect_of_weathering_Some_expected_and_some_unexpected_results) (2006)
16. [Spectral reflectance and photometric properties of selected rocks - USGS](https://pubs.usgs.gov/publication/70010345) (1967)
17. [Holdridge life zones - Wikipedia](https://en.wikipedia.org/wiki/Holdridge_life_zones) (2024)
18. [Mapping Surface Broadband Albedo from Satellite Observations - MDPI Remote Sensing](https://www.mdpi.com/2072-4292/7/1/990) (2015)
19. [HAMSTER: Hyperspectral Albedo Maps - AMT Copernicus](https://amt.copernicus.org/articles/17/6025/2024/) (2024)
20. [ESA - Reflecting on Earth's albedo](https://www.esa.int/Applications/Observing_the_Earth/Reflecting_on_Earth_s_albedo) (2023)
21. [Measuring Earth's Albedo - NASA Earth Observatory](https://earthobservatory.nasa.gov/images/84499/measuring-earths-albedo) (2014)
22. [Albedo and Climate - UCAR](https://scied.ucar.edu/learning-zone/how-climate-works/albedo-and-climate) (2023)
23. [Reflectance Spectroscopy Tutorial](https://ser.im-ldi.com/SPECTRA/intro.html) (2023)
24. [Felsic - Wikipedia](https://en.wikipedia.org/wiki/Felsic) (2024)
25. [Holdridge Life Zones: climate and vegetation types - Columbia IRI](https://iridl.ldeo.columbia.edu/SOURCES/.ECOSYSTEMS/.Holdridge/.dataset_documentation.html) (2023)
26. [The Holdridge life zones of the conterminous United States - USFS](https://research.fs.usda.gov/treesearch/30306) (2008)
27. [Space weathering on airless planetary bodies: Clues from hapkeite - PNAS](https://www.pnas.org/doi/10.1073/pnas.0401565101) (2004)
28. [Basalt: Composition, Properties, Types, Uses - Geology In](https://www.geologyin.com/2024/01/basalt-composition-properties-types-uses.html) (2024)
