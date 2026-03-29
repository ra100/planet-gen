use eframe::egui;
use std::sync::Arc;

use crate::gpu::GpuContext;
use crate::planet::{DerivedProperties, PlanetParams};
use crate::preview::{self, PreviewRenderer, PreviewUniforms};

pub struct PlanetGenApp {
    gpu: Arc<GpuContext>,
    preview_renderer: PreviewRenderer,
    texture_handle: Option<egui::TextureHandle>,
    params: PlanetParams,
    derived: DerivedProperties,
    rotation_y: f32,
    rotation_x: f32,
    needs_regenerate: bool,
}

impl PlanetGenApp {
    pub fn new(gpu: Arc<GpuContext>) -> Self {
        let preview_renderer = PreviewRenderer::new(&gpu);
        let params = PlanetParams::default();
        let derived = DerivedProperties::from_params(&params);
        Self {
            gpu,
            preview_renderer,
            texture_handle: None,
            params,
            derived,
            rotation_y: 0.0,
            rotation_x: 0.0,
            needs_regenerate: true,
        }
    }

    fn build_uniforms(&self) -> PreviewUniforms {
        // --- Continuous parameter → terrain mapping ---
        // Based on research spectral exponents:
        //   Earth β=2.0, Mars β=2.38, Venus β=1.47
        //   persistence (gain) = 2^(-H) where H = (β-1)/2
        //   So: Earth H=0.5 → gain=0.707, Mars H=0.69 → gain=0.62, Venus H=0.235 → gain=0.85

        // Distance affects temperature → terrain character continuously
        // Closer = hotter = more volcanic/smooth (Venus-like, lower β)
        // Farther = colder = more rugged/cratered (Mars-like, higher β)
        let dist = self.params.star_distance_au;
        let dist_factor = (dist.ln() / 3.0_f32.ln()).clamp(0.0, 1.0); // 0 at 1AU, 1 at ~3AU+

        // Base spectral exponent: interpolate Venus(1.47) → Earth(2.0) → Mars(2.38)
        let base_beta = 1.47 + 0.91 * dist_factor; // 1.47 → 2.38

        // Metallicity shifts β: more metals → rougher terrain (higher β)
        let beta = (base_beta + 0.3 * self.params.metallicity).clamp(1.2, 3.0);

        // Convert β to fBm gain (persistence): gain = 2^(-H), H = (β-1)/2
        let hurst = (beta - 1.0) / 2.0;
        let gain = 2.0_f32.powf(-hurst); // Earth: 0.707, Mars: 0.62, Venus: 0.85

        // Mass affects amplitude continuously via gravity scaling
        // g ∝ M^0.46, more gravity → more relief but compressed
        let mass = self.params.mass_earth;
        let amplitude = 0.6 + 0.6 * mass.powf(0.3).min(2.0);

        // Frequency: smaller planets have relatively larger features
        // Larger planets have finer detail relative to their size
        let frequency = 1.0 + 0.5 * mass.powf(0.2);

        // Lacunarity: rotation period affects banding tendency
        // Fast rotation → slight banding (higher lacunarity)
        let rotation_factor = (24.0 / self.params.rotation_period_h).clamp(0.5, 2.0);
        let lacunarity = 1.9 + 0.2 * rotation_factor;

        // Octaves: minimum 8 per research section 7.3, up to 12 for active surfaces
        // Tilt adds detail (artistic, not physics-based)
        let tilt_factor = self.params.axial_tilt_deg / 90.0;
        let octaves = (8.0 + 4.0 * tilt_factor * self.derived.tectonics_factor) as u32; // 8-12

        // Ocean level from derived properties
        let ocean_level = -1.0 + 2.0 * self.derived.ocean_fraction;

        // Rotation matrix: Y then X
        let cy = self.rotation_y.cos();
        let sy = self.rotation_y.sin();
        let cx = self.rotation_x.cos();
        let sx = self.rotation_x.sin();

        PreviewUniforms {
            rotation: [
                [cy, sy * sx, sy * cx, 0.0],
                [0.0, cx, -sx, 0.0],
                [-sy, cy * sx, cy * cx, 0.0],
                [0.0, 0.0, 0.0, 1.0],
            ],
            light_dir: [0.5, 0.7, -1.0],
            ocean_level,
            seed_offset: preview::seed_to_offset(self.params.seed),
            frequency,
            lacunarity,
            gain,
            amplitude,
            octaves,
            base_temp_c: self.derived.base_temperature_c,
            ocean_fraction: self.derived.ocean_fraction,
            axial_tilt_rad: self.params.axial_tilt_deg.to_radians(),
            tectonics_factor: self.derived.tectonics_factor,
        }
    }

    fn generate_preview(&mut self, ctx: &egui::Context) {
        let uniforms = self.build_uniforms();
        let size = self.preview_renderer.size;
        let pixels = self.preview_renderer.render(&self.gpu, &uniforms);

        let image =
            egui::ColorImage::from_rgba_unmultiplied([size as usize, size as usize], &pixels);

        self.texture_handle = Some(ctx.load_texture(
            "planet_preview",
            image,
            egui::TextureOptions::LINEAR,
        ));

        self.needs_regenerate = false;
    }

    fn update_derived(&mut self) {
        self.derived = DerivedProperties::from_params(&self.params);
    }
}

impl eframe::App for PlanetGenApp {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        if self.needs_regenerate {
            self.generate_preview(ctx);
        }

        egui::SidePanel::left("params_panel")
            .resizable(true)
            .default_width(280.0)
            .show(ctx, |ui| {
                ui.heading("Planet Parameters");
                ui.separator();

                let mut changed = false;

                changed |= ui
                    .add(
                        egui::Slider::new(&mut self.params.star_distance_au, 0.1..=50.0)
                            .text("Distance (AU)")
                            .logarithmic(true),
                    )
                    .on_hover_text("Distance from star. Closer = hotter/smoother (Venus-like), farther = colder/rougher (Mars-like)")
                    .changed();

                changed |= ui
                    .add(
                        egui::Slider::new(&mut self.params.mass_earth, 0.01..=10.0)
                            .text("Mass (M⊕)")
                            .logarithmic(true),
                    )
                    .on_hover_text("Planet mass. Affects gravity, terrain relief, and feature scale")
                    .changed();

                changed |= ui
                    .add(
                        egui::Slider::new(&mut self.params.metallicity, -1.0..=1.0)
                            .text("[Fe/H]"),
                    )
                    .on_hover_text(
                        "Stellar metallicity. Higher = rougher terrain (more rocky minerals). Affects spectral exponent β",
                    )
                    .changed();

                changed |= ui
                    .add(
                        egui::Slider::new(&mut self.params.axial_tilt_deg, 0.0..=90.0)
                            .text("Tilt (°)"),
                    )
                    .on_hover_text("Axial tilt. Higher tilt = more terrain detail (seasonal erosion effects)")
                    .changed();

                changed |= ui
                    .add(
                        egui::Slider::new(&mut self.params.rotation_period_h, 1.0..=1000.0)
                            .text("Day (hours)")
                            .logarithmic(true),
                    )
                    .on_hover_text("Rotation period. Faster rotation = slightly more banded features")
                    .changed();

                ui.separator();

                ui.horizontal(|ui| {
                    changed |= ui
                        .add(egui::DragValue::new(&mut self.params.seed).prefix("Seed: "))
                        .changed();
                    if ui.button("🎲").on_hover_text("Random seed").clicked() {
                        self.params.seed = rand_seed();
                        changed = true;
                    }
                });

                if changed {
                    self.update_derived();
                    self.needs_regenerate = true;
                }

                ui.separator();

                ui.heading("Derived Properties");
                ui.separator();

                egui::Grid::new("derived_grid")
                    .num_columns(2)
                    .spacing([8.0, 4.0])
                    .show(ui, |ui| {
                        ui.label("Type:");
                        ui.label(format!("{:?}", self.derived.planet_type));
                        ui.end_row();

                        ui.label("Tectonics:");
                        ui.label(format!("{:?} ({:.0}%)", self.derived.tectonic_regime, self.derived.tectonics_factor * 100.0));
                        ui.end_row();

                        ui.label("Atmosphere:");
                        ui.label(format!("{:?} ({:.0}%)", self.derived.atmosphere_type, self.derived.atmosphere_strength * 100.0));
                        ui.end_row();

                        ui.label("Gravity:");
                        ui.label(format!("{:.2} m/s²", self.derived.surface_gravity));
                        ui.end_row();

                        ui.label("Temp:");
                        ui.label(format!("{:.1} °C", self.derived.base_temperature_c));
                        ui.end_row();

                        ui.label("Ocean:");
                        ui.label(format!("{:.0}%", self.derived.ocean_fraction * 100.0));
                        ui.end_row();

                        ui.label("Frost line:");
                        ui.label(format!("{:.1} AU", self.derived.frost_line_au));
                        ui.end_row();

                        ui.label("Isolation M:");
                        ui.label(format!("{:.2} M⊕", self.derived.isolation_mass));
                        ui.end_row();
                    });

                // MMSN plausibility warning
                if self.params.mass_earth > self.derived.isolation_mass * 5.0 {
                    ui.colored_label(
                        egui::Color32::YELLOW,
                        format!(
                            "Mass {:.1} M⊕ exceeds isolation mass {:.2} M⊕ — requires planetary migration",
                            self.params.mass_earth, self.derived.isolation_mass
                        ),
                    );
                }

                ui.separator();

                ui.heading("View");
                ui.separator();

                ui.horizontal(|ui| {
                    if ui.button("Reset rotation").clicked() {
                        self.rotation_y = 0.0;
                        self.rotation_x = 0.0;
                        self.needs_regenerate = true;
                    }
                    if ui.button("Face sun").on_hover_text("Rotate to face the light source").clicked() {
                        // Light is at (0.5, 0.7, -1.0) → planet should face toward it
                        self.rotation_y = 0.0;
                        self.rotation_x = 0.0;
                        self.needs_regenerate = true;
                    }
                });

                ui.separator();
                ui.small(format!("GPU: {}", self.gpu.adapter_name()));
                ui.small("Drag preview to rotate");
            });

        egui::CentralPanel::default().show(ctx, |ui| {
            if let Some(ref tex) = self.texture_handle {
                let available = ui.available_size();
                let size = available.x.min(available.y);

                let (response, painter) = ui.allocate_painter(
                    egui::Vec2::splat(size),
                    egui::Sense::click_and_drag(),
                );

                let rect = response.rect;
                painter.image(
                    tex.id(),
                    rect,
                    egui::Rect::from_min_max(egui::pos2(0.0, 0.0), egui::pos2(1.0, 1.0)),
                    egui::Color32::WHITE,
                );

                if response.dragged() {
                    let delta = response.drag_delta();
                    self.rotation_y += delta.x * 0.01;
                    self.rotation_x -= delta.y * 0.01;
                    self.rotation_x = self.rotation_x.clamp(
                        -std::f32::consts::FRAC_PI_2 + 0.1,
                        std::f32::consts::FRAC_PI_2 - 0.1,
                    );
                    self.needs_regenerate = true;
                }
            } else {
                ui.centered_and_justified(|ui| {
                    ui.label("Generating preview...");
                });
            }
        });
    }
}

fn rand_seed() -> u32 {
    use std::time::{SystemTime, UNIX_EPOCH};
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .subsec_nanos()
}
