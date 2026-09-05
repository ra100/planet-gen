//! UI layer for Planet Gen: theme tokens + the control grammar.
//!
//! Design lineage (see `docs/plans/2026-09-05-001-feat-ui-redesign-instrument-panel-plan.md`):
//! Braun functionalism x observatory readouts. Cool tinted neutrals, one
//! sodium-amber accent, labels over icons, tabular monospace numerics, flat
//! chrome with elevation only on overlays.
//!
//! ## How to add a parameter (extensibility contract)
//!
//! 1. Add the field + default to `PlanetGenApp` (`src/app.rs`).
//! 2. Map it in `build_uniforms()` if the shader consumes it.
//! 3. Render one row in the owning tab function, e.g.:
//!    ```ignore
//!    if ui::widgets::slider_row(ui, &mut self.my_param, 0.0..=1.0,
//!        Param::new("My Param", "hover help", 0.5).with(2, ""))
//!    {
//!        self.needs_render = true; // or needs_terrain / invalidate_weather
//!    }
//!    ```
//!    The row provides units, the modified dot, and double-click reset.
//! 4. If it is a view mode or export layer, add one row to `VIEW_MODES` /
//!    `EXPORT_LAYERS` in `app.rs` — those tables drive header chips, HUD
//!    badges, summaries, and texture-slot selection from a single source.

pub mod theme;
pub mod widgets;

pub use theme::apply as apply_theme;
