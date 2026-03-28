# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

## Project Overview

Planet Gen is a GPU-accelerated procedural planet generator producing physically plausible planets with biomes, atmospheres, oceans, and up to 32K textures. Currently in **research/pre-implementation phase** — no source code yet, only comprehensive research documents.

## Repository Structure

- `docs/research/final.md` — Complete research reference (~9,200 lines) covering planetary science, GPU architecture, tiling, atmospheric rendering, biome mapping, and lessons from existing engines
- `docs/research/researcher-a.md` — Planet formation and accretion research
- `docs/research/researcher-b.md` — GPU noise and procedural generation techniques

## Architecture (from research)

The system is designed around three operational modes:

1. **Preview Mode**: 256×256 noise per cube face for instant parameter feedback
2. **Generation Mode**: Async compute producing 24,576 tiles (6 faces × 64×64 tiles, 512² each) for full 32K output
3. **Runtime Mode**: Virtual texturing with quadtree LOD, atmospheric scattering, FFT ocean

### Core Pipeline

User parameters (star distance, mass, metallicity, tilt, rotation) → terrain generation (multi-octave fBm, 8-12 octaves) → hydraulic erosion (GPU compute, 20-50 iterations) → biome assignment (Whittaker/Köppen lookup) → atmosphere (Bruneton 2017 precomputed scattering) → ocean (Tessendorf FFT) → compressed output (BC7/BC5/BC4 in KTX2 format).

### Sphere Representation

Cube-sphere with quadtree LOD using 6 faces, compatible with GPU cubemap sampling.

### Key Physical Models

- Frost line determines planet type (rocky vs gas giant)
- MMSN surface density model: Σ(r) = 1700(r/AU)^(-3/2) g/cm²
- Rayleigh number determines tectonic regime
- 7×9 Whittaker table maps (temperature, precipitation) → biome
- Crater distribution: N(>D) ∝ D^(-2) via Poisson disk sampling

### Output Maps (per 32K planet, ~9.3 GB compressed)

- Albedo (BC7): 3.0 GB
- Height (R16): 4.0 GB
- Normal (BC5): 1.5 GB
- Roughness (BC4): 0.75 GB

## Technology Decisions Pending

- Graphics API: Vulkan vs WebGPU
- Language: C++, Rust, or TypeScript
- Build system: not yet chosen
- No tests, CI/CD, or package management yet
