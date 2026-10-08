use bytemuck::Zeroable;
use planet_gen::{
    export::{ExportConfig, ExportLayers, run_export},
    gpu::GpuContext,
    planet::{DerivedProperties, PlanetParams},
    plates::{PlateGenParams, generate_plates},
    preview::{PreviewRenderer, PreviewUniforms},
    terrain_compute::{TerrainComputePipeline, TerrainGenerationParams, WindFieldPipeline},
    weather::WeatherSnapshot,
};

// cargo run --release --bin surface_capture -- OUTPUT_DIR [SEED] [SIZE] [--export]
fn main() {
    let args: Vec<String> = std::env::args().collect();
    let out = args
        .get(1)
        .expect("surface_capture OUTPUT_DIR [SEED] [SIZE]");
    let seed = args.get(2).map(|v| v.parse().unwrap()).unwrap_or(42);
    let size: u32 = args.get(3).map(|v| v.parse().unwrap()).unwrap_or(768);
    assert!((128..=1536).contains(&size) && size.is_multiple_of(64));
    std::fs::create_dir_all(out).unwrap();
    let gpu = GpuContext::new().unwrap();
    let params = PlanetParams {
        seed,
        ..Default::default()
    };
    let derived = DerivedProperties::from_params(&params);
    let ocean_level = -0.5 + 1.7 * derived.ocean_fraction * 0.5;
    let plates = generate_plates(&PlateGenParams {
        seed,
        mass_earth: 1.0,
        ocean_fraction: derived.ocean_fraction * 0.5,
        tectonics_factor: derived.tectonics_factor,
        continental_scale: 1.0,
        num_plates_override: 0,
        num_continents: 4,
        continent_size_variety: 0.35,
    });
    let compute = TerrainComputePipeline::new(&gpu);
    let terrain = compute.generate(
        &gpu,
        &plates,
        size,
        seed,
        1.2,
        1.5,
        (8.0 + 4.0 * (params.axial_tilt_deg / 90.0) * derived.tectonics_factor) as u32,
        2.0_f32.powf(-(1.47_f32 - 1.0) / 2.0),
        2.1,
        1.0,
        0.10,
        1.0,
        1.0,
        derived.surface_gravity,
        derived.tectonics_factor,
        derived.surface_age,
        1.0,
    );
    let renderer = PreviewRenderer::new(&gpu);
    let terrain_view = renderer.upload_terrain(&gpu, &terrain);
    let wind = WindFieldPipeline::new(&gpu).unwrap();
    let dynamics = wind.create_textures(&gpu, 128, &terrain, ocean_level);
    wind.generate_gpu(
        &gpu,
        &terrain,
        &dynamics,
        seed.wrapping_add(1000),
        ocean_level,
        params.axial_tilt_deg.to_radians(),
        0.5,
        1.0,
        derived.base_temperature_c,
        derived.surface_pressure_bar,
        1.0,
    );
    let mut u = PreviewUniforms {
        light_dir: [0.5, 0.3, 1.0],
        ocean_level,
        base_temp_c: derived.base_temperature_c,
        ocean_fraction: derived.ocean_fraction,
        axial_tilt_rad: params.axial_tilt_deg.to_radians(),
        season: 0.5,
        height_scale: 3.0,
        zoom: 1.0,
        surface_seed: seed,
        star_color_temp: 0.5,
        show_ao: 1.0,
        show_water: 1.0,
        show_ice: 1.0,
        show_biomes: 1.0,
        cloud_advection: 1.0,
        rotation_rate: 1.0,
        atm_pressure: derived.surface_pressure_bar,
        planet_radius_km: derived.radius_km,
        ..PreviewUniforms::zeroed()
    };
    for view in 0..4 {
        let (s, c) = (view as f32 * std::f32::consts::FRAC_PI_2).sin_cos();
        u.rotation = [
            [c, 0.0, s, 0.0],
            [0.0, 1.0, 0.0, 0.0],
            [-s, 0.0, c, 0.0],
            [0.0, 0.0, 0.0, 1.0],
        ];
        for (name, mode) in [("surface", 0), ("albedo", 19)] {
            u.view_mode = mode;
            let pixels = renderer.render(
                &gpu,
                &u,
                &terrain_view,
                Some(&dynamics.wind_continentality),
                None,
                size,
            );
            image::save_buffer(
                format!("{out}/{name}-{view}.png"),
                &pixels,
                size,
                size,
                image::ColorType::Rgba8,
            )
            .unwrap();
        }
    }
    println!("saved seed {seed}, four surface/albedo views to {out}");
    if args.get(4).is_some_and(|arg| arg == "--export") {
        let config = ExportConfig {
            face_resolution: size,
            tile_size: size.min(256),
            output_dir: out.into(),
            planet_name: "export".into(),
            erosion_iterations: 0,
            layers: ExportLayers {
                height: false,
                albedo: true,
                normals: false,
                roughness: false,
                water_mask: false,
                clouds: false,
                emission: false,
            },
            weather: WeatherSnapshot {
                season: 0.5,
                moisture: 1.0,
                ..Default::default()
            },
            night_lights: 0.0,
        };
        let terrain_params = TerrainGenerationParams {
            seed,
            amplitude: 1.2,
            frequency: 1.5,
            octaves: (8.0 + 4.0 * (params.axial_tilt_deg / 90.0) * derived.tectonics_factor) as u32,
            gain: 2.0_f32.powf(-(1.47_f32 - 1.0) / 2.0),
            lacunarity: 2.1,
            mountain_scale: 1.0,
            boundary_width: 0.1,
            warp_strength: 1.0,
            detail_scale: 1.0,
            surface_gravity: derived.surface_gravity,
            tectonics_factor: derived.tectonics_factor,
            surface_age: derived.surface_age,
            continental_scale: 1.0,
            num_plates_override: 0,
            num_continents: 4,
            continent_size_variety: 0.35,
        };
        let (tx, _rx) = std::sync::mpsc::channel();
        run_export(
            &gpu,
            &config,
            &params,
            &derived,
            1.0,
            0.5,
            terrain_params,
            &tx,
            &std::sync::atomic::AtomicBool::new(false),
        )
        .unwrap();
        println!("saved exported albedo to {out}/export/albedo.png");
    }
}
