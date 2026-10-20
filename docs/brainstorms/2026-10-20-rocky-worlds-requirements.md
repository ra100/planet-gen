---
date: 2026-10-20
topic: rocky-worlds
origin: R in the PlanetGen project thread (2026-10-20)
---

# Rocky worlds: Moon-like and Mars-like planets

## Problem

Earth-like planets look good, but the range beyond them does not. The MOON,
MERCURY and MARS presets rendered as the same Earth desert: plate-tectonic
continents with camouflage blotches, blue lakes, a blue Rayleigh limb, no
craters, and colours far from the real bodies (Moon disk albedo 0.28 instead
of ~0.12; Mars barely redder than gray, red/blue 1.3 instead of ~3.5). The
physics model also handed small bodies atmospheres they cannot keep (Mars
0.16 bar, Moon 0.02 bar, Mercury 0.05 bar).

R asked to focus on rocky worlds with thin or no atmospheres and with past
or current tectonic activity, approach them the way the field's experts do,
check against references, add controls where needed, and keep Earth from
deteriorating.

## How the field approaches it

- **Planetary geologists** read a surface as a stratigraphic history: crust,
  basin-forming impacts, volcanic resurfacing (maria, plains, shields),
  continued cratering that dates each unit, and degradation. Crater
  densities date surfaces through a chronology (Neukum et al. 2001);
  saturated highlands stop accumulating craters (Gault 1970).
- **Crater morphometry** (Pike 1977, 1980; Melosh 1989): simple bowls
  d ≈ 0.2 D with rims 0.036 D; above the simple-to-complex transition
  (~15 km on the Moon, ~7 km on Mars, scaling with 1/g) floors flatten,
  central peaks rise and depth grows as D^0.3; peak rings and multi-ring
  basins follow. Ejecta thins as r^-3 (McGetchin et al. 1973).
- **Atmospheric escape**: the cosmic shoreline (Zahnle & Catling 2017)
  separates bodies with and without air by insolation against escape
  velocity^4; Mars sits on it, the Moon and Mercury far beyond.
  Liquid water needs pressure above its triple point (6.1 mbar).
- **Photometrists** model airless regolith with lunar-Lambert (McEwen 1991,
  1996) and the opposition surge (Hapke 1986); albedo follows maturity
  (space weathering darkens and reddens; fresh ejecta and rays are bright).
- **Mars** colour is bright oxidized dust over darker basalt and sand (TES
  dust cover index), with dust aerosol τ ~0.3-0.5 scattering red and
  absorbing blue (Wolff et al. 2009). MOLA shows the bimodal hypsometry of
  the crustal dichotomy, the Tharsis rise and Valles Marineris (Smith et al.
  1999).

## Requirements

- R1. Atmosphere from escape physics: pressure falls off past the cosmic
  shoreline; Earth's derived values stay exact.
- R2. No liquid water below the triple point: water goes to polar ice; dry
  worlds have no oceans or lakes.
- R3. Stagnant-lid crust: craters and basins with realistic morphometry and
  chronology-driven density, maria, volcanic provinces and shields, an
  optional hemispheric dichotomy. Plate relief fades with the lid.
- R4. Regolith look: lunar palette with maria, fresh craters and rays;
  Mars dust over basalt; polar frost; lunar-Lambert photometry and black
  shadows on airless worlds; Mars dust haze instead of a blue limb.
- R5. Controls for what the model derives: pressure, plate activity,
  crater age, maria, volcanoes, dichotomy, dust (each AUTO by default).
- R6. Export parity: albedo and roughness maps show the same surface.
- R7. Earth renders and terrain stay bit-identical.
- R8. Validation against reference values with a repeatable capture tool.
