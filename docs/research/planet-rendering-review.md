# Planet Rendering Research: Critic Review & Comparison

## Phase 1: Individual Reviews

### Document A: Coder 4 / Opus — `planet-rendering-research.md`

**Completeness: 9/10** — Covers all 10 requested topics thoroughly. Includes bonus appendices (checklist, shader snippets). Slight gap: no mention of Pearl-Bracey effective transmittance or opposition surge on planet surfaces.

**Technical Accuracy: 9/10** — Equations are correct and well-presented. Rayleigh cross-section correctly omits the depolarization factor (King correction) with a note about ozone handling. Mie extinction/scattering ratio (0.9) is correct. The Draine phase function attribution to NVIDIA SIGGRAPH 2023 is reasonable. Minor: Rayleigh scale height stated as 8 km in some places, 8.5 km would be more precise (the standard value varies between sources).

**Implementation Usefulness: 9/10** — Excellent for a graphics programmer. GLSL snippets are production-quality (not pseudocode). The Hillaire transmittance LUT parameterization code is directly usable. The appendix checklist provides a clear implementation roadmap.

**Depth: 9/10** — Goes well beyond surface-level. Detailed treatment of multiple scattering approximations, density profiles (including Bruneton's struct), cloud multi-scattering octaves, and ring rendering with full GLSL implementations.

**Code/Pseudocode: 9/10** — Extensive GLSL code: phase functions, density sampling, cloud density (Schneider-style), ocean specular, eclipse shadows, ring shadows, and a complete single-scattering fragment shader. All compilable quality.

**References: 9/10** — 17 well-organized references with DOIs and URLs. Includes NVIDIA SIGGRAPH 2023, Schneider's Horizon Zero Dawn talks, Bruneton's evaluation paper. All appear real and relevant.

**Organization: 9/10** — Excellent structure with numbered sections, subsections, tables, and two appendices. Table of contents with anchors. Easy to navigate.

**Clarity: 9/10** — Writing is precise and well-suited to a technical audience. Good use of bold for key terms. Consistent notation throughout.

**Total: 9.0/10**

---

### Document B: Coder 6 / GLM-5-Turbo — `planet-rendering-research-glm5t.md`

**Completeness: 7/10** — Covers all 10 topics but several are noticeably thinner. Gas giant section lacks GLM-5-Turbo code examples. Cloud rendering section has no actual shader code. Ring rendering is descriptive only.

**Technical Accuracy: 7/10** — Mostly correct but with some issues. Rayleigh cross-section correctly includes the depolarization factor (King correction), which is a plus over Opus. However, the limb path length formula "√(2πRh)" is wrong — should be "√(2Rh)". The Pearl-Bracey effective transmittance formula is a nice unique inclusion. The Hillaire powder effect formula is presented oddly (mixing notation). Some LUT resolution numbers differ from standard Hillaire values without explanation.

**Implementation Usefulness: 6/10** — Has pseudocode for ray marching and discusses LUT strategies, but lacks actual usable shader code. The parameter tables are useful for quick reference. A programmer would need to go to the references to write actual code.

**Depth: 6/10** — Covers the basics well but doesn't go deep on any topic. Cloud rendering describes approaches but doesn't show implementation. Gas giant chemistry table is useful but brief.

**Code/Pseudocode: 5/10** — Only one pseudocode block (ray marching algorithm). No actual shader code. This is the biggest weakness relative to Opus.

**References: 7/10** — Good references including Riley & McGuire 2018 (missing from Opus) and Wrenninge's volume rendering course. Some URLs are generic/placeholder (Shadertoy search URL, "various examples"). Bouthors 2008 is a good cloud reference Opus lacks.

**Organization: 7/10** — Clean structure matching the 10-topic outline. No bonus appendices. Tables are well-used.

**Clarity: 7/10** — Writing is clear but occasionally imprecise. The rendering equation presentation in §1.1 is a mix of surface and volume terms that could confuse readers.

**Total: 6.5/10**

---

## Phase 2: Comparative Analysis

### Topic-by-Topic

| Topic | Winner | Notes |
|-------|--------|-------|
| 1. Math Foundation | **Opus** | Far more detailed phase functions (Draine, Cornette-Shanks with full derivation). Bruneton density profile struct. Better volume rendering integral presentation. |
| 2. Atmospheric Scattering | **Opus** | More precise coefficients, GLM-5-Turbo optical depth formula is unwieldy vs. Opus's clean ray marching. Opus has GLSL code. GLM-5-Turbo correctly includes King correction (depolarization factor). |
| 3. Layers & Composition | **Opus** | Better planet parameter tables. Mars blue sunset explanation is excellent. GLM-5-Turbo has slightly better stratospheric aerosol profile (Gaussian). |
| 4. Aerosols | **Opus** | More detailed single-scattering albedo table. Better haze level quantification. GLM-5-Turbo's aerosol type table is cleaner. |
| 5. Cloud Rendering | **Opus** | Dramatically better — full Schneider-style GLSL density sampling, multi-scattering octave code, powder effect. GLM-5-Turbo has Pearl-Bracey formula (unique, useful) and silver lining discussion. |
| 6. Surface Rendering | **Opus** | Full ocean GLM-5-Turbo shader, city lights code, Hapke model mention. GLM-5-Turbo has cleaner biome albedo table. |
| 7. Limb Effects | **Opus** | Better Chapman function discussion, full limb rendering GLM-5-Turbo. GLM-5-Turbo's path length formula has an error (√(2πRh) should be √(2Rh)). |
| 8. Light Transport | **Opus** | Eclipse shadow GLM-5-Turbo code is a standout. GLM-5-Turbo has cleaner binary star formula. |
| 9. Implementation | **Opus** | Much more detailed LUT documentation (Hillaire parameterization code, GPU tips). GLM-5-Turbo has a cleaner LUT summary table. |
| 10. Exotic Planets | **Opus** | Thermal emission GLM-5-Turbo code, ring shadow code, ring self-scattering code. GLM-5-Turbo has good methane absorption table for ice giants. |

### Unique Content in Each

**Opus-only (not in GLM-5-Turbo):**
- Draine phase function with invertible sampling
- Complete single-scattering GLSL fragment shader (Appendix B)
- Implementation checklist (Appendix A)
- HG importance sampling GLM-5-Turbo code
- Eclipse shadow computation code
- Ring shadow + ring rendering GLM-5-Turbo code
- Mars blue sunset explanation
- Open-source implementations table (detailed)
- Virtual Terrain Project reference

**GLM-5-Turbo-only (not in Opus):**
- Rayleigh cross-section with King correction (depolarization factor)
- Pearl-Bracey effective transmittance for thick clouds
- Powder effect formula (Hillaire ground-atmosphere bouncing)
- Hillaire 2015 Frostbite reference (early multi-scattering clouds)
- Bouthors 2008 interactive cloud scattering reference
- Riley & McGuire 2018 GDC talk reference
- Wrenninge 2015 volume rendering reference
- Methane absorption table for Neptune/Uranus
- Stratospheric aerosol Gaussian profile
- Opposition surge / Seeliger effect mention
- Ocean albedo variation specifics
- Atmospheric night glow mention

### Errors Found

**Opus:**
- Minor: Rayleigh β_R values use 8 km scale height in some places; 8.5 km is more standard
- The Opus single-scattering shader uses `dot(ro, rd)` for `b` instead of `2*dot(ro, rd)` — this is actually correct for unit-length direction vectors but inconsistent with the `a=dot(dir,dir)` general form used elsewhere

**GLM-5-Turbo:**
- **Error:** Limb path length formula "√(2πRh)" should be "√(2Rh)" — the π should not be there
- **Error:** Rayleigh scattering coefficient values in the table (e.g., 1.185×10⁻⁵ at 440 nm) differ from both Opus and standard Bruneton values — these may be from a different reference but are presented without source
- **Inconsistency:** Cloud multi-scattering "octave" approach attributed to Hillaire 2016 but the paper is from 2020
- The rendering equation in §1.1 mixes surface and volume terms in a confusing way

### Overall Winner

**Opus (Coder 4) wins decisively: 9.0 vs 6.5.**

Opus provides a document that a graphics programmer could genuinely implement from. The GLSL code is production-quality, the mathematical derivations are rigorous, and the implementation roadmap is clear. GLM-5-Turbo's document reads more like a survey/literature review — good for understanding the landscape but insufficient for implementation.

GLM-5-Turbo's strengths (King correction, Pearl-Bracey, some unique references, methane absorption table) should be merged into the final document.
