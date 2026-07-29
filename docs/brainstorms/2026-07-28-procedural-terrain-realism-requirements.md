---
date: 2026-07-28
topic: procedural-terrain-realism
---

# Procedural Terrain Realism Requirements

## Summary

Replace the abandoned Terrain Diffusion direction with deterministic, GPU-only procedural terrain that produces geologically legible whole-planet structure, climate-conditioned drainage, and preserved fine relief. The work prioritizes balanced macro, meso, and residual detail, with geological structure winning when performance requires a tradeoff.

---

## Problem Frame

The current Terrain Diffusion exploration adds runtime and asset complexity without meeting the product's deterministic GPU terrain direction. Existing terrain must become more coherent across scales and cube faces: plate behavior should visibly explain major landforms, precipitation should influence erosion, and generated maps must remain continuous and comparable between preview and export.

---

## Requirements

**Product direction and cleanup**
- R1. The terrain generator shall use deterministic GPU procedural generation only; no ML runtime shall participate in terrain generation, preview, or export.
- R2. The Terrain Diffusion runtime, application integration, and spike code/configuration shall be removed as part of this work.
- R3. Ignored downloaded Terrain Diffusion assets may be deleted only after their exact deletion scope is explicitly confirmed; negative research manifests and evidence explaining the abandonment shall be retained.
- R4. Useful quality metrics discovered during Terrain Diffusion exploration shall be generalized for procedural-terrain validation without retaining ML-specific runtime requirements.

**Terrain hierarchy and geological behavior**
- R5. Terrain shall be produced in a coarse macro layer, a meso layer for tectonic relief and hydrology, and a high-frequency residual layer.
- R6. Erosion shall retain high-frequency residual detail rather than globally smoothing it away.
- R7. Plate-boundary type and stress shall visibly drive convergent orogens, divergent rifts, and transform landforms.
- R8. Climate precipitation shall condition erosion and drainage behavior.
- R9. The terrain hierarchy shall target balanced macro, meso, and residual detail; if the performance budget forces a choice, geological structure has priority.

**Continuity, parity, and determinism**
- R10. Terrain, drainage, normals, ambient occlusion, roughness, and export outputs shall be continuous across spherical cube-face boundaries.
- R11. Drainage shall not create artificial river terminations at cube-face edges.
- R12. Preview and export shall use equivalent terrain parameters and produce materially equivalent height and water results.
- R13. A repeated generation with the same seed on the same build and adapter shall reproduce exactly.
- R14. Cross-adapter output variation shall be measured against a documented tolerance rather than assumed to be bit-exact.

**Performance and delivery scope**
- R15. Regeneration at the 768-face preview target shall complete within one second.
- R16. An 8K export shall complete within three minutes; a 30-second 8K export remains an explicitly documented aspiration, not a rejection budget.
- R17. Minimal corrections within the balanced scope shall be the first milestone.
- R18. An ambitious equal-area grid or global terrain rewrite shall be deferred.

---

## Acceptance Examples

- AE1. **Covers R5, R6, R9.** Given a fixed seed, when erosion is enabled, the output retains fine-scale residual relief while adding coherent tectonic and hydrologic meso structure instead of becoming uniformly diffused.
- AE2. **Covers R7, R8.** Given otherwise comparable plate and climate inputs, when boundary type/stress or precipitation changes, convergent, divergent, transform, and wetter-versus-drier erosion regions exhibit visibly corresponding terrain changes.
- AE3. **Covers R10, R11.** Given a generated planet, when every cube-face boundary is inspected, terrain-derived maps join continuously and no river ends solely because it reaches an artificial face edge.
- AE4. **Covers R12, R13, R14.** Given the same parameters and seed, when preview and export run on one build/adapter, repeat results are exact and preview/export metrics meet parity gates; when another adapter is tested, its measured deviation is within the documented tolerance.
- AE5. **Covers R2, R3, R4.** Given the cleanup milestone, when Terrain Diffusion references are removed, no runtime or application dependency remains, retained negative manifests still describe the rejected path, and ignored asset deletion is limited to the explicitly approved target set.

---

## Success Criteria

- Normal seams meet p95 <= 5 degrees and maximum <= 15 degrees.
- Generated drainage has zero artificial river terminations at cube-face edges.
- Preview/export height correlation is >= 0.995 and water-mask IoU is >= 0.98.
- Resolution doubling changes spectral energy by no more than +/-10%.
- Convergent settings produce measurable relief enrichment relative to the relevant control.
- At least 90% of fine-band energy remains after erosion.
- Pole distribution satisfies the defined polar sampling/distribution gate.
- The 768-face preview and 8K export meet their hard rejection budgets.
- Same-build/same-adapter repeats are exact; cross-adapter tolerance is measured, recorded, and satisfied.

---

## Scope Boundaries

**In scope**
- Deterministic GPU procedural terrain realism, erosion/hydrology conditioning, continuity, parity, determinism measurement, quality gates, and the required Terrain Diffusion code/config cleanup.
- Retaining negative research evidence and generalized quality metrics from the abandoned exploration.

**Deferred**
- Equal-area grid conversion and a whole-system global terrain rewrite beyond the minimal-corrections milestone.
- The aspirational 30-second 8K export target unless it can be achieved without weakening the hard quality or geological-priority requirements.

**Out of scope**
- Whole-planet 90 m terrain generation.
- ML terrain generation or inference, including Python, ONNX, TorchScript, model distribution, and hidden experiments.
- Deleting ignored multi-GB downloads without an explicit approved deletion scope.

---

## Key Decisions

- Terrain Diffusion is abandoned: remove its runtime/application/spike integration, retain negative research manifests, and preserve portable quality lessons.
- Deterministic GPU procedural generation is the sole terrain direction.
- The quality target is balanced scale hierarchy, with geological structure preferred over decorative residual detail when constrained by performance.
- The hard performance limits are one second for 768-face preview regeneration and three minutes for 8K export; 30 seconds is aspirational.
- Minimal corrections are the first milestone; a global/equal-area rewrite is intentionally deferred.

---

## Dependencies / Assumptions

- The existing Rust/wgpu preview and export path remains the product baseline; planning must verify its current continuity and measurement hooks before changing it.
- Existing negative-research artifacts, including `docs/research/terrain-diffusion-native-spike-manifest.md` and `docs/research/terrain-diffusion-libtorch-spike-manifest.md`, are retained as evidence of the abandoned direction.
- Cross-adapter tolerance requires representative adapter coverage and a stable metric harness; the tolerance value will be established by measurement during planning and validation.
- Explicit approval is required before removing ignored downloaded assets, including any multi-GB data outside tracked source/configuration files.
