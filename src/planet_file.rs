//! Save/load file format for complete planet configurations.
//!
//! A planet file is a single pretty-printed JSON document containing every
//! user-settable generation parameter (physics + visual overrides + name)
//! plus the viewport state (view mode, rotation, zoom, pan) so a loaded
//! planet looks exactly as it was left. `DerivedProperties` are recomputed
//! on load — they are deterministic from `PlanetParams`.

use crate::planet::PlanetParams;
use serde::{Deserialize, Serialize};

/// Magic string identifying a planet-gen file.
pub const FILE_FORMAT: &str = "planet-gen";
/// Current file format version. v2 added viewport state (view_mode, rot,
/// zoom, pan); v1 files load with the factory view defaults.
pub const FILE_VERSION: u32 = 2;

/// Complete serializable planet configuration.
///
/// Flat on purpose: the struct doubles as the file envelope so the JSON stays
/// greppable and diffable. Missing fields fall back to factory defaults via
/// the hand-written `Default` (mirroring the app constructor), unknown fields
/// are ignored so newer files load in older apps.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct PlanetFile {
    pub format: String,
    pub version: u32,
    // Physics (PlanetParams)
    pub star_distance_au: f32,
    pub mass_earth: f32,
    pub metallicity: f32,
    pub axial_tilt_deg: f32,
    pub rotation_period_h: f32,
    pub seed: u32,
    // Terrain / climate overrides
    pub continental_scale: f32,
    pub water_loss: f32,
    pub climate_moisture: f32,
    pub season: f32,
    pub erosion_iterations: u32,
    pub light_azimuth: f32,
    pub light_elevation: f32,
    pub height_scale: f32,
    // Layer toggles
    pub show_atmosphere: bool,
    pub show_ao: bool,
    pub show_water: bool,
    pub show_ice: bool,
    pub show_biomes: bool,
    pub show_clouds: bool,
    pub show_cloud_shadows: bool,
    pub show_wind_effects: bool,
    pub show_cities: bool,
    pub show_erosion: bool,
    // Advanced terrain tweaks
    pub mountain_scale: f32,
    pub boundary_width: f32,
    pub warp_strength: f32,
    pub detail_scale: f32,
    pub age_override: Option<f32>,
    pub num_plates_override: u32,
    pub num_continents: u32,
    pub continent_size_variety: f32,
    // Clouds / weather
    pub cloud_coverage: f32,
    pub cloud_seed: u32,
    pub cloud_opacity: f32,
    pub wind_scale: f32,
    pub storm_count: u32,
    pub storm_size: f32,
    // Surface extras
    pub lava_glow: f32,
    pub ring_inner: f32,
    pub ring_outer: f32,
    pub ring_tilt: f32,
    pub ring_opacity: f32,
    pub night_lights: f32,
    pub star_color_temp: f32,
    pub city_light_hue: f32,
    // Viewport state (v2) — the view as the user left it
    pub view_mode: u32,
    /// Planet orientation in view space (rows of an orthogonal 3×3).
    pub rot: [[f32; 3]; 3],
    pub zoom: f32,
    /// Viewport pan in NDC units.
    pub pan: [f32; 2],
    // Identity
    pub planet_name: String,
}

impl Default for PlanetFile {
    /// Factory defaults mirroring `PlanetGenApp::new()` — the meaning of a
    /// field missing from an older file. Adding a field to `PlanetFile`
    /// forces a deliberate entry here (no silent zero-defaults).
    fn default() -> Self {
        let params = PlanetParams::default();
        Self {
            format: FILE_FORMAT.to_owned(),
            version: FILE_VERSION,
            star_distance_au: params.star_distance_au,
            mass_earth: params.mass_earth,
            metallicity: params.metallicity,
            axial_tilt_deg: params.axial_tilt_deg,
            rotation_period_h: params.rotation_period_h,
            seed: params.seed,
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
            mountain_scale: 1.0,
            boundary_width: 0.10,
            warp_strength: 1.0,
            detail_scale: 1.0,
            age_override: None,
            num_plates_override: 0,
            num_continents: 4,
            continent_size_variety: 0.35,
            cloud_coverage: 0.5,
            cloud_seed: params.seed.wrapping_add(1000),
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
            rot: [[1.0, 0.0, 0.0], [0.0, 1.0, 0.0], [0.0, 0.0, 1.0]],
            zoom: 1.0,
            pan: [0.0, 0.0],
            planet_name: format!("planet_{}", params.seed),
        }
    }
}

fn finite(name: &str, value: f32) -> Result<(), String> {
    if value.is_finite() {
        Ok(())
    } else {
        Err(format!("{name} is not finite ({value})"))
    }
}

/// Parse and validate a planet file from JSON text.
///
/// Rejects, in order: malformed JSON, wrong format magic, version < 1,
/// non-finite floats (hand-edited exponents like `1e999` saturate to inf and
/// must not reach GPU uniforms), and physics values outside the
/// `PlanetParams::validate()` ranges. Never clamps — an out-of-range file
/// describes a different planet than the one saved.
pub fn checked_load(json: &str) -> Result<PlanetFile, String> {
    let file: PlanetFile = serde_json::from_str(json).map_err(|e| format!("invalid JSON: {e}"))?;

    if file.format != FILE_FORMAT {
        return Err(format!(
            "not a planet-gen file (format {:?}, expected {:?})",
            file.format, FILE_FORMAT
        ));
    }
    if file.version < 1 {
        return Err(format!("unsupported version {}", file.version));
    }
    if file.version > FILE_VERSION {
        log::warn!(
            "planet file version {} is newer than supported {}; loading with defaults for missing fields",
            file.version,
            FILE_VERSION
        );
    }

    finite("star_distance_au", file.star_distance_au)?;
    finite("mass_earth", file.mass_earth)?;
    finite("metallicity", file.metallicity)?;
    finite("axial_tilt_deg", file.axial_tilt_deg)?;
    finite("rotation_period_h", file.rotation_period_h)?;
    finite("continental_scale", file.continental_scale)?;
    finite("water_loss", file.water_loss)?;
    finite("climate_moisture", file.climate_moisture)?;
    finite("season", file.season)?;
    finite("light_azimuth", file.light_azimuth)?;
    finite("light_elevation", file.light_elevation)?;
    finite("height_scale", file.height_scale)?;
    finite("mountain_scale", file.mountain_scale)?;
    finite("boundary_width", file.boundary_width)?;
    finite("warp_strength", file.warp_strength)?;
    finite("detail_scale", file.detail_scale)?;
    if let Some(age) = file.age_override {
        finite("age_override", age)?;
    }
    finite("continent_size_variety", file.continent_size_variety)?;
    finite("cloud_coverage", file.cloud_coverage)?;
    finite("cloud_opacity", file.cloud_opacity)?;
    finite("wind_scale", file.wind_scale)?;
    finite("storm_size", file.storm_size)?;
    finite("lava_glow", file.lava_glow)?;
    finite("ring_inner", file.ring_inner)?;
    finite("ring_outer", file.ring_outer)?;
    finite("ring_tilt", file.ring_tilt)?;
    finite("ring_opacity", file.ring_opacity)?;
    finite("night_lights", file.night_lights)?;
    finite("star_color_temp", file.star_color_temp)?;
    finite("city_light_hue", file.city_light_hue)?;
    finite("zoom", file.zoom)?;
    for (r, row) in file.rot.iter().enumerate() {
        for (c, v) in row.iter().enumerate() {
            finite(&format!("rot[{r}][{c}]"), *v)?;
        }
    }
    for (i, v) in file.pan.iter().enumerate() {
        finite(&format!("pan[{i}]"), *v)?;
    }

    let params = PlanetParams {
        star_distance_au: file.star_distance_au,
        mass_earth: file.mass_earth,
        metallicity: file.metallicity,
        axial_tilt_deg: file.axial_tilt_deg,
        rotation_period_h: file.rotation_period_h,
        seed: file.seed,
    };
    if let Err(errors) = params.validate() {
        return Err(format!(
            "invalid physics parameters: {}",
            errors
                .iter()
                .map(|e| format!("{}: {}", e.field, e.message))
                .collect::<Vec<_>>()
                .join("; ")
        ));
    }

    Ok(file)
}

/// Sanitize a free-text planet name for use as a default file name.
pub fn default_filename(planet_name: &str) -> String {
    let mut out = String::with_capacity(planet_name.len());
    let mut prev_underscore = false;
    for c in planet_name.chars() {
        if c.is_ascii_alphanumeric() || c == '_' || c == '-' {
            out.push(c);
            prev_underscore = false;
        } else if !prev_underscore {
            out.push('_');
            prev_underscore = true;
        }
    }
    let trimmed = out.trim_matches('_');
    let base = if trimmed.is_empty() {
        "planet"
    } else {
        trimmed
    };
    format!("{base}.json")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn roundtrip_default() {
        let file = PlanetFile::default();
        let json = serde_json::to_string_pretty(&file).unwrap();
        let loaded = checked_load(&json).unwrap();
        assert_eq!(file, loaded);
    }

    #[test]
    fn missing_fields_get_factory_defaults() {
        let json = r#"{
            "format": "planet-gen",
            "version": 1,
            "star_distance_au": 2.7
        }"#;
        let file = checked_load(json).unwrap();
        assert_eq!(file.star_distance_au, 2.7);
        // Spot-check defaults that would be wrong as silent zeros.
        assert!(!file.show_erosion);
        assert!(file.show_clouds);
        assert_eq!(file.star_color_temp, 0.5);
        assert_eq!(file.ring_tilt, 15.0);
        assert_eq!(file.cloud_seed, 42u32.wrapping_add(1000));
        assert_eq!(file.planet_name, "planet_42");
        // v1 files lack the viewport fields — factory view defaults apply.
        assert_eq!(file.view_mode, 0);
        assert_eq!(
            file.rot,
            [[1.0, 0.0, 0.0], [0.0, 1.0, 0.0], [0.0, 0.0, 1.0]]
        );
        assert_eq!(file.zoom, 1.0);
        assert_eq!(file.pan, [0.0, 0.0]);
    }

    #[test]
    fn viewport_state_roundtrips() {
        let mut file = PlanetFile::default();
        file.view_mode = 2;
        file.rot = [[0.0, -1.0, 0.0], [1.0, 0.0, 0.0], [0.0, 0.0, 1.0]];
        file.zoom = 3.5;
        file.pan = [-0.25, 0.75];
        let json = serde_json::to_string_pretty(&file).unwrap();
        let loaded = checked_load(&json).unwrap();
        assert_eq!(file, loaded);
    }

    #[test]
    fn unknown_fields_ignored() {
        let json = r#"{
            "format": "planet-gen",
            "version": 1,
            "future_field": 1
        }"#;
        assert!(checked_load(json).is_ok());
    }

    #[test]
    fn wrong_format_rejected() {
        let json = r#"{ "format": "something-else", "version": 1 }"#;
        let err = checked_load(json).unwrap_err();
        assert!(err.contains("not a planet-gen file"), "{err}");
    }

    #[test]
    fn zero_version_rejected() {
        let json = r#"{ "format": "planet-gen", "version": 0 }"#;
        let err = checked_load(json).unwrap_err();
        assert!(err.contains("unsupported version"), "{err}");
    }

    #[test]
    fn non_finite_rejected() {
        // Overflowing exponents must be rejected: serde_json refuses values
        // outside f64 at parse time, and the finite scan in checked_load is
        // the defense-in-depth backstop for anything that still saturates.
        let json_999 = r#"{ "format": "planet-gen", "version": 1, "mass_earth": 1e999 }"#;
        assert!(checked_load(json_999).is_err());
        let json_39 = r#"{ "format": "planet-gen", "version": 1, "mass_earth": 1e39 }"#;
        assert!(checked_load(json_39).is_err());
    }

    #[test]
    fn finite_scan_catches_inf() {
        // Direct unit check of the backstop (serde_json text parsing never
        // produces inf, but a future API change must not let it through).
        assert!(finite("x", f32::INFINITY).is_err());
        assert!(finite("x", f32::NAN).is_err());
        assert!(finite("x", 1.0).is_ok());
    }

    #[test]
    fn out_of_range_physics_rejected() {
        let json = r#"{
            "format": "planet-gen",
            "version": 1,
            "mass_earth": 50.0,
            "rotation_period_h": -1.0
        }"#;
        let err = checked_load(json).unwrap_err();
        assert!(err.contains("mass_earth"), "{err}");
        assert!(err.contains("rotation_period_h"), "{err}");
    }

    #[test]
    fn default_filename_sanitizes() {
        assert_eq!(default_filename("a/b c"), "a_b_c.json");
        assert_eq!(default_filename("kepler-442 b"), "kepler-442_b.json");
        assert_eq!(default_filename("///"), "planet.json");
        assert_eq!(default_filename("normal_name-1"), "normal_name-1.json");
    }
}
