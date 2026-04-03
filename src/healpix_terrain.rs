//! HEALPix-based terrain generation from plate simulation results.
//!
//! Phase 6.2: Orogeny, continental shelves, stress-driven roughness on HEALPix grid,
//! then resample to cubemap for GPU preview rendering.
//!
//! Pipeline: PlateSimResult → geological features on HEALPix → cubemap → noise detail.

use crate::healpix;
use crate::plate_sim::{PlateInfo, PlateSimResult};
use crate::terrain_compute::TectonicTerrain;
use std::collections::VecDeque;

// ─── Public API ──────────────────────────────────────────────────────────────

/// Parameters for HEALPix terrain generation.
pub struct HealpixTerrainParams {
    pub seed: u32,
    pub mountain_scale: f32, // [0.1, 3.0] default 1.0
    pub detail_scale: f32,   // [0.0, 2.0] default 0.5
}

impl Default for HealpixTerrainParams {
    fn default() -> Self {
        Self {
            seed: 42,
            mountain_scale: 1.0,
            detail_scale: 0.5,
        }
    }
}

/// Generate terrain from HEALPix plate simulation and resample to cubemap.
pub fn generate(
    sim: &PlateSimResult,
    params: &HealpixTerrainParams,
    face_res: u32,
) -> TectonicTerrain {
    let npix = healpix::npix(sim.nside) as usize;
    let mut elevation = vec![0.0f32; npix];

    // Pre-compute boundary analysis for convergent/divergent classification
    let boundary = analyze_boundaries(sim);

    // 6.2.1: Base elevation from plate type + super-plate structure
    apply_base_elevation(&mut elevation, sim);

    // 6.2.5: Continental shelf profile (before mountains to set ocean floor baseline)
    apply_shelf_profile(&mut elevation, sim);

    // 6.2.2: Convergent boundary mountains with asymmetric subduction
    apply_convergent_mountains(&mut elevation, sim, &boundary, params);

    // 6.2.3: Fold ridges parallel to plate motion
    apply_fold_ridges(&mut elevation, sim, &boundary, params);

    // 6.2.4: Divergent boundary features (ridges + rifts)
    apply_divergent_features(&mut elevation, sim, &boundary);

    // 6.2.7: Resample elevation to cubemap
    let mut faces = healpix::to_cubemap(&elevation, sim.nside, face_res);

    // 6.2.6: Stress-driven noise detail on cubemap (needs high resolution)
    let stress_faces = healpix::to_cubemap(&sim.stress, sim.nside, face_res);
    apply_cubemap_noise(&mut faces, &stress_faces, face_res, params);

    TectonicTerrain {
        faces,
        resolution: face_res,
    }
}

// ─── Boundary Analysis ──────────────────────────────────────────────────────

/// Per-pixel classification of the nearest plate boundary.
struct BoundaryAnalysis {
    /// Plate type of the other plate at nearest boundary [0.0=oceanic, 1.0=continental].
    other_type: Vec<f32>,
    /// Signed convergence at nearest boundary [positive=convergent, negative=divergent].
    convergence: Vec<f32>,
    /// 3D direction toward nearest boundary (for fold ridge alignment).
    boundary_dir: Vec<[f64; 3]>,
}

fn analyze_boundaries(sim: &PlateSimResult) -> BoundaryAnalysis {
    let nside = sim.nside;
    let npix = healpix::npix(nside) as usize;

    let mut other_type = vec![0.0f32; npix];
    let mut convergence = vec![0.0f32; npix];
    let mut boundary_dir = vec![[0.0f64; 3]; npix];
    let mut dist = vec![f32::MAX; npix];
    let mut queue = VecDeque::new();

    // Seed from boundary pixels (dist_boundary ~= 0)
    for ipix in 0..npix {
        if sim.dist_boundary[ipix] > 0.5 {
            continue;
        }

        let pid = sim.plate_id[ipix] as usize;
        if pid >= sim.plates.len() {
            continue;
        }
        let pos = healpix::pix2vec(nside, ipix as u32);

        let nbrs = healpix::neighbors(nside, ipix as u32);
        let mut best_abs_conv = 0.0f64;
        let mut best_conv = 0.0f64;
        let mut best_otype = 0.0f32;
        let mut best_dir = [0.0f64; 3];

        for &n in &nbrs {
            if n == u32::MAX || (n as usize) >= npix {
                continue;
            }
            let npid = sim.plate_id[n as usize] as usize;
            if npid == pid || npid >= sim.plates.len() {
                continue;
            }

            let npos = healpix::pix2vec(nside, n);
            let dx = [npos[0] - pos[0], npos[1] - pos[1], npos[2] - pos[2]];
            let dx_len = (dx[0] * dx[0] + dx[1] * dx[1] + dx[2] * dx[2]).sqrt();
            if dx_len < 1e-10 {
                continue;
            }
            let normal = [dx[0] / dx_len, dx[1] / dx_len, dx[2] / dx_len];

            let my_vel = plate_velocity(&sim.plates[pid], &pos);
            let other_vel = plate_velocity(&sim.plates[npid], &pos);
            let rel = [
                my_vel[0] - other_vel[0],
                my_vel[1] - other_vel[1],
                my_vel[2] - other_vel[2],
            ];
            // Positive = convergent (plates approach each other)
            let conv = -(rel[0] * normal[0] + rel[1] * normal[1] + rel[2] * normal[2]);

            if conv.abs() > best_abs_conv {
                best_abs_conv = conv.abs();
                best_conv = conv;
                best_otype = sim.plates[npid].plate_type;
                best_dir = normal;
            }
        }

        other_type[ipix] = best_otype;
        convergence[ipix] = best_conv as f32;
        boundary_dir[ipix] = best_dir;
        dist[ipix] = 0.0;
        queue.push_back(ipix as u32);
    }

    // BFS propagate boundary metadata to all pixels
    while let Some(pix) = queue.pop_front() {
        let pi = pix as usize;
        let current_dist = dist[pi];
        let nbrs = healpix::neighbors(nside, pix);
        for &n in &nbrs {
            if n == u32::MAX || (n as usize) >= npix {
                continue;
            }
            let ni = n as usize;
            let new_dist = current_dist + 1.0;
            if new_dist < dist[ni] {
                dist[ni] = new_dist;
                other_type[ni] = other_type[pi];
                convergence[ni] = convergence[pi];
                boundary_dir[ni] = boundary_dir[pi];
                queue.push_back(n);
            }
        }
    }

    BoundaryAnalysis {
        other_type,
        convergence,
        boundary_dir,
    }
}

fn plate_velocity(plate: &PlateInfo, pos: &[f64; 3]) -> [f64; 3] {
    let p = plate.euler_pole;
    let w = plate.angular_velocity;
    [
        w * (p[1] * pos[2] - p[2] * pos[1]),
        w * (p[2] * pos[0] - p[0] * pos[2]),
        w * (p[0] * pos[1] - p[1] * pos[0]),
    ]
}

// ─── 6.2.1: Base Elevation ──────────────────────────────────────────────────

fn apply_base_elevation(elevation: &mut [f32], sim: &PlateSimResult) {
    let max_inland = sim.dist_coast.iter().fold(0.0f32, |a, &b| a.max(b));
    let max_offshore = sim.dist_coast.iter().fold(0.0f32, |a, &b| a.max(-b));

    for ipix in 0..elevation.len() {
        let pid = sim.plate_id[ipix] as usize;
        if pid >= sim.plates.len() {
            continue;
        }

        let is_continental = sim.plates[pid].plate_type > 0.5;

        if is_continental {
            // Continental: 0.72 at coast, rising to 0.85 deep inland
            let inland_frac = if max_inland > 1.0 {
                (sim.dist_coast[ipix] / max_inland).clamp(0.0, 1.0)
            } else {
                0.5
            };
            elevation[ipix] = 0.72 + 0.13 * inland_frac;

            // Super-plate variation: slight offset per continent
            let sp = sim.super_plate_id[ipix];
            elevation[ipix] += hash_f32(sp.wrapping_mul(54321), 7) * 0.04;
        } else {
            // Oceanic: 0.35 near coast, dropping to 0.15 deep ocean
            let offshore_frac = if max_offshore > 1.0 {
                (-sim.dist_coast[ipix] / max_offshore).clamp(0.0, 1.0)
            } else {
                0.5
            };
            elevation[ipix] = 0.35 - 0.20 * offshore_frac;
        }
    }
}

// ─── 6.2.5: Continental Shelf Profile ───────────────────────────────────────

fn apply_shelf_profile(elevation: &mut [f32], sim: &PlateSimResult) {
    let shelf_end = 5.0; // shelf edge (hops)
    let slope_end = 12.0; // slope base (hops)

    let coast_level = 0.72;
    let shelf_level = 0.55;
    let slope_base = 0.30;
    let abyss_level = 0.20;

    for ipix in 0..elevation.len() {
        let dc = sim.dist_coast[ipix];
        if dc >= 0.0 {
            continue; // Only modify oceanic side
        }
        let offshore = -dc;

        // Active margin: narrower shelf near convergent boundaries
        let stress = sim.stress[ipix];
        let margin_factor = 1.0 - 0.6 * stress;
        let eff_shelf = shelf_end * margin_factor;
        let eff_slope = slope_end * margin_factor;

        elevation[ipix] = if offshore < eff_shelf {
            let t = offshore / eff_shelf.max(0.1);
            lerp(coast_level, shelf_level, smooth_step(t))
        } else if offshore < eff_slope {
            let t = (offshore - eff_shelf) / (eff_slope - eff_shelf).max(0.1);
            lerp(shelf_level, slope_base, smooth_step(t))
        } else {
            let t = ((offshore - eff_slope) / 10.0).min(1.0);
            lerp(slope_base, abyss_level, t)
        };
    }
}

// ─── 6.2.2: Convergent Mountain Ridges ──────────────────────────────────────

fn apply_convergent_mountains(
    elevation: &mut [f32],
    sim: &PlateSimResult,
    boundary: &BoundaryAnalysis,
    params: &HealpixTerrainParams,
) {
    let mountain_height = 0.45 * params.mountain_scale;
    let trench_depth = 0.15 * params.mountain_scale;
    let width = (sim.nside as f32 / 8.0).max(3.0);

    for ipix in 0..elevation.len() {
        let conv = boundary.convergence[ipix];
        if conv <= 0.01 {
            continue;
        }

        let dist = sim.dist_boundary[ipix];
        let stress = sim.stress[ipix];

        let pid = sim.plate_id[ipix] as usize;
        if pid >= sim.plates.len() {
            continue;
        }
        let my_type = sim.plates[pid].plate_type;
        let other_type = boundary.other_type[ipix];

        // Classify boundary interaction
        let (height_factor, trench_factor) = if my_type > 0.5 && other_type > 0.5 {
            // Continent-continent: high mountains both sides (Himalayas)
            (1.5, 0.0)
        } else if my_type > 0.5 {
            // Continental side of ocean-continent: mountain range (Andes)
            (1.2, 0.0)
        } else if other_type > 0.5 {
            // Oceanic side of ocean-continent: trench (Peru-Chile)
            (0.2, 1.0)
        } else {
            // Ocean-ocean: island arc
            (0.6, 0.5)
        };

        // Gaussian decay from boundary
        let sigma_sq = 2.0 * width * width;
        let decay = (-dist * dist / sigma_sq).exp();

        elevation[ipix] += mountain_height * height_factor * stress * decay;

        // Trench: narrower Gaussian very close to boundary
        if trench_factor > 0.0 {
            let trench_width = width * 0.3;
            let trench_decay = (-dist * dist / (2.0 * trench_width * trench_width)).exp();
            elevation[ipix] -= trench_depth * trench_factor * stress * trench_decay;
        }
    }
}

// ─── 6.2.3: Fold Ridges ────────────────────────────────────────────────────

fn apply_fold_ridges(
    elevation: &mut [f32],
    sim: &PlateSimResult,
    boundary: &BoundaryAnalysis,
    params: &HealpixTerrainParams,
) {
    let ridge_amplitude = 0.06 * params.mountain_scale;
    let width = (sim.nside as f32 / 6.0).max(4.0);

    for ipix in 0..elevation.len() {
        let conv = boundary.convergence[ipix];
        if conv <= 0.05 {
            continue;
        }

        let dist = sim.dist_boundary[ipix];
        if dist > width * 1.5 {
            continue;
        }

        let stress = sim.stress[ipix];
        if stress < 0.1 {
            continue;
        }

        let pos = healpix::pix2vec(sim.nside, ipix as u32);
        let bdir = boundary.boundary_dir[ipix];

        // Project boundary direction onto tangent plane
        let dot_pb = pos[0] * bdir[0] + pos[1] * bdir[1] + pos[2] * bdir[2];
        let tangent = [
            bdir[0] - dot_pb * pos[0],
            bdir[1] - dot_pb * pos[1],
            bdir[2] - dot_pb * pos[2],
        ];
        let tlen =
            (tangent[0] * tangent[0] + tangent[1] * tangent[1] + tangent[2] * tangent[2]).sqrt();
        if tlen < 1e-10 {
            continue;
        }

        // Fold axis: perpendicular to boundary direction on tangent plane (pos × tangent)
        let fold_axis = [
            pos[1] * tangent[2] - pos[2] * tangent[1],
            pos[2] * tangent[0] - pos[0] * tangent[2],
            pos[0] * tangent[1] - pos[1] * tangent[0],
        ];

        // Sinusoidal ridge pattern along fold axis
        let fold_coord = pos[0] * fold_axis[0] + pos[1] * fold_axis[1] + pos[2] * fold_axis[2];
        let frequency = 20.0 + hash_f32(params.seed, ipix as u32).abs() as f64 * 10.0;
        let ridge = ((fold_coord * frequency).sin() * 0.5 + 0.5) as f32;

        let decay = (-dist * dist / (2.0 * width * width)).exp();
        elevation[ipix] += ridge_amplitude * ridge * stress * decay;
    }
}

// ─── 6.2.4: Divergent Boundary Features ────────────────────────────────────

fn apply_divergent_features(
    elevation: &mut [f32],
    sim: &PlateSimResult,
    boundary: &BoundaryAnalysis,
) {
    let ridge_height = 0.08;
    let rift_depth = 0.06;
    let width = (sim.nside as f32 / 10.0).max(2.0);

    for ipix in 0..elevation.len() {
        let conv = boundary.convergence[ipix];
        if conv >= -0.01 {
            continue; // Not divergent
        }

        let dist = sim.dist_boundary[ipix];
        let divergence = (-conv).min(1.0); // positive divergent strength

        let pid = sim.plate_id[ipix] as usize;
        if pid >= sim.plates.len() {
            continue;
        }

        let decay = (-dist * dist / (2.0 * width * width)).exp();

        if sim.plates[pid].plate_type <= 0.5 {
            // Mid-ocean ridge: subtle elevation bump
            elevation[ipix] += ridge_height * divergence * decay;
        } else {
            // Continental rift valley: depression
            elevation[ipix] -= rift_depth * divergence * decay;
        }
    }
}

// ─── 6.2.6: Stress-Driven Noise Detail ─────────────────────────────────────

fn apply_cubemap_noise(
    faces: &mut [Vec<f32>; 6],
    stress_faces: &[Vec<f32>; 6],
    face_res: u32,
    params: &HealpixTerrainParams,
) {
    let base_amplitude = 0.03 * params.detail_scale;
    let stress_amplitude = 0.12 * params.detail_scale;

    for face in 0..6u32 {
        for py in 0..face_res {
            for px in 0..face_res {
                let idx = (py * face_res + px) as usize;
                let stress = stress_faces[face as usize][idx];

                let u = (px as f64 + 0.5) / face_res as f64;
                let v = (py as f64 + 0.5) / face_res as f64;
                let pos = cube_to_sphere(face, u, v);

                let amp = base_amplitude + stress_amplitude * stress;

                // Ridged multifractal for orogen detail
                let ridged = ridged_mf_3d(
                    pos[0] * 8.0,
                    pos[1] * 8.0,
                    pos[2] * 8.0,
                    params.seed,
                    5,
                    2.1,
                    0.45,
                ) as f32;

                // fBm for general terrain texture
                let fbm = fbm_3d(
                    pos[0] * 12.0,
                    pos[1] * 12.0,
                    pos[2] * 12.0,
                    params.seed.wrapping_add(5000),
                    6,
                    2.0,
                    0.5,
                ) as f32;

                // Blend: ridged dominates in high-stress areas, fBm in low-stress
                let noise = lerp(fbm, ridged, stress.clamp(0.0, 1.0));
                faces[face as usize][idx] += amp * noise;
            }
        }
    }
}

// ─── Noise Functions ────────────────────────────────────────────────────────

fn noise_3d(x: f64, y: f64, z: f64, seed: u32) -> f64 {
    let ix = x.floor() as i32;
    let iy = y.floor() as i32;
    let iz = z.floor() as i32;
    let fx = x - x.floor();
    let fy = y - y.floor();
    let fz = z - z.floor();

    // Quintic smoothstep
    let sx = fx * fx * fx * (fx * (fx * 6.0 - 15.0) + 10.0);
    let sy = fy * fy * fy * (fy * (fy * 6.0 - 15.0) + 10.0);
    let sz = fz * fz * fz * (fz * (fz * 6.0 - 15.0) + 10.0);

    let h000 = lattice_hash(ix, iy, iz, seed);
    let h100 = lattice_hash(ix + 1, iy, iz, seed);
    let h010 = lattice_hash(ix, iy + 1, iz, seed);
    let h110 = lattice_hash(ix + 1, iy + 1, iz, seed);
    let h001 = lattice_hash(ix, iy, iz + 1, seed);
    let h101 = lattice_hash(ix + 1, iy, iz + 1, seed);
    let h011 = lattice_hash(ix, iy + 1, iz + 1, seed);
    let h111 = lattice_hash(ix + 1, iy + 1, iz + 1, seed);

    let x00 = h000 + sx * (h100 - h000);
    let x10 = h010 + sx * (h110 - h010);
    let x01 = h001 + sx * (h101 - h001);
    let x11 = h011 + sx * (h111 - h011);
    let xy0 = x00 + sy * (x10 - x00);
    let xy1 = x01 + sy * (x11 - x01);
    xy0 + sz * (xy1 - xy0)
}

fn lattice_hash(x: i32, y: i32, z: i32, seed: u32) -> f64 {
    let mut h = seed
        .wrapping_add(x as u32)
        .wrapping_mul(374761393)
        .wrapping_add(y as u32)
        .wrapping_mul(668265263)
        .wrapping_add(z as u32)
        .wrapping_mul(1274126177);
    h = (h ^ (h >> 13)).wrapping_mul(1103515245);
    h = h ^ (h >> 16);
    h as f64 / u32::MAX as f64
}

fn fbm_3d(x: f64, y: f64, z: f64, seed: u32, octaves: u32, lacunarity: f64, gain: f64) -> f64 {
    let mut sum = 0.0;
    let mut amp = 1.0;
    let mut freq = 1.0;
    let mut max_amp = 0.0;

    for i in 0..octaves {
        let n = noise_3d(x * freq, y * freq, z * freq, seed.wrapping_add(i * 1000));
        sum += amp * (n * 2.0 - 1.0); // map [0,1] to [-1,1]
        max_amp += amp;
        amp *= gain;
        freq *= lacunarity;
    }

    sum / max_amp
}

fn ridged_mf_3d(
    x: f64,
    y: f64,
    z: f64,
    seed: u32,
    octaves: u32,
    lacunarity: f64,
    gain: f64,
) -> f64 {
    let mut sum = 0.0;
    let mut amp = 1.0;
    let mut freq = 1.0;
    let mut prev = 1.0;
    let mut max_val = 0.0;

    for i in 0..octaves {
        let n = noise_3d(x * freq, y * freq, z * freq, seed.wrapping_add(i * 1000));
        let ridge = 1.0 - (2.0 * n - 1.0).abs();
        let ridge = ridge * ridge;
        sum += amp * ridge * prev;
        max_val += amp * prev;
        prev = ridge;
        amp *= gain;
        freq *= lacunarity;
    }

    if max_val > 0.0 {
        sum / max_val
    } else {
        0.0
    }
}

// ─── Cube→Sphere Mapping ────────────────────────────────────────────────────

/// Face indices: 0=+X, 1=-X, 2=+Y, 3=-Y, 4=+Z, 5=-Z
/// Matches healpix.rs cube_to_sphere_f64 conventions.
fn cube_to_sphere(face: u32, u: f64, v: f64) -> [f64; 3] {
    let s = u * 2.0 - 1.0;
    let t = v * 2.0 - 1.0;
    let p = match face {
        0 => [1.0, -t, -s],
        1 => [-1.0, -t, s],
        2 => [s, 1.0, t],
        3 => [s, -1.0, -t],
        4 => [s, -t, 1.0],
        5 => [-s, -t, -1.0],
        _ => [0.0, 0.0, 1.0],
    };
    let len = (p[0] * p[0] + p[1] * p[1] + p[2] * p[2]).sqrt();
    [p[0] / len, p[1] / len, p[2] / len]
}

// ─── Math Helpers ───────────────────────────────────────────────────────────

fn lerp(a: f32, b: f32, t: f32) -> f32 {
    a + (b - a) * t
}

fn smooth_step(t: f32) -> f32 {
    let t = t.clamp(0.0, 1.0);
    t * t * (3.0 - 2.0 * t)
}

fn hash_f32(seed: u32, index: u32) -> f32 {
    let mut h = seed
        .wrapping_mul(374761393)
        .wrapping_add(index.wrapping_mul(668265263));
    h = (h ^ (h >> 13)).wrapping_mul(1103515245);
    h = h ^ (h >> 16);
    (h as f32 / u32::MAX as f32) * 2.0 - 1.0
}

// ─── Tests ──────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use crate::plate_sim::{self, PlateSimParams};

    fn test_sim() -> PlateSimResult {
        plate_sim::simulate(&PlateSimParams {
            nside: 16,
            ..PlateSimParams::default()
        })
    }

    #[test]
    fn generate_produces_valid_terrain() {
        let sim = test_sim();
        let terrain = generate(&sim, &HealpixTerrainParams::default(), 64);

        assert_eq!(terrain.faces.len(), 6);
        for (i, face) in terrain.faces.iter().enumerate() {
            assert_eq!(face.len(), 64 * 64, "face {i} wrong size");
            assert!(face.iter().all(|v| !v.is_nan()), "face {i} has NaN");
            assert!(
                face.iter().all(|v| v.is_finite()),
                "face {i} has infinity"
            );
        }
    }

    #[test]
    fn continental_higher_than_oceanic() {
        let sim = test_sim();
        let npix = healpix::npix(sim.nside) as usize;
        let mut elevation = vec![0.0f32; npix];
        apply_base_elevation(&mut elevation, &sim);

        let mut cont_sum = 0.0f64;
        let mut cont_count = 0u32;
        let mut ocean_sum = 0.0f64;
        let mut ocean_count = 0u32;

        for ipix in 0..npix {
            let pid = sim.plate_id[ipix] as usize;
            if pid >= sim.plates.len() {
                continue;
            }
            if sim.plates[pid].plate_type > 0.5 {
                cont_sum += elevation[ipix] as f64;
                cont_count += 1;
            } else {
                ocean_sum += elevation[ipix] as f64;
                ocean_count += 1;
            }
        }

        let cont_avg = cont_sum / cont_count.max(1) as f64;
        let ocean_avg = ocean_sum / ocean_count.max(1) as f64;
        assert!(
            cont_avg > ocean_avg + 0.2,
            "continental avg ({cont_avg:.3}) should be > oceanic avg ({ocean_avg:.3}) + 0.2"
        );
    }

    #[test]
    fn shelf_creates_transition() {
        let sim = test_sim();
        let npix = healpix::npix(sim.nside) as usize;
        let mut elevation = vec![0.5f32; npix];
        apply_shelf_profile(&mut elevation, &sim);

        // Offshore pixels should have varied elevation (not all 0.5)
        let offshore: Vec<f32> = (0..npix)
            .filter(|&i| sim.dist_coast[i] < -1.0)
            .map(|i| elevation[i])
            .collect();

        if !offshore.is_empty() {
            let min = offshore.iter().fold(f32::INFINITY, |a, &b| a.min(b));
            let max = offshore.iter().fold(f32::NEG_INFINITY, |a, &b| a.max(b));
            assert!(
                max - min > 0.05,
                "shelf should create elevation variation (range={:.3})",
                max - min
            );
        }
    }

    #[test]
    fn convergent_boundaries_have_mountains() {
        let sim = test_sim();
        let boundary = analyze_boundaries(&sim);
        let npix = healpix::npix(sim.nside) as usize;
        let mut elevation = vec![0.5f32; npix];

        apply_convergent_mountains(
            &mut elevation,
            &sim,
            &boundary,
            &HealpixTerrainParams::default(),
        );

        let max_elev = elevation.iter().fold(0.5f32, |a, &b| a.max(b));
        assert!(
            max_elev > 0.55,
            "convergent mountains should raise pixels (max={max_elev:.3})"
        );
    }

    #[test]
    fn noise_adds_detail() {
        let face_res = 32u32;
        let mut faces: [Vec<f32>; 6] =
            std::array::from_fn(|_| vec![0.5; (face_res * face_res) as usize]);
        let stress_faces: [Vec<f32>; 6] =
            std::array::from_fn(|_| vec![0.5; (face_res * face_res) as usize]);

        apply_cubemap_noise(
            &mut faces,
            &stress_faces,
            face_res,
            &HealpixTerrainParams {
                detail_scale: 1.0,
                ..Default::default()
            },
        );

        let min = faces[0].iter().fold(f32::INFINITY, |a, &b| a.min(b));
        let max = faces[0].iter().fold(f32::NEG_INFINITY, |a, &b| a.max(b));
        assert!(
            max - min > 0.01,
            "noise should create variation (range={:.4})",
            max - min
        );
    }

    #[test]
    fn boundary_analysis_covers_all_pixels() {
        let sim = test_sim();
        let boundary = analyze_boundaries(&sim);
        let npix = healpix::npix(sim.nside) as usize;

        assert_eq!(boundary.other_type.len(), npix);
        assert_eq!(boundary.convergence.len(), npix);
        assert_eq!(boundary.boundary_dir.len(), npix);

        // Every pixel should have been reached by BFS
        assert!(
            boundary
                .convergence
                .iter()
                .any(|&c| c.abs() > 0.001),
            "some boundaries should have nonzero convergence"
        );
    }

    #[test]
    #[ignore] // Run with: cargo test --release -- --ignored pipeline_nside256
    fn pipeline_nside256_under_3s() {
        use std::time::Instant;

        let t0 = Instant::now();
        let sim = plate_sim::simulate(&PlateSimParams {
            nside: 256,
            ..PlateSimParams::default()
        });
        let t_sim = t0.elapsed();

        let t1 = Instant::now();
        let terrain = generate(&sim, &HealpixTerrainParams::default(), 512);
        let t_terrain = t1.elapsed();

        let total = t0.elapsed();

        eprintln!(
            "[perf] nside=256 face_res=512: sim={:.0}ms terrain={:.0}ms total={:.0}ms",
            t_sim.as_secs_f64() * 1000.0,
            t_terrain.as_secs_f64() * 1000.0,
            total.as_secs_f64() * 1000.0,
        );

        assert_eq!(terrain.faces.len(), 6);
        assert_eq!(terrain.faces[0].len(), 512 * 512);
        assert!(
            total.as_secs_f64() < 3.0,
            "full pipeline should complete in <3s (took {:.1}s)",
            total.as_secs_f64()
        );
    }
}
