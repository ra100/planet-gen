---
date: 2026-10-28
topic: giants-icy-worlds-rings
origin: R in the PlanetGen project thread (2026-10-28)
---

# Giant planets, icy worlds and rings

## Problem

PlanetGen only makes rocky planets. Mass stops at 10 M⊕ in the UI (20 in
validation) and every planet, whatever its mass or distance, renders as a
terrain with oceans, biomes and Earth clouds. There are no gas giants, no
ice giants, and icy moons render as grey lunar regolith. The ring system
is broken: it is only drawn where the view ray misses the planet, so the
near arc never crosses the disk; its radius ignores one axis of the hit
point; its plane has its own tilt rather than the planet's equator; and its
opacity is a linear ramp with two fixed gaps.

R asked to implement what is missing (gas giants, cold gas giants, icy
planets) and fix the rings, starting from what the field's experts know.

## How the field approaches it

See [docs/research/giant-planets-icy-worlds-rings.md](../research/giant-planets-icy-worlds-rings.md).

- Giant-planet scientists read a disk as cloud decks set by temperature
  (Sudarsky classes), banded by zonal jets whose spacing follows the
  Rhines scale, with vortices living in anticyclonic shear and a polar
  regime of its own. Ice giants differ in their shallow jets, retrograde
  equators and methane-absorbed colour.
- Ring scientists describe rings by their radial optical depth, particle
  albedo and colour, and shade them with thin-layer radiative transfer.
- Icy-satellite geologists date and read ice crusts the way the Moon is
  read, but on ice: albedo darkens with age, craters relax, tectonics
  (lineae, chaos, grooves) replace volcanism.

## Requirements

- R1. Mass up to ~4000 M⊕ (12.6 M_J). An envelope fraction, AUTO from mass
  (3-5 M⊕) with an override, blends radius, gravity, temperature and scale
  height to a giant's and swaps the terrain for a cloud deck.
- R2. Gas giants: Jupiter- and Saturn-like bands derived from rotation,
  radius and temperature; vortices; polar cyclones or a hexagon; colours
  close to measured albedos.
- R3. Ice giants: Uranus- and Neptune-like greenish-blue, low-contrast
  decks with bright methane clouds, a dark spot, brighter poles.
- R4. Hot giants: temperature classes from water clouds to clear blue to
  dark roasters and bright silicate decks, with thermal glow.
- R5. Rings: equatorial, drawn in front of and behind the planet, Saturn-
  like optical-depth structure, physically shaded, shadows both ways.
- R6. Icy worlds: dry, airless worlds beyond the frost line show ice crusts
  whose look follows the crater age: Europa, Ganymede, Callisto.
- R7. Presets for Jupiter, Saturn, Uranus, Neptune, a hot Jupiter, Europa,
  Ganymede, Callisto.
- R8. Earth and every existing preset stay unchanged (pixel and export
  bytes), except RINGED, whose rings are the fix.
- R9. Export albedo follows the preview for giants and icy worlds.
