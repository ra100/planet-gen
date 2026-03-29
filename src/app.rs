use eframe::egui;
use std::sync::Arc;

use crate::compute;
use crate::gpu::GpuContext;

pub struct PlanetGenApp {
    gpu: Arc<GpuContext>,
    texture_handle: Option<egui::TextureHandle>,
    // Placeholder planet parameters
    star_distance: f32,
    planet_mass: f32,
    metallicity: f32,
    axial_tilt: f32,
    rotation_period: f32,
    seed: u32,
    needs_regenerate: bool,
}

impl PlanetGenApp {
    pub fn new(gpu: Arc<GpuContext>) -> Self {
        Self {
            gpu,
            texture_handle: None,
            star_distance: 1.0,
            planet_mass: 1.0,
            metallicity: 0.0,
            axial_tilt: 23.4,
            rotation_period: 24.0,
            seed: 42,
            needs_regenerate: true,
        }
    }

    fn generate_preview(&mut self, ctx: &egui::Context) {
        let pixels = compute::run_gradient(&self.gpu, 256, 256);

        let image = egui::ColorImage::from_rgba_unmultiplied([256, 256], &pixels);

        self.texture_handle = Some(ctx.load_texture(
            "planet_preview",
            image,
            egui::TextureOptions::LINEAR,
        ));

        self.needs_regenerate = false;
    }
}

impl eframe::App for PlanetGenApp {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        if self.needs_regenerate {
            self.generate_preview(ctx);
        }

        egui::SidePanel::left("params_panel")
            .resizable(true)
            .default_width(250.0)
            .show(ctx, |ui| {
                ui.heading("Planet Parameters");
                ui.separator();

                let mut changed = false;

                changed |= ui
                    .add(
                        egui::Slider::new(&mut self.star_distance, 0.1..=50.0)
                            .text("Star Distance (AU)")
                            .logarithmic(true),
                    )
                    .changed();

                changed |= ui
                    .add(
                        egui::Slider::new(&mut self.planet_mass, 0.01..=10.0)
                            .text("Mass (M⊕)")
                            .logarithmic(true),
                    )
                    .changed();

                changed |= ui
                    .add(
                        egui::Slider::new(&mut self.metallicity, -1.0..=1.0)
                            .text("Metallicity [Fe/H]"),
                    )
                    .changed();

                changed |= ui
                    .add(
                        egui::Slider::new(&mut self.axial_tilt, 0.0..=90.0)
                            .text("Axial Tilt (°)"),
                    )
                    .changed();

                changed |= ui
                    .add(
                        egui::Slider::new(&mut self.rotation_period, 1.0..=1000.0)
                            .text("Rotation (hours)")
                            .logarithmic(true),
                    )
                    .changed();

                ui.separator();

                ui.horizontal(|ui| {
                    changed |= ui
                        .add(egui::DragValue::new(&mut self.seed).prefix("Seed: "))
                        .changed();
                    if ui.button("🎲").on_hover_text("Random seed").clicked() {
                        self.seed = rand_seed();
                        changed = true;
                    }
                });

                if changed {
                    self.needs_regenerate = true;
                }

                ui.separator();

                ui.label(format!("GPU: {}", self.gpu.adapter_name()));
            });

        egui::CentralPanel::default().show(ctx, |ui| {
            if let Some(ref tex) = self.texture_handle {
                let available = ui.available_size();
                let size = available.min(egui::Vec2::splat(512.0));
                ui.centered_and_justified(|ui| {
                    ui.image(egui::load::SizedTexture::new(tex.id(), size));
                });
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
