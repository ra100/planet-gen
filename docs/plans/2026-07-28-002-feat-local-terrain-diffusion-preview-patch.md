---
title: Local Terrain Diffusion Preview Patch
type: feat
status: dropped
date: 2026-07-28
origin: BE-005
---

# Local Terrain Diffusion Preview Patch

> **ARCHIVED / NOT A PRODUCT PATH.** This developer-preview exception is dropped with the whole-planet Terrain Diffusion direction; no worker or preview work is active. The retained document records the narrow local exception only. See [the archived roadmap entry](../../Plans.md#archived-terrain-diffusion-and-imported-terrain).

Implement a maintainer-only process boundary for one fixed local eager-PyTorch 256×256 elevation patch. It is a developer-preview exception only: a local environment-gated preview UI may display the validated patch, but it has no globe application, scientific suitability, model distribution, network access, export, or product activation claim.

## Frozen contract

- Require the accepted local interpreter, source, weights, and statistics pins before importing model code; run offline with fixed seed 1, fp32, direct cache, batch 1, no compile, bbox `[0,0,256,256)`, and no climate.
- Publish only a hash-checked `256×256` LE-f32 meters elevation payload with SHA-256 `990f8e0ded02f2cde99727a9ce4a22fa88168c221cbe74849bae1dbeb91532e0`.
- Use bounded JSONL v1 on stdout, atomic `.incomplete` staging below a fixed local app-runs root, and cancellation cleanup.
- Rust independently validates the complete artifact and exposes validated bytes plus min/max only after `done` and a successful worker exit.
- The fixed seed-1 reference patch is independent of current planet parameters.

## Units

| Unit | Scope | Status |
|------|-------|--------|
| BE-005 | Local worker, bounded Rust process/protocol boundary, and validator tests | Archived / not planned |
| FE-005 | Environment-gated native preview panel, state handling, and grayscale display | Archived / not planned |

## Decision boundary

This developer-preview exception does not supersede the existing native feasibility or TorchScript NO-GO decisions. The FE-005 panel is the approved local preview exception only; globe use, product readiness, model distribution, or scientific claims need separate approved evidence.
