//! Curated parameter bundles behind the PRESETS chips. Shared with the
//! headless capture tool so a preset renders the same there as in the app.

use crate::cloud_synthesis::DEFAULT_STORM_INTENSITY;
use crate::planet::PlanetParams;
use crate::planet_file::PlanetFile;

pub const PRESETS: [&str; 9] = [
    "EARTH", "MARS", "MOON", "MERCURY", "OCEAN", "SNOWBALL", "HOTHOUSE", "VOLCANIC", "RINGED",
];

/// Reset the generation settings a preset owns to factory defaults. Layer
/// toggles, lighting and the view are left as they are.
fn preset_base(file: &mut PlanetFile) {
    let params = PlanetParams::default();
    file.star_distance_au = params.star_distance_au;
    file.mass_earth = params.mass_earth;
    file.metallicity = params.metallicity;
    file.axial_tilt_deg = params.axial_tilt_deg;
    file.rotation_period_h = params.rotation_period_h;
    file.seed = params.seed;
    file.continental_scale = 1.0;
    file.water_loss = 0.5;
    file.climate_moisture = 1.0;
    file.season = 0.5;
    file.erosion_iterations = 25;
    file.height_scale = 3.0;
    file.mountain_scale = 1.0;
    file.boundary_width = 0.10;
    file.warp_strength = 1.0;
    file.detail_scale = 1.0;
    file.age_override = None;
    file.num_plates_override = 0;
    file.num_continents = 4;
    file.continent_size_variety = 0.35;
    file.geologic_time_myr = 0.0;
    file.cloud_coverage = 0.5;
    file.cloud_opacity = 1.0;
    file.cloud_detail = 1.0;
    file.cloud_thin_lift = 0.0;
    file.cloud_synthesis = 1.0;
    file.wind_scale = 1.0;
    file.lava_glow = 0.0;
    file.ring_inner = 0.0;
    file.ring_outer = 0.0;
    file.ring_tilt = 15.0;
    file.ring_opacity = 0.7;
    file.storm_count = 2;
    file.storm_size = 1.0;
    file.storm_intensity = DEFAULT_STORM_INTENSITY;
    file.night_lights = 0.0;
    file.star_color_temp = 0.5;
    file.city_light_hue = 0.0;
}

/// Apply the preset named in `PRESETS` (unknown names leave the Earth base).
pub fn apply(file: &mut PlanetFile, name: &str) {
    preset_base(file);
    match name {
        "MARS" => {
            // Mars analog: small, cold, dry, dust-rough.
            file.star_distance_au = 1.52;
            file.mass_earth = 0.107;
            file.axial_tilt_deg = 25.2;
            file.rotation_period_h = 24.6;
            file.water_loss = 0.85;
            file.climate_moisture = 0.15;
            file.cloud_coverage = 0.15;
            file.erosion_iterations = 5;
            file.detail_scale = 1.3;
            file.mountain_scale = 1.2;
            file.height_scale = 4.0;
        }
        "MOON" => {
            // Earth's Moon: airless, tidally locked (27.3 d), 1.5° obliquity.
            file.mass_earth = 0.0123;
            file.axial_tilt_deg = 1.5;
            file.rotation_period_h = 655.7;
            file.water_loss = 1.0;
            file.climate_moisture = 0.0;
            file.cloud_coverage = 0.0;
            file.erosion_iterations = 0;
            file.height_scale = 4.0;
        }
        "MERCURY" => {
            // Mercury: airless, 0.39 AU, 3:2 spin-orbit resonance (58.6 d).
            file.star_distance_au = 0.387;
            file.mass_earth = 0.055;
            file.axial_tilt_deg = 0.03;
            file.rotation_period_h = 1407.6;
            file.water_loss = 1.0;
            file.climate_moisture = 0.0;
            file.cloud_coverage = 0.0;
            file.erosion_iterations = 0;
            file.height_scale = 4.0;
        }
        "OCEAN" => {
            // Ocean world: no water loss, heavy cloud, few landmasses.
            file.water_loss = 0.0;
            file.cloud_coverage = 0.65;
            file.num_continents = 2;
            file.continent_size_variety = 0.25;
            file.erosion_iterations = 30;
        }
        "SNOWBALL" => {
            // Snowball: far, tilted-cold axis, thin moisture.
            file.star_distance_au = 2.7;
            file.mass_earth = 0.9;
            file.axial_tilt_deg = 8.0;
            file.water_loss = 0.25;
            file.climate_moisture = 0.35;
            file.cloud_coverage = 0.45;
            file.erosion_iterations = 10;
            file.season = 0.15;
        }
        "HOTHOUSE" => {
            // Hothouse: close-in, heavy atmosphere, overcast.
            file.star_distance_au = 0.72;
            file.mass_earth = 1.6;
            file.climate_moisture = 0.95;
            file.cloud_coverage = 0.85;
            file.water_loss = 0.35;
            file.erosion_iterations = 40;
            file.season = 0.85;
        }
        "VOLCANIC" => {
            // Volcanic: tectonically violent, lava emission, sparse water.
            file.mass_earth = 1.3;
            file.lava_glow = 0.85;
            file.mountain_scale = 2.2;
            file.num_plates_override = 26;
            file.detail_scale = 1.6;
            file.water_loss = 0.65;
            file.cloud_coverage = 0.3;
            file.erosion_iterations = 8;
        }
        "RINGED" => {
            // Ringed world: massive planet with prominent ring system.
            file.star_distance_au = 1.05;
            file.mass_earth = 1.6;
            file.ring_inner = 1.35;
            file.ring_outer = 2.35;
            file.ring_tilt = 18.0;
            file.ring_opacity = 0.75;
            file.cloud_coverage = 0.6;
        }
        _ => {} // EARTH: factory defaults.
    }
    file.cloud_seed = file.seed.wrapping_add(1000);
    file.planet_name = format!("planet_{}", file.seed);
}

/// A preset as a complete planet file with the factory view.
pub fn preset_file(name: &str) -> PlanetFile {
    let mut file = PlanetFile::default();
    apply(&mut file, name);
    file
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn earth_preset_is_the_factory_planet() {
        assert_eq!(preset_file("EARTH"), PlanetFile::default());
    }

    #[test]
    fn every_preset_passes_validation() {
        for name in PRESETS {
            let file = preset_file(name);
            assert!(
                crate::scene::planet_params(&file).validate().is_ok(),
                "{name} preset is out of range"
            );
        }
    }
}
