/// User-facing planet parameters that drive the entire generation pipeline.
#[derive(Debug, Clone)]
pub struct PlanetParams {
    /// Distance from the star in AU (0.1 - 50.0)
    pub star_distance_au: f32,
    /// Planet mass in Earth masses (0.01 - 10.0)
    pub mass_earth: f32,
    /// Stellar metallicity [Fe/H] in dex (-1.0 to 1.0)
    pub metallicity: f32,
    /// Axial tilt in degrees (0 - 90)
    pub axial_tilt_deg: f32,
    /// Rotation period in hours (1.0 - 1000.0)
    pub rotation_period_h: f32,
    /// Random seed for procedural generation
    pub seed: u32,
}

impl Default for PlanetParams {
    /// Earth-like defaults
    fn default() -> Self {
        Self {
            star_distance_au: 1.0,
            mass_earth: 1.0,
            metallicity: 0.0,
            axial_tilt_deg: 23.4,
            rotation_period_h: 24.0,
            seed: 42,
        }
    }
}

#[derive(Debug, Clone)]
pub struct ValidationError {
    pub field: &'static str,
    pub message: String,
}

impl PlanetParams {
    pub fn validate(&self) -> Result<(), Vec<ValidationError>> {
        let mut errors = Vec::new();

        if self.star_distance_au <= 0.0 || self.star_distance_au > 100.0 {
            errors.push(ValidationError {
                field: "star_distance_au",
                message: format!(
                    "must be in (0, 100] AU, got {}",
                    self.star_distance_au
                ),
            });
        }

        if self.mass_earth <= 0.0 || self.mass_earth > 20.0 {
            errors.push(ValidationError {
                field: "mass_earth",
                message: format!(
                    "must be in (0, 20] Earth masses, got {}",
                    self.mass_earth
                ),
            });
        }

        if self.metallicity < -2.0 || self.metallicity > 2.0 {
            errors.push(ValidationError {
                field: "metallicity",
                message: format!(
                    "must be in [-2, 2] dex, got {}",
                    self.metallicity
                ),
            });
        }

        if self.axial_tilt_deg < 0.0 || self.axial_tilt_deg > 180.0 {
            errors.push(ValidationError {
                field: "axial_tilt_deg",
                message: format!(
                    "must be in [0, 180] degrees, got {}",
                    self.axial_tilt_deg
                ),
            });
        }

        if self.rotation_period_h <= 0.0 || self.rotation_period_h > 10000.0 {
            errors.push(ValidationError {
                field: "rotation_period_h",
                message: format!(
                    "must be in (0, 10000] hours, got {}",
                    self.rotation_period_h
                ),
            });
        }

        if errors.is_empty() {
            Ok(())
        } else {
            Err(errors)
        }
    }
}

/// Derived planet type from distance and mass.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum PlanetType {
    HotRocky,
    Terrestrial,
    IcyRocky,
}

/// Tectonic regime based on Rayleigh number and water content.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum TectonicRegime {
    PlateTectonics,
    StagnantLid,
}

/// Atmosphere type derived from planet properties.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum AtmosphereType {
    None,
    ThinCO2,
    ThickCO2,
    Nitrogen,
}

/// Properties derived from PlanetParams using planetary science rules.
#[derive(Debug, Clone)]
pub struct DerivedProperties {
    pub planet_type: PlanetType,
    pub tectonic_regime: TectonicRegime,
    pub atmosphere_type: AtmosphereType,
    /// Surface gravity in m/s² (assuming rocky composition)
    pub surface_gravity: f32,
    /// Base equatorial temperature in °C
    pub base_temperature_c: f32,
    /// Ocean coverage fraction (0.0 - 1.0)
    pub ocean_fraction: f32,
    /// Surface age factor (0.0 = young/active, 1.0 = old/inactive)
    pub surface_age: f32,
    /// Frost line distance in AU for the star
    pub frost_line_au: f32,
}

impl DerivedProperties {
    /// Derive planet properties from user parameters using planetary science rules.
    pub fn from_params(params: &PlanetParams) -> Self {
        let frost_line_au = compute_frost_line(params.metallicity);
        let planet_type = classify_planet(params.star_distance_au, frost_line_au);
        let tectonic_regime = determine_tectonics(params.mass_earth, params.star_distance_au);
        let atmosphere_type = determine_atmosphere(planet_type, params.mass_earth);
        let surface_gravity = compute_surface_gravity(params.mass_earth);
        let base_temperature_c =
            compute_base_temperature(params.star_distance_au, atmosphere_type);
        let ocean_fraction =
            compute_ocean_fraction(planet_type, params.mass_earth, base_temperature_c);
        let surface_age = compute_surface_age(tectonic_regime, params.mass_earth);

        Self {
            planet_type,
            tectonic_regime,
            atmosphere_type,
            surface_gravity,
            base_temperature_c,
            ocean_fraction,
            surface_age,
            frost_line_au,
        }
    }
}

/// Frost line: water ice condenses beyond this distance.
/// Metallicity shifts it slightly (more metals → closer frost line).
fn compute_frost_line(metallicity: f32) -> f32 {
    // Base frost line ~2.7 AU for solar-type star
    // Metallicity shifts: higher metallicity → slightly closer
    2.7 * (1.0 - 0.1 * metallicity)
}

fn classify_planet(distance_au: f32, frost_line_au: f32) -> PlanetType {
    if distance_au < 0.5 {
        PlanetType::HotRocky
    } else if distance_au < frost_line_au {
        PlanetType::Terrestrial
    } else {
        PlanetType::IcyRocky
    }
}

/// Rayleigh-number-based tectonic regime.
/// Larger planets with water are more likely to have plate tectonics.
fn determine_tectonics(mass_earth: f32, distance_au: f32) -> TectonicRegime {
    // Simplified: mass > 0.5 Earth masses and in habitable zone → plate tectonics
    let has_water = distance_au > 0.5 && distance_au < 3.0;
    if mass_earth > 0.5 && has_water {
        TectonicRegime::PlateTectonics
    } else {
        TectonicRegime::StagnantLid
    }
}

fn determine_atmosphere(planet_type: PlanetType, mass_earth: f32) -> AtmosphereType {
    match planet_type {
        PlanetType::HotRocky => {
            if mass_earth < 0.3 {
                AtmosphereType::None
            } else {
                AtmosphereType::ThinCO2
            }
        }
        PlanetType::Terrestrial => {
            if mass_earth < 0.3 {
                AtmosphereType::ThinCO2
            } else if mass_earth < 2.0 {
                AtmosphereType::Nitrogen
            } else {
                AtmosphereType::ThickCO2
            }
        }
        PlanetType::IcyRocky => {
            if mass_earth < 0.3 {
                AtmosphereType::None
            } else {
                AtmosphereType::ThinCO2
            }
        }
    }
}

/// Surface gravity assuming rocky composition.
/// g ∝ M^(1/3) for constant density (simplified).
/// More accurately: radius ∝ M^0.27 for rocky planets, g = GM/R².
fn compute_surface_gravity(mass_earth: f32) -> f32 {
    // For rocky planets: R ≈ M^0.27, so g = GM/R² ≈ M^(1-0.54) = M^0.46
    // Earth: 9.81 m/s²
    9.81 * mass_earth.powf(0.46)
}

/// Base temperature from stellar flux and greenhouse effect.
fn compute_base_temperature(distance_au: f32, atmosphere: AtmosphereType) -> f32 {
    // Equilibrium temperature with albedo ~0.3: T_eq = 255 / sqrt(distance) K
    let t_eff_k = 255.0 / distance_au.sqrt();

    // Greenhouse warming
    let greenhouse_k = match atmosphere {
        AtmosphereType::None => 0.0,
        AtmosphereType::ThinCO2 => 10.0,
        AtmosphereType::Nitrogen => 33.0, // Earth-like
        AtmosphereType::ThickCO2 => 450.0, // Venus-like
    };

    t_eff_k + greenhouse_k - 273.15 // Convert to Celsius
}

fn compute_ocean_fraction(
    planet_type: PlanetType,
    mass_earth: f32,
    base_temp_c: f32,
) -> f32 {
    match planet_type {
        PlanetType::HotRocky => 0.0, // Too hot, water evaporated
        PlanetType::IcyRocky => {
            // Frozen oceans under ice
            if mass_earth > 0.3 { 0.3 } else { 0.0 }
        }
        PlanetType::Terrestrial => {
            if base_temp_c < -20.0 {
                0.1 // Mostly frozen
            } else if base_temp_c > 100.0 {
                0.0 // Boiled off
            } else {
                // Scale with mass (more mass → more outgassing → more water)
                (0.3 + 0.4 * mass_earth).min(0.9)
            }
        }
    }
}

fn compute_surface_age(regime: TectonicRegime, mass_earth: f32) -> f32 {
    match regime {
        TectonicRegime::PlateTectonics => {
            // Active surface → younger appearance
            0.2 + 0.3 * (1.0 / mass_earth).min(1.0)
        }
        TectonicRegime::StagnantLid => {
            // Inactive → old, cratered surface
            0.6 + 0.3 * (1.0 / mass_earth).min(1.0)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_is_earth_like() {
        let params = PlanetParams::default();
        assert!((params.star_distance_au - 1.0).abs() < 0.01);
        assert!((params.mass_earth - 1.0).abs() < 0.01);
        assert!(params.validate().is_ok());
    }

    #[test]
    fn validation_rejects_negative_mass() {
        let params = PlanetParams {
            mass_earth: -1.0,
            ..Default::default()
        };
        let err = params.validate().unwrap_err();
        assert!(err.iter().any(|e| e.field == "mass_earth"));
    }

    #[test]
    fn validation_rejects_zero_distance() {
        let params = PlanetParams {
            star_distance_au: 0.0,
            ..Default::default()
        };
        assert!(params.validate().is_err());
    }

    #[test]
    fn earth_params_derive_terrestrial() {
        let params = PlanetParams::default();
        let derived = DerivedProperties::from_params(&params);
        assert_eq!(derived.planet_type, PlanetType::Terrestrial);
        assert_eq!(derived.tectonic_regime, TectonicRegime::PlateTectonics);
        assert_eq!(derived.atmosphere_type, AtmosphereType::Nitrogen);
    }

    #[test]
    fn earth_gravity_is_reasonable() {
        let params = PlanetParams::default();
        let derived = DerivedProperties::from_params(&params);
        assert!(
            (derived.surface_gravity - 9.81).abs() < 0.1,
            "Earth gravity should be ~9.81, got {}",
            derived.surface_gravity
        );
    }

    #[test]
    fn earth_temperature_is_reasonable() {
        let params = PlanetParams::default();
        let derived = DerivedProperties::from_params(&params);
        // Earth average ~15°C
        assert!(
            derived.base_temperature_c > 10.0 && derived.base_temperature_c < 25.0,
            "Earth temp should be ~15°C, got {}",
            derived.base_temperature_c
        );
    }

    #[test]
    fn earth_ocean_fraction_is_reasonable() {
        let params = PlanetParams::default();
        let derived = DerivedProperties::from_params(&params);
        assert!(
            derived.ocean_fraction > 0.5 && derived.ocean_fraction < 0.9,
            "Earth ocean fraction should be ~0.7, got {}",
            derived.ocean_fraction
        );
    }

    #[test]
    fn mars_like_is_stagnant_lid() {
        let params = PlanetParams {
            star_distance_au: 1.5,
            mass_earth: 0.1,
            ..Default::default()
        };
        let derived = DerivedProperties::from_params(&params);
        assert_eq!(derived.tectonic_regime, TectonicRegime::StagnantLid);
    }

    #[test]
    fn hot_rocky_at_close_distance() {
        let params = PlanetParams {
            star_distance_au: 0.2,
            mass_earth: 0.5,
            ..Default::default()
        };
        let derived = DerivedProperties::from_params(&params);
        assert_eq!(derived.planet_type, PlanetType::HotRocky);
    }

    #[test]
    fn icy_beyond_frost_line() {
        let params = PlanetParams {
            star_distance_au: 5.0,
            mass_earth: 1.0,
            ..Default::default()
        };
        let derived = DerivedProperties::from_params(&params);
        assert_eq!(derived.planet_type, PlanetType::IcyRocky);
    }
}
