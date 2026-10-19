use bytemuck::Zeroable;
use planet_gen::{
    cloud_synthesis::weather_systems,
    gpu::GpuContext,
    planet::{DerivedProperties, PlanetParams},
    plates::{PlateGenParams, generate_plates},
    preview::{PreviewRenderer, PreviewUniforms},
    terrain_compute::{TerrainComputePipeline, WindFieldPipeline, earth_relative_rotation_rate},
    weather::{WeatherFieldPipeline, WeatherSnapshot},
};

// Lit preview close-ups of the clouds at chosen places, as the app shows them
// when zoomed in, for judging cloud structure at 1 km per pixel.
// Usage: cloud_closeup OUTPUT_DIR [SEED] [SIZE] [--cloud-settings PLANET_JSON]
//        [--rotation-hours H] [--views LAT,LON,ZOOM;...] [--watch JOB_JSON]
// Writes view-LAT-LON-ZOOM[-TAG].png per view. --watch re-renders whenever
// the job file changes, reusing the generated weather. A job is JSON:
// {"tag", "shader": path to a cloud_synthesis.wgsl, "views": "LAT,LON,ZOOM;...",
// "synthesis": strength, "quit": true}.
fn main() {
    let args: Vec<String> = std::env::args().collect();
    let arg = |name: &str| {
        args.windows(2)
            .find(|pair| pair[0] == name)
            .map(|pair| pair[1].clone())
    };
    let cloud_settings: serde_json::Value = arg("--cloud-settings")
        .map(|path| {
            serde_json::from_slice(&std::fs::read(path).expect("read cloud settings"))
                .expect("parse cloud settings")
        })
        .unwrap_or(serde_json::Value::Null);
    let setting = |name: &str, fallback: f32| {
        cloud_settings[name]
            .as_f64()
            .map(|value| value as f32)
            .unwrap_or(fallback)
    };
    let wind_override: Option<f32> = arg("--wind-scale").map(|v| v.parse().expect("wind scale"));
    let out = args
        .get(1)
        .expect("Usage: cloud_closeup OUTPUT_DIR [SEED] [SIZE]")
        .clone();
    let seed = args
        .get(2)
        .filter(|v| !v.starts_with("--"))
        .map(|v| v.parse::<u32>().expect("seed must be a u32"))
        .unwrap_or(42);
    let size = args
        .get(3)
        .filter(|v| !v.starts_with("--"))
        .map(|v| v.parse::<u32>().expect("size must be a u32"))
        .unwrap_or(512);
    assert!(size.is_multiple_of(64), "size must be divisible by 64");
    std::fs::create_dir_all(&out).unwrap();
    let gpu = GpuContext::new().unwrap();
    println!("adapter={}", gpu.adapter_info.name);
    let mut params = PlanetParams {
        seed,
        ..PlanetParams::default()
    };
    if let Some(hours) = arg("--rotation-hours") {
        params.rotation_period_h = hours.parse().expect("rotation hours");
    }
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
    let wind = WindFieldPipeline::new(&gpu).unwrap();
    let dynamics = wind.create_textures(&gpu, 384, &terrain, ocean_level);
    let weather_pipeline = WeatherFieldPipeline::new(&gpu).unwrap();
    let weather = weather_pipeline.create_textures(&gpu, 384);
    let snapshot = WeatherSnapshot {
        resolution: 384,
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
        wind_scale: if let Some(value) = wind_override {
            value
        } else if cloud_settings["show_wind_effects"].as_bool() == Some(false) {
            0.0
        } else {
            setting("wind_scale", 1.0)
        },
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
    // Weather systems for cloud synthesis (Clouds > Structure), as the app uploads them.
    let terrain_height = |direction: [f32; 3]| {
        let [x, y, z] = direction;
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
    };
    let synthesis = weather_systems(&snapshot, setting("cloud_synthesis", 1.0), |direction| {
        terrain_height(direction) < ocean_level
    });
    let mut views = parse_views(
        &arg("--views").unwrap_or_else(|| "0,0,20;30,-60,20;-45,100,20;10,150,6".to_owned()),
    );
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
    u.ocean_fraction = snapshot.ocean_fraction;
    u.axial_tilt_rad = snapshot.axial_tilt_rad;
    u.season = snapshot.season;
    u.atmosphere_density = derived.surface_pressure_bar;
    u.atmosphere_height = derived.atmosphere_shell_height();
    u.height_scale = 3.0;
    u.zoom = 1.0;
    u.cloud_coverage = snapshot.coverage;
    u.cloud_seed = snapshot.seed;
    u.surface_seed = params.seed;
    u.star_color_temp = 0.5;
    u.show_ao = 1.0;
    u.show_water = 1.0;
    u.show_ice = 1.0;
    u.show_biomes = 1.0;
    u.show_clouds = 1.0;
    u.show_cloud_shadows = 1.0;
    u.show_atmosphere_layer = 1.0;
    u.show_cities = 1.0;
    u.cloud_opacity = setting("cloud_opacity", 1.0);
    u.cloud_detail = setting("cloud_detail", 1.0);
    u.cloud_thin_lift = setting("cloud_thin_lift", 0.0);
    u.cloud_advection = 1.0;
    u.rotation_rate = earth_relative_rotation_rate(derived.rotation_rate_rad_s);
    u.atm_pressure = derived.surface_pressure_bar;
    u.planet_radius_km = derived.radius_km;
    let render = |renderer: &mut PreviewRenderer,
                  synthesis: &planet_gen::cloud_synthesis::CloudSynthesisGpu,
                  views: &[(f32, f32, f32)],
                  tag: &str| {
        renderer.resize_target(&gpu, size, |_| {});
        renderer.upload_cloud_synthesis(&gpu, synthesis);
        let terrain_view = renderer.upload_terrain(&gpu, &terrain);
        for &(lat, lon, zoom) in views {
            let (la, lo) = (lat.to_radians(), lon.to_radians());
            let target = [la.cos() * lo.sin(), la.sin(), la.cos() * lo.cos()];
            let east = [lo.cos(), 0.0, -lo.sin()];
            let north = [
                target[1] * east[2] - target[2] * east[1],
                target[2] * east[0] - target[0] * east[2],
                target[0] * east[1] - target[1] * east[0],
            ];
            let settings = PreviewUniforms {
                rotation: [
                    [east[0], east[1], east[2], 0.0],
                    [north[0], north[1], north[2], 0.0],
                    [target[0], target[1], target[2], 0.0],
                    [0.0, 0.0, 0.0, 1.0],
                ],
                light_dir: [0.35, 0.45, 0.82],
                zoom,
                ..u
            };
            let pixels = renderer.render(
                &gpu,
                &settings,
                &terrain_view,
                Some(&dynamics.wind_continentality),
                Some((&weather.mass, &weather.geometry)),
                size,
            );
            let name = if tag.is_empty() {
                format!("view{lat}_{lon}_z{zoom}")
            } else {
                format!("view{lat}_{lon}_z{zoom}-{tag}")
            };
            image::save_buffer(
                format!("{out}/{name}.png"),
                &pixels,
                size,
                size,
                image::ColorType::Rgba8,
            )
            .unwrap();
            println!("saved {name}");
        }
    };
    let mut renderer = PreviewRenderer::new(&gpu);
    render(&mut renderer, &synthesis, &views, "");
    let Some(job_path) = arg("--watch") else {
        return;
    };
    let mut last = None;
    loop {
        let modified = std::fs::metadata(&job_path).and_then(|m| m.modified()).ok();
        if modified.is_some() && modified != last {
            last = modified;
            let Ok(json) = std::fs::read_to_string(&job_path)
                .map_err(|_| ())
                .and_then(|text| serde_json::from_str::<serde_json::Value>(&text).map_err(|_| ()))
            else {
                println!("job: invalid JSON");
                continue;
            };
            if json["quit"].as_bool() == Some(true) {
                break;
            }
            if let Some(list) = json["views"].as_str() {
                views = parse_views(list);
            }
            let tag = json["tag"].as_str().unwrap_or("watch").to_owned();
            let strength = json["synthesis"]
                .as_f64()
                .map_or(setting("cloud_synthesis", 1.0), |v| v as f32);
            let synthesis = weather_systems(&snapshot, strength, |direction| {
                terrain_height(direction) < ocean_level
            });
            gpu.device.push_error_scope(wgpu::ErrorFilter::Validation);
            let mut renderer = match json["shader"].as_str() {
                Some(path) => PreviewRenderer::new_with_synthesis_source(
                    &gpu,
                    &std::fs::read_to_string(path).expect("read shader"),
                ),
                None => PreviewRenderer::new(&gpu),
            };
            if let Some(error) = pollster::block_on(gpu.device.pop_error_scope()) {
                println!("shader error: {error}");
                continue;
            }
            render(&mut renderer, &synthesis, &views, &tag);
            println!("job done {tag}");
        }
        std::thread::sleep(std::time::Duration::from_millis(300));
    }
}

fn parse_views(list: &str) -> Vec<(f32, f32, f32)> {
    list.split(';')
        .filter(|view| !view.trim().is_empty())
        .map(|view| {
            let v: Vec<f32> = view
                .split(',')
                .map(|x| x.trim().parse().expect("view number"))
                .collect();
            (v[0], v[1], v[2])
        })
        .collect()
}
