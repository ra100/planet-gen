# Product

## Register

product

## Users

General creators exploring physically plausible planets. They need to make useful terrain decisions without managing raw paths, approval files, model runtimes, or other provenance mechanics in the primary workflow.

## Product Purpose

Planet Gen lets creators generate and explore physically plausible planets through approachable controls backed by scientific models. Its planned detailed-terrain path keeps model availability, download, generation, validation, application, and procedural fallback inside the app while exposing provenance only when needed.

This is a future, gated direction. UI implementation waits for the native feasibility spike and model/distribution approval; it must not claim that managed models or detailed terrain generation already work. The approved implementation direction is a local native Rust-facing ONNX/CUDA worker, but that terminology never belongs in the primary creator workflow. The sole exception is a developer-only, environment-gated model-derived reference-patch preview: pinned reference and file integrity are checked, not scientific suitability; it never changes the globe or exports and makes no product-readiness or model-distribution claim.

## Brand Personality

Clear, calm, trustworthy. It has the practical utility of Blender-style tooling, simplified for general creators: direct controls, precise feedback, and no theatrical decoration.

## Anti-references

Do not make raw paths, approval files, ONNX/CUDA terminology, or configuration terminology the primary detailed-terrain flow. Do not hide model, generation, validation, or fallback outcomes behind unexplained states or let color alone communicate status.

## Design Principles

- Progressive disclosure: begin with the creator's decision; reveal model, generation, technical, and provenance detail on demand.
- Explicit validation: state what was checked, the outcome, and the next available action.
- Safe defaults: make the supported model path clear, provide procedural terrain as a reliable fallback, and preserve approved provenance without requiring users to manage it.
- One clear primary action: each workflow should expose its next useful action without competing calls to action.
- Scientific honesty: distinguish modeled, generated, validated, active, unavailable, and future-gated capabilities plainly.

## Accessibility & Inclusion

Support keyboard navigation and high-contrast states. Every status must use text and an affordance in addition to color. Preserve readable labels, tooltips, focus visibility, and recovery actions for creators with varied experience levels.
