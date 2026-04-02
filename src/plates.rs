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
    /// Continental scale: lower = fewer, larger plates. Higher = more, smaller plates.
    pub continental_scale: f32,
    /// Override plate count (0 = auto from physics).
    pub num_plates_override: u32,
}

/// Generate tectonic plates from planet parameters.
/// Returns a Vec of PlateGpu ready for GPU upload.
pub fn generate_plates(params: &PlateGenParams) -> Vec<PlateGpu> {
    let n = if params.num_plates_override > 0 {
        params.num_plates_override as usize
    } else {
        compute_plate_count(params.mass_earth, params.tectonics_factor, params.continental_scale)
    };
    let centers = fibonacci_sphere(n, params.seed);
    let continental_count = ((n as f32) * (1.0 - params.ocean_fraction)).round() as usize;
    let velocities = generate_velocities(n, params.seed, params.tectonics_factor, &centers);

    // Assign continental/oceanic by seed-based scoring, not index order.
    // This prevents continental plates from clustering at one pole.
    let mut plate_scores: Vec<(usize, f32)> = (0..n)
        .map(|i| (i, hash_f32(params.seed.wrapping_add(7777), i as u32, 3)))
        .collect();
    plate_scores.sort_by(|a, b| a.1.partial_cmp(&b.1).unwrap());
    let continental_indices: Vec<bool> = {
        let mut is_continental = vec![false; n];
        for k in 0..continental_count.min(n) {
            is_continental[plate_scores[k].0] = true;
        }
        is_continental
    };

    let mut plates = Vec::with_capacity(n);
    for i in 0..n {
        plates.push(PlateGpu {
            center: centers[i],
            plate_type: if continental_indices[i] { 1.0 } else { 0.0 },
            velocity: velocities[i],
            _pad: 0.0,
        });
    }
    plates
}

fn compute_plate_count(mass_earth: f32, tectonics_factor: f32, continental_scale: f32) -> usize {
    // Base count from physics: small planets ~5-6, Earth-like ~8-14, large ~15-20
    let base = 6.0 + mass_earth * 4.0 + tectonics_factor * 6.0;
    // Continental scale modifies: lower scale → fewer plates (bigger continents)
    let scale_factor = 0.4 + 0.4 * continental_scale;
    let raw = (base * scale_factor) as usize;
    // Round to even numbers to reduce frequency of discrete jumps when sliding parameters
    let even = (raw / 2) * 2;
    even.clamp(4, 24)
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

/// Generate velocity vectors for each plate, tangent to the sphere surface.
/// Uses an Euler pole rotation model: each plate rotates around a seed-derived
/// pole axis, giving velocity = cross(pole, center) * speed.
/// The radial component is explicitly projected out to guarantee tangency.
/// Requires the actual plate centers so the projection is accurate.
fn generate_velocities(n: usize, seed: u32, tectonics_factor: f32, centers: &[[f32; 3]]) -> Vec<[f32; 3]> {
    let mut velocities = Vec::with_capacity(n);
    let speed = tectonics_factor * 0.5;

    for i in 0..n {
        let center = centers[i];

        // Each plate gets a unique Euler rotation pole derived from its index + seed
        let pole_seed = seed.wrapping_add(1000);
        let px = hash_f32(pole_seed, i as u32, 0);
        let py = hash_f32(pole_seed, i as u32, 1);
        let pz = hash_f32(pole_seed, i as u32, 2);
        let pole_len = (px * px + py * py + pz * pz).sqrt().max(1e-6);
        let pole = [px / pole_len, py / pole_len, pz / pole_len];

        // cross(pole, center) — nominally tangent to sphere at center
        let vx = pole[1] * center[2] - pole[2] * center[1];
        let vy = pole[2] * center[0] - pole[0] * center[2];
        let vz = pole[0] * center[1] - pole[1] * center[0];

        // Project out radial component: v_tangent = v - dot(v, center) * center
        // (center is already a unit vector since it's on the unit sphere)
        let dot_vc = vx * center[0] + vy * center[1] + vz * center[2];
        let tx = vx - dot_vc * center[0];
        let ty = vy - dot_vc * center[1];
        let tz = vz - dot_vc * center[2];

        let t_len = (tx * tx + ty * ty + tz * tz).sqrt().max(1e-6);
        velocities.push([
            (tx / t_len) * speed,
            (ty / t_len) * speed,
            (tz / t_len) * speed,
        ]);
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
            continental_scale: 1.0,
            num_plates_override: 0,
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
            continental_scale: 1.0,
            num_plates_override: 0,
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
            continental_scale: 1.0,
            num_plates_override: 0,
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
            continental_scale: 1.0,
            num_plates_override: 0,
        });
        let p2 = generate_plates(&PlateGenParams {
            seed: 999,
            mass_earth: 1.0,
            ocean_fraction: 0.7,
            tectonics_factor: 0.85,
            continental_scale: 1.0,
            num_plates_override: 0,
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
            continental_scale: 1.0,
            num_plates_override: 0,
        });
        assert!(
            plates.len() <= 8,
            "Small planet should have ≤8 plates, got {}",
            plates.len()
        );
    }

    #[test]
    fn velocities_are_nonzero_with_tectonics() {
        let params = PlateGenParams {
            seed: 42,
            mass_earth: 1.0,
            ocean_fraction: 0.7,
            tectonics_factor: 0.85,
            continental_scale: 1.0,
            num_plates_override: 0,
        };
        let plates = generate_plates(&params);
        for (i, p) in plates.iter().enumerate() {
            let mag = (p.velocity[0].powi(2) + p.velocity[1].powi(2) + p.velocity[2].powi(2)).sqrt();
            assert!(
                mag > 0.001,
                "Plate {} velocity should be non-zero, got {}",
                i, mag
            );
        }
    }

    #[test]
    fn velocities_scale_with_tectonics_factor() {
        let plates_low = generate_plates(&PlateGenParams {
            seed: 42,
            mass_earth: 1.0,
            ocean_fraction: 0.7,
            tectonics_factor: 0.1,
            continental_scale: 1.0,
            num_plates_override: 0,
        });
        let plates_high = generate_plates(&PlateGenParams {
            seed: 42,
            mass_earth: 1.0,
            ocean_fraction: 0.7,
            tectonics_factor: 1.0,
            continental_scale: 1.0,
            num_plates_override: 0,
        });
        let avg_low: f32 = plates_low.iter()
            .map(|p| (p.velocity[0].powi(2) + p.velocity[1].powi(2) + p.velocity[2].powi(2)).sqrt())
            .sum::<f32>() / plates_low.len() as f32;
        let avg_high: f32 = plates_high.iter()
            .map(|p| (p.velocity[0].powi(2) + p.velocity[1].powi(2) + p.velocity[2].powi(2)).sqrt())
            .sum::<f32>() / plates_high.len() as f32;
        assert!(
            avg_high > avg_low * 2.0,
            "High tectonics_factor should produce faster plates: low={:.4}, high={:.4}",
            avg_low, avg_high
        );
    }

    #[test]
    fn velocities_tangent_to_sphere() {
        let params = PlateGenParams {
            seed: 42,
            mass_earth: 1.0,
            ocean_fraction: 0.7,
            tectonics_factor: 0.85,
            continental_scale: 1.0,
            num_plates_override: 8,
        };
        let plates = generate_plates(&params);
        for (i, p) in plates.iter().enumerate() {
            // dot(velocity, center) should be ~0 for tangent vectors
            let dot = p.velocity[0] * p.center[0]
                + p.velocity[1] * p.center[1]
                + p.velocity[2] * p.center[2];
            let v_mag = (p.velocity[0].powi(2) + p.velocity[1].powi(2) + p.velocity[2].powi(2)).sqrt();
            // Normalize dot by velocity magnitude; should be small
            let normalized_dot = if v_mag > 1e-6 { (dot / v_mag).abs() } else { 0.0 };
            assert!(
                normalized_dot < 0.1,
                "Plate {} velocity not tangent to sphere: normalized dot = {:.4}",
                i, normalized_dot
            );
        }
    }
}
