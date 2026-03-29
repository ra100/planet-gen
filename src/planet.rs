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
    /// Continuous tectonics factor [0, 1]: 0 = stagnant lid, 1 = vigorous plate tectonics
    pub tectonics_factor: f32,
    pub atmosphere_type: AtmosphereType,
    /// Continuous atmosphere strength [0, 1]
    pub atmosphere_strength: f32,
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
    /// MMSN isolation mass at this distance
    pub isolation_mass: f32,
}

impl DerivedProperties {
    /// Derive planet properties from user parameters using planetary science rules.
    pub fn from_params(params: &PlanetParams) -> Self {
        let frost_line_au = compute_frost_line(params.metallicity);
        let planet_type = classify_planet(params.star_distance_au, frost_line_au);
        let surface_gravity = compute_surface_gravity(params.mass_earth);
        let tectonics_factor =
            compute_tectonics_factor(params.mass_earth, params.star_distance_au, surface_gravity);
        let tectonic_regime = if tectonics_factor > 0.5 {
            TectonicRegime::PlateTectonics
        } else {
            TectonicRegime::StagnantLid
        };
        let atmosphere_strength =
            compute_atmosphere_strength(params.mass_earth, params.star_distance_au);
        let atmosphere_type = classify_atmosphere(atmosphere_strength, planet_type);
        let base_temperature_c = compute_base_temperature(
            params.star_distance_au,
            atmosphere_strength,
        );
        let ocean_fraction = compute_ocean_fraction(
            params.star_distance_au,
            params.mass_earth,
            base_temperature_c,
            frost_line_au,
        );
        let surface_age = compute_surface_age(tectonics_factor, params.mass_earth);
        let isolation_mass = compute_isolation_mass(params.star_distance_au);

        Self {
            planet_type,
            tectonic_regime,
            tectonics_factor,
            atmosphere_type,
            atmosphere_strength,
            surface_gravity,
            base_temperature_c,
            ocean_fraction,
            surface_age,
            frost_line_au,
            isolation_mass,
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

/// Continuous tectonics factor based on simplified Rayleigh number.
/// Ra ∝ g · ΔT · D³ / (ν · κ). Returns [0, 1].
fn compute_tectonics_factor(mass_earth: f32, distance_au: f32, gravity: f32) -> f32 {
    // Water availability factor: peaks in habitable zone, drops outside
    let water_factor = (-((distance_au - 1.5).powi(2)) / 2.0).exp();

    // Rayleigh proxy: gravity × mass (mantle thickness ∝ mass^0.3)
    let ra_proxy = gravity * mass_earth.powf(0.3) * water_factor;

    // Normalize: Earth (g=9.81, M=1) gives ra_proxy ≈ 9.81 * 1.0 * 0.89 ≈ 8.7
    // Threshold for plate tectonics onset ~5 (corresponds to Ra ~ 10⁶)
    let normalized = (ra_proxy / 5.0).min(2.0);

    // Smooth sigmoid transition
    let x = (normalized - 1.0) * 4.0;
    1.0 / (1.0 + (-x).exp())
}

/// Continuous atmosphere strength based on escape velocity vs thermal velocity.
/// Returns [0, 1]: 0 = no atmosphere, 1 = thick atmosphere.
fn compute_atmosphere_strength(mass_earth: f32, distance_au: f32) -> f32 {
    // v_esc ∝ sqrt(M/R) ∝ M^0.365 (using R ∝ M^0.27)
    let v_esc_factor = mass_earth.powf(0.365);

    // Thermal velocity ∝ sqrt(T) ∝ distance^(-0.25)
    let thermal_factor = distance_au.powf(-0.25);

    // Retention: high v_esc / v_thermal → strong atmosphere
    // Calibrated so Earth (M=1, d=1) gives ~0.7
    let retention = v_esc_factor / thermal_factor * 0.7;
    retention.clamp(0.0, 1.0)
}

fn classify_atmosphere(strength: f32, planet_type: PlanetType) -> AtmosphereType {
    if strength < 0.15 {
        AtmosphereType::None
    } else if strength < 0.35 {
        AtmosphereType::ThinCO2
    } else if planet_type == PlanetType::HotRocky && strength > 0.6 {
        AtmosphereType::ThickCO2 // Venus-like runaway
    } else if strength > 0.95 {
        AtmosphereType::ThickCO2
    } else {
        AtmosphereType::Nitrogen
    }
}

/// Surface gravity: g = GM/R², R ∝ M^0.27, so g ∝ M^0.46
fn compute_surface_gravity(mass_earth: f32) -> f32 {
    9.81 * mass_earth.powf(0.46)
}

/// Temperature with greenhouse feedback (carbonate-silicate cycle approximation).
/// Colder planets accumulate more CO₂ → stronger greenhouse, extending habitable zone.
fn compute_base_temperature(distance_au: f32, atmosphere_strength: f32) -> f32 {
    // Equilibrium temperature: T_eq = 255 / sqrt(distance) K
    let t_eq_k = 255.0 / distance_au.sqrt();

    // Greenhouse warming: scales non-linearly with atmosphere strength
    // Calibrated so Earth (strength ~0.7) gets ~33K
    let base_greenhouse = 33.0 * (atmosphere_strength / 0.7).min(1.5).powf(0.7);

    // Carbonate-silicate feedback: colder → more CO₂ accumulates → stronger greenhouse
    // This is the main mechanism that extends the habitable zone outer edge to ~1.7 AU
    let feedback_factor = if t_eq_k < 255.0 {
        // Colder than Earth → greenhouse strengthens significantly (up to 2.5x)
        1.0 + 1.5 * ((255.0 - t_eq_k) / 80.0).min(1.0)
    } else if t_eq_k > 300.0 {
        // Hotter → CO₂ weathered out faster → weaker greenhouse
        1.0 - 0.5 * ((t_eq_k - 300.0) / 100.0).min(1.0)
    } else {
        1.0
    };

    let greenhouse_k = base_greenhouse * feedback_factor;
    t_eq_k + greenhouse_k - 273.15
}

/// Ocean fraction from water budget model.
/// Water delivery ∝ mass × distance factor (more beyond frost line).
fn compute_ocean_fraction(
    distance_au: f32,
    mass_earth: f32,
    base_temp_c: f32,
    frost_line_au: f32,
) -> f32 {
    // Water delivery: more mass captures more water, proximity to frost line helps
    let frost_proximity = (1.0 - (distance_au / frost_line_au - 0.5).abs()).max(0.0);
    let water_budget = mass_earth.powf(0.5) * (0.3 + 0.7 * frost_proximity);

    // Temperature affects surface water state
    let temp_factor = if base_temp_c < -40.0 {
        0.1 // Mostly frozen under ice
    } else if base_temp_c < 0.0 {
        0.1 + 0.4 * ((base_temp_c + 40.0) / 40.0) // Partially frozen
    } else if base_temp_c > 200.0 {
        0.0 // Boiled off
    } else if base_temp_c > 80.0 {
        1.0 - 0.8 * ((base_temp_c - 80.0) / 120.0) // Starting to evaporate
    } else {
        1.0 // Liquid water stable
    };

    (water_budget * temp_factor * 0.7).clamp(0.0, 0.85)
}

fn compute_surface_age(tectonics_factor: f32, mass_earth: f32) -> f32 {
    // More tectonically active → younger surface
    let activity = tectonics_factor * mass_earth.powf(0.2);
    1.0 - activity.clamp(0.0, 0.8)
}

/// MMSN isolation mass: maximum planet mass that can form in-situ at given distance.
/// Σ(r) = 1700(r/AU)^(-3/2) g/cm², M_iso ≈ 0.11 (r/AU)^(3/4) M⊕
fn compute_isolation_mass(distance_au: f32) -> f32 {
    // Beyond frost line, surface density jumps 4× → higher isolation mass
    let ice_factor = if distance_au > 2.7 { 4.0_f32.powf(0.75) } else { 1.0 };
    0.11 * distance_au.powf(0.75) * ice_factor
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
            derived.ocean_fraction > 0.3 && derived.ocean_fraction < 0.9,
            "Earth ocean fraction should be ~0.5-0.7, got {}",
            derived.ocean_fraction
        );
    }

    #[test]
    fn earth_tectonics_factor_is_high() {
        let params = PlanetParams::default();
        let derived = DerivedProperties::from_params(&params);
        assert!(
            derived.tectonics_factor > 0.6,
            "Earth tectonics factor should be high, got {}",
            derived.tectonics_factor
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
        assert!(
            derived.tectonics_factor < 0.5,
            "Mars tectonics should be low, got {}",
            derived.tectonics_factor
        );
    }

    #[test]
    fn habitable_zone_extends_to_1_5_au() {
        // With greenhouse feedback, 1.5 AU should not be a frozen wasteland
        let params = PlanetParams {
            star_distance_au: 1.5,
            mass_earth: 1.0,
            ..Default::default()
        };
        let derived = DerivedProperties::from_params(&params);
        assert!(
            derived.base_temperature_c > -10.0,
            "1.5 AU with greenhouse feedback should be > -10°C, got {}",
            derived.base_temperature_c
        );
    }

    #[test]
    fn isolation_mass_at_1_au() {
        let params = PlanetParams::default();
        let derived = DerivedProperties::from_params(&params);
        assert!(
            derived.isolation_mass > 0.05 && derived.isolation_mass < 0.2,
            "Isolation mass at 1 AU should be ~0.11 M⊕, got {}",
            derived.isolation_mass
        );
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
