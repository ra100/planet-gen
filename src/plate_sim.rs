//! HEALPix-based plate tectonics simulation.
//!
//! Implements the full plate simulation pipeline on a HEALPix sphere grid:
//! 1. Seed plates from Fibonacci sphere points
//! 2. BFS flood-fill with noise perturbation for organic boundaries
//! 3. Distance fields (boundary + coast) via multi-source BFS
//! 4. Super-plate clustering for continent-scale structure
//! 5. Collision stress from relative plate velocities

use crate::healpix;
use std::collections::VecDeque;

// ─── Parameters & Results ─────────────────────────────────────────────────────

/// Parameters for HEALPix plate simulation.
pub struct PlateSimParams {
    pub nside: u32,
    pub seed: u32,
    pub num_plates: u32,
    pub ocean_fraction: f32,
    pub num_continents: u32,
    pub continent_size_variety: f32,
    pub tectonics_factor: f32,
}

impl Default for PlateSimParams {
    fn default() -> Self {
        Self {
            nside: 64,
            seed: 42,
            num_plates: 14,
            ocean_fraction: 0.7,
            num_continents: 4,
            continent_size_variety: 0.35,
            tectonics_factor: 0.85,
        }
    }
}

/// Per-plate data generated during simulation.
pub struct PlateInfo {
    pub center: [f64; 3],
    pub plate_type: f32,     // 1.0 = continental, 0.0 = oceanic
    pub euler_pole: [f64; 3],
    pub angular_velocity: f64,
    pub area: u32,           // pixel count
}

/// Complete plate simulation result.
pub struct PlateSimResult {
    pub nside: u32,
    pub plate_id: Vec<u32>,         // per-pixel plate index
    pub plates: Vec<PlateInfo>,     // per-plate metadata
    pub dist_boundary: Vec<f32>,    // per-pixel distance to nearest plate boundary (hops)
    pub dist_coast: Vec<f32>,       // per-pixel signed distance to coast (+inland, -offshore)
    pub super_plate_id: Vec<u32>,   // per-pixel super-plate index
    pub stress: Vec<f32>,           // per-pixel collision stress [0, 1]
}

// ─── Main entry point ─────────────────────────────────────────────────────────

/// Run the full plate simulation pipeline.
pub fn simulate(params: &PlateSimParams) -> PlateSimResult {
    let npix = healpix::npix(params.nside) as usize;

    // 6.1.1: Seed plates
    let (seeds, mut plates) = generate_plate_seeds(params);

    // 6.1.2: BFS flood-fill
    let plate_id = assign_plates_bfs(params.nside, &seeds, params.seed, npix);

    // Update plate areas and centroids
    update_plate_stats(params.nside, &plate_id, &mut plates);

    // 6.1.3: Distance to boundary
    let dist_boundary = compute_boundary_distance(params.nside, &plate_id, npix);

    // 6.1.4: Distance to coast
    let dist_coast = compute_coast_distance(params.nside, &plate_id, &plates, npix);

    // 6.1.5: Super-plate clustering
    let super_plate_id = cluster_super_plates(
        params.nside,
        &plate_id,
        &plates,
        params.num_continents,
        params.continent_size_variety,
        params.seed,
        npix,
    );

    // 6.1.6: Stress computation
    let stress = compute_stress(params.nside, &plate_id, &plates, &dist_boundary, npix);

    PlateSimResult {
        nside: params.nside,
        plate_id,
        plates,
        dist_boundary,
        dist_coast,
        super_plate_id,
        stress,
    }
}

// ─── 6.1.1: Plate Seed Generation ────────────────────────────────────────────

fn generate_plate_seeds(params: &PlateSimParams) -> (Vec<u32>, Vec<PlateInfo>) {
    let n = params.num_plates as usize;
    let centers = fibonacci_sphere(n, params.seed);

    // Map 3D positions to HEALPix pixels
    let seeds: Vec<u32> = centers
        .iter()
        .map(|c| healpix::vec2pix(params.nside, c))
        .collect();

    // Assign continental/oceanic type
    let continental_count = ((n as f32) * (1.0 - params.ocean_fraction)).round() as usize;

    // Score-based assignment to avoid polar clustering
    let mut scores: Vec<(usize, f64)> = (0..n)
        .map(|i| (i, hash_f64(params.seed.wrapping_add(7777), i as u32, 3)))
        .collect();
    scores.sort_by(|a, b| a.1.partial_cmp(&b.1).unwrap());

    let mut is_continental = vec![false; n];
    for k in 0..continental_count.min(n) {
        is_continental[scores[k].0] = true;
    }

    // Generate Euler poles and velocities
    let plates: Vec<PlateInfo> = (0..n)
        .map(|i| {
            let pole = random_unit_vector(params.seed.wrapping_add(1000), i as u32);
            let omega = (0.5 + hash_f64(params.seed.wrapping_add(2000), i as u32, 0).abs() * 1.5)
                * if hash_f64(params.seed.wrapping_add(3000), i as u32, 0) > 0.0 {
                    1.0
                } else {
                    -1.0
                }
                * params.tectonics_factor as f64;

            PlateInfo {
                center: centers[i],
                plate_type: if is_continental[i] { 1.0 } else { 0.0 },
                euler_pole: pole,
                angular_velocity: omega,
                area: 0,
            }
        })
        .collect();

    (seeds, plates)
}

// ─── 6.1.2: BFS Flood-Fill Plate Assignment ──────────────────────────────────

fn assign_plates_bfs(nside: u32, seeds: &[u32], seed: u32, npix: usize) -> Vec<u32> {
    let mut plate_id = vec![u32::MAX; npix];
    let mut queue = VecDeque::new();

    // Initialize seeds
    for (i, &pix) in seeds.iter().enumerate() {
        let idx = pix as usize;
        if idx < npix && plate_id[idx] == u32::MAX {
            plate_id[idx] = i as u32;
            queue.push_back(pix);
        }
    }

    // Round-robin BFS with noise perturbation for organic boundaries
    // Instead of strict BFS order, we process plates in rounds to get even growth
    let num_plates = seeds.len();
    let mut plate_queues: Vec<VecDeque<u32>> = vec![VecDeque::new(); num_plates];

    // Move initial seeds to per-plate queues
    while let Some(pix) = queue.pop_front() {
        let pid = plate_id[pix as usize] as usize;
        plate_queues[pid].push_back(pix);
    }

    // Round-robin expansion
    let mut active = true;
    while active {
        active = false;
        for pid in 0..num_plates {
            // Each plate expands a few cells per round (noise-modulated)
            let steps = 1 + (hash_f64(seed.wrapping_add(5000), pid as u32, 0).abs() * 3.0) as usize;
            for _ in 0..steps {
                if let Some(pix) = plate_queues[pid].pop_front() {
                    let nbrs = healpix::neighbors(nside, pix);
                    // Shuffle neighbors using seed for irregularity
                    let mut nbr_list: Vec<u32> = nbrs
                        .iter()
                        .copied()
                        .filter(|&n| n != u32::MAX && (n as usize) < npix)
                        .collect();

                    // Simple seed-based shuffle for boundary irregularity
                    for j in (1..nbr_list.len()).rev() {
                        let h = hash_u32(seed.wrapping_add(pix), j as u32) as usize;
                        nbr_list.swap(j, h % (j + 1));
                    }

                    for nbr in nbr_list {
                        if plate_id[nbr as usize] == u32::MAX {
                            plate_id[nbr as usize] = pid as u32;
                            plate_queues[pid].push_back(nbr);
                            active = true;
                        }
                    }
                }
            }
        }
    }

    // Handle any remaining unassigned pixels (shouldn't happen but safety)
    for ipix in 0..npix {
        if plate_id[ipix] == u32::MAX {
            let v = healpix::pix2vec(nside, ipix as u32);
            let nearest_seed = seeds
                .iter()
                .enumerate()
                .min_by(|&(_, a), &(_, b)| {
                    let va = healpix::pix2vec(nside, *a);
                    let vb = healpix::pix2vec(nside, *b);
                    let da = angular_dist_3d(&v, &va);
                    let db = angular_dist_3d(&v, &vb);
                    da.partial_cmp(&db).unwrap()
                })
                .map(|(i, _)| i)
                .unwrap_or(0);
            plate_id[ipix] = nearest_seed as u32;
        }
    }

    plate_id
}

/// Update plate centroids and areas from the assignment.
fn update_plate_stats(nside: u32, plate_id: &[u32], plates: &mut [PlateInfo]) {
    // Reset
    for p in plates.iter_mut() {
        p.area = 0;
        p.center = [0.0, 0.0, 0.0];
    }

    for (ipix, &pid) in plate_id.iter().enumerate() {
        if (pid as usize) < plates.len() {
            let v = healpix::pix2vec(nside, ipix as u32);
            let p = &mut plates[pid as usize];
            p.area += 1;
            p.center[0] += v[0];
            p.center[1] += v[1];
            p.center[2] += v[2];
        }
    }

    // Normalize centroids
    for p in plates.iter_mut() {
        if p.area > 0 {
            let len = (p.center[0].powi(2) + p.center[1].powi(2) + p.center[2].powi(2)).sqrt();
            if len > 1e-10 {
                p.center[0] /= len;
                p.center[1] /= len;
                p.center[2] /= len;
            }
        }
    }
}

// ─── 6.1.3: Boundary Distance Field ──────────────────────────────────────────

fn compute_boundary_distance(nside: u32, plate_id: &[u32], npix: usize) -> Vec<f32> {
    let mut dist = vec![f32::MAX; npix];
    let mut queue = VecDeque::new();

    // Find boundary pixels: pixels with at least one neighbor on a different plate
    for ipix in 0..npix {
        let pid = plate_id[ipix];
        let nbrs = healpix::neighbors(nside, ipix as u32);
        let is_boundary = nbrs.iter().any(|&n| {
            n != u32::MAX && (n as usize) < npix && plate_id[n as usize] != pid
        });
        if is_boundary {
            dist[ipix] = 0.0;
            queue.push_back(ipix as u32);
        }
    }

    // Multi-source BFS
    bfs_distance(nside, &mut dist, &mut queue, npix);

    dist
}

// ─── 6.1.4: Coast Distance Field ─────────────────────────────────────────────

fn compute_coast_distance(
    nside: u32,
    plate_id: &[u32],
    plates: &[PlateInfo],
    npix: usize,
) -> Vec<f32> {
    let mut dist = vec![f32::MAX; npix];
    let mut queue = VecDeque::new();

    // Find coast pixels: continental pixels adjacent to oceanic (or vice versa)
    for ipix in 0..npix {
        let pid = plate_id[ipix] as usize;
        if pid >= plates.len() {
            continue;
        }
        let my_type = plates[pid].plate_type > 0.5; // true = continental
        let nbrs = healpix::neighbors(nside, ipix as u32);
        let is_coast = nbrs.iter().any(|&n| {
            if n == u32::MAX || (n as usize) >= npix {
                return false;
            }
            let np = plate_id[n as usize] as usize;
            np < plates.len() && (plates[np].plate_type > 0.5) != my_type
        });
        if is_coast {
            dist[ipix] = 0.0;
            queue.push_back(ipix as u32);
        }
    }

    // Multi-source BFS
    bfs_distance(nside, &mut dist, &mut queue, npix);

    // Sign convention: positive for continental (inland), negative for oceanic (offshore)
    for ipix in 0..npix {
        let pid = plate_id[ipix] as usize;
        if pid < plates.len() && plates[pid].plate_type <= 0.5 {
            dist[ipix] = -dist[ipix];
        }
    }

    dist
}

// ─── 6.1.5: Super-Plate Clustering ───────────────────────────────────────────

fn cluster_super_plates(
    nside: u32,
    plate_id: &[u32],
    plates: &[PlateInfo],
    num_continents: u32,
    continent_size_variety: f32,
    seed: u32,
    npix: usize,
) -> Vec<u32> {
    let num_plates = plates.len();
    if num_plates == 0 {
        return vec![0; npix];
    }

    // Build plate adjacency graph
    let mut adjacency: Vec<Vec<u32>> = vec![Vec::new(); num_plates];
    for ipix in 0..npix {
        let pid = plate_id[ipix] as usize;
        if pid >= num_plates {
            continue;
        }
        let nbrs = healpix::neighbors(nside, ipix as u32);
        for &n in &nbrs {
            if n == u32::MAX || (n as usize) >= npix {
                continue;
            }
            let npid = plate_id[n as usize] as usize;
            if npid < num_plates && npid != pid && !adjacency[pid].contains(&(npid as u32)) {
                adjacency[pid].push(npid as u32);
            }
        }
    }

    // Separate continental and oceanic plates
    let continental_plates: Vec<usize> = (0..num_plates)
        .filter(|&i| plates[i].plate_type > 0.5)
        .collect();
    let oceanic_plates: Vec<usize> = (0..num_plates)
        .filter(|&i| plates[i].plate_type <= 0.5)
        .collect();

    let mut plate_to_super = vec![u32::MAX; num_plates];
    let mut next_super_id = 0u32;

    // Assign continental plates to continents using farthest-point seeding
    let nc = (num_continents as usize).min(continental_plates.len()).max(1);

    if !continental_plates.is_empty() {
        // Farthest-point seeding among continental plates
        let mut continent_seeds = Vec::with_capacity(nc);

        // First seed: random continental plate
        let first_idx = hash_u32(seed.wrapping_add(8888), 0) as usize % continental_plates.len();
        continent_seeds.push(continental_plates[first_idx]);

        // Subsequent seeds: farthest from existing seeds
        for _ in 1..nc {
            let mut best_plate = continental_plates[0];
            let mut best_dist = 0.0f64;
            for &cp in &continental_plates {
                if continent_seeds.contains(&cp) {
                    continue;
                }
                let min_dist = continent_seeds
                    .iter()
                    .map(|&s| angular_dist_3d(&plates[cp].center, &plates[s].center))
                    .fold(f64::MAX, f64::min);
                if min_dist > best_dist {
                    best_dist = min_dist;
                    best_plate = cp;
                }
            }
            continent_seeds.push(best_plate);
        }

        // Assign seed plates to their own super-plate
        for (ci, &sp) in continent_seeds.iter().enumerate() {
            plate_to_super[sp] = ci as u32;
        }
        next_super_id = nc as u32;

        // Growth targets with variety
        let total_continental_area: u32 = continental_plates.iter().map(|&i| plates[i].area).sum();
        let targets = compute_growth_targets(
            nc,
            total_continental_area as f64,
            continent_size_variety,
            seed,
        );

        // Grow continents by absorbing adjacent unassigned continental plates
        let mut changed = true;
        while changed {
            changed = false;
            for ci in 0..nc {
                let current_area: u32 = continental_plates
                    .iter()
                    .filter(|&&p| plate_to_super[p] == ci as u32)
                    .map(|&p| plates[p].area)
                    .sum();

                if current_area as f64 >= targets[ci] {
                    continue;
                }

                // Find unassigned adjacent continental plates
                let mut candidates = Vec::new();
                for &cp in &continental_plates {
                    if plate_to_super[cp] != u32::MAX {
                        continue;
                    }
                    // Check if adjacent to this continent
                    let adjacent = adjacency[cp]
                        .iter()
                        .any(|&adj| plate_to_super[adj as usize] == ci as u32);
                    if adjacent {
                        candidates.push(cp);
                    }
                }

                if let Some(&best) = candidates.first() {
                    plate_to_super[best] = ci as u32;
                    changed = true;
                }
            }
        }

        // Assign remaining unassigned continental plates to nearest continent
        for &cp in &continental_plates {
            if plate_to_super[cp] == u32::MAX {
                let nearest = continent_seeds
                    .iter()
                    .enumerate()
                    .min_by(|&(_, a), &(_, b)| {
                        let da = angular_dist_3d(&plates[cp].center, &plates[*a].center);
                        let db = angular_dist_3d(&plates[cp].center, &plates[*b].center);
                        da.partial_cmp(&db).unwrap()
                    })
                    .map(|(i, _)| i)
                    .unwrap_or(0);
                plate_to_super[cp] = nearest as u32;
            }
        }
    }

    // Assign oceanic plates: group connected oceanic plates into super-plates
    if !oceanic_plates.is_empty() {
        let mut visited = vec![false; num_plates];
        for &op in &oceanic_plates {
            if visited[op] {
                continue;
            }
            // BFS on oceanic plate adjacency
            let mut component = Vec::new();
            let mut q = VecDeque::new();
            q.push_back(op);
            visited[op] = true;
            while let Some(p) = q.pop_front() {
                component.push(p);
                for &adj in &adjacency[p] {
                    let adj = adj as usize;
                    if !visited[adj] && adj < num_plates && plates[adj].plate_type <= 0.5 {
                        visited[adj] = true;
                        q.push_back(adj);
                    }
                }
            }

            let sp_id = next_super_id;
            next_super_id += 1;
            for &p in &component {
                plate_to_super[p] = sp_id;
            }
        }
    }

    // Map pixel-level plate_id to super_plate_id
    let mut super_plate_id = vec![0u32; npix];
    for ipix in 0..npix {
        let pid = plate_id[ipix] as usize;
        if pid < num_plates {
            super_plate_id[ipix] = plate_to_super[pid];
        }
    }

    super_plate_id
}

fn compute_growth_targets(nc: usize, total_area: f64, variety: f32, seed: u32) -> Vec<f64> {
    if nc == 0 {
        return Vec::new();
    }
    if variety < 0.01 || nc == 1 {
        return vec![total_area / nc as f64; nc];
    }

    // Log-normal distributed weights (reference: variety × 2.5 spread)
    let mut weights: Vec<f64> = (0..nc)
        .map(|i| {
            let log_w = (hash_f64(seed.wrapping_add(9999), i as u32, 0) - 0.0) * variety as f64 * 2.5;
            log_w.exp()
        })
        .collect();

    let sum: f64 = weights.iter().sum();
    for w in &mut weights {
        *w = total_area * *w / sum;
    }
    weights
}

// ─── 6.1.6: Stress Computation ───────────────────────────────────────────────

fn compute_stress(
    nside: u32,
    plate_id: &[u32],
    plates: &[PlateInfo],
    dist_boundary: &[f32],
    npix: usize,
) -> Vec<f32> {
    let mut stress = vec![0.0f32; npix];

    // Compute stress at boundary pixels, then decay inward
    for ipix in 0..npix {
        if dist_boundary[ipix] > 0.5 {
            continue; // Not a boundary pixel
        }

        let pid = plate_id[ipix] as usize;
        if pid >= plates.len() {
            continue;
        }

        let pos = healpix::pix2vec(nside, ipix as u32);

        // Velocity at this pixel: v = omega * (pole × pos)
        let my_vel = plate_velocity_at(&plates[pid], &pos);

        // Find the neighboring plate(s) and compute relative velocity
        let nbrs = healpix::neighbors(nside, ipix as u32);
        let mut max_convergence = 0.0f64;

        for &n in &nbrs {
            if n == u32::MAX || (n as usize) >= npix {
                continue;
            }
            let npid = plate_id[n as usize] as usize;
            if npid == pid || npid >= plates.len() {
                continue;
            }

            let other_vel = plate_velocity_at(&plates[npid], &pos);

            // Relative velocity
            let rel = [
                my_vel[0] - other_vel[0],
                my_vel[1] - other_vel[1],
                my_vel[2] - other_vel[2],
            ];

            // Boundary normal: direction from this pixel to neighbor
            let npos = healpix::pix2vec(nside, n);
            let dx = [npos[0] - pos[0], npos[1] - pos[1], npos[2] - pos[2]];
            let dx_len = (dx[0].powi(2) + dx[1].powi(2) + dx[2].powi(2)).sqrt();
            if dx_len < 1e-10 {
                continue;
            }
            let normal = [dx[0] / dx_len, dx[1] / dx_len, dx[2] / dx_len];

            // Convergence = negative dot product of relative velocity with boundary normal
            // Positive convergence means plates are moving toward each other
            let convergence = -(rel[0] * normal[0] + rel[1] * normal[1] + rel[2] * normal[2]);
            max_convergence = max_convergence.max(convergence);
        }

        stress[ipix] = max_convergence.max(0.0) as f32;
    }

    // Normalize boundary stress to [0, 1]
    let max_stress = stress.iter().copied().fold(0.0f32, f32::max);
    if max_stress > 1e-10 {
        for s in &mut stress {
            *s /= max_stress;
        }
    }

    // Propagate stress inward with exponential decay via BFS
    let decay = 0.85f32; // per-hop decay factor
    let max_hops = (nside / 4).max(8) as f32;

    let mut propagated = stress.clone();
    let mut queue = VecDeque::new();
    let mut visited = vec![false; npix];

    // Seed BFS from boundary pixels with nonzero stress
    for ipix in 0..npix {
        if stress[ipix] > 0.01 {
            queue.push_back((ipix as u32, 0u32));
            visited[ipix] = true;
        }
    }

    while let Some((pix, hops)) = queue.pop_front() {
        if hops as f32 >= max_hops {
            continue;
        }
        let current_stress = propagated[pix as usize];
        let nbrs = healpix::neighbors(nside, pix);
        for &n in &nbrs {
            if n == u32::MAX || (n as usize) >= npix || visited[n as usize] {
                continue;
            }
            let decayed = current_stress * decay;
            if decayed > propagated[n as usize] {
                propagated[n as usize] = decayed;
                visited[n as usize] = true;
                queue.push_back((n, hops + 1));
            }
        }
    }

    propagated
}

/// Compute plate velocity at a given position using Euler pole rotation.
fn plate_velocity_at(plate: &PlateInfo, pos: &[f64; 3]) -> [f64; 3] {
    let p = plate.euler_pole;
    let w = plate.angular_velocity;
    // v = omega * (pole × pos)
    [
        w * (p[1] * pos[2] - p[2] * pos[1]),
        w * (p[2] * pos[0] - p[0] * pos[2]),
        w * (p[0] * pos[1] - p[1] * pos[0]),
    ]
}

// ─── Shared BFS helper ───────────────────────────────────────────────────────

fn bfs_distance(nside: u32, dist: &mut [f32], queue: &mut VecDeque<u32>, npix: usize) {
    while let Some(pix) = queue.pop_front() {
        let current_dist = dist[pix as usize];
        let nbrs = healpix::neighbors(nside, pix);
        for &n in &nbrs {
            if n == u32::MAX || (n as usize) >= npix {
                continue;
            }
            let new_dist = current_dist + 1.0;
            if new_dist < dist[n as usize] {
                dist[n as usize] = new_dist;
                queue.push_back(n);
            }
        }
    }
}

// ─── Geometry & hash helpers ──────────────────────────────────────────────────

fn fibonacci_sphere(n: usize, seed: u32) -> Vec<[f64; 3]> {
    let golden_ratio = (1.0 + 5.0_f64.sqrt()) / 2.0;
    let mut points = Vec::with_capacity(n);

    for i in 0..n {
        let theta = 2.0 * std::f64::consts::PI * (i as f64) / golden_ratio;
        let phi = (1.0 - 2.0 * (i as f64 + 0.5) / n as f64).acos();

        let x = phi.sin() * theta.cos();
        let y = phi.cos();
        let z = phi.sin() * theta.sin();

        // Seed-based perturbation
        let hx = hash_f64(seed, i as u32, 0) * 0.3;
        let hy = hash_f64(seed, i as u32, 1) * 0.3;
        let hz = hash_f64(seed, i as u32, 2) * 0.3;

        let px = x + hx;
        let py = y + hy;
        let pz = z + hz;
        let len = (px * px + py * py + pz * pz).sqrt();
        points.push([px / len, py / len, pz / len]);
    }
    points
}

fn random_unit_vector(seed: u32, index: u32) -> [f64; 3] {
    let x = hash_f64(seed, index, 0);
    let y = hash_f64(seed, index, 1);
    let z = hash_f64(seed, index, 2);
    let len = (x * x + y * y + z * z).sqrt().max(1e-10);
    [x / len, y / len, z / len]
}

fn angular_dist_3d(a: &[f64; 3], b: &[f64; 3]) -> f64 {
    let d = (a[0] * b[0] + a[1] * b[1] + a[2] * b[2]).clamp(-1.0, 1.0);
    d.acos()
}

fn hash_f64(seed: u32, index: u32, channel: u32) -> f64 {
    let mut h = seed
        .wrapping_mul(374761393)
        .wrapping_add(index.wrapping_mul(668265263))
        .wrapping_add(channel.wrapping_mul(1274126177));
    h = (h ^ (h >> 13)).wrapping_mul(1103515245);
    h = h ^ (h >> 16);
    (h as f64 / u32::MAX as f64) * 2.0 - 1.0
}

fn hash_u32(seed: u32, index: u32) -> u32 {
    let mut h = seed
        .wrapping_mul(374761393)
        .wrapping_add(index.wrapping_mul(668265263));
    h = (h ^ (h >> 13)).wrapping_mul(1103515245);
    h ^ (h >> 16)
}

// ─── Tests ────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    fn default_params() -> PlateSimParams {
        PlateSimParams::default()
    }

    #[test]
    fn seeds_are_well_distributed() {
        let params = default_params();
        let (seeds, _) = generate_plate_seeds(&params);
        assert_eq!(seeds.len(), params.num_plates as usize);

        // No duplicate seed pixels
        let mut sorted = seeds.clone();
        sorted.sort();
        sorted.dedup();
        assert_eq!(sorted.len(), seeds.len(), "Duplicate seed pixels found");
    }

    #[test]
    fn continental_oceanic_ratio() {
        let params = default_params();
        let (_, plates) = generate_plate_seeds(&params);
        let continental = plates.iter().filter(|p| p.plate_type > 0.5).count();
        let expected = ((params.num_plates as f32) * (1.0 - params.ocean_fraction)).round() as usize;
        assert_eq!(continental, expected, "Continental count mismatch");
    }

    #[test]
    fn bfs_assigns_all_pixels() {
        let params = default_params();
        let (seeds, _) = generate_plate_seeds(&params);
        let npix = healpix::npix(params.nside) as usize;
        let plate_id = assign_plates_bfs(params.nside, &seeds, params.seed, npix);

        let unassigned = plate_id.iter().filter(|&&p| p == u32::MAX).count();
        assert_eq!(unassigned, 0, "All pixels should be assigned");

        // All assigned to valid plates
        let max_pid = *plate_id.iter().max().unwrap();
        assert!(
            max_pid < params.num_plates,
            "Max plate id {max_pid} >= num_plates {}",
            params.num_plates
        );
    }

    #[test]
    fn boundary_distance_zero_at_boundaries() {
        let params = default_params();
        let npix = healpix::npix(params.nside) as usize;
        let (seeds, _) = generate_plate_seeds(&params);
        let plate_id = assign_plates_bfs(params.nside, &seeds, params.seed, npix);
        let dist = compute_boundary_distance(params.nside, &plate_id, npix);

        // Some pixels should have distance 0 (boundary pixels)
        let boundary_count = dist.iter().filter(|&&d| d < 0.5).count();
        assert!(boundary_count > 0, "Should have boundary pixels");
        assert!(
            boundary_count < npix,
            "Not all pixels should be boundaries"
        );

        // Interior pixels should have positive distance
        let interior_count = dist.iter().filter(|&&d| d > 1.5).count();
        assert!(interior_count > 0, "Should have interior pixels");
    }

    #[test]
    fn coast_distance_has_both_signs() {
        let params = default_params();
        let npix = healpix::npix(params.nside) as usize;
        let (seeds, mut plates) = generate_plate_seeds(&params);
        let plate_id = assign_plates_bfs(params.nside, &seeds, params.seed, npix);
        update_plate_stats(params.nside, &plate_id, &mut plates);
        let dist_coast = compute_coast_distance(params.nside, &plate_id, &plates, npix);

        let positive = dist_coast.iter().filter(|&&d| d > 0.5).count();
        let negative = dist_coast.iter().filter(|&&d| d < -0.5).count();
        assert!(positive > 0, "Should have inland pixels (positive dist)");
        assert!(negative > 0, "Should have offshore pixels (negative dist)");
    }

    #[test]
    fn super_plates_respect_continent_count() {
        let params = PlateSimParams {
            num_continents: 3,
            ..default_params()
        };
        let npix = healpix::npix(params.nside) as usize;
        let (seeds, mut plates) = generate_plate_seeds(&params);
        let plate_id = assign_plates_bfs(params.nside, &seeds, params.seed, npix);
        update_plate_stats(params.nside, &plate_id, &mut plates);

        let super_ids = cluster_super_plates(
            params.nside,
            &plate_id,
            &plates,
            params.num_continents,
            params.continent_size_variety,
            params.seed,
            npix,
        );

        // Count unique super-plate IDs among continental pixels
        let mut continental_supers: Vec<u32> = (0..npix)
            .filter(|&i| {
                let pid = plate_id[i] as usize;
                pid < plates.len() && plates[pid].plate_type > 0.5
            })
            .map(|i| super_ids[i])
            .collect();
        continental_supers.sort();
        continental_supers.dedup();

        assert!(
            continental_supers.len() <= params.num_continents as usize + 1,
            "Too many continental super-plates: {} (expected ≤{})",
            continental_supers.len(),
            params.num_continents
        );
        assert!(
            !continental_supers.is_empty(),
            "Should have at least one continent"
        );
    }

    #[test]
    fn stress_concentrated_at_convergent_boundaries() {
        let params = default_params();
        let npix = healpix::npix(params.nside) as usize;
        let (seeds, mut plates) = generate_plate_seeds(&params);
        let plate_id = assign_plates_bfs(params.nside, &seeds, params.seed, npix);
        update_plate_stats(params.nside, &plate_id, &mut plates);
        let dist_boundary = compute_boundary_distance(params.nside, &plate_id, npix);
        let stress = compute_stress(params.nside, &plate_id, &plates, &dist_boundary, npix);

        // Average stress near boundaries should be higher than far from boundaries
        let near_boundary: Vec<f32> = (0..npix)
            .filter(|&i| dist_boundary[i] < 3.0)
            .map(|i| stress[i])
            .collect();
        let far_from_boundary: Vec<f32> = (0..npix)
            .filter(|&i| dist_boundary[i] > 10.0)
            .map(|i| stress[i])
            .collect();

        let avg_near = near_boundary.iter().sum::<f32>() / near_boundary.len().max(1) as f32;
        let avg_far = far_from_boundary.iter().sum::<f32>() / far_from_boundary.len().max(1) as f32;

        assert!(
            avg_near > avg_far,
            "Stress should be higher near boundaries: near={avg_near:.4}, far={avg_far:.4}"
        );
    }

    #[test]
    fn full_simulation_runs() {
        let params = default_params();
        let result = simulate(&params);

        let npix = healpix::npix(params.nside) as usize;
        assert_eq!(result.plate_id.len(), npix);
        assert_eq!(result.dist_boundary.len(), npix);
        assert_eq!(result.dist_coast.len(), npix);
        assert_eq!(result.super_plate_id.len(), npix);
        assert_eq!(result.stress.len(), npix);
        assert_eq!(result.plates.len(), params.num_plates as usize);
    }

    #[test]
    fn different_seeds_produce_different_results() {
        let r1 = simulate(&PlateSimParams {
            seed: 1,
            ..default_params()
        });
        let r2 = simulate(&PlateSimParams {
            seed: 999,
            ..default_params()
        });

        let diff: u32 = r1
            .plate_id
            .iter()
            .zip(r2.plate_id.iter())
            .map(|(a, b)| if a != b { 1 } else { 0 })
            .sum();
        let npix = healpix::npix(64) as u32;
        assert!(
            diff > npix / 4,
            "Different seeds should produce substantially different plates"
        );
    }
}
