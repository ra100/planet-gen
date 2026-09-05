---
title: "feat: UI redesign — instrument-panel shell"
type: feat
status: active
date: 2026-09-05
origin: direct request ("review the whole UI design, create new modern UI with great UX, keep all bells and whistles, extensible")
---

# feat: UI redesign — Instrument Panel shell

## 1. Review of the current UI

Audited `src/app.rs` (the entire UI lives in one ~700-line `update()`).
Baseline screenshot: `design-captures/ui_before.png`.

| # | Finding | Evidence | Severity |
|---|---------|----------|----------|
| R1 | No visual identity — stock egui defaults: default grey, framework-blue selection, uniform rounding, heading+separator spam. Reads as a debug tool, not a product. | `update()` never touches `egui::Style`/`Visuals` | High |
| R2 | One 40-control column. Physics, terrain, climate, lighting, clouds, civ, layers, view modes and advanced tweaks all stack in a single scroll area. Violates Hick's law; no way to focus on the stage of work you're in. | left `SidePanel` body, lines ~810–1230 | High |
| R3 | Misplaced controls: "Export Maps" view-mode chips and preview-resolution chips are buried inside *Render Layers*; the export action itself is in the right panel; "Reset rotation" is also inside Render Layers. Grouping follows code history, not user task. | lines ~1084–1134 | High |
| R4 | Orphaned engine features: `ring_inner/outer/tilt/opacity` and `lava_glow` are wired into `PreviewUniforms` but have **zero UI controls**. | fields at lines 68–72, uniforms at 319–323, no slider anywhere | High |
| R5 | Viewport is second-class: canvas clamped to min(w,h) with no HUD; loading overlay is a full-panel black scrim; gesture hints are small text at the bottom of a scrolled-away panel. | lines ~1361–1454 | Medium |
| R6 | Inconsistent control grammar: `ui.heading` for some groups, plain labels for others ("Clouds", "Civilization"); unlabeled checkbox drives the Surface-Age override; button captions embed keyboard hints ("Randomize Seed (N)"); sun angles display raw radians. | throughout | Medium |
| R7 | No global status: GPU name is a `ui.small` at the bottom of a scrolled panel; weather/erosion busy state has no indicator anywhere near the work surface. | lines ~1224–1227 | Medium |
| R8 | No value-state affordances: no units discipline, no double-click-to-reset, no modified-vs-default indicator. | throughout | Low |
| R9 | Extensibility tax: every parameter is a bespoke inline block with its own dirty-flag wiring; adding one slider means editing the 700-line `update()`. | architecture | High |

## 2. Lineage: Functionalist Instrument Panel

Braun-style functionalism × observatory/avionics readouts, delivered through
DCC-tool conventions (Jakob's Law: this audience lives in Blender/Houdini).

**Reference artifacts**
1. Blender 4.x Properties editor — dark neutral chrome that recedes behind the artwork; viewport-first layout.
2. Teenage Engineering OP-1 field / RUNNER — physical-feeling controls, micro-labels, tight tolerances.
3. Observatory / avionics status strips — tabular monospace figures with units, state lamps, a permanent bottom status line.

**Five rules this imposes**
1. **Type:** humanist sans (Ubuntu-Light, embedded) for labels ≤14 px; monospace (Hack, embedded) for every numeric readout and value; section headers are uppercase micro-caps in the faint text tone. Hierarchy from position and weight, never size inflation.
2. **Colour:** cool blue-charcoal tinted neutrals (OKLCH hue ≈ 258°, no pure black/white); **one accent: sodium amber** `oklch(0.80 0.13 79)` — the sun/city-light association, complementary to planet blues; semantic hues derived at the same muted chroma; colour is a state signal only.
3. **Composition:** the viewport is the protagonist — full-bleed canvas with floating HUD chips and a Blender-style viewport header strip; controls live in rails (left = authoring tabs, right = readouts + export); asymmetric density; flush-left everything.
4. **Space:** 8 px base rhythm; sections separated by space + micro-header, not separator spam; dividers only between functional zones.
5. **Depth:** flat chrome with 1 px edge definition; elevation (shadow) reserved for overlays (HUD chips, help window).

**Five things this forbids** — the operative half
1. No emoji or decorative icons — labels over icons; where a glyph is needed, geometric characters only (`▸ ● ⌘`).
2. No pure `#000`/`#fff`; every neutral tinted toward hue 258°.
3. No default egui blue selection or framework highlights anywhere.
4. Never show more than one workflow stage of controls at a time (tabs own their scope).
5. No decorative separators; a hairline must delimit functional zones or it is deleted.

**The one deliberate departure:** the planet render itself is saturated and colourful, so the chrome stays near-monochrome and the accent appears ≤3× per view — the artwork is the only loud thing in the room.

## 3. Tokens (computed from OKLCH in code — `src/ui/theme.rs`)

| Token | OKLCH | Use |
|-------|-------|-----|
| `bg_deep` | 0.145 0.010 258 | viewport backdrop, extreme bg |
| `bg_panel` | 0.175 0.012 258 | side rails, top bar, status bar |
| `bg_raised` | 0.215 0.014 258 | chips, HUD frames, windows, hovering |
| `bg_input` | 0.130 0.010 258 | text edits, slider rails (recessed) |
| `edge` / `edge_strong` | 0.28 / 0.36 C .014–.016 | hairlines, widget strokes |
| `text` / `dim` / `faint` | 0.90 / 0.72 / 0.55 | label tones (all tinted 258) |
| `accent` | 0.80 0.130 79 | primary action, active state, slider fill |
| `accent_ink` | dark on accent | text over accent |
| `ok` / `danger` | 0.75 0.12 165 / 0.63 0.16 27 | success, errors (semantic = family + text) |

## 4. Information architecture (all existing controls preserved, R4 features exposed)

```
┌────────────────────────────────────────────────────────────────────────┐
│ PLANET·GEN   [⌘N Randomize Seed]  planet_1234            [? Help]      │ ← top bar
├──────────────┬────────────────────────────────────────┬────────────────┤
│ Planet Terrain Climate Look Render   ← tabs           │ DERIVED        │
│ ── section headers (uppercase micro-caps)             │  instrument    │
│ • sliders: units, modified-dot, dbl-click reset       │  rows (mono)   │
│ PLANET: orbit/mass/tilt/day · seed · presets          │ ⚠ plausibility │
│ TERRAIN: continents · water/erosion · tect adv        │── EXPORT       │
│ CLIMATE: moisture/season · clouds · storms            │  name, res,    │
│ LOOK: sun/star/relief · night side · RINGS ●LAVA      │  layer list w/ │
│ RENDER: layer toggles · preview resolution            │  filenames,    │
│                                                       │  [Export],     │
│  viewport header: SHADED|MAPS ▸ | DEBUG ▸   zoom reset │  progress      │
│ ┌──────────────────────────────────────────┐          │                │
│ │ canvas (full-bleed)   HUD: mode chip ↘  │          │                │
│ │ loading pill instead of black scrim      │          │                │
│ └──────────────────────────────────────────┘          │                │
├──────────────┴────────────────────────────────────────┴────────────────┤
│ GPU · res · view · WEATHER ● · EROSION n · export %           ⌘ hints  │ ← status bar
└────────────────────────────────────────────────────────────────────────┘
```

View-mode selection moves from inside "Render Layers" to the **viewport header**
(Blender convention: shading mode lives above the canvas). Wind-transport toggle
moves to Climate (it is a climate control, not a render layer). Erosion toggle
moves to Terrain (generation, not rendering).

## 5. Extensibility architecture

New module tree:

```
src/ui/
  mod.rs      — apply_theme(); docs for "how to add a parameter"
  theme.rs    — OKLCH→sRGB converter; token constants; build_style()
  widgets.rs  — section_header, slider_row/int_row/angle_row (units, modified
                dot, double-click reset), chip_row, instrument_row, callout,
                status_lamp, hud_chip, key_cap
```

* **Declarative tables as single source of truth:** `VIEW_MODES` (id, label,
  group) drives the header chips, HUD badge, and the debug-texture slot pick;
  `EXPORT_LAYERS` (label, output filename, bound flag) drives the checklist and
  the export summary string. Adding a view mode or layer = one table row.
* **Parameter rows are one call:** `widgets::slider_row(ui, &mut value, Spec)`
  returns `changed`; the tab function sets the dirty flag. Adding a slider is
  one line + the uniform mapping that already exists.
* **Presets** (`PRESETS` table): archetype name + parameter bundle; adding a
  preset is one data row. Ships: Earth analog, Mars analog, Ocean world,
  Snowball, Hothouse, Volcanic, Ringed world.

## 6. Tasks

| Task | DoD | Status |
|------|-----|--------|
| UI-1 Review + lineage doc | this file | cc:完了 |
| UI-2 `ui/theme.rs` tokens + style | compiles; no framework blue anywhere in screenshots | cc:完了 |
| UI-3 `ui/widgets.rs` param rows/chips/instruments | headless egui smoke test passes | cc:完了 |
| UI-4 Shell: top bar, tabbed rail, viewport header+HUD, inspector, status bar | all existing controls reachable; behaviour parity on dirty flags | cc:完了 |
| UI-5 Expose rings + lava glow; presets table | sliders drive uniforms already wired | cc:完了 |
| UI-6 Help overlay + shortcut polish | F1 / ? opens overlay; N/R/arrows/+/- preserved | cc:完了 |
| UI-7 `cargo test --lib` green + headless UI smoke test | 0 failures | cc:完了 |

## 7. Implementation notes (post-build)

* **Verification:** `cargo build` zero warnings; `cargo test --lib` 185 passed /
  0 failed under software Vulkan (lavapipe), including six new tests in
  `ui::theme` (OKLCH converter vs hardcoded tokens, temperature discipline,
  no pure black/white) and `ui::widgets` (full widget vocabulary rendered
  through a headless `egui::Context`). Visual screenshot review of the running
  app was **not possible in this sandbox** (X11 window mapping panics, Xvfb
  segfaults); run `design-captures/capture.sh design-captures/ui_v2.png` on a
  real display for pixel evidence.
* **Callouts use text tags** (`ERROR`, `NOTICE`, `DONE`) rather than glyph
  icons — font-fallback-safe and consistent with the labels-over-icons rule.
* **DESIGN.md governance:** the workspace `DESIGN.md` pins egui's native
  palette but permits overrides "with a dedicated UI change" — this plan is
  that change. Its durable rules are honored throughout: text-first status
  (lamps, callouts, and progress all carry words), one primary action per
  workflow region (RANDOMIZE SEED top bar; EXPORT TEXTURES inspector),
  progressive disclosure via tabs, flat chrome with elevation only on
  overlays. A dated amendment was appended there pointing at `src/ui/theme.rs`
  as the token source of truth.
* **Presets shipped:** Earth analog, Mars analog, Ocean world, Snowball,
  Hothouse, Volcanic, Ringed world (`PRESETS` table + `apply_preset`).
* **Behavior deltas (deliberate):** double-click with any button resets zoom
  and pan (was middle-button only); seed edits now also refresh the planet
  name; export button is genuinely disabled (not inert) when no layer is
  selected, with a disabled-state tooltip.

