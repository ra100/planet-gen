# Deep Review: Research vs Implementation

**Date:** 2026-03-30
**Scope:** All 33 research documents vs current codebase (15 .rs files, 13 .wgsl shaders)

---

## Executive Summary

The project implements **~60-65%** of the research corpus. Core planetary science, terrain noise, climate modeling, and biome classification are well-implemented. Major gaps exist in atmospheric rendering, ocean simulation, erosion sophistication, and advanced material systems. The implementation makes pragmatic tradeoffs — favoring visual plausibility over physical accuracy — which is appropriate for a VFX asset generator.

---

## 1. TERRAIN GENERATION

### ✅ Implemented from Research
| Research Recommendation | Implementation | Fidelity |
|---|---|---|
| Simplex noise for 3D | `noise.wgsl` — Ashima Arts simplex (42.0× scale) | High |
| fBm with configurable octaves | 8-12 octaves, gain 0.5, lacunarity 2.0 | High |
| Cube-sphere mapping (no pole pinch) | `cube_sphere.wgsl` — 6 face transforms | High |
| Bimodal elevation (Earth-like) | Continental +0.3, oceanic -0.4 | High |
| Domain warping for irregular coastlines | `plates.wgsl` — warped Voronoi boundaries | High |
| Hypsometric profile transformation | Power curve (exponent 1.4) in `plates.wgsl` | Medium |
| Seed-based deterministic generation | Wang hash seed → all noise functions | High |

### ⚠️ Partially Implemented
| Research Recommendation | Current State | Gap |
|---|---|---|
| **Ridged multifractal** for mountains | Ridge mask with `snoise` + `smooth_step` for gaps, but not true ridged multifractal (Hurst exponent, offset, gain feedback) | Missing weight feedback loop that creates sharp ridges and erosion-like features |
| **Diamond-Square** as alternative | Not used — fBm only | Acceptable; fBm is superior for GPU |
| **Erosion** (hydraulic + thermal) | D8 flow accumulation + stream power, but research recommends **particle-based hydraulic** for realism | Current approach is grid-based approximation; no sediment particles, no meandering |

### ❌ Not Implemented
| Research Recommendation | Impact | Priority |
|---|---|---|
| **Thermal erosion** (talus angle) | Deliberately removed per brainstorm — was destroying detail | By design |
| **Particle-based hydraulic erosion** | Would produce realistic river channels, deltas, alluvial fans | Low (v1.1+ deferred) |
| **Wang tiling** for anti-repetition | Would eliminate visible noise repetition at planet scale | Medium |
| **Worley/Voronoi noise** for rivers | Would create natural drainage networks | Medium |

### Assessment
Terrain generation is **strong**. The tectonic plate system (Voronoi + boundary types) is a significant implementation beyond what most research documents suggest as "procedural approximation." The main gap is erosion sophistication — the current D8 approach is functional but produces grid-aligned artifacts vs the research-recommended particle system.

---

## 2. TECTONIC PLATES

### ✅ Implemented from Research
| Research Recommendation | Implementation | Fidelity |
|---|---|---|
| Voronoi-based plate assignment | Fibonacci sphere → Voronoi in compute shader | High |
| Continental vs oceanic classification | Based on ocean fraction parameter | High |
| 4 boundary types | Convergent, divergent, transform, subduction | High |
| Mountain ranges at convergent boundaries | Continental collision → elevated ridges | High |
| Volcanic chains at subduction zones | Oceanic-continental convergence → volcanic arcs | High |
| Mid-ocean ridges at divergent boundaries | Underwater ridge generation | High |
| Plate count from planet mass | Small ~4-6, Earth-like ~8-14, large ~15-20 | High |

### ⚠️ Partially Implemented
| Research Recommendation | Current State | Gap |
|---|---|---|
| **Euler pole rotation** | Plates have velocity vectors but no Euler pole kinematics | Velocities are simplified, not great-circle rotations |
| **Island arcs** at oceanic-oceanic convergence | Mentioned in brainstorm, unclear if fully in shader | May be simplified |
| **Hotspot volcanism** | Present in `plates.wgsl` | Implementation exists but unclear how prominent |

### ❌ Not Implemented
| Research Recommendation | Impact | Priority |
|---|---|---|
| **Convection-driven plate emergence** | Research suggests plates should emerge from mantle convection simulation | Very Low (overkill for asset gen) |
| **Subduction zone geometry** (Wadati-Benioff) | Slab angle, depth-dependent volcanism | Low |
| **Plate boundary evolution over time** | Static snapshot vs dynamic history | Low |

### Assessment
Tectonic implementation is **excellent** — arguably the project's strongest alignment with research. The Voronoi approach with boundary classification produces geologically plausible results. Not implementing full convection simulation is the right call for a VFX tool.

---

## 3. CLIMATE & ATMOSPHERE

### ✅ Implemented from Research
| Research Recommendation | Implementation | Fidelity |
|---|---|---|
| Hadley cell 3-cell circulation | ITCZ wet, subtropical dry (~30°), polar front | High |
| Latitude-based temperature | 30°C equator → -20°C poles (cosine law) | High |
| Altitude lapse rate | -6.5°C/km (matches research exactly) | High |
| Rain shadow effect | Wind-terrain interaction on leeward slopes | High |
| Continentality (inland drying) | Moisture decreases from coast | Medium |
| Axial tilt → seasonal migration | Sub-solar latitude shifts Hadley cells | High |
| Rayleigh number → tectonic regime | Continuous factor from convective vigor | High |
| Escape velocity → atmosphere retention | Physics-derived atmosphere strength | High |
| Greenhouse feedback | CO₂ accumulation extends habitable zone | High |

### ⚠️ Partially Implemented
| Research Recommendation | Current State | Gap |
|---|---|---|
| **Köppen classification** | Whittaker lookup (temp × moisture) is used, but not full Köppen scheme with seasonal criteria | Missing: seasonal precipitation patterns, monsoons, Cwb vs Cfb distinction |
| **Wind direction model** | Trade winds (0-30°), westerlies (30-60°), polar easterlies — but simplified | No Coriolis-driven spiral patterns, no jet streams |
| **Orographic precipitation** | Rain shadow exists but is simplified | Research suggests more nuanced uplift + condensation model |

### ❌ Not Implemented
| Research Recommendation | Impact | Priority |
|---|---|---|
| **Rayleigh scattering** (atmospheric rendering) | No atmospheric haze, sky color, limb darkening from scattering | High — would dramatically improve visual quality |
| **Mie scattering** (aerosols) | No dust/haze layer | Medium |
| **Bruneton/Hillaire atmospheric model** | Research strongly recommends precomputed LUTs | High for visual quality |
| **Beer-Lambert transmittance** | No optical depth calculation for atmosphere | Medium |
| **Cloud rendering** (ray marching) | No clouds at all | Medium |
| **Ocean currents** | No warm/cold current effects on regional climate | Low |
| **Monsoon systems** | No seasonal wind reversal | Low |

### Assessment
Climate modeling is **good for biome assignment** but **weak for visual rendering**. The Hadley cell model drives realistic desert/wet zone placement, which is the primary goal. However, the atmospheric rendering gap is the project's **biggest missed opportunity** — research documents extensively cover Bruneton/Hillaire methods that would add atmospheric haze, limb effects, and sky color for dramatically improved planet views.

---

## 4. BIOME & SURFACE CLASSIFICATION

### ✅ Implemented from Research
| Research Recommendation | Implementation | Fidelity |
|---|---|---|
| Whittaker biome diagram | 13 discrete biomes from temp × moisture | High |
| Altitude zonation | Forest → alpine → rock → snow transitions | High |
| Desert band placement | ~30° latitude subtropical deserts | High |
| Polar ice/tundra | Temperature-driven ice coverage | High |

### ⚠️ Partially Implemented
| Research Recommendation | Current State | Gap |
|---|---|---|
| **Gradient biome coloring** | Brainstormed and partially implemented (continuous gradient system exists alongside discrete), but biome boundaries still somewhat visible in export maps | Transition zones could be smoother |
| **Holdridge life zones** | Not used; Whittaker is simpler | Holdridge adds PET ratio for more nuanced classification |

### Assessment
Biome classification is **solid**. The Whittaker approach is appropriate for the target resolution and use case. The gradient coloring work addresses the main visual limitation of discrete boundaries.

---

## 5. SURFACE MATERIALS & PBR

### ✅ Implemented from Research
| Research Recommendation | Implementation | Fidelity |
|---|---|---|
| PBR roughness by biome | `roughness_map.wgsl`: ice 0.15, desert 0.85, forest 0.50 | High |
| Cook-Torrance specular | GGX microfacet + Schlick Fresnel in `preview_cubemap.wgsl` | High |
| Height-derived normals | Central difference (0.0015 step) in `normal_map.wgsl` | High |
| Albedo by biome | Color lookup per biome type in `albedo_map.wgsl` | High |

### Comparison: Research Roughness Values vs Implementation

| Surface | Research | Implementation | Match? |
|---|---|---|---|
| Calm water | 0.0-0.05 | 0.05 (ocean) | ✅ |
| Ice | 0.05-0.15 | 0.15 | ✅ |
| Snow | 0.3-0.5 | ~0.3 (implied) | ✅ |
| Grassland | 0.4-0.6 | 0.50 | ✅ |
| Desert sand | 0.8-1.0 | 0.85 | ✅ |
| Exposed rock | 0.6-0.9 | ~0.7 (implied) | ✅ |
| Forest | 0.4-0.6 | 0.50 | ✅ |

### ⚠️ Partially Implemented
| Research Recommendation | Current State | Gap |
|---|---|---|
| **ORM texture packing** | Separate roughness/normal/albedo maps exported | Could pack AO+Roughness+Metallic into single texture |
| **Slope-dependent roughness** | Biome-based only | Research suggests `mix(roughness, 0.85, smoothstep(0.4, 0.8, slope))` |
| **Moisture-dependent roughness** | Not implemented | Research suggests `roughness *= mix(1.0, 0.7, moisture)` |

### ❌ Not Implemented
| Research Recommendation | Impact | Priority |
|---|---|---|
| **Metallic map** | No metallic variation (wet surfaces, ice, mineral deposits) | Low |
| **Ambient occlusion** | No AO generation | Medium |
| **Subsurface scattering** for vegetation | Would improve forest/grass realism | Low |
| **Emission map** (city lights, volcanic glow) | Deferred to v1.1 | Low |

### Assessment
PBR implementation is **well-aligned** with research. Roughness values match closely. The main gap is slope/moisture-dependent roughness modulation and AO generation, both relatively easy additions.

---

## 6. OCEAN RENDERING

### ✅ Implemented
| Feature | Implementation |
|---|---|
| Ocean mask generation | Height threshold → binary mask |
| Flat ocean normals | Geometric normals for water (not terrain-derived) |
| Ocean specular | Fresnel-based reflection in PBR shader |
| Polar ocean ice | Temperature-based ice rendering on ocean surfaces |
| Ocean roughness | Uniformly smooth (0.05) |

### ❌ Not Implemented (Research Has Extensive Coverage)
| Research Recommendation | Impact | Priority |
|---|---|---|
| **FFT ocean waves** (Tessendorf) | Research: "0.5ms for 256² on modern GPU" — animated waves | Medium (for preview) |
| **Phillips spectrum** | Wave height distribution from wind speed | Medium |
| **Subsurface scattering** in water | Translucent water color at shallow depths | Low |
| **Foam generation** (Jacobian-based) | White caps, shore foam | Low |
| **Depth-based color gradient** | Shallow turquoise → deep navy (in gradient brainstorm) | Medium |

### Assessment
Ocean rendering is **minimal**. The research documents have extensive wave simulation coverage (FFT, Phillips spectrum, foam) that would add significant visual quality. For a still-image asset generator, static ocean is acceptable, but the depth-based color gradient from the brainstorm would be a quick win.

---

## 7. IMPACT CRATERING

### ✅ Implemented
| Feature | Implementation |
|---|---|
| Crater stamp morphology | d/D ≈ 0.2 depth ratio, ejecta ±1 radius |
| Basic crater placement | Stamp-based placement on terrain |

### ❌ Not Implemented
| Research Recommendation | Impact | Priority |
|---|---|---|
| **Pi-scaling laws** (size-dependent morphology) | Simple → central peak → peak-ring → multi-ring transitions | Medium |
| **Poisson disk distribution** | Even spacing without clustering (v1.1 planned) | Medium |
| **Size-frequency distribution** | Power law crater counts (many small, few large) | Medium |
| **Complex crater features** | Central peaks, terraced walls, flat floors | Low |

### Assessment
Cratering is **basic but functional**. The research on pi-scaling morphology transitions (simple→complex→multi-ring with increasing size) would add significant realism for heavily cratered bodies.

---

## 8. GPU ARCHITECTURE & PERFORMANCE

### ✅ Implemented from Research
| Research Recommendation | Implementation | Fidelity |
|---|---|---|
| Compute shaders for terrain | wgpu compute pipeline | High |
| Cube-sphere representation | 6-face cubemap with seamless edges | High |
| Tiled generation for high resolution | 16×16 tiles @ 512px per face | High |
| Device persistence | wgpu device singleton pattern | High |
| Background threading | UI-responsive generation with progress | High |

### ⚠️ Not Implemented
| Research Recommendation | Impact | Priority |
|---|---|---|
| **Quadtree LOD** per cube face | Fixed resolution; no adaptive detail | Low (export is tiled anyway) |
| **Temporal reprojection** | No frame-to-frame coherence optimization | Low (not real-time) |
| **Shared memory** in compute shaders | No workgroup memory optimization | Low |
| **LUT-based atmospheric computation** | No precomputed transmittance tables | Medium (if atmosphere added) |

### Assessment
GPU architecture is **well-designed** for the use case. The tiled generation approach is a practical solution that avoids the complexity of quadtree LOD while achieving 8K output. Performance target (<30s for 8K) is met.

---

## 9. ALBEDO / COLOR VALUES

### Comparison: Research vs Implementation

| Surface | Research Albedo | Implementation Color (approx) | Match? |
|---|---|---|---|
| Ocean | 0.06 (very dark) | Deep blue (~0.08, 0.24, 0.42) | ✅ Appropriate |
| Fresh snow | 0.80-0.90 | Near white (~0.95, 0.95, 0.95) | ✅ |
| Desert sand | 0.30-0.40 | Tan/orange tones | ✅ |
| Grassland | 0.20-0.25 | Green tones | ✅ |
| Coniferous forest | 0.10-0.15 | Dark green | ✅ |
| Deciduous forest | 0.15-0.20 | Medium green | ✅ |
| Bare rock | 0.10-0.30 | Grey tones | ✅ |

### Assessment
Color values are **well-calibrated** against research albedo data. The visual palette serves the VFX use case well.

---

## 10. OVERALL GAP ANALYSIS

### Tier 1: High-Impact Gaps (Would Most Improve the Product)

1. **Atmospheric Scattering** — Research covers Bruneton/Hillaire extensively. Adding even simplified Rayleigh scattering would add limb darkening, atmospheric haze, and sky-colored horizon that make planet renders immediately more convincing. This is the single biggest visual quality gap.

2. **Cloud Layer** — Even a simple procedural cloud texture (noise-based, latitude-varying density) would dramatically improve planet realism. Research covers ray-marching and Beer-Lambert approaches.

3. **Ocean Depth Gradient** — Shallow→deep color transition. Already brainstormed. Quick implementation, high visual payoff.

### Tier 2: Medium-Impact Gaps

4. **Slope/Moisture-Dependent Roughness** — Research provides exact formulas; 5-line shader change.

5. **Ridged Multifractal Noise** — Would improve mountain realism with sharper ridges. Currently using basic ridge masking.

6. **Size-Dependent Crater Morphology** — Pi-scaling laws for simple→complex→multi-ring transitions.

7. **Ambient Occlusion Map** — Would enhance Blender renders significantly.

### Tier 3: Low-Priority Gaps (Diminishing Returns)

8. Particle-based erosion (expensive, current approach adequate)
9. Euler pole plate kinematics (visual difference minimal)
10. Full Köppen seasonal classification (Whittaker sufficient)
11. FFT ocean waves (static asset generator)
12. Emission maps (niche use case)

---

## 11. AREAS WHERE IMPLEMENTATION EXCEEDS RESEARCH

The project does some things **better** than what the research documents suggest:

1. **Tectonic plate system** — More sophisticated than the "procedural approximation" most research recommends. The Voronoi + boundary classification + terrain derivation pipeline is well-engineered.

2. **Seasonal Hadley cell migration** — The seasonal thermal equator shift with axial tilt is a nuance not covered in most research docs.

3. **Hypsometric profile transformation** — The power curve (1.4 exponent) for realistic elevation distribution is a nice touch.

4. **Height scale / relief exaggeration** — Artistic control not mentioned in research but essential for VFX work.

5. **Continuous gradient biome coloring** — Goes beyond the discrete Whittaker lookup the research describes.

---

## 12. RECOMMENDATIONS

### Quick Wins (< 1 day each)
- [ ] Add slope-dependent roughness modulation (5 lines in `roughness_map.wgsl`)
- [ ] Add ocean depth color gradient (10 lines in `albedo_map.wgsl`)
- [ ] Add AO approximation from terrain curvature

### Medium Effort (1-3 days each)
- [ ] Simplified atmospheric scattering (single-scatter Rayleigh, no LUTs)
- [ ] Procedural cloud noise layer (latitude-varying, exportable as separate map)
- [ ] Size-dependent crater morphology transitions

### Major Features (1+ weeks)
- [ ] Full Bruneton atmospheric model with precomputed LUTs
- [ ] Particle-based erosion with river channels
- [ ] FFT ocean wave simulation for animated preview

---

*This review covers the full research corpus (33 documents) against the current implementation state. The project demonstrates strong research-to-implementation translation for its core systems, with atmospheric rendering being the primary area where the extensive research has not yet been leveraged.*
