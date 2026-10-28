//! Planet settings to pipeline inputs. The app and the headless capture tool
//! both go through these functions, so a capture renders a planet file the
//! way the app does.

use crate::planet::{DerivedProperties, PlanetParams, SurfaceOverrides};
use crate::planet_file::PlanetFile;
use crate::plates::PlateGenParams;
use crate::preview::{GiantUniforms, PreviewUniforms, RockyUniforms};
use crate::terrain_compute::{RockyTerrainParams, TerrainGenerationParams};
use crate::weather::WeatherSnapshot;

pub fn planet_params(file: &PlanetFile) -> PlanetParams {
    PlanetParams {
        star_distance_au: file.star_distance_au,
        mass_earth: file.mass_earth,
        metallicity: file.metallicity,
        axial_tilt_deg: file.axial_tilt_deg,
        rotation_period_h: file.rotation_period_h,
        seed: file.seed,
    }
}

pub fn surface_overrides(file: &PlanetFile) -> SurfaceOverrides {
    SurfaceOverrides {
        pressure_bar: file.pressure_override,
        tectonics: file.tectonics_override,
        crater_age_gyr: file.crater_age_override,
        mare_flooding: file.mare_override,
        shield_volcanism: file.volcanism_override,
        dichotomy: file.dichotomy,
        dust: file.dust_override,
        envelope: file.envelope_override,
        ice_crust: file.ice_override,
    }
}

pub fn derived(file: &PlanetFile) -> DerivedProperties {
    DerivedProperties::from_params_with(&planet_params(file), &surface_overrides(file))
}

/// Sea level of a world that cannot hold liquid water: far below any relief.
const DRY_SEA_LEVEL: f32 = -4.0;

fn sea_level_base(derived: &DerivedProperties) -> f32 {
    if derived.liquid_water >= 1.0 {
        -0.5
    } else {
        DRY_SEA_LEVEL + (-0.5 - DRY_SEA_LEVEL) * derived.liquid_water
    }
}

/// Sea level in terrain units (1 unit = 5 km) for a water-loss setting, as
/// the preview shades and the weather sees it. Below water's triple point
/// the basins stay dry whatever the inventory.
pub fn sea_level(derived: &DerivedProperties, water_loss: f32) -> f32 {
    sea_level_base(derived) + 1.7 * derived.ocean_fraction * (1.0 - water_loss)
}

/// The same sea level as the terrain build, erosion and export compute it:
/// from the effective ocean fraction, which rounds differently in f32 for
/// some water-loss values. Kept separate so plate worlds stay bit-identical.
pub fn terrain_sea_level(derived: &DerivedProperties, water_loss: f32) -> f32 {
    sea_level_base(derived) + 1.7 * (derived.ocean_fraction * (1.0 - water_loss))
}

/// Erosion iterations the preview runs at a terrain resolution; low
/// resolutions erode less so the preview stays interactive.
pub fn preview_erosion_iterations(resolution: u32, iterations: u32) -> u32 {
    match resolution {
        r if r <= 256 => (iterations as f32 * 0.2) as u32,
        r if r <= 512 => (iterations as f32 * 0.4) as u32,
        r if r <= 768 => (iterations as f32 * 0.6) as u32,
        _ => iterations,
    }
    .max(1)
}

pub fn plate_params(file: &PlanetFile, derived: &DerivedProperties) -> PlateGenParams {
    PlateGenParams {
        seed: file.seed,
        mass_earth: file.mass_earth,
        ocean_fraction: derived.ocean_fraction * (1.0 - file.water_loss),
        tectonics_factor: derived.tectonics_factor,
        continental_scale: file.continental_scale,
        num_plates_override: file.num_plates_override,
        num_continents: file.num_continents,
        continent_size_variety: file.continent_size_variety,
    }
}

pub fn terrain_params(file: &PlanetFile, derived: &DerivedProperties) -> TerrainGenerationParams {
    derive_terrain_params(
        &planet_params(file),
        derived,
        file.continental_scale,
        file.mountain_scale,
        file.boundary_width,
        file.warp_strength,
        file.detail_scale,
        file.age_override,
        file.num_plates_override,
        file.num_continents,
        file.continent_size_variety,
        file.geologic_time_myr,
    )
}

#[expect(
    clippy::too_many_arguments,
    reason = "maps each independent planet parameter onto terrain params one-to-one"
)]
pub fn derive_terrain_params(
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
    geologic_time_myr: f32,
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
        geologic_time_myr,
        rocky: RockyTerrainParams::from_derived(derived),
    }
}

/// Cloud coverage the atmosphere can carry: below ~0.3 bar the air holds
/// too little vapour for decks (as the weather field's moisture supply), so
/// thin-air worlds like Mars stay clear apart from their dust.
/// Giants carry their clouds in the banded deck (giant.wgsl), not in the
/// weather layer.
pub fn cloud_coverage(file: &PlanetFile, derived: &DerivedProperties) -> f32 {
    let t = ((derived.surface_pressure_bar - 0.05) / 0.25).clamp(0.0, 1.0);
    let coverage = file.cloud_coverage * (t * t * (3.0 - 2.0 * t));
    if derived.envelope > 0.0 {
        coverage * (1.0 - derived.envelope)
    } else {
        coverage
    }
}

/// Weather inputs for the current settings; `ocean_level` is the sea level
/// the weather's terrain was generated against.
pub fn weather_snapshot(
    file: &PlanetFile,
    derived: &DerivedProperties,
    resolution: u32,
    ocean_level: f32,
) -> WeatherSnapshot {
    WeatherSnapshot {
        face: 0,
        resolution,
        seed: file.cloud_seed,
        surface_seed: file.seed,
        ocean_fraction: derived.ocean_fraction * file.climate_moisture,
        storm_count: file.storm_count,
        coverage: cloud_coverage(file, derived),
        moisture: file.climate_moisture,
        surface_pressure_bar: derived.surface_pressure_bar,
        base_temp_c: derived.base_temperature_c,
        ocean_level,
        axial_tilt_rad: file.axial_tilt_deg.to_radians(),
        season: file.season,
        storm_size: file.storm_size,
        radius_km: derived.radius_km,
        rotation_rate_rad_s: derived.rotation_rate_rad_s,
        wind_scale: if file.show_wind_effects {
            file.wind_scale
        } else {
            0.0
        },
    }
}

fn flag(value: bool) -> f32 {
    if value { 1.0 } else { 0.0 }
}

/// Preview uniforms for the current settings. `weather` is the snapshot whose
/// cubemaps are bound, when one has finished.
pub fn preview_uniforms(
    file: &PlanetFile,
    derived: &DerivedProperties,
    weather: Option<WeatherSnapshot>,
) -> PreviewUniforms {
    let rot = file.rot;
    PreviewUniforms {
        rotation: [
            [rot[0][0], rot[0][1], rot[0][2], 0.0],
            [rot[1][0], rot[1][1], rot[1][2], 0.0],
            [rot[2][0], rot[2][1], rot[2][2], 0.0],
            [0.0, 0.0, 0.0, 1.0],
        ],
        light_dir: [
            file.light_azimuth.cos() * file.light_elevation.cos(),
            file.light_elevation.sin(),
            file.light_azimuth.sin() * file.light_elevation.cos(),
        ],
        ocean_level: sea_level(derived, file.water_loss),
        base_temp_c: derived.base_temperature_c,
        // Climate moisture: physics ocean_fraction scaled by user's moisture slider.
        // water_loss controls sea level only; climate_moisture controls atmosphere wetness.
        ocean_fraction: derived.ocean_fraction * file.climate_moisture,
        axial_tilt_rad: file.axial_tilt_deg.to_radians(),
        view_mode: file.view_mode,
        season: file.season,
        atmosphere_density: if file.show_atmosphere {
            derived.rayleigh_density()
        } else {
            0.0
        },
        atmosphere_height: derived.atmosphere_shell_height(),
        height_scale: file.height_scale,
        zoom: file.zoom,
        pan_x: file.pan[0],
        pan_y: file.pan[1],
        cloud_coverage: weather
            .map(|snapshot| snapshot.coverage)
            .unwrap_or(cloud_coverage(file, derived)),
        cloud_seed: weather
            .map(|snapshot| snapshot.seed)
            .unwrap_or(file.cloud_seed),
        night_lights: file.night_lights,
        star_color_temp: file.star_color_temp,
        city_light_hue: file.city_light_hue,
        show_ao: flag(file.show_ao),
        show_water: flag(file.show_water),
        show_ice: flag(file.show_ice),
        show_biomes: flag(file.show_biomes),
        show_clouds: flag(file.show_clouds),
        show_atmosphere_layer: flag(file.show_atmosphere),
        show_cities: flag(file.show_cities),
        cloud_opacity: file.cloud_opacity,
        cloud_advection: flag(file.show_wind_effects),
        rotation_rate: derived.rotation_rate_rad_s / (std::f32::consts::TAU / 86400.0),
        atm_pressure: derived.surface_pressure_bar,
        cloud_detail: file.cloud_detail,
        cloud_thin_lift: file.cloud_thin_lift,
        rocky: RockyUniforms::from_derived(derived, file.star_distance_au),
        giant: GiantUniforms::from_derived(derived, file.rotation_period_h, file.storm_intensity),
        _pad6: [0.0; 3],
        lava_glow: file.lava_glow,
        ring_inner: file.ring_inner,
        ring_outer: file.ring_outer,
        ring_tilt: file.ring_tilt.to_radians(),
        ring_opacity: file.ring_opacity,
        planet_radius_km: weather
            .map(|snapshot| snapshot.radius_km)
            .unwrap_or(derived.radius_km),
        show_cloud_shadows: flag(file.show_cloud_shadows),
        surface_seed: file.seed,
    }
}
