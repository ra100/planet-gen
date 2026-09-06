use bytemuck::Zeroable;
use planet_gen::{
    gpu::GpuContext,
    planet::{DerivedProperties, PlanetParams},
    plates::{PlateGenParams, generate_plates},
    preview::{PreviewRenderer, PreviewUniforms},
    terrain_compute::{TerrainComputePipeline, WindFieldPipeline, earth_relative_rotation_rate},
    weather::{WeatherFieldPipeline, WeatherSnapshot},
};

// Reproducible native-default rendering, without a display server.
// Usage: cargo run --bin realism_capture -- OUTPUT_DIR [SEED] [size]
// size must be a multiple of 64 for aligned readback rows.
fn read_interactive(gpu: &GpuContext, renderer: &PreviewRenderer, size: u32) -> Vec<u8> {
    let buffer = gpu.device.create_buffer(&wgpu::BufferDescriptor {
        label: Some("review readback"),
        size: (size * size * 4) as u64,
        usage: wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::MAP_READ,
        mapped_at_creation: false,
    });
    let mut encoder = gpu.device.create_command_encoder(&Default::default());
    encoder.copy_texture_to_buffer(
        wgpu::TexelCopyTextureInfo {
            texture: renderer.target_view().texture(),
            mip_level: 0,
            origin: wgpu::Origin3d::ZERO,
            aspect: wgpu::TextureAspect::All,
        },
        wgpu::TexelCopyBufferInfo {
            buffer: &buffer,
            layout: wgpu::TexelCopyBufferLayout {
                offset: 0,
                bytes_per_row: Some(size * 4),
                rows_per_image: Some(size),
            },
        },
        wgpu::Extent3d {
            width: size,
            height: size,
            depth_or_array_layers: 1,
        },
    );
    gpu.queue.submit(Some(encoder.finish()));
    let (tx, rx) = std::sync::mpsc::channel();
    buffer
        .slice(..)
        .map_async(wgpu::MapMode::Read, move |result| tx.send(result).unwrap());
    gpu.device
        .poll(wgpu::PollType::Wait {
            submission_index: None,
            timeout: None,
        })
        .unwrap();
    rx.recv().unwrap().unwrap();
    buffer.slice(..).get_mapped_range().to_vec()
}

fn save(out: &str, size: u32, name: &str, pixels: &[u8]) {
    image::save_buffer(
        format!("{out}/{name}.png"),
        pixels,
        size,
        size,
        image::ColorType::Rgba8,
    )
    .unwrap();
    println!("saved {name}");
}

fn main() {
    let args: Vec<String> = std::env::args().collect();
    let out = args
        .get(1)
        .expect("Usage: realism_capture OUTPUT_DIR [SEED] [SIZE]");
    let seed = args
        .get(2)
        .map(|v| v.parse::<u32>().expect("seed must be a u32"))
        .unwrap_or(42);
    let size = args
        .get(3)
        .map(|v| v.parse::<u32>().expect("size must be a u32"))
        .unwrap_or(768);
    assert!(
        (64..=1536).contains(&size) && size % 64 == 0,
        "size must be 64..1536 and divisible by 64"
    );
    std::fs::create_dir_all(out).unwrap();
    let gpu = GpuContext::new().unwrap();
    println!("adapter={}", gpu.adapter_info.name);
    let params = PlanetParams {
        seed,
        ..PlanetParams::default()
    };
    let derived = DerivedProperties::from_params(&params);
    let ocean_level = -0.5 + 1.7 * derived.ocean_fraction * 0.5;
    let plate_params = PlateGenParams {
        seed: params.seed,
        mass_earth: params.mass_earth,
        ocean_fraction: derived.ocean_fraction * 0.5,
        tectonics_factor: derived.tectonics_factor,
        continental_scale: 1.0,
        num_plates_override: 0,
        num_continents: 4,
        continent_size_variety: 0.35,
    };
    let plates = generate_plates(&plate_params);
    let compute = TerrainComputePipeline::new(&gpu);
    let mut renderer = PreviewRenderer::new(&gpu);
    renderer.resize_target(&gpu, size, |_| {});
    let gain = 2.0_f32.powf(-(1.47_f32 - 1.0) / 2.0);
    let generate = |plates: &[_], width: f32| {
        compute.generate(
            &gpu,
            plates,
            size,
            params.seed,
            1.2,
            1.5,
            (8.0 + 4.0 * (params.axial_tilt_deg / 90.0) * derived.tectonics_factor) as u32,
            gain,
            2.1,
            1.0,
            width,
            1.0,
            1.0,
            derived.surface_gravity,
            derived.tectonics_factor,
            derived.surface_age,
            1.0,
        )
    };
    let terrain = generate(&plates, 0.10);
    let mut velocity_changed = plates.clone();
    for plate in &mut velocity_changed {
        plate.velocity = [0.0; 3];
    }
    let zero_velocity_terrain = generate(&velocity_changed, 0.10);
    let wide_terrain = generate(&plates, 0.25);
    assert_ne!(
        terrain.faces, zero_velocity_terrain.faces,
        "plate velocity must influence terrain"
    );
    println!("plate_velocity_affects_terrain=true");
    assert_ne!(
        terrain.faces, wide_terrain.faces,
        "range width must influence terrain"
    );
    println!("range_width_affects_terrain=true");
    let mut area = 0.0f64;
    let mut wet = 0.0f64;
    for face in &terrain.faces {
        for (i, &height) in face.iter().enumerate() {
            let x = 2.0 * (i % size as usize) as f64 / (size - 1) as f64 - 1.0;
            let y = 2.0 * (i / size as usize) as f64 / (size - 1) as f64 - 1.0;
            let weight = (1.0 + x * x + y * y).powf(-1.5);
            area += weight;
            if height < ocean_level {
                wet += weight;
            }
        }
    }
    println!(
        "water_budget={} measured_solid_angle_ocean={} pressure_bar={} scale_height_km={} shell_km={}",
        derived.ocean_fraction * 0.5,
        wet / area,
        derived.surface_pressure_bar,
        derived.rayleigh_scale_height_km(),
        derived.atmosphere_shell_height() * derived.radius_km
    );
    let terrain_view = renderer.upload_terrain(&gpu, &terrain);
    let wind = WindFieldPipeline::new(&gpu).unwrap();
    let dynamics = wind.create_textures(&gpu, 384, &terrain, ocean_level);
    let weather_pipeline = WeatherFieldPipeline::new(&gpu).unwrap();
    let weather = weather_pipeline.create_textures(&gpu, 384);
    let snapshot = WeatherSnapshot {
        resolution: 384,
        seed: params.seed.wrapping_add(1000),
        storm_count: 2,
        coverage: 0.5,
        moisture: 1.0,
        surface_pressure_bar: derived.surface_pressure_bar,
        base_temp_c: derived.base_temperature_c,
        ocean_level,
        axial_tilt_rad: params.axial_tilt_deg.to_radians(),
        season: 0.5,
        storm_size: 1.0,
        radius_km: derived.radius_km,
        rotation_rate_rad_s: derived.rotation_rate_rad_s,
        wind_scale: 1.0,
        face: 0,
    };
    wind.generate_gpu(
        &gpu,
        &terrain,
        &dynamics,
        snapshot.seed,
        ocean_level,
        snapshot.axial_tilt_rad,
        snapshot.season,
        earth_relative_rotation_rate(snapshot.rotation_rate_rad_s),
        snapshot.base_temp_c,
        snapshot.surface_pressure_bar,
        snapshot.wind_scale,
    );
    weather_pipeline.generate(&gpu, snapshot, &terrain, &dynamics, &weather);
    let mut u = PreviewUniforms::zeroed();
    u.rotation = [
        [1.0, 0.0, 0.0, 0.0],
        [0.0, 1.0, 0.0, 0.0],
        [0.0, 0.0, 1.0, 0.0],
        [0.0, 0.0, 0.0, 1.0],
    ];
    u.light_dir = [
        (-0.5_f32).cos() * 0.3_f32.cos(),
        0.3_f32.sin(),
        (-0.5_f32).sin() * 0.3_f32.cos(),
    ];
    u.ocean_level = ocean_level;
    u.base_temp_c = derived.base_temperature_c;
    u.ocean_fraction = derived.ocean_fraction;
    u.axial_tilt_rad = snapshot.axial_tilt_rad;
    u.season = 0.5;
    u.atmosphere_density = derived.surface_pressure_bar;
    u.atmosphere_height = derived.atmosphere_shell_height();
    u.height_scale = 3.0;
    u.zoom = 1.0;
    u.cloud_coverage = 0.5;
    u.cloud_seed = snapshot.seed;
    u.star_color_temp = 0.5;
    u.show_ao = 1.0;
    u.show_water = 1.0;
    u.show_ice = 1.0;
    u.show_biomes = 1.0;
    u.show_clouds = 1.0;
    u.show_cloud_shadows = 1.0;
    u.show_atmosphere_layer = 1.0;
    u.show_cities = 1.0;
    u.cloud_opacity = 1.0;
    u.cloud_advection = 1.0;
    u.rotation_rate = 1.0;
    u.atm_pressure = derived.surface_pressure_bar;
    u.planet_radius_km = derived.radius_km;
    let daylight = [0.5, 0.3, 1.0];
    let night = [0.1, 0.0, -1.0];
    let cases = [
        ("actual-default", u),
        (
            "actual-no-clouds",
            PreviewUniforms {
                show_clouds: 0.0,
                show_cloud_shadows: 0.0,
                ..u
            },
        ),
        (
            "actual-no-atmosphere",
            PreviewUniforms {
                atmosphere_density: 0.0,
                show_atmosphere_layer: 0.0,
                ..u
            },
        ),
        (
            "actual-surface",
            PreviewUniforms {
                show_clouds: 0.0,
                show_cloud_shadows: 0.0,
                atmosphere_density: 0.0,
                show_atmosphere_layer: 0.0,
                ..u
            },
        ),
        ("actual-density", PreviewUniforms { view_mode: 9, ..u }),
        (
            "density-closeup",
            PreviewUniforms {
                view_mode: 9,
                zoom: 1.55,
                ..u
            },
        ),
        (
            "daylight",
            PreviewUniforms {
                light_dir: daylight,
                ..u
            },
        ),
        (
            "daylight-closeup",
            PreviewUniforms {
                light_dir: daylight,
                zoom: 1.55,
                ..u
            },
        ),
        (
            "daylight-surface",
            PreviewUniforms {
                light_dir: daylight,
                show_clouds: 0.0,
                show_cloud_shadows: 0.0,
                atmosphere_density: 0.0,
                show_atmosphere_layer: 0.0,
                ..u
            },
        ),
        (
            "backlit",
            PreviewUniforms {
                light_dir: [0.0, 0.0, -1.0],
                ..u
            },
        ),
        (
            "night-clouds",
            PreviewUniforms {
                light_dir: night,
                atmosphere_density: 0.0,
                show_atmosphere_layer: 0.0,
                ..u
            },
        ),
        (
            "night-clear",
            PreviewUniforms {
                light_dir: night,
                show_clouds: 0.0,
                show_cloud_shadows: 0.0,
                atmosphere_density: 0.0,
                show_atmosphere_layer: 0.0,
                ..u
            },
        ),
    ];
    let no_detail = PreviewRenderer::new_with_cloud_detail(&gpu, 0.0);
    for (name, settings) in cases {
        let png = renderer.render(
            &gpu,
            &settings,
            &terrain_view,
            Some(&dynamics.wind_continentality),
            Some((&weather.mass, &weather.geometry)),
            size,
        );
        save(out, size, name, &png);
        if matches!(
            name,
            "daylight" | "actual-density" | "density-closeup" | "daylight-closeup"
        ) {
            let smooth = no_detail.render(
                &gpu,
                &settings,
                &terrain_view,
                Some(&dynamics.wind_continentality),
                Some((&weather.mass, &weather.geometry)),
                size,
            );
            save(out, size, &format!("{name}-no-detail"), &smooth);
        }
        if name == "actual-default" || name == "daylight" || name == "actual-density" {
            renderer.render_interactive(
                &gpu,
                &settings,
                &terrain_view,
                Some(&dynamics.wind_continentality),
                Some((&weather.mass, &weather.geometry)),
            );
            let interactive = read_interactive(&gpu, &renderer, size);
            save(out, size, &format!("{name}-interactive"), &interactive);
            let max_delta = png
                .iter()
                .zip(&interactive)
                .map(|(a, b)| a.abs_diff(*b))
                .max()
                .unwrap();
            println!("{name} live_png_max_channel_delta={max_delta}");
            assert!(max_delta <= 2, "live/PNG encoding regression");
            let i = (size as usize / 2 * size as usize + size as usize / 2) * 4;
            println!(
                "{name} center_rgb offline={:?} interactive={:?}",
                &png[i..i + 3],
                &interactive[i..i + 3]
            );
        }
    }
}
