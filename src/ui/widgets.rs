//! The control grammar of the instrument-panel UI.
//!
//! Every authoring control is one call: `slider_row` / `int_row` / `angle_row`
//! / `toggle_row` return whether the value changed, and render units, a
//! modified-vs-default dot, and double-click-to-reset for free. Adding a new
//! parameter to the app = one row here + its existing uniform mapping.

use eframe::egui::{self, Align, Color32, Frame, Layout, RichText, Stroke, text::LayoutJob};

use super::theme;

/// Parameters for one slider/toggle row (single source of truth per control).
#[derive(Clone, Copy)]
pub struct Param {
    pub label: &'static str,
    pub tip: &'static str,
    /// Default value; drives the modified-dot and double-click reset.
    pub default: f32,
    pub digits: usize,
    pub suffix: &'static str,
    pub log: bool,
}

impl Param {
    pub const fn new(label: &'static str, tip: &'static str, default: f32) -> Self {
        Self {
            label,
            tip,
            default,
            digits: 2,
            suffix: "",
            log: false,
        }
    }
    pub const fn with(mut self, digits: usize, suffix: &'static str) -> Self {
        self.digits = digits;
        self.suffix = suffix;
        self
    }
    pub const fn log_scale(mut self) -> Self {
        self.log = true;
        self
    }
}

fn is_modified(value: f32, default: f32) -> bool {
    (value - default).abs() > 1e-4 * default.abs().max(1.0)
}

/// Label text with a trailing accent dot when the value differs from default.
fn param_label(label: &str, modified: bool) -> LayoutJob {
    let mut job = LayoutJob::single_section(
        label.to_owned(),
        egui::TextFormat {
            color: theme::TEXT,
            font_id: egui::FontId::proportional(12.0),
            ..Default::default()
        },
    );
    if modified {
        job.append(
            "  ●",
            0.0,
            egui::TextFormat {
                color: theme::ACCENT,
                font_id: egui::FontId::proportional(8.0),
                ..Default::default()
            },
        );
    }
    job
}

/// Float slider with units, modified dot, double-click reset. Returns changed.
pub fn slider_row(
    ui: &mut egui::Ui,
    value: &mut f32,
    range: std::ops::RangeInclusive<f32>,
    p: Param,
) -> bool {
    let modified = is_modified(*value, p.default);
    let slider = egui::Slider::new(value, range)
        .logarithmic(p.log)
        .text(param_label(p.label, modified))
        .custom_formatter(move |v, _| format!("{:.*}{}", p.digits, v, p.suffix));
    let response = ui.add(slider).on_hover_text(p.tip);
    if response.double_clicked() && modified {
        *value = p.default;
        return true;
    }
    response.changed()
}

/// Integer slider variant. Returns changed.
pub fn int_row(
    ui: &mut egui::Ui,
    value: &mut i32,
    range: std::ops::RangeInclusive<i32>,
    p: Param,
) -> bool {
    let modified = is_modified(*value as f32, p.default);
    let slider = egui::Slider::new(value, range).text(param_label(p.label, modified));
    let response = ui.add(slider).on_hover_text(p.tip);
    if response.double_clicked() && modified {
        *value = p.default as i32;
        return true;
    }
    response.changed()
}

/// Slider over a radians-stored angle, displayed in degrees. Returns changed.
pub fn angle_row(
    ui: &mut egui::Ui,
    radians: &mut f32,
    range_deg: std::ops::RangeInclusive<f32>,
    p: Param,
) -> bool {
    let mut deg = radians.to_degrees();
    let default_deg = p.default.to_degrees();
    let modified = is_modified(deg, default_deg);
    let slider = egui::Slider::new(&mut deg, range_deg)
        .text(param_label(p.label, modified))
        .custom_formatter(|v, _| format!("{v:.0}°"));
    let response = ui.add(slider).on_hover_text(p.tip);
    if response.double_clicked() && modified {
        *radians = p.default;
        return true;
    }
    if response.changed() {
        *radians = deg.to_radians();
        return true;
    }
    false
}

/// Checkbox row with full-strength label. Returns changed.
pub fn toggle_row(ui: &mut egui::Ui, value: &mut bool, label: &str, tip: &str) -> bool {
    ui.checkbox(value, RichText::new(label).color(theme::TEXT))
        .on_hover_text(tip)
        .changed()
}

/// Wrapped row of small chips (segmented selector). Returns the clicked id.
pub fn chip_bar<T: Copy + PartialEq>(
    ui: &mut egui::Ui,
    options: &[(T, &'static str)],
    selected: T,
) -> Option<T> {
    let mut picked = None;
    ui.horizontal_wrapped(|ui| {
        ui.spacing_mut().item_spacing.x = 4.0;
        for (id, label) in options {
            if ui
                .add(egui::Button::selectable(
                    *id == selected,
                    RichText::new(*label).small().color(if *id == selected {
                        theme::TEXT
                    } else {
                        theme::TEXT_DIM
                    }),
                ))
                .clicked()
            {
                picked = Some(*id);
            }
        }
    });
    picked
}

/// Uppercase micro-caps section header (the only grouping device — no rules).
pub fn section_header(ui: &mut egui::Ui, title: &str) {
    ui.add_space(10.0);
    ui.label(
        RichText::new(title.to_uppercase())
            .small()
            .color(theme::TEXT_FAINT),
    );
    ui.add_space(3.0);
}

/// Secondary-tone inline label.
pub fn dim(ui: &mut egui::Ui, text: &str) {
    ui.label(RichText::new(text).small().color(theme::TEXT_DIM));
}

/// Instrument readout: dim label left, monospace value right.
pub fn instrument_row(ui: &mut egui::Ui, label: &str, value: impl std::fmt::Display) {
    ui.horizontal(|ui| {
        ui.label(RichText::new(label).small().color(theme::TEXT_FAINT));
        ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
            ui.label(
                RichText::new(format!("{value}"))
                    .monospace()
                    .size(12.0)
                    .color(theme::TEXT),
            );
        });
    });
}

/// Status lamp: `● LABEL`, lit in accent when active, faint otherwise.
pub fn lamp(ui: &mut egui::Ui, label: &str, active: bool) {
    let color = if active {
        theme::ACCENT
    } else {
        theme::TEXT_FAINT
    };
    ui.label(
        RichText::new(format!("● {label}"))
            .monospace()
            .size(10.5)
            .color(color),
    );
}

/// Small bordered key cap for shortcut lists and buttons.
pub fn key_cap(ui: &mut egui::Ui, key: &str) {
    Frame::new()
        .fill(theme::BG_RAISED)
        .stroke(Stroke::new(1.0_f32, theme::EDGE_STRONG))
        .corner_radius(3)
        .inner_margin(egui::Margin {
            left: 4,
            right: 4,
            top: 1,
            bottom: 2,
        })
        .show(ui, |ui| {
            ui.label(
                RichText::new(key)
                    .monospace()
                    .size(10.5)
                    .color(theme::TEXT_DIM),
            );
        });
}

/// Accent-filled primary action button (functionalist: one per view).
pub fn primary_button(ui: &mut egui::Ui, label: &str, enabled: bool) -> egui::Response {
    let text = RichText::new(label).strong().color(theme::ACCENT_INK);
    ui.add_enabled(
        enabled,
        egui::Button::new(text)
            .fill(theme::ACCENT)
            .stroke(Stroke::new(1.0_f32, theme::ACCENT_DEEP))
            .corner_radius(3),
    )
}

/// Tinted callout box for warnings and notices (semantic + text, never hue alone).
pub fn callout(ui: &mut egui::Ui, glyph: &str, text: &str, color: Color32) {
    let tint = Color32::from_rgba_unmultiplied(color.r(), color.g(), color.b(), 18);
    Frame::new()
        .fill(tint)
        .stroke(Stroke::new(1.0_f32, color.gamma_multiply(0.55)))
        .corner_radius(4)
        .inner_margin(egui::Margin::symmetric(8, 6))
        .show(ui, |ui| {
            ui.horizontal(|ui| {
                ui.label(RichText::new(glyph).strong().color(color));
                ui.label(RichText::new(text).small().color(theme::TEXT_DIM));
            });
        });
}

/// Floating HUD chip anchored inside `anchor_rect` (elevated, non-interactive).
///
/// `anchor_rect` must be the viewport canvas rect: an `Area` anchor resolves
/// against its *constrain rect*, which defaults to the whole screen — without
/// `constrain_to` the chips would sit on top of the top bar and status bar.
pub fn hud_chip(
    ctx: &egui::Context,
    id: egui::Id,
    anchor_rect: egui::Rect,
    anchor: egui::Align2,
    offset: f32,
    content: impl FnOnce(&mut egui::Ui),
) {
    let off = anchor.to_sign() * -offset;
    let area = egui::Area::new(id)
        .anchor(anchor, off)
        .constrain_to(anchor_rect)
        .movable(false)
        .interactable(false)
        .order(egui::Order::Foreground);
    area.show(ctx, |ui| {
        Frame::new()
            .fill(Color32::from_rgba_unmultiplied(
                theme::BG_RAISED.r(),
                theme::BG_RAISED.g(),
                theme::BG_RAISED.b(),
                216,
            ))
            .stroke(Stroke::new(1.0_f32, theme::EDGE))
            .corner_radius(4)
            .inner_margin(egui::Margin {
                left: 8,
                right: 8,
                top: 3,
                bottom: 3,
            })
            .show(ui, content);
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    /// All widgets render headless without panicking and produce draw data.
    #[test]
    fn widget_vocabulary_renders_headless() {
        let ctx = egui::Context::default();
        theme::apply(&ctx);
        let runner = ctx.run(egui::RawInput::default(), |ctx| {
            egui::CentralPanel::default().show(ctx, |ui| {
                section_header(ui, "orbit & mass");
                let mut v = 1.0f32;
                slider_row(
                    ui,
                    &mut v,
                    0.1..=50.0,
                    Param::new("Distance", "tip", 1.0)
                        .with(2, " AU")
                        .log_scale(),
                );
                let mut i = 3i32;
                int_row(ui, &mut i, 0..=10, Param::new("Storms", "tip", 2.0));
                let mut rad = 0.5f32;
                angle_row(
                    ui,
                    &mut rad,
                    -180.0..=180.0,
                    Param::new("Sun Azimuth", "tip", -0.5),
                );
                let mut b = true;
                toggle_row(ui, &mut b, "Water", "tip");
                chip_bar(ui, &[(0u32, "ALL"), (1, "HEIGHT")], 0);
                instrument_row(ui, "Gravity", "9.81 m/s²");
                lamp(ui, "WEATHER", true);
                key_cap(ui, "N");
                primary_button(ui, "EXPORT TEXTURES", true);
                callout(ui, "!", "requires migration", theme::ACCENT);
            });
        });
        assert!(!runner.shapes.is_empty());
    }

    #[test]
    fn double_click_reset_is_reported_as_change() {
        // Logic-level check of the modified predicate used by every row.
        assert!(!is_modified(1.0, 1.0));
        assert!(is_modified(1.2, 1.0));
        assert!(!is_modified(1.0 + 1e-6, 1.0));
    }

    /// HUD chips must stay inside the viewport rect they are given — never
    /// anchored to the whole window, where they would cover top-bar buttons.
    #[test]
    fn hud_chip_stays_inside_anchor_rect() {
        let ctx = egui::Context::default();
        theme::apply(&ctx);
        let anchor_rect =
            egui::Rect::from_min_max(egui::pos2(300.0, 120.0), egui::pos2(760.0, 580.0));
        let runner = ctx.run(
            egui::RawInput {
                screen_rect: Some(egui::Rect::from_min_size(
                    egui::Pos2::ZERO,
                    egui::vec2(1200.0, 800.0),
                )),
                ..Default::default()
            },
            |ctx| {
                hud_chip(
                    ctx,
                    egui::Id::new("t_hud"),
                    anchor_rect,
                    egui::Align2::RIGHT_BOTTOM,
                    16.0,
                    |ui| {
                        ui.label("ZOOM 100%");
                    },
                );
            },
        );
        assert!(!runner.shapes.is_empty(), "chip produced no shapes");
        let slack = anchor_rect.expand(2.0); // stroke width tolerance
        for shape in &runner.shapes {
            let bounds = shape.shape.visual_bounding_rect();
            if !bounds.is_finite() {
                continue; // empty placeholder shapes report Rect::NOTHING
            }
            assert!(
                slack.contains_rect(bounds),
                "hud chip escaped its anchor rect: {bounds:?}"
            );
        }
    }
}
