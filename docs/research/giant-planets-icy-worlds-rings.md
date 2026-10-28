# Giant planets, icy worlds and rings

Research for phase 19 (gas giants, ice giants, icy worlds, ring systems).
It collects who works on each problem, what the literature says that a
renderer can use, and how PlanetGen turns it into parameters. Numbers are
quoted from the sources listed at the end; where a number is our own fit it
says so.

## Who to read

| Area | People | Why they matter here |
|------|--------|----------------------|
| Giant-planet jets and bands | Peter Read, Roland Young (Oxford); Yohai Kaspi (Weizmann); Tapio Schneider (Caltech); the late Adam Showman (Arizona); Geoffrey Vallis, Wanying Kang (MIT) | Rhines-scale jet spacing, deep vs shallow jets, why Uranus/Neptune have retrograde equators |
| Giant-planet clouds and colour | Leigh Fletcher (Leicester); Patrick Irwin (Oxford); Agustín Sánchez-Lavega, Ricardo Hueso (UPV/EHU); Amy Simon (NASA GSFC, OPAL); Imke de Pater (Berkeley); Larry Sromovsky (Wisconsin); Heidi Hammel (AURA) | Belt/zone chemistry, chromophores, Saturn's haze and hexagon, ice-giant methane and haze colour |
| Exoplanet giants | David Sudarsky, Adam Burrows (Princeton); Mark Marley (Arizona); Jonathan Fortney (UCSC); Tristan Guillot (OCA); Jingjing Chen, David Kipping (Columbia) | Albedo classes by temperature, internal heat, the mass-radius relation |
| Planet photometry | Anthony Mallama; Erich Karkoschka (Arizona); Ulyana Dyudina (Caltech); Luke Dones (SwRI) | Disk-integrated albedos in B, V, R; limb-darkening laws for Jupiter and Saturn |
| Rings | Jeff Cuzzi (NASA Ames); Philip Nicholson (Cornell); Matt Hedman (Idaho); Luke Dones; Larry Esposito (Colorado); Heikki Salo (Oulu); Kelly Miller, Gianrico Filacchione, Mauro Ciarniello (composition) | Radial optical-depth structure, particle sizes, colour, thin-layer photometry |
| Icy worlds | Paul Schenk (LPI); Robert Pappalardo (JPL); Francis Nimmo (UCSC); Britney Schmidt (Cornell); Geoffrey Collins (Wheaton); William McKinnon (WashU); Bonnie Buratti (JPL); Ines Belgacem, Frédéric Schmidt (Paris-Saclay) | Crater morphology on ice, lineae and chaos on Europa, grooved terrain on Ganymede, icy photometry |
| Procedural rendering | Robert Bridson (curl noise); Inigo Quilez (domain warping); SpaceEngine's gas-giant generator; the classic 1D colour-ramp gas-giant shader | How real-time renderers fake bands and turbulence; what to keep and what to replace with physics |

## Giant planets

### Mass, radius and gravity

Chen & Kipping (2017) fit four regimes to measured planets. Rocky "Terran"
worlds follow R ∝ M^0.28 up to ~2 M⊕. "Neptunian" worlds follow R ∝ M^0.59
up to ~130 M⊕ (0.41 M_J). "Jovian" worlds barely grow, R ∝ M^-0.04,
because degeneracy pressure takes over. A 2023 revision (Müller et al.)
puts the break at 124 M⊕ with R ∝ M^0.55 below and M^0.01 above. Rogers
(2015) found most planets above ~1.6 R⊕ (~4–6 M⊕) are not rocky.

PlanetGen keeps its rocky R ∝ M^0.27 and blends towards a giant radius
with the envelope fraction:

- R_nep = 1.21 R⊕ (M / 2 M⊕)^0.589 and R_jov = 11.2 R⊕ (M / 318 M⊕)^-0.04,
  joined by a smooth minimum (p-norm, p = 4; our fit). This gives Uranus
  3.9 R⊕ (real 4.0), Neptune 4.3 (3.9), Saturn 9.9 (9.45) and Jupiter 11.1
  (11.2).
- Gravity at the 1 bar level is g = 9.81 M / R² (Jupiter 25, Saturn 9.5,
  Uranus 9.4, Neptune 9.1 m/s²; real 24.8, 10.4, 8.7, 11.2).

### Temperature and internal heat

Giants radiate more than they absorb. T_eff⁴ = T_eq⁴ + T_int⁴, with
T_int ≈ 99 K for Jupiter, 77 K Saturn, 53 K Neptune and < 40 K Uranus
(Guillot 2005; Pearl & Conrath 1991). A power law T_int ≈ 100 K (M/M_J)^0.3
reproduces these within ~10 K (our fit). The 1 bar temperature is about
1.3 T_eff (165 K Jupiter, 134 K Saturn, 76 K Uranus, 72 K Neptune).

### What sets the colour: cloud decks by temperature

Sudarsky, Burrows & Pinto (2000) classify giants by which condensate forms
the visible deck:

| Class | T_eff | Visible deck | Bond albedo | Look |
|-------|-------|--------------|-------------|------|
| I Jovian | < 150 K | NH₃ ice over NH₄SH and H₂O | 0.38–0.63 | cream zones, brown belts |
| II water cloud | ~250 K | H₂O ice | 0.56–0.88 | brightest, near white |
| III clear | > 350 K | none above ~50 bar | 0.01–0.2 | dark; Rayleigh blue with alkali absorption |
| IV roaster | 900–1500 K | silicate, below Na/K absorption | < 0.04 | darkest objects known |
| V hot roaster | > 1500 K | high silicate cloud | 0.51–0.57 | bright again, alkali lines |

HD 189733b, a class III/IV planet, was measured deep azure (Evans et al.
2013). Below ~80 K methane condenses and the NH₃ deck sinks out of view; the
ice giants show a methane-ice and H₂S deck under a photochemical haze.

### Measured colour targets (Mallama, Krobusek & Pavlov 2017)

Geometric albedos in Johnson-Cousins bands:

| Planet | B | V | R | R/B |
|--------|---|---|---|-----|
| Jupiter | 0.443 | 0.538 | 0.495 | 1.12 |
| Saturn | 0.339 | 0.499 | 0.568 | 1.68 |
| Uranus | 0.561 | 0.488 | 0.202 | 0.36 |
| Neptune | 0.562 | 0.442 | 0.181 | 0.32 |

Irwin et al. (2024) reprocessed Voyager and Hubble data: Uranus and Neptune
are both a similar pale greenish blue; Neptune is only slightly bluer
because its haze layer is thinner. Uranus is greener at solstice because
its polar regions hold about half the methane and a sunlit polar "hood" of
methane-ice haze brightens green and red.

### Belts, zones and jets

- Bands are ~10⁴ km wide and closely follow latitude; >90 % of Jupiter's
  cloud-top kinetic energy is in the zonal jets (Read et al. 2020).
- Zones are anticyclonic upwelling regions with bright, fresh NH₃ ice;
  belts are cyclonic downwelling regions where the deck thins and red-brown
  chromophores and deeper clouds show (Ingersoll 2004; Fletcher et al.
  2020). Lightning and convective plumes concentrate in belts.
- Jet spacing follows the Rhines scale L_Rh = π (2U / β)^½ with
  β = 2Ω cos φ / R (Rhines 1975). Faster spin and a larger radius give more,
  narrower jets; stronger eddies (larger U) fewer, wider ones. Rhines scales
  are 7,300 km (Jupiter), 9,900 (Saturn), 16,700 (Uranus), 12,100 km
  (Neptune) (Zeng, Kang, Flierl & Vallis 2025).
- Equatorial jet direction depends on how deep the jets reach relative to
  the equatorial Rhines scale: deep jets (Jupiter ~2,400 km, Saturn
  ~8,700 km measured by Juno and Cassini gravity, Kaspi et al. 2018, Galanti
  et al. 2019) give prograde equatorial superrotation; shallow jets
  (Uranus, Neptune < 1,100 km) give a retrograde equator and a single
  prograde jet per hemisphere (Zeng et al. 2025).
- Because β shrinks poleward, jets widen poleward: the band phase grows as
  ∫ √cos φ dφ. Above ~60–65° Jupiter's bands give way to turbulent polar
  regions with arrays of cyclones (8 around the north pole, 5 around the
  south; Adriani et al. 2018). Saturn keeps a jet to ~76° N that meanders as
  a wavenumber-6 hexagon (Godfrey 1988; Sánchez-Lavega et al.).
- Vortices range from a few hundred km to ~10⁴ km. Long-lived compact ones
  are anticyclones sitting in anticyclonic shear (the zones' flanks): the
  Great Red Spot at 22° S, white ovals. Cyclones are less compact and
  circular (Read et al. 2020). Neptune's Great Dark Spot was an anticyclone
  with bright orographic methane clouds on its edge.
- Saturn's banding is muted beneath a tenuous upper tropospheric haze
  (Read et al. 2020): lower gravity and lower temperature give a haze scale
  height twice Jupiter's.
- Jupiter's North Equatorial Belt edge carries ~10 regularly spaced dark
  "hot spots" with bright plumes trailing between them (a Rossby wave).

### Photometry

Giant clouds are not Lambertian. Dones et al. (1993), used by Dyudina et
al. (2005), fit Saturn with I/F = (A/μ) (μ μ₀ / (μ + μ₀))^B with B ≈ 1.48 at
zero phase, 1.42 at 90° and 1.34 at 150° (red). Normalised to the disk
centre at zero phase, this darkens the limb far more than Lambert. Jupiter
fits a two-term Henyey-Greenstein phase (g₁ = 0.8, g₂ = -0.38, f = 0.9).
The same paper shows ring brightness saturating above normal optical depth
~3–5.

### Procedural practice

The standard real-time gas giant looks up a 1D latitude colour ramp and
displaces the lookup with fBm, ridged noise and thresholded storm noise,
animating by sliding the noise (moddb "Procedural gas giant rendering with
GPU noise"). It gives bands and turbulence but no physical scale: band
count, contrast, vortex placement and colour are all hand tuned. Curl
noise (Bridson et al. 2007) gives divergence-free swirls; domain warping
(Quilez) folds bands into filaments.

**What PlanetGen does instead**: the same latitude lookup, but the number
of bands, their poleward widening, the jet direction, the vortex latitudes
and the palette are derived from the physics above. Turbulence is sheared
along the jets, strongest on jet flanks and in belts; vortices swirl the
lookup so band material spirals around them.

## Rings

### Structure (Saturn, Cassini)

Miller et al. (2024, composition review with Cuzzi, Nicholson, Hedman,
Filacchione and others), 1 R_S = 60,268 km:

| Region | Radius (R_S) | Normal optical depth τ | Notes |
|--------|--------------|------------------------|-------|
| D | < 1.13 | ≪ 0.01 | dust, mostly invisible |
| C | 1.24–1.53 | 0.05–0.1, plateaux ~0.5 | darker, greyer, more silicate |
| B | 1.53–1.95 | 0.9 inner to > 5 core | brightest, reddish |
| Cassini Division | 1.96–2.03 | 0.1–0.2 | dark, like the C ring |
| A | 2.03–2.27 | ~0.5 | Encke and Keeler gaps, density waves |
| F | 2.32 | narrow | shepherded |

- Particles follow n(a) ∝ a^-q with q ≈ 2.7–3.2 from mm to ~10 m.
- The rings are ≥ 95 % water ice. The A and B rings are reddish from a UV
  absorber (tholin-like organics); the C ring and Cassini Division hold 5–10
  times more neutral dark absorber and look darker and greyer.
- They are ~10 m thick: in a renderer, a plane.
- Main rings sit inside the Roche limit (~2.4 planet radii for ice) in the
  planet's equatorial plane, which the planet's oblateness (J₂) holds them
  in. Uranus' rings are narrow and dark (albedo ~0.05); Jupiter's are faint
  dust; Neptune's are arcs.

### Shading a thin ring

Single scattering in a thin layer of optical depth τ (Chandrasekhar;
Cuzzi et al. 1984): on the lit face

    I/F = ϖ₀ P(α) / 4 · μ₀ / (μ + μ₀) · (1 − exp(−τ (1/μ + 1/μ₀)))

and seen from the unlit face, through the layer,

    I/F = ϖ₀ P(α) / 4 · μ₀ / (μ₀ − μ) · (exp(−τ/μ₀) − exp(−τ/μ)).

Ring particles are large icy bodies with a backscattering, Callisto-like
phase function and a strong opposition surge. Transmission along the view
is exp(−τ/μ). The planet's shadow falls on the rings and the rings' shadow
on the planet attenuates sunlight by exp(−τ/μ₀) at the point where the
sun ray crosses the ring plane.

### What was wrong in PlanetGen

1. Rings were only composited where the view ray missed the planet and its
   atmosphere, so the near side of the rings never crossed the planet disk.
2. The ring radius ignored the y component of the hit point, which
   distorted the ellipse.
3. The ring plane had its own tilt instead of the planet's equator.
4. The opacity profile was a linear ramp with two fixed gaps, lit with an
   ad-hoc 0.3 ambient; no ring shadow on the planet.

## Icy worlds

- Galilean satellite geometric albedos: Europa ~0.67, Ganymede ~0.43,
  Callisto ~0.20; Enceladus ~1.4 (Bond ~0.8, the brightest surface known),
  Triton ~0.76, Pluto ~0.5 with tholin-red regions.
- Europa's surface is young (~40–90 Myr from crater counts; Zahnle et al.
  2003): few craters, global lineae and double ridges, bands, and chaos
  terrain (~25 % of the surface; Schmidt et al. 2011). Lineae and chaos are
  reddish brown from hydrated salts and radiolytic sulphur, concentrated on
  the trailing hemisphere. Europa's Hapke single-scattering albedo is
  ~0.93 (leading 0.95, trailing 0.89) with ~16° macroscopic roughness and
  an opposition amplitude B₀ ~0.5 (Belgacem et al. 2020).
- Ganymede: ~1/3 dark, old, cratered terrain and ~2/3 bright grooved terrain
  in swaths of parallel ridges; bright polar frost caps above ~40°.
- Callisto: saturated with craters, dark lag deposits on crests, bright
  fresh craters.
- On ice, crater relief relaxes viscously, so large craters are shallow,
  have central pits instead of central peaks above ~35 km (Ganymede) and
  turn into palimpsests (Schenk 2002). Europa's craters change form at
  ~8 and ~30 km, which constrains its ice shell to > 19 km (Schenk 2002).

## How PlanetGen uses it

- **Envelope fraction** (AUTO from mass, rising from 3 to 8 M⊕ after
  Rogers 2015, with a slider): blends radius and gravity to the giant
  relation and swaps the surface for a cloud deck. Zero below 3 M⊕, so every
  rocky world including Earth stays bit-identical.
- **Ice-giant fraction** from mass (Uranus/Neptune below ~30 M⊕, Saturn
  above ~90 M⊕): shallow jets, retrograde equator, methane colours.
- **Band count** from the Rhines scale, normalised to Jupiter (our fit):
  N = 6.5 √((Ω R)/(Ω_J R_J) / (U/U_J)), with eddy speed rising as the planet
  gets colder, U/U_J = (T_eff / 124 K)^-1.5 clamped to [0.5, 3]. Jupiter 6.5
  jets per hemisphere, Saturn 4.7, Uranus and Neptune 1.7.
- **Palette** from T_eff through the Sudarsky classes and the ice-giant
  methane regime, calibrated to the Mallama albedos; haze from the scale
  height relative to Jupiter mutes Saturn-like bands.
- **Rings**: in the equatorial plane, composited in front of and behind
  the planet, an optical-depth profile with C/B/Cassini/A-like structure,
  single-scattering thin-layer shading, planet shadow on the rings and ring
  shadow on the planet.
- **Icy worlds**: the rocky stagnant-lid model with an ice crust beyond the
  frost line: bright ice palette, relaxed craters, Europa lineae and chaos,
  Ganymede grooved terrain.

## Sources

- Chen & Kipping 2017, "Probabilistic forecasting of the masses and radii of
  other worlds", ApJ 834, 17 ([arXiv:1603.08614](https://arxiv.org/abs/1603.08614));
  Müller et al. 2024, "The mass-radius relation of exoplanets, revisited",
  A&A ([arXiv:2311.12593](https://arxiv.org/abs/2311.12593)).
- Rogers 2015, "Most 1.6 Earth-radius planets are not rocky", ApJ 801, 41.
- Guillot 2005, "The interiors of giant planets", Annu. Rev. Earth Planet.
  Sci. 33.
- Sudarsky, Burrows & Pinto 2000, "Albedo and reflection spectra of
  extrasolar giant planets", ApJ 538, 885
  ([arXiv:astro-ph/9910504](https://arxiv.org/abs/astro-ph/9910504)).
- Evans et al. 2013, "The deep blue color of HD 189733b", ApJL 772, L16.
- Mallama, Krobusek & Pavlov 2017, "Comprehensive wide-band magnitudes and
  albedos for the planets", Icarus 282
  ([arXiv:1609.05048](https://arxiv.org/abs/1609.05048)).
- Irwin et al. 2024, "Modelling the seasonal cycle of Uranus's colour and
  magnitude, and comparison with Neptune", MNRAS 527
  ([Oxford news](https://www.ox.ac.uk/news/2024-01-05-new-images-reveal-what-neptune-and-uranus-really-look-1)).
- Read, Young & Kennedy 2020, "The turbulent dynamics of Jupiter's and
  Saturn's weather layers: order out of chaos?", Geosci. Lett.
  ([arXiv:2004.02815](https://arxiv.org/abs/2004.02815)).
- Zeng, Kang, Flierl & Vallis 2025, "Influence of penetration depth on jets
  on giant planets" ([arXiv:2503.17828](https://arxiv.org/abs/2503.17828)).
- Rhines 1975, "Waves and turbulence on a beta-plane", JFM 69.
- Kaspi et al. 2018, Nature 555; Galanti et al. 2019, GRL 46 (jet depths).
- Adriani et al. 2018, "Clusters of cyclones encircling Jupiter's poles",
  Nature 555.
- Fletcher et al. 2020, "How well do we understand the belt/zone
  circulation of giant planet atmospheres?", Space Sci. Rev. 216.
- Ingersoll et al. 2004, "Dynamics of Jupiter's atmosphere", in *Jupiter:
  The Planet, Satellites and Magnetosphere*.
- Dyudina et al. 2005, "Phase light curves for extrasolar Jupiters and
  Saturns", ApJ 618 ([arXiv:astro-ph/0406390](https://arxiv.org/abs/astro-ph/0406390));
  Dones, Cuzzi & Showalter 1993, Icarus 105.
- Miller et al. 2024, "The composition of Saturn's rings", Space Sci. Rev.
  ([arXiv:2311.17344](https://arxiv.org/abs/2311.17344)).
- Cuzzi et al. 1984, "Saturn's rings: properties and processes", in
  *Planetary Rings*; Chandrasekhar 1960, *Radiative Transfer*.
- Belgacem, Schmidt & Jonniaux 2020, "Regional study of Europa's
  photometry", Icarus ([arXiv:2007.11445](https://arxiv.org/abs/2007.11445)).
- Schenk 2002, "Thickness constraints on the icy shells of the Galilean
  satellites from a comparison of crater shapes", Nature 417.
- Zahnle et al. 2003, "Cratering rates in the outer Solar System", Icarus
  163; Schmidt et al. 2011, "Active formation of chaos terrain over shallow
  subsurface water on Europa", Nature 479.
- Bridson, Hourihan & Nordenstam 2007, "Curl-noise for procedural fluid
  flow", SIGGRAPH; "Procedural gas giant rendering with GPU noise",
  [moddb](https://www.moddb.com/news/procedural-gas-giant-rendering-with-gpu-noise).
