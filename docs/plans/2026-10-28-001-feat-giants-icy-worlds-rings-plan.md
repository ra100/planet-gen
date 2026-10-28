---
title: Giant planets, icy worlds and rings
type: feat
status: completed
date: 2026-10-28
origin: R in the PlanetGen project thread (2026-10-28): implement the missing planet types (gas giants, cold gas giants, icy planets) and fix the rings, starting from the field's experts
---

# Giant planets, icy worlds and rings

Requirements: [docs/brainstorms/2026-10-28-giants-icy-worlds-rings-requirements.md](../brainstorms/2026-10-28-giants-icy-worlds-rings-requirements.md)
Research: [docs/research/giant-planets-icy-worlds-rings.md](../research/giant-planets-icy-worlds-rings.md)

## Problem

Every planet rendered as a terrain, whatever its mass; mass stopped at
10 M⊕. Icy moons were grey lunar regolith. Rings were only drawn off the
planet's disk, with a distorted radius and a tilt of their own.

## Approach

### Physics (`src/planet.rs`)

- **Envelope** [0, 1]: AUTO `smoothstep(3, 5, M)` (Rogers 2015; Chen &
  Kipping's Terran-Neptunian break at 2 M⊕), override in the planet file.
  Exactly 0 below 3 M⊕, so every rocky value is computed as before.
- Radius log-blends to Chen & Kipping's Neptunian (M^0.59) and Jovian
  (M^-0.04) branches joined by a smooth minimum; gravity is GM/R² at 1 bar.
- **T_eff** = (T_eq⁴ + T_int⁴)^¼ with Bond albedo 0.34 and
  T_int ≈ 100 K (M/M_J)^0.3; the reference temperature moves to the 1 bar
  level (1.3 T_eff). Scale height RT/μg with μ 2.3 (gas) to 2.6 (ice).
- **Ice giant** fraction from mass (30-90 M⊕).
- The envelope covers the surface: no oceans, ice, craters, regolith or
  dust; pressure becomes the 1 bar reference unless overridden; clouds of
  the weather layer are off (`scene::cloud_coverage`).
- The preview sky over a giant is a thin limb haze (τ_blue ~0.05): the
  deck colours are calibrated to whole-disk albedos that already contain
  the gas above the deck.
- **Ice crust** [0, 1]: AUTO `smoothstep(1.2, 1.8, d / frost line)` on
  bodies without an envelope; zero for SNOWBALL (2.7 AU). Ice crusts carry
  no oxidized dust.
- `PlanetType::GasGiant` / `IceGiant` once the envelope passes 0.5.

### Cloud deck (`src/shaders/giant.wgsl`, preview and export albedo)

- Band phase N F(x)^1.5 with F the integral of √cos φ (jets widen poleward
  as β shrinks) and N from the Rhines scale normalised to Jupiter,
  N = 5.5 √(Ω R / U) with U ∝ T_eff^-1.5 (`GiantUniforms::from_derived`):
  Jupiter 5.5, Saturn ~4, Uranus and Neptune ~1.7.
- Zonal wind: alternating jets plus a prograde equatorial jet for deep
  (gas) giants; a retrograde equator and prograde high-latitude jets for
  shallow (ice) giants.
- Turbulence: fBm with a two-level domain warp, sheared by the wind,
  stretched 3.2x along longitude, strongest in belts and on jet flanks,
  muted by haze. Octaves follow the pixel footprint.
- Vortices: a great spot (red anticyclone in the first southern tropical
  zone; on ice giants a dark spot), white ovals in zones and brown barges
  in belts; each swirls the band lookup. NEB hot spots on Jupiter-likes.
- Polar regions: Jupiter's 8 + 5 cyclone clusters; Saturn's hexagon near
  76° N; ice giants' methane-depleted bright poles and summer hood.
- Palette by T_eff (Sudarsky classes I-V) and the ice-giant methane regime;
  haze from the scale height mutes and yellows Saturn-likes.
- Photometry: Jupiter's two-term Henyey-Greenstein × μ₀ blended to
  Saturn's Dones et al. law by haze. Hot decks glow above ~900 K.

### Rings (`preview_cubemap.wgsl`)

- Plane = the planet's equator: the tilted pole carried through the view
  rotation. Drawn over the body when in front, hidden behind it.
- Optical depth: Saturn's C / B / Cassini Division / A template over the
  inner-outer span with seeded boundaries, ringlets and four narrow gaps,
  scaled by **Ring density** (the old `ring_opacity`, 1 = Saturn).
- Single-scattering thin-layer reflection on the lit face and diffuse
  transmission on the unlit face, Lambert-sphere particle phase, opposition
  surge and a multiple-scattering gain; dense rings reddish, sparse ones
  grey. Planet shadow on the rings and ring shadow on the planet.

### Icy crusts (`rocky.wgsl`)

- Ice albedo darkens with crater age (clean 0.7 to lagged 0.2). Young
  crusts: great-circle lineae and triple bands, chaos (the mare provinces)
  stained red-brown, more on the trailing side, ridged-plains texture. Old
  crusts: resurfaced provinces become bright grooved terrain with domain-
  wise groove directions. Fresh craters excavate clean ice.
- Photometry: lunar-Lambert moved 40 % toward Lambert for bright ice.

### Controls and presets

PLANET: Mass to 4000 M⊕, **Envelope** (AUTO). TERRAIN > surface history:
**Ice crust** (AUTO). LOOK > rings: Inner, Outer, **Ring density**.
Presets JUPITER, SATURN, URANUS, NEPTUNE, HOT JUPITER, EUROPA, GANYMEDE,
CALLISTO.

## Validation

See the PR for the before/after table: `planet_capture` geometric albedo
(zero-phase disk mean of I/F) against Mallama et al. (2017), radii and
gravity against the real giants, and Earth's preview pixels and exported
maps against main.
