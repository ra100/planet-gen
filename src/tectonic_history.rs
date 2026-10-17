//! Plate motion over geologic time (Plans.md 9b.3–9b.5).
//!
//! Plates are simulated on the CPU as rigid sets of *seeds* on the unit
//! sphere that rotate about their Euler poles. Converging seeds of different
//! plates collide: continental pairs merge into one plate and leave a suture,
//! oceanic seeds subduct and leave a volcanic arc on the overriding plate.
//! Gaps opened by diverging plates fill with new oceanic seeds (seafloor
//! spreading), and long-lived merged continents rift apart again, so
//! supercontinents assemble and break up over time.
//!
//! The result is packed into the existing `array<Plate>` storage buffer as
//! trailing records (see `TectonicHistory::gpu_records`), so terrain
//! dispatch, export and the Plates view keep a single plate buffer. With no
//! simulated time the records are byte-identical to the input plates.

use crate::plates::PlateGpu;

/// Simulation step.
const STEP_MYR: f32 = 5.0;
/// Angular speed per unit of `PlateGpu::velocity` magnitude, in rad/Myr.
/// Earth-like tectonics (|ω| ≈ 0.43) moves ≈ 0.0077 rad/Myr ≈ 5 cm/yr.
const RAD_PER_MYR_PER_OMEGA: f32 = 0.018;
/// Upper bound on plates the terrain shader loops over.
pub const MAX_PLATES: usize = 28;
/// Upper bound on seeds across all plates.
const MAX_SEEDS: usize = 128;
/// Upper bound on recorded history features.
const MAX_FEATURES: usize = 48;
/// Upper bound on simulated time (slider range).
pub const MAX_GEOLOGIC_TIME_MYR: f32 = 3000.0;
/// E-folding time of orogen relief under erosion.
const EROSION_EFOLD_MYR: f32 = 600.0;

/// Marker in `plate_type` for the record that separates real plates from
/// the seed and feature records that follow them.
pub const META_RECORD_MARKER: f32 = -1.0;

#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub enum FeatureKind {
    /// Continental collision belt, left inside the merged plate.
    Suture = 0,
    /// Volcanic arc on the plate overriding a subducting oceanic seed.
    Arc = 1,
    /// Rift where a continent split.
    Rift = 2,
}

#[derive(Copy, Clone, Debug)]
pub struct HistoryFeature {
    pub kind: FeatureKind,
    /// Great-circle segment endpoints in the present-day frame.
    pub a: [f32; 3],
    pub b: [f32; 3],
    /// Relief strength when the feature formed.
    pub strength: f32,
    pub formed_myr: f32,
    owner: usize,
}

#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub enum HistoryEventKind {
    Merge,
    Split,
    Subduction,
    Spreading,
    Reorganization,
}

#[derive(Copy, Clone, Debug)]
pub struct HistoryEvent {
    pub time_myr: f32,
    pub kind: HistoryEventKind,
}

#[derive(Clone, Debug)]
pub struct SimPlate {
    pub seeds: Vec<[f32; 3]>,
    /// Euler angular velocity, same units as `PlateGpu::velocity`.
    pub omega: [f32; 3],
    pub continental: bool,
    /// Number of original continents merged into this plate.
    members: u32,
    tenure_myr: f32,
    breakup_after_myr: f32,
    next_reorganization_myr: f32,
}

#[derive(Clone, Debug)]
pub struct TectonicHistory {
    pub plates: Vec<SimPlate>,
    pub features: Vec<HistoryFeature>,
    pub events: Vec<HistoryEvent>,
    /// (time, continental plate count) sampled every step.
    pub continental_plate_counts: Vec<(f32, usize)>,
    pub time_myr: f32,
    /// Input plates, returned unchanged when no time was simulated.
    initial: Vec<PlateGpu>,
}

/// Number of real plates in a record slice built by `gpu_records`.
pub fn plate_count(records: &[PlateGpu]) -> usize {
    records
        .iter()
        .position(|record| record.plate_type == META_RECORD_MARKER)
        .unwrap_or(records.len())
}

/// Run the history and pack it for the GPU. `geologic_time_myr <= 0`
/// returns `plates` unchanged.
pub fn plate_records(plates: &[PlateGpu], seed: u32, geologic_time_myr: f32) -> Vec<PlateGpu> {
    simulate(plates, seed, geologic_time_myr).gpu_records()
}

pub fn simulate(plates: &[PlateGpu], seed: u32, geologic_time_myr: f32) -> TectonicHistory {
    let mut sim = Simulation::new(plates, seed);
    let total = geologic_time_myr.clamp(0.0, MAX_GEOLOGIC_TIME_MYR);
    let moving = plates.iter().any(|p| length(p.velocity) > 0.0);
    if total > 0.0 && moving && !plates.is_empty() {
        let steps = (total / STEP_MYR).ceil() as u32;
        let dt = total / steps as f32;
        for _ in 0..steps {
            sim.step(dt);
        }
    }
    TectonicHistory {
        plates: sim.plates,
        features: sim.features,
        events: sim.events,
        continental_plate_counts: sim.counts,
        time_myr: if moving { total } else { 0.0 },
        initial: plates.to_vec(),
    }
}

impl TectonicHistory {
    /// Present-day relief strength of a feature after erosion.
    pub fn feature_amplitude(&self, feature: &HistoryFeature) -> f32 {
        let age = (self.time_myr - feature.formed_myr).max(0.0);
        feature.strength * (0.25 + 0.75 * (-age / EROSION_EFOLD_MYR).exp())
    }

    /// Present-day belt half-width (radians); old belts are broader.
    pub fn feature_width(&self, feature: &HistoryFeature) -> f32 {
        let age = (self.time_myr - feature.formed_myr).max(0.0);
        let base = match feature.kind {
            FeatureKind::Suture => 0.055,
            FeatureKind::Arc => 0.035,
            FeatureKind::Rift => 0.045,
        };
        base * (1.0 + age / 1000.0)
    }

    pub fn continental_plate_count(&self) -> usize {
        self.plates.iter().filter(|p| p.continental).count()
    }

    /// Pack plates, seeds and features into one `array<Plate>` buffer.
    ///
    /// Layout: real plates, then (only when needed) a meta record whose
    /// `plate_type` is `META_RECORD_MARKER` and whose center holds the seed
    /// and feature counts as u32 bits, then one record per seed (position,
    /// owning plate index) and two per feature. A plate with several seeds
    /// stores `seed_start | seed_count << 16` in `_pad`; single-seed plates
    /// keep `_pad = 0` and their seed as `center`, exactly like plain plates.
    pub fn gpu_records(&self) -> Vec<PlateGpu> {
        if self.time_myr <= 0.0 {
            return self.initial.clone();
        }
        let multi_seed = self.plates.iter().any(|p| p.seeds.len() > 1);
        let mut records: Vec<PlateGpu> = Vec::with_capacity(self.plates.len());
        let mut seed_records = Vec::new();
        for (index, plate) in self.plates.iter().enumerate() {
            let mut pad = 0.0;
            if plate.seeds.len() > 1 {
                let start = seed_records.len() as u32;
                let count = plate.seeds.len() as u32;
                pad = f32::from_bits(start | (count << 16));
                for seed in &plate.seeds {
                    seed_records.push(PlateGpu {
                        center: *seed,
                        plate_type: index as f32,
                        velocity: [0.0; 3],
                        _pad: 0.0,
                    });
                }
            }
            records.push(PlateGpu {
                center: plate.centroid(),
                plate_type: if plate.continental { 1.0 } else { 0.0 },
                velocity: plate.omega,
                _pad: pad,
            });
        }
        if !multi_seed && self.features.is_empty() {
            return records;
        }
        records.push(PlateGpu {
            center: [
                f32::from_bits(seed_records.len() as u32),
                f32::from_bits(self.features.len() as u32),
                0.0,
            ],
            plate_type: META_RECORD_MARKER,
            velocity: [0.0; 3],
            _pad: 0.0,
        });
        records.extend(seed_records);
        for feature in &self.features {
            records.push(PlateGpu {
                center: feature.a,
                plate_type: feature.kind as u32 as f32,
                velocity: feature.b,
                _pad: self.feature_amplitude(feature),
            });
            let age = (self.time_myr - feature.formed_myr).max(0.0);
            records.push(PlateGpu {
                center: [
                    self.feature_width(feature),
                    age / self.time_myr.max(1.0),
                    0.0,
                ],
                plate_type: 0.0,
                velocity: [0.0; 3],
                _pad: 0.0,
            });
        }
        records
    }
}

impl SimPlate {
    pub fn centroid(&self) -> [f32; 3] {
        if self.seeds.len() == 1 {
            return self.seeds[0];
        }
        let mut sum = [0.0; 3];
        for seed in &self.seeds {
            sum = add(sum, *seed);
        }
        if length(sum) < 1e-4 {
            self.seeds[0]
        } else {
            normalize(sum)
        }
    }
}

struct Simulation {
    plates: Vec<SimPlate>,
    features: Vec<HistoryFeature>,
    events: Vec<HistoryEvent>,
    counts: Vec<(f32, usize)>,
    time: f32,
    seed: u32,
    /// Typical plate spacing (radians) of the initial layout; sets belt lengths.
    spacing: f32,
    /// Lattice spacing of the seeds (radians); sets contact and gap distances.
    seed_spacing: f32,
    /// Seed count the spreading step refills toward.
    target_seeds: usize,
    base_speed: f32,
    samples: Vec<[f32; 3]>,
    step_index: u32,
}

impl Simulation {
    fn new(plates: &[PlateGpu], seed: u32) -> Self {
        let n = plates.len().max(1);
        let spacing = (4.0 * std::f32::consts::PI / n as f32).sqrt();
        let base_speed = plates
            .iter()
            .map(|p| length(p.velocity))
            .fold(0.0_f32, f32::max);
        // Subdivide every plate into lattice seeds so plates can merge,
        // shrink and split while keeping their area.
        let target_seeds = (6 * n).clamp(48, 96);
        let mut seeds_by_plate: Vec<Vec<[f32; 3]>> = vec![Vec::new(); plates.len()];
        if !plates.is_empty() {
            for point in fibonacci_points(target_seeds) {
                let owner = (0..plates.len())
                    .max_by(|&a, &b| {
                        dot(point, plates[a].center).total_cmp(&dot(point, plates[b].center))
                    })
                    .unwrap_or(0);
                seeds_by_plate[owner].push(point);
            }
        }
        let sim_plates = plates
            .iter()
            .zip(seeds_by_plate)
            .enumerate()
            .map(|(i, (p, seeds))| SimPlate {
                seeds: if seeds.is_empty() {
                    vec![p.center]
                } else {
                    seeds
                },
                omega: p.velocity,
                continental: p.plate_type > 0.5,
                members: u32::from(p.plate_type > 0.5),
                tenure_myr: 0.0,
                breakup_after_myr: 200.0 + 150.0 * unit_hash(seed, i as u32, 11),
                next_reorganization_myr: 100.0 + 100.0 * unit_hash(seed, i as u32, 12),
            })
            .collect();
        Self {
            plates: sim_plates,
            features: Vec::new(),
            events: Vec::new(),
            counts: Vec::new(),
            time: 0.0,
            seed,
            spacing,
            seed_spacing: (4.0 * std::f32::consts::PI / target_seeds as f32).sqrt(),
            target_seeds,
            base_speed,
            samples: fibonacci_points(600),
            step_index: 0,
        }
    }

    fn step(&mut self, dt: f32) {
        self.time += dt;
        self.step_index += 1;
        self.advect(dt);
        self.reorganize();
        self.collide();
        if self.step_index.is_multiple_of(2) {
            self.spread();
        }
        self.break_up();
        self.counts.push((
            self.time,
            self.plates.iter().filter(|p| p.continental).count(),
        ));
    }

    fn advect(&mut self, dt: f32) {
        for (index, plate) in self.plates.iter_mut().enumerate() {
            plate.tenure_myr += dt;
            let speed = length(plate.omega);
            if speed <= 0.0 {
                continue;
            }
            let axis = scale(plate.omega, 1.0 / speed);
            let angle = speed * RAD_PER_MYR_PER_OMEGA * dt;
            for seed in &mut plate.seeds {
                *seed = normalize(rotate(*seed, axis, angle));
            }
            for feature in self.features.iter_mut().filter(|f| f.owner == index) {
                feature.a = normalize(rotate(feature.a, axis, angle));
                feature.b = normalize(rotate(feature.b, axis, angle));
            }
        }
    }

    /// Plate reorganisation: Euler poles wander on a seeded schedule.
    fn reorganize(&mut self) {
        for index in 0..self.plates.len() {
            if self.time < self.plates[index].next_reorganization_myr {
                continue;
            }
            let draw = self.draw(index as u32, 21);
            let speed = length(self.plates[index].omega).max(self.base_speed * 0.5);
            let pole = random_unit(self.seed, draw);
            self.plates[index].omega = scale(pole, speed);
            self.plates[index].next_reorganization_myr =
                self.time + 100.0 + 100.0 * unit_hash(self.seed, draw, 22);
            self.events.push(HistoryEvent {
                time_myr: self.time,
                kind: HistoryEventKind::Reorganization,
            });
        }
    }

    fn collide(&mut self) {
        let contact = 0.85 * self.seed_spacing;
        let mut touched = vec![false; self.plates.len()];
        let mut index_a = 0;
        while index_a < self.plates.len() {
            let mut index_b = index_a + 1;
            while index_b < self.plates.len() {
                if !touched[index_a]
                    && !touched[index_b]
                    && let Some((sa, sb)) = self.converging_contact(index_a, index_b, contact)
                {
                    touched[index_a] = true;
                    touched[index_b] = true;
                    self.resolve_contact(index_a, sa, index_b, sb);
                }
                index_b += 1;
            }
            index_a += 1;
        }
        self.remove_empty_plates();
    }

    /// Closest converging seed pair between two plates within `contact`.
    fn converging_contact(&self, a: usize, b: usize, contact: f32) -> Option<(usize, usize)> {
        let plate_a = &self.plates[a];
        let plate_b = &self.plates[b];
        let mut best: Option<(usize, usize, f32)> = None;
        for (ia, pa) in plate_a.seeds.iter().enumerate() {
            for (ib, pb) in plate_b.seeds.iter().enumerate() {
                let angle = angle_between(*pa, *pb);
                if angle >= contact || best.is_some_and(|(_, _, d)| angle >= d) {
                    continue;
                }
                let toward_b = sub(*pb, *pa);
                let rel = sub(cross(plate_a.omega, *pa), cross(plate_b.omega, *pb));
                if dot(rel, toward_b) > 0.0 {
                    best = Some((ia, ib, angle));
                }
            }
        }
        best.map(|(ia, ib, _)| (ia, ib))
    }

    fn resolve_contact(&mut self, a: usize, sa: usize, b: usize, sb: usize) {
        let pa = self.plates[a].seeds[sa];
        let pb = self.plates[b].seeds[sb];
        let mid = normalize(add(pa, pb));
        if self.plates[a].continental && self.plates[b].continental {
            self.add_feature(FeatureKind::Suture, mid, sub(pb, pa), 0.45, 1.0, a);
            self.merge(a, b);
            self.events.push(HistoryEvent {
                time_myr: self.time,
                kind: HistoryEventKind::Merge,
            });
            return;
        }
        // Oceanic lithosphere subducts; for two oceanic plates the smaller one goes down.
        let a_subducts = if self.plates[a].continental != self.plates[b].continental {
            !self.plates[a].continental
        } else {
            self.plates[a].seeds.len() < self.plates[b].seeds.len()
        };
        let (down, down_seed, over, over_seed) = if a_subducts {
            (a, sa, b, pb)
        } else {
            (b, sb, a, pa)
        };
        let down_pos = self.plates[down].seeds[down_seed];
        let arc_center = normalize(add(scale(mid, 0.65), scale(over_seed, 0.35)));
        let strength = if self.plates[over].continental {
            0.9
        } else {
            0.55
        };
        self.add_feature(
            FeatureKind::Arc,
            arc_center,
            sub(over_seed, down_pos),
            0.35,
            strength,
            over,
        );
        if self.plates[down].seeds.len() > 1 {
            self.plates[down].seeds.remove(down_seed);
        } else {
            // A lone oceanic seed cannot vanish; slab pull turns it away.
            let away = tangent(down_pos, sub(down_pos, over_seed));
            let speed = length(self.plates[down].omega).max(self.base_speed * 0.5);
            self.plates[down].omega = scale(cross(down_pos, away), speed);
        }
        self.events.push(HistoryEvent {
            time_myr: self.time,
            kind: HistoryEventKind::Subduction,
        });
    }

    fn merge(&mut self, a: usize, b: usize) {
        let nb = self.plates[b].seeds.len() as f32;
        let na = self.plates[a].seeds.len() as f32;
        let absorbed = std::mem::take(&mut self.plates[b].seeds);
        let omega = scale(
            add(
                scale(self.plates[a].omega, na),
                scale(self.plates[b].omega, nb),
            ),
            1.0 / (na + nb),
        );
        let members = self.plates[a].members + self.plates[b].members;
        let draw = self.draw(a as u32, 31);
        let plate = &mut self.plates[a];
        plate.seeds.extend(absorbed);
        plate.omega = omega;
        plate.members = members;
        plate.tenure_myr = 0.0;
        plate.breakup_after_myr = 200.0 + 150.0 * unit_hash(self.seed, draw, 32);
        for feature in self.features.iter_mut().filter(|f| f.owner == b) {
            feature.owner = a;
        }
    }

    /// Seafloor spreading: fill the largest gaps with new oceanic seeds.
    fn spread(&mut self) {
        let hole = 0.85 * self.seed_spacing;
        let mut spawned: Vec<[f32; 3]> = Vec::new();
        for _ in 0..2 {
            // Spreading replaces subducted seeds; it does not grow the total.
            if self.seed_total() >= self.target_seeds.min(MAX_SEEDS) {
                return;
            }
            let mut best: Option<([f32; 3], f32)> = None;
            for sample in &self.samples {
                let gap = self.nearest_seed_angle(*sample);
                let near_spawned = spawned
                    .iter()
                    .any(|s| angle_between(*s, *sample) < self.seed_spacing);
                if gap > hole && !near_spawned && best.is_none_or(|(_, g)| gap > g) {
                    best = Some((*sample, gap));
                }
            }
            let Some((position, _)) = best else {
                return;
            };
            spawned.push(position);
            self.spawn_oceanic_seed(position);
        }
    }

    fn spawn_oceanic_seed(&mut self, position: [f32; 3]) {
        let mut nearest: Vec<(f32, usize)> = self
            .plates
            .iter()
            .enumerate()
            .map(|(i, p)| {
                let d = p
                    .seeds
                    .iter()
                    .map(|s| angle_between(*s, position))
                    .fold(f32::MAX, f32::min);
                (d, i)
            })
            .collect();
        nearest.sort_by(|x, y| x.0.total_cmp(&y.0));
        let oceanic_neighbor = nearest
            .iter()
            .take(2)
            .find(|(_, i)| !self.plates[*i].continental)
            .map(|(_, i)| *i);
        let owner = if let Some(index) = oceanic_neighbor {
            index
        } else if self.plates.len() < MAX_PLATES && nearest.len() >= 2 {
            // A new ocean opens between two continents.
            let (ia, ib) = (nearest[0].1, nearest[1].1);
            let omega = scale(add(self.plates[ia].omega, self.plates[ib].omega), 0.5);
            let draw = self.draw(self.plates.len() as u32, 41);
            self.plates.push(SimPlate {
                seeds: Vec::new(),
                omega,
                continental: false,
                members: 0,
                tenure_myr: 0.0,
                breakup_after_myr: f32::MAX,
                next_reorganization_myr: self.time + 100.0 + 100.0 * unit_hash(self.seed, draw, 42),
            });
            self.plates.len() - 1
        } else if let Some((_, index)) = nearest.iter().find(|(_, i)| !self.plates[*i].continental)
        {
            *index
        } else {
            return;
        };
        self.plates[owner].seeds.push(position);
        self.events.push(HistoryEvent {
            time_myr: self.time,
            kind: HistoryEventKind::Spreading,
        });
    }

    /// Long-lived merged continents rift into two diverging plates.
    fn break_up(&mut self) {
        for index in 0..self.plates.len() {
            if self.plates.len() >= MAX_PLATES {
                return;
            }
            let plate = &self.plates[index];
            if !plate.continental
                || plate.members < 2
                || plate.seeds.len() < 3
                || plate.tenure_myr < plate.breakup_after_myr
            {
                continue;
            }
            let center = plate.centroid();
            let draw = self.draw(index as u32, 51);
            let mut normal = tangent(center, random_unit(self.seed, draw));
            let mut sides = self.partition(index, center, normal);
            if sides.0.is_empty() || sides.1.is_empty() {
                normal = normalize(cross(center, normal));
                sides = self.partition(index, center, normal);
            }
            if sides.0.is_empty() || sides.1.is_empty() {
                self.plates[index].breakup_after_myr += 50.0;
                continue;
            }
            let push = scale(cross(center, normal), self.base_speed * 0.8);
            let omega = self.plates[index].omega;
            let members = self.plates[index].members;
            let (plus, minus) = sides;
            self.plates[index].seeds = minus;
            self.plates[index].omega = sub(omega, push);
            self.plates[index].members = (members / 2).max(1);
            self.plates[index].tenure_myr = 0.0;
            self.plates[index].breakup_after_myr = 200.0 + 150.0 * unit_hash(self.seed, draw, 52);
            let new_index = self.plates.len();
            self.plates.push(SimPlate {
                seeds: plus,
                omega: add(omega, push),
                continental: true,
                members: (members - members / 2).max(1),
                tenure_myr: 0.0,
                breakup_after_myr: 200.0 + 150.0 * unit_hash(self.seed, draw, 53),
                next_reorganization_myr: self.time + 100.0 + 100.0 * unit_hash(self.seed, draw, 54),
            });
            for feature in self.features.iter_mut().filter(|f| f.owner == index) {
                let mid = normalize(add(feature.a, feature.b));
                if dot(sub(mid, center), normal) > 0.0 {
                    feature.owner = new_index;
                }
            }
            self.add_feature(FeatureKind::Rift, center, normal, 0.6, 0.7, new_index);
            self.events.push(HistoryEvent {
                time_myr: self.time,
                kind: HistoryEventKind::Split,
            });
        }
    }

    fn partition(
        &self,
        index: usize,
        center: [f32; 3],
        normal: [f32; 3],
    ) -> (Vec<[f32; 3]>, Vec<[f32; 3]>) {
        self.plates[index]
            .seeds
            .iter()
            .partition(|s| dot(sub(**s, center), normal) > 0.0)
    }

    /// Record a belt centered on `center`, running perpendicular to
    /// `across` (the direction between the two plates), half-length in
    /// units of the seed spacing.
    fn add_feature(
        &mut self,
        kind: FeatureKind,
        center: [f32; 3],
        across: [f32; 3],
        half_length: f32,
        strength: f32,
        owner: usize,
    ) {
        let across = tangent(center, across);
        let along = normalize(cross(center, across));
        if !along.iter().all(|c| c.is_finite()) || length(along) < 0.5 {
            return;
        }
        let half = half_length * self.spacing;
        if kind == FeatureKind::Arc {
            // A long-lived subduction zone keeps one arc and refreshes it.
            let reach = 0.5 * self.spacing;
            if let Some(existing) = self.features.iter_mut().find(|f| {
                f.kind == FeatureKind::Arc
                    && f.owner == owner
                    && angle_between(normalize(add(f.a, f.b)), center) < reach
            }) {
                existing.formed_myr = self.time;
                existing.strength = existing.strength.max(strength);
                return;
            }
        }
        let feature = HistoryFeature {
            kind,
            a: normalize(rotate(center, across, -half)),
            b: normalize(rotate(center, across, half)),
            strength,
            formed_myr: self.time,
            owner,
        };
        if self.features.len() >= MAX_FEATURES {
            // Drop the most eroded feature.
            let time = self.time;
            if let Some((weakest, _)) = self.features.iter().enumerate().min_by(|x, y| {
                let ax = x.1.strength * (-(time - x.1.formed_myr) / EROSION_EFOLD_MYR).exp();
                let ay = y.1.strength * (-(time - y.1.formed_myr) / EROSION_EFOLD_MYR).exp();
                ax.total_cmp(&ay)
            }) {
                self.features.remove(weakest);
            }
        }
        self.features.push(feature);
    }

    fn remove_empty_plates(&mut self) {
        let mut index = 0;
        while index < self.plates.len() {
            if !self.plates[index].seeds.is_empty() {
                index += 1;
                continue;
            }
            self.plates.remove(index);
            self.features.retain(|f| f.owner != index);
            for feature in &mut self.features {
                if feature.owner > index {
                    feature.owner -= 1;
                }
            }
        }
    }

    fn seed_total(&self) -> usize {
        self.plates.iter().map(|p| p.seeds.len()).sum()
    }

    fn nearest_seed_angle(&self, position: [f32; 3]) -> f32 {
        self.plates
            .iter()
            .flat_map(|p| p.seeds.iter())
            .map(|s| angle_between(*s, position))
            .fold(f32::MAX, f32::min)
    }

    /// Deterministic per-event hash input from the step counter.
    fn draw(&self, index: u32, channel: u32) -> u32 {
        self.step_index
            .wrapping_mul(7919)
            .wrapping_add(index.wrapping_mul(104_729))
            .wrapping_add(channel)
    }
}

fn unit_hash(seed: u32, index: u32, channel: u32) -> f32 {
    let mut h = seed
        .wrapping_mul(0x9E37_79B9)
        .wrapping_add(index.wrapping_mul(0x85EB_CA6B))
        .wrapping_add(channel.wrapping_mul(0xC2B2_AE35));
    h ^= h >> 16;
    h = h.wrapping_mul(0x7FEB_352D);
    h ^= h >> 15;
    h = h.wrapping_mul(0x846C_A68B);
    h ^= h >> 16;
    (h >> 8) as f32 / (1u32 << 24) as f32
}

fn random_unit(seed: u32, index: u32) -> [f32; 3] {
    let z = unit_hash(seed, index, 1) * 2.0 - 1.0;
    let azimuth = unit_hash(seed, index, 2) * std::f32::consts::TAU;
    let r = (1.0 - z * z).max(0.0).sqrt();
    [r * azimuth.cos(), z, r * azimuth.sin()]
}

fn fibonacci_points(n: usize) -> Vec<[f32; 3]> {
    let golden = (1.0 + 5.0_f32.sqrt()) / 2.0;
    (0..n)
        .map(|i| {
            let z = 1.0 - 2.0 * (i as f32 + 0.5) / n as f32;
            let r = (1.0 - z * z).max(0.0).sqrt();
            let theta = std::f32::consts::TAU * i as f32 / golden;
            [r * theta.cos(), z, r * theta.sin()]
        })
        .collect()
}

fn add(a: [f32; 3], b: [f32; 3]) -> [f32; 3] {
    [a[0] + b[0], a[1] + b[1], a[2] + b[2]]
}

fn sub(a: [f32; 3], b: [f32; 3]) -> [f32; 3] {
    [a[0] - b[0], a[1] - b[1], a[2] - b[2]]
}

fn scale(a: [f32; 3], s: f32) -> [f32; 3] {
    [a[0] * s, a[1] * s, a[2] * s]
}

fn dot(a: [f32; 3], b: [f32; 3]) -> f32 {
    a[0] * b[0] + a[1] * b[1] + a[2] * b[2]
}

fn cross(a: [f32; 3], b: [f32; 3]) -> [f32; 3] {
    [
        a[1] * b[2] - a[2] * b[1],
        a[2] * b[0] - a[0] * b[2],
        a[0] * b[1] - a[1] * b[0],
    ]
}

fn length(a: [f32; 3]) -> f32 {
    dot(a, a).sqrt()
}

fn normalize(a: [f32; 3]) -> [f32; 3] {
    let len = length(a);
    if len > 0.0 { scale(a, 1.0 / len) } else { a }
}

/// Component of `v` tangent to the sphere at `p`, normalized.
fn tangent(p: [f32; 3], v: [f32; 3]) -> [f32; 3] {
    normalize(sub(v, scale(p, dot(v, p))))
}

fn angle_between(a: [f32; 3], b: [f32; 3]) -> f32 {
    // atan2 stays accurate for nearly coincident seeds, unlike acos.
    length(cross(a, b)).atan2(dot(a, b))
}

/// Rodrigues rotation of `v` about unit `axis` by `angle`.
fn rotate(v: [f32; 3], axis: [f32; 3], angle: f32) -> [f32; 3] {
    let (sin, cos) = angle.sin_cos();
    add(
        add(scale(v, cos), scale(cross(axis, v), sin)),
        scale(axis, dot(axis, v) * (1.0 - cos)),
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::plates::{PlateGenParams, generate_plates};

    fn earth_plates(seed: u32) -> Vec<PlateGpu> {
        generate_plates(&PlateGenParams {
            seed,
            mass_earth: 1.0,
            ocean_fraction: 0.7,
            tectonics_factor: 0.85,
            continental_scale: 1.0,
            num_plates_override: 0,
            num_continents: 0,
            continent_size_variety: 0.0,
        })
    }

    fn bytes(records: &[PlateGpu]) -> &[u8] {
        bytemuck::cast_slice(records)
    }

    /// Fraction of a Fibonacci lattice owned by continental plates (nearest seed).
    fn continental_fraction(history: &TectonicHistory) -> f32 {
        let samples = fibonacci_points(4000);
        let continental = samples
            .iter()
            .filter(|s| {
                history
                    .plates
                    .iter()
                    .map(|p| {
                        let d = p
                            .seeds
                            .iter()
                            .map(|seed| 1.0 - dot(*seed, **s))
                            .fold(f32::MAX, f32::min);
                        (d, p.continental)
                    })
                    .min_by(|a, b| a.0.total_cmp(&b.0))
                    .is_some_and(|(_, c)| c)
            })
            .count();
        continental as f32 / samples.len() as f32
    }

    #[test]
    fn zero_time_returns_input_plates_bit_for_bit() {
        let plates = earth_plates(42);
        assert_eq!(bytes(&plate_records(&plates, 42, 0.0)), bytes(&plates));
        assert_eq!(plate_count(&plates), plates.len());
    }

    #[test]
    fn stagnant_lid_has_no_history() {
        let plates = generate_plates(&PlateGenParams {
            seed: 9,
            mass_earth: 1.0,
            ocean_fraction: 0.7,
            tectonics_factor: 0.0,
            continental_scale: 1.0,
            num_plates_override: 0,
            num_continents: 0,
            continent_size_variety: 0.0,
        });
        assert_eq!(bytes(&plate_records(&plates, 9, 1500.0)), bytes(&plates));
    }

    #[test]
    fn history_is_deterministic_and_stays_on_the_sphere() {
        let plates = earth_plates(42);
        let first = plate_records(&plates, 42, 1500.0);
        let second = plate_records(&plates, 42, 1500.0);
        assert_eq!(bytes(&first), bytes(&second));

        let history = simulate(&plates, 42, 1500.0);
        assert!(history.plates.len() <= MAX_PLATES);
        assert!(history.plates.iter().map(|p| p.seeds.len()).sum::<usize>() <= MAX_SEEDS);
        for plate in &history.plates {
            assert!(!plate.seeds.is_empty());
            for seed in &plate.seeds {
                assert!(
                    (length(*seed) - 1.0).abs() < 1e-4,
                    "seed off sphere: {seed:?}"
                );
            }
        }
        for feature in &history.features {
            assert!((length(feature.a) - 1.0).abs() < 1e-4);
            assert!((length(feature.b) - 1.0).abs() < 1e-4);
        }
    }

    #[test]
    fn records_round_trip_plate_seed_and_feature_layout() {
        let plates = earth_plates(42);
        let history = simulate(&plates, 42, 1500.0);
        let records = history.gpu_records();
        let count = plate_count(&records);
        assert_eq!(count, history.plates.len());
        assert!(
            records.len() > count,
            "1.5 Gyr should leave seeds or features"
        );

        let meta = records[count];
        let seed_total = meta.center[0].to_bits() as usize;
        let feature_total = meta.center[1].to_bits() as usize;
        assert_eq!(feature_total, history.features.len());
        assert_eq!(records.len(), count + 1 + seed_total + 2 * feature_total);
        for (index, plate) in records[..count].iter().enumerate() {
            let packed = plate._pad.to_bits();
            let seeds = &history.plates[index].seeds;
            if seeds.len() > 1 {
                let start = (packed & 0xffff) as usize;
                let n = (packed >> 16) as usize;
                assert_eq!(n, seeds.len());
                for (k, seed) in seeds.iter().enumerate() {
                    let record = records[count + 1 + start + k];
                    assert_eq!(record.center, *seed);
                    assert_eq!(record.plate_type, index as f32);
                }
            } else {
                assert_eq!(packed, 0);
                assert_eq!(plate.center, seeds[0]);
            }
        }
    }

    #[test]
    fn older_planets_accumulate_more_history() {
        let plates = earth_plates(42);
        let young = simulate(&plates, 42, 200.0);
        let old = simulate(&plates, 42, 2000.0);
        assert!(
            old.features.len() > young.features.len(),
            "features young {} old {}",
            young.features.len(),
            old.features.len()
        );
        assert!(old.events.len() > young.events.len());
    }

    #[test]
    fn supercontinents_assemble_and_break_up() {
        // Over the full range at least one seed sees continents merge
        // (assembly) and a merged continent split again (breakup).
        let mut cycled = 0;
        for seed in [7, 42, 101, 509] {
            let history = simulate(&earth_plates(seed), seed, MAX_GEOLOGIC_TIME_MYR);
            let merges = history
                .events
                .iter()
                .filter(|e| e.kind == HistoryEventKind::Merge)
                .count();
            let splits = history
                .events
                .iter()
                .filter(|e| e.kind == HistoryEventKind::Split)
                .count();
            let first_merge = history
                .events
                .iter()
                .find(|e| e.kind == HistoryEventKind::Merge)
                .map(|e| e.time_myr);
            let later_split = history
                .events
                .iter()
                .any(|e| e.kind == HistoryEventKind::Split && Some(e.time_myr) > first_merge);
            if merges > 0 && splits > 0 && later_split {
                cycled += 1;
            }
            let counts: Vec<usize> = history
                .continental_plate_counts
                .iter()
                .map(|c| c.1)
                .collect();
            assert!(
                counts.iter().all(|&c| c >= 1),
                "seed {seed}: continents vanished"
            );
        }
        assert!(
            cycled >= 2,
            "only {cycled}/4 seeds completed a merge-then-split cycle"
        );
    }

    #[test]
    fn continental_area_is_roughly_preserved() {
        for seed in [7, 42, 101] {
            let plates = earth_plates(seed);
            let before = continental_fraction(&simulate(&plates, seed, 0.0001));
            let after = continental_fraction(&simulate(&plates, seed, 1500.0));
            assert!(
                (after - before).abs() < 0.1,
                "seed {seed}: continental fraction {before:.3} -> {after:.3}"
            );
        }
    }

    #[test]
    fn rotation_preserves_length_and_angle() {
        let v = normalize([0.3, -0.5, 0.8]);
        let axis = normalize([0.0, 1.0, 0.2]);
        let rotated = rotate(v, axis, 0.7);
        assert!((length(rotated) - 1.0).abs() < 1e-5);
        assert!((dot(rotated, axis) - dot(v, axis)).abs() < 1e-5);
    }
}
