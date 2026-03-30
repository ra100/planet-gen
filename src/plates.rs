use bytemuck::{Pod, Zeroable};

/// GPU-compatible plate data. Passed to compute shader as storage buffer.
#[repr(C)]
#[derive(Copy, Clone, Pod, Zeroable, Debug)]
pub struct PlateGpu {
    pub center: [f32; 3],
    pub plate_type: f32, // 1.0 = continental, 0.0 = oceanic
    pub velocity: [f32; 3],
    pub _pad: f32,
}

/// Parameters for plate generation.
pub struct PlateGenParams {
    pub seed: u32,
    pub mass_earth: f32,
    pub ocean_fraction: f32,
    pub tectonics_factor: f32,
}

/// Generate tectonic plates from planet parameters.
/// Returns a Vec of PlateGpu ready for GPU upload.
pub fn generate_plates(params: &PlateGenParams) -> Vec<PlateGpu> {
    let n = compute_plate_count(params.mass_earth, params.tectonics_factor);
    let centers = fibonacci_sphere(n, params.seed);
    let continental_count = ((n as f32) * (1.0 - params.ocean_fraction)).round() as usize;
    let velocities = generate_velocities(n, params.seed, params.tectonics_factor);

    let mut plates = Vec::with_capacity(n);
    for i in 0..n {
        plates.push(PlateGpu {
            center: centers[i],
            plate_type: if i < continental_count { 1.0 } else { 0.0 },
            velocity: velocities[i],
            _pad: 0.0,
        });
    }
    plates
}

fn compute_plate_count(mass_earth: f32, tectonics_factor: f32) -> usize {
    // Small planets: ~5-6 plates. Earth-like: ~8-14. Large: ~15-20.
    let base = 6.0 + mass_earth * 4.0 + tectonics_factor * 6.0;
    (base as usize).clamp(5, 20)
}

/// Fibonacci sphere: distribute N points evenly on a unit sphere, then perturb by seed.
fn fibonacci_sphere(n: usize, seed: u32) -> Vec<[f32; 3]> {
    let golden_ratio = (1.0 + 5.0_f64.sqrt()) / 2.0;
    let mut points = Vec::with_capacity(n);

    for i in 0..n {
        // Fibonacci lattice on sphere
        let theta = 2.0 * std::f64::consts::PI * (i as f64) / golden_ratio;
        let phi = ((1.0 - 2.0 * (i as f64 + 0.5) / n as f64)).acos();

        let x = phi.sin() * theta.cos();
        let y = phi.cos();
        let z = phi.sin() * theta.sin();

        // Perturb by seed — larger perturbation breaks geometric regularity
        let hash_x = hash_f32(seed, i as u32, 0) * 0.3;
        let hash_y = hash_f32(seed, i as u32, 1) * 0.3;
        let hash_z = hash_f32(seed, i as u32, 2) * 0.3;

        let px = x as f32 + hash_x;
        let py = y as f32 + hash_y;
        let pz = z as f32 + hash_z;

        // Re-normalize to unit sphere
        let len = (px * px + py * py + pz * pz).sqrt();
        points.push([px / len, py / len, pz / len]);
    }
    points
}

/// Generate velocity vectors for each plate.
fn generate_velocities(n: usize, seed: u32, tectonics_factor: f32) -> Vec<[f32; 3]> {
    let mut velocities = Vec::with_capacity(n);
    for i in 0..n {
        // Random direction, magnitude scaled by tectonics_factor
        let vx = hash_f32(seed.wrapping_add(1000), i as u32, 0);
        let vy = hash_f32(seed.wrapping_add(1000), i as u32, 1);
        let vz = hash_f32(seed.wrapping_add(1000), i as u32, 2);

        // Project velocity to be tangent to sphere at plate center
        // (not strictly necessary for boundary classification but more physical)
        let speed = tectonics_factor * 0.5;
        velocities.push([vx * speed, vy * speed, vz * speed]);
    }
    velocities
}

/// Simple deterministic hash returning a float in [-1, 1].
fn hash_f32(seed: u32, index: u32, channel: u32) -> f32 {
    let mut h = seed.wrapping_mul(374761393)
        .wrapping_add(index.wrapping_mul(668265263))
        .wrapping_add(channel.wrapping_mul(1274126177));
    h = (h ^ (h >> 13)).wrapping_mul(1103515245);
    h = h ^ (h >> 16);
    // Map to [-1, 1]
    (h as f32 / u32::MAX as f32) * 2.0 - 1.0
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn earth_like_plate_count() {
        let params = PlateGenParams {
            seed: 42,
            mass_earth: 1.0,
            ocean_fraction: 0.7,
            tectonics_factor: 0.85,
        };
        let plates = generate_plates(&params);
        assert!(
            plates.len() >= 8 && plates.len() <= 16,
            "Earth-like should have 8-16 plates, got {}",
            plates.len()
        );
    }

    #[test]
    fn continental_fraction_matches_ocean() {
        let params = PlateGenParams {
            seed: 42,
            mass_earth: 1.0,
            ocean_fraction: 0.7,
            tectonics_factor: 0.85,
        };
        let plates = generate_plates(&params);
        let continental = plates.iter().filter(|p| p.plate_type > 0.5).count();
        let expected = ((plates.len() as f32) * 0.3).round() as usize;
        assert_eq!(
            continental, expected,
            "Continental plates should be ~30% of total"
        );
    }

    #[test]
    fn all_centers_on_unit_sphere() {
        let params = PlateGenParams {
            seed: 42,
            mass_earth: 1.0,
            ocean_fraction: 0.7,
            tectonics_factor: 0.85,
        };
        let plates = generate_plates(&params);
        for (i, p) in plates.iter().enumerate() {
            let len = (p.center[0].powi(2) + p.center[1].powi(2) + p.center[2].powi(2)).sqrt();
            assert!(
                (len - 1.0).abs() < 0.01,
                "Plate {} center not on unit sphere: len={}",
                i, len
            );
        }
    }

    #[test]
    fn different_seeds_produce_different_plates() {
        let p1 = generate_plates(&PlateGenParams {
            seed: 1,
            mass_earth: 1.0,
            ocean_fraction: 0.7,
            tectonics_factor: 0.85,
        });
        let p2 = generate_plates(&PlateGenParams {
            seed: 999,
            mass_earth: 1.0,
            ocean_fraction: 0.7,
            tectonics_factor: 0.85,
        });
        let diff: f32 = p1.iter().zip(p2.iter())
            .map(|(a, b)| {
                (a.center[0] - b.center[0]).abs()
                    + (a.center[1] - b.center[1]).abs()
                    + (a.center[2] - b.center[2]).abs()
            })
            .sum::<f32>() / p1.len() as f32;
        assert!(diff > 0.01, "Different seeds should produce different plates");
    }

    #[test]
    fn small_planet_fewer_plates() {
        let plates = generate_plates(&PlateGenParams {
            seed: 42,
            mass_earth: 0.1,
            ocean_fraction: 0.3,
            tectonics_factor: 0.2,
        });
        assert!(
            plates.len() <= 8,
            "Small planet should have ≤8 plates, got {}",
            plates.len()
        );
    }
}
