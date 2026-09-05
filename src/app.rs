use eframe::egui;
use std::sync::Arc;

use crate::export::{self, ExportConfig, ExportHandle, ExportLayers, ExportProgress};
use crate::gpu::GpuContext;
use crate::planet::{DerivedProperties, PlanetParams};
use crate::plates::{PlateGenParams, generate_plates};
use crate::preview::{PreviewRenderer, PreviewUniforms};
use crate::terrain_compute::{
    DynamicsTextures, ErosionPipeline, TerrainComputePipeline, TerrainGenerationParams,
    WindFieldPipeline,
};
use crate::weather::{
    DEFAULT_WEATHER_RESOLUTION, WeatherFieldPipeline, WeatherLifecycle, WeatherSnapshot,
};

pub struct PlanetGenApp {
    gpu: Arc<GpuContext>,
    preview_renderer: PreviewRenderer,
    terrain_compute: TerrainComputePipeline,
    erosion_pipeline: Option<ErosionPipeline>,
    wind_pipeline: WindFieldPipeline,
    dynamics: Option<DynamicsTextures>,
    weather_pipeline: WeatherFieldPipeline,
    weather: Option<WeatherLifecycle>,
    weather_terrain: Option<crate::terrain_compute::TectonicTerrain>,
    texture_id: egui::TextureId,
    render_state: eframe::egui_wgpu::RenderState,
    params: PlanetParams,
    derived: DerivedProperties,
    /// View-space accumulated planet orientation (rows of an orthogonal 3×3).
    /// Incremental drag rotations are *pre-multiplied* in screen axes, so
    /// up/down always tilts about the viewer's horizontal — never a yawed-away
    /// planet axis as Euler-angle accumulation would do.
    rot: [[f32; 3]; 3],
    // Visual override parameters
    continental_scale: f32,
    water_loss: f32,
    climate_moisture: f32, // 0=bone dry atmosphere, 1=full moisture from physics
    season: f32,           // 0=winter, 0.5=equinox, 1=summer
    erosion_iterations: u32,
    light_azimuth: f32,    // sun horizontal angle in radians
    light_elevation: f32,  // sun vertical angle in radians
    height_scale: f32,     // normal map height exaggeration
    show_atmosphere: bool, // toggle atmosphere rendering
    show_ao: bool,         // toggle ambient occlusion
    // Layer toggles for Normal view
    show_water: bool,
    show_ice: bool,
    show_biomes: bool,
    show_clouds: bool,
    show_cloud_shadows: bool,
    show_wind_effects: bool, // false makes weather transport calm; continentality remains active
    show_cities: bool,
    show_erosion: bool,
    zoom: f32,     // viewport zoom level
    pan: [f32; 2], // viewport pan in NDC units
    // Advanced terrain tweaks
    mountain_scale: f32,
    boundary_width: f32,
    warp_strength: f32,
    detail_scale: f32,
    age_override: Option<f32>, // None = derived from physics, Some = manual override
    num_plates_override: u32,  // 0 = auto from physics
    num_continents: u32,       // target number of distinct landmasses (1-10)
    continent_size_variety: f32, // 0 = equal sizes, 1 = heavily skewed
    cloud_coverage: f32,
    cloud_seed: u32,
    cloud_opacity: f32,
    wind_scale: f32,
    lava_glow: f32,    // tectonic emission intensity (0.0-1.0)
    ring_inner: f32,   // ring inner radius (planet radii)
    ring_outer: f32,   // ring outer radius
    ring_tilt: f32,    // ring tilt (degrees)
    ring_opacity: f32, // ring opacity
    storm_count: u32,
    storm_size: f32,
    night_lights: f32,
    star_color_temp: f32,
    city_light_hue: f32,
    view_mode: u32,
    // UI shell state
    active_tab: usize,
    show_help: bool,
    weather_busy: bool,
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
    export_done_ok: bool,
    export_destination: String,
    export_layers: String,
    gpu_error: Option<String>,
    /// Canvas rect captured during the current frame's CentralPanel layout;
    /// HUD chips anchor inside it, not inside the whole window.
    hud_rect: egui::Rect,
}

fn derive_terrain_params(
    params: &PlanetParams,
    derived: &DerivedProperties,
    continental_scale: f32,
    mountain_scale: f32,
    boundary_width: f32,
    warp_strength: f32,
    detail_scale: f32,
    age_override: Option<f32>,
    num_plates_override: u32,
    num_continents: u32,
    continent_size_variety: f32,
) -> TerrainGenerationParams {
    let dist_factor = (params.star_distance_au.ln() / 3.0_f32.ln()).clamp(0.0, 1.0);
    let beta = (1.47 + 0.91 * dist_factor + 0.3 * params.metallicity).clamp(1.2, 3.0);
    let gain = 2.0_f32.powf(-(beta - 1.0) / 2.0);
    let amplitude = 0.6 + 0.6 * params.mass_earth.powf(0.3).min(2.0);
    let frequency = (1.0 + 0.5 * params.mass_earth.powf(0.2)) * continental_scale;
    let octaves = (8.0 + 4.0 * (params.axial_tilt_deg / 90.0) * derived.tectonics_factor) as u32;
    let lacunarity = 1.9 + 0.2 * (24.0 / params.rotation_period_h).clamp(0.5, 2.0);

    TerrainGenerationParams {
        seed: params.seed,
        amplitude,
        frequency,
        octaves,
        gain,
        lacunarity,
        mountain_scale,
        boundary_width,
        warp_strength,
        detail_scale,
        surface_gravity: derived.surface_gravity,
        tectonics_factor: derived.tectonics_factor,
        surface_age: age_override.unwrap_or(derived.surface_age),
        continental_scale,
        num_plates_override,
        num_continents,
        continent_size_variety,
    }
}

impl PlanetGenApp {
    pub fn new(cc: &eframe::CreationContext<'_>) -> Result<Self, String> {
        crate::ui::apply_theme(&cc.egui_ctx);
        let render_state = cc
            .wgpu_render_state
            .as_ref()
            .ok_or_else(|| "eframe did not provide the required wgpu render state".to_owned())?;
        let gpu = Arc::new(GpuContext::from_eframe(render_state));
        let preview_renderer = PreviewRenderer::new(&gpu);
        let texture_id = render_state.renderer.write().register_native_texture(
            &gpu.device,
            preview_renderer.target_view(),
            wgpu::FilterMode::Linear,
        );
        let terrain_compute = TerrainComputePipeline::new(&gpu);
        let erosion_pipeline = Some(ErosionPipeline::new(&gpu));
        let wind_pipeline = WindFieldPipeline::new(&gpu)?;
        let weather_pipeline = WeatherFieldPipeline::new(&gpu)?;
        let params = PlanetParams::default();
        let derived = DerivedProperties::from_params(&params);
        let default_cloud_seed = params.seed.wrapping_add(1000);
        Ok(Self {
            gpu,
            preview_renderer,
            terrain_compute,
            erosion_pipeline,
            wind_pipeline,
            dynamics: None,
            weather_pipeline,
            weather: None,
            weather_terrain: None,
            texture_id,
            render_state: render_state.clone(),
            params,
            derived,
            rot: IDENTITY3,
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
            show_cloud_shadows: true,
            show_wind_effects: true,
            show_cities: true,
            show_erosion: false,
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
            cloud_opacity: 1.0,
            wind_scale: 1.0,
            lava_glow: 0.0,
            ring_inner: 0.0,
            ring_outer: 0.0,
            ring_tilt: 15.0,
            ring_opacity: 0.7,
            storm_count: 2,
            storm_size: 1.0,
            night_lights: 0.0,
            star_color_temp: 0.5,
            city_light_hue: 0.0,
            view_mode: 0,
            active_tab: 0,
            show_help: false,
            weather_busy: false,
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
            export_done_ok: false,
            export_destination: String::new(),
            export_layers: String::new(),
            gpu_error: None,
            hud_rect: egui::Rect::NOTHING,
        })
    }

    fn build_uniforms(&self) -> PreviewUniforms {
        let ocean_level = self.ocean_level();

        let weather_snapshot = self
            .weather
            .as_ref()
            .and_then(WeatherLifecycle::front_snapshot);

        PreviewUniforms {
            rotation: [
                [self.rot[0][0], self.rot[0][1], self.rot[0][2], 0.0],
                [self.rot[1][0], self.rot[1][1], self.rot[1][2], 0.0],
                [self.rot[2][0], self.rot[2][1], self.rot[2][2], 0.0],
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
            atmosphere_density: if self.show_atmosphere {
                self.derived.atmosphere_strength
            } else {
                0.0
            },
            atmosphere_height: 0.02 + 0.02 * self.derived.atmosphere_strength,
            height_scale: self.height_scale,
            zoom: self.zoom,
            pan_x: self.pan[0],
            pan_y: self.pan[1],
            cloud_coverage: weather_snapshot
                .map(|snapshot| snapshot.coverage)
                .unwrap_or(self.cloud_coverage),
            cloud_seed: weather_snapshot
                .map(|snapshot| snapshot.seed)
                .unwrap_or(self.cloud_seed),
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
            cloud_opacity: self.cloud_opacity,
            cloud_advection: if self.show_wind_effects { 1.0 } else { 0.0 },
            rotation_rate: self.derived.rotation_rate_rad_s / (std::f32::consts::TAU / 86400.0),
            atm_pressure: self.derived.surface_pressure_bar,
            _pad4: 0.0,
            lava_glow: self.lava_glow,
            ring_inner: self.ring_inner,
            ring_outer: self.ring_outer,
            ring_tilt: self.ring_tilt.to_radians(),
            ring_opacity: self.ring_opacity,
            planet_radius_km: weather_snapshot
                .map(|snapshot| snapshot.radius_km)
                .unwrap_or(self.derived.radius_km),
            show_cloud_shadows: if self.show_cloud_shadows { 1.0 } else { 0.0 },
            _pad5: 0.0,
        }
    }

    fn terrain_params(&self) -> TerrainGenerationParams {
        derive_terrain_params(
            &self.params,
            &self.derived,
            self.continental_scale,
            self.mountain_scale,
            self.boundary_width,
            self.warp_strength,
            self.detail_scale,
            self.age_override,
            self.num_plates_override,
            self.num_continents,
            self.continent_size_variety,
        )
    }

    fn regenerate_terrain(&mut self) {
        // Install custom wgpu error handler that stores errors instead of panicking
        self.gpu
            .device
            .push_error_scope(wgpu::ErrorFilter::Validation);
        self.gpu
            .device
            .push_error_scope(wgpu::ErrorFilter::OutOfMemory);

        use std::time::Instant;
        let t0 = Instant::now();

        let plates = generate_plates(&PlateGenParams {
            seed: self.params.seed,
            mass_earth: self.params.mass_earth,
            ocean_fraction: self.derived.ocean_fraction * (1.0 - self.water_loss),
            tectonics_factor: self.derived.tectonics_factor,
            continental_scale: self.continental_scale,
            num_plates_override: self.num_plates_override,
            num_continents: self.num_continents,
            continent_size_variety: self.continent_size_variety,
        });

        let terrain_params = self.terrain_params();
        let terrain = self.terrain_compute.generate(
            &self.gpu,
            &plates,
            self.preview_resolution,
            terrain_params.seed,
            terrain_params.amplitude,
            terrain_params.frequency,
            terrain_params.octaves,
            terrain_params.gain,
            terrain_params.lacunarity,
            terrain_params.mountain_scale,
            terrain_params.boundary_width,
            terrain_params.warp_strength,
            terrain_params.detail_scale,
            terrain_params.surface_gravity,
            terrain_params.tectonics_factor,
            terrain_params.surface_age,
            terrain_params.continental_scale,
        );

        let effective_ocean = self.derived.ocean_fraction * (1.0 - self.water_loss);
        let ocean_level = -0.5 + 1.7 * effective_ocean;

        // Show un-eroded terrain immediately
        self.cached_cubemap_view = Some(self.preview_renderer.upload_terrain(&self.gpu, &terrain));

        // Generate pressure-based wind field (for debug views + continentality cubemap)
        {
            let cloud_res = (self.preview_resolution / 2).max(192);
            let t_wind = std::time::Instant::now();

            if self.dynamics.as_ref().map(|textures| textures.resolution) != Some(cloud_res) {
                self.dynamics = Some(self.wind_pipeline.create_textures(
                    &self.gpu,
                    cloud_res,
                    &terrain,
                    ocean_level,
                ));
            }
            let dynamics = self.dynamics.as_ref().unwrap();
            let weather = self.weather_snapshot(cloud_res, ocean_level);
            self.wind_pipeline.generate_gpu(
                &self.gpu,
                &terrain,
                dynamics,
                weather.seed,
                weather.ocean_level,
                weather.axial_tilt_rad,
                weather.season,
                weather.rotation_rate_rad_s,
                weather.base_temp_c,
                weather.surface_pressure_bar,
                weather.wind_scale,
            );
            log::info!(
                "[wind {}px] {:.0}ms",
                cloud_res,
                t_wind.elapsed().as_secs_f64() * 1000.0
            );
        }

        // Schedule progressive erosion (skipped when erosion layer is disabled)
        if self.show_erosion {
            let adaptive_iters = match self.preview_resolution {
                r if r <= 256 => (self.erosion_iterations as f32 * 0.2) as u32,
                r if r <= 512 => (self.erosion_iterations as f32 * 0.4) as u32,
                r if r <= 768 => (self.erosion_iterations as f32 * 0.6) as u32,
                _ => self.erosion_iterations,
            }
            .max(1);
            self.erosion_terrain = Some(terrain);
            self.erosion_remaining = adaptive_iters;
            self.erosion_ocean_level = ocean_level;
        } else {
            self.weather_terrain = Some(terrain);
            self.request_weather(ocean_level);
            self.dispatch_weather();
            self.erosion_remaining = 0;
        }

        log::info!(
            "[terrain {}px] plates+compute: {:.0}ms, {} plates ({}c/{}o), continents={}, variety={:.2}, scheduling {} erosion iters",
            self.preview_resolution,
            t0.elapsed().as_secs_f64() * 1000.0,
            plates.len(),
            plates.iter().filter(|p| p.plate_type > 0.5).count(),
            plates.iter().filter(|p| p.plate_type <= 0.5).count(),
            self.num_continents,
            self.continent_size_variety,
            self.erosion_remaining,
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
            if let Err(error) = self
                .erosion_pipeline
                .as_ref()
                .expect("procedural terrain owns erosion pipeline")
                .erode(&self.gpu, terrain, iters, self.erosion_ocean_level)
            {
                self.gpu_error = Some(error.to_string());
                self.erosion_remaining = 0;
                return;
            }
            self.cached_cubemap_view =
                Some(self.preview_renderer.upload_terrain(&self.gpu, terrain));
            self.erosion_remaining -= iters;

            log::info!(
                "[erosion batch] {} iters in {:.0}ms, {} remaining",
                iters,
                t.elapsed().as_secs_f64() * 1000.0,
                self.erosion_remaining,
            );
        }

        if self.erosion_remaining == 0 {
            self.weather_terrain = self.erosion_terrain.take();
            self.terrain_start = None;
            self.request_weather(self.erosion_ocean_level);
            self.dispatch_weather();
        }
        self.needs_render = true;

        // Check for GPU errors (OOM, validation)
        if let Some(err) = pollster::block_on(self.gpu.device.pop_error_scope()) {
            self.gpu_error = Some(format!("GPU OOM: {err}"));
        }
        if let Some(err) = pollster::block_on(self.gpu.device.pop_error_scope()) {
            self.gpu_error = Some(format!("GPU validation: {err}"));
        }
    }

    fn render_preview(&mut self) {
        if let Some(ref cubemap_view) = self.cached_cubemap_view {
            let uniforms = self.build_uniforms();
            let size = self.preview_resolution;
            // Debug views 16/17 use continentality/pressure cubemap in the cloud_tex slot
            let cloud_ref = self.dynamics.as_ref().map(|dynamics| match self.view_mode {
                17 => &dynamics.pressure,
                _ => &dynamics.wind_continentality,
            });
            let renderer = self.render_state.renderer.clone();
            let texture_id = self.texture_id;
            self.preview_renderer
                .resize_target(&self.gpu, size, |view| {
                    renderer.write().update_egui_texture_from_wgpu_texture(
                        &self.gpu.device,
                        view,
                        wgpu::FilterMode::Linear,
                        texture_id,
                    );
                });
            let weather_views = self
                .weather
                .as_ref()
                .map(|weather| (&weather.front().mass, &weather.front().geometry));
            self.preview_renderer.render_interactive(
                &self.gpu,
                &uniforms,
                cubemap_view,
                cloud_ref,
                weather_views,
            );
        }
        self.needs_render = false;
    }

    fn request_weather(&mut self, ocean_level: f32) {
        if self.weather.is_none() {
            self.weather = Some(WeatherLifecycle::new(
                &self.weather_pipeline,
                &self.gpu,
                DEFAULT_WEATHER_RESOLUTION,
            ));
        }
        let resolution = self.weather.as_ref().unwrap().front().resolution;
        let snapshot = self.weather_snapshot(resolution, ocean_level);
        self.weather.as_mut().unwrap().request(snapshot);
    }

    fn weather_snapshot(&self, resolution: u32, ocean_level: f32) -> WeatherSnapshot {
        WeatherSnapshot {
            face: 0,
            resolution,
            seed: self.cloud_seed,
            storm_count: self.storm_count,
            coverage: self.cloud_coverage,
            moisture: self.climate_moisture,
            surface_pressure_bar: self.derived.surface_pressure_bar,
            base_temp_c: self.derived.base_temperature_c,
            ocean_level,
            axial_tilt_rad: self.params.axial_tilt_deg.to_radians(),
            season: self.season,
            storm_size: self.storm_size,
            radius_km: self.derived.radius_km,
            rotation_rate_rad_s: self.derived.rotation_rate_rad_s,
            wind_scale: if self.show_wind_effects {
                self.wind_scale
            } else {
                0.0
            },
        }
    }

    fn invalidate_weather(&mut self) {
        let ocean_level = self.ocean_level();
        self.request_weather(ocean_level);
        self.needs_render = true;
    }

    fn dispatch_weather(&mut self) {
        let Some(terrain) = self.weather_terrain.as_ref() else {
            return;
        };
        let Some(dynamics) = self.dynamics.as_ref() else {
            return;
        };
        let Some(weather) = self.weather.as_mut() else {
            return;
        };
        let Some((revision, snapshot)) = weather.next_submission() else {
            return;
        };
        self.weather_pipeline
            .generate(&self.gpu, snapshot, terrain, dynamics, weather.back());
        weather.mark_submitted(&self.gpu.queue, revision);
    }

    fn poll_weather(&mut self) -> bool {
        let _ = self.gpu.device.poll(wgpu::PollType::Poll);
        if self.weather.as_mut().is_some_and(WeatherLifecycle::poll) {
            self.needs_render = true;
        }
        self.dispatch_weather();
        self.weather.as_ref().is_some_and(WeatherLifecycle::is_busy)
    }

    fn update_derived(&mut self) {
        self.derived = DerivedProperties::from_params(&self.params);
    }

    fn ocean_level(&self) -> f32 {
        -0.5 + 1.7 * self.derived.ocean_fraction * (1.0 - self.water_loss)
    }

    fn start_export(&mut self) {
        // Capture authored weather inputs at click time. Presentation-only controls are
        // deliberately absent from WeatherSnapshot, so later visibility/opacity edits
        // cannot change this export.
        let weather = self.weather_snapshot(
            self.weather
                .as_ref()
                .map_or(DEFAULT_WEATHER_RESOLUTION, |weather| {
                    weather.front().resolution
                }),
            self.ocean_level(),
        );
        let output_dir = std::env::current_dir().unwrap_or_default().join("output");
        self.export_destination = output_dir.join(&self.planet_name).display().to_string();
        self.export_layers = self
            .export_layer_flags()
            .iter()
            .zip(EXPORT_LAYERS)
            .filter_map(|(enabled, (_, file))| enabled.then_some(*file))
            .collect::<Vec<_>>()
            .join(", ");
        let config = ExportConfig {
            face_resolution: self.export_resolution,
            tile_size: export::TILE_SIZE,
            output_dir,
            planet_name: self.planet_name.clone(),
            erosion_iterations: self.erosion_iterations,
            layers: ExportLayers {
                height: self.export_height,
                albedo: self.export_albedo,
                normals: self.export_normals,
                roughness: self.export_roughness,
                water_mask: self.export_water_mask,
                clouds: self.export_clouds,
                emission: self.export_emission,
            },
            weather,
            night_lights: self.night_lights,
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
        self.export_status = format!(
            "Starting export to {} (layers: {}).",
            self.export_destination, self.export_layers
        );
        self.export_progress = 0.0;
        self.export_done_ok = false;
    }

    fn poll_export(&mut self) {
        // Interactive export shares eframe's device and queue. Poll that shared device
        // from the update loop so async export readbacks progress without creating a
        // competing standalone context.
        let _ = self.gpu.device.poll(wgpu::PollType::Poll);
        let mut finished = false;
        if let Some(ref handle) = self.export_handle {
            while let Ok(progress) = handle.progress_rx.try_recv() {
                match progress {
                    ExportProgress::Progress { message, fraction } => {
                        self.export_status = message;
                        self.export_progress = fraction;
                    }
                    ExportProgress::Complete => {
                        self.export_status = format!(
                            "Export complete: wrote {} (layers: {}).",
                            self.export_destination, self.export_layers
                        );
                        self.export_progress = 1.0;
                        self.export_done_ok = true;
                        finished = true;
                    }
                    ExportProgress::Error(e) => {
                        self.export_status = if e == "Cancelled" {
                            format!(
                                "Export cancelled for {} (layers: {}). Recovery: adjust settings and retry.",
                                self.export_destination, self.export_layers
                            )
                        } else {
                            format!(
                                "Export failed for {} (layers: {}). {e} Recovery: check disk space or GPU, then retry.",
                                self.export_destination, self.export_layers
                            )
                        };
                        self.export_progress = 0.0;
                        self.export_done_ok = false;
                        finished = true;
                    }
                }
            }
        }
        if finished {
            self.export_handle = None;
        }
    }

    fn reset_preview(&mut self) {
        self.rot = IDENTITY3;
        self.zoom = 1.0;
        self.pan = [0.0, 0.0];
        self.needs_render = true;
    }

    /// Apply a view-space rotation: tilt by `pitch` about the viewer's
    /// horizontal axis and yaw by `yaw` about the viewer's vertical axis,
    /// composed onto the current orientation (pre-multiply). No Euler clamp —
    /// the accumulated matrix has no gimbal singularity; RESET VIEW returns
    /// to identity.
    fn apply_view_rotation(&mut self, pitch: f32, yaw: f32) {
        if pitch == 0.0 && yaw == 0.0 {
            return;
        }
        let d = rot3_view_delta(pitch, yaw);
        self.rot = rot3_orthonormalize(&rot3_mul(&d, &self.rot));
    }

    fn handle_keyboard_shortcuts(&mut self, ctx: &egui::Context) {
        if ctx.wants_keyboard_input() {
            return;
        }
        let (randomize_seed, reset, left, right, up, down, zoom_in, zoom_out, help, escape) =
            ctx.input(|input| {
                (
                    input.key_pressed(egui::Key::N),
                    input.key_pressed(egui::Key::R),
                    input.key_pressed(egui::Key::ArrowLeft),
                    input.key_pressed(egui::Key::ArrowRight),
                    input.key_pressed(egui::Key::ArrowUp),
                    input.key_pressed(egui::Key::ArrowDown),
                    input.key_pressed(egui::Key::Plus) || input.key_pressed(egui::Key::Equals),
                    input.key_pressed(egui::Key::Minus),
                    input.key_pressed(egui::Key::F1)
                        || (input.key_pressed(egui::Key::Slash) && input.modifiers.shift),
                    input.key_pressed(egui::Key::Escape),
                )
            });

        if help {
            self.show_help = !self.show_help;
        }
        if escape && self.show_help {
            self.show_help = false;
        }
        if randomize_seed {
            self.params.seed = rand_seed();
            self.planet_name = format!("planet_{}", self.params.seed);
            self.update_derived();
            self.needs_terrain = true;
        }
        if reset {
            self.reset_preview();
        }
        let rotation_step = 0.1;
        self.apply_view_rotation(
            rotation_step * (down as i8 - up as i8) as f32,
            rotation_step * (left as i8 - right as i8) as f32,
        );
        if left || right || up || down {
            self.needs_render = true;
        }
        if zoom_in || zoom_out {
            self.zoom = (self.zoom * if zoom_in { 1.1 } else { 1.0 / 1.1 }).clamp(0.1, 20.0);
            self.needs_render = true;
        }
    }
}

// ============================================================================
// UI shell — instrument panel.
// Layout: top bar / tabbed left rail / viewport (header + HUD) / right
// inspector / bottom status line. Design lineage and rationale:
// docs/plans/2026-09-05-001-feat-ui-redesign-instrument-panel-plan.md
// ============================================================================

use crate::ui::theme;
use crate::ui::widgets::{self, Param};
use eframe::egui::RichText;

#[derive(Clone, Copy, PartialEq)]
enum ViewGroup {
    Shaded,
    Maps,
    Debug,
}

/// Single source of truth for viewport view modes: drives the header chips,
/// the HUD badge, and debug texture-slot selection. Adding a map = one row.
struct ViewMode {
    id: u32,
    label: &'static str,
    group: ViewGroup,
}

const VIEW_MODES: &[ViewMode] = &[
    ViewMode { id: 0, label: "ALL", group: ViewGroup::Shaded },
    ViewMode { id: 1, label: "HEIGHT", group: ViewGroup::Maps },
    ViewMode { id: 7, label: "ROUGHNESS", group: ViewGroup::Maps },
    ViewMode { id: 9, label: "CLOUDS", group: ViewGroup::Maps },
    ViewMode { id: 10, label: "EMISSION", group: ViewGroup::Maps },
    ViewMode { id: 13, label: "NORMALS", group: ViewGroup::Maps },
    ViewMode { id: 8, label: "AO", group: ViewGroup::Debug },
    ViewMode { id: 6, label: "PLATES", group: ViewGroup::Debug },
    ViewMode { id: 2, label: "TEMP", group: ViewGroup::Debug },
    ViewMode { id: 3, label: "MOISTURE", group: ViewGroup::Debug },
    ViewMode { id: 4, label: "BIOME", group: ViewGroup::Debug },
    ViewMode { id: 5, label: "OCEAN/ICE", group: ViewGroup::Debug },
    ViewMode { id: 11, label: "BOUNDARY", group: ViewGroup::Debug },
    ViewMode { id: 12, label: "SNOW", group: ViewGroup::Debug },
    ViewMode { id: 14, label: "WIND", group: ViewGroup::Debug },
    ViewMode { id: 15, label: "CURRENTS", group: ViewGroup::Debug },
    ViewMode { id: 16, label: "CONTINENTALITY", group: ViewGroup::Debug },
    ViewMode { id: 17, label: "PRESSURE", group: ViewGroup::Debug },
];

fn view_mode_info(id: u32) -> &'static ViewMode {
    VIEW_MODES.iter().find(|v| v.id == id).unwrap_or(&VIEW_MODES[0])
}

fn view_opts(group: ViewGroup) -> Vec<(u32, &'static str)> {
    VIEW_MODES
        .iter()
        .filter(|v| v.group == group)
        .map(|v| (v.id, v.label))
        .collect()
}

/// Single source of truth for export layers: drives the checklist and the
/// summary string. Order must match `export_layer_flags`.
const EXPORT_LAYERS: &[(&str, &str)] = &[
    ("Albedo", "albedo.exr + ao.png"),
    ("Roughness", "roughness.png"),
    ("Clouds", "clouds.exr · 6ch"),
    ("Height", "height.exr"),
    ("Emission", "emission.exr"),
    ("Water mask", "water_mask.png"),
    ("Normals", "normal.exr"),
];

const TABS: [&str; 5] = ["PLANET", "TERRAIN", "CLIMATE", "LOOK", "RENDER"];

const PRESETS: [&str; 7] = [
    "EARTH",
    "MARS",
    "OCEAN",
    "SNOWBALL",
    "HOTHOUSE",
    "VOLCANIC",
    "RINGED",
];

impl PlanetGenApp {
    fn export_layer_flags(&self) -> [bool; EXPORT_LAYERS.len()] {
        [
            self.export_albedo,
            self.export_roughness,
            self.export_clouds,
            self.export_height,
            self.export_emission,
            self.export_water_mask,
            self.export_normals,
        ]
    }

    /// Reset visual overrides to factory defaults before applying a preset.
    fn preset_base(&mut self) {
        self.params = PlanetParams::default();
        self.continental_scale = 1.0;
        self.water_loss = 0.5;
        self.climate_moisture = 1.0;
        self.season = 0.5;
        self.erosion_iterations = 25;
        self.height_scale = 3.0;
        self.mountain_scale = 1.0;
        self.boundary_width = 0.10;
        self.warp_strength = 1.0;
        self.detail_scale = 1.0;
        self.age_override = None;
        self.num_plates_override = 0;
        self.num_continents = 4;
        self.continent_size_variety = 0.35;
        self.cloud_coverage = 0.5;
        self.cloud_opacity = 1.0;
        self.wind_scale = 1.0;
        self.lava_glow = 0.0;
        self.ring_inner = 0.0;
        self.ring_outer = 0.0;
        self.ring_tilt = 15.0;
        self.ring_opacity = 0.7;
        self.storm_count = 2;
        self.storm_size = 1.0;
        self.night_lights = 0.0;
        self.star_color_temp = 0.5;
        self.city_light_hue = 0.0;
    }

    fn apply_preset(&mut self, idx: usize) {
        self.preset_base();
        match idx {
            1 => {
                // Mars analog: small, cold, dry, dust-rough.
                self.params.star_distance_au = 1.52;
                self.params.mass_earth = 0.107;
                self.params.axial_tilt_deg = 25.2;
                self.params.rotation_period_h = 24.6;
                self.water_loss = 0.85;
                self.climate_moisture = 0.15;
                self.cloud_coverage = 0.15;
                self.erosion_iterations = 5;
                self.detail_scale = 1.3;
                self.mountain_scale = 1.2;
                self.height_scale = 4.0;
            }
            2 => {
                // Ocean world: no water loss, heavy cloud, few landmasses.
                self.water_loss = 0.0;
                self.cloud_coverage = 0.65;
                self.num_continents = 2;
                self.continent_size_variety = 0.25;
                self.erosion_iterations = 30;
            }
            3 => {
                // Snowball: far, tilted-cold axis, thin moisture.
                self.params.star_distance_au = 2.7;
                self.params.mass_earth = 0.9;
                self.params.axial_tilt_deg = 8.0;
                self.water_loss = 0.25;
                self.climate_moisture = 0.35;
                self.cloud_coverage = 0.45;
                self.erosion_iterations = 10;
                self.season = 0.15;
            }
            4 => {
                // Hothouse: close-in, heavy atmosphere, overcast.
                self.params.star_distance_au = 0.72;
                self.params.mass_earth = 1.6;
                self.climate_moisture = 0.95;
                self.cloud_coverage = 0.85;
                self.water_loss = 0.35;
                self.erosion_iterations = 40;
                self.season = 0.85;
            }
            5 => {
                // Volcanic: tectonically violent, lava emission, sparse water.
                self.params.mass_earth = 1.3;
                self.lava_glow = 0.85;
                self.mountain_scale = 2.2;
                self.num_plates_override = 26;
                self.detail_scale = 1.6;
                self.water_loss = 0.65;
                self.cloud_coverage = 0.3;
                self.erosion_iterations = 8;
            }
            6 => {
                // Ringed world: massive planet with prominent ring system.
                self.params.star_distance_au = 1.05;
                self.params.mass_earth = 1.6;
                self.ring_inner = 1.35;
                self.ring_outer = 2.35;
                self.ring_tilt = 18.0;
                self.ring_opacity = 0.75;
                self.cloud_coverage = 0.6;
            }
            _ => {} // 0 = Earth analog: factory defaults.
        }
        self.cloud_seed = self.params.seed.wrapping_add(1000);
        self.planet_name = format!("planet_{}", self.params.seed);
        self.update_derived();
        self.needs_terrain = true;
    }

    // ---- Shell regions ----------------------------------------------------

    fn top_bar(&mut self, ctx: &egui::Context) {
        egui::TopBottomPanel::top("top_bar").show(ctx, |ui| {
            ui.horizontal(|ui| {
                let mut wordmark = egui::text::LayoutJob::single_section(
                    "PLANET".to_owned(),
                    egui::TextFormat {
                        font_id: egui::FontId::monospace(14.0),
                        color: theme::TEXT,
                        ..Default::default()
                    },
                );
                wordmark.append(
                    "·GEN",
                    0.0,
                    egui::TextFormat {
                        font_id: egui::FontId::monospace(14.0),
                        color: theme::ACCENT,
                        ..Default::default()
                    },
                );
                ui.label(wordmark);
                ui.add_space(16.0);

                if widgets::primary_button(ui, "RANDOMIZE SEED", true).clicked() {
                    self.params.seed = rand_seed();
                    self.planet_name = format!("planet_{}", self.params.seed);
                    self.update_derived();
                    self.needs_terrain = true;
                }

                ui.add_space(8.0);
                ui.label(
                    RichText::new(&self.planet_name)
                        .monospace()
                        .size(12.0)
                        .color(theme::TEXT_FAINT),
                );

                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    if ui
                        .small_button(RichText::new("?  HELP").color(theme::TEXT_DIM))
                        .on_hover_text("Shortcuts and gestures (F1)")
                        .clicked()
                    {
                        self.show_help = !self.show_help;
                    }
                });
            });
        });
    }

    fn status_bar(&self, ctx: &egui::Context) {
        egui::TopBottomPanel::bottom("status_bar").show(ctx, |ui| {
            ui.horizontal(|ui| {
                let seg = |ui: &mut egui::Ui, text: String| {
                    ui.label(
                        RichText::new(text)
                            .monospace()
                            .size(10.5)
                            .color(theme::TEXT_FAINT),
                    );
                };
                let dot = |ui: &mut egui::Ui| {
                    ui.label(RichText::new("·").monospace().size(10.5).color(theme::EDGE_STRONG))
                };
                let gpu = self.gpu.adapter_name();
                let gpu_short: String = gpu.chars().take(26).collect();
                seg(ui, gpu_short);
                dot(ui);
                seg(ui, format!("PREVIEW {}px", self.preview_resolution));
                dot(ui);
                seg(ui, format!("VIEW {}", view_mode_info(self.view_mode).label));
                ui.add_space(6.0);
                widgets::lamp(ui, "WEATHER", self.weather_busy);
                if self.erosion_remaining > 0 {
                    widgets::lamp(ui, &format!("EROSION {}", self.erosion_remaining), true);
                }
                if self.export_handle.is_some() {
                    ui.label(
                        RichText::new(format!("EXPORT {:.0}%", self.export_progress * 100.0))
                            .monospace()
                            .size(10.5)
                            .color(theme::ACCENT),
                    );
                }

                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    ui.label(
                        RichText::new("DRAG ROTATE · SCROLL ZOOM · MMB PAN · DBL-CLICK RESET · N SEED · R VIEW · ? HELP")
                            .monospace()
                            .size(10.5)
                            .color(theme::TEXT_FAINT),
                    );
                });
            });
        });
    }

    fn controls_panel(&mut self, ctx: &egui::Context) {
        egui::SidePanel::left("controls")
            .resizable(true)
            .default_width(300.0)
            .show(ctx, |ui| {
                let tab_opts: Vec<(usize, &'static str)> =
                    TABS.iter().enumerate().map(|(i, t)| (i, *t)).collect();
                if let Some(tab) = widgets::chip_bar(ui, &tab_opts, self.active_tab) {
                    self.active_tab = tab;
                }
                ui.add_space(2.0);
                egui::ScrollArea::vertical()
                    .auto_shrink([false, false])
                    .show(ui, |ui| match self.active_tab {
                        1 => Self::tab_terrain(self, ui),
                        2 => Self::tab_climate(self, ui),
                        3 => Self::tab_look(self, ui),
                        4 => Self::tab_render_layers(self, ui),
                        _ => Self::tab_planet(self, ui),
                    });
            });
    }

    fn tab_planet(&mut self, ui: &mut egui::Ui) {
        widgets::section_header(ui, "orbit & mass");
        let mut changed = false;
        changed |= widgets::slider_row(
            ui,
            &mut self.params.star_distance_au,
            0.1..=50.0,
            Param::new(
                "Distance",
                "Distance from star. Closer = hotter/smoother, farther = colder/rougher",
                1.0,
            )
            .with(2, " AU")
            .log_scale(),
        );
        changed |= widgets::slider_row(
            ui,
            &mut self.params.mass_earth,
            0.01..=10.0,
            Param::new("Mass", "Planet mass. Affects gravity, terrain relief, plate count", 1.0)
                .with(2, " M⊕")
                .log_scale(),
        );
        changed |= widgets::slider_row(
            ui,
            &mut self.params.metallicity,
            -1.0..=1.0,
            Param::new("[Fe/H]", "Stellar metallicity. Higher = rougher terrain", 0.0).with(2, ""),
        );
        changed |= widgets::slider_row(
            ui,
            &mut self.params.axial_tilt_deg,
            0.0..=90.0,
            Param::new(
                "Axial tilt",
                "Axial tilt. Shifts climate zones, affects terrain detail",
                23.4,
            )
            .with(1, "°"),
        );
        changed |= widgets::slider_row(
            ui,
            &mut self.params.rotation_period_h,
            1.0..=1000.0,
            Param::new("Day length", "Rotation period", 24.0)
                .with(1, " h")
                .log_scale(),
        );

        widgets::section_header(ui, "seed");
        ui.horizontal(|ui| {
            if ui
                .small_button(RichText::new("RANDOMIZE").color(theme::TEXT_DIM))
                .on_hover_text("New random seed (N)")
                .clicked()
            {
                self.params.seed = rand_seed();
                self.planet_name = format!("planet_{}", self.params.seed);
                changed = true;
            }
            if ui
                .add(egui::DragValue::new(&mut self.params.seed).speed(2.0))
                .changed()
            {
                self.planet_name = format!("planet_{}", self.params.seed);
                changed = true;
            }
        });

        widgets::section_header(ui, "presets");
        let preset_opts: Vec<(usize, &'static str)> =
            PRESETS.iter().enumerate().map(|(i, p)| (i, *p)).collect();
        if let Some(idx) = widgets::chip_bar(ui, &preset_opts, usize::MAX) {
            self.apply_preset(idx);
        }
        ui.add_space(2.0);
        widgets::dim(
            ui,
            "Curated parameter bundles. Sliders marked ● differ from default; double-click resets.",
        );

        if changed {
            self.update_derived();
            self.needs_terrain = true;
        }
    }

    fn tab_terrain(&mut self, ui: &mut egui::Ui) {
        widgets::section_header(ui, "continents");
        if widgets::slider_row(
            ui,
            &mut self.continental_scale,
            0.5..=4.0,
            Param::new(
                "Continent scale",
                "Lower = fewer, larger continents. Higher = many small islands",
                1.0,
            )
            .with(2, ""),
        ) {
            self.needs_terrain = true;
        }
        let mut nc = self.num_continents as i32;
        if widgets::int_row(
            ui,
            &mut nc,
            1..=10,
            Param::new(
                "Continents",
                "Target number of distinct landmasses. 1 = supercontinent, 10 = archipelago",
                4.0,
            ),
        ) {
            self.num_continents = nc as u32;
            self.needs_terrain = true;
        }
        if widgets::slider_row(
            ui,
            &mut self.continent_size_variety,
            0.0..=1.0,
            Param::new(
                "Size variety",
                "Continent size distribution. 0 = equal sizes, 1 = one large + many small",
                0.35,
            )
            .with(2, ""),
        ) {
            self.needs_terrain = true;
        }

        widgets::section_header(ui, "water & erosion");
        if widgets::slider_row(
            ui,
            &mut self.water_loss,
            0.0..=1.0,
            Param::new(
                "Water loss",
                "Sea level control. 0 = ocean world, 1 = no surface water",
                0.5,
            )
            .with(2, ""),
        ) {
            self.needs_terrain = true;
        }
        let mut er = self.erosion_iterations as i32;
        if widgets::int_row(
            ui,
            &mut er,
            0..=50,
            Param::new(
                "Erosion iters",
                "Hydraulic erosion iterations. 0 = none, 25 = default, 50 = heavily eroded",
                25.0,
            ),
        ) {
            self.erosion_iterations = er as u32;
            self.needs_terrain = true;
        }
        if widgets::toggle_row(
            ui,
            &mut self.show_erosion,
            "Hydraulic erosion",
            "Carve rivers and valleys (progressive, batched per frame)",
        ) {
            self.needs_terrain = true;
        }

        widgets::section_header(ui, "tectonic detail");
        let mut plates = self.num_plates_override as i32;
        if widgets::int_row(
            ui,
            &mut plates,
            0..=30,
            Param::new("Plates", "Number of tectonic plates. 0 = auto from planet mass", 0.0),
        ) {
            self.num_plates_override = plates as u32;
            self.needs_terrain = true;
        }
        if widgets::slider_row(
            ui,
            &mut self.mountain_scale,
            0.0..=3.0,
            Param::new(
                "Mountain height",
                "Multiplier for tectonic mountain height. 0 = flat, 1 = default, 3 = extreme",
                1.0,
            )
            .with(2, ""),
        ) {
            self.needs_terrain = true;
        }
        if widgets::slider_row(
            ui,
            &mut self.boundary_width,
            0.03..=0.30,
            Param::new(
                "Range width",
                "How wide mountain ranges spread from plate boundaries. Low = narrow ridges, high = broad highlands",
                0.10,
            )
            .with(2, ""),
        ) {
            self.needs_terrain = true;
        }
        if widgets::slider_row(
            ui,
            &mut self.warp_strength,
            0.0..=3.0,
            Param::new(
                "Shape warp",
                "How organic plate boundaries look. 0 = geometric, 1 = default, 3 = very irregular",
                1.0,
            )
            .with(2, ""),
        ) {
            self.needs_terrain = true;
        }
        if widgets::slider_row(
            ui,
            &mut self.detail_scale,
            0.0..=3.0,
            Param::new(
                "Detail",
                "Fine terrain noise intensity. 0 = smooth, 1 = default, 3 = very rough",
                1.0,
            )
            .with(2, ""),
        ) {
            self.needs_terrain = true;
        }

        let mut age_val = self.age_override.unwrap_or(self.derived.surface_age);
        let mut use_override = self.age_override.is_some();
        ui.horizontal(|ui| {
            if widgets::toggle_row(
                ui,
                &mut use_override,
                "OVERRIDE",
                "Override surface age from physics",
            ) {
                self.age_override = if use_override { Some(age_val) } else { None };
                self.needs_terrain = true;
            }
            if widgets::slider_row(
                ui,
                &mut age_val,
                0.0..=1.0,
                Param::new(
                    "Surface age",
                    "0 = young (sharp ridges, active volcanism), 1 = old (smooth peneplains). Toggle = override physics",
                    self.derived.surface_age,
                )
                .with(2, ""),
            ) {
                self.age_override = Some(age_val);
                self.needs_terrain = true;
            }
        });

        widgets::section_header(ui, "physics");
        widgets::instrument_row(
            ui,
            "GRAVITY",
            format!("{:.1} m/s²", self.derived.surface_gravity),
        );
        widgets::instrument_row(
            ui,
            "TECTONICS",
            format!("{:.0}%", self.derived.tectonics_factor * 100.0),
        );
        widgets::instrument_row(
            ui,
            "SURFACE AGE",
            format!("{:.2}", self.age_override.unwrap_or(self.derived.surface_age)),
        );
    }

    fn tab_climate(&mut self, ui: &mut egui::Ui) {
        widgets::section_header(ui, "atmosphere");
        if widgets::slider_row(
            ui,
            &mut self.climate_moisture,
            0.0..=1.0,
            Param::new(
                "Atm. moisture",
                "Atmospheric moisture. 0 = bone dry (desert world), 1 = full moisture. Independent of water loss.",
                1.0,
            )
            .with(2, ""),
        ) {
            self.invalidate_weather();
        }
        if widgets::slider_row(
            ui,
            &mut self.season,
            0.0..=1.0,
            Param::new(
                "Season",
                "0 = deep winter, 0.5 = equinox, 1 = deep summer. Affects vegetation color and ice extent",
                0.5,
            )
            .with(2, ""),
        ) {
            self.needs_terrain = true;
        }

        widgets::section_header(ui, "clouds");
        if widgets::slider_row(
            ui,
            &mut self.cloud_coverage,
            0.0..=1.0,
            Param::new(
                "Coverage",
                "Cloud coverage fraction. 0 = clear sky, 1 = heavy overcast",
                0.5,
            )
            .with(2, ""),
        ) {
            self.invalidate_weather();
        }
        if widgets::slider_row(
            ui,
            &mut self.cloud_opacity,
            0.0..=1.0,
            Param::new(
                "Opacity",
                "Cloud layer transparency: 0 = invisible, 1 = fully opaque",
                1.0,
            )
            .with(2, ""),
        ) {
            self.needs_render = true;
        }
        if widgets::slider_row(
            ui,
            &mut self.wind_scale,
            0.0..=4.0,
            Param::new(
                "Wind strength",
                "0 = calm, 1 = physical baseline, 2 = strong transport, 4 = extreme. Regenerates weather transport.",
                1.0,
            )
            .with(2, ""),
        ) {
            self.invalidate_weather();
        }
        if widgets::toggle_row(
            ui,
            &mut self.show_wind_effects,
            "Wind transport",
            "Enable weather transport and wind-organized uplift; continentality and surface moisture remain active when off",
        ) {
            self.invalidate_weather();
        }
        ui.horizontal(|ui| {
            if ui
                .small_button(RichText::new("RANDOMIZE").color(theme::TEXT_DIM))
                .clicked()
            {
                self.cloud_seed = rand_seed();
                self.invalidate_weather();
            }
            let mut seed_i = self.cloud_seed as i64;
            if ui
                .add(
                    egui::DragValue::new(&mut seed_i)
                        .prefix("cloud seed ")
                        .speed(2.0),
                )
                .changed()
            {
                self.cloud_seed = seed_i.clamp(0, u32::MAX as i64) as u32;
                self.invalidate_weather();
            }
        });

        widgets::section_header(ui, "storms");
        let mut storms = self.storm_count as i32;
        if widgets::int_row(
            ui,
            &mut storms,
            0..=8,
            Param::new(
                "Storm count",
                "Number of localized storm regions within physically eligible weather",
                2.0,
            ),
        ) {
            self.storm_count = storms as u32;
            self.invalidate_weather();
        }
        if self.storm_count > 0
            && widgets::slider_row(
                ui,
                &mut self.storm_size,
                0.3..=3.0,
                Param::new(
                    "Storm size",
                    "Scale of eligible storm regions: 0.3 = compact, 3.0 = broad",
                    1.0,
                )
                .with(2, ""),
            )
        {
            self.invalidate_weather();
        }
    }

    fn tab_look(&mut self, ui: &mut egui::Ui) {
        widgets::section_header(ui, "star & sun");
        if widgets::angle_row(
            ui,
            &mut self.light_azimuth,
            -180.0..=180.0,
            Param::new("Sun azimuth", "Horizontal angle of the sun", -0.5),
        ) {
            self.needs_render = true;
        }
        if widgets::angle_row(
            ui,
            &mut self.light_elevation,
            0.0..=180.0,
            Param::new(
                "Sun elevation",
                "Height of the sun: 0° = horizon, 90° = overhead, 180° = below",
                0.3,
            ),
        ) {
            self.needs_render = true;
        }
        if widgets::slider_row(
            ui,
            &mut self.star_color_temp,
            0.0..=1.0,
            Param::new(
                "Star color",
                "Star type: 0 = hot blue (O/B), 0.5 = sun-like (G), 1.0 = red dwarf (M)",
                0.5,
            )
            .with(2, ""),
        ) {
            self.needs_render = true;
        }
        if widgets::slider_row(
            ui,
            &mut self.height_scale,
            0.5..=10.0,
            Param::new(
                "Relief",
                "How pronounced terrain relief appears in lighting. 1 = subtle, 5 = dramatic",
                3.0,
            )
            .with(1, ""),
        ) {
            self.needs_render = true;
        }

        widgets::section_header(ui, "night side");
        if widgets::slider_row(
            ui,
            &mut self.night_lights,
            0.0..=1.0,
            Param::new(
                "Development",
                "Urbanization level: 0 = pristine wilderness, 1 = heavily developed. Grey cities by day, lights at night",
                0.0,
            )
            .with(2, ""),
        ) {
            self.needs_render = true;
        }
        if self.night_lights > 0.0
            && widgets::slider_row(
                ui,
                &mut self.city_light_hue,
                0.0..=1.0,
                Param::new(
                    "Light color",
                    "Night light color: 0 = warm amber (sodium), 0.5 = white (LED), 1.0 = cool blue (alien/futuristic)",
                    0.0,
                )
                .with(2, ""),
            )
        {
            self.needs_render = true;
        }

        widgets::section_header(ui, "planetary rings");
        if widgets::slider_row(
            ui,
            &mut self.ring_inner,
            0.0..=3.0,
            Param::new(
                "Inner radius",
                "Ring inner radius in planet radii. 0 = rings disabled",
                0.0,
            )
            .with(2, " R"),
        ) {
            self.needs_render = true;
        }
        if widgets::slider_row(
            ui,
            &mut self.ring_outer,
            0.0..=4.0,
            Param::new("Outer radius", "Ring outer radius in planet radii", 0.0).with(2, " R"),
        ) {
            if self.ring_outer < self.ring_inner {
                self.ring_outer = self.ring_inner;
            }
            self.needs_render = true;
        }
        if widgets::slider_row(
            ui,
            &mut self.ring_tilt,
            0.0..=90.0,
            Param::new("Ring tilt", "Ring inclination in degrees", 15.0).with(0, "°"),
        ) {
            self.needs_render = true;
        }
        if widgets::slider_row(
            ui,
            &mut self.ring_opacity,
            0.0..=1.0,
            Param::new("Ring opacity", "Ring band opacity", 0.7).with(2, ""),
        ) {
            self.needs_render = true;
        }

        widgets::section_header(ui, "tectonic glow");
        if widgets::slider_row(
            ui,
            &mut self.lava_glow,
            0.0..=1.0,
            Param::new(
                "Lava glow",
                "Tectonic emission intensity along boundaries and young terrain",
                0.0,
            )
            .with(2, ""),
        ) {
            self.needs_render = true;
        }
    }

    fn tab_render_layers(&mut self, ui: &mut egui::Ui) {
        widgets::section_header(ui, "surface");
        for (flag, label, tip) in [
            (
                &mut self.show_water,
                "Water / ocean",
                "Ocean surface with depth shading and specular",
            ),
            (&mut self.show_ice, "Ice caps", "Polar and altitude ice rendering"),
            (
                &mut self.show_biomes,
                "Biome colors",
                "Temperature/moisture-driven biome coloring",
            ),
            (
                &mut self.show_ao,
                "Ambient occlusion",
                "Valley darkening for depth perception",
            ),
        ] {
            if widgets::toggle_row(ui, flag, label, tip) {
                self.needs_render = true;
            }
        }

        widgets::section_header(ui, "sky");
        for (flag, label, tip) in [
            (
                &mut self.show_clouds,
                "Clouds",
                "Volumetric cloud layer rendering",
            ),
            (
                &mut self.show_cloud_shadows,
                "Cloud shadows",
                "Cloud shadows cast on the planet surface",
            ),
            (
                &mut self.show_atmosphere,
                "Atmosphere",
                "Atmospheric scattering (blue limb glow, red sunsets)",
            ),
        ] {
            if widgets::toggle_row(ui, flag, label, tip) {
                self.needs_render = true;
            }
        }

        widgets::section_header(ui, "night");
        if widgets::toggle_row(
            ui,
            &mut self.show_cities,
            "City lights",
            "Night-side city lights and day-side urban patches",
        ) {
            self.needs_render = true;
        }

        widgets::section_header(ui, "viewport resolution");
        let res_opts: Vec<(u32, &'static str)> =
            [(256u32, "256"), (512, "512"), (768, "768"), (1024, "1K"), (2048, "2K")]
                .into_iter()
                .collect();
        if let Some(res) = widgets::chip_bar(ui, &res_opts, self.preview_resolution)
            && res != self.preview_resolution
        {
            self.preview_resolution = res;
            self.needs_terrain = true;
        }
    }

    fn viewport_header(&mut self, ctx: &egui::Context) {
        egui::TopBottomPanel::top("viewport_bar").show(ctx, |ui| {
            ui.horizontal_wrapped(|ui| {
                let group_label = |ui: &mut egui::Ui, t: &str| {
                    ui.add_space(4.0);
                    ui.label(RichText::new(t).small().color(theme::TEXT_FAINT));
                };
                group_label(ui, "SHADE");
                if let Some(id) = widgets::chip_bar(ui, &view_opts(ViewGroup::Shaded), self.view_mode)
                {
                    self.view_mode = id;
                    self.needs_render = true;
                }
                group_label(ui, "MAPS");
                if let Some(id) = widgets::chip_bar(ui, &view_opts(ViewGroup::Maps), self.view_mode)
                {
                    self.view_mode = id;
                    self.needs_render = true;
                }
                group_label(ui, "DEBUG");
                if let Some(id) = widgets::chip_bar(ui, &view_opts(ViewGroup::Debug), self.view_mode)
                {
                    self.view_mode = id;
                    self.needs_render = true;
                }
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    if ui
                        .small_button(RichText::new("RESET VIEW").color(theme::TEXT_DIM))
                        .on_hover_text("Reset rotation, zoom and pan (R)")
                        .clicked()
                    {
                        self.reset_preview();
                    }
                });
            });
        });
    }

    fn inspector_panel(&mut self, ctx: &egui::Context) {
        egui::SidePanel::right("inspector")
            .resizable(true)
            .default_width(272.0)
            .show(ctx, |ui| {
                egui::ScrollArea::vertical()
                    .auto_shrink([false, false])
                    .show(ui, |ui| {
                        widgets::section_header(ui, "derived properties");
                        widgets::instrument_row(
                            ui,
                            "TYPE",
                            format!("{:?}", self.derived.planet_type),
                        );
                        widgets::instrument_row(
                            ui,
                            "TECTONICS",
                            format!(
                                "{:?} {:.0}%",
                                self.derived.tectonic_regime,
                                self.derived.tectonics_factor * 100.0
                            ),
                        );
                        widgets::instrument_row(
                            ui,
                            "ATMOSPHERE",
                            format!(
                                "{:?} {:.0}%",
                                self.derived.atmosphere_type,
                                self.derived.atmosphere_strength * 100.0
                            ),
                        );
                        widgets::instrument_row(
                            ui,
                            "GRAVITY",
                            format!("{:.2} m/s²", self.derived.surface_gravity),
                        );
                        widgets::instrument_row(
                            ui,
                            "BASE TEMP",
                            format!("{:.1} °C", self.derived.base_temperature_c),
                        );
                        let ocean = if self.water_loss > 0.01 {
                            format!(
                                "{:.0}% → {:.0}%",
                                self.derived.ocean_fraction * 100.0,
                                self.derived.ocean_fraction * (1.0 - self.water_loss) * 100.0
                            )
                        } else {
                            format!("{:.0}%", self.derived.ocean_fraction * 100.0)
                        };
                        widgets::instrument_row(ui, "OCEAN", ocean);
                        widgets::instrument_row(
                            ui,
                            "FROST LINE",
                            format!("{:.1} AU", self.derived.frost_line_au),
                        );
                        widgets::instrument_row(
                            ui,
                            "ISOLATION M",
                            format!("{:.2} M⊕", self.derived.isolation_mass),
                        );

                        if self.params.mass_earth > self.derived.isolation_mass * 15.0 {
                            ui.add_space(6.0);
                            widgets::callout(
                                ui,
                                "NOTICE",
                                &format!(
                                    "Mass {:.1} M⊕ exceeds isolation mass {:.2} M⊕ — requires migration.",
                                    self.params.mass_earth, self.derived.isolation_mass
                                ),
                                theme::ACCENT,
                            );
                        }

                        widgets::section_header(ui, "export");
                        ui.label(RichText::new("NAME").small().color(theme::TEXT_FAINT));
                        ui.add(
                            egui::TextEdit::singleline(&mut self.planet_name)
                                .desired_width(f32::INFINITY),
                        );

                        widgets::section_header(ui, "resolution");
                        let res_opts: Vec<(u32, &'static str)> =
                            [(2048u32, "2K"), (4096, "4K"), (8192, "8K")].into_iter().collect();
                        if let Some(res) =
                            widgets::chip_bar(ui, &res_opts, self.export_resolution)
                        {
                            self.export_resolution = res;
                        }

                        widgets::section_header(ui, "layers");
                        {
                            let flags = [
                                &mut self.export_albedo,
                                &mut self.export_roughness,
                                &mut self.export_clouds,
                                &mut self.export_height,
                                &mut self.export_emission,
                                &mut self.export_water_mask,
                                &mut self.export_normals,
                            ];
                            for (flag, (label, file)) in flags.into_iter().zip(EXPORT_LAYERS) {
                                ui.horizontal(|ui| {
                                    ui.checkbox(flag, RichText::new(*label).color(theme::TEXT));
                                    ui.with_layout(
                                        egui::Layout::right_to_left(egui::Align::Center),
                                        |ui| {
                                            ui.label(
                                                RichText::new(*file)
                                                    .monospace()
                                                    .size(10.5)
                                                    .color(theme::TEXT_FAINT),
                                            );
                                        },
                                    );
                                });
                            }
                        }
                        widgets::dim(
                            ui,
                            "Clouds export writes a six-channel reconstruction EXR, not the composited preview.",
                        );

                        let is_exporting = self.export_handle.is_some();
                        let has_layers = self.export_layer_flags().iter().any(|f| *f);
                        ui.add_space(6.0);
                        if is_exporting {
                            ui.add(
                                egui::ProgressBar::new(self.export_progress)
                                    .text(RichText::new(&self.export_status).small()),
                            );
                            if ui.button("Cancel export").clicked()
                                && let Some(ref handle) = self.export_handle
                            {
                                handle.cancel.store(true, std::sync::atomic::Ordering::Relaxed);
                                self.export_status = format!(
                                    "Cancelling export to {} (layers: {}).",
                                    self.export_destination, self.export_layers
                                );
                            }
                        } else {
                            let button = widgets::primary_button(ui, "EXPORT TEXTURES", has_layers);
                            let clicked = button.clicked();
                            if !has_layers {
                                button.on_disabled_hover_text("Select at least one export layer");
                            } else if clicked {
                                self.start_export();
                            }
                            if self.export_done_ok {
                                ui.add_space(4.0);
                                widgets::callout(
                                    ui,
                                    "DONE",
                                    &format!("Export complete: {}", self.export_destination),
                                    theme::OK,
                                );
                            }
                        }
                    });
            });
    }

    fn hud_overlay(&self, ctx: &egui::Context) {
        // Anchor to the canvas captured this frame; skip until it exists.
        if !self.hud_rect.is_finite() {
            return;
        }
        let mode = view_mode_info(self.view_mode);
        let group = match mode.group {
            ViewGroup::Shaded => "SHADING",
            ViewGroup::Maps => "EXPORT MAP",
            ViewGroup::Debug => "DEBUG",
        };
        widgets::hud_chip(
            ctx,
            egui::Id::new("hud_mode"),
            self.hud_rect,
            egui::Align2::LEFT_TOP,
            16.0,
            |ui| {
                ui.label(
                    RichText::new(format!("{group} · {}", mode.label))
                        .monospace()
                        .size(10.5)
                        .color(theme::TEXT_DIM),
                );
            },
        );
        widgets::hud_chip(
            ctx,
            egui::Id::new("hud_zoom"),
            self.hud_rect,
            egui::Align2::RIGHT_BOTTOM,
            16.0,
            |ui| {
                ui.label(
                    RichText::new(format!("ZOOM {:.0}%", self.zoom * 100.0))
                        .monospace()
                        .size(10.5)
                        .color(theme::TEXT_DIM),
                );
            },
        );
    }

    fn help_window(&mut self, ctx: &egui::Context) {
        if !self.show_help {
            return;
        }
        egui::Window::new("SHORTCUTS")
            .open(&mut self.show_help)
            .default_pos([520.0, 160.0])
            .resizable(false)
            .show(ctx, |ui| {
                let rows: &[(&str, &str)] = &[
                    ("N", "Randomize seed"),
                    ("R", "Reset view"),
                    ("← → ↑ ↓", "Rotate planet"),
                    ("+ / −", "Zoom in / out"),
                    ("Drag", "Rotate planet"),
                    ("Scroll", "Zoom toward cursor"),
                    ("Middle-drag", "Pan viewport"),
                    ("Double-click", "Reset zoom & pan"),
                    ("F1 / ?", "Toggle this help"),
                    ("Esc", "Close help"),
                ];
                for (key, desc) in rows {
                    ui.horizontal(|ui| {
                        widgets::key_cap(ui, key);
                        ui.label(RichText::new(*desc).color(theme::TEXT_DIM));
                    });
                }
            });
    }
}

impl eframe::App for PlanetGenApp {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        self.handle_keyboard_shortcuts(ctx);
        if self.export_handle.is_some() {
            self.poll_export();
            ctx.request_repaint();
        }

        // GPU error banner (declared first => sits above everything else).
        let mut dismiss_error = false;
        if let Some(ref err) = self.gpu_error.clone() {
            egui::TopBottomPanel::top("gpu_error").show(ctx, |ui| {
                ui.horizontal(|ui| {
                    widgets::callout(ui, "ERROR", &format!("GPU error: {err}"), theme::DANGER);
                    if ui
                        .small_button(RichText::new("DISMISS").color(theme::TEXT_DIM))
                        .clicked()
                    {
                        dismiss_error = true;
                    }
                });
            });
        }
        if dismiss_error {
            self.gpu_error = None;
        }

        self.top_bar(ctx);
        self.status_bar(ctx);
        self.controls_panel(ctx);
        self.inspector_panel(ctx);
        self.viewport_header(ctx);

        egui::CentralPanel::default()
            .frame(egui::Frame::new().fill(theme::BG_DEEP).inner_margin(14.0))
            .show(ctx, |ui| {
                // Square canvas region (also the anchor rect for HUD chips).
                let all = ui.available_rect_before_wrap();
                let side = all.width().min(all.height()).max(64.0);
                let rect = egui::Rect::from_center_size(all.center(), egui::vec2(side, side));
                self.hud_rect = rect;

                if self.cached_cubemap_view.is_some() {
                    let response = ui.allocate_rect(rect, egui::Sense::click_and_drag());

                    ui.painter().image(
                        self.texture_id,
                        rect,
                        egui::Rect::from_min_max(egui::pos2(0.0, 0.0), egui::pos2(1.0, 1.0)),
                        egui::Color32::WHITE,
                    );

                    // Left-drag: rotate planet.
                    if response.dragged_by(egui::PointerButton::Primary) {
                        let delta = response.drag_delta();
                        self.apply_view_rotation(-delta.y * 0.01, delta.x * 0.01);
                        self.needs_render = true;
                    }

                    // Middle-drag: pan viewport.
                    if response.dragged_by(egui::PointerButton::Middle) {
                        let delta = response.drag_delta();
                        let ndc_per_pixel = 2.0 / (0.85 * side);
                        self.pan[0] += delta.x * ndc_per_pixel;
                        self.pan[1] += delta.y * ndc_per_pixel;
                        self.needs_render = true;
                    }

                    // Scroll: zoom toward cursor position.
                    if response.hovered() {
                        let scroll = ui.input(|i| i.smooth_scroll_delta.y);
                        if scroll != 0.0 {
                            let zoom_old = self.zoom;
                            let zoom_new = (zoom_old * (1.0 + scroll * 0.005)).clamp(0.1, 20.0);
                            if let Some(cursor_pos) = response.hover_pos() {
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

                    // Double-click: reset zoom and pan.
                    if response.double_clicked() {
                        self.zoom = 1.0;
                        self.pan = [0.0, 0.0];
                        self.needs_render = true;
                    }
                } else {
                    ui.centered_and_justified(|ui| {
                        ui.spinner();
                    });
                    ui.centered_and_justified(|ui| {
                        ui.label(
                            RichText::new("GENERATING TERRAIN")
                                .monospace()
                                .size(10.5)
                                .color(theme::TEXT_FAINT),
                        );
                    });
                }

                // Loading scrim — only after 1s so quick edits never flash.
                let gen_elapsed = self
                    .terrain_start
                    .map(|t| t.elapsed().as_secs_f32())
                    .unwrap_or(0.0);
                if (self.terrain_pending || self.erosion_remaining > 0) && gen_elapsed > 1.0 {
                    ui.painter()
                        .rect_filled(ui.max_rect(), 0.0, egui::Color32::from_black_alpha(70));
                }
            });

        // Loading pill floats over the scrim (elevated, instrument style).
        let gen_elapsed = self
            .terrain_start
            .map(|t| t.elapsed().as_secs_f32())
            .unwrap_or(0.0);
        if (self.terrain_pending || self.erosion_remaining > 0) && gen_elapsed > 1.0 {
            widgets::hud_chip(
                ctx,
                egui::Id::new("hud_loading"),
                self.hud_rect,
                egui::Align2::CENTER_CENTER,
                0.0,
                |ui| {
                    ui.horizontal(|ui| {
                        ui.spinner();
                        ui.label(
                            RichText::new(format!(
                                "GENERATING TERRAIN · {}px",
                                self.preview_resolution
                            ))
                            .monospace()
                            .size(11.0)
                            .color(theme::TEXT),
                        );
                    });
                },
            );
        }

        self.hud_overlay(ctx);
        self.help_window(ctx);

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
        let mouse_busy = ctx.input(|i| i.pointer.any_pressed() || i.pointer.any_down());
        if self.erosion_remaining > 0 && !mouse_busy {
            self.erode_batch();
            ctx.request_repaint();
        } else if self.erosion_remaining > 0 {
            ctx.request_repaint(); // retry next frame when mouse released
        }
        let weather_busy = self.poll_weather();
        self.weather_busy = weather_busy;
        if weather_busy {
            ctx.request_repaint();
        }
        if self.needs_render {
            self.render_preview();
        }
    }

    fn on_exit(&mut self, _gl: Option<&eframe::glow::Context>) {
        self.render_state
            .renderer
            .write()
            .free_texture(&self.texture_id);
    }
}
/// Incremental view-space rotation Rx(pitch)·Ry(yaw): tilt about the viewer's
/// horizontal axis and yaw about the viewer's vertical axis, independent of
/// the orientation accumulated so far.
fn rot3_view_delta(pitch: f32, yaw: f32) -> [[f32; 3]; 3] {
    let (sp, cp) = pitch.sin_cos();
    let (sy, cy) = yaw.sin_cos();
    [
        [cy, 0.0, sy],
        [sp * sy, cp, -sp * cy],
        [-cp * sy, sp, cp * cy],
    ]
}

/// Identity orientation for the view-space rotation accumulator.
const IDENTITY3: [[f32; 3]; 3] = [[1.0, 0.0, 0.0], [0.0, 1.0, 0.0], [0.0, 0.0, 1.0]];

/// Row-major 3×3 matrix product.
fn rot3_mul(a: &[[f32; 3]; 3], b: &[[f32; 3]; 3]) -> [[f32; 3]; 3] {
    let mut out = [[0.0f32; 3]; 3];
    for i in 0..3 {
        for j in 0..3 {
            out[i][j] = a[i][0] * b[0][j] + a[i][1] * b[1][j] + a[i][2] * b[2][j];
        }
    }
    out
}

/// Re-orthonormalize rows via Gram–Schmidt so float drift never accumulates
/// into skew/scale in the orientation matrix.
fn rot3_orthonormalize(m: &[[f32; 3]; 3]) -> [[f32; 3]; 3] {
    fn dot(a: [f32; 3], b: [f32; 3]) -> f32 {
        a[0] * b[0] + a[1] * b[1] + a[2] * b[2]
    }
    fn norm(v: [f32; 3]) -> Option<[f32; 3]> {
        let len = dot(v, v).sqrt();
        (len > 1e-6).then(|| [v[0] / len, v[1] / len, v[2] / len])
    }
    fn cross(a: [f32; 3], b: [f32; 3]) -> [f32; 3] {
        [
            a[1] * b[2] - a[2] * b[1],
            a[2] * b[0] - a[0] * b[2],
            a[0] * b[1] - a[1] * b[0],
        ]
    }
    let Some(r0) = norm(m[0]) else {
        return IDENTITY3;
    };
    let r1_raw = [
        m[1][0] - r0[0] * dot(m[1], r0),
        m[1][1] - r0[1] * dot(m[1], r0),
        m[1][2] - r0[2] * dot(m[1], r0),
    ];
    let Some(r1) = norm(r1_raw) else {
        return IDENTITY3;
    };
    // Keep right-handed: derive the third row from the first two.
    [r0, r1, cross(r0, r1)]
}

fn rand_seed() -> u32 {
    use std::time::{SystemTime, UNIX_EPOCH};
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .subsec_nanos()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::export::{ExportConfig, ExportLayers, run_export};

    fn apply_vec(m: &[[f32; 3]; 3], v: [f32; 3]) -> [f32; 3] {
        let mut out = [0.0f32; 3];
        for (i, o) in out.iter_mut().enumerate() {
            *o = m[i][0] * v[0] + m[i][1] * v[1] + m[i][2] * v[2];
        }
        out
    }

    fn transpose3(m: &[[f32; 3]; 3]) -> [[f32; 3]; 3] {
        [
            [m[0][0], m[1][0], m[2][0]],
            [m[0][1], m[1][1], m[2][1]],
            [m[0][2], m[1][2], m[2][2]],
        ]
    }

    /// The core regression: a vertical drag must move surface features along
    /// the screen's vertical axis *no matter what yaw was applied first*.
    /// With the old Euler accumulation (Ry·Rx) this failed at yaw = 90°,
    /// where "pitch" rotated about a planet axis swung into depth.
    #[test]
    fn vertical_drag_tilts_about_viewer_axis_after_any_yaw() {
        let center_ray = [0.0f32, 0.0, 1.0];
        for yaw_deg in [0i32, 45, 90, 135, 180] {
            let yaw = yaw_deg as f32 * std::f32::consts::PI / 180.0;
            let s1 = rot3_view_delta(0.0, yaw);
            // Feature visible at screen center: Sᵀ·v = p.
            let p = apply_vec(&transpose3(&s1), center_ray);
            let s2 = rot3_mul(&rot3_view_delta(0.25, 0.0), &s1);
            let v_new = apply_vec(&s2, p);
            let disp = [v_new[0] - center_ray[0], v_new[1] - center_ray[1]];
            assert!(disp[0].abs() < 1e-5, "yaw {yaw_deg}°: horizontal drift {}", disp[0]);
            assert!(disp[1].abs() > 1e-3, "yaw {yaw_deg}°: pitch did nothing");
        }
    }

    /// Symmetric guarantee: a horizontal drag spins about the viewer's
    /// vertical axis regardless of any tilt applied first.
    #[test]
    fn horizontal_drag_yaws_about_viewer_axis_after_any_pitch() {
        let center_ray = [0.0f32, 0.0, 1.0];
        for pitch_deg in [0i32, 40, 80, -60] {
            let pitch = pitch_deg as f32 * std::f32::consts::PI / 180.0;
            let s1 = rot3_view_delta(pitch, 0.0);
            let p = apply_vec(&transpose3(&s1), center_ray);
            let s2 = rot3_mul(&rot3_view_delta(0.0, 0.25), &s1);
            let v_new = apply_vec(&s2, p);
            let disp = [v_new[0] - center_ray[0], v_new[1] - center_ray[1]];
            assert!(disp[1].abs() < 1e-5, "pitch {pitch_deg}°: vertical drift {}", disp[1]);
            assert!(disp[0].abs() > 1e-3, "pitch {pitch_deg}°: yaw did nothing");
        }
    }

    /// The accumulator must stay a proper rotation under long drag sessions.
    #[test]
    fn rotation_stays_orthonormal_under_many_updates() {
        let mut s = IDENTITY3;
        let mut x = 0x1234_5678u32;
        for _ in 0..5000 {
            x = x.wrapping_mul(1_664_525).wrapping_add(1_013_904_223);
            let a = ((x >> 8) as f32 / u32::MAX as f32 - 0.5) * 0.4;
            let b = ((x >> 20) as f32 / u32::MAX as f32 - 0.5) * 0.4;
            s = rot3_orthonormalize(&rot3_mul(&rot3_view_delta(a, b), &s));
        }
        for i in 0..3 {
            let len: f32 = s[i].iter().map(|v| v * v).sum::<f32>().sqrt();
            assert!((len - 1.0).abs() < 1e-4, "row {i} length {len}");
        }
        for i in 0..3 {
            for j in 0..3 {
                let d: f32 = (0..3).map(|k| s[i][k] * s[j][k]).sum();
                assert!((d - if i == j { 1.0 } else { 0.0 }).abs() < 1e-4, "rows {i},{j} dot {d}");
            }
        }
    }

    #[test]
    fn zero_delta_is_a_no_op() {
        let d = rot3_view_delta(0.0, 0.0);
        assert_eq!(d, IDENTITY3);
    }

    fn export_config(output_dir: std::path::PathBuf, planet_name: &str) -> ExportConfig {
        ExportConfig {
            face_resolution: 32,
            tile_size: 16,
            output_dir,
            planet_name: planet_name.into(),
            erosion_iterations: 0,
            layers: ExportLayers {
                height: true,
                albedo: false,
                normals: false,
                roughness: false,
                water_mask: false,
                clouds: false,
                emission: false,
            },
            weather: WeatherSnapshot::default(),
            night_lights: 0.0,
        }
    }

    #[test]
    fn preview_export_v1_preset_preserves_non_default_plate_controls() {
        let params = PlanetParams {
            seed: 42,
            ..PlanetParams::default()
        };
        let derived = DerivedProperties::from_params(&params);
        let preview = derive_terrain_params(
            &params,
            &derived,
            1.2,
            1.3,
            0.08,
            0.7,
            0.9,
            Some(0.6),
            13,
            7,
            0.8,
        );
        assert_eq!(preview.num_plates_override, 13);
        assert_eq!(preview.num_continents, 7);
        assert_eq!(preview.continent_size_variety, 0.8);

        let gpu = GpuContext::new().expect("GPU init failed");
        let root = std::env::temp_dir().join(format!("planet-gen-parity-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        let (progress, _) = std::sync::mpsc::channel();
        let exported = run_export(
            &gpu,
            &export_config(root.clone(), "preview"),
            &params,
            &derived,
            preview.continental_scale,
            0.0,
            preview,
            &progress,
            &std::sync::atomic::AtomicBool::new(false),
        )
        .expect("preview export failed");
        let altered = TerrainGenerationParams {
            num_plates_override: 4,
            num_continents: 1,
            continent_size_variety: 0.0,
            ..preview
        };
        let exported_altered = run_export(
            &gpu,
            &export_config(root.clone(), "altered"),
            &params,
            &derived,
            altered.continental_scale,
            0.0,
            altered,
            &progress,
            &std::sync::atomic::AtomicBool::new(false),
        )
        .expect("altered export failed");

        assert_ne!(
            std::fs::read(exported.join("height.exr")).unwrap(),
            std::fs::read(exported_altered.join("height.exr")).unwrap(),
        );
        let _ = std::fs::remove_dir_all(root);
    }
}
