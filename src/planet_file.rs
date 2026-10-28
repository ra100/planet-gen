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
    /// Simulated plate history in Myr; files without it load as 0.
    pub geologic_time_myr: f32,
    // Atmosphere and surface history overrides (None = physics model);
    // files without them load as the physics model.
    /// Surface pressure in bar.
    pub pressure_override: Option<f32>,
    /// Plate activity (0 = stagnant lid, 1 = vigorous plates).
    pub tectonics_override: Option<f32>,
    /// Surface age recorded by impact craters, in Gyr.
    pub crater_age_override: Option<f32>,
    /// Volcanic plains flooding impact basins (0-1).
    pub mare_override: Option<f32>,
    /// Shield volcanoes and volcanic rises (0-1).
    pub volcanism_override: Option<f32>,
    /// Hemispheric crustal dichotomy (0 = none).
    pub dichotomy: f32,
    /// Oxidized surface dust (0-1).
    pub dust_override: Option<f32>,
    /// Gas envelope (0 = rocky, 1 = giant); None follows the mass.
    pub envelope_override: Option<f32>,
    /// Water-ice crust of a dry world (0-1); None follows the frost line.
    pub ice_override: Option<f32>,
    // Clouds / weather
    pub cloud_coverage: f32,
    pub cloud_seed: u32,
    pub cloud_opacity: f32,
    pub cloud_detail: f32,
    /// Advanced: lifts thin cloud (0 = solver field); files without it load as 0.
    pub cloud_thin_lift: f32,
    /// Weather systems and cloud regimes over the solver field (0 = solver
    /// field only); files without it load as 1.
    pub cloud_synthesis: f32,
    pub wind_scale: f32,
    pub storm_count: u32,
    pub storm_size: f32,
    /// Tropical cyclone intensity (0-1); files without it load as the default.
    pub storm_intensity: f32,
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
            geologic_time_myr: 0.0,
            pressure_override: None,
            tectonics_override: None,
            crater_age_override: None,
            mare_override: None,
            volcanism_override: None,
            dichotomy: 0.0,
            dust_override: None,
            envelope_override: None,
            ice_override: None,
            cloud_coverage: 0.5,
            cloud_seed: params.seed.wrapping_add(1000),
            cloud_opacity: 1.0,
            cloud_detail: 1.0,
            cloud_thin_lift: 0.0,
            cloud_synthesis: 1.0,
            wind_scale: 1.0,
            lava_glow: 0.0,
            ring_inner: 0.0,
            ring_outer: 0.0,
            ring_tilt: 15.0,
            ring_opacity: 0.7,
            storm_count: 2,
            storm_size: 1.0,
            storm_intensity: crate::cloud_synthesis::DEFAULT_STORM_INTENSITY,
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
    finite("geologic_time_myr", file.geologic_time_myr)?;
    for (name, value) in [
        ("pressure_override", file.pressure_override),
        ("tectonics_override", file.tectonics_override),
        ("crater_age_override", file.crater_age_override),
        ("mare_override", file.mare_override),
        ("volcanism_override", file.volcanism_override),
        ("dust_override", file.dust_override),
        ("envelope_override", file.envelope_override),
        ("ice_override", file.ice_override),
    ] {
        if let Some(value) = value {
            finite(name, value)?;
        }
    }
    finite("dichotomy", file.dichotomy)?;
    finite("cloud_coverage", file.cloud_coverage)?;
    finite("cloud_opacity", file.cloud_opacity)?;
    finite("cloud_detail", file.cloud_detail)?;
    finite("cloud_thin_lift", file.cloud_thin_lift)?;
    finite("cloud_synthesis", file.cloud_synthesis)?;
    finite("wind_scale", file.wind_scale)?;
    finite("storm_size", file.storm_size)?;
    finite("storm_intensity", file.storm_intensity)?;
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

/// Version-number separators recognised at the end of a file stem.
const VERSION_SEPARATORS: [char; 3] = ['.', '_', '-'];
/// Longest bare trailing number read as a version. Longer runs are almost
/// always data (`planet_<seed>`), not a version, and get a fresh `.001`.
const MAX_BARE_VERSION_DIGITS: usize = 4;
/// Longest `v`-prefixed version (`_v12345`), bounded so it fits a `u32`.
const MAX_PREFIXED_VERSION_DIGITS: usize = 9;

/// A file stem split into its base and trailing version number, e.g.
/// `abc_v03` → base `abc`, separator `_`, prefix `v`, number 3, width 2.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct VersionedStem {
    pub base: String,
    pub separator: char,
    /// `v`/`V` before the digits, or empty.
    pub prefix: String,
    pub number: u32,
    /// Digit count as written; zero padding is kept on increment.
    pub width: usize,
}

impl VersionedStem {
    /// Parse a trailing version (`.`, `_` or `-`, an optional `v`, then
    /// digits). `None` when the stem carries no version.
    pub fn parse(stem: &str) -> Option<Self> {
        let digits_start = stem.trim_end_matches(|c: char| c.is_ascii_digit()).len();
        let digits = &stem[digits_start..];
        if digits.is_empty() {
            return None;
        }
        let head = &stem[..digits_start];
        let (head, prefix) = match head.strip_suffix(['v', 'V']) {
            Some(rest) if rest.ends_with(VERSION_SEPARATORS) => (rest, &head[rest.len()..]),
            _ => (head, ""),
        };
        let max_digits = if prefix.is_empty() {
            MAX_BARE_VERSION_DIGITS
        } else {
            MAX_PREFIXED_VERSION_DIGITS
        };
        if digits.len() > max_digits {
            return None;
        }
        let separator = head.chars().next_back()?;
        let base = &head[..head.len() - separator.len_utf8()];
        if !VERSION_SEPARATORS.contains(&separator) || base.is_empty() {
            return None;
        }
        Some(Self {
            base: base.to_owned(),
            separator,
            prefix: prefix.to_owned(),
            number: digits.parse().ok()?,
            width: digits.len(),
        })
    }

    /// The stem with `number`, padded to at least this stem's width.
    pub fn with_number(&self, number: u32) -> String {
        format!(
            "{}{}{}{:0width$}",
            self.base,
            self.separator,
            self.prefix,
            number,
            width = self.width
        )
    }

    /// True when `stem` is a version of the same file: same base, separator
    /// and prefix, any number of digits. Returns that version number.
    fn sibling_number(&self, stem: &str) -> Option<u32> {
        let rest = stem
            .strip_prefix(self.base.as_str())?
            .strip_prefix(self.separator)?
            .strip_prefix(self.prefix.as_str())?;
        if rest.is_empty()
            || rest.len() > MAX_PREFIXED_VERSION_DIGITS
            || !rest.bytes().all(|b| b.is_ascii_digit())
        {
            return None;
        }
        rest.parse().ok()
    }
}

/// File name of the next incremental save after `current_name`, given the
/// names already in its folder.
///
/// `abc.001.json` → `abc.002.json`, `abc_v03.json` → `abc_v04.json`,
/// `abc-9.json` → `abc-10.json`; a stem without a version gets `.001`
/// (`abc.json` → `abc.001.json`). The number goes one past the highest
/// existing version of the same file, so saving from an older version never
/// fills a gap or overwrites a newer one. `None` when the number would
/// overflow.
pub fn next_version_name<'a>(
    current_name: &str,
    existing: impl IntoIterator<Item = &'a str>,
) -> Option<String> {
    let (stem, ext) = match current_name.rsplit_once('.') {
        Some((stem, ext)) if !stem.is_empty() => (stem, ext),
        _ => (current_name, "json"),
    };
    let version = VersionedStem::parse(stem).unwrap_or_else(|| VersionedStem {
        base: stem.to_owned(),
        separator: '.',
        prefix: String::new(),
        number: 0,
        width: 3,
    });
    let suffix = format!(".{ext}");
    let highest = existing
        .into_iter()
        .filter_map(|name| name.strip_suffix(suffix.as_str()))
        .filter_map(|other| version.sibling_number(other))
        .fold(version.number, u32::max);
    let next = highest.checked_add(1)?;
    if next.to_string().len() > MAX_PREFIXED_VERSION_DIGITS {
        return None;
    }
    Some(format!("{}{suffix}", version.with_number(next)))
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
        assert_eq!(file.geologic_time_myr, 0.0);
        // Files from before the rocky-surface model keep the physics model.
        assert_eq!(file.pressure_override, None);
        assert_eq!(file.crater_age_override, None);
        assert_eq!(file.dichotomy, 0.0);
    }

    #[test]
    fn surface_overrides_roundtrip() {
        let file = PlanetFile {
            pressure_override: Some(0.006),
            tectonics_override: Some(0.1),
            crater_age_override: Some(3.9),
            mare_override: Some(0.4),
            volcanism_override: Some(1.0),
            dichotomy: 0.8,
            dust_override: Some(0.0),
            ..Default::default()
        };
        let json = serde_json::to_string_pretty(&file).unwrap();
        assert_eq!(checked_load(&json).unwrap(), file);
        let bad = json.replace("\"dichotomy\": 0.8", "\"dichotomy\": 1e999");
        assert!(checked_load(&bad).is_err());
    }

    #[test]
    fn viewport_state_roundtrips() {
        let file = PlanetFile {
            view_mode: 2,
            rot: [[0.0, -1.0, 0.0], [1.0, 0.0, 0.0], [0.0, 0.0, 1.0]],
            zoom: 3.5,
            pan: [-0.25, 0.75],
            ..Default::default()
        };
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
            "mass_earth": 5000.0,
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

    fn next(name: &str, existing: &[&str]) -> String {
        next_version_name(name, existing.iter().copied()).unwrap()
    }

    #[test]
    fn incremental_name_bumps_existing_version() {
        assert_eq!(next("abc.001.json", &[]), "abc.002.json");
        assert_eq!(next("abc.json", &[]), "abc.001.json");
        assert_eq!(next("abc_01.json", &[]), "abc_02.json");
        assert_eq!(next("abc-7.json", &[]), "abc-8.json");
        assert_eq!(next("abc-9.json", &[]), "abc-10.json");
        assert_eq!(next("abc.999.json", &[]), "abc.1000.json");
        assert_eq!(next("abc_v03.json", &[]), "abc_v04.json");
        assert_eq!(next("abc-V12.json", &[]), "abc-V13.json");
        assert_eq!(next("my.planet.002.json", &[]), "my.planet.003.json");
    }

    #[test]
    fn incremental_name_skips_past_highest_sibling() {
        let dir = ["abc.json", "abc.001.json", "abc.005.json", "abc.02.json"];
        assert_eq!(next("abc.json", &dir), "abc.006.json");
        assert_eq!(next("abc.002.json", &dir), "abc.006.json");
        // Other separators, prefixes and extensions are different families.
        let dir = [
            "abc_009.json",
            "abc.v010.json",
            "abc.004.exr",
            "abcd.008.json",
        ];
        assert_eq!(next("abc.001.json", &dir), "abc.002.json");
    }

    #[test]
    fn long_numbers_and_bare_names_are_not_versions() {
        // A seed is data, not a version.
        assert_eq!(next("planet_3185410.json", &[]), "planet_3185410.001.json");
        assert_eq!(next("001.json", &[]), "001.001.json");
        assert_eq!(next("abc_.json", &[]), "abc_.001.json");
        assert_eq!(next("abc2.json", &[]), "abc2.001.json");
        assert_eq!(VersionedStem::parse("abc_v"), None);
        assert_eq!(
            VersionedStem::parse("abc_v3").map(|v| (v.base, v.prefix, v.number)),
            Some(("abc".to_owned(), "v".to_owned(), 3))
        );
    }

    #[test]
    fn incremental_name_reports_overflow() {
        assert_eq!(next_version_name("abc_v999999999.json", []), None);
    }
}
