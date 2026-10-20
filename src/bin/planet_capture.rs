//! Headless render of presets or saved planet files through the app's own
//! settings path (`scene`, `presets`), for before/after reviews and
//! reference comparisons.
//!
//! cargo run --release --bin planet_capture -- OUT_DIR [--preset NAME]... [--file PLANET.json]...
//!     [--size 768] [--res 768] [--seed N]
//!
//! Writes per planet: `NAME-globe.png` (the file's own light), `NAME-full.png`
//! (zero phase, four longitudes), `NAME-terminator.png` (low sun),
//! `NAME-closeup.png` (zoom 4 at low sun), `NAME-albedo.png` (unlit surface
//! albedo), `NAME-height.png`, and `NAME-stats.json` (hypsometry in km and
//! the disk-mean linear albedo).

use planet_gen::{
    cloud_synthesis::weather_systems_with_storms,
    gpu::GpuContext,
    planet_file::{self, PlanetFile},
    plates::generate_plates,
    presets,
    preview::{PreviewRenderer, PreviewUniforms},
    scene,
    terrain_compute::{
        ErosionClimate, ErosionPipeline, TectonicTerrain, TerrainComputePipeline,
        WindFieldPipeline, earth_relative_rotation_rate,
    },
    weather::{DEFAULT_WEATHER_RESOLUTION, WeatherFieldPipeline},
};

fn cube_height(terrain: &TectonicTerrain, [x, y, z]: [f32; 3]) -> f32 {
    let (ax, ay, az) = (x.abs(), y.abs(), z.abs());
    let (face, s, t) = if ax >= ay && ax >= az {
        if x > 0.0 {
            (0, -z / ax, -y / ax)
        } else {
            (1, z / ax, -y / ax)
        }
    } else if ay >= az {
        if y > 0.0 {
            (2, x / ay, z / ay)
        } else {
            (3, x / ay, -z / ay)
        }
    } else if z > 0.0 {
        (4, x / az, -y / az)
    } else {
        (5, -x / az, -y / az)
    };
    let res = terrain.resolution as usize;
    let column = (((s + 1.0) * 0.5) * (res - 1) as f32).round() as usize;
    let row = (((t + 1.0) * 0.5) * (res - 1) as f32).round() as usize;
    terrain.faces[face][row.min(res - 1) * res + column.min(res - 1)]
}

fn rotation_y(longitude_deg: f32) -> [[f32; 3]; 3] {
    let (s, c) = longitude_deg.to_radians().sin_cos();
    [[c, 0.0, s], [0.0, 1.0, 0.0], [-s, 0.0, c]]
}

fn with_view(u: &PreviewUniforms, rot: [[f32; 3]; 3]) -> PreviewUniforms {
    PreviewUniforms {
        rotation: [
            [rot[0][0], rot[0][1], rot[0][2], 0.0],
            [rot[1][0], rot[1][1], rot[1][2], 0.0],
            [rot[2][0], rot[2][1], rot[2][2], 0.0],
            [0.0, 0.0, 0.0, 1.0],
        ],
        ..*u
    }
}

fn light(azimuth: f32, elevation: f32) -> [f32; 3] {
    [
        azimuth.cos() * elevation.cos(),
        elevation.sin(),
        azimuth.sin() * elevation.cos(),
    ]
}

fn srgb_to_linear(v: u8) -> f64 {
    let c = v as f64 / 255.0;
    if c <= 0.04045 {
        c / 12.92
    } else {
        ((c + 0.055) / 1.055).powf(2.4)
    }
}

fn save(path: &str, size: u32, pixels: &[u8]) {
    image::save_buffer(path, pixels, size, size, image::ColorType::Rgba8).unwrap();
    println!("saved {path}");
}

/// Tile four square renders into a 2x2 image.
fn montage(tiles: &[Vec<u8>; 4], size: u32) -> Vec<u8> {
    let s = size as usize;
    let mut out = vec![0u8; 4 * s * s * 4];
    for (index, tile) in tiles.iter().enumerate() {
        let (ox, oy) = ((index % 2) * s, (index / 2) * s);
        for row in 0..s {
            let src = &tile[row * s * 4..(row + 1) * s * 4];
            let start = ((oy + row) * 2 * s + ox) * 4;
            out[start..start + s * 4].copy_from_slice(src);
        }
    }
    out
}

fn percentile(sorted: &[f32], p: f64) -> f32 {
    sorted[((sorted.len() - 1) as f64 * p).round() as usize]
}

fn capture(gpu: &GpuContext, out: &str, name: &str, file: &PlanetFile, size: u32, res: u32) {
    let derived = scene::derived(file);
    let ocean_level = scene::sea_level(&derived, file.water_loss);
    let plates = generate_plates(&scene::plate_params(file, &derived));
    let plates =
        planet_gen::tectonic_history::plate_records(&plates, file.seed, file.geologic_time_myr);
    let tp = scene::terrain_params(file, &derived);
    let compute = TerrainComputePipeline::new(gpu);
    let started = std::time::Instant::now();
    let mut terrain = compute.generate(
        gpu,
        &plates,
        res,
        tp.seed,
        tp.amplitude,
        tp.frequency,
        tp.octaves,
        tp.gain,
        tp.lacunarity,
        tp.mountain_scale,
        tp.boundary_width,
        tp.warp_strength,
        tp.detail_scale,
        tp.surface_gravity,
        tp.tectonics_factor,
        tp.surface_age,
        tp.continental_scale,
    );
    println!("{name}: terrain {:.1}s", started.elapsed().as_secs_f32());
    if file.show_erosion {
        let iterations = scene::preview_erosion_iterations(res, file.erosion_iterations);
        ErosionPipeline::new(gpu)
            .erode(
                gpu,
                &mut terrain,
                iterations,
                ocean_level,
                ErosionClimate {
                    seed: file.seed,
                    base_temp_c: derived.base_temperature_c,
                    axial_tilt_rad: file.axial_tilt_deg.to_radians(),
                    moisture: file.climate_moisture,
                    ocean_fraction: derived.ocean_fraction,
                },
            )
            .expect("erosion");
    }

    let mut renderer = PreviewRenderer::new(gpu);
    renderer.resize_target(gpu, size, |_| {});
    renderer.upload_plates(gpu, &plates, tp.seed, tp.warp_strength);
    let terrain_view = renderer.upload_terrain(gpu, &terrain);
    let wind = WindFieldPipeline::new(gpu).unwrap();
    let dynamics = wind.create_textures(gpu, (res / 2).max(192), &terrain, ocean_level);
    let weather_pipeline = WeatherFieldPipeline::new(gpu).unwrap();
    let weather = weather_pipeline.create_textures(gpu, DEFAULT_WEATHER_RESOLUTION);
    let snapshot = scene::weather_snapshot(file, &derived, DEFAULT_WEATHER_RESOLUTION, ocean_level);
    wind.generate_gpu(
        gpu,
        &terrain,
        &dynamics,
        snapshot.seed,
        snapshot.ocean_level,
        snapshot.axial_tilt_rad,
        snapshot.season,
        earth_relative_rotation_rate(snapshot.rotation_rate_rad_s),
        snapshot.base_temp_c,
        snapshot.surface_pressure_bar,
        snapshot.wind_scale,
    );
    weather_pipeline.generate(gpu, snapshot, &terrain, &dynamics, &weather);
    renderer.upload_cloud_synthesis(
        gpu,
        &weather_systems_with_storms(
            &snapshot,
            file.cloud_synthesis,
            file.storm_intensity,
            |direction| cube_height(&terrain, direction) < snapshot.ocean_level,
        ),
    );

    let base = scene::preview_uniforms(file, &derived, Some(snapshot));
    let base = PreviewUniforms {
        zoom: 1.0,
        pan_x: 0.0,
        pan_y: 0.0,
        view_mode: 0,
        ..base
    };
    let render = |u: &PreviewUniforms| {
        renderer.render(
            gpu,
            u,
            &terrain_view,
            Some(&dynamics.wind_continentality),
            Some((&weather.mass, &weather.geometry)),
            size,
        )
    };
    let front = rotation_y(0.0);
    let path = |view: &str| format!("{out}/{name}-{view}.png");

    save(&path("globe"), size, &render(&with_view(&base, front)));
    let full = PreviewUniforms {
        light_dir: light(std::f32::consts::FRAC_PI_2, 0.0),
        ..base
    };
    let tiles = [0.0, 90.0, 180.0, 270.0].map(|lon| render(&with_view(&full, rotation_y(lon))));
    let full_montage = montage(&tiles, size);
    image::save_buffer(
        path("full"),
        &full_montage,
        size * 2,
        size * 2,
        image::ColorType::Rgba8,
    )
    .unwrap();
    println!("saved {}", path("full"));
    let low_sun = PreviewUniforms {
        light_dir: light(0.25, 0.12),
        ..base
    };
    save(
        &path("terminator"),
        size,
        &render(&with_view(&low_sun, front)),
    );
    save(
        &path("closeup"),
        size,
        &render(&with_view(
            &PreviewUniforms {
                zoom: 4.0,
                pan_x: -0.35,
                pan_y: -0.1,
                ..low_sun
            },
            front,
        )),
    );
    let albedo = render(&with_view(
        &PreviewUniforms {
            view_mode: 19,
            ..base
        },
        front,
    ));
    save(&path("albedo"), size, &albedo);
    save(
        &path("height"),
        size,
        &render(&with_view(
            &PreviewUniforms {
                view_mode: 1,
                ..base
            },
            front,
        )),
    );

    // Disk-mean linear albedo of the front hemisphere (inside 0.8 of the
    // disk radius, away from the limb), weighted equally per pixel.
    let mut sum = [0.0f64; 3];
    let mut count = 0.0f64;
    let half = size as f64 * 0.5;
    let disk = half * 0.85;
    for y in 0..size as usize {
        for x in 0..size as usize {
            let (dx, dy) = (x as f64 + 0.5 - half, y as f64 + 0.5 - half);
            if dx * dx + dy * dy > (disk * 0.8).powi(2) {
                continue;
            }
            let i = (y * size as usize + x) * 4;
            for c in 0..3 {
                sum[c] += srgb_to_linear(albedo[i + c]);
            }
            count += 1.0;
        }
    }
    let mean = sum.map(|v| v / count.max(1.0));
    let mut heights: Vec<f32> = terrain.faces.iter().flatten().map(|h| h * 5.0).collect();
    heights.sort_by(f32::total_cmp);
    let stats = serde_json::json!({
        "name": name,
        "pressure_bar": derived.surface_pressure_bar,
        "base_temperature_c": derived.base_temperature_c,
        "ocean_fraction": derived.ocean_fraction,
        "tectonics_factor": derived.tectonics_factor,
        "ocean_level_km": ocean_level * 5.0,
        "ocean_coverage": terrain.solid_angle_ocean_coverage(ocean_level),
        "height_km": {
            "min": heights[0],
            "p01": percentile(&heights, 0.01),
            "p10": percentile(&heights, 0.10),
            "p50": percentile(&heights, 0.50),
            "p90": percentile(&heights, 0.90),
            "p99": percentile(&heights, 0.99),
            "max": heights[heights.len() - 1],
        },
        "albedo_linear_rgb": mean,
        "albedo_red_over_blue": mean[0] / mean[2].max(1e-6),
    });
    std::fs::write(
        format!("{out}/{name}-stats.json"),
        serde_json::to_string_pretty(&stats).unwrap(),
    )
    .unwrap();
    println!("{stats}");
}

fn main() {
    let args: Vec<String> = std::env::args().collect();
    let out = args
        .get(1)
        .filter(|arg| !arg.starts_with("--"))
        .expect("planet_capture OUT_DIR [--preset NAME]... [--file PLANET.json]... [--size 768] [--res 768] [--seed N]")
        .clone();
    let value = |flag: &str| {
        args.windows(2)
            .find(|pair| pair[0] == flag)
            .map(|pair| pair[1].clone())
    };
    let size: u32 = value("--size").map_or(768, |v| v.parse().unwrap());
    let res: u32 = value("--res").map_or(planet_gen::preview::DEFAULT_PREVIEW_SIZE, |v| {
        v.parse().unwrap()
    });
    let seed: Option<u32> = value("--seed").map(|v| v.parse().unwrap());
    assert!(
        (64..=1536).contains(&size) && size.is_multiple_of(64),
        "size must be 64..1536 and divisible by 64"
    );
    std::fs::create_dir_all(&out).unwrap();

    let mut planets = Vec::new();
    for pair in args.windows(2) {
        match pair[0].as_str() {
            "--preset" => {
                let name = pair[1].to_uppercase();
                assert!(
                    presets::PRESETS.contains(&name.as_str()),
                    "unknown preset {name}; presets: {:?}",
                    presets::PRESETS
                );
                planets.push((name.to_lowercase(), presets::preset_file(&name)));
            }
            "--file" => {
                let json = std::fs::read_to_string(&pair[1]).expect("read planet file");
                let file = planet_file::checked_load(&json).expect("load planet file");
                let stem = std::path::Path::new(&pair[1])
                    .file_stem()
                    .unwrap()
                    .to_string_lossy()
                    .into_owned();
                planets.push((stem, file));
            }
            _ => {}
        }
    }
    if planets.is_empty() {
        planets.push(("earth".to_owned(), presets::preset_file("EARTH")));
    }
    let gpu = GpuContext::new().unwrap();
    println!("adapter={}", gpu.adapter_info.name);
    for (name, mut file) in planets {
        if let Some(seed) = seed {
            file.seed = seed;
            file.cloud_seed = seed.wrapping_add(1000);
        }
        capture(&gpu, &out, &name, &file, size, res);
    }
}
