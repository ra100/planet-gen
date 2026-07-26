# Terrain Diffusion Evaluation Manifest

## Harness provenance

- Harness: `src/bin/terrain_diffusion_eval.rs`
- Control: `earthlike-v1` procedural terrain, seed 42
- Canonical artifact contract: six LE-f32 cubemap faces in the order defined by `terrain-diffusion-integration.md`
- Capture sampling: evaluator-only `2*N+1` source generation with odd `(2*x+1,2*y+1)` extraction to canonical texel centers
- Determinism authority: exact artifact bytes; displayed identity: FNV-1a-64

## Recorded 2026-07-26 local evidence

- Commands ran as separate processes beneath `target/be001-r3-centered/`.
- `capture-control --resolution 512 --dir control-a`: PASS, FNV `243e1887675e77a8`.
- `capture-control --resolution 512 --dir control-b`: PASS, FNV `243e1887675e77a8`.
- `compare-bytes --first control-a --second control-b`: PASS; exact bytes match.
- `validate-control --resolution 512 --dir control-a`: exit 3. Size, finite, orientation fixture, control range, height edge, and height corner passed. Normal and pole-elevation gates failed; pole-slope passed.
- `preview-control --resolution 512 --dir control-a`: exit 3 before GPU rendering; validation/render/PNG gates are all FAIL and no PNG is claimed.

### Frozen local metrics

- Normal p95: `14.004798889°`; normal max: `39.932952881°`.
- North elevation cap/ring p10/p50/p90: `0.511487070/0.615200698/0.722269360` vs `0.329641500/0.548291500/0.661965200`.
- South elevation cap/ring p10/p50/p90: `0.092189920/0.129674673/0.214496780` vs `0.104251740/0.156128180/0.305730000`.
- North slope cap/ring p10/p50/p90: `0.010620171/0.010849974/0.011072072` vs `0.009152237/0.009770864/0.010404518`.
- South slope cap/ring p10/p50/p90: `0.010567857/0.010785690/0.011010768` vs `0.009118327/0.009725587/0.010353151`.

## Evidence gates

| Gate | Status | Evidence |
|---|---|---|
| Procedural fresh-process repeatability | PASS | Two 512² centered captures exactly match at FNV `243e1887675e77a8`. |
| Local artifact format and CPU validation | FAIL | Frozen normal and pole-elevation gates fail with the metrics recorded above. |
| Isolated preview PNG | FAIL | Preview correctly stops at mandatory validation; render and PNG are not run. |
| External model/checkpoint | NOT RUN | No checkpoint downloaded or executed. |
| Candidate artifact | NOT RUN | No candidate asset supplied. |
| Projection | NOT RUN | No planar-to-cubemap projection implemented. |
| Rights and provenance | NOT RUN | No reviewed model/data rights evidence supplied. |
| Resource capture | NOT RUN | No hardware measurements supplied. |
| Human review | NOT RUN | No reviewer score supplied. |
| Product integration | NOT RUN | Evaluation harness only. |

## Decision

**NO-GO pending separately approved control/contract correction**

Centered sampling and exact fresh-process determinism are implemented, but the unmodified frozen scientific gates still reject the procedural control. This manifest makes no claim that Terrain Diffusion was evaluated, suitable for integration, or productized.
