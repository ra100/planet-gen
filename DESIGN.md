---
name: Planet Gen
description: A native egui workbench for generating and exploring physically plausible planets.
---

# Design System: Planet Gen

## Overview

**Creative North Star: "The Planetarium Workbench"**

Planet Gen is a restrained native desktop tool for focused exploration. It preserves the existing egui dark/default system instead of introducing a parallel brand palette: a resizable parameter side panel, a rendered planet workspace, clear group boundaries, and direct manipulation are the visual language.

The product starts simple and makes technical detail available when it matters. Primary controls remain visible; advanced tuning, render-layer switches, managed-model details, generation status, warnings, and provenance details are progressively disclosed. The system rejects raw paths, approval files, ONNX/CUDA terminology, and configuration terminology in the primary detailed-terrain flow, decorative web-card styling, and status that depends on color alone.

The detailed-terrain workflow described here is future direction only. Its UI must wait for the native feasibility spike and model/distribution approval; current screens must not imply that managed model download or local detailed terrain generation is available.

### Local Terrain Patch (Development)

The one explicit exception is a collapsed developer-only section, visible only with the local development environment flag. It generates one fixed 256×256 model-derived grayscale reference patch after pinned-reference and file-integrity checks, not scientific validation, without applying it to the globe or exports. Its inline states are unavailable, ready, running with Cancel, failed with Retry, and complete with Generate again; a retained prior patch remains visible during retries, failures, or cancellation. The section uses only text, native controls, and the active theme—no model/runtime internals, paths, cards, modal, or color-only status.

**Key Characteristics:**
- Direct, labeled controls for creator-facing decisions.
- Mostly flat tonal layers with separators and spacing, not decorative shadows.
- Scientific and provenance claims stated as plain text.
- Existing side panel, slider, button, checkbox, selectable label, and collapsing-header vocabulary remains normative.

## Colors

Use the active egui dark/default visuals and semantic `Color32` status treatment already present in the app. This project does not define custom color tokens or a brand palette; do not invent hex values or override egui defaults without a dedicated UI change.

### Primary
- **Native egui accent:** use the active egui theme's interaction accent for the one clear primary action, current selection, and keyboard focus. Preserve theme-level contrast rather than hardcoding a replacement.

### Neutral
- **Native egui surfaces:** use the framework's panel, window, and text colors for the side panel, workspace, separators, and ordinary labels. Tonal separation comes from the native theme and layout hierarchy.
- **Semantic status colors:** retain existing error and warning cues only alongside explicit text, such as a GPU error message or a physical-plausibility warning.

**The Text-First Status Rule.** Every model, generation, error, warning, validation, active-terrain, unavailable-action, and recovery state must remain understandable without color.

## Typography

**Display Font:** native egui proportional font (framework default)
**Body Font:** native egui proportional font (framework default)
**Label/Mono Font:** use egui's native monospace treatment only for provenance identifiers or technical values when needed.

**Character:** the framework-default proportional type keeps labels, sliders, buttons, and data compact and familiar. Headings provide group hierarchy; control labels remain sentence case and descriptive.

### Hierarchy
- **Heading:** `ui.heading` for the parameter panel's major groups, such as Planet Parameters and Visual Overrides.
- **Section label:** `ui.label` plus separators for compact subsections such as Clouds, Civilization, Export Maps, and Debug Views.
- **Body:** `ui.label` for concise explanatory text, status, and derived-property values.
- **Small detail:** `ui.small` for secondary provenance, inline export status, and lower-priority context.
- **Control label:** slider, checkbox, selectable-label, and button text must describe the effect or outcome, not an internal field name.

**The Plain-Language Rule.** Creator-facing labels say what changes or what happens. Raw paths, approval-file names, ONNX/CUDA terminology, and config keys belong only in disclosed technical detail.

## Elevation

The interface is flat by default. Depth comes from native egui panels, separators, grouping, scroll containment, and the rendered planet workspace rather than decorative shadows. Keep the existing side-panel-to-workspace structure intact.

**The Separator Rule.** Use spacing and `ui.separator()` to establish groups. Do not introduce web-style cards, glass surfaces, or large soft shadows merely to create hierarchy.

## Components

### Side Panel
- **Shape:** the existing resizable left `egui::SidePanel` is the primary control surface; preserve its approximately 280-point default width and vertical scroll behavior.
- **Organization:** use headings, separators, and compact groups. Keep common controls visible and use `egui::CollapsingHeader` for Render Layers, Advanced Tweaks, and comparable technical detail.
- **State:** an active generated terrain state identifies the current source in text and preserves a visible route back to procedural terrain.

### Sliders and Value Controls
- **Style:** retain labeled `egui::Slider` controls with logical ranges, logarithmic behavior where already useful, and hover help that describes the creative or physical effect.
- **Companion controls:** use `egui::DragValue` for exact seed and numeric entry beside a direct randomize action when applicable.
- **State:** disabling a control must pair the disabled appearance with nearby text explaining why it is unavailable.

### Buttons and Recovery Actions
- **Primary action by state:** use **Download Detailed Terrain Model** when no approved model is available, **Generate Detailed Terrain** when it is ready, and **Cancel** while download, verification, or generation is in progress.
- **Recovery by state:** use **Retry** after a recoverable download, verification, generation, or validation failure; **Manage Model** when storage, availability, or model selection needs attention; and **Use Procedural Terrain** for unsupported GPU, insufficient VRAM or disk, unavailable models, or any creator choice to fall back.
- **Gating:** these actions are future UI only and must not be added until the native feasibility spike and model/distribution approval complete.
- **Existing actions:** retain compact native buttons for Reset rotation, Dismiss, Cancel, and other immediate actions; use clear verb labels where an icon alone would be ambiguous.

### Detailed Terrain Generation (Future, Gated)
- **Availability flow:** model unavailable -> **Download Detailed Terrain Model** -> download/verify progress -> model ready -> **Generate Detailed Terrain**. Show model source, size, requirements, and provenance only in a collapsible **Model Details** disclosure.
- **Generation flow:** Generate Detailed Terrain -> inline progress -> **Cancel** -> validate result -> generated terrain active. Keep a **Use Procedural Terrain** fallback available after success as well as failure.
- **Failure states:** unsupported GPU, insufficient VRAM, insufficient disk, download failure, verification failure, generation failure, and terrain validation failure each name the constraint in plain language and present the applicable primary recovery action: Retry, Manage Model, or Use Procedural Terrain.
- **Apply state:** do not replace the active terrain until validation succeeds. After success, state that generated terrain is active and expose generation and provenance details on demand.
- **Internal plumbing:** artifact import may remain advanced/internal provenance support, but it is not a primary creator-facing workflow.

### Status and Progress
- **Inline status:** keep export, model download, verification, generation, and validation feedback near the initiating action using text and native `egui::ProgressBar` where progress is measurable.
- **Errors and warnings:** use the existing top error panel and inline warning pattern with explicit messages and dismiss/recovery actions.
- **Keyboard and contrast:** every button, disclosure, slider, and recovery action must be reachable with egui's keyboard focus behavior and must retain a visible high-contrast focus state from the active theme.

## Do's and Don'ts

### Do:
- **Do** preserve the native egui dark/default system and existing flat side-panel vocabulary.
- **Do** keep the future detailed-terrain path fully in-app: managed availability, download, verification, generation, validation, application, and procedural fallback.
- **Do** use one state-specific primary action: **Download Detailed Terrain Model**, **Generate Detailed Terrain**, **Cancel**, **Retry**, **Manage Model**, or **Use Procedural Terrain**.
- **Do** disclose model, generation, and provenance details progressively: concise status first, Model Details only on demand.
- **Do** pair every status color with clear text, an icon or affordance where helpful, and a recovery action when the user can act.
- **Do** state that this workflow is future and gated until the native feasibility spike and model/distribution approval complete.

### Don't:
- **Don't** place raw paths, approval files, ONNX/CUDA terminology, or config terminology in the primary flow.
- **Don't** invent a web palette, Tailwind token system, HTML components, cards, gradients, glassmorphism, or decorative shadows for this native egui app.
- **Don't** make a modal the default response to a recoverable validation problem; use inline progressive disclosure first.
- **Don't** communicate model, generation, validation, provenance, warning, or error status through color alone.
- **Don't** claim that model download or detailed terrain generation already works; label modeled, future-gated, generated, validated, active, and unavailable states exactly.

---

## Amendment — 2026-09-05 (Instrument Panel UI redesign)

The Colors section above ("use the active egui dark/default visuals") is
superseded for the whole app by the dedicated UI change authorized there:
Phase 11 / `docs/plans/2026-09-05-001-feat-ui-redesign-instrument-panel-plan.md`.

- Token source of truth: `src/ui/theme.rs` (OKLCH-derived cool-tinted neutral
  chrome + single sodium-amber accent; no pure black/white, no framework blue).
- Control grammar and layout vocabulary: `src/ui/widgets.rs`, applied via
  `crate::ui::apply_theme`.
- All other sections of this document remain in force: text-first status,
  progressive disclosure, one clear primary action, flat tonal layers,
  plain-text provenance.
