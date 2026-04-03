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
    climate_moisture: f32, // 0=bone dry atmosphere, 1=full moisture from physics
    season: f32, // 0=winter, 0.5=equinox, 1=summer
    erosion_iterations: u32,
    light_azimuth: f32,   // sun horizontal angle in radians
    light_elevation: f32, // sun vertical angle in radians
    height_scale: f32,    // normal map height exaggeration
    show_atmosphere: bool, // toggle atmosphere rendering
    show_ao: bool,         // toggle ambient occlusion
    // Layer toggles for Normal view
    show_water: bool,
    show_ice: bool,
    show_biomes: bool,
    show_clouds: bool,
    show_cities: bool,
    show_erosion: bool,
    zoom: f32,            // viewport zoom level
    pan: [f32; 2],        // viewport pan in NDC units
    // Advanced terrain tweaks
    mountain_scale: f32,
    boundary_width: f32,
    warp_strength: f32,
    detail_scale: f32,
    age_override: Option<f32>, // None = derived from physics, Some = manual override
    num_plates_override: u32, // 0 = auto from physics
    num_continents: u32,      // target number of distinct landmasses (1-10)
    continent_size_variety: f32, // 0 = equal sizes, 1 = heavily skewed
    cloud_coverage: f32,
    cloud_seed: u32,
    cloud_type: f32,
    storm_count: u32,
    storm_size: f32,
    night_lights: f32,
    star_color_temp: f32,
    city_light_hue: f32,
    view_mode: u32,
    preview_resolution: u32,
    needs_terrain: bool,   // full terrain recompute (plates + compute + erosion)
    terrain_pending: bool, // true = overlay painted, next frame does the work
    terrain_start: Option<std::time::Instant>, // when terrain gen started (for overlay delay)
    needs_render: bool,    // just re-render sphere from cached cubemap
    cached_cubemap_view: Option<wgpu::TextureView>,
    // Progressive erosion state
    erosion_terrain: Option<crate::terrain_compute::TectonicTerrain>,
    erosion_remaining: u32,
    erosion_ocean_level: f32,
    // Export state
    planet_name: String,
    export_resolution: u32,
    // Export layer toggles
    export_albedo: bool,
    export_roughness: bool,
    export_clouds: bool,
    export_height: bool,
    export_emission: bool,
    export_water_mask: bool,
    export_normals: bool,
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
        let default_cloud_seed = params.seed.wrapping_add(1000);
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
            water_loss: 0.5,
            climate_moisture: 1.0,
            season: 0.5,
            erosion_iterations: 25,
            light_azimuth: -0.5,
            light_elevation: 0.3,
            height_scale: 3.0,
            show_atmosphere: true,
            show_ao: true,
            show_water: true,
            show_ice: true,
            show_biomes: true,
            show_clouds: true,
            show_cities: true,
            show_erosion: true,
            zoom: 1.0,
            pan: [0.0, 0.0],
            mountain_scale: 1.0,
            boundary_width: 0.10,
            warp_strength: 1.0,
            detail_scale: 1.0,
            age_override: None,
            num_plates_override: 0,
            num_continents: 4,
            continent_size_variety: 0.35,
            cloud_coverage: 0.5,
            cloud_seed: default_cloud_seed,
            cloud_type: 0.5,
            storm_count: 0,
            storm_size: 1.0,
            night_lights: 0.0,
            star_color_temp: 0.5,
            city_light_hue: 0.0,
            view_mode: 0,
            preview_resolution: crate::preview::DEFAULT_PREVIEW_SIZE,
            needs_terrain: true,
            terrain_pending: false,
            terrain_start: None,
            needs_render: true,
            cached_cubemap_view: None,
            erosion_terrain: None,
            erosion_remaining: 0,
            erosion_ocean_level: 0.0,
            planet_name: format!("planet_{}", PlanetParams::default().seed),
            export_resolution: export::DEFAULT_EXPORT_RESOLUTION,
            export_albedo: true,
            export_roughness: true,
            export_clouds: true,
            export_height: true,
            export_emission: true,
            export_water_mask: false,
            export_normals: false,
            export_handle: None,
            export_status: String::new(),
            export_progress: 0.0,
        }
    }

    fn build_uniforms(&self) -> PreviewUniforms {
        let effective_ocean = self.derived.ocean_fraction * (1.0 - self.water_loss);
        // Map effective_ocean [0,1] to ocean_level across full terrain height range
        // 0.0 → -0.5 (all land), ~0.45 → Earth-like, 0.7 → 0.69 (near-total ocean)
        let ocean_level = -0.5 + 1.7 * effective_ocean;

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
            light_dir: [
                self.light_azimuth.cos() * self.light_elevation.cos(),
                self.light_elevation.sin(),
                self.light_azimuth.sin() * self.light_elevation.cos(),
            ],
            ocean_level,
            base_temp_c: self.derived.base_temperature_c,
            // Climate moisture: physics ocean_fraction scaled by user's moisture slider.
            // water_loss controls sea level only; climate_moisture controls atmosphere wetness.
            ocean_fraction: self.derived.ocean_fraction * self.climate_moisture,
            axial_tilt_rad: self.params.axial_tilt_deg.to_radians(),
            view_mode: self.view_mode,
            season: self.season,
            atmosphere_density: if self.show_atmosphere { self.derived.atmosphere_strength } else { 0.0 },
            atmosphere_height: 0.02 + 0.02 * self.derived.atmosphere_strength,
            height_scale: self.height_scale,
            zoom: self.zoom,
            pan_x: self.pan[0],
            pan_y: self.pan[1],
            cloud_coverage: self.cloud_coverage,
            cloud_seed: crate::preview::seed_to_offset(self.cloud_seed)[0],
            cloud_altitude: 0.008,
            cloud_type: self.cloud_type,
            storm_count: self.storm_count as f32,
            storm_size: self.storm_size,
            night_lights: self.night_lights,
            star_color_temp: self.star_color_temp,
            city_light_hue: self.city_light_hue,
            show_ao: if self.show_ao { 1.0 } else { 0.0 },
            show_water: if self.show_water { 1.0 } else { 0.0 },
            show_ice: if self.show_ice { 1.0 } else { 0.0 },
            show_biomes: if self.show_biomes { 1.0 } else { 0.0 },
            show_clouds: if self.show_clouds { 1.0 } else { 0.0 },
            show_atmosphere_layer: if self.show_atmosphere { 1.0 } else { 0.0 },
            show_cities: if self.show_cities { 1.0 } else { 0.0 },
            _pad5: 0.0,
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

    fn regenerate_terrain(&mut self) {
        use std::time::Instant;
        let t0 = Instant::now();

        let plates = generate_plates(&PlateGenParams {
            seed: self.params.seed,
            mass_earth: self.params.mass_earth,
            ocean_fraction: self.derived.ocean_fraction * (1.0 - self.water_loss),
            tectonics_factor: self.derived.tectonics_factor,
            continental_scale: self.continental_scale,
            num_plates_override: self.num_plates_override,
        });

        let (amplitude, frequency, octaves, gain, lacunarity) = self.terrain_params();
        let terrain = self.terrain_compute.generate(
            &self.gpu,
            &plates,
            self.preview_resolution,
            self.params.seed,
            amplitude,
            frequency,
            octaves,
            gain,
            lacunarity,
            self.mountain_scale,
            self.boundary_width,
            self.warp_strength,
            self.detail_scale,
            self.derived.surface_gravity,
            self.derived.tectonics_factor,
            self.age_override.unwrap_or(self.derived.surface_age),
        );

        let effective_ocean = self.derived.ocean_fraction * (1.0 - self.water_loss);
        // Map effective_ocean [0,1] to ocean_level across full terrain height range
        // 0.0 → -0.5 (all land), ~0.45 → Earth-like, 0.7 → 0.69 (near-total ocean)
        let ocean_level = -0.5 + 1.7 * effective_ocean;

        // Show un-eroded terrain immediately
        self.cached_cubemap_view = Some(self.preview_renderer.upload_terrain(&self.gpu, &terrain));

        // Schedule progressive erosion (skipped when erosion layer is disabled)
        if self.show_erosion {
            let adaptive_iters = match self.preview_resolution {
                r if r <= 256 => (self.erosion_iterations as f32 * 0.2) as u32,
                r if r <= 512 => (self.erosion_iterations as f32 * 0.4) as u32,
                r if r <= 768 => (self.erosion_iterations as f32 * 0.6) as u32,
                _ => self.erosion_iterations,
            }.max(1);
            self.erosion_terrain = Some(terrain);
            self.erosion_remaining = adaptive_iters;
            self.erosion_ocean_level = ocean_level;
        } else {
            self.erosion_remaining = 0;
        }

        eprintln!(
            "[terrain {}px] plates+compute: {:.0}ms, scheduling {} erosion iters progressively",
            self.preview_resolution, t0.elapsed().as_secs_f64() * 1000.0, self.erosion_remaining,
        );

        self.needs_terrain = false;
        self.needs_render = true;
    }

    /// Apply a batch of erosion iterations and re-render. Called each frame.
    fn erode_batch(&mut self) {
        use std::time::Instant;
        let batch_size = 5u32;
        let iters = batch_size.min(self.erosion_remaining);

        if let Some(ref mut terrain) = self.erosion_terrain {
            let t = Instant::now();
            self.erosion_pipeline.erode(&self.gpu, terrain, iters, self.erosion_ocean_level);
            self.cached_cubemap_view = Some(self.preview_renderer.upload_terrain(&self.gpu, terrain));
            self.erosion_remaining -= iters;

            eprintln!(
                "[erosion batch] {} iters in {:.0}ms, {} remaining",
                iters, t.elapsed().as_secs_f64() * 1000.0, self.erosion_remaining,
            );
        }

        if self.erosion_remaining == 0 {
            self.erosion_terrain = None;
            self.terrain_start = None;
        }
        self.needs_render = true;
    }

    fn render_preview(&mut self, ctx: &egui::Context) {
        if let Some(ref cubemap_view) = self.cached_cubemap_view {
            let uniforms = self.build_uniforms();
            let size = self.preview_resolution;
            let pixels = self.preview_renderer.render(&self.gpu, &uniforms, cubemap_view, size);

            let image =
                egui::ColorImage::from_rgba_unmultiplied([size as usize, size as usize], &pixels);

            self.texture_handle = Some(ctx.load_texture(
                "planet_preview",
                image,
                egui::TextureOptions::LINEAR,
            ));
        }
        self.needs_render = false;
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

        egui::SidePanel::left("params_panel")
            .resizable(true)
            .default_width(280.0)
            .show(ctx, |ui| {
                egui::ScrollArea::vertical().show(ui, |ui| {
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
                    if ui.button("🎲").on_hover_text("Random seed").clicked() {
                        self.params.seed = rand_seed();
                        self.planet_name = format!("planet_{}", self.params.seed);
                        changed = true;
                    }
                    changed |= ui
                        .add(egui::DragValue::new(&mut self.params.seed).prefix("Seed: "))
                        .changed();
                });

                if changed {
                    self.update_derived();
                    self.needs_terrain = true;
                }

                ui.separator();
                ui.heading("Visual Overrides");
                ui.separator();

                if ui.add(egui::Slider::new(&mut self.continental_scale, 0.5..=4.0)
                    .text("Continent Scale"))
                    .on_hover_text("Lower = fewer, larger continents. Higher = many small islands")
                    .changed()
                {
                    self.needs_terrain = true;
                }

                let mut nc_i32 = self.num_continents as i32;
                if ui.add(egui::Slider::new(&mut nc_i32, 1..=10)
                    .text("Continents"))
                    .on_hover_text("Target number of distinct landmasses. 1 = supercontinent, 10 = archipelago")
                    .changed()
                {
                    self.num_continents = nc_i32 as u32;
                    self.needs_terrain = true;
                }

                if ui.add(egui::Slider::new(&mut self.continent_size_variety, 0.0..=1.0)
                    .text("Size Variety"))
                    .on_hover_text("Continent size distribution. 0 = equal sizes, 1 = one large + many small")
                    .changed()
                {
                    self.needs_terrain = true;
                }

                if ui.add(egui::Slider::new(&mut self.water_loss, 0.0..=1.0)
                    .text("Water Loss"))
                    .on_hover_text("Sea level control. 0 = ocean world, 1 = no surface water")
                    .changed()
                {
                    self.needs_terrain = true;
                }
                if ui.add(egui::Slider::new(&mut self.climate_moisture, 0.0..=1.0)
                    .text("Atm. Moisture"))
                    .on_hover_text("Atmospheric moisture. 0 = bone dry (desert world), 1 = full moisture. Independent of water loss.")
                    .changed()
                {
                    self.needs_render = true;
                }

                let mut erosion_i32 = self.erosion_iterations as i32;
                if ui.add(egui::Slider::new(&mut erosion_i32, 0..=50)
                    .text("Erosion"))
                    .on_hover_text("Hydraulic erosion iterations. 0 = none, 25 = default, 50 = heavily eroded")
                    .changed()
                {
                    self.erosion_iterations = erosion_i32 as u32;
                    self.needs_terrain = true;
                }

                if ui.add(egui::Slider::new(&mut self.season, 0.0..=1.0)
                    .text("Season"))
                    .on_hover_text("0 = deep winter, 0.5 = equinox, 1 = deep summer. Affects vegetation color and ice extent")
                    .changed()
                {
                    self.needs_render = true;
                }

                ui.separator();
                ui.heading("Lighting");
                ui.separator();

                if ui.add(egui::Slider::new(&mut self.light_azimuth, -std::f32::consts::PI..=std::f32::consts::PI)
                    .text("Sun Azimuth"))
                    .on_hover_text("Horizontal angle of the sun")
                    .changed()
                {
                    self.needs_render = true;
                }

                if ui.add(egui::Slider::new(&mut self.light_elevation, 0.0..=std::f32::consts::PI)
                    .text("Sun Elevation"))
                    .on_hover_text("Height of the sun: 0 = horizon, π/2 = overhead, π = below")
                    .changed()
                {
                    self.needs_render = true;
                }

                if ui.add(egui::Slider::new(&mut self.height_scale, 0.5..=10.0)
                    .text("Relief"))
                    .on_hover_text("How pronounced terrain relief appears in lighting. 1 = subtle, 5 = dramatic")
                    .changed()
                {
                    self.needs_render = true;
                }

                ui.separator();
                egui::CollapsingHeader::new("Render Layers")
                    .default_open(true)
                    .show(ui, |ui| {
                    for (flag, label, tip) in [
                        (&mut self.show_water, "Water / Ocean", "Ocean surface with depth shading and specular"),
                        (&mut self.show_ice, "Ice Caps", "Polar and altitude ice rendering"),
                        (&mut self.show_biomes, "Biome Colors", "Temperature/moisture-driven biome coloring"),
                        (&mut self.show_clouds, "Clouds", "Cloud layer rendering"),
                        (&mut self.show_atmosphere, "Atmosphere", "Atmospheric scattering (blue limb glow, red sunsets)"),
                        (&mut self.show_cities, "City Lights", "Night-side city lights and day-side urban patches"),
                    ] {
                        if ui.checkbox(flag, label).on_hover_text(tip).changed() {
                            self.needs_render = true;
                        }
                    }
                    if ui.checkbox(&mut self.show_erosion, "Erosion")
                        .on_hover_text("Hydraulic erosion carving rivers and valleys")
                        .changed()
                    {
                        self.needs_terrain = true;
                    }

                    ui.separator();
                    // View mode selection
                    let export_views: &[(u32, &str)] = &[
                        (0, "ALL"), (1, "Height"), (7, "Roughness"),
                        (9, "Clouds"), (10, "Emission"), (13, "Normals"),
                    ];
                    ui.label("Export Maps");
                    ui.horizontal_wrapped(|ui| {
                        for &(idx, label) in export_views {
                            if ui.selectable_label(self.view_mode == idx, label).clicked() {
                                self.view_mode = idx;
                                self.needs_render = true;
                            }
                        }
                    });

                    let debug_views: &[(u32, &str)] = &[
                        (8, "AO"), (6, "Plates"), (2, "Temp"), (3, "Moisture"),
                        (4, "Biome"), (5, "Ocean/Ice"), (11, "Boundary"), (12, "Snow"),
                    ];
                    ui.label("Debug Views");
                    ui.horizontal_wrapped(|ui| {
                        for &(idx, label) in debug_views {
                            if ui.selectable_label(self.view_mode == idx, label).clicked() {
                                self.view_mode = idx;
                                self.needs_render = true;
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
                                if self.preview_resolution != *res {
                                    self.preview_resolution = *res;
                                    self.needs_terrain = true;
                                }
                            }
                        }
                    });

                    if ui.button("Reset rotation").clicked() {
                        self.rotation_y = 0.0;
                        self.rotation_x = 0.0;
                        self.needs_render = true;
                    }
                });

                ui.separator();
                ui.label("Clouds");
                if ui.add(egui::Slider::new(&mut self.cloud_coverage, 0.0..=1.0)
                    .text("Coverage"))
                    .on_hover_text("Cloud coverage fraction. 0 = clear sky, 1 = heavy overcast")
                    .changed()
                {
                    self.needs_render = true;
                }
                ui.horizontal(|ui| {
                    if ui.small_button("🎲").on_hover_text("Randomize cloud pattern").clicked() {
                        self.cloud_seed = rand_seed();
                        self.needs_render = true;
                    }
                    let mut seed_i64 = self.cloud_seed as i64;
                    if ui.add(egui::DragValue::new(&mut seed_i64).prefix("Seed: ")).changed() {
                        self.cloud_seed = seed_i64.clamp(0, u32::MAX as i64) as u32;
                        self.needs_render = true;
                    }
                });
                if ui.add(egui::Slider::new(&mut self.cloud_type, 0.0..=1.0)
                    .text("Type"))
                    .on_hover_text("Cloud style: 0 = smooth flowing stratus, 1 = puffy cumulus blobs")
                    .changed()
                {
                    self.needs_render = true;
                }
                let mut storms_i32 = self.storm_count as i32;
                if ui.add(egui::Slider::new(&mut storms_i32, 0..=8)
                    .text("Storms"))
                    .on_hover_text("Number of cyclone storm systems. 0 = none, 4 = Earth-like, 8 = stormy planet")
                    .changed()
                {
                    self.storm_count = storms_i32 as u32;
                    self.needs_render = true;
                }
                if self.storm_count > 0 {
                    if ui.add(egui::Slider::new(&mut self.storm_size, 0.3..=3.0)
                        .text("Storm Size"))
                        .on_hover_text("Storm radius: 0.3 = compact, 1.0 = Earth-like, 3.0 = massive")
                        .changed()
                    {
                        self.needs_render = true;
                    }
                }

                ui.separator();
                ui.label("Civilization");
                if ui.add(egui::Slider::new(&mut self.night_lights, 0.0..=1.0)
                    .text("Development"))
                    .on_hover_text("Urbanization level: 0 = pristine wilderness, 1 = heavily developed. Shows grey cities by day, lights at night")
                    .changed()
                {
                    self.needs_render = true;
                }
                if self.night_lights > 0.0 {
                    if ui.add(egui::Slider::new(&mut self.city_light_hue, 0.0..=1.0)
                        .text("Light Color"))
                        .on_hover_text("Night light color: 0 = warm amber (sodium), 0.5 = white (LED), 1.0 = cool blue (alien/futuristic)")
                        .changed()
                    {
                        self.needs_render = true;
                    }
                }

                ui.separator();
                egui::CollapsingHeader::new("Advanced Tweaks")
                    .default_open(false)
                    .show(ui, |ui| {
                        let mut plates_i32 = self.num_plates_override as i32;
                        if ui.add(egui::Slider::new(&mut plates_i32, 0..=30)
                            .text("Plates"))
                            .on_hover_text("Number of tectonic plates. 0 = auto from planet mass")
                            .changed()
                        {
                            self.num_plates_override = plates_i32 as u32;
                            self.needs_terrain = true;
                        }

                        if ui.add(egui::Slider::new(&mut self.mountain_scale, 0.0..=3.0)
                            .text("Mountain Height"))
                            .on_hover_text("Multiplier for tectonic mountain height. 0 = flat, 1 = default, 3 = extreme")
                            .changed()
                        {
                            self.needs_terrain = true;
                        }

                        if ui.add(egui::Slider::new(&mut self.boundary_width, 0.03..=0.30)
                            .text("Range Width"))
                            .on_hover_text("How wide mountain ranges spread from plate boundaries. Low = narrow ridges, high = broad highlands")
                            .changed()
                        {
                            self.needs_terrain = true;
                        }

                        if ui.add(egui::Slider::new(&mut self.warp_strength, 0.0..=3.0)
                            .text("Shape Warp"))
                            .on_hover_text("How organic plate boundaries look. 0 = geometric, 1 = default, 3 = very irregular")
                            .changed()
                        {
                            self.needs_terrain = true;
                        }

                        if ui.add(egui::Slider::new(&mut self.detail_scale, 0.0..=3.0)
                            .text("Detail"))
                            .on_hover_text("Fine terrain noise intensity. 0 = smooth, 1 = default, 3 = very rough")
                            .changed()
                        {
                            self.needs_terrain = true;
                        }

                        // Age override slider
                        let mut age_val = self.age_override.unwrap_or(self.derived.surface_age);
                        let mut use_override = self.age_override.is_some();
                        ui.horizontal(|ui| {
                            if ui.checkbox(&mut use_override, "").changed() {
                                if use_override {
                                    self.age_override = Some(age_val);
                                } else {
                                    self.age_override = None;
                                }
                                self.needs_terrain = true;
                            }
                            if ui.add(egui::Slider::new(&mut age_val, 0.0..=1.0)
                                .text("Surface Age"))
                                .on_hover_text("0 = young (sharp ridges, active volcanism), 1 = old (smooth peneplains). Checkbox = override physics")
                                .changed()
                            {
                                self.age_override = Some(age_val);
                                self.needs_terrain = true;
                            }
                        });

                        // Show derived physics info
                        ui.label(format!("Gravity: {:.1} m/s²  Tectonics: {:.0}%  Age: {:.2}",
                            self.derived.surface_gravity,
                            self.derived.tectonics_factor * 100.0,
                            self.age_override.unwrap_or(self.derived.surface_age),
                        ));

                        ui.separator();
                        if ui.add(egui::Slider::new(&mut self.star_color_temp, 0.0..=1.0)
                            .text("Star Color"))
                            .on_hover_text("Star type: 0 = hot blue (O/B), 0.5 = sun-like (G), 1.0 = red dwarf (M)")
                            .changed()
                        {
                            self.needs_render = true;
                        }
                    });

                ui.separator();
                ui.small(format!("GPU: {}", self.gpu.adapter_name()));
                ui.small("Drag to rotate • Scroll to zoom • Middle-drag to pan");
                }); // ScrollArea
            });

        // Right panel: Derived Properties + Export
        egui::SidePanel::right("info_panel")
            .resizable(true)
            .default_width(220.0)
            .show(ctx, |ui| {
                egui::ScrollArea::vertical().show(ui, |ui| {
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
                ui.heading("Export");
                ui.separator();

                ui.horizontal(|ui| {
                    ui.label("Name:");
                    ui.text_edit_singleline(&mut self.planet_name);
                });

                let export_resolutions: [(u32, &str); 3] = [
                    (2048, "2K"), (4096, "4K"), (8192, "8K"),
                ];
                ui.horizontal(|ui| {
                    ui.label("Resolution:");
                    for (res, label) in &export_resolutions {
                        if ui.selectable_label(self.export_resolution == *res, *label).clicked() {
                            self.export_resolution = *res;
                        }
                    }
                });

                ui.separator();
                ui.label("Export Layers:");
                ui.checkbox(&mut self.export_albedo, "Albedo (with AO)");
                ui.checkbox(&mut self.export_roughness, "Roughness");
                ui.checkbox(&mut self.export_clouds, "Clouds");
                ui.checkbox(&mut self.export_height, "Height");
                ui.checkbox(&mut self.export_emission, "Emission (city lights)");
                ui.checkbox(&mut self.export_water_mask, "Water Mask");
                ui.checkbox(&mut self.export_normals, "Normals");

                ui.separator();

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
                }); // ScrollArea
            });

        egui::CentralPanel::default().show(ctx, |ui| {
            if let Some(ref tex) = self.texture_handle {
                // Square image aligned left, 1:1 pixel ratio
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

                // Left-drag: rotate planet
                if response.dragged_by(egui::PointerButton::Primary) {
                    let delta = response.drag_delta();
                    self.rotation_y += delta.x * 0.01;
                    self.rotation_x -= delta.y * 0.01;
                    self.rotation_x = self.rotation_x.clamp(
                        -std::f32::consts::FRAC_PI_2 + 0.1,
                        std::f32::consts::FRAC_PI_2 - 0.1,
                    );
                    self.needs_render = true;
                }

                // Middle-drag: pan viewport
                if response.dragged_by(egui::PointerButton::Middle) {
                    let delta = response.drag_delta();
                    let shorter = available.x.min(available.y);
                    let ndc_per_pixel = 2.0 / (0.85 * shorter);
                    self.pan[0] += delta.x * ndc_per_pixel;
                    self.pan[1] += delta.y * ndc_per_pixel;
                    self.needs_render = true;
                }

                // Scroll: zoom toward cursor position
                if response.hovered() {
                    let scroll = ui.input(|i| i.smooth_scroll_delta.y);
                    if scroll != 0.0 {
                        let zoom_old = self.zoom;
                        let zoom_new = (zoom_old * (1.0 + scroll * 0.005)).clamp(0.1, 20.0);
                        // Keep the NDC point under the cursor fixed during zoom
                        if let Some(cursor_pos) = response.hover_pos() {
                            let rect = response.rect;
                            let cx = (cursor_pos.x - rect.min.x) / rect.width() - 0.5;
                            let cy = (cursor_pos.y - rect.min.y) / rect.height() - 0.5;
                            let sndc_x = cx * 2.0 / 0.85;
                            let sndc_y = cy * 2.0 / 0.85;
                            let ratio = zoom_new / zoom_old;
                            self.pan[0] = sndc_x - (sndc_x - self.pan[0]) * ratio;
                            self.pan[1] = sndc_y - (sndc_y - self.pan[1]) * ratio;
                        }
                        self.zoom = zoom_new;
                        self.needs_render = true;
                    }
                }

                // Double-click: reset zoom and pan
                if response.double_clicked_by(egui::PointerButton::Middle) {
                    self.zoom = 1.0;
                    self.pan = [0.0, 0.0];
                    self.needs_render = true;
                }
            } else {
                ui.centered_and_justified(|ui| {
                    ui.spinner();
                    ui.label("Generating terrain...");
                });
            }

            // Loading overlay — only show if generation takes >1s
            let gen_elapsed = self.terrain_start.map(|t| t.elapsed().as_secs_f32()).unwrap_or(0.0);
            if (self.terrain_pending || self.erosion_remaining > 0) && gen_elapsed > 1.0 {
                let panel_rect = ui.max_rect();
                let painter = ui.painter();
                painter.rect_filled(
                    panel_rect,
                    0.0,
                    egui::Color32::from_rgba_premultiplied(0, 0, 0, 140),
                );
                painter.text(
                    panel_rect.center(),
                    egui::Align2::CENTER_CENTER,
                    format!("Generating terrain ({}px)...", self.preview_resolution),
                    egui::FontId::proportional(18.0),
                    egui::Color32::WHITE,
                );
            }
        });

        // Two-frame terrain generation:
        // Frame 1: needs_terrain=true → set terrain_pending, paint overlay, request repaint
        // Frame 2: terrain_pending=true → do the actual blocking work
        // This ensures the overlay is visible before the UI freezes.
        if self.needs_terrain && !self.terrain_pending {
            self.terrain_pending = true;
            self.needs_terrain = false;
            self.terrain_start = Some(std::time::Instant::now());
            ctx.request_repaint();
        } else if self.terrain_pending {
            self.terrain_pending = false;
            self.regenerate_terrain();
        }
        // Progressive erosion: apply one batch per frame, but skip when mouse is
        // pressed to avoid blocking input processing (prevents slider sticking)
        let mouse_busy = ctx.input(|i| {
            i.pointer.any_pressed() || i.pointer.any_down()
        });
        if self.erosion_remaining > 0 && !mouse_busy {
            self.erode_batch();
            ctx.request_repaint();
        } else if self.erosion_remaining > 0 {
            ctx.request_repaint(); // retry next frame when mouse released
        }
        if self.needs_render {
            self.render_preview(ctx);
        }
    }
}

fn rand_seed() -> u32 {
    use std::time::{SystemTime, UNIX_EPOCH};
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .subsec_nanos()
}
