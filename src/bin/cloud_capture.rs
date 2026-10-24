use planet_gen::{
    cloud_synthesis::{DEFAULT_STORM_INTENSITY, weather_systems_with_storms},
    export::render_cloud_columns_equirect,
    gpu::GpuContext,
    planet::{DerivedProperties, PlanetParams},
    planet_file,
    plates::{PlateGenParams, generate_plates},
    scene,
    terrain_compute::{
        DynamicsTextures, ErosionClimate, ErosionPipeline, TectonicTerrain, TerrainComputePipeline,
        WindFieldPipeline, earth_relative_rotation_rate,
    },
    weather::{DEFAULT_WEATHER_RESOLUTION, WeatherFieldPipeline, WeatherSnapshot, WeatherTextures},
};

// Equirectangular maps of the exported cloud column, without a full export.
// Usage: cloud_capture OUTPUT_DIR [SEED] [WIDTH] [--cloud-settings PLANET_JSON]
//        [--file PLANET_JSON] [--synthesis STRENGTH] [--rows FIRST COUNT]
//        [--watch JOB_JSON]
// --synthesis defaults to the planet file's `cloud_synthesis`, else 1; --rows
// renders a band of rows only (for 8K crops); --watch re-renders whenever the
// job file changes, reusing the generated weather. --file renders a saved
// planet (terrain, weather and storms through the app's settings path);
// otherwise the default planet of SEED takes only the cloud settings of
// --cloud-settings.
//
// Writes, for the default planet of SEED (the planet's seed with --file):
//   clouds-SEED.png    two-stream cloud reflectance (white cloud on black),
//                      the brightness a satellite composite would record
//   depth-SEED.png     the exported optical depth (Y of cloud_density.exr) / 2
//   land-SEED.png      land mask at the same size, for overlays
//   columns-SEED.f32   raw CLOUD_EXR_CHANNELS floats, row-major, for analysis
fn main() {
    let args: Vec<String> = std::env::args().collect();
    let out = args
        .get(1)
        .cloned()
        .expect("Usage: cloud_capture OUTPUT_DIR [SEED] [WIDTH] [--cloud-settings PLANET_JSON] [--synthesis S]");
    let seed = args
        .get(2)
        .map(|v| v.parse::<u32>().expect("seed must be a u32"))
        .unwrap_or(42);
    let width = args
        .get(3)
        .map(|v| v.parse::<u32>().expect("width must be a u32"))
        .unwrap_or(2048);
    assert!(
        width >= 64 && width.is_multiple_of(2),
        "width must be even and >= 64"
    );
    let cloud_settings: serde_json::Value = args
        .windows(2)
        .find(|pair| pair[0] == "--cloud-settings")
        .map(|pair| {
            serde_json::from_slice(&std::fs::read(&pair[1]).expect("read cloud settings"))
                .expect("parse cloud settings")
        })
        .unwrap_or(serde_json::Value::Null);
    std::fs::create_dir_all(&out).unwrap();
    let gpu = GpuContext::new().unwrap();
    println!("adapter={}", gpu.adapter_info.name);

    let planet = match args.windows(2).find(|pair| pair[0] == "--file") {
        Some(pair) => file_planet(&gpu, &pair[1]),
        None => default_planet(&gpu, seed, &cloud_settings),
    };

    let strength = args
        .windows(2)
        .find(|pair| pair[0] == "--synthesis")
        .map(|pair| pair[1].parse::<f32>().expect("synthesis must be a number"))
        .unwrap_or(planet.strength);
    let rows = args
        .windows(3)
        .find(|w| w[0] == "--rows")
        .map(|w| (w[1].parse::<u32>().unwrap(), w[2].parse::<u32>().unwrap()));
    let capture = Capture {
        gpu: &gpu,
        terrain: &planet.terrain,
        dynamics: &planet.dynamics,
        weather: &planet.weather,
        snapshot: planet.snapshot,
        ocean_level: planet.ocean_level,
        thin_lift: planet.thin_lift,
        storm_intensity: planet.storm_intensity,
        out: out.clone(),
        seed: planet.seed,
    };
    capture.render(&Job {
        width,
        rows,
        strength,
        tag: String::new(),
        shader: None,
    });

    // Watch mode: keep the weather on the GPU and re-render whenever the job
    // file changes. A job is JSON: {"width", "rows": [first, count],
    // "synthesis", "tag", "shader": path to a cloud_synthesis.wgsl}.
    if let Some(job_path) = args
        .windows(2)
        .find(|w| w[0] == "--watch")
        .map(|w| w[1].clone())
    {
        let mut last = None;
        loop {
            let modified = std::fs::metadata(&job_path).and_then(|m| m.modified()).ok();
            if modified.is_some() && modified != last {
                last = modified;
                let Ok(text) = std::fs::read_to_string(&job_path) else {
                    continue;
                };
                let Ok(json) = serde_json::from_str::<serde_json::Value>(&text) else {
                    println!("job: invalid JSON");
                    continue;
                };
                if json["quit"].as_bool() == Some(true) {
                    break;
                }
                let job = Job {
                    width: json["width"].as_u64().map_or(width, |w| w as u32),
                    rows: json["rows"]
                        .as_array()
                        .map(|r| (r[0].as_u64().unwrap() as u32, r[1].as_u64().unwrap() as u32)),
                    strength: json["synthesis"].as_f64().map_or(strength, |v| v as f32),
                    tag: json["tag"].as_str().unwrap_or("watch").to_owned(),
                    shader: json["shader"].as_str().map(str::to_owned),
                };
                capture.render(&job);
                println!("job done {}", job.tag);
            }
            std::thread::sleep(std::time::Duration::from_millis(300));
        }
    }
}

/// The default planet of `seed`, with only the cloud settings of a planet file.
fn default_planet(gpu: &GpuContext, seed: u32, cloud_settings: &serde_json::Value) -> Planet {
    let setting = |name: &str, fallback: f32| {
        cloud_settings[name]
            .as_f64()
            .map(|value| value as f32)
            .unwrap_or(fallback)
    };
    let params = PlanetParams {
        seed,
        ..PlanetParams::default()
    };
    let derived = DerivedProperties::from_params(&params);
    let ocean_level = -0.5 + 1.7 * derived.ocean_fraction * 0.5;
    let plates = generate_plates(&PlateGenParams {
        seed: params.seed,
        mass_earth: params.mass_earth,
        ocean_fraction: derived.ocean_fraction * 0.5,
        tectonics_factor: derived.tectonics_factor,
        continental_scale: 1.0,
        num_plates_override: 0,
        num_continents: 4,
        continent_size_variety: 0.35,
    });
    let terrain_size = 512;
    let terrain = TerrainComputePipeline::new(gpu).generate(
        gpu,
        &plates,
        terrain_size,
        params.seed,
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
    let resolution = DEFAULT_WEATHER_RESOLUTION;
    let wind = WindFieldPipeline::new(gpu).unwrap();
    let dynamics = wind.create_textures(gpu, resolution, &terrain, ocean_level);
    let weather_pipeline = WeatherFieldPipeline::new(gpu).unwrap();
    let weather = weather_pipeline.create_textures(gpu, resolution);
    let snapshot = WeatherSnapshot {
        resolution,
        surface_seed: params.seed,
        ocean_fraction: derived.ocean_fraction * setting("climate_moisture", 1.0),
        seed: cloud_settings["cloud_seed"]
            .as_u64()
            .map(|seed| seed as u32)
            .unwrap_or_else(|| params.seed.wrapping_add(1000)),
        storm_count: cloud_settings["storm_count"]
            .as_u64()
            .map(|count| count as u32)
            .unwrap_or(2),
        coverage: setting("cloud_coverage", 0.5),
        moisture: setting("climate_moisture", 1.0),
        surface_pressure_bar: derived.surface_pressure_bar,
        base_temp_c: derived.base_temperature_c,
        ocean_level,
        axial_tilt_rad: params.axial_tilt_deg.to_radians(),
        season: setting("season", 0.5),
        storm_size: setting("storm_size", 1.0),
        radius_km: derived.radius_km,
        rotation_rate_rad_s: derived.rotation_rate_rad_s,
        wind_scale: setting("wind_scale", 1.0),
        face: 0,
    };
    wind.generate_gpu(
        gpu,
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
    weather_pipeline.generate(gpu, snapshot, &terrain, &dynamics, &weather);
    Planet {
        terrain,
        dynamics,
        weather,
        snapshot,
        ocean_level,
        thin_lift: setting("cloud_thin_lift", 0.0),
        strength: setting("cloud_synthesis", 1.0),
        storm_intensity: setting("storm_intensity", DEFAULT_STORM_INTENSITY),
        seed,
    }
}

/// A saved planet through the app's settings path, as `planet_capture` and
/// the export build it.
fn file_planet(gpu: &GpuContext, path: &str) -> Planet {
    let json = std::fs::read_to_string(path).expect("read planet file");
    let file = planet_file::checked_load(&json).expect("load planet file");
    let derived = scene::derived(&file);
    let ocean_level = scene::sea_level(&derived, file.water_loss);
    let terrain_ocean_level = scene::terrain_sea_level(&derived, file.water_loss);
    let plates = generate_plates(&scene::plate_params(&file, &derived));
    let plates =
        planet_gen::tectonic_history::plate_records(&plates, file.seed, file.geologic_time_myr);
    let res = planet_gen::preview::DEFAULT_PREVIEW_SIZE;
    let mut terrain = TerrainComputePipeline::new(gpu).generate_params(
        gpu,
        &plates,
        res,
        &scene::terrain_params(&file, &derived),
    );
    if file.show_erosion {
        ErosionPipeline::new(gpu)
            .erode(
                gpu,
                &mut terrain,
                scene::preview_erosion_iterations(res, file.erosion_iterations),
                terrain_ocean_level,
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
    let resolution = DEFAULT_WEATHER_RESOLUTION;
    let snapshot = scene::weather_snapshot(&file, &derived, resolution, ocean_level);
    let wind = WindFieldPipeline::new(gpu).unwrap();
    let dynamics = wind.create_textures(gpu, resolution, &terrain, terrain_ocean_level);
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
    let weather_pipeline = WeatherFieldPipeline::new(gpu).unwrap();
    let weather = weather_pipeline.create_textures(gpu, resolution);
    weather_pipeline.generate(gpu, snapshot, &terrain, &dynamics, &weather);
    Planet {
        terrain,
        dynamics,
        weather,
        snapshot,
        ocean_level,
        thin_lift: file.cloud_thin_lift,
        strength: file.cloud_synthesis,
        storm_intensity: file.storm_intensity,
        seed: file.seed,
    }
}

struct Planet {
    terrain: TectonicTerrain,
    dynamics: DynamicsTextures,
    weather: WeatherTextures,
    snapshot: WeatherSnapshot,
    ocean_level: f32,
    thin_lift: f32,
    strength: f32,
    storm_intensity: f32,
    seed: u32,
}

struct Job {
    width: u32,
    rows: Option<(u32, u32)>,
    strength: f32,
    tag: String,
    shader: Option<String>,
}

struct Capture<'a> {
    gpu: &'a GpuContext,
    terrain: &'a planet_gen::terrain_compute::TectonicTerrain,
    dynamics: &'a planet_gen::terrain_compute::DynamicsTextures,
    weather: &'a planet_gen::weather::WeatherTextures,
    snapshot: WeatherSnapshot,
    ocean_level: f32,
    thin_lift: f32,
    storm_intensity: f32,
    out: String,
    seed: u32,
}

impl Capture<'_> {
    fn render(&self, job: &Job) {
        let width = job.width;
        let (first_row, height) = job.rows.unwrap_or((0, width / 2));
        let synthesis =
            weather_systems_with_storms(&self.snapshot, job.strength, self.storm_intensity, |d| {
                terrain_height(self.terrain, d) < self.ocean_level
            });
        println!(
            "synthesis={} systems={}",
            job.strength,
            synthesis.system_count()
        );
        let source = job
            .shader
            .as_ref()
            .map(|path| std::fs::read_to_string(path).expect("read shader"));
        self.gpu
            .device
            .push_error_scope(wgpu::ErrorFilter::Validation);
        let started = std::time::Instant::now();
        let columns = render_cloud_columns_equirect(
            self.gpu,
            self.terrain,
            self.dynamics,
            self.weather,
            self.snapshot,
            self.thin_lift,
            &synthesis,
            source.as_deref(),
            width,
            job.rows,
        );
        if let Some(error) = pollster::block_on(self.gpu.device.pop_error_scope()) {
            println!("shader error: {error}");
            return;
        }
        let columns = columns.expect("render cloud columns");
        println!("columns_ms={:.0}", started.elapsed().as_secs_f64() * 1000.0);
        let channels = columns.len() / (width * height) as usize;
        let depth: Vec<f32> = columns.chunks_exact(channels).map(|c| c[0]).collect();

        // Two-stream reflectance of the column, R = a tau / (1 + a tau) with the
        // droplet asymmetry g = 0.85 (a = 0.075), as a satellite would see it.
        let reflectance = |tau: f32| {
            let scattered = 0.075 * tau.max(0.0);
            scattered / (1.0 + scattered)
        };
        let gray = |values: &[f32], map: &dyn Fn(f32) -> f32| -> Vec<u8> {
            values
                .iter()
                .map(|&v| (map(v).clamp(0.0, 1.0).powf(1.0 / 2.2) * 255.0).round() as u8)
                .collect()
        };
        // The exported depth is the solver's normalized column; the preview and
        // the Blender bridge turn it into a physical one with this curve.
        let physical = |d: f32| {
            let t = ((d - 0.05) / 0.30).clamp(0.0, 1.0);
            36.0 * (d - 0.05).max(0.0) * t.powf(0.6)
        };
        let suffix = if job.tag.is_empty() {
            format!("{}", self.seed)
        } else {
            format!("{}-{}", self.seed, job.tag)
        };
        let save = |name: &str, pixels: Vec<u8>| {
            image::save_buffer(
                format!("{}/{name}-{suffix}.png", self.out),
                &pixels,
                width,
                height,
                image::ColorType::L8,
            )
            .unwrap();
            println!("saved {name}-{suffix}.png");
        };
        save("clouds", gray(&depth, &|d| reflectance(physical(d))));
        save("depth", gray(&depth, &|d| d / 2.0));

        // Land mask from the terrain cube at each pixel centre.
        let mut land = Vec::with_capacity((width * height) as usize);
        for y in first_row..first_row + height {
            for x in 0..width {
                let lon =
                    std::f32::consts::TAU * (x as f32 + 0.5) / width as f32 - std::f32::consts::PI;
                let lat = std::f32::consts::PI * (0.5 - (y as f32 + 0.5) / (width / 2) as f32);
                let d = [lat.cos() * lon.sin(), lat.sin(), lat.cos() * lon.cos()];
                land.push(if terrain_height(self.terrain, d) >= self.ocean_level {
                    255u8
                } else {
                    0
                });
            }
        }
        save("land", land);
        let raw: Vec<u8> = columns.iter().flat_map(|v| v.to_le_bytes()).collect();
        std::fs::write(format!("{}/columns-{suffix}.f32", self.out), raw).unwrap();
        println!("channels={channels} width={width} height={height}");
    }
}

fn terrain_height(terrain: &planet_gen::terrain_compute::TectonicTerrain, d: [f32; 3]) -> f32 {
    let [dx, dy, dz] = d;
    let (ax, ay, az) = (dx.abs(), dy.abs(), dz.abs());
    let (face, s, t) = if ax >= ay && ax >= az {
        if dx > 0.0 {
            (0, -dz / ax, -dy / ax)
        } else {
            (1, dz / ax, -dy / ax)
        }
    } else if ay >= az {
        if dy > 0.0 {
            (2, dx / ay, dz / ay)
        } else {
            (3, dx / ay, -dz / ay)
        }
    } else if dz > 0.0 {
        (4, dx / az, -dy / az)
    } else {
        (5, -dx / az, -dy / az)
    };
    let res = terrain.resolution as usize;
    let x = (((s + 1.0) * 0.5) * (res - 1) as f32).round() as usize;
    let y = (((t + 1.0) * 0.5) * (res - 1) as f32).round() as usize;
    terrain.faces[face][y.min(res - 1) * res + x.min(res - 1)]
}
