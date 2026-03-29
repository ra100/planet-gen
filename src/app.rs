use eframe::egui;
use std::sync::Arc;

use crate::gpu::GpuContext;
use crate::planet::{DerivedProperties, PlanetParams, TectonicRegime};
use crate::preview::PreviewRenderer;
use crate::terrain::{self, TerrainParams};

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

    fn terrain_params_from_planet(&self) -> TerrainParams {
        // Map derived planet properties to terrain generation parameters
        let frequency = match self.derived.tectonic_regime {
            TectonicRegime::PlateTectonics => 1.5, // More varied terrain
            TectonicRegime::StagnantLid => 0.8,    // Smoother, older surface
        };

        // Heavier planets → slightly higher amplitude (more relief)
        let amplitude = 0.8 + 0.4 * self.params.mass_earth.min(3.0) / 3.0;

        // More octaves for active surfaces
        let octaves = match self.derived.tectonic_regime {
            TectonicRegime::PlateTectonics => 8,
            TectonicRegime::StagnantLid => 6,
        };

        TerrainParams {
            resolution: 256,
            seed: self.params.seed,
            frequency,
            amplitude,
            octaves,
            lacunarity: 2.0,
            gain: 0.5,
            face: 0,
        }
    }

    fn generate_preview(&mut self, ctx: &egui::Context) {
        let terrain_params = self.terrain_params_from_planet();
        let mut terrain_data = terrain::generate_terrain(&self.gpu, &terrain_params);

        // Normalize heights to [-1, 1] so color mapping works for any seed/params
        normalize_terrain(&mut terrain_data);

        let size = self.preview_renderer.size;
        let pixels = self.preview_renderer.render(
            &self.gpu,
            &terrain_data,
            self.rotation_y,
            self.rotation_x,
            self.derived.ocean_fraction,
        );

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

/// Normalize all face heightmaps to [-1, 1] range.
fn normalize_terrain(terrain: &mut terrain::TerrainData) {
    let mut global_min = f32::INFINITY;
    let mut global_max = f32::NEG_INFINITY;

    for face in &terrain.faces {
        for &v in face {
            global_min = global_min.min(v);
            global_max = global_max.max(v);
        }
    }

    let range = global_max - global_min;
    if range < 1e-6 {
        return;
    }

    for face in &mut terrain.faces {
        for v in face.iter_mut() {
            *v = (*v - global_min) / range * 2.0 - 1.0;
        }
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
                    .on_hover_text("Distance from the star in Astronomical Units")
                    .changed();

                changed |= ui
                    .add(
                        egui::Slider::new(&mut self.params.mass_earth, 0.01..=10.0)
                            .text("Mass (M⊕)")
                            .logarithmic(true),
                    )
                    .on_hover_text("Planet mass in Earth masses")
                    .changed();

                changed |= ui
                    .add(
                        egui::Slider::new(&mut self.params.metallicity, -1.0..=1.0)
                            .text("[Fe/H]"),
                    )
                    .on_hover_text(
                        "Stellar metallicity in dex. 0 = Sun-like, negative = metal-poor, positive = metal-rich",
                    )
                    .changed();

                changed |= ui
                    .add(
                        egui::Slider::new(&mut self.params.axial_tilt_deg, 0.0..=90.0)
                            .text("Tilt (°)"),
                    )
                    .on_hover_text("Axial tilt in degrees. Earth = 23.4°")
                    .changed();

                changed |= ui
                    .add(
                        egui::Slider::new(&mut self.params.rotation_period_h, 1.0..=1000.0)
                            .text("Day (hours)")
                            .logarithmic(true),
                    )
                    .on_hover_text("Rotation period in hours. Earth = 24h")
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

                // Derived properties (read-only)
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
                        ui.label(format!("{:?}", self.derived.tectonic_regime));
                        ui.end_row();

                        ui.label("Atmosphere:");
                        ui.label(format!("{:?}", self.derived.atmosphere_type));
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
                    });

                ui.separator();
                ui.small(format!("GPU: {}", self.gpu.adapter_name()));
                ui.small("Drag preview to rotate");
            });

        egui::CentralPanel::default().show(ctx, |ui| {
            if let Some(ref tex) = self.texture_handle {
                let available = ui.available_size();
                let size = available.x.min(available.y);

                // Allocate interactive area for drag-to-rotate
                let (response, painter) = ui.allocate_painter(
                    egui::Vec2::splat(size),
                    egui::Sense::click_and_drag(),
                );

                // Draw the planet texture centered in the allocated area
                let rect = response.rect;
                painter.image(
                    tex.id(),
                    rect,
                    egui::Rect::from_min_max(egui::pos2(0.0, 0.0), egui::pos2(1.0, 1.0)),
                    egui::Color32::WHITE,
                );

                // Handle drag rotation
                if response.dragged() {
                    let delta = response.drag_delta();
                    self.rotation_y += delta.x * 0.01;
                    self.rotation_x -= delta.y * 0.01;
                    // Clamp vertical rotation
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
