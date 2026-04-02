# Planetary Interior & Formation: Consolidated Deep-Dive Reference

_Consolidated from four research reports -- 2026-04-02_
_Sources: planetary-internal-structure-geodynamics.md, planetary-accretion-differentiation.md, planet-formation-accretion.md, researcher-a.md_

---

## Executive Summary

This document consolidates deep-dive research on planetary formation, interior structure, and geodynamics into a single reference for procedural planet generation. It covers the full lifecycle from protoplanetary disk structure through accretion, differentiation, internal layering, convection, tectonic regime selection, and eventual geological death. Content that overlaps with the parent `final.md` (frost line table, basic MMSN formulas, bulk Earth composition, basic Rayleigh number, Toomre Q mention) is either omitted or expanded with substantially deeper detail here.

**Key themes for planet-gen:**

- Disk structure sets the compositional palette available at each orbital distance
- Multiple accretion mechanisms (planetesimal, pebble, gravitational instability) operate on different timescales
- Differentiation timing and mechanism depend on planet mass and heat sources
- Tectonic regime is a multi-parameter decision (size, water, temperature, viscosity, yield stress)
- Planetary cooling and geological death follow predictable scaling laws modulated by tectonic mode

---

## 1. Protoplanetary Disk Structure

### 1.1 Surface Density Profiles -- MMSN Model

The Minimum Mass Solar Nebula (MMSN) is derived by augmenting current planetary masses back to solar composition and spreading into annuli (Hayashi 1981).

**Gas surface density:**

$$\Sigma_{\mathrm{gas}}(r) = 1700 \left(\frac{r}{1\;\mathrm{AU}}\right)^{-3/2} \;\mathrm{g\,cm^{-2}}$$

Alternative normalization by Weidenschilling (1977): $\Sigma_0 \approx 4200\;\mathrm{g\,cm^{-2}}$.

**Solids surface density** (with ice line jump):

$$\Sigma_{\mathrm{solids}}(r) = \begin{cases} 7.1 \left(\frac{r}{1\;\mathrm{AU}}\right)^{-3/2}\;\mathrm{g\,cm^{-2}} & r < r_{\mathrm{ice}} \\ 30 \left(\frac{r}{1\;\mathrm{AU}}\right)^{-3/2}\;\mathrm{g\,cm^{-2}} & r > r_{\mathrm{ice}} \end{cases}$$

The factor-of-~4 jump beyond the ice line reflects the addition of water ice to the solid inventory.

### 1.2 Temperature Profile

For a passively irradiated, optically thin disk:

$$T(r) = 280 \left(\frac{r}{1\;\mathrm{AU}}\right)^{-1/2}\;\mathrm{K}$$

For an actively accreting disk with viscous heating:

$$T(r) \propto r^{-3/4} \quad \text{(viscous heating dominated, inner disk } \lesssim \text{few AU)}$$

$$T(r) \propto r^{-1/2} \quad \text{(stellar irradiation dominated, outer disk)}$$

Real disks transition between these regimes.

### 1.3 Disk Scale Height

The vertical pressure scale height:

$$H = \frac{c_s}{\Omega_K} = \frac{c_s}{\sqrt{GM_\star/r^3}}$$

For the MMSN temperature profile:

$$\frac{H}{r} \approx 0.033 \left(\frac{r}{1\;\mathrm{AU}}\right)^{1/4}$$

This gives $H/r \approx 0.033$ at 1 AU and $H/r \approx 0.07$ at 30 AU (a flared disk geometry).

### 1.4 Dust-to-Gas Ratio

The canonical initial dust-to-gas mass ratio:

$$f_{\mathrm{d/g}} \approx 0.01$$

This ratio evolves with time: radial drift concentrates solids at pressure bumps and near snow lines, and after a few Myr the ratio can locally exceed 0.01 by factors of several.

### 1.5 Disk Lifetime Constraints

| Quantity                                  | Value      | Source                      |
| ----------------------------------------- | ---------- | --------------------------- |
| Median disk lifetime                      | ~3 Myr     | Haisch et al. 2001          |
| Mean disk lifetime                        | 3.7 Myr    | Alexander et al. 2014, PPVI |
| Range                                     | 1--10 Myr  | Alexander et al. 2014       |
| Gas disk upper limit                      | 10--20 Myr | Fedele et al. 2010          |
| Dispersal timescale (UV photoevaporation) | ~$10^5$ yr | Alexander et al. 2014       |

The rapid final dispersal (~$10^5$ yr) after a longer viscous-evolution phase (~few Myr) is the "two-timescale" problem, explained by UV/X-ray photoevaporation clearing the disk from the inside out once the accretion rate drops below the photoevaporative mass-loss rate.

---

## 2. Frost/Snow Lines and Compositional Boundaries

### 2.1 The Soot Line (~0.1 AU)

The **soot line** at ~0.1 AU marks where carbon-rich refractory organics are destroyed by high temperatures, potentially affecting the carbon budget of close-in planets.

### 2.2 Full Condensation Sequence

| Species                              | Condensation Temperature | Approx. Distance (MMSN) | Notes                         |
| ------------------------------------ | ------------------------ | ----------------------- | ----------------------------- |
| Refractory oxides (Al$_2$O$_3$, CaO) | ~1,700 K                 | <0.5 AU                 | First solids to condense      |
| Silicates (MgSiO$_3$)                | ~1,300--1,500 K          | ~0.1--0.4 AU            | Rock-forming minerals         |
| Iron/Nickel metals                   | ~1,400 K                 | ~0.1--0.3 AU            | Metallic grains               |
| **H$_2$O (water ice)**               | **150--170 K**           | **2.7--3.2 AU**         | Most important frost line     |
| NH$_3$ (ammonia)                     | ~80 K                    | ~9 AU                   | Ammonia hydrate               |
| CO$_2$ (carbon dioxide)              | ~70 K                    | ~10 AU                  | Dry ice                       |
| CH$_4$ (methane)                     | ~30--31 K                | ~30 AU                  | Methane clathrate at higher T |
| CO (carbon monoxide)                 | ~20--25 K                | ~30--50 AU              | Highly volatile               |
| N$_2$ (molecular nitrogen)           | ~20--22 K                | ~30--50+ AU             | Similar to CO                 |
| Ar (argon)                           | ~20 K                    | ~50+ AU                 | Noble gas                     |

**H$_2$O ice line estimates vary:**

- 170 K at 2.7 AU (Hayashi 1981)
- 143 K at 3.2 AU (Podolak & Zucker 2010)
- ~150 K for micron grains, ~200 K for km bodies (D'Angelo & Podolak 2015)
- Formation-epoch frost line: ~5 AU (based on asteroid belt composition)

### 2.3 Snow Line Time Evolution

The snow line is not static:

- **Early phase** (Class 0/I, high accretion): viscous heating pushes the water ice line out to ~5--10 AU
- **Peak**: up to ~17.4 AU for a solar-mass star during the high-luminosity protostellar phase
- **Late phase** (Class II/III): as accretion drops, the snow line retreats inward to ~1--3 AU
- The present-day solar system preserves the "fossil" snow line at ~2.7 AU (asteroid belt boundary)

### 2.4 Full Compositional Gradient from Rocky to Kuiper Belt

| Zone            | Distance     | Dominant Composition           | Key Species                              |
| --------------- | ------------ | ------------------------------ | ---------------------------------------- |
| Inner           | < 1.5 AU     | Refractory metals + silicates  | Fe, Ni, MgSiO$_3$, SiO$_2$               |
| Inner/mid       | 1.5--2.7 AU  | Dry silicates + some hydration | S-type asteroids                         |
| Ice line        | ~2.7--3.2 AU | Water ice + silicates          | C-type asteroids, hydrated minerals      |
| Outer           | 5--10 AU     | H/He gas + ice/rock core       | Jupiter, Saturn                          |
| Far outer       | 15--30 AU    | Ice mantle + thin H/He         | Uranus, Neptune (H$_2$O, NH$_3$, CH$_4$) |
| Trans-Neptunian | > 30 AU      | Volatile ices + dust           | KBOs, comets (CO, N$_2$, CH$_4$ ice)     |

**Asteroid belt composition maps the frost line:**

- S-type asteroids (< 2.7 AU): rocky, anhydrous silicates
- C-type asteroids (> 2.7 AU): carbonaceous, hydrated minerals, up to ~10--20% water by mass
- M-type asteroids: metallic (iron-nickel), differentiated core remnants

**Planet-to-planet metal/silicate variation:**

- Mercury: ~70% metal, ~30% silicates (unusually iron-rich)
- Venus, Earth: ~33% metal, ~67% silicates
- Mars: ~25% metal, ~75% silicates

**Gas and ice giant structure:**

| Body                               | H+He                          | Heavy elements                        | Core                                      |
| ---------------------------------- | ----------------------------- | ------------------------------------- | ----------------------------------------- |
| Jupiter (317.8 $M_\oplus$, 5.2 AU) | ~80--87%                      | ~13--20% (25--45 $M_\oplus$)          | ~7--25 $M_\oplus$ (possibly dilute/fuzzy) |
| Saturn (95.2 $M_\oplus$, 9.5 AU)   | ~70--80%                      | ~20--30% (16--30 $M_\oplus$)          | ~10--20 $M_\oplus$                        |
| Uranus (14.5 $M_\oplus$, 19.2 AU)  | 0.5--1.5 $M_\oplus$ (~5--15%) | Ices: 9.3--13.5 $M_\oplus$ (~65--80%) | Rock: 0.5--3.7 $M_\oplus$                 |
| Neptune (17.1 $M_\oplus$, 30.1 AU) | 1--2 $M_\oplus$ (~6--12%)     | Ices: 10--15 $M_\oplus$ (~60--80%)    | Rock: 1.2--3 $M_\oplus$                   |

**Kuiper Belt / Comets** (most primitive volatile inventory):

- Water ice: ~50--60%
- Silicate dust: ~25--30%
- Organic compounds: ~10--15%
- CO, CO$_2$, CH$_4$, N$_2$, NH$_3$ ices: ~5--10%
- Comet 67P dust-to-ice ratio: ~4:1 (more refractory than expected)

**Volatile depletion:** Inner system bodies are depleted in volatiles (H, C, N, noble gases) by factors of $10^3$--$10^6$ relative to solar abundances.

---

## 3. Accretion Mechanisms

### 3.1 Dust to Planetesimals: Overcoming the Meter-Size Barrier

Planet formation begins with micron-sized dust grains colliding and sticking via van der Waals forces. The **meter-size barrier** -- bodies ~1 m experience aerodynamic drag spiraling them into the star in ~100 years -- is overcome by:

- **Streaming instability**: Solids concentrate when local solid-to-gas ratio exceeds ~1, with optimal Stokes numbers 0.01--3. Dense filaments collapse into ~100 km-scale planetesimals.
- **Turbulent concentration**: In stagnant zones between turbulent eddies, solid-to-gas ratios can reach ~100, enabling rapid gravitational collapse.

The dust-to-planetesimal stage spans approximately **$10^6$ years (1 Myr)**.

### 3.2 Gravitational Focusing and Safronov Number

The collision cross-section including gravitational focusing:

$$\sigma_{\mathrm{col}} = \pi (R_1 + R_2)^2 \left(1 + \frac{v_{\mathrm{esc}}^2}{\Delta v^2}\right) = \sigma_{\mathrm{geo}}(1 + \Theta)$$

where $\Theta = v_{\mathrm{esc}}^2 / \Delta v^2$ is the **Safronov gravitational focusing factor**. When $\Theta \gg 1$, the effective cross-section is enormously larger than geometric, enabling rapid accretion.

### 3.3 Hill Sphere and Feeding Zone Width

$$R_H = a \left(\frac{M_p}{3 M_\star}\right)^{1/3}$$

Numerically: $R_H \approx 0.01\;\mathrm{AU}$ for an Earth-mass planet at 1 AU around a solar-mass star.

The **feeding zone** extends to $\sim \tilde{b} R_H$ where $\tilde{b} \approx 4$--$5$. During the giant impact phase, feeding zones overlap and embryos scatter material from a range of heliocentric distances -- Earth likely accreted material from ~0.5--4 AU, progressively sampling more volatile-rich (oxidized) material over time.

### 3.4 Runaway Growth

In the dispersion-dominated regime where $R_H \Omega_K < \Delta v < v_{\mathrm{esc}}$:

$$\sigma_{\mathrm{col}} \propto M_p^{4/3}, \qquad t_{\mathrm{gr}} \propto M_p^{-1/3}$$

More massive bodies grow **faster**, making mass growth super-exponential:

$$M_p(t) \propto \exp(t/t_0)$$

Timescale: ~$10^4$--$10^5$ years. Ceases once the protoplanet stirs up surrounding planetesimals.

### 3.5 Oligarchic Growth

Once the protoplanet dominates local velocity dispersion, growth transitions to **oligarchic growth**:

- Protoplanets spaced by ~$10\,R_H$
- Growth is orderly: similar-sized bodies grow at comparable rates
- Produces **several tens to ~100 lunar- to Mars-mass embryos** (0.01--0.1 $M_\oplus$)

**Isolation mass** (maximum from depleting the feeding zone):

$$M_{\mathrm{iso}} = \frac{(2\pi \tilde{b} \Sigma_s r^2)^{3/2}}{(3 M_\star)^{1/2}} \approx 0.11\;M_\oplus \quad \text{(at 1 AU in MMSN)}$$

At 5 AU (beyond ice line, $\Sigma_s$ increases ~4x): $M_{\mathrm{iso}} \sim 5$--$10\;M_\oplus$.

### 3.6 Pebble Accretion

A more recently recognized mechanism that dramatically accelerates core growth. Millimeter- to centimeter-sized pebbles drifting inward are efficiently captured by existing planetesimals due to aerodynamic drag. Cores can grow to several Earth masses in as little as **~100,000 years**, resolving earlier timescale problems for giant planet core formation.

### 3.7 Giant Impact Phase

Over 10--100 Myr, mutual gravitational perturbations among embryos lead to orbit-crossing and high-velocity collisions that assemble final terrestrial planets. Earth's Moon-forming giant impact is the archetype.

### 3.8 N-Body Mechanics for Planetesimal Formation

The planetesimal accretion rate from two-body collision timescale:

$$t_{\mathrm{col}} = (n_s \sigma_s \Delta v)^{-1} \sim \frac{R_s \rho_\bullet}{\Sigma_s \Omega_K} \approx 1.6 \times 10^3\;\mathrm{yr}$$

Growth timescale in the geometric limit (no focusing):

$$t_{\mathrm{gr,geo}} = \frac{m_p}{m_s (n_s \sigma_p \Delta v)} \approx 8 \times 10^7\;\mathrm{yr}$$

With gravitational focusing ($\Theta \gg 1$):

$$\frac{dM}{dt} \sim \Sigma_s \Omega_K R_p^2 \Theta$$

### 3.9 Summary of Formation Timescales

| Stage                                            | Timescale          | Product                              |
| ------------------------------------------------ | ------------------ | ------------------------------------ |
| Dust coagulation to pebbles                      | ~$10^3$--$10^4$ yr | mm--cm grains                        |
| Pebbles to planetesimals (streaming instability) | ~$10^5$--$10^6$ yr | ~100 km bodies                       |
| Runaway growth                                   | ~$10^4$--$10^5$ yr | Dominant embryo per zone             |
| Oligarchic growth                                | ~$10^6$ yr         | 10s--100s of lunar/Mars-mass embryos |
| Giant impacts / final assembly                   | $10^7$--$10^8$ yr  | Terrestrial planets                  |

---

## 4. Gravitational Instability (Disk Instability)

### 4.1 Toomre Q Parameter with Full Detail

The stability of a self-gravitating, differentially rotating gas disk:

$$Q = \frac{c_s \kappa}{\pi G \Sigma}$$

where $c_s$ = sound speed, $\kappa$ = epicyclic frequency ($= \Omega_K$ for Keplerian disk), $\Sigma$ = gas surface density.

**Stability criterion:**

- $Q > 1$: stable against axisymmetric perturbations
- $Q < 1$: gravitationally unstable, fragments
- $Q \lesssim 1.5$--$1.7$: unstable to non-axisymmetric (spiral) modes in 3D disks

**Dispersion relation** for axisymmetric perturbations in a thin gas disk:

$$\omega^2 = \kappa^2 - 2\pi G \Sigma |k| + c_s^2 k^2$$

**Most unstable wavelength** (fastest-growing mode):

$$\lambda_{\mathrm{crit}} = \frac{2 c_s^2}{G \Sigma}$$

**Maximum unstable wavelength:**

$$\lambda_{\mathrm{max}} = \frac{4\pi^2 G \Sigma}{\kappa^2}$$

### 4.2 Cooling Time Constraint for Fragmentation

A disk with $Q \sim 1$ can self-regulate through spiral-arm heating unless cooling is rapid. Fragmentation requires (Gammie 2001):

$$\beta \equiv t_{\mathrm{cool}} \cdot \Omega_K < \beta_{\mathrm{crit}}$$

| Adiabatic index $\gamma$ | $\beta_{\mathrm{crit}}$ | Source                         |
| ------------------------ | ----------------------- | ------------------------------ |
| -- (2D local)            | ~3                      | Gammie (2001)                  |
| 5/3                      | 6--7                    | Rice, Lodato & Armitage (2005) |
| 7/5                      | 12--13                  | Rice, Lodato & Armitage (2005) |

The critical $\beta$ remains debated; simulations find $\beta_{\mathrm{crit}} \approx 2$--$13$ depending on resolution and equation of state.

### 4.3 Where GI Operates and Outcomes

**Conditions:**

- Outer disk ($r \gtrsim 50$--$100$ AU) where $\Sigma$ is high relative to temperature
- Massive disks ($M_{\mathrm{disk}}/M_\star \gtrsim 0.1$)
- Early times when the disk is still being fed by envelope infall

**Typical fragment masses:**

- Set by the local Jeans mass: a few $M_J$ (Jupiter masses)
- Initial clump masses: ~1--10 $M_J$
- Formation timescale: ~a few $\times 10^3$ yr (dynamical timescale)

GI naturally explains directly-imaged giant planets at wide orbits ($\gtrsim 20$--$100$ AU) but has difficulty producing close-in planets without subsequent migration.

---

## 5. Planet Migration

### 5.1 Type I Migration (Low-Mass Planets)

Affects planets too small to open a gap ($M_p \lesssim M_{\mathrm{Saturn}}$). The planet excites spiral density waves at Lindblad resonances.

**Normalization torque:**

$$\Gamma_0 = \left(\frac{M_p}{M_\star}\right)^2 \left(\frac{H}{r}\right)^{-2} \Sigma_g r^4 \Omega_K^2$$

**Lindblad (wave) torque** (Tanaka et al. 2002):

$$\Gamma_L \approx -(2.0\text{--}2.3)\;\Gamma_0 \left(\frac{H}{r}\right)^{-1}$$

**Type I migration timescale:**

$$\tau_I = \frac{L}{2\Gamma} \sim \frac{1}{2} \left(\frac{M_\star}{M_p}\right) \left(\frac{M_\star}{\Sigma_g r^2}\right) \left(\frac{H}{r}\right)^2 \Omega_K^{-1}$$

Numerical estimate for 1 $M_\oplus$ in MMSN at 1 AU: $\tau_I \sim 10^{4}$--$10^{5}\;\mathrm{yr}$

This creates the **Type I migration problem** -- low-mass planets should spiral into the star before growing. Solutions include entropy-related corotation torques, opacity transition traps, and planet traps at ice lines and dead-zone edges.

### 5.2 Type II Migration (Gap-Opening Planets)

Massive planets ($M_p \gtrsim M_{\mathrm{Saturn}}$) open a gap. **Gap-opening thresholds:**

**Thermal criterion** (tidal torques exceed pressure):

$$\frac{M_p}{M_\star} \gtrsim 3 \left(\frac{H}{r}\right)^3$$

**Viscous criterion** (tidal torques exceed viscous diffusion):

$$\frac{M_p}{M_\star} \gtrsim 40\alpha \left(\frac{H}{r}\right)^2$$

For $H/r = 0.05$ and $\alpha = 10^{-3}$, minimum gap-opening mass:

$$M_{\mathrm{gap}} \approx 30 \left(\frac{\alpha}{10^{-3}}\right) \left(\frac{r}{1\;\mathrm{AU}}\right)^{1/2} \left(\frac{M_\star}{M_\odot}\right)\;M_\oplus$$

which is roughly Saturn-mass (~0.3 $M_J$) for typical parameters.

**Type II migration timescale** (planet locked to viscous evolution):

$$\tau_{II} = \frac{r^2}{\nu} = \frac{1}{\alpha} \left(\frac{H}{r}\right)^{-2} \Omega_K^{-1}$$

$$\tau_{II} \approx 0.7 \times 10^5 \left(\frac{\alpha}{10^{-3}}\right)^{-1} \left(\frac{a}{1\;\mathrm{AU}}\right) \left(\frac{M_\star}{M_\odot}\right)^{-1/2}\;\mathrm{yr}$$

### 5.3 The Nice Model (Post-Disk Dynamical Evolution)

Initial configuration after disk dispersal: Jupiter ~5.5 AU, Saturn ~8.0 AU, Uranus ~11 AU, Neptune ~14 AU, with a dense Kuiper belt disk (15--35 AU, ~35 $M_\oplus$).

Sequence: slow planetesimal-driven migration over ~500--800 Myr, Jupiter/Saturn cross 2:1 mean-motion resonance, triggering gravitational instability that scatters Uranus/Neptune outward and causes the Late Heavy Bombardment (~3.8--4.1 Gyr ago).

---

## 6. Differentiation & Core Formation

### 6.1 The Iron Catastrophe

The **iron catastrophe** is the runaway process by which metallic iron-nickel separates from silicate material to form a planetary core. Heat sources that raise internal temperature past the melting point of iron (~1,538 C / ~1,811 K):

- **Accretional energy**: kinetic energy of impacting planetesimals converts to heat
- **Short-lived radioactive isotopes**: primarily **Al-26** (half-life ~0.7 Myr) -- the main heat source for early protoplanet differentiation. Planetesimals larger than ~20 km radius accreting within 2 Myr of solar system formation receive sufficient Al-26 heating to melt.
- **Gravitational potential energy release**: once iron begins sinking, released gravitational energy further heats the interior in a positive feedback loop, making the process self-reinforcing ("catastrophic"). Core formation released ~$10^{31}$ J of gravitational energy.

### 6.2 Mechanisms of Metal-Silicate Separation

1. **Percolation**: liquid metal migrates through solid silicate matrix along grain boundaries. Timescale: ~1--10 Myr.
2. **Diapirism**: large metal-rich blobs sink through partially molten silicate.
3. **Diking**: metal-filled fractures propagate downward.
4. **Direct delivery**: large impacts deliver pre-differentiated metallic cores directly.

### 6.3 Magma Ocean and Core Segregation

Giant impacts create extensive magma oceans enabling rapid metal-silicate separation:

- Earth's magma ocean: 770--1,600 km deep after major impacts; Moon-forming impact produced up to **2,000 km** deep magma ocean
- Metal-silicate equilibration pressure: **30--40 GPa** (depth ~700 km)
- Minimum pressure >25 GPa required to explain observed Ni and Co partitioning
- Lunar magma ocean: initially 200--300 km, ~2,000 K; solidified ~80% in first ~1,000 years, complete crystallization took **150--200 Myr**

### 6.4 Magma Ocean Degassing Timeline

Volatiles dissolved in the magma ocean are released during crystallization, building the early atmosphere and hydrosphere. The degassing sequence:

- During magma ocean solidification, dissolved H$_2$O, CO$_2$, N$_2$, and noble gases exsolve
- A steam atmosphere forms rapidly during the magma ocean phase
- As the surface cools below ~500 K, water vapor condenses to form oceans
- The transition from magma ocean to solid surface with oceans takes ~1--10 Myr depending on atmospheric opacity

### 6.5 Hf-W Chronometer for Dating Core Formation

The extinct **$^{182}$Hf -> $^{182}$W** decay system (half-life = **9 Myr**) is the premier tool for dating core formation. Hf is lithophile (prefers silicates) while W is siderophile (prefers metal). When metal separates, Hf remains in the mantle while W partitions into the core. Excess $^{182}$W in the mantle timestamps the event.

### 6.6 Comparative Differentiation Timescales

| Body                          | Core Formation (after T$_0$) | Notes                                                         |
| ----------------------------- | ---------------------------- | ------------------------------------------------------------- |
| Small asteroids (e.g., Vesta) | ~2 Myr                       | Earliest differentiated bodies; Al-26 heated                  |
| Mars                          | ~7 Myr                       | Rapid differentiation; magma ocean crystallized in <5--10 Myr |
| Earth                         | ~30 Myr (bulk)               | Exponential growth $\tau$ ~10--11 Myr; 63% mass by 11 Myr     |
| Moon                          | ~60--150 Myr after T$_0$     | Formed from Earth's mantle after giant impact                 |

### 6.7 Late Veneer

After core formation was largely complete (~30 Myr), a final **~0.5--1% of Earth's mass** was added as a "late veneer" of volatile-rich material, delivering:

- Highly siderophile elements (Pt, Ir, Os) -- these would have partitioned into the core during earlier accretion but are found in chondritic ratios in the mantle
- Possibly significant water and carbon
- The late veneer is constrained to post-core-formation because HSE abundances in the mantle exceed what metal-silicate equilibrium would predict

### 6.8 Impact Erosion of Volatiles

Large impacts can strip atmospheres and volatiles, depleting the volatile inventory. This creates a tension: accretion delivers volatiles, but the same impacts can blow them away. The net volatile budget depends on impact velocity, angle, and the ratio of impactor to target mass.

---

## 7. Chondrite Classification: Planetary Building Blocks

Chondrites -- primitive, undifferentiated meteorites -- represent the raw materials from which terrestrial planets were assembled. They formed **4,566.6 +/- 1.0 Ma** ago.

### 7.1 Carbonaceous Chondrites (C Chondrites)

- <5% of chondrite falls
- Carbon content up to **3%** (graphite, carbonates, organic compounds including amino acids)
- Contain significant water and hydrated minerals
- **CI chondrites** most closely match solar photosphere composition (excluding H and He), with Mg/Si = 1.07 matching solar values
- Lack metallic iron; highly oxidized
- Formed at larger distances from the Sun

### 7.2 Ordinary Chondrites (O Chondrites)

- ~80% of all meteorites, >90% of chondrites
- Three subtypes by iron content:
  - **H chondrites**: 15--20% Fe-Ni metal by mass
  - **L chondrites**: 7--11% Fe-Ni metal by mass
  - **LL chondrites**: 3--5% Fe-Ni metal by mass
- Matrix content: 10--15% of the rock

### 7.3 Enstatite Chondrites (E Chondrites)

- ~2% of chondrite falls
- Iron predominantly in **metallic or sulfide** state (not oxides) -- formed under highly reducing conditions
- Display the **least isotopic differences** relative to Bulk Silicate Earth for O, Ti, Ca, Cr, Mo, N, and H isotopes
- Strong genetic link to Earth's composition at the ppm--ppb level

### 7.4 Compositional Consequences

- **Enstatite chondrite planet**: larger metallic core, more reduced mantle. Earth's isotopic fingerprint most closely matches E chondrites.
- **Carbonaceous chondrite planet**: volatile-rich (more water, carbon), smaller metallic core, more oxidized mantle.
- **Earth's actual composition**: reflects a mixture, with progressive oxidation during accretion. Began as reduced (enstatite-like), accreted increasingly oxidized, volatile-rich material from expanding feeding zones.

---

## 8. Internal Structure

### 8.1 Earth's Layered Structure (PREM Reference)

| Layer               | Depth range (km)      | Density (g/cm$^3$) | Temperature    | Key boundary                                |
| ------------------- | --------------------- | ------------------ | -------------- | ------------------------------------------- |
| Continental crust   | 0--30 (avg), up to 70 | ~2.7               | Surface--200 C | --                                          |
| Oceanic crust       | 0--5 to 10            | ~3.0               | Surface--200 C | --                                          |
| Upper mantle        | Moho (~7--70) to 410  | 3.2--3.4           | 200--900 C     | Mohorovicic discontinuity                   |
| Transition zone     | 410--660              | 3.7--4.0           | 900--1600 C    | 410-km olivine -> wadsleyite                |
| Lower mantle        | 660--2890             | 4.4--5.6           | 1600--~4000 C  | 660-km spinel -> perovskite+magnesiowustite |
| D'' layer           | ~2700--2890           | ~5.5--5.7          | ~3500--4000 C  | --                                          |
| Outer core (liquid) | 2890--5150            | 9.9--12.2          | 4000--5000 C   | Gutenberg discontinuity (2890 km)           |
| Inner core (solid)  | 5150--6371            | 12.6--13.0         | ~5000--6000 C  | Lehmann discontinuity (5150 km)             |

- Average Earth density: **5.515 g/cm$^3$**
- Pressure at core-mantle boundary: **~136 GPa**; at center: **~360 GPa**
- Mantle viscosity range: **$10^{21}$ to $10^{24}$ Pa-s**
- Core composition: predominantly iron-nickel alloy with ~10% light elements (S, O, Si, H)
- Core comprises **~32% of Earth's total mass** and contains **85--90% of Earth's iron**

### 8.2 Whole-Mantle vs. Layered Convection

- **Whole-mantle convection**: material circulates from surface to core-mantle boundary. Supported by seismic tomography showing subducted slabs penetrating the 660-km discontinuity.
- **Layered convection**: the 660-km phase transition acts as a barrier.
- Current consensus: predominantly **whole-mantle convection**, with the 660-km boundary causing temporary impediment. The endothermic phase transition has a Clapeyron slope of **-2 to -4 MPa/K**.

---

## 9. Convection & Geodynamics

### 9.1 Rayleigh Number (Detailed)

**Thermal Rayleigh number (bottom-heated):**

$$Ra = \frac{\rho g \alpha \Delta T d^3}{\kappa \eta}$$

| Parameter                     | Symbol     | Earth value             |
| ----------------------------- | ---------- | ----------------------- |
| Mantle density                | $\rho$     | ~3300--4000 kg/m$^3$    |
| Gravitational acceleration    | $g$        | ~10 m/s$^2$             |
| Thermal expansion coefficient | $\alpha$   | ~2 x $10^{-5}$ K$^{-1}$ |
| Temperature difference        | $\Delta T$ | ~2500 K                 |
| Layer thickness               | $d$        | 2.89 x $10^6$ m         |
| Thermal diffusivity           | $\kappa$   | ~$10^{-6}$ m$^2$/s      |
| Dynamic viscosity             | $\eta$     | ~3 x $10^{20}$ Pa-s     |

**Internally heated Rayleigh number:**

$$Ra_H = \frac{g \rho_0^2 \beta H D^5}{\eta \alpha k}$$

where $H$ = volumetric heating rate, $k$ = thermal conductivity.

**Critical Rayleigh numbers:**

- Plane layer (Rayleigh-Benard): Ra$_c$ ~ **1,708** (free-free) to **1,100** (free-rigid)
- Spherical shell: Ra$_c$ ~ **660**
- The critical Ra would be attained for a temperature difference of only 0.025 K across Earth's mantle

**Earth's mantle Ra:** estimated at **$10^6$ to $10^8$** -- roughly **10,000--100,000 times critical**, indicating vigorous, chaotic convection.

### 9.2 Nusselt-Rayleigh Scaling Laws

The Nusselt number is the ratio of total (convective + conductive) to purely conductive heat transfer:

$$Nu = a \cdot Ra^\beta$$

| Boundary condition                         | Exponent $\beta$ | Notes                           |
| ------------------------------------------ | ---------------- | ------------------------------- |
| Free-slip surfaces                         | ~1/3 (0.33)      | Classical boundary layer theory |
| Rigid surfaces                             | ~1/5 (0.20)      | Turcotte & Schubert             |
| Basally heated spherical shell (numerical) | 0.294 +/- 0.004  | Wolstencroft et al. (2009)      |
| Internally heated (converted)              | 0.337 +/- 0.009  | Wolstencroft et al. (2009)      |
| Hard turbulence regime                     | ~2/7 (0.286)     | Experimental                    |

**Practical significance:** Using $\beta$ = 0.29 instead of 1/3, an Ra of $10^9$ gives a surface heat flux **~32% lower** than classical 1/3 scaling would predict.

The convective heat flux parameterization:

$$q_{\mathrm{conv}} = a' \cdot T^{(1+\beta)} / \eta(T)^\beta$$

where $\beta$ is the tectonic cooling efficiency exponent (0 to ~0.33) and $\eta(T)$ is temperature-dependent viscosity.

### 9.3 Convective Parameters for Earth's Mantle

- Convective velocities at surface (plate speeds): **1--10 cm/yr**
- Shallow convection cycle timescale: **~50 Myr**
- Deep convection cycle timescale: **~200 Myr**
- Typical mantle stresses: **3--30 MPa**
- Strain rates: **$10^{-14}$ to $10^{-16}$ /s**
- Homologous temperature ($T/T_{\mathrm{melt}}$): **0.65--0.75** for most of the mantle
- Primary upper mantle mineral: olivine (Mg,Fe)$_2$SiO$_4$

---

## 10. Heat Sources Quantified

### 10.1 Earth's Total Internal Heat Budget

Total surface heat loss: **47 +/- 2 TW** (average flux ~91.6 mW/m$^2$).

### 10.2 Radiogenic Heat (~20 TW, ~40--50% of total)

Four isotopes dominate (>99.5% of radiogenic heat):

| Isotope    | Half-life (Gyr) | Heat release ($\mu$W/kg isotope) | Mean mantle concentration (ppb) | Global contribution |
| ---------- | --------------- | -------------------------------- | ------------------------------- | ------------------- |
| **U-238**  | 4.47            | 94.6                             | 30.8                            | ~8 TW               |
| **U-235**  | 0.704           | 569                              | 0.22                            | ~0.3 TW             |
| **Th-232** | 14.0            | 26.4                             | 124                             | ~8 TW               |
| **K-40**   | 1.25            | 29.2                             | 36.9                            | ~4 TW               |

Combined geoneutrino-constrained estimate: **~20 TW total radiogenic power**.

### 10.3 Primordial Heat (~12--30 TW, ~50--60% of total)

Residual heat from:

- Gravitational potential energy released during accretion and core-mantle differentiation
- Energy from the giant Moon-forming impact (~4.5 Ga)
- Core formation released ~$10^{31}$ J of gravitational energy

### 10.4 Tidal Heating

For a synchronously rotating satellite with eccentric orbit:

$$\dot{E}_{\mathrm{tidal}} = -\mathrm{Im}(k_2) \cdot \frac{21}{2} \cdot \frac{G M_h^2 R^5 n e^2}{a^6}$$

**Key dependences:** scales as $R^5$, $M_h^2$, $e^2$, and **$a^{-6}$** (extremely sensitive to orbital distance).

| Body                  | Surface heat flux | Total power  | Notes                         |
| --------------------- | ----------------- | ------------ | ----------------------------- |
| **Io**                | 2--3 W/m$^2$      | ~100 TW      | Most volcanically active body |
| **Europa**            | ~0.19 W/m$^2$     | ~$10^{12}$ W | Maintains subsurface ocean    |
| **Enceladus**         | up to ~16 GW      | ~5--16 GW    | Powers cryovolcanic jets      |
| **Earth** (from Moon) | negligible        | ~0.1 TW      | Minor contribution            |

---

## 11. Tectonic Regime Decision Framework

### 11.1 The Three Primary Regimes

**Stagnant lid** (default mode): single rigid immobile shell. The most common regime in the solar system (Mercury, Moon, Mars, Venus presently, most moons). Requires viscosity contrast > $10^4$ between surface and deep interior.

**Mobile lid** (plate tectonics): multiple cold surface plates move continuously. **Earth is the only body known to operate in this regime.** Requires weak, localized shear zones in the lithosphere.

**Episodic lid**: mostly stagnant with periodic catastrophic overturns. **Venus is the leading candidate** -- ~1000 craters distributed nearly randomly, consistent with single resurfacing ~500 Ma ago. Occurs at intermediate lithospheric yield stress.

Recent work (Nature Communications, 2025) distinguishes **six** quantitative regimes: mobile lid, stagnant lid, sluggish lid, plutonic-squishy lid, episodic lid, and transitional.

### 11.2 Comprehensive Controlling Parameters

The fundamental condition:

> If lithospheric strength > convective stresses --> **stagnant lid**
> If lithospheric strength < convective stresses --> **mobile lid**
> Intermediate --> **episodic**

| Factor                         | Effect on tectonics                                                                                                                                                                                                 | Quantitative constraints                                                                                                                                                                             |
| ------------------------------ | ------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- | ---------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| **Planet size/mass**           | Conflicting predictions. O'Neill & Lenardic (2007): 1.1 $M_\oplus$ reduced driving stresses, favoring stagnant lid. Valencia et al. (2007): larger planets have higher mantle velocities, favoring plate tectonics. | "The influence of size may be small to irrelevant compared to the presence of surface water." Range ~0.5--5 $M_\oplus$ is rough guideline.                                                           |
| **Surface temperature**        | At 273 K: plate tectonics possible. At 759 K: only stagnant lid.                                                                                                                                                    | Liquid water enables damage processes that weaken lithosphere.                                                                                                                                       |
| **Water content**              | **Critical enabler.** High pore fluid content lowers friction coefficient below critical value for sustained subduction.                                                                                            | Water lubricates faults, enables grain-size reduction. Wet rheology alone is not sufficient -- requires surface water for serpentinization.                                                          |
| **Mantle viscosity**           | Low reference viscosity (high Ra) favors plate tectonics.                                                                                                                                                           | Upper mantle: $10^{19}$--$10^{24}$ Pa-s. For PT, damage must reduce lithospheric shear zones to ~$10^{21}$ Pa-s. Viscosity contrast > $10^4$ between surface and interior required for stagnant lid. |
| **Initial/mantle temperature** | Hot interior with large internal heating may favor stagnant lid.                                                                                                                                                    | Initial CMB temp 6100 K --> stagnant; 8100 K --> plate tectonics eventually. Warm initial conditions reverse size-scaling predictions.                                                               |
| **Yield stress**               | Low --> mobile lid; high --> stagnant lid; intermediate --> episodic.                                                                                                                                               | Fundamental parameter in Moresi-Solomatov framework.                                                                                                                                                 |

### 11.3 Heat Transport Efficiency by Regime

| Body  | Tectonic Mode            | Heat Flow (mW/m$^2$) | Notes                     |
| ----- | ------------------------ | -------------------- | ------------------------- |
| Earth | Mobile lid               | ~87 (86--95)         | Bimodal crust             |
| Venus | Stagnant lid (episodic?) | ~31 (10--40)         | Possible past resurfacing |
| Mars  | Stagnant lid             | ~19 (14--25)         | Archetype                 |
| Moon  | Stagnant lid             | ~12--18              | Apollo 15/17 measurements |

Without plate tectonics, Earth's mantle temperature would be **700--1500 K higher** for the same surface heat flux.

---

## 12. Planetary Cooling & Geological Death

### 12.1 Surface-Area-to-Volume Scaling

$$\frac{SA}{V} = \frac{3}{R}$$

Larger planets retain heat longer. Naive cooling timescale: $t_{\mathrm{cool}} \sim R/3$. **Doubling radius roughly doubles the conductive cooling timescale.**

### 12.2 Convective Cooling Modifies the Picture

The Seales & Lenardic (2021) result: **a 5 $M_\oplus$ planet with $\beta$=0.2 can reach the same temperature as a planet an order of magnitude less massive after 10 Gyr**, because tectonic regime efficiency ($\beta$ exponent) dominates over simple SA/V scaling.

### 12.3 Initial CMB Temperature Effects on Evolution

The initial core-mantle boundary temperature has first-order effects on long-term evolution:

- CMB temp of 6100 K --> stagnant lid regime
- CMB temp of 8100 K --> plate tectonics eventually develops
- Hot initial conditions reverse predictions from mass-scaling arguments alone

### 12.4 Case Studies: Moon, Mars, Earth

| Property               | Moon                           | Mars                       | Earth              |
| ---------------------- | ------------------------------ | -------------------------- | ------------------ |
| Mass ($M_\oplus$)      | 0.012                          | 0.107                      | 1.0                |
| SA/V ratio (relative)  | 3.7x                           | 1.9x                       | 1.0x               |
| Dynamo duration        | ~1 Gyr?                        | ~0.5--0.8 Gyr              | >4.5 Gyr (ongoing) |
| Last volcanism         | ~2 Ga (bulk); ~120 Ma (minor?) | ~50--200 Ma (localized)    | Present            |
| Current tectonic mode  | Dead (stagnant lid)            | Nearly dead (stagnant lid) | Active mobile lid  |
| Current magnetic field | None                           | Crustal remnants only      | Active dipole      |

**The Moon** (0.012 $M_\oplus$):

- Dynamo ceased probably by ~3.5 Ga (possibly weakened state to ~1 Ga)
- Major volcanism (mare basalts): 3.9--3.1 Ga
- Chang'e 5 samples: basalts as young as ~2 Ga
- Some irregular mare patches possibly ~120 Ma old

**Mars** (0.107 $M_\oplus$):

- Dynamo active from formation to ~3.9 Ga
- Core mostly or entirely liquid; composition differs from Earth (more sulfur)
- Olympus Mons lava flows possibly as young as ~200 Ma; Elysium Planitia ~50 Ma
- Loss of magnetic field --> atmospheric stripping --> loss of surface water
- Not fully dead but approaching geological dormancy

**Earth** (1.0 $M_\oplus$):

- Active plate tectonics, Ra ~ $10^7$--$10^8$
- Active magnetic dynamo powered by inner core solidification + compositional convection
- Inner core began solidifying ~1--1.5 Ga (possibly as recently as ~0.5 Ga)
- Surface heat flow 47 TW; will remain active for billions of years
- Expected active until Sun enters red giant phase (~5 Gyr hence)

**Fundamental scaling:** Mars had ~11% of Earth's mass and cooled to dynamo death in ~0.5--0.8 Gyr. The Moon had ~1.2% and lost its dynamo even earlier. Earth, 10x more massive than Mars, retains its dynamo after 4.5 Gyr.

---

## 13. Volatile Inventory Factors

A planet's volatile budget depends on multiple interacting factors:

1. **Formation location**: distance from star sets baseline volatile availability via the condensation sequence
2. **Feeding zone width**: during giant impacts, embryos scatter and mix material from a range of distances. Earth sampled ~0.5--4 AU, progressively incorporating more volatile-rich material.
3. **Late veneer / late accretion**: final ~0.5--1% of mass added as volatile-rich material after core formation
4. **Impact erosion**: large impacts strip atmospheres and volatiles
5. **Magma ocean degassing**: volatiles dissolved in magma ocean released during crystallization, building early atmosphere/hydrosphere
6. **Atmospheric escape**: Jeans escape, hydrodynamic escape, and solar wind stripping remove light species over time
7. **Stellar activity**: early UV/X-ray luminosity drives enhanced atmospheric loss

Earth's silicate mantle retains **chondritic (solar) relative proportions** of refractory lithophile elements (Ca, Al, Sc, Ti, rare earths) but shows **progressive depletion of volatile elements** relative to CI chondrite reference.

---

## 14. Key Numerical Summary

| Parameter                                | Value                                  | Source                |
| ---------------------------------------- | -------------------------------------- | --------------------- |
| Solar system age                         | 4,568 +/- 0.001 Gyr                    | CAI dating            |
| Earth core mass fraction                 | 32%                                    | Physics Today         |
| Metal-silicate equilibration pressure    | 30--40 GPa                             | Physics Today         |
| Hf-182 half-life                         | 9 Myr                                  | Nature (2002)         |
| Earth core formation timing              | ~30 Myr after T$_0$                    | Physics Today         |
| Mars core formation timing               | ~7 Myr after T$_0$                     | Nature (2002)         |
| Water snow line                          | 2.7--5 AU (~150--170 K)                | Hayashi / CfA         |
| Median disk lifetime                     | ~3 Myr                                 | Haisch et al. 2001    |
| Disk dispersal timescale                 | ~$10^5$ yr                             | Alexander et al. 2014 |
| Critical core mass for gas accretion     | ~10 $M_\oplus$ (2--5 with low opacity) | Pollack et al. 1996   |
| Isolation mass at 1 AU                   | ~0.11 $M_\oplus$                       | Ormel 2024            |
| Isolation mass at 5 AU (beyond ice line) | ~5--10 $M_\oplus$                      | Ormel 2024            |
| Type I migration timescale (1 AU)        | $10^4$--$10^5$ yr                      | Tanaka et al. 2002    |
| Gap-opening mass                         | ~Saturn mass (~0.3 $M_J$)              | Nelson 2018           |
| Earth mantle Ra                          | $10^6$--$10^8$                         | Various               |
| Earth surface heat flow                  | 47 +/- 2 TW                            | Heat budget studies   |
| Radiogenic heat                          | ~20 TW                                 | Geoneutrino data      |
| Primordial heat                          | ~12--30 TW                             | Heat budget studies   |
| Magma ocean depth (Moon-forming impact)  | up to 2,000 km                         | Royal Society (2018)  |
| Lunar magma ocean solidification         | 150--200 Myr                           | Royal Society (2018)  |
| Iron melting point                       | 1,538 C (1,811 K)                      | Standard              |

---

## References

### Disk Structure & Formation

- [Hayashi 1981 -- MMSN](https://ui.adsabs.harvard.edu/abs/1981PThPS..70...35H)
- [Weidenschilling 1977](https://ui.adsabs.harvard.edu/abs/1977Ap%26SS..51..153W)
- [A&A 2015 -- Disk structure review](https://www.aanda.org/articles/aa/full_html/2015/03/aa24964-14/aa24964-14.html)
- [Dullemond -- Les Houches 2013 lectures](https://www.ita.uni-heidelberg.de/~dullemond/lectures/leshouches2013.pdf)
- [Birnstiel et al. 2010 -- Gas and dust evolution](https://www.aanda.org/articles/aa/full_html/2010/05/aa13731-09/aa13731-09.html)
- [Alexander et al. 2014/2017 -- Disk dispersal](https://pmc.ncbi.nlm.nih.gov/articles/PMC5414277/)
- [Haisch et al. 2001 -- Disk lifetimes](https://ui.adsabs.harvard.edu/abs/2001ApJ...553L.153H)

### Accretion & Planet Formation

- [Safronov 1969 -- Evolution of the Protoplanetary Cloud](https://ui.adsabs.harvard.edu/abs/1969epc..book.....S)
- [Ormel 2024 -- Planet Formation Mechanisms](https://arxiv.org/html/2410.14430v1)
- [Kokubo & Ida -- Dynamics and accretion](https://academic.oup.com/ptep/article/2012/1/01A308/1570529)
- [Pollack et al. 1996 -- Giant planet formation](https://ui.adsabs.harvard.edu/abs/1996Icar..124...62P)
- [Terrestrial planet formation (PNAS, 2011)](https://pmc.ncbi.nlm.nih.gov/articles/PMC3228478/)

### Gravitational Instability

- [Toomre 1964 -- Stability criterion](https://en.wikipedia.org/wiki/Toomre%27s_stability_criterion)
- [Gammie 2001 -- Disk fragmentation](https://ui.adsabs.harvard.edu/abs/2001ApJ...553..174G)
- [Rice, Lodato & Armitage 2005 -- Critical cooling time](https://academic.oup.com/mnras/article/410/1/559/1036146)
- [Boss 1997 -- Giant Planet Formation by GI](https://www.science.org/doi/10.1126/science.276.5320.1836)

### Migration

- [Tanaka, Takeuchi & Ward 2002 -- Type I](https://ui.adsabs.harvard.edu/abs/2002ApJ...565.1257T)
- [Paardekooper et al. 2010 -- Non-isothermal torques](https://academic.oup.com/mnras/article/401/3/1950/1097210)
- [Nelson 2018 -- Migration in Protoplanetary Disks](https://arxiv.org/pdf/1804.10578)
- [Chambers 2009 -- Planetary Migration review](https://www.eoas.ubc.ca/~mjelline/453website/eosc453/E_prints/newfer010/chambers_planetarymigration_AR09.pdf)
- [Nice model -- Wikipedia](https://en.wikipedia.org/wiki/Nice_model)

### Frost Lines & Composition

- [Frost line -- Wikipedia](<https://en.wikipedia.org/wiki/Frost_line_(astrophysics)>)
- [Morbidelli et al. 2016 -- Fossilized condensation lines](https://www.sciencedirect.com/science/article/pii/S0019103515005448)
- [Pontoppidan et al. -- Volatiles in protoplanetary disks (PPVI)](https://www2.mpia-hd.mpg.de/homes/ppvi/chapter/pontoppidan.pdf)
- [Helled et al. 2024 -- Fuzzy cores](https://agupubs.onlinelibrary.wiley.com/doi/full/10.1029/2024AV001171)

### Differentiation & Core Formation

- [Hf-W chronometry (Nature, 2002)](https://www.nature.com/articles/nature00995)
- [Rapid accretion and early core formation (Nature, 2002)](https://www.nature.com/articles/nature00982)
- [The formation and differentiation of Earth (Physics Today)](https://physicstoday.aip.org/features/the-formation-and-differentiation-of-earth)
- [The early differentiation of Mars (E&PSL, 2017)](https://www.sciencedirect.com/science/article/abs/pii/S0012821X17303710)
- [Magma oceans as a critical stage (Royal Society, 2018)](https://pmc.ncbi.nlm.nih.gov/articles/PMC6189560/)
- [Iron catastrophe -- Wikipedia](https://en.wikipedia.org/wiki/Iron_catastrophe)
- [Planetary differentiation -- Wikipedia](https://en.wikipedia.org/wiki/Planetary_differentiation)
- [Chondrite -- Wikipedia](https://en.wikipedia.org/wiki/Chondrite)
- [Enstatite chondrites (Nature Scientific Reports, 2020)](https://www.nature.com/articles/s41598-020-57635-1)

### Internal Structure & Geodynamics

- [Internal structure of Earth -- Wikipedia](https://en.wikipedia.org/wiki/Internal_structure_of_Earth)
- [PREM (Dziewonski & Anderson, 1981)](https://www.researchgate.net/figure/Density-profile-of-the-Earth-according-to-the-PREM-model-19-Different-colors_fig4_323944696)
- [Inside the Earth -- USGS](https://pubs.usgs.gov/gip/dynamic/inside.html)
- [Mantle convection -- Wikipedia](https://en.wikipedia.org/wiki/Mantle_convection)

### Convection & Heat Transfer

- [Rayleigh number -- Wikipedia](https://en.wikipedia.org/wiki/Rayleigh_number)
- [Nusselt-Rayleigh scaling (Wolstencroft et al., 2009)](https://ui.adsabs.harvard.edu/abs/2009PEPI..176..132W/abstract)
- [Scaling Laws in Rayleigh-Benard Convection (AGU, 2019)](https://agupubs.onlinelibrary.wiley.com/doi/full/10.1029/2019EA000583)
- [A Note on Planet Size and Cooling Rate (Seales & Lenardic, 2021)](https://ar5iv.labs.arxiv.org/html/2102.01077)

### Tectonic Regimes

- [Lid tectonics -- Wikipedia](https://en.wikipedia.org/wiki/Lid_tectonics)
- [Plate tectonics -- Wikipedia](https://en.wikipedia.org/wiki/Plate_tectonics)
- [Geodynamics of terrestrial exoplanets -- Wikipedia](https://en.wikipedia.org/wiki/Geodynamics_of_terrestrial_exoplanets)
- [Geodynamics of Venus -- Wikipedia](https://en.wikipedia.org/wiki/Geodynamics_of_Venus)
- [Dissecting tectonic lid regimes (Nature Communications, 2025)](https://www.nature.com/articles/s41467-025-65943-1)
- [Stagnant lid tectonics (Stern et al., 2018)](https://www.sciencedirect.com/science/article/pii/S1674987117301135)
- [The dependence of planetary tectonics on mantle thermal state (Royal Society, 2018)](https://royalsocietypublishing.org/doi/10.1098/rsta.2017.0409)
- [O'Neill et al. 2007](https://www.sciencedirect.com/science/article/abs/pii/S0012821X07003457)
- [Noack & Breuer 2014](https://www.sciencedirect.com/science/article/abs/pii/S003206331300161X)

### Heat Sources

- [Earth's internal heat budget -- Wikipedia](https://en.wikipedia.org/wiki/Earth%27s_internal_heat_budget)
- [What Keeps the Earth Cooking? -- Berkeley Lab (2011)](https://newscenter.lbl.gov/2011/07/17/kamland-geoneutrinos/)
- [Tidal heating -- Wikipedia](https://en.wikipedia.org/wiki/Tidal_heating)

### Planetary Cooling & Death

- [Planetary cooling: SA/V ratio (University of Victoria)](https://web.uvic.ca/~jwillis/teaching/astr201/maths.5.planetary_cooling.pdf)
- [Two-billion-year-old volcanism on the Moon (Nature, 2021)](https://www.nature.com/articles/s41586-021-04100-2)
- [Revisiting Mars dynamo timeline (Harvard Gazette, 2023)](https://news.harvard.edu/gazette/story/2023/07/revisiting-timeline-that-pinpoints-when-mars-lost-its-dynamo/)
- [Planetary size and cooling rate (Phys.org, 2024)](https://phys.org/news/2024-01-planetary-size-cooling-mars-died.html)
