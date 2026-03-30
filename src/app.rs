use eframe::egui;
use std::sync::Arc;

use crate::export::{self, ExportConfig, ExportHandle, ExportProgress};
use crate::gpu::GpuContext;
use crate::planet::{DerivedProperties, PlanetParams};
use crate::plates::{generate_plates, PlateGenParams};
use crate::preview::{PreviewRenderer, PreviewUniforms};
use crate::terrain_compute::{ErosionPipeline, TerrainComputePipeline};

pub struct PlanetGenApp {
    gpu: Arc<GpuContext>,
    preview_renderer: PreviewRenderer,
    terrain_compute: TerrainComputePipeline,
    erosion_pipeline: ErosionPipeline,
    texture_handle: Option<egui::TextureHandle>,
    params: PlanetParams,
    derived: DerivedProperties,
    rotation_y: f32,
    rotation_x: f32,
    // Visual override parameters
    continental_scale: f32,
    water_loss: f32,
    season: f32, // 0=winter, 0.5=equinox, 1=summer
    erosion_iterations: u32,
    view_mode: u32,
    preview_resolution: u32,
    needs_regenerate: bool,
    // Export state
    planet_name: String,
    export_resolution: u32,
    export_handle: Option<ExportHandle>,
    export_status: String,
    export_progress: f32,
}

impl PlanetGenApp {
    pub fn new(gpu: Arc<GpuContext>) -> Self {
        let preview_renderer = PreviewRenderer::new(&gpu);
        let terrain_compute = TerrainComputePipeline::new(&gpu);
        let erosion_pipeline = ErosionPipeline::new(&gpu);
        let params = PlanetParams::default();
        let derived = DerivedProperties::from_params(&params);
        Self {
            gpu,
            preview_renderer,
            terrain_compute,
            erosion_pipeline,
            texture_handle: None,
            params,
            derived,
            rotation_y: 0.0,
            rotation_x: 0.0,
            continental_scale: 1.0,
            water_loss: 0.0,
            season: 0.5,
            erosion_iterations: 25,
            view_mode: 0,
            preview_resolution: crate::preview::DEFAULT_PREVIEW_SIZE,
            needs_regenerate: true,
            planet_name: format!("planet_{}", PlanetParams::default().seed),
            export_resolution: export::DEFAULT_EXPORT_RESOLUTION,
            export_handle: None,
            export_status: String::new(),
            export_progress: 0.0,
        }
    }

    fn build_uniforms(&self) -> PreviewUniforms {
        let effective_ocean = self.derived.ocean_fraction * (1.0 - self.water_loss);
        let ocean_level = -1.0 + 2.0 * effective_ocean;

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
            base_temp_c: self.derived.base_temperature_c,
            ocean_fraction: effective_ocean,
            axial_tilt_rad: self.params.axial_tilt_deg.to_radians(),
            view_mode: self.view_mode,
            season: self.season,
            _pad: [0.0; 3],
        }
    }

    fn terrain_params(&self) -> (f32, f32, u32, f32, f32) {
        // Spectral exponent mapping from research
        let dist = self.params.star_distance_au;
        let dist_factor = (dist.ln() / 3.0_f32.ln()).clamp(0.0, 1.0);
        let base_beta = 1.47 + 0.91 * dist_factor;
        let beta = (base_beta + 0.3 * self.params.metallicity).clamp(1.2, 3.0);
        let hurst = (beta - 1.0) / 2.0;
        let gain = 2.0_f32.powf(-hurst);

        let mass = self.params.mass_earth;
        let amplitude = 0.6 + 0.6 * mass.powf(0.3).min(2.0);
        let frequency = (1.0 + 0.5 * mass.powf(0.2)) * self.continental_scale;

        let tilt_factor = self.params.axial_tilt_deg / 90.0;
        let octaves = (8.0 + 4.0 * tilt_factor * self.derived.tectonics_factor) as u32;

        let rotation_factor = (24.0 / self.params.rotation_period_h).clamp(0.5, 2.0);
        let lacunarity = 1.9 + 0.2 * rotation_factor;

        (amplitude, frequency, octaves, gain, lacunarity)
    }

    fn generate_preview(&mut self, ctx: &egui::Context) {
        // 1. Generate plates on CPU
        let plates = generate_plates(&PlateGenParams {
            seed: self.params.seed,
            mass_earth: self.params.mass_earth,
            ocean_fraction: self.derived.ocean_fraction * (1.0 - self.water_loss),
            tectonics_factor: self.derived.tectonics_factor,
            continental_scale: self.continental_scale,
        });

        // 2. Run compute pipeline to produce heightmap cubemap
        let (amplitude, frequency, octaves, gain, lacunarity) = self.terrain_params();
        let mut terrain = self.terrain_compute.generate(
            &self.gpu,
            &plates,
            512, // cubemap resolution per face
            self.params.seed,
            amplitude,
            frequency,
            octaves,
            gain,
            lacunarity,
        );

        // 3. Run hydraulic erosion
        let effective_ocean = self.derived.ocean_fraction * (1.0 - self.water_loss);
        let ocean_level = -1.0 + 2.0 * effective_ocean;
        self.erosion_pipeline.erode(&self.gpu, &mut terrain, self.erosion_iterations, ocean_level);

        // 4. Upload cubemap
        let cubemap_view = self.preview_renderer.upload_terrain(&self.gpu, &terrain);

        // 4. Render preview
        let uniforms = self.build_uniforms();
        let size = self.preview_resolution;
        let pixels = self.preview_renderer.render(&self.gpu, &uniforms, &cubemap_view, size);

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

    fn start_export(&mut self) {
        let config = ExportConfig {
            face_resolution: self.export_resolution,
            tile_size: export::TILE_SIZE,
            output_dir: std::env::current_dir().unwrap_or_default().join("output"),
            planet_name: self.planet_name.clone(),
            erosion_iterations: self.erosion_iterations,
            season: self.season,
        };

        let terrain_params = self.terrain_params();

        let handle = export::spawn_export(
            self.gpu.clone(),
            config,
            self.params.clone(),
            self.derived.clone(),
            self.continental_scale,
            self.water_loss,
            terrain_params,
        );

        self.export_handle = Some(handle);
        self.export_status = "Starting export...".into();
        self.export_progress = 0.0;
    }

    fn poll_export(&mut self) {
        let mut finished = false;
        if let Some(ref handle) = self.export_handle {
            while let Ok(progress) = handle.progress_rx.try_recv() {
                match progress {
                    ExportProgress::Progress { message, fraction } => {
                        self.export_status = message;
                        self.export_progress = fraction;
                    }
                    ExportProgress::Complete => {
                        self.export_status = "Export complete!".into();
                        self.export_progress = 1.0;
                        finished = true;
                    }
                    ExportProgress::Error(e) => {
                        self.export_status = format!("Error: {e}");
                        self.export_progress = 0.0;
                        finished = true;
                    }
                }
            }
        }
        if finished {
            self.export_handle = None;
        }
    }
}

impl eframe::App for PlanetGenApp {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        if self.export_handle.is_some() {
            self.poll_export();
            ctx.request_repaint();
        }

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
                    .add(egui::Slider::new(&mut self.params.star_distance_au, 0.1..=50.0)
                        .text("Distance (AU)").logarithmic(true))
                    .on_hover_text("Distance from star. Closer = hotter/smoother, farther = colder/rougher")
                    .changed();

                changed |= ui
                    .add(egui::Slider::new(&mut self.params.mass_earth, 0.01..=10.0)
                        .text("Mass (M⊕)").logarithmic(true))
                    .on_hover_text("Planet mass. Affects gravity, terrain relief, plate count")
                    .changed();

                changed |= ui
                    .add(egui::Slider::new(&mut self.params.metallicity, -1.0..=1.0)
                        .text("[Fe/H]"))
                    .on_hover_text("Stellar metallicity. Higher = rougher terrain")
                    .changed();

                changed |= ui
                    .add(egui::Slider::new(&mut self.params.axial_tilt_deg, 0.0..=90.0)
                        .text("Tilt (°)"))
                    .on_hover_text("Axial tilt. Shifts climate zones, affects terrain detail")
                    .changed();

                changed |= ui
                    .add(egui::Slider::new(&mut self.params.rotation_period_h, 1.0..=1000.0)
                        .text("Day (hours)").logarithmic(true))
                    .on_hover_text("Rotation period")
                    .changed();

                ui.separator();

                ui.horizontal(|ui| {
                    changed |= ui
                        .add(egui::DragValue::new(&mut self.params.seed).prefix("Seed: "))
                        .changed();
                    if ui.button("🎲").on_hover_text("Random seed").clicked() {
                        self.params.seed = rand_seed();
                        self.planet_name = format!("planet_{}", self.params.seed);
                        changed = true;
                    }
                });

                if changed {
                    self.update_derived();
                    self.needs_regenerate = true;
                }

                ui.separator();
                ui.heading("Visual Overrides");
                ui.separator();

                if ui.add(egui::Slider::new(&mut self.continental_scale, 0.5..=4.0)
                    .text("Continent Scale"))
                    .on_hover_text("Lower = fewer, larger continents. Higher = many small islands")
                    .changed()
                {
                    self.needs_regenerate = true;
                }

                if ui.add(egui::Slider::new(&mut self.water_loss, 0.0..=1.0)
                    .text("Water Loss"))
                    .on_hover_text("Simulate water loss. 0 = physics default, 1 = completely dry")
                    .changed()
                {
                    self.needs_regenerate = true;
                }

                let mut erosion_i32 = self.erosion_iterations as i32;
                if ui.add(egui::Slider::new(&mut erosion_i32, 0..=50)
                    .text("Erosion"))
                    .on_hover_text("Hydraulic erosion iterations. 0 = none, 25 = default, 50 = heavily eroded")
                    .changed()
                {
                    self.erosion_iterations = erosion_i32 as u32;
                    self.needs_regenerate = true;
                }

                if ui.add(egui::Slider::new(&mut self.season, 0.0..=1.0)
                    .text("Season"))
                    .on_hover_text("0 = deep winter, 0.5 = equinox, 1 = deep summer. Affects vegetation color and ice extent")
                    .changed()
                {
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
                        if self.water_loss > 0.01 {
                            let effective = self.derived.ocean_fraction * (1.0 - self.water_loss);
                            ui.label(format!("{:.0}% (eff: {:.0}%)", self.derived.ocean_fraction * 100.0, effective * 100.0));
                        } else {
                            ui.label(format!("{:.0}%", self.derived.ocean_fraction * 100.0));
                        }
                        ui.end_row();

                        ui.label("Frost line:");
                        ui.label(format!("{:.1} AU", self.derived.frost_line_au));
                        ui.end_row();

                        ui.label("Isolation M:");
                        ui.label(format!("{:.2} M⊕", self.derived.isolation_mass));
                        ui.end_row();
                    });

                if self.params.mass_earth > self.derived.isolation_mass * 15.0 {
                    ui.colored_label(
                        egui::Color32::YELLOW,
                        format!("Mass {:.1} M⊕ exceeds isolation mass {:.2} M⊕ — requires migration",
                            self.params.mass_earth, self.derived.isolation_mass),
                    );
                }

                ui.separator();
                ui.heading("View");
                ui.separator();

                let view_labels = ["Normal", "Height", "Temperature", "Moisture", "Biome", "Ocean/Ice", "Plates"];
                ui.horizontal_wrapped(|ui| {
                    for (i, label) in view_labels.iter().enumerate() {
                        if ui.selectable_label(self.view_mode == i as u32, *label).clicked() {
                            self.view_mode = i as u32;
                            self.needs_regenerate = true;
                        }
                    }
                });

                ui.add_space(4.0);

                let resolutions: [(u32, &str); 5] = [
                    (256, "256"), (512, "512"), (768, "768"), (1024, "1K"), (2048, "2K"),
                ];
                ui.horizontal(|ui| {
                    ui.label("Resolution:");
                    for (res, label) in &resolutions {
                        if ui.selectable_label(self.preview_resolution == *res, *label).clicked() {
                            self.preview_resolution = *res;
                            self.needs_regenerate = true;
                        }
                    }
                });

                ui.add_space(4.0);

                ui.horizontal(|ui| {
                    if ui.button("Reset rotation").clicked() {
                        self.rotation_y = 0.0;
                        self.rotation_x = 0.0;
                        self.needs_regenerate = true;
                    }
                    if ui.button("Face sun").clicked() {
                        self.rotation_y = 0.0;
                        self.rotation_x = 0.0;
                        self.needs_regenerate = true;
                    }
                });

                ui.separator();
                ui.heading("Export");
                ui.separator();

                ui.horizontal(|ui| {
                    ui.label("Name:");
                    ui.text_edit_singleline(&mut self.planet_name);
                });

                let resolutions: [(u32, &str); 3] = [
                    (2048, "2K"), (4096, "4K"), (8192, "8K"),
                ];
                ui.horizontal(|ui| {
                    ui.label("Resolution:");
                    for (res, label) in &resolutions {
                        if ui.selectable_label(self.export_resolution == *res, *label).clicked() {
                            self.export_resolution = *res;
                        }
                    }
                });

                let is_exporting = self.export_handle.is_some();

                if is_exporting {
                    ui.add(egui::ProgressBar::new(self.export_progress).text(&self.export_status));
                    if ui.button("Cancel").clicked() {
                        if let Some(ref handle) = self.export_handle {
                            handle.cancel.store(true, std::sync::atomic::Ordering::Relaxed);
                        }
                    }
                } else {
                    if ui.button("Export Textures").clicked() {
                        self.start_export();
                    }
                    if !self.export_status.is_empty() {
                        ui.small(&self.export_status);
                    }
                }

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

                painter.image(
                    tex.id(),
                    response.rect,
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
