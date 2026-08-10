use bytemuck::{Pod, Zeroable};

/// GPU-compatible plate data. Passed to compute shader as storage buffer.
#[repr(C)]
#[derive(Copy, Clone, Pod, Zeroable, Debug)]
pub struct PlateGpu {
    pub center: [f32; 3],
    pub plate_type: f32, // 1.0 = continental, 0.0 = oceanic
    /// Euler angular velocity (ω), not a velocity sampled at `center`.
    /// Shaders derive local tangential velocity with `cross(ω, position)`.
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
    /// Target number of continental plates (1-10). 0 = derive from ocean_fraction.
    pub num_continents: u32,
    /// Continent size distribution. 0 = equal sizes, 1 = one large supercontinent + small islands.
    pub continent_size_variety: f32,
}

/// Generate tectonic plates from planet parameters.
/// Returns a Vec of PlateGpu ready for GPU upload.
pub fn generate_plates(params: &PlateGenParams) -> Vec<PlateGpu> {
    let base_n = if params.num_plates_override > 0 {
        params.num_plates_override as usize
    } else {
        compute_plate_count(
            params.mass_earth,
            params.tectonics_factor,
            params.continental_scale,
        )
    };
    let continental_count = if params.num_continents > 0 {
        params.num_continents as usize
    } else {
        ((base_n as f32) * (1.0 - params.ocean_fraction)).round() as usize
    };
    // Ensure enough total plates: at least num_continents + 3 oceanic plates for ocean coverage
    let n = base_n.max(continental_count + 3);
    let centers = fibonacci_sphere(n, params.seed);
    let velocities = generate_angular_velocities(n, params.seed, params.tectonics_factor);

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

    // Apply continent size variety: cluster continental centers toward an attractor
    let mut final_centers = centers.clone();
    if params.continent_size_variety > 0.0 {
        // Seed-derived attractor point on sphere
        let ax = hash_f32(params.seed.wrapping_add(9999), 0, 0);
        let ay = hash_f32(params.seed.wrapping_add(9999), 0, 1);
        let az = hash_f32(params.seed.wrapping_add(9999), 0, 2);
        let alen = (ax * ax + ay * ay + az * az).sqrt().max(1e-6);
        let attractor = [ax / alen, ay / alen, az / alen];

        let strength = params.continent_size_variety * 0.6; // cap lerp to avoid full collapse
        for i in 0..n {
            if continental_indices[i] {
                // Lerp toward attractor, then re-normalize to sphere
                let cx = final_centers[i][0] * (1.0 - strength) + attractor[0] * strength;
                let cy = final_centers[i][1] * (1.0 - strength) + attractor[1] * strength;
                let cz = final_centers[i][2] * (1.0 - strength) + attractor[2] * strength;
                let clen = (cx * cx + cy * cy + cz * cz).sqrt().max(1e-6);
                final_centers[i] = [cx / clen, cy / clen, cz / clen];
            }
        }
    }

    let mut plates = Vec::with_capacity(n);
    for i in 0..n {
        plates.push(PlateGpu {
            center: final_centers[i],
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
        let phi = (1.0 - 2.0 * (i as f64 + 0.5) / n as f64).acos();

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

/// Generate seed-derived Euler angular velocities for each plate.
/// Local tangential velocity is derived in the shader as `cross(ω, position)`.
fn generate_angular_velocities(n: usize, seed: u32, tectonics_factor: f32) -> Vec<[f32; 3]> {
    if tectonics_factor == 0.0 {
        return vec![[0.0; 3]; n];
    }

    let mut velocities = Vec::with_capacity(n);
    let speed = tectonics_factor * 0.5;

    for i in 0..n {
        // Each plate gets a unique Euler rotation pole derived from its index + seed
        let pole_seed = seed.wrapping_add(1000);
        let px = hash_f32(pole_seed, i as u32, 0);
        let py = hash_f32(pole_seed, i as u32, 1);
        let pz = hash_f32(pole_seed, i as u32, 2);
        let pole_len = (px * px + py * py + pz * pz).sqrt().max(1e-6);
        let pole = [px / pole_len, py / pole_len, pz / pole_len];

        velocities.push([pole[0] * speed, pole[1] * speed, pole[2] * speed]);
    }
    velocities
}

/// Simple deterministic hash returning a float in [-1, 1].
fn hash_f32(seed: u32, index: u32, channel: u32) -> f32 {
    let mut h = seed
        .wrapping_mul(374761393)
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

    fn local_velocity(omega: [f32; 3], position: [f32; 3]) -> [f32; 3] {
        [
            omega[1] * position[2] - omega[2] * position[1],
            omega[2] * position[0] - omega[0] * position[2],
            omega[0] * position[1] - omega[1] * position[0],
        ]
    }

    fn magnitude(vector: [f32; 3]) -> f32 {
        (vector[0].powi(2) + vector[1].powi(2) + vector[2].powi(2)).sqrt()
    }

    fn boundary_classification_and_stress(
        velocity_a: [f32; 3],
        velocity_b: [f32; 3],
        boundary_normal: [f32; 3],
    ) -> (f32, f32) {
        let approach_speed = (velocity_a[0] - velocity_b[0]) * boundary_normal[0]
            + (velocity_a[1] - velocity_b[1]) * boundary_normal[1]
            + (velocity_a[2] - velocity_b[2]) * boundary_normal[2];
        (
            (-approach_speed * 5.0).clamp(-1.0, 1.0),
            (approach_speed * 3.0).clamp(0.0, 1.0),
        )
    }

    #[test]
    fn plate_gpu_layout_is_32_bytes() {
        assert_eq!(std::mem::size_of::<PlateGpu>(), 32);
    }

    #[test]
    fn earth_like_plate_count() {
        let params = PlateGenParams {
            seed: 42,
            mass_earth: 1.0,
            ocean_fraction: 0.7,
            tectonics_factor: 0.85,
            continental_scale: 1.0,
            num_plates_override: 0,
            num_continents: 0,
            continent_size_variety: 0.0,
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
            num_continents: 0,
            continent_size_variety: 0.0,
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
            num_continents: 0,
            continent_size_variety: 0.0,
        };
        let plates = generate_plates(&params);
        for (i, p) in plates.iter().enumerate() {
            let len = (p.center[0].powi(2) + p.center[1].powi(2) + p.center[2].powi(2)).sqrt();
            assert!(
                (len - 1.0).abs() < 0.01,
                "Plate {} center not on unit sphere: len={}",
                i,
                len
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
            num_continents: 0,
            continent_size_variety: 0.0,
        });
        let p2 = generate_plates(&PlateGenParams {
            seed: 999,
            mass_earth: 1.0,
            ocean_fraction: 0.7,
            tectonics_factor: 0.85,
            continental_scale: 1.0,
            num_plates_override: 0,
            num_continents: 0,
            continent_size_variety: 0.0,
        });
        let diff: f32 = p1
            .iter()
            .zip(p2.iter())
            .map(|(a, b)| {
                (a.center[0] - b.center[0]).abs()
                    + (a.center[1] - b.center[1]).abs()
                    + (a.center[2] - b.center[2]).abs()
            })
            .sum::<f32>()
            / p1.len() as f32;
        assert!(
            diff > 0.01,
            "Different seeds should produce different plates"
        );
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
            num_continents: 0,
            continent_size_variety: 0.0,
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
            num_continents: 0,
            continent_size_variety: 0.0,
        };
        let plates = generate_plates(&params);
        for (i, p) in plates.iter().enumerate() {
            let mag =
                (p.velocity[0].powi(2) + p.velocity[1].powi(2) + p.velocity[2].powi(2)).sqrt();
            assert!(
                mag > 0.001,
                "Plate {} velocity should be non-zero, got {}",
                i,
                mag
            );
        }
    }

    #[test]
    fn angular_velocities_are_finite_and_deterministic() {
        let params = PlateGenParams {
            seed: 42,
            mass_earth: 1.0,
            ocean_fraction: 0.7,
            tectonics_factor: 0.85,
            continental_scale: 1.0,
            num_plates_override: 8,
            num_continents: 0,
            continent_size_variety: 0.5,
        };
        let first = generate_plates(&params);
        let second = generate_plates(&params);

        for (first, second) in first.iter().zip(second.iter()) {
            assert!(first.velocity.iter().all(|component| component.is_finite()));
            assert_eq!(
                first.velocity.map(f32::to_bits),
                second.velocity.map(f32::to_bits)
            );
        }
    }

    #[test]
    fn zero_tectonics_produces_zero_angular_velocity() {
        let plates = generate_plates(&PlateGenParams {
            seed: 42,
            mass_earth: 1.0,
            ocean_fraction: 0.7,
            tectonics_factor: 0.0,
            continental_scale: 1.0,
            num_plates_override: 8,
            num_continents: 0,
            continent_size_variety: 0.5,
        });

        assert!(plates.iter().all(|plate| plate.velocity == [0.0, 0.0, 0.0]));
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
            num_continents: 0,
            continent_size_variety: 0.0,
        });
        let plates_high = generate_plates(&PlateGenParams {
            seed: 42,
            mass_earth: 1.0,
            ocean_fraction: 0.7,
            tectonics_factor: 1.0,
            continental_scale: 1.0,
            num_plates_override: 0,
            num_continents: 0,
            continent_size_variety: 0.0,
        });
        let avg_low: f32 = plates_low
            .iter()
            .map(|p| (p.velocity[0].powi(2) + p.velocity[1].powi(2) + p.velocity[2].powi(2)).sqrt())
            .sum::<f32>()
            / plates_low.len() as f32;
        let avg_high: f32 = plates_high
            .iter()
            .map(|p| (p.velocity[0].powi(2) + p.velocity[1].powi(2) + p.velocity[2].powi(2)).sqrt())
            .sum::<f32>()
            / plates_high.len() as f32;
        assert!(
            avg_high > avg_low * 2.0,
            "High tectonics_factor should produce faster plates: low={:.4}, high={:.4}",
            avg_low,
            avg_high
        );
    }

    #[test]
    fn local_euler_velocities_are_tangent_to_sphere() {
        let params = PlateGenParams {
            seed: 42,
            mass_earth: 1.0,
            ocean_fraction: 0.7,
            tectonics_factor: 0.85,
            continental_scale: 1.0,
            num_plates_override: 8,
            num_continents: 0,
            continent_size_variety: 0.0,
        };
        let plates = generate_plates(&params);
        for (i, p) in plates.iter().enumerate() {
            let velocity = local_velocity(p.velocity, p.center);
            let dot =
                velocity[0] * p.center[0] + velocity[1] * p.center[1] + velocity[2] * p.center[2];
            assert!(
                dot.abs() < 1e-6,
                "Plate {} local velocity not tangent to sphere: dot = {:.4}",
                i,
                dot
            );
        }
    }

    #[test]
    fn euler_pole_has_near_zero_local_velocity() {
        let omega = [0.3, -0.4, 0.0];
        let omega_magnitude = magnitude(omega);
        let pole = [
            omega[0] / omega_magnitude,
            omega[1] / omega_magnitude,
            omega[2] / omega_magnitude,
        ];

        assert!(magnitude(local_velocity(omega, pole)) < 1e-6);
    }

    #[test]
    fn euler_velocity_matches_90_degree_magnitude() {
        let omega = [0.0, 0.0, 0.5];
        let equatorial_position = [1.0, 0.0, 0.0];

        assert!(
            (magnitude(local_velocity(omega, equatorial_position)) - magnitude(omega)).abs() < 1e-6
        );
    }

    #[test]
    fn local_motion_classifies_convergence_and_stress_deterministically() {
        let position = [0.0, 0.0, 1.0];
        let boundary_normal_a_to_b = [1.0, 0.0, 0.0];
        let convergent_a = local_velocity([0.0, 1.0, 0.0], position);
        let convergent_b = local_velocity([0.0, -1.0, 0.0], position);
        let divergent_a = local_velocity([0.0, -1.0, 0.0], position);
        let divergent_b = local_velocity([0.0, 1.0, 0.0], position);

        let convergent =
            boundary_classification_and_stress(convergent_a, convergent_b, boundary_normal_a_to_b);
        let divergent =
            boundary_classification_and_stress(divergent_a, divergent_b, boundary_normal_a_to_b);

        assert!(
            convergent.0 < 0.0,
            "convergent boundary must classify negative"
        );
        assert!(
            convergent.1 > 0.0,
            "convergent boundary must have positive stress"
        );
        assert!(
            divergent.0 > 0.0,
            "divergent boundary must classify positive"
        );
        assert_eq!(divergent.1, 0.0, "divergent boundary must have zero stress");
    }

    #[test]
    fn num_continents_controls_continental_count() {
        for nc in 1..=8u32 {
            let plates = generate_plates(&PlateGenParams {
                seed: 42,
                mass_earth: 1.0,
                ocean_fraction: 0.7,
                tectonics_factor: 0.85,
                continental_scale: 1.0,
                num_plates_override: 0,
                num_continents: nc,
                continent_size_variety: 0.0,
            });
            let continental = plates.iter().filter(|p| p.plate_type > 0.5).count();
            assert_eq!(
                continental, nc as usize,
                "Expected {} continental plates, got {}",
                nc, continental
            );
        }
    }

    #[test]
    fn continent_size_variety_shifts_centers() {
        let plates_equal = generate_plates(&PlateGenParams {
            seed: 42,
            mass_earth: 1.0,
            ocean_fraction: 0.7,
            tectonics_factor: 0.85,
            continental_scale: 1.0,
            num_plates_override: 0,
            num_continents: 4,
            continent_size_variety: 0.0,
        });
        let plates_clustered = generate_plates(&PlateGenParams {
            seed: 42,
            mass_earth: 1.0,
            ocean_fraction: 0.7,
            tectonics_factor: 0.85,
            continental_scale: 1.0,
            num_plates_override: 0,
            num_continents: 4,
            continent_size_variety: 1.0,
        });

        // Continental plate centers should differ when variety changes
        let cont_equal: Vec<_> = plates_equal
            .iter()
            .filter(|p| p.plate_type > 0.5)
            .map(|p| p.center)
            .collect();
        let cont_clustered: Vec<_> = plates_clustered
            .iter()
            .filter(|p| p.plate_type > 0.5)
            .map(|p| p.center)
            .collect();

        assert_eq!(cont_equal.len(), cont_clustered.len());
        let mut any_different = false;
        for (a, b) in cont_equal.iter().zip(cont_clustered.iter()) {
            let dist =
                ((a[0] - b[0]).powi(2) + (a[1] - b[1]).powi(2) + (a[2] - b[2]).powi(2)).sqrt();
            if dist > 0.01 {
                any_different = true;
            }
        }
        assert!(
            any_different,
            "Variety=1 should shift continental centers relative to variety=0"
        );
    }
}
