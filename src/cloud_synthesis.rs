//! Cloud synthesis: Earth-like cloud structure on top of the weather solver.
//!
//! The weather solver places condensate on a coarse grid (about 26 km per
//! texel on an Earth-size planet) and has no synoptic weather systems, so its
//! field reads as one wind-streaked texture everywhere. Synthesis keeps the
//! solver's fields as moisture and cloud-type hints and adds what satellite
//! cloud maps are made of:
//!
//! - a catalogue of synoptic systems ([`weather_systems`]): extratropical
//!   cyclones along the storm tracks, sized and counted from the most unstable
//!   baroclinic (Eady) wavelength, and tropical cyclones over warm ocean;
//! - per-pixel regimes in `cloud_synthesis.wgsl` (ITCZ convective clusters,
//!   trade cumulus, west-coast stratocumulus decks, cold-sector open cells,
//!   frontal stratiform, polar stratus, jet cirrus, dry subtropical land);
//! - a resolution-independent coverage remap, so a 2K and an 8K map of the
//!   same planet agree once downsampled while the 8K one resolves more detail.
//!
//! The catalogue is a pure function of the weather snapshot, so preview and
//! export build the same systems from the same snapshot.

use crate::weather::WeatherSnapshot;
use bytemuck::{Pod, Zeroable};

/// Systems the shader loops over; the uniform array has this fixed size.
pub const MAX_CLOUD_SYSTEMS: usize = 48;

/// Extratropical (baroclinic) cyclone: comma cloud, fronts, dry slot.
pub const SYSTEM_EXTRATROPICAL: f32 = 0.0;
/// Tropical cyclone: eye, central dense overcast, spiral rainbands.
pub const SYSTEM_TROPICAL: f32 = 1.0;
/// Ridge between storm-track lows: subsidence clearing and anticyclonic flow.
pub const SYSTEM_ANTICYCLONE: f32 = 2.0;

const EARTH_ROTATION_RAD_S: f32 = 7.292_115e-5;
/// Most unstable Eady wavelength on Earth at 45 degrees latitude, km.
const EARTH_EADY_WAVELENGTH_KM: f32 = 4_000.0;
/// Tropical cyclone cloud-shield radius on Earth, km.
const EARTH_TROPICAL_RADIUS_KM: f32 = 520.0;

/// One synoptic system, laid out as four `vec4<f32>` for `cloud_synthesis.wgsl`.
#[repr(C)]
#[derive(Copy, Clone, Debug, Default, PartialEq, Pod, Zeroable)]
pub struct CloudSystemGpu {
    /// xyz unit centre in planet space, w angular radius (radians).
    pub center: [f32; 4],
    /// xyz unit east tangent at the centre (circulation frame), w kind.
    pub frame: [f32; 4],
    /// Life-cycle stage (0 open wave .. 1 occluded), intensity (0-1),
    /// hemisphere (+1 north, -1 south), variant (0-1).
    pub shape: [f32; 4],
    /// Cold-front length (radii), front curvature, east-west aspect, front tilt.
    pub extra: [f32; 4],
}

/// Uniform block shared by the preview and the cloud export.
#[repr(C)]
#[derive(Copy, Clone, Debug, PartialEq, Pod, Zeroable)]
pub struct CloudSynthesisGpu {
    /// Strength (0 = solver field only), system count, climate moisture (0-1),
    /// reserved.
    pub control: [f32; 4],
    /// Hadley edge, subpolar low and storm-track latitudes (degrees), and the
    /// relative rotation rate.
    pub climate: [f32; 4],
    pub systems: [CloudSystemGpu; MAX_CLOUD_SYSTEMS],
}

impl Default for CloudSynthesisGpu {
    fn default() -> Self {
        Self::zeroed()
    }
}

impl CloudSynthesisGpu {
    /// Synthesis off: the shaders render the solver field unchanged.
    pub fn disabled() -> Self {
        Self::zeroed()
    }

    pub fn strength(&self) -> f32 {
        self.control[0]
    }

    pub fn system_count(&self) -> usize {
        self.control[1] as usize
    }
}

/// Small deterministic generator (PCG-style) so the catalogue only depends on
/// the snapshot, never on platform RNG state.
struct Rng(u64);

impl Rng {
    fn new(seed: u32, stream: u32) -> Self {
        let mut rng = Self(
            (u64::from(seed) << 32 | u64::from(stream)).wrapping_mul(0x9e37_79b9_7f4a_7c15)
                ^ 0xd1b5_4a32_d192_ed03,
        );
        rng.next();
        rng.next();
        rng
    }

    fn next(&mut self) -> u32 {
        self.0 = self
            .0
            .wrapping_mul(6_364_136_223_846_793_005)
            .wrapping_add(1_442_695_040_888_963_407);
        let x = ((self.0 >> 18) ^ self.0) >> 27;
        let rot = (self.0 >> 59) as u32;
        (x as u32).rotate_right(rot)
    }

    /// Uniform in [0, 1).
    fn unit(&mut self) -> f32 {
        (self.next() >> 8) as f32 / (1u32 << 24) as f32
    }

    fn range(&mut self, low: f32, high: f32) -> f32 {
        low + (high - low) * self.unit()
    }
}

/// Hadley cell edge latitude in degrees; mirrors `hadley_top_lat` in
/// `wind_field.wgsl` (Kaspi & Showman rotation scaling, temperature widening).
pub fn hadley_edge_deg(rotation_relative: f32, base_temp_c: f32) -> f32 {
    let omega = rotation_relative.max(0.1);
    let mut edge = (30.0 / omega.powf(0.3)).min(70.0);
    if base_temp_c <= 21.0 {
        edge += (base_temp_c - 15.0).clamp(-20.0, 6.0) * 0.25;
    } else {
        edge += 1.5 - (base_temp_c - 21.0).clamp(0.0, 14.0) * 0.25;
    }
    edge.clamp(15.0, 70.0)
}

/// Subpolar low latitude in degrees; mirrors `subpolar_lat` in `wind_field.wgsl`.
pub fn subpolar_low_deg(rotation_relative: f32) -> f32 {
    (60.0 / rotation_relative.max(0.1).powf(0.2)).min(80.0)
}

/// Storm-track latitude: cyclones form on the poleward side of the Hadley
/// edge and occlude toward the subpolar low (about 46 degrees on Earth).
pub fn storm_track_deg(rotation_relative: f32, base_temp_c: f32) -> f32 {
    let hadley = hadley_edge_deg(rotation_relative, base_temp_c);
    let subpolar = subpolar_low_deg(rotation_relative).max(hadley + 10.0);
    hadley + 0.55 * (subpolar - hadley)
}

/// Unit vector from latitude/longitude in the circulation (tilted) frame.
fn circulation_direction(latitude: f32, longitude: f32, tilt: f32) -> [f32; 3] {
    // Pole of the circulation frame, matching `cs_pole()` in the shader and the
    // tilted latitude used by `climate_latitude`.
    let pole = [0.0, tilt.cos(), tilt.sin()];
    let e1 = [1.0, 0.0, 0.0];
    let e2 = cross(pole, e1);
    let (sin_lat, cos_lat) = latitude.sin_cos();
    let (sin_lon, cos_lon) = longitude.sin_cos();
    normalize([
        cos_lat * (cos_lon * e1[0] + sin_lon * e2[0]) + sin_lat * pole[0],
        cos_lat * (cos_lon * e1[1] + sin_lon * e2[1]) + sin_lat * pole[1],
        cos_lat * (cos_lon * e1[2] + sin_lon * e2[2]) + sin_lat * pole[2],
    ])
}

fn cross(a: [f32; 3], b: [f32; 3]) -> [f32; 3] {
    [
        a[1] * b[2] - a[2] * b[1],
        a[2] * b[0] - a[0] * b[2],
        a[0] * b[1] - a[1] * b[0],
    ]
}

fn normalize(v: [f32; 3]) -> [f32; 3] {
    let length = (v[0] * v[0] + v[1] * v[1] + v[2] * v[2])
        .sqrt()
        .max(1.0e-12);
    [v[0] / length, v[1] / length, v[2] / length]
}

fn east_at(center: [f32; 3], tilt: f32) -> [f32; 3] {
    let pole = [0.0, tilt.cos(), tilt.sin()];
    normalize(cross(pole, center))
}

/// Builds the synoptic catalogue for `snapshot`. `is_ocean` reports whether a
/// planet-space direction is open water; tropical cyclones only form there.
///
/// Physics: cyclone spacing is the most unstable baroclinic wavelength, which
/// scales with the Rossby radius `N H / f` and therefore inversely with the
/// rotation rate. Storm tracks sit between the Hadley edge and the subpolar
/// low, shifted equatorward and strengthened in the winter hemisphere. Young
/// waves form on the equatorward side of the track and occlude poleward.
/// Tropical cyclones need warm water (a base temperature that puts the deep
/// tropics above about 26 C) and form at 10-25 degrees, mostly in the summer
/// hemisphere.
pub fn weather_systems(
    snapshot: &WeatherSnapshot,
    strength: f32,
    is_ocean: impl Fn([f32; 3]) -> bool,
) -> CloudSynthesisGpu {
    let mut data = CloudSynthesisGpu::zeroed();
    let strength = if strength.is_finite() {
        strength.clamp(0.0, 1.0)
    } else {
        0.0
    };
    let rotation = (snapshot.rotation_rate_rad_s.abs() / EARTH_ROTATION_RAD_S).max(0.05);
    let hadley = hadley_edge_deg(rotation, snapshot.base_temp_c);
    let subpolar = subpolar_low_deg(rotation);
    let track = storm_track_deg(rotation, snapshot.base_temp_c);
    data.control = [strength, 0.0, snapshot.moisture.clamp(0.0, 1.0), 0.0];
    data.climate = [hadley, subpolar, track, rotation];
    if strength <= 0.0 || snapshot.coverage <= 0.0 || snapshot.moisture <= 0.0 {
        return data;
    }

    let tilt = snapshot.axial_tilt_rad;
    let radius_km = snapshot.radius_km.max(100.0);
    // +1: northern summer, -1: northern winter.
    let season_sign = ((snapshot.season - 0.5) * 2.0).clamp(-1.0, 1.0)
        * (tilt / 23.44_f32.to_radians()).clamp(0.0, 1.5);
    let mut systems = Vec::with_capacity(MAX_CLOUD_SYSTEMS);

    // Baroclinic cyclones. The Coriolis parameter at the track latitude sets
    // the Rossby radius, so the wavelength follows 1 / (Omega sin(lat)).
    let track_rad = track.to_radians();
    let wavelength_km = (EARTH_EADY_WAVELENGTH_KM / rotation
        * (45.0_f32.to_radians().sin() / track_rad.sin().max(0.2)))
    .clamp(600.0, 12_000.0);
    // Cold, dry planets have weak baroclinic storms; hot ones keep them.
    let vigor = (snapshot.moisture.clamp(0.0, 1.0) * 0.6 + 0.4)
        * ((snapshot.base_temp_c + 40.0) / 55.0).clamp(0.2, 1.0);
    for hemisphere in [1.0_f32, -1.0] {
        let mut rng = Rng::new(snapshot.seed, if hemisphere > 0.0 { 11 } else { 12 });
        // Winter hemisphere: stronger, more numerous, further equatorward.
        let winter = (-season_sign * hemisphere).clamp(-1.0, 1.0);
        let lat_center = track - 3.0 * winter;
        let circumference = std::f32::consts::TAU * radius_km * lat_center.to_radians().cos();
        let count = ((circumference / wavelength_km) * (1.0 + 0.12 * winter))
            .round()
            .clamp(2.0, 13.0) as usize;
        let phase = rng.unit() * std::f32::consts::TAU;
        let mut longitudes = Vec::with_capacity(count);
        for index in 0..count {
            if systems.len() >= MAX_CLOUD_SYSTEMS - 4 {
                break;
            }
            let stage = rng.range(0.05, 1.0);
            // Young waves near the equatorward flank, occluded lows poleward.
            let latitude = (lat_center - 7.0 + 14.0 * stage + rng.range(-4.0, 4.0))
                .clamp(hadley + 4.0, 80.0)
                * hemisphere;
            let longitude = phase
                + std::f32::consts::TAU * (index as f32 + rng.range(-0.3, 0.3)) / count as f32;
            longitudes.push(longitude);
            let center = circulation_direction(latitude.to_radians(), longitude, tilt);
            let size_km = wavelength_km * 0.32 * rng.range(0.7, 1.3) * (1.0 - 0.15 * stage);
            let intensity = (rng.range(0.55, 1.0) * vigor * (1.0 + 0.15 * winter)).clamp(0.0, 1.0);
            let front_length = rng.range(2.0, 3.2);
            let curvature = rng.range(0.06, 0.2);
            let primary = CloudSystemGpu {
                center: [center[0], center[1], center[2], size_km / radius_km],
                frame: {
                    let east = east_at(center, tilt);
                    [east[0], east[1], east[2], SYSTEM_EXTRATROPICAL]
                },
                shape: [stage, intensity, hemisphere, rng.unit()],
                extra: [
                    front_length,
                    curvature,
                    rng.range(1.0, 1.45),
                    rng.range(-0.25, 0.3),
                ],
            };
            systems.push(primary);
            // Secondary frontal wave: mature lows often spawn a young wave on
            // their trailing cold front, 1.2-1.9 radii (1000-2000 km) away.
            if stage > 0.45 && rng.unit() < 0.55 && systems.len() < MAX_CLOUD_SYSTEMS - 4 {
                let along = rng
                    .range(1.2, 1.9)
                    .min(0.8 * front_length)
                    .min(2_000.0 / size_km);
                let local = [0.06 - 0.32 * along - curvature * along * along, -along];
                let angle_radius = size_km / radius_km;
                let east = east_at(center, tilt);
                let north = cross(center, east);
                let offset = [
                    local[0] * east[0] + local[1] * hemisphere * north[0],
                    local[0] * east[1] + local[1] * hemisphere * north[1],
                    local[0] * east[2] + local[1] * hemisphere * north[2],
                ];
                let wave_center = normalize([
                    center[0] + angle_radius * offset[0],
                    center[1] + angle_radius * offset[1],
                    center[2] + angle_radius * offset[2],
                ]);
                let wave_east = east_at(wave_center, tilt);
                let wave_latitude = (wave_center[1] * tilt.cos() + wave_center[2] * tilt.sin())
                    .clamp(-1.0, 1.0)
                    .asin()
                    .to_degrees();
                // Waves only form on the baroclinic flank, poleward of the Hadley edge.
                if wave_latitude.abs() < hadley {
                    continue;
                }
                systems.push(CloudSystemGpu {
                    center: [
                        wave_center[0],
                        wave_center[1],
                        wave_center[2],
                        angle_radius * rng.range(0.5, 0.75),
                    ],
                    frame: [
                        wave_east[0],
                        wave_east[1],
                        wave_east[2],
                        SYSTEM_EXTRATROPICAL,
                    ],
                    shape: [
                        rng.range(0.0, 0.25),
                        intensity * rng.range(0.6, 0.9),
                        hemisphere,
                        rng.unit(),
                    ],
                    extra: [
                        rng.range(1.6, 2.6),
                        rng.range(0.04, 0.18),
                        rng.range(1.1, 1.5),
                        rng.range(-0.2, 0.3),
                    ],
                });
            }
        }
        // Ridges between consecutive lows, on the equatorward side of the track.
        for (index, &longitude) in longitudes.iter().enumerate() {
            if systems.len() >= MAX_CLOUD_SYSTEMS - 4 {
                break;
            }
            let next = longitudes[(index + 1) % longitudes.len()]
                + if index + 1 == longitudes.len() {
                    std::f32::consts::TAU
                } else {
                    0.0
                };
            let latitude = (lat_center - 6.0 + rng.range(-4.0, 4.0)) * hemisphere;
            let center =
                circulation_direction(latitude.to_radians(), 0.5 * (longitude + next), tilt);
            let east = east_at(center, tilt);
            systems.push(CloudSystemGpu {
                center: [
                    center[0],
                    center[1],
                    center[2],
                    wavelength_km * 0.3 * rng.range(0.7, 1.2) / radius_km,
                ],
                frame: [east[0], east[1], east[2], SYSTEM_ANTICYCLONE],
                shape: [0.0, rng.range(0.5, 1.0), hemisphere, rng.unit()],
                extra: [0.0, 0.0, rng.range(1.2, 1.8), 0.0],
            });
        }
    }

    // Tropical cyclones over warm ocean. The tropics run about 12 C above the
    // base temperature, so a 14 C base keeps them near the 26 C threshold.
    let tropical_sst = snapshot.base_temp_c + 12.0;
    if tropical_sst > 24.0 && snapshot.moisture > 0.2 {
        let mut rng = Rng::new(snapshot.seed, 21);
        let warmth = ((tropical_sst - 24.0) / 6.0).clamp(0.0, 1.5);
        let wanted = (rng.range(0.4, 2.6) * warmth).round() as usize;
        let size_km = (EARTH_TROPICAL_RADIUS_KM / rotation.sqrt()).clamp(150.0, 1500.0);
        let mut attempts = 0;
        let mut placed = 0;
        while placed < wanted && attempts < 24 && systems.len() < MAX_CLOUD_SYSTEMS {
            attempts += 1;
            // Mostly the summer hemisphere.
            let hemisphere = if rng.unit() < 0.5 + 0.35 * season_sign {
                1.0
            } else {
                -1.0
            };
            let latitude = rng.range(10.0, 24.0) * hemisphere;
            let longitude = rng.unit() * std::f32::consts::TAU;
            let center = circulation_direction(latitude.to_radians(), longitude, tilt);
            let east = east_at(center, tilt);
            // Open water around the eye too, so preview and export terrain of
            // different resolutions agree near coasts.
            let north = cross(center, east);
            let reach = 300.0 / radius_km;
            let open_water = is_ocean(center)
                && [(1.0, 0.0), (-1.0, 0.0), (0.0, 1.0), (0.0, -1.0)]
                    .iter()
                    .all(|(a, b)| {
                        is_ocean(normalize([
                            center[0] + reach * (a * east[0] + b * north[0]),
                            center[1] + reach * (a * east[1] + b * north[1]),
                            center[2] + reach * (a * east[2] + b * north[2]),
                        ]))
                    });
            if !open_water {
                continue;
            }
            systems.push(CloudSystemGpu {
                center: [
                    center[0],
                    center[1],
                    center[2],
                    size_km * rng.range(0.75, 1.25) / radius_km,
                ],
                frame: [east[0], east[1], east[2], SYSTEM_TROPICAL],
                shape: [
                    rng.range(0.3, 1.0),
                    rng.range(0.6, 1.0),
                    hemisphere,
                    rng.unit(),
                ],
                extra: [rng.range(1.2, 2.2), rng.range(2.0, 4.0), 1.0, 0.0],
            });
            placed += 1;
        }
    }

    data.control[1] = systems.len() as f32;
    for (slot, system) in data.systems.iter_mut().zip(systems) {
        *slot = system;
    }
    data
}

#[cfg(test)]
mod tests {
    use super::*;

    fn earth() -> WeatherSnapshot {
        WeatherSnapshot {
            seed: 1042,
            coverage: 0.5,
            moisture: 1.0,
            axial_tilt_rad: 23.44_f32.to_radians(),
            ..WeatherSnapshot::default()
        }
    }

    fn latitude_deg(center: [f32; 4], tilt: f32) -> f32 {
        (center[1] * tilt.cos() + center[2] * tilt.sin())
            .clamp(-1.0, 1.0)
            .asin()
            .to_degrees()
    }

    #[test]
    fn layout_matches_the_shader_uniform_block() {
        assert_eq!(std::mem::size_of::<CloudSystemGpu>(), 64);
        assert_eq!(
            std::mem::size_of::<CloudSynthesisGpu>(),
            32 + 64 * MAX_CLOUD_SYSTEMS
        );
    }

    #[test]
    fn earth_gets_storm_track_cyclones_in_both_hemispheres() {
        for seed in [1042, 7, 73, 137, 509, 997] {
            earth_storm_tracks(WeatherSnapshot { seed, ..earth() });
        }
    }

    fn earth_storm_tracks(snapshot: WeatherSnapshot) {
        let data = weather_systems(&snapshot, 1.0, |_| true);
        let systems = &data.systems[..data.system_count()];
        for hemisphere in [1.0, -1.0] {
            let of_kind = |kind: f32| {
                systems
                    .iter()
                    .filter(|s| s.frame[3] == kind && s.shape[2] == hemisphere)
                    .collect::<Vec<_>>()
            };
            // One ridge between each pair of primary lows: five to eight per
            // hemisphere, as on Earth; secondary waves add at most one each.
            let ridges = of_kind(SYSTEM_ANTICYCLONE).len();
            let lows = of_kind(SYSTEM_EXTRATROPICAL);
            assert!((5..=9).contains(&ridges), "ridges {ridges}");
            assert!(
                (ridges..=2 * ridges).contains(&lows.len()),
                "lows {}",
                lows.len()
            );
            for system in lows {
                let latitude = latitude_deg(system.center, snapshot.axial_tilt_rad);
                assert!(
                    (25.0..=75.0).contains(&latitude.abs()),
                    "latitude {latitude}"
                );
                assert_eq!(latitude.signum(), hemisphere);
                let radius_km = system.center[3] * snapshot.radius_km;
                assert!((300.0..=2000.0).contains(&radius_km), "radius {radius_km}");
            }
        }
    }

    #[test]
    fn faster_rotation_makes_more_smaller_cyclones() {
        let earth = earth();
        let fast = WeatherSnapshot {
            rotation_rate_rad_s: earth.rotation_rate_rad_s * 3.0,
            ..earth
        };
        let count = |s: &WeatherSnapshot| {
            let data = weather_systems(s, 1.0, |_| false);
            let lows: Vec<_> = data.systems[..data.system_count()]
                .iter()
                .filter(|s| s.frame[3] == SYSTEM_EXTRATROPICAL)
                .collect();
            let mean_radius = lows.iter().map(|s| s.center[3]).sum::<f32>() / lows.len() as f32;
            (lows.len(), mean_radius)
        };
        let (earth_count, earth_radius) = count(&earth);
        let (fast_count, fast_radius) = count(&fast);
        assert!(fast_count > earth_count);
        assert!(fast_radius < earth_radius * 0.6);
    }

    #[test]
    fn tropical_cyclones_need_warm_ocean() {
        let earth = earth();
        let warm = WeatherSnapshot {
            base_temp_c: 22.0,
            ..earth
        };
        let tropical = |s: &WeatherSnapshot, ocean: bool| {
            let data = weather_systems(s, 1.0, |_| ocean);
            data.systems[..data.system_count()]
                .iter()
                .filter(|s| s.frame[3] == SYSTEM_TROPICAL)
                .count()
        };
        assert_eq!(tropical(&warm, false), 0);
        let cold = WeatherSnapshot {
            base_temp_c: -10.0,
            ..earth
        };
        assert_eq!(tropical(&cold, true), 0);
        let total: usize = (0..16)
            .map(|seed| tropical(&WeatherSnapshot { seed, ..warm }, true))
            .sum();
        assert!(
            total >= 16,
            "warm oceans should average one or more ({total})"
        );
    }

    #[test]
    fn catalogue_is_deterministic_and_off_when_disabled() {
        let snapshot = earth();
        assert_eq!(
            weather_systems(&snapshot, 1.0, |_| true),
            weather_systems(&snapshot, 1.0, |_| true)
        );
        let off = weather_systems(&snapshot, 0.0, |_| true);
        assert_eq!(off.system_count(), 0);
        assert_eq!(off.strength(), 0.0);
        let dry = weather_systems(
            &WeatherSnapshot {
                moisture: 0.0,
                ..snapshot
            },
            1.0,
            |_| true,
        );
        assert_eq!(dry.system_count(), 0);
        let other_seed = weather_systems(
            &WeatherSnapshot {
                seed: 7,
                ..snapshot
            },
            1.0,
            |_| true,
        );
        assert_ne!(
            other_seed.systems,
            weather_systems(&snapshot, 1.0, |_| true).systems
        );
    }
}
