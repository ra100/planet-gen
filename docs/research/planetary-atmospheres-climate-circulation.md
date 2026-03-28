# Planetary Atmospheres, Climate, and Circulation

_Quantitative reference for procedural planet generation_
_Research date: 2026-03-28_

---

## 1. Atmospheric Composition by Planet Type

### 1.1 Terrestrial (Earth-like)

| Species | Volume mixing ratio |
|---------|-------------------|
| N2 | 78.084% |
| O2 | 20.946% |
| Ar | 0.934% |
| CO2 | ~421 ppm (0.0421%) |
| Ne | 18.18 ppm |
| He | 5.24 ppm |
| CH4 | ~1.9 ppm |
| H2O | 0-4% (variable) |

- Surface pressure: 101,325 Pa (1 atm)
- Surface temperature: 288 K (15 C)
- Mean molecular mass: 28.97 g/mol

Source: [Planetary Atmospheres, Britannica](https://www.britannica.com/science/atmosphere/The-atmospheres-of-other-planets)

### 1.2 Venus-like (Dense CO2)

| Species | Volume mixing ratio |
|---------|-------------------|
| CO2 | 96.5% |
| N2 | 3.5% |
| SO2 | 150 ppm |
| Ar | 70 ppm |
| H2O | 20 ppm |
| CO | 17 ppm |
| He | 12 ppm |
| HCl | 0.1-0.6 ppm |
| HF | 1-5 ppb |

- Surface pressure: 92 bar (9.2 MPa)
- Surface temperature: 740 K (467 C)
- Mean molecular mass: 43.45 g/mol
- Cloud layers: sulfuric acid droplets (75-96% H2SO4), altitude 48-70 km
- Cloud-top wind: 100 +/- 10 m/s
- Surface wind: < 2 m/s
- Albedo: ~0.75 (Bond)

Sources: [Atmosphere of Venus, Wikipedia](https://en.wikipedia.org/wiki/Atmosphere_of_Venus)

### 1.3 Mars-like (Thin CO2)

| Species | Volume mixing ratio |
|---------|-------------------|
| CO2 | 95.32% |
| N2 | 2.7% |
| Ar | 1.6% |
| O2 | 0.13% |
| CO | 0.08% |
| H2O | 0.03% (variable) |
| Ne | 2.5 ppm |
| Kr | 0.3 ppm |
| H2 | ~15 ppm |
| Xe | 0.08 ppm |

- Surface pressure: 610 Pa (0.006 atm), ~25% seasonal variation from CO2 condensation
- Mean surface temperature: 210 K (-63 C), range -75 C to ~0 C
- Mean molecular mass: 43.34 g/mol
- Total atmospheric mass: 2.5 x 10^16 kg
- Dust background optical depth: 0.15, up to >4.0 during global storms
- Dust particle effective radius: 0.6-2 um

Source: [Atmosphere of Mars, Wikipedia](https://en.wikipedia.org/wiki/Atmosphere_of_Mars)

### 1.4 Titan-like (Dense N2 + CH4)

| Species | Volume mixing ratio |
|---------|-------------------|
| N2 | 94.2% (lower atmosphere); 98.4% (stratosphere) |
| CH4 | 5.65% (surface); 4.9% (below 8 km); 1.4% (stratosphere) |
| H2 | 0.099% (0.1-0.2% stratosphere) |
| C2H6 | trace |
| C2H2 | trace |
| HCN | trace |

- Surface pressure: 1.5 bar (146.7 kPa)
- Surface temperature: 94 K (-179 C)
- Main tholin haze layer: 100-210 km altitude
- Detached haze layer: 450-500 km altitude

Source: [Atmosphere of Titan, Wikipedia](https://en.wikipedia.org/wiki/Atmosphere_of_Titan)

### 1.5 Gas Giant (Jupiter/Saturn)

**Jupiter:**

| Species | Volume mixing ratio |
|---------|-------------------|
| H2 | 86.2% |
| He | 13.6% |
| CH4 | 0.21% (2100 ppm) |
| NH3 | 0.07% (700 ppm) |
| HD | 0.003% |
| C2H6 | 5.8 ppm |
| H2O | ~0.0004% (variable) |
| PH3 | ~0.6 ppm |
| H2S | trace |
| CO2 | 5-35 ppb |

- He mass fraction: 0.236 (vs protosolar 0.274)
- Mean molecular mass: 2.22 g/mol
- Cloud-top temperature: ~140 K
- No solid surface; reference level at 1 bar

**Saturn:**

| Species | Volume mixing ratio |
|---------|-------------------|
| H2 | ~96.3% |
| He | ~3.25% |
| CH4 | 0.45% |
| NH3 | 0.0125% |
| HD | 0.011% |
| C2H6 | 7.0 ppm |

- He mass fraction: 0.18-0.25
- Cloud-top temperature: ~95 K

Source: [Atmospheres of the Giant Planets](https://pressbooks.online.ucf.edu/astronomybc/chapter/11-3-atmospheres-of-the-giant-planets/), [Jupiter, Wikipedia](https://en.wikipedia.org/wiki/Jupiter)

### 1.6 Ice Giant (Uranus/Neptune)

**Uranus:**

| Species | Volume mixing ratio |
|---------|-------------------|
| H2 | 82.5% |
| He | 15.2% |
| CH4 | 2.3% (below 1.3 bar cloud deck) |
| HD | 0.015% |

Stratospheric trace species:
- C2H2, C2H6: ~10^-7
- CO: 3 x 10^-8
- H2O: ~8 x 10^-9
- CO2: ~10^-11

Cloud decks:
| Cloud type | Pressure (bar) |
|-----------|----------------|
| CH4 ice | 1.2-2 |
| H2S / NH3 | 3-10 |
| NH4SH | 20-40 |
| H2O | 50-300 |

- He molar fraction: 0.152 +/- 0.033
- Effective temperature: 59.1 +/- 0.3 K
- Tropopause temperature: 49-57 K (varies with latitude)

**Neptune:**

| Species | Volume mixing ratio |
|---------|-------------------|
| H2 | 80% |
| He | 19% |
| CH4 | ~3% |

- H2S ice clouds detected at ~3 bar
- Effective temperature: ~59 K
- Internal heat source: radiates 2.6x what it absorbs from Sun

Sources: [Atmosphere of Uranus, Wikipedia](https://en.wikipedia.org/wiki/Atmosphere_of_Uranus), [Atmospheric chemistry on Uranus and Neptune, PMC](https://pmc.ncbi.nlm.nih.gov/articles/PMC7658780/)

---

## 2. Pressure-Temperature Profiles

### 2.1 Adiabatic Lapse Rate

**Dry adiabatic lapse rate (DALR):**

```
Gamma_d = g / c_p
```

Where:
- g = gravitational acceleration (m/s^2)
- c_p = specific heat capacity at constant pressure (J/kg/K)

For Earth: Gamma_d = 9.8067 / 1004 = **9.8 K/km**

**Moist (saturated) adiabatic lapse rate (MALR):**

```
Gamma_w = g * [1 + (H_v * r) / (R_sd * T)] / [c_pd + (H_v^2 * r) / (R_sw * T^2)]
```

Where:
- H_v = latent heat of vaporization of water = 2,501,000 J/kg
- r = mixing ratio of water vapor (kg/kg)
- R_sd = specific gas constant for dry air = 287 J/(kg K)
- R_sw = specific gas constant for water vapor = 461.5 J/(kg K)
- c_pd = specific heat of dry air at constant pressure = 1003.5 J/(kg K)
- T = temperature (K)

Typical Earth value: **~5 K/km** (range 3.6-9.2 K/km depending on temperature and moisture)

Source: [Lapse rate, Wikipedia](https://en.wikipedia.org/wiki/Lapse_rate)

### 2.2 Lapse Rate Values by Planet

| Planet | Dry adiabatic (K/km) | Observed average (K/km) | Notes |
|--------|---------------------|------------------------|-------|
| Earth | 9.8 | 6.5 (ICAO standard) | Tropopause at 11-12 km, T = 217 K |
| Venus | 10.5 | ~7.7 | ~identical to dry below clouds |
| Mars | 4.3 | ~2.5 | Reduced by dust absorption of solar radiation |
| Jupiter | ~2.0 | ~2.0 (troposphere) | Convective below tropopause |
| Titan | ~1.3 | ~1.0-1.3 | Low gravity (1.35 m/s^2), N2/CH4 atmosphere |

Venus dry adiabat: g/c_p = 8.87 / 850 = 10.4 K/km (CO2-dominated c_p)

Mars dry adiabat: g/c_p = 3.72 / 860 = 4.3 K/km

Sources: [Lapse rate, Wikipedia](https://en.wikipedia.org/wiki/Lapse_rate), [Atmospheric lapse rates table, ResearchGate](https://www.researchgate.net/figure/Atmospheric-lapse-rates-of-the-planets-and-selected-satellites_tbl2_281525629)

### 2.3 Scale Height

```
H = kT / (mg) = RT / (Mg)
```

Where:
- k = Boltzmann constant = 1.381 x 10^-23 J/K
- R = universal gas constant = 8.314 J/(mol K)
- T = temperature (K)
- m = mean molecular mass (kg)
- M = mean molar mass (kg/mol)
- g = gravitational acceleration (m/s^2)

| Body | Scale height (km) | T (K) | M (g/mol) | g (m/s^2) |
|------|--------------------|--------|-----------|-----------|
| Venus | 15.9 | 229 | 43.45 | 8.87 |
| Earth | 8.5 | 250 | 28.97 | 9.81 |
| Mars | 11.1 | 210 | 43.34 | 3.72 |
| Jupiter | 27 | 124 | 2.22 | 24.79 |
| Saturn | 59.5 | 95 | 2.07 | 10.44 |
| Titan | 21 | 85 | 28.6 | 1.35 |
| Uranus | 27.7 | 59 | 2.64 | 8.87 |
| Neptune | 19.1-20.3 | 59 | 2.53-2.69 | 11.15 |

Source: [Scale height, Wikipedia](https://en.wikipedia.org/wiki/Scale_height)

### 2.4 Vertical Structure Summaries

**Earth:**
- Troposphere: 0-12 km, -6.5 K/km, surface 288 K -> tropopause 217 K
- Stratosphere: 12-50 km, inversion due to O3 absorption, stratopause ~270 K
- Mesosphere: 50-85 km, -2.5 K/km, mesopause ~190 K
- Thermosphere: 85-600 km, strong heating, T up to 1500 K

**Venus:**
- Troposphere: 0-65 km, ~7.7 K/km
- Cloud deck: 48-70 km (lower: 48-52 km, middle: 52-57 km, upper: 57-70 km)
- At ~50 km altitude: P ~ 1 bar, T ~ 340 K (most Earth-like conditions in solar system)
- Tropopause: ~65 km, T ~ 240 K
- Mesosphere: 65-120 km

**Mars:**
- Troposphere: 0-40 km, ~2.5 K/km
- No stratospheric inversion (no O3 layer)
- Mesosphere: 40-100 km, mesopause T ~ 100-120 K
- Thermosphere: 100-230 km, daytime T up to 240-390 K
- Planetary boundary layer: can extend >10 km daytime

**Jupiter:**
- Troposphere: below ~0.1 bar, adiabatic, T increases with depth
- Tropopause: ~0.1 bar, T ~ 110 K
- Stratosphere: 0.1 bar to ~1 mbar, heated by CH4 absorption
- Temperature inversion above tropopause

---

## 3. Atmospheric Circulation

### 3.1 Hadley Cell Theory

The Held-Hou model (1980) predicts the width of the Hadley cell from angular momentum conservation and thermal wind balance.

**Thermal wind equation:**

```
f * du/dz = -(g / theta_0) * d_theta/dy
```

**Angular momentum per unit mass:**

```
M = (Omega * a * cos(phi) + u) * a * cos(phi)
```

**Held-Hou Hadley cell edge latitude (small-angle approximation):**

```
phi_H = (5/3 * Delta_theta / theta_0 * gH / Omega^2 * a^2)^(1/2)
```

Where:
- Delta_theta = equator-to-pole temperature difference
- theta_0 = reference potential temperature (~255 K)
- g = gravity
- H = tropopause height
- Omega = planetary rotation rate
- a = planetary radius

For Earth: using Delta_theta = 40 K, theta_0 = 255 K, H = 12 km, Omega = 7.27 x 10^-5 s^-1, a = 6.37 x 10^6 m, the predicted Hadley cell width is ~2400 km (~30 degrees latitude).

**Key prediction: the Hadley cell width is inversely proportional to the planetary rotation rate.** Slowly rotating planets have wider Hadley cells; rapidly rotating planets have narrower cells.

Sources: [Hadley cell, Wikipedia](https://en.wikipedia.org/wiki/Hadley_cell), [Tropical Meteorology Lectures](https://www.meteo.physik.uni-muenchen.de/~roger/Lectures/TropicalMetweb/TropicalMeteorology_Ch5.html)

### 3.2 Number of Circulation Cells vs Rotation Rate

| Rotation regime | Rossby number Ro | Cell pattern |
|----------------|------------------|-------------|
| Very slow (Ro >> 1) | >1 | 1 hemisphere-wide Hadley cell (Venus, Titan) |
| Earth-like (Ro ~ 1) | ~0.1-1 | 3 cells: Hadley, Ferrel, Polar |
| Fast (Ro << 1) | <0.01 | Multiple jets and bands (Jupiter, Saturn) |

**Rossby number:**

```
Ro = U / (f * L)
```

Where:
- U = characteristic wind speed (m/s)
- f = Coriolis parameter = 2 * Omega * sin(phi) (s^-1)
- L = characteristic length scale (m)

**Coriolis parameter f at 45 degrees latitude:**
- Earth: f = 2 * 7.27x10^-5 * sin(45) = 1.03 x 10^-4 s^-1
- Jupiter: f = 2 * 1.76x10^-4 * sin(45) = 2.49 x 10^-4 s^-1

**Thermal Rossby number:**

```
R_T = g * H * Delta_theta / (Omega^2 * a^2 * theta_0)
```

The thermal Rossby number has a quadratic dependence on the rotation rate (1/Omega^2), making rotation rate the dominant control on circulation pattern.

Source: [Wang et al. 2018, Comparative terrestrial atmospheric circulation regimes](https://rmets.onlinelibrary.wiley.com/doi/full/10.1002/qj.3350)

### 3.3 Jet Stream Formation and the Rhines Scale

Jet streams form when turbulent eddies encounter the beta effect (variation of the Coriolis parameter with latitude).

**Beta parameter:**

```
beta = df/dy = 2 * Omega * cos(phi) / a
```

For Earth at 45 degrees: beta = 2 * 7.27x10^-5 * cos(45) / 6.37x10^6 = 1.61 x 10^-11 m^-1 s^-1

**Rhines scale** (transition scale from turbulence to waves):

```
L_Rh = sqrt(U / beta)
```

Where U = characteristic eddy velocity. For U ~ 10 m/s on Earth:
L_Rh = sqrt(10 / 1.61x10^-11) ~ 2.5 x 10^4 m ~ 2500 km

This corresponds to the observed jet spacing (~30 degrees latitude).

**Rossby deformation radius** (baroclinic):

```
L_d = N * H / (f_0)
```

Where N = Brunt-Vaisala frequency, H = scale height.

For Earth: L_d ~ 1000 km at midlatitudes, decreasing poleward.
For ocean: L_d ~ 200 km at equator, <10 km at high latitudes.

The jet spacing scales with the Rhines scale when L_Rh > L_d (supercritical regime).

Source: [Rhines 1975, Waves and turbulence on a beta-plane](https://courses.physics.ucsd.edu/2018/Winter/physics116_216/Rhines75.pdf), [Rossby radius of deformation, Wikipedia](https://en.wikipedia.org/wiki/Rossby_radius_of_deformation)

### 3.4 Superrotation

Atmospheric superrotation occurs when the atmosphere rotates faster than the solid body. Quantified by the superrotation index s = (mean atmospheric angular velocity / solid body angular velocity) - 1.

| Body | Superrotation index s | Cloud-top wind (m/s) | Mechanism |
|------|----------------------|---------------------|-----------|
| Venus | 55-65 (at ~70 km) | 100 +/- 10 | Thermal tides + planetary Rossby waves + Gierasch mechanism |
| Titan | 8.5-15 (above 100 km) | 100-200 (stratosphere) | Meridional circulation + eddy momentum transport |
| Jupiter | 0.005-0.011 | ~90 (equatorial jet) | Deep convection + beta-plane turbulence |
| Saturn | 0.035-0.045 | ~450 (equatorial) | Similar to Jupiter |
| Earth | <0.01 (thermosphere) | 60-70 (upper tropics) | Minor; Hadley cell angular momentum transport |

**Gierasch mechanism (1975):** Meridional Hadley circulation transports angular momentum poleward in the upper branch. Equatorward momentum transport by eddies (Rossby waves, barotropic instability) completes the cycle, leading to net angular momentum accumulation at the equator.

**Venus specifics:** Atmosphere circles planet in ~4 Earth days vs 243-day rotation period. Wind speed decreases from ~100 m/s at cloud top (70 km) to <2 m/s at surface.

**Titan specifics:** TitanWRF simulations show rapid buildup to >100 m/s in a few Titan years. Obliquity 26.7 degrees drives strong seasonal Hadley circulation.

Source: [Atmospheric super-rotation, Wikipedia](https://en.wikipedia.org/wiki/Atmospheric_super-rotation), [Read & Lebonnois 2018, Superrotation on Venus, on Titan, and Elsewhere](https://web.lmd.jussieu.fr/~sllmd/pub/REF/2018AREPS..46..175R.pdf)

### 3.5 Wind Speed Summary

| Body | Peak wind speed (m/s) | Location |
|------|----------------------|----------|
| Earth | 60-70 | Upper troposphere subtropical jet |
| Venus | ~100 | Cloud top (70 km) |
| Mars | >30 | Dust mobilization threshold |
| Titan | 100-200 | Stratosphere |
| Jupiter | ~90 | Equatorial jet; 178 m/s in Great Red Spot |
| Saturn | ~500 | Equatorial jet |
| Uranus | ~200 | +/-60 latitude |
| Neptune | ~580 | Equatorial jet (2100 km/h) |

Neptune has the fastest winds in the solar system despite receiving the least solar energy, driven by internal heat release.

Sources: [Atmospheres of the Giant Planets](https://pressbooks.online.ucf.edu/astronomybc/chapter/11-3-atmospheres-of-the-giant-planets/), [NESDIS, How's the Weather on Other Planets](https://www.nesdis.noaa.gov/about/k-12-education/space-weather/hows-the-weather-other-planets)

---

## 4. Cloud Formation

### 4.1 Condensation Conditions

A species condenses when its partial pressure exceeds the saturation vapor pressure at the local temperature. The saturation vapor pressure is given by the Clausius-Clapeyron equation:

```
dP_sat/dT = L * P_sat / (R * T^2)
```

Integrated (approximate):

```
P_sat(T) = P_ref * exp[-L/(R) * (1/T - 1/T_ref)]
```

Where L = latent heat of vaporization, R = specific gas constant for the species.

### 4.2 Gas Giant Cloud Decks (Jupiter)

| Cloud layer | Condensate | Pressure (bar) | Temperature (K) | Color contribution |
|-------------|-----------|----------------|------------------|--------------------|
| Upper | NH3 ice | 0.5-1.0 | ~140-150 | White/cream |
| Middle | NH4SH | ~2 | ~200 | Brown/orange |
| Lower | H2O ice/liquid | ~5 | ~270 | White (deep, rarely visible) |

**Saturn:** Similar structure but deeper due to lower gravity and temperature:
- NH3 clouds: ~1.0-1.5 bar
- NH4SH: ~4-5 bar
- H2O: ~10 bar

**Uranus/Neptune:** Different due to low NH3 abundance:
- CH4 ice: 1.2-2 bar (~80 K)
- H2S/NH3: 3-10 bar
- NH4SH: 20-40 bar
- H2O: 50-300 bar

Source: [MDPI, Water Clouds on Jupiter](https://www.mdpi.com/2072-4292/14/18/4567), [Astronomy.com, Cloud formation on gas giants](https://www.astronomy.com/science/how-do-clouds-form-on-jupiter-or-other-gas-giants-and-how-deep-do-they-extend/)

### 4.3 Optical Depth

**Beer-Lambert law:**

```
I(s) = I_0 * exp(-tau)
```

**Optical depth definition:**

```
tau = integral_0^L alpha(z) dz
```

Where alpha(z) = extinction coefficient = sigma_ext * n(z), with sigma_ext = extinction cross-section and n(z) = number density.

**Transmittance:**

```
T = exp(-tau)      (T = 1 fully transparent, T = 0 opaque)
```

**Cross-section form:**

```
tau = sigma * N * L
```

Where sigma = attenuation cross-section, N = number density, L = path length.

**Cloud optical depth:**

```
tau_cloud = Q_e * [9 * pi * L^2 * H_c * N_d / (16 * rho_l^2)]^(1/3)
```

Where Q_e = extinction efficiency (~2 for large droplets), L = liquid water content, H_c = cloud thickness, N_d = droplet number density, rho_l = liquid water density.

**Slant optical depth:** For observations at zenith angle theta:

```
tau_slant = tau_vertical / cos(theta) = tau_vertical * m
```

Where m = airmass factor.

**Typical values:**
- Clear Earth atmosphere (vertical, visible): tau ~ 0.1-0.3
- Thin cirrus cloud: tau ~ 0.5-3
- Thick stratus cloud: tau ~ 20-50
- Cumulonimbus: tau ~ 50-200
- Mars background dust: tau ~ 0.15
- Mars global dust storm: tau > 4.0
- Venus cloud deck: tau ~ 30-40

Source: [Optical depth, Wikipedia](https://en.wikipedia.org/wiki/Optical_depth), [Atmosphere of Mars, Wikipedia](https://en.wikipedia.org/wiki/Atmosphere_of_Mars)

### 4.4 Mie Scattering for Rendering

**Applicable regime:** When particle diameter d ~ wavelength lambda (cloud droplets, aerosols, haze).

**Henyey-Greenstein phase function** (common approximation):

```
p_HG(theta) = (1 - g^2) / (4*pi * (1 + g^2 - 2*g*cos(theta))^(3/2))
```

Where:
- theta = scattering angle
- g = asymmetry parameter (-1 to +1)
  - g = 0: isotropic (Rayleigh limit)
  - g = 0.75-0.85: typical cloud droplets (forward scattering)
  - g = -0.75 to -0.999: Mie aerosol scattering in some rendering contexts

**Rayleigh scattering** (molecular, d << lambda):
- Phase function: p(theta) = (3/4)(1 + cos^2(theta))
- Cross-section proportional to lambda^-4 (blue sky)

**Mie scattering characteristics:**
- Weakly wavelength-dependent (white clouds)
- Strong forward peak
- Multiple scattering dominates when tau > 1

**Rendering approximation (GPU Gems 2, O'Neil):**
- Density falloff: rho(h) = rho_0 * exp(-h/H_0)
- Out-scattering: tau(Pa, Pb) = 4*pi * K * integral_Pa^Pb rho(s) ds
- In-scattering combines phase function, optical depth, and solar intensity
- Scale height H_0 typically set to 0.25 of atmosphere thickness for rendering

Source: [GPU Gems 2, Chapter 16, NVIDIA](https://developer.nvidia.com/gpugems/gpugems2/part-ii-shading-lighting-and-shadows/chapter-16-accurate-atmospheric-scattering), [NVIDIA Research, Approximate Mie](https://research.nvidia.com/labs/rtr/approximate-mie/publications/approximate-mie.pdf)

---

## 5. Greenhouse Effect

### 5.1 Radiative Equilibrium Temperature

**Fundamental equation:**

```
T_eq = [S * (1 - A_B) / (4 * sigma)]^(1/4)
```

Where:
- S = stellar flux at planet's orbital distance (W/m^2)
- A_B = Bond albedo
- sigma = Stefan-Boltzmann constant = 5.670 x 10^-8 W/(m^2 K^4)
- Factor of 4 accounts for ratio of cross-section to surface area of sphere

**Stellar flux (inverse-square law):**

```
S = L_star / (4 * pi * d^2)
```

For the Sun at 1 AU: S = 1361 W/m^2 (solar constant)

### 5.2 Greenhouse Warming by Planet

| Planet | Albedo A_B | T_eq (K) | T_surface (K) | Delta_T_greenhouse (K) |
|--------|-----------|----------|---------------|----------------------|
| Venus | 0.76 | 226 | 740 | **+514** |
| Earth | 0.306 | 255 | 288 | **+33** |
| Mars | 0.25 | 210 | 215 | **+5** |
| Moon | 0.12 | 271 | 250 (avg) | -21 (no atmosphere) |
| Titan | 0.22 | 82 | 94 | **+12** |

Earth's equilibrium temperature calculation:
T_eq = [1361 * (1 - 0.306) / (4 * 5.670x10^-8)]^(1/4) = [944.2 / 2.268x10^-7]^(1/4) = [4.163x10^9]^(1/4) = 254 K

Source: [Planetary equilibrium temperature, Wikipedia](https://en.wikipedia.org/wiki/Planetary_equilibrium_temperature)

### 5.3 Runaway Greenhouse Effect

**Simpson-Nakajima limit:** The maximum outgoing longwave radiation (OLR) that a water-saturated atmosphere can emit, regardless of surface temperature.

```
OLR_max ~ 282 W/m^2 (Goldblatt et al. 2013)
OLR_max ~ 293 W/m^2 (Simpson-Nakajima classic estimate)
```

**Komabayashi-Ingersoll limit:** The upper stratospheric radiation limit:

```
OLR_KI ~ 385 W/m^2
```

**3D model result (Leconte et al. 2013):** Accounting for atmospheric dynamics (Hadley circulation, unsaturated subsident regions), the threshold is raised to:

```
Absorbed solar flux threshold ~ 375 W/m^2
```

**Earth's current absorbed solar flux:**

```
F_abs = S * (1 - A_B) / 4 = 1361 * 0.694 / 4 = 240 W/m^2
```

This is well below all estimated thresholds, providing a margin of 42-135 W/m^2 before runaway.

Sources: [Leconte et al. 2013, Nature](https://www.nature.com/articles/nature12827), [Goldblatt et al. 2013, Nature Geoscience](https://www.nature.com/articles/ngeo1892), [Runaway greenhouse effect, Wikipedia](https://en.wikipedia.org/wiki/Runaway_greenhouse_effect)

### 5.4 Habitable Zone Boundaries

**General scaling:**

```
d_HZ = sqrt(L_star / L_Sun) * d_Sun,HZ
```

Where d_Sun,HZ is the HZ distance for the Sun.

**Estimates for the Sun:**

| Source | Inner edge (AU) | Outer edge (AU) | Basis |
|--------|----------------|-----------------|-------|
| Kasting et al. 1993 (conservative) | 0.95 | 1.37 | Runaway greenhouse / max CO2 greenhouse |
| Kasting et al. 1993 (optimistic) | 0.84 | 1.67 | Recent Venus / early Mars |
| Kopparapu et al. 2013 (conservative) | 0.99 | 1.67 | Updated opacity database |
| Kopparapu et al. 2013 (optimistic) | 0.97 | 1.67 | |
| Hart 1979 | 0.958 | 1.004 | Very narrow (now considered too restrictive) |
| Ramirez & Kaltenegger 2017 | 0.95 | 2.4 | With volcanic hydrogen |
| Pierrehumbert & Gaidos 2011 | -- | up to 10 | With primordial H2 greenhouse |

**Inner edge criterion:** Water loss via moist/runaway greenhouse. Surface temperature reaches ~340 K (60 C), stratosphere becomes wet, hydrogen escapes to space.

**Outer edge criterion:** Maximum CO2 greenhouse. Increasing CO2 eventually causes Rayleigh scattering to dominate, reflecting more sunlight than it traps.

Sources: [Habitable zone, Wikipedia](https://en.wikipedia.org/wiki/Habitable_zone), [Astrobites, Finding the Edges of the Habitable Zone](https://astrobites.org/2013/02/07/finding-the-edges-of-the-habitable-zone/)

---

## 6. Biome Distribution Models

### 6.1 Koppen Climate Classification

Five primary groups with quantitative temperature and precipitation thresholds:

**Group A: Tropical (all months >= 18 C mean)**

| Subtype | Code | Criteria |
|---------|------|----------|
| Tropical rainforest | Af | Driest month >= 60 mm precipitation |
| Tropical monsoon | Am | Driest month < 60 mm but >= 100 - (P_annual/25) |
| Tropical savanna | Aw/As | Driest month < 60 mm AND < 100 - (P_annual/25) |

**Group B: Arid (precipitation below threshold)**

Precipitation threshold P_th (mm):

```
P_th = 20 * T_mean + 280    (if >= 70% of precip falls in summer half)
P_th = 20 * T_mean + 140    (if 30-70% falls in summer half)
P_th = 20 * T_mean           (if < 30% falls in summer half)
```

| Subtype | Code | Criteria |
|---------|------|----------|
| Hot desert | BWh | P_annual < 0.5 * P_th, T_mean >= 18 C |
| Cold desert | BWk | P_annual < 0.5 * P_th, T_mean < 18 C |
| Hot steppe | BSh | P_annual 0.5-1.0 * P_th, T_mean >= 18 C |
| Cold steppe | BSk | P_annual 0.5-1.0 * P_th, T_mean < 18 C |

**Group C: Temperate (coldest month 0 C to 18 C, warmest month > 10 C)**

| Subtype | Code | Second letter criteria | Third letter criteria |
|---------|------|----------------------|---------------------|
| Humid subtropical | Cfa | f: no dry season | a: warmest month >= 22 C |
| Oceanic | Cfb | f: no dry season | b: all months < 22 C, >= 4 months > 10 C |
| Subpolar oceanic | Cfc | f: no dry season | c: 1-3 months > 10 C |
| Monsoon subtropical | Cwa | w: dry winter | a: warmest month >= 22 C |
| Mediterranean hot | Csa | s: dry summer, driest summer month < 40 mm and < 1/3 wettest winter month | a: warmest month >= 22 C |
| Mediterranean warm | Csb | s: dry summer | b: warmest < 22 C |

**Group D: Continental (coldest month < 0 C, warmest month > 10 C)**

Same second/third letters as C, plus:

| Subtype | Code | Extra criteria |
|---------|------|---------------|
| Subarctic extreme | Dfd/Dwd/Dsd | Coldest month < -38 C |

**Group E: Polar (warmest month < 10 C)**

| Subtype | Code | Criteria |
|---------|------|----------|
| Tundra | ET | Warmest month 0-10 C |
| Ice cap | EF | All months < 0 C |

Source: [Koppen climate classification, Wikipedia](https://en.wikipedia.org/wiki/K%C3%B6ppen_climate_classification), [Koppen climate classification, Britannica](https://www.britannica.com/science/Koppen-climate-classification)

### 6.2 Whittaker Biome Diagram

Classifies biomes on two axes: **mean annual temperature (C)** vs **mean annual precipitation (cm/year)**.

Approximate boundary values for major biomes:

| Biome | Temperature range (C) | Precipitation range (cm/yr) |
|-------|----------------------|----------------------------|
| Tropical rainforest | 20-30 | 250-500+ |
| Tropical seasonal forest | 20-30 | 100-250 |
| Subtropical desert | 20-30 | 0-25 |
| Temperate grassland | 0-20 | 30-100 |
| Temperate deciduous forest | 5-20 | 75-250 |
| Temperate rainforest | 5-15 | 200-400+ |
| Boreal forest (taiga) | -10 to 5 | 30-85 |
| Tundra | -15 to 0 | 15-50 |
| Arctic/alpine desert | < -10 | 0-25 |
| Woodland/shrubland | 10-25 | 25-75 |
| Savanna | 20-30 | 50-130 |

Key boundaries:
- 10 C isotherm separates forest from tundra (warmest month)
- 18 C isotherm separates tropical from temperate
- ~25 cm/yr precipitation separates desert from steppe/grassland
- ~75 cm/yr separates grassland from forest (in temperate zone)

Source: [Biome, Wikipedia](https://en.wikipedia.org/wiki/Biome), [SERC/Carleton, Introduction to Biomes](https://serc.carleton.edu/eslabs/weather/4a.html), [Whittaker_biomes dataset, plotbiomes R package](https://rdrr.io/github/valentinitnelav/plotbiomes/man/Whittaker_biomes.html)

### 6.3 Holdridge Life Zones

A triaxial classification system using logarithmic scales for three variables:

**Axes (all logarithmic base-2 intervals):**

1. **Mean annual biotemperature (C):** Temperature averaged over the year with values below 0 C and above 30 C set to 0 (plants dormant outside this range).
   - Boundary values: 1.5, 3, 6, 12, 24 C

2. **Mean annual precipitation (mm):**
   - Boundary values: 62.5, 125, 250, 500, 1000, 2000, 4000, 8000 mm

3. **Potential evapotranspiration ratio (PET/P):**
   - Boundary values: 0.125, 0.25, 0.5, 1.0, 2.0, 4.0, 8.0, 16.0, 32.0, 64.0

**Latitudinal belts (defined by mean annual biotemperature):**

| Belt | Biotemperature range (C) |
|------|-------------------------|
| Polar | < 1.5 |
| Subpolar | 1.5-3 |
| Boreal | 3-6 |
| Cool temperate | 6-12 |
| Warm temperate | 12-18 |
| Subtropical | 18-24 |
| Tropical | > 24 |

**Humidity provinces:**

| Province | PET ratio range |
|----------|----------------|
| Superarid | > 32 |
| Perarid | 16-32 |
| Arid | 8-16 |
| Semiarid | 4-8 |
| Subhumid | 2-4 |
| Humid | 1-2 |
| Perhumid | 0.5-1 |
| Superhumid | 0.25-0.5 |
| Supersaturated | < 0.25 |

**Aridity index thresholds:**
- AI < 0.2: arid/hyperarid
- AI < 0.5: dry

The system yields ~38 distinct life zones arranged as hexagons in the triangular diagram, from polar desert (low biotemperature, low precipitation) to tropical rain forest (high biotemperature, high precipitation).

**PET calculation (Holdridge):**

```
PET = BT * 58.93 (mm/year)
```

Where BT = mean annual biotemperature in C.

Sources: [Holdridge life zones, Wikipedia](https://en.wikipedia.org/wiki/Holdridge_life_zones), [Holdridge 1967, Life Zone Ecology](https://app.ingemmet.gob.pe/biblioteca/pdf/Amb-56.pdf), [US Forest Service, Holdridge Life Zones of the US](https://research.fs.usda.gov/treesearch/30306)

---

## Quick Reference: Key Equations Summary

| Equation | Formula | Application |
|----------|---------|-------------|
| Scale height | H = kT/(mg) | Exponential pressure decay |
| Dry lapse rate | Gamma = g/c_p | Temperature decrease with altitude |
| Equilibrium temperature | T_eq = [S(1-A)/(4*sigma)]^(1/4) | Planetary temperature without greenhouse |
| Optical depth | tau = integral sigma*n*ds | Light attenuation through medium |
| Beer-Lambert | I = I_0 * exp(-tau) | Transmitted intensity |
| Rossby number | Ro = U/(fL) | Rotation vs advection dominance |
| Coriolis parameter | f = 2*Omega*sin(phi) | Planetary vorticity at latitude phi |
| Rhines scale | L_Rh = sqrt(U/beta) | Jet spacing prediction |
| Rossby deformation radius | L_d = NH/(n*pi*f_0) | Baroclinic instability scale |
| Hadley cell width | phi_H ~ (5/3 * DeltaT/T_0 * gH/(Omega^2 * a^2))^(1/2) | Held-Hou prediction |
| HG phase function | p(theta) = (1-g^2)/(4*pi*(1+g^2-2g*cos(theta))^(3/2)) | Cloud/aerosol scattering |
| Clausius-Clapeyron | dP_sat/dT = L*P/(R*T^2) | Saturation vapor pressure |
| HZ distance scaling | d_HZ = sqrt(L/L_Sun) * d_Sun | Habitable zone for other stars |
| Koppen B threshold | P_th = 20*T + (0/140/280) | Arid climate boundary |
| Holdridge PET | PET = BT * 58.93 mm/yr | Potential evapotranspiration |
