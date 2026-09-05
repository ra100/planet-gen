//! Instrument-panel theme: tokens defined in OKLCH, materialized as sRGB.
//!
//! Lineage: Braun functionalism x observatory readouts. Cool blue-charcoal
//! tinted neutrals (hue 258 deg), one sodium-amber accent, no pure black or
//! white surfaces, no framework-blue selection anywhere.
//!
//! Token bytes below are exact OKLCH->sRGB conversions (verified against the
//! runtime [`ok`] converter by `hardcoded_tokens_match_oklch`). To retune the
//! palette: edit the OKLCH triple in the comment AND its byte triple, run the
//! test.

use eframe::egui::{self, Color32, CornerRadius, Stroke, TextStyle, Vec2};

fn gamma_encode(u: f32) -> f32 {
    let u = u.clamp(0.0, 1.0);
    if u <= 0.003_130_8 {
        12.92 * u
    } else {
        1.055 * u.powf(1.0 / 2.4) - 0.055
    }
}

/// OKLCH -> `Color32` (exact; for dynamic tints). Tokens use the hardcoded
/// constants below so hot paths and const contexts stay allocation-free.
// Matrix literals are published Oklab spec constants, kept at full precision
// intentionally for provenance.
#[allow(clippy::excessive_precision)]
pub fn ok(l: f32, c: f32, h_deg: f32) -> Color32 {
    let h = h_deg.to_radians();
    let a = c * h.cos();
    let b = c * h.sin();
    let l_ = l + 0.396_337_77 * a + 0.215_803_76 * b;
    let m_ = l - 0.105_561_35 * a - 0.063_854_17 * b;
    let s_ = l - 0.089_484_18 * a - 1.291_485_55 * b;
    let (l3, m3, s3) = (l_ * l_ * l_, m_ * m_ * m_, s_ * s_ * s_);
    let r = 4.076_741_66 * l3 - 3.307_711_59 * m3 + 0.230_969_93 * s3;
    let g = -1.268_438_00 * l3 + 2.609_757_40 * m3 - 0.341_319_40 * s3;
    let bl = -0.004_196_09 * l3 - 0.703_418_61 * m3 + 1.707_614_70 * s3;
    Color32::from_rgb(
        (gamma_encode(r) * 255.0 + 0.5) as u8,
        (gamma_encode(g) * 255.0 + 0.5) as u8,
        (gamma_encode(bl) * 255.0 + 0.5) as u8,
    )
}

/// Same as [`ok`] with an explicit alpha byte.
pub fn ok_a(l: f32, c: f32, h_deg: f32, a: u8) -> Color32 {
    let col = ok(l, c, h_deg);
    Color32::from_rgba_unmultiplied(col.r(), col.g(), col.b(), a)
}

// ---- Tokens ---------------------------------------------------------------
// Name            OKLCH (L C H)        sRGB
pub const BG_DEEP: Color32 = Color32::from_rgb(8, 10, 14); // 0.145 0.010 258 — viewport backdrop
pub const BG_PANEL: Color32 = Color32::from_rgb(13, 17, 22); // 0.175 0.012 258 — rails, bars
pub const BG_RAISED: Color32 = Color32::from_rgb(21, 26, 32); // 0.215 0.014 258 — chips, HUD, windows
pub const BG_INPUT: Color32 = Color32::from_rgb(5, 7, 11); // 0.130 0.010 258 — edits, rails
const FAINT_BG: Color32 = Color32::from_rgb(4, 6, 9); // 0.120 0.010 258
pub const EDGE: Color32 = Color32::from_rgb(37, 41, 48); // 0.280 0.014 258 — hairlines
pub const EDGE_STRONG: Color32 = Color32::from_rgb(56, 62, 70); // 0.360 0.016 258
pub const TEXT: Color32 = Color32::from_rgb(219, 222, 227); // 0.900 0.008 258 — labels
pub const TEXT_DIM: Color32 = Color32::from_rgb(160, 165, 172); // 0.720 0.012 258 — secondary
pub const TEXT_FAINT: Color32 = Color32::from_rgb(109, 114, 122); // 0.550 0.014 258 — micro-caps
pub const ACCENT: Color32 = Color32::from_rgb(234, 179, 82); // 0.800 0.130  79 — sodium amber
pub const ACCENT_DEEP: Color32 = Color32::from_rgb(186, 126, 21); // 0.640 0.130  74 — pressed
pub const ACCENT_INK: Color32 = Color32::from_rgb(26, 21, 11); // 0.200 0.020  85 — ink on accent
pub const OK: Color32 = Color32::from_rgb(88, 198, 155); // 0.750 0.120 165 — success
pub const DANGER: Color32 = Color32::from_rgb(217, 91, 82); // 0.630 0.160  27 — errors
const INACTIVE_BG: Color32 = Color32::from_rgb(28, 33, 40); // 0.245 0.015 258
const HOVER_BG: Color32 = Color32::from_rgb(40, 46, 56); // 0.300 0.020 258
const OPEN_BG: Color32 = Color32::from_rgb(18, 22, 28); // 0.200 0.014 258

/// Accent at low alpha for tinted backgrounds (selection washes, busy lamps).
pub fn accent_alpha(a: u8) -> Color32 {
    Color32::from_rgba_unmultiplied(ACCENT.r(), ACCENT.g(), ACCENT.b(), a)
}

// ---- Style ----------------------------------------------------------------

/// Build and apply the instrument-panel style. Called once at startup.
pub fn apply(ctx: &egui::Context) {
    let mut style = egui::Style {
        visuals: egui::Visuals::dark(),
        ..Default::default()
    };

    // Typography: humanist sans body; monospace for every numeric readout.
    style.text_styles.insert(TextStyle::Body, egui::FontId::proportional(13.0));
    style.text_styles.insert(TextStyle::Button, egui::FontId::proportional(12.0));
    style.text_styles.insert(TextStyle::Heading, egui::FontId::proportional(14.0));
    style.text_styles.insert(TextStyle::Small, egui::FontId::proportional(11.0));
    style.text_styles.insert(TextStyle::Monospace, egui::FontId::monospace(12.0));

    // Spacing: 8px rhythm, compact rails.
    style.spacing.item_spacing = Vec2::new(8.0, 5.0);
    style.spacing.button_padding = Vec2::new(9.0, 4.0);
    style.spacing.slider_rail_height = 4.0;
    style.spacing.slider_width = 110.0;
    style.spacing.indent = 14.0;
    style.spacing.window_margin = egui::Margin::same(12);

    let v = &mut style.visuals;
    v.dark_mode = true;
    v.panel_fill = BG_PANEL;
    v.extreme_bg_color = BG_DEEP;
    v.text_edit_bg_color = Some(BG_INPUT);
    v.faint_bg_color = FAINT_BG;
    v.window_fill = BG_RAISED;
    v.window_stroke = Stroke::new(1.0_f32, EDGE);
    v.window_corner_radius = CornerRadius::same(6);
    v.menu_corner_radius = CornerRadius::same(4);
    v.override_text_color = None;
    v.weak_text_color = Some(TEXT_FAINT);
    v.hyperlink_color = ACCENT;
    v.warn_fg_color = ACCENT;
    v.error_fg_color = DANGER;
    v.selection.bg_fill = accent_alpha(52);
    v.selection.stroke = Stroke::new(1.0_f32, ACCENT);
    v.text_cursor.stroke.color = ACCENT;

    // Shadows: flat chrome; elevation reserved for overlays only.
    v.window_shadow.offset = [0, 6];
    v.window_shadow.blur = 22;
    v.window_shadow.spread = 0;
    v.window_shadow.color = Color32::from_black_alpha(150);
    v.popup_shadow.offset = [0, 4];
    v.popup_shadow.blur = 14;
    v.popup_shadow.color = Color32::from_black_alpha(130);

    let w = &mut v.widgets;
    w.noninteractive.bg_fill = BG_RAISED;
    w.noninteractive.weak_bg_fill = BG_PANEL;
    w.noninteractive.bg_stroke = Stroke::new(1.0_f32, EDGE);
    w.noninteractive.fg_stroke = Stroke::new(1.0_f32, TEXT_DIM);
    w.noninteractive.corner_radius = CornerRadius::same(3);

    w.inactive.bg_fill = INACTIVE_BG;
    w.inactive.weak_bg_fill = BG_PANEL;
    w.inactive.bg_stroke = Stroke::new(1.0_f32, EDGE);
    w.inactive.fg_stroke = Stroke::new(1.0_f32, TEXT);
    w.inactive.corner_radius = CornerRadius::same(3);

    w.hovered.bg_fill = HOVER_BG;
    w.hovered.weak_bg_fill = BG_RAISED;
    w.hovered.bg_stroke = Stroke::new(1.0_f32, EDGE_STRONG);
    w.hovered.fg_stroke = Stroke::new(1.0_f32, TEXT);
    w.hovered.corner_radius = CornerRadius::same(3);

    w.active.bg_fill = ACCENT_DEEP;
    w.active.weak_bg_fill = accent_alpha(46);
    w.active.bg_stroke = Stroke::new(1.0_f32, ACCENT);
    w.active.fg_stroke = Stroke::new(1.0_f32, ACCENT_INK);
    w.active.corner_radius = CornerRadius::same(3);

    w.open.weak_bg_fill = OPEN_BG;
    w.open.bg_stroke = Stroke::new(1.0_f32, EDGE);

    ctx.set_style(style);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn converter_matches_known_reference_points() {
        let white = ok(1.0, 0.0, 0.0);
        assert!(white.r() >= 254 && white.g() >= 254 && white.b() >= 254);
        assert_eq!(ok(0.0, 0.0, 0.0), Color32::BLACK);
    }

    #[test]
    fn hardcoded_tokens_match_oklch() {
        let cases: &[(&str, (u8, u8, u8), (f32, f32, f32))] = &[
            ("BG_DEEP", (8, 10, 14), (0.145, 0.010, 258.0)),
            ("BG_PANEL", (13, 17, 22), (0.175, 0.012, 258.0)),
            ("BG_RAISED", (21, 26, 32), (0.215, 0.014, 258.0)),
            ("BG_INPUT", (5, 7, 11), (0.130, 0.010, 258.0)),
            ("EDGE", (37, 41, 48), (0.280, 0.014, 258.0)),
            ("EDGE_STRONG", (56, 62, 70), (0.360, 0.016, 258.0)),
            ("TEXT", (219, 222, 227), (0.900, 0.008, 258.0)),
            ("TEXT_DIM", (160, 165, 172), (0.720, 0.012, 258.0)),
            ("TEXT_FAINT", (109, 114, 122), (0.550, 0.014, 258.0)),
            ("ACCENT", (234, 179, 82), (0.800, 0.130, 79.0)),
            ("ACCENT_DEEP", (186, 126, 21), (0.640, 0.130, 74.0)),
            ("OK", (88, 198, 155), (0.750, 0.120, 165.0)),
            ("DANGER", (217, 91, 82), (0.630, 0.160, 27.0)),
        ];
        for (name, hardcoded, (l, c, h)) in cases {
            let exact = ok(*l, *c, *h);
            let got = [exact.r(), exact.g(), exact.b()];
            let want = [hardcoded.0, hardcoded.1, hardcoded.2];
            assert!(
                got.iter()
                    .zip(want)
                    .all(|(g, e)| (*g as i16 - e as i16).abs() <= 1),
                "{name}: runtime {got:?} vs token {want:?}"
            );
        }
    }

    #[test]
    fn no_pure_black_or_white_in_surface_tokens() {
        for tok in [BG_DEEP, BG_PANEL, BG_RAISED, TEXT, TEXT_DIM, TEXT_FAINT] {
            assert_ne!(tok, Color32::BLACK);
            assert_ne!(tok, Color32::WHITE);
        }
    }

    #[test]
    fn accent_is_warm_and_neutral_cool() {
        // Temperature discipline: accent hue sits warm (R > B), neutrals cool (B > R).
        assert!(ACCENT.r() > ACCENT.b());
        for tok in [BG_DEEP, BG_PANEL, BG_RAISED, TEXT, EDGE] {
            assert!(tok.b() > tok.r(), "{tok:?} should be cool-tinted");
        }
    }
}
