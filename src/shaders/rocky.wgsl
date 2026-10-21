// Rocky-world surface history: impact craters and basins, volcanic plains,
// shield volcanoes and a hemispheric dichotomy for stagnant-lid worlds, and
// the regolith colours of airless and dusty surfaces. Shared by terrain
// generation (plates.wgsl), the preview and texture export; concatenate
// after noise.wgsl. Lengths are km, angles radians on the unit sphere. Every
// field is a function of the 3D position, so cube faces, export tiles and
// the preview agree.
//
// Crater morphometry follows lunar and Martian surveys (Pike 1977, 1980):
// simple bowls are ~0.2 D deep with rims 0.036 D high. Above the simple-to-
// complex transition, which scales with 1/g (~15 km at lunar gravity, ~7 km
// on Mars, ~2.5 km on Earth), floors flatten, central peaks rise and depth
// grows only as D^0.3; past ~11 transition diameters peaks give way to peak
// rings and multi-ring basins. Ejecta thins as r^-3 (McGetchin et al. 1973).
// Crater density follows the Neukum (2001) lunar chronology, saturating on
// the oldest highlands at ~2e-4 craters >= 20 km per km² with a cumulative
// size-frequency slope of -2. Volcanic plains, lowlands and lava provinces
// are younger and bury the craters older than themselves.

struct RockyParams {
    seed: u32,
    radius_km: f32,
    gravity: f32,
    crater_age: f32,    // Gyr
    erosion: f32,       // crater degradation by air and water
    mare: f32,
    volcanism: f32,
    dichotomy: f32,
    lid: f32,
    min_crater_km: f32, // smallest crater the caller resolves
    airless: f32,
    dust: f32,
}

struct RockyCraters {
    height: f32, // km
    fresh: f32,  // immature (bright) walls and ejecta of young craters
    ray: f32,    // ray streaks of large young craters
    floor: f32,  // flat crater floors
}

struct RockyProvinces {
    mare: f32,     // dark volcanic plains flooding basins
    plains: f32,   // resurfaced lowlands of the dichotomy
    volcanic: f32, // volcanic rise and its lava plains
}

struct RockyTerrain {
    height: f32, // km
    provinces: RockyProvinces,
    fresh: f32,
    ray: f32,
}

// Craters per cell on a saturated surface, spread over RK_SLOTS candidate
// positions so placement stays close to Poisson; see rk_craters.
const RK_SATURATED_PER_CELL: f32 = 2.0;
const RK_SLOTS: u32 = 4u;
// Cumulative craters >= D per km² per D⁻² that the octaves place on a
// saturated surface: RK_SATURATED_PER_CELL per (5 D)² cell, summed over
// octaves (x4/3). Mapping Neukum's N(1) through this density keeps young,
// unsaturated surfaces at the chronology's crater count.
const RK_SATURATION_N1: f32 = RK_SATURATED_PER_CELL * 0.0533;
// Cell size in units of the octave's smallest diameter. Influence reaches
// 2.75 radii (wobbled rims fade out by 2.5), at most 0.55 cell, so a 3x3x3
// neighbourhood finds every crater.
const RK_CELL_DIAMETERS: f32 = 5.0;
const RK_MAX_BASINS: u32 = 40u;
const RK_MAX_RAYED: u32 = 12u;

fn rk_ramp(a: f32, b: f32, x: f32) -> f32 {
    let t = clamp((x - a) / (b - a), 0.0, 1.0);
    return t * t * (3.0 - 2.0 * t);
}

fn rk_pcg3d(input: vec3<u32>) -> vec3<u32> {
    var v = input * 1664525u + vec3<u32>(1013904223u);
    v.x += v.y * v.z;
    v.y += v.z * v.x;
    v.z += v.x * v.y;
    v ^= v >> vec3<u32>(16u);
    v.x += v.y * v.z;
    v.y += v.z * v.x;
    v.z += v.x * v.y;
    return v;
}

fn rk_unit3(h: vec3<u32>) -> vec3<f32> {
    return vec3<f32>(h >> vec3<u32>(8u)) / 16777216.0;
}

fn rk_rand(seed: u32, stream: u32) -> f32 {
    return rk_unit3(rk_pcg3d(vec3<u32>(seed, stream, 0x5bd1e995u))).x;
}

fn rk_direction(seed: u32, stream: u32) -> vec3<f32> {
    let r = rk_unit3(rk_pcg3d(vec3<u32>(seed, stream, 0x68e31da4u)));
    let y = r.x * 2.0 - 1.0;
    let azimuth = r.y * 6.2831853;
    let ring = sqrt(max(1.0 - y * y, 0.0));
    return vec3<f32>(ring * cos(azimuth), y, ring * sin(azimuth));
}

// Great-circle angle between unit vectors, accurate for small angles.
fn rk_angle(a: vec3<f32>, b: vec3<f32>) -> f32 {
    return 2.0 * asin(clamp(length(a - b) * 0.5, 0.0, 1.0));
}

// Rotate unit vector `origin` toward `toward` by `angle`.
fn rk_rotate_toward(origin: vec3<f32>, toward: vec3<f32>, angle: f32) -> vec3<f32> {
    let side = toward - origin * dot(toward, origin);
    let axis = side / max(length(side), 1.0e-6);
    return normalize(origin * cos(angle) + axis * sin(angle));
}

fn rk_smin(a: f32, b: f32, k: f32) -> f32 {
    if (k <= 0.0) { return min(a, b); }
    let h = max(k - abs(a - b), 0.0) / k;
    return min(a, b) - h * h * k * 0.25;
}

// Neukum (2001) cumulative lunar crater density N(>1 km) per km² at age t Gyr.
fn rk_neukum(t: f32) -> f32 {
    let age = max(t, 0.0);
    return 5.44e-14 * (exp(6.93 * age) - 1.0) + 8.38e-4 * age;
}

// Fraction of the saturated crater population a surface of age t retains.
fn rk_fill(t: f32) -> f32 {
    return 1.0 - exp(-rk_neukum(t) / RK_SATURATION_N1);
}

// Simple-to-complex transition diameter, inversely proportional to gravity.
fn rk_transition_km(prm: RockyParams) -> f32 {
    return 15.0 * 1.62 / max(prm.gravity, 0.1);
}

fn rk_area_km2(prm: RockyParams) -> f32 {
    return 12.566371 * prm.radius_km * prm.radius_km;
}

// Smallest basin diameter: above it craters come from an explicit catalogue
// of at most RK_MAX_BASINS, below it from hashed octaves.
fn rk_basin_floor_km(prm: RockyParams) -> f32 {
    let per_area = RK_SATURATION_N1 * rk_fill(prm.crater_age);
    return max(250.0, sqrt(rk_area_km2(prm) * per_area / f32(RK_MAX_BASINS)));
}

fn rk_basin_count(prm: RockyParams) -> f32 {
    let floor_km = rk_basin_floor_km(prm);
    let per_area = RK_SATURATION_N1 * rk_fill(prm.crater_age);
    return min(rk_area_km2(prm) * per_area / (floor_km * floor_km), f32(RK_MAX_BASINS));
}

// One crater's relief (km, relative to the pre-impact surface) at radius r
// in crater radii. x = height, y = inside the rim, z = flat floor, w =
// continuous ejecta weight outside the rim.
fn rk_crater_shape(r: f32, diameter: f32, transition: f32, degrade: f32) -> vec4<f32> {
    let complex_t = rk_ramp(0.8 * transition, 1.8 * transition, diameter);
    let ring_t = rk_ramp(9.0 * transition, 14.0 * transition, diameter);
    let ratio = max(diameter / transition, 1.0e-3);
    let depth = mix(0.2 * diameter, 0.157 * transition * pow(ratio, 0.301), complex_t)
        * (1.0 - 0.8 * degrade);
    let rim = mix(0.036 * diameter, 0.036 * transition * pow(ratio, 0.4), complex_t)
        * (1.0 - 0.9 * degrade);
    let floor_r = min(complex_t * (0.25 + 0.3 * rk_ramp(1.5, 12.0, ratio)) + 0.3 * degrade, 0.75);

    // Interior, continued past the rim with the wall's slope.
    var inner: f32;
    if (r < 1.0) {
        let t = clamp((r - floor_r) / max(1.0 - floor_r, 1.0e-3), 0.0, 1.0);
        var shape = mix(r * r, pow(t, 1.6), complex_t);
        shape = mix(shape, sqrt(shape), 0.5 * degrade);
        // Slumped terraces on fresh complex walls.
        shape += complex_t * (1.0 - degrade) * 0.18 * sin(t * 18.85) * t * (1.0 - t);
        inner = rim - depth * (1.0 - shape);
        let peak = depth * 0.5 * complex_t * (1.0 - ring_t) * (1.0 - 0.8 * degrade);
        inner += peak * exp(-r * r / 0.018);
        let ring_d = (r - 0.5) / 0.07;
        inner += depth * 0.3 * ring_t * (1.0 - 0.7 * degrade) * exp(-ring_d * ring_d);
    } else {
        inner = rim + 2.0 * depth * (r - 1.0);
    }
    // Ejecta blanket thinning as r^-3, continued inward above the rim.
    let outer = rim * pow(max(r, 0.5), -3.0) * (1.0 - rk_ramp(2.0, 2.5, r));
    // Degraded rims round off and floors fill with slumped debris.
    let height = rk_smin(inner, outer, (0.15 + 1.4 * degrade) * rim);
    let interior = 1.0 - rk_ramp(0.95, 1.05, r);
    let floor = complex_t * (1.0 - rk_ramp(floor_r * 0.85, floor_r + 0.02, r));
    let ejecta = (1.0 - rk_ramp(1.0, 2.3, r)) * (1.0 - interior);
    return vec4<f32>(height, interior, floor, ejecta);
}

// Burial weight of a crater of age rank `rank` (0 = youngest) by the
// provinces covering this point; each province is younger than the surface.
fn rk_buried(prov: RockyProvinces, thresholds: vec3<f32>, rank: f32) -> f32 {
    let mare = prov.mare * rk_ramp(thresholds.x - 0.01, thresholds.x + 0.01, rank);
    let plains = prov.plains * rk_ramp(thresholds.y - 0.01, thresholds.y + 0.01, rank);
    let volcanic = prov.volcanic * rk_ramp(thresholds.z - 0.01, thresholds.z + 0.01, rank);
    return max(mare, max(plains, volcanic));
}

fn rk_province_thresholds(prm: RockyParams) -> vec3<f32> {
    let total = max(rk_fill(prm.crater_age), 1.0e-6);
    return vec3<f32>(
        rk_fill(0.80 * prm.crater_age),
        rk_fill(0.84 * prm.crater_age),
        rk_fill(0.40 * prm.crater_age),
    ) / total;
}

// Rank below which a crater is younger than ~1 Gyr and still immature.
fn rk_fresh_rank(prm: RockyParams) -> f32 {
    let total = max(rk_fill(prm.crater_age), 1.0e-6);
    return min(rk_fill(min(prm.crater_age, 1.0)) / total, 1.0);
}

// Hashed crater octaves from `top_km` down to the resolved size. Each octave
// holds diameters [d, 2d) drawn from a -2 cumulative slope. `fresh_only` skips mature
// craters (the albedo needs only the bright young ones).
fn rk_craters(p: vec3<f32>, prm: RockyParams, prov: RockyProvinces, fresh_only: bool) -> RockyCraters {
    var out: RockyCraters;
    out.height = 0.0;
    out.fresh = 0.0;
    out.ray = 0.0;
    out.floor = 0.0;
    let total = rk_fill(prm.crater_age);
    if (prm.crater_age <= 0.0 || total <= 0.0) {
        return out;
    }
    let per_slot = min(RK_SATURATED_PER_CELL * total / f32(RK_SLOTS), 1.0);
    let transition = rk_transition_km(prm);
    let thresholds = rk_province_thresholds(prm);
    let fresh_rank = rk_fresh_rank(prm);
    let min_km = max(prm.min_crater_km, 0.05);
    var octave_d = rk_basin_floor_km(prm) * 0.5;
    for (var k = 0u; k < 14u; k++) {
        if (octave_d < min_km * 0.5) {
            break;
        }
        let cell_size = RK_CELL_DIAMETERS * octave_d / prm.radius_km;
        let base_cell = vec3<i32>(floor(p / cell_size));
        for (var n = 0u; n < 27u; n++) {
            let cell = base_cell + vec3<i32>(i32(n % 3u) - 1, i32((n / 3u) % 3u) - 1, i32(n / 9u) - 1);
            let key = vec3<u32>(bitcast<u32>(cell.x), bitcast<u32>(cell.y), bitcast<u32>(cell.z));
            for (var slot = 0u; slot < RK_SLOTS; slot++) {
                let h = rk_pcg3d(key ^ vec3<u32>(prm.seed * 0x9e3779b9u, k * 0x85ebca6bu + slot * 0xc2b2ae35u, 0x27d4eb2fu));
                let r2 = rk_unit3(rk_pcg3d(h.yzx ^ vec3<u32>(0x165667b1u)));
                if (r2.x >= per_slot) {
                    continue;
                }
                let rank = r2.z;
                if (fresh_only && rank > fresh_rank) {
                    continue;
                }
                let q = (vec3<f32>(cell) + rk_unit3(h)) * cell_size;
                let q_len = length(q);
                if (abs(q_len - 1.0) > 0.5 * cell_size) {
                    continue;
                }
                let center = q / q_len;
                let diameter = octave_d / sqrt(1.0 - r2.y * 0.75);
                let radius = 0.5 * diameter / prm.radius_km;
                let angle = rk_angle(p, center);
                if (angle > 2.75 * radius) {
                    continue;
                }
                let resolved = rk_ramp(min_km * 0.5, min_km, diameter);
                let local = (p - center) / radius;
                let phase = vec3<f32>(f32(h.x & 1023u), f32(h.y & 1023u), 0.0);
                let wobble = 0.06 * snoise(local * 1.1 + phase) + 0.025 * snoise(local * 3.2 + phase);
                let r = angle / radius * (1.0 + wobble);
                let young = 1.0 - rk_ramp(0.0, fresh_rank, rank);
                let degrade = clamp((sqrt(rank) * 0.9 + prm.erosion * 0.5) * (1.0 - young), 0.0, 1.0);
                let shape = rk_crater_shape(r, diameter, transition, degrade);
                let visible = (1.0 - 0.88 * rk_buried(prov, thresholds, rank)) * resolved;
                out.height += shape.x * visible;
                out.floor = max(out.floor, shape.z * visible);
                let bright = max(shape.y, shape.w * (1.2 - rk_ramp(1.0, 2.3, r)));
                out.fresh = max(out.fresh, young * young * bright * visible * (1.0 - prm.erosion));
            }
        }
        octave_d *= 0.5;
    }
    return out;
}

// Large young craters whose rays reach far beyond their cell: Tycho-like
// systems on airless worlds. x = height (km), y = fresh, z = ray.
// Rays fade within ~1 Gyr; a surface older than ~150 Myr (N(1) ~1.25e-4)
// has had time to collect the full set.
fn rk_rayed_count(prm: RockyParams) -> u32 {
    let young = min(rk_neukum(min(prm.crater_age, 1.0)) / 1.25e-4, 1.0);
    let area_scale = sqrt(rk_area_km2(prm) / 3.79e7);
    return min(u32(round(10.0 * prm.airless * area_scale * young)), RK_MAX_RAYED);
}

fn rk_rayed(p: vec3<f32>, prm: RockyParams) -> vec3<f32> {
    var out = vec3<f32>(0.0);
    let count = rk_rayed_count(prm);
    let transition = rk_transition_km(prm);
    for (var i = 0u; i < RK_MAX_RAYED; i++) {
        if (i >= count) {
            break;
        }
        let stream = 9000u + i * 8u;
        let center = rk_direction(prm.seed, stream);
        let diameter = mix(22.0, 95.0, pow(rk_rand(prm.seed, stream + 1u), 2.0))
            * sqrt(prm.radius_km / 1737.0);
        let radius = 0.5 * diameter / prm.radius_km;
        let angle = rk_angle(p, center);
        let r = angle / radius;
        if (r > 32.0) {
            continue;
        }
        let maturity = rk_rand(prm.seed, stream + 2u);
        if (r < 2.65) {
            let wobble = 0.05 * snoise((p - center) / radius * 1.1 + vec3<f32>(f32(i) * 7.3));
            out.x += rk_crater_shape(r * (1.0 + wobble), diameter, transition, 0.0).x;
        }
        // Rays: narrow azimuthal streaks broken into secondary clumps.
        let tangent = p - center * dot(p, center);
        let dir = tangent / max(length(tangent), 1.0e-6);
        let offset = vec3<f32>(f32(i) * 13.1, f32(i) * 5.7, f32(i) * 9.3);
        let streak = pow(max(snoise(dir * 6.0 + offset), 0.0), 2.0)
            + 0.8 * pow(max(snoise(dir * 17.0 + offset * 1.7), 0.0), 3.0);
        let clumps = clamp(0.5 + 0.7 * snoise(p * (4.0 / radius) + offset), 0.0, 1.0);
        let reach = mix(32.0, 12.0, maturity);
        let radial = rk_ramp(1.0, 1.4, r) * pow(max(r, 1.0), -0.6) * (1.0 - rk_ramp(reach * 0.4, reach, r));
        let strength = 1.0 - 0.6 * maturity;
        out.z = max(out.z, streak * clumps * radial * 3.0 * strength);
        let halo = (1.0 - rk_ramp(0.9, 2.6, r)) * strength;
        out.y = max(out.y, halo);
    }
    out.z = clamp(out.z, 0.0, 1.0) * prm.airless;
    out.y *= prm.airless;
    return out;
}

// Thin-crust pole: maria concentrate on one hemisphere (the lunar nearside,
// Mars' lowlands) where magma reached the surface most easily.
fn rk_mare_pole(prm: RockyParams) -> vec3<f32> {
    if (prm.dichotomy > 0.0) {
        return rk_dichotomy_center(prm);
    }
    return rk_direction(prm.seed, 31u);
}

fn rk_dichotomy_center(prm: RockyParams) -> vec3<f32> {
    // Biased toward the north, as Mars' lowlands are.
    return normalize(rk_direction(prm.seed, 41u) + vec3<f32>(0.0, 1.4, 0.0));
}

fn rk_dichotomy_radius(prm: RockyParams) -> f32 {
    let area = 0.36 * sqrt(clamp(prm.dichotomy, 0.0, 1.0));
    return acos(1.0 - 2.0 * area);
}

// Lowland mask of the crustal dichotomy: a warped cap covering up to a third
// of the planet.
fn rk_lowland(p: vec3<f32>, prm: RockyParams) -> f32 {
    if (prm.dichotomy <= 0.0) {
        return 0.0;
    }
    let off = noise_seed_offset(prm.seed, 141u);
    let warp = 0.11 * snoise(p * 1.6 + off) + 0.045 * snoise(p * 4.2 + off * 1.3)
        + 0.018 * snoise(p * 11.0 + off * 0.7);
    let a = rk_angle(p, rk_dichotomy_center(prm)) + warp;
    let edge = rk_dichotomy_radius(prm);
    return 1.0 - rk_ramp(edge - 0.045, edge + 0.045, a);
}

fn rk_relief_scale(prm: RockyParams) -> f32 {
    // Relief supported by the lithosphere scales roughly as 1/g.
    return sqrt(3.71 / max(prm.gravity, 0.5));
}

fn rk_volcanic_center(prm: RockyParams) -> vec3<f32> {
    if (prm.dichotomy > 0.0) {
        // Tharsis straddles the dichotomy boundary at low latitude: head
        // from the lowland center toward the equator, give or take 40°.
        let c = rk_dichotomy_center(prm);
        let swing = (rk_rand(prm.seed, 52u) - 0.5) * 1.4;
        let meridian = normalize(vec3<f32>(c.x, 0.0, c.z) + vec3<f32>(1.0e-4, 0.0, 0.0));
        let east = normalize(cross(vec3<f32>(0.0, 1.0, 0.0), meridian));
        let toward = normalize(meridian * cos(swing) + east * sin(swing) - vec3<f32>(0.0, 0.3, 0.0));
        return rk_rotate_toward(c, toward, rk_dichotomy_radius(prm));
    }
    return rk_direction(prm.seed, 51u);
}

fn rk_rise_radius(prm: RockyParams) -> f32 {
    return 0.55 * sqrt(clamp(prm.volcanism, 0.0, 1.0)) + 0.1;
}

fn rk_volcanic_province(p: vec3<f32>, prm: RockyParams) -> f32 {
    if (prm.volcanism <= 0.0) {
        return 0.0;
    }
    let off = noise_seed_offset(prm.seed, 151u);
    let a = rk_angle(p, rk_volcanic_center(prm)) + 0.12 * snoise(p * 2.3 + off);
    let radius = rk_rise_radius(prm);
    return (1.0 - rk_ramp(0.85 * radius, 1.3 * radius, a)) * sqrt(prm.volcanism);
}

struct RockyShield {
    center: vec3<f32>,
    height_km: f32,
    radius: f32, // angular base radius
}

fn rk_shield_count(prm: RockyParams) -> u32 {
    if (prm.volcanism <= 0.0) {
        return 0u;
    }
    return 1u + u32(floor(prm.volcanism * 4.99));
}

// Olympus-like giant at the rise edge, a Tharsis Montes chain across it,
// then Elysium-like outliers.
fn rk_shield(prm: RockyParams, i: u32) -> RockyShield {
    var s: RockyShield;
    let rise = rk_volcanic_center(prm);
    let rise_r = rk_rise_radius(prm);
    let along = rk_direction(prm.seed, 61u);
    let scale = prm.volcanism * rk_relief_scale(prm) * rk_relief_scale(prm);
    if (i == 0u) {
        s.center = rk_rotate_toward(rise, rk_direction(prm.seed, 62u), 0.85 * rise_r);
        s.height_km = 21.0 * scale;
    } else if (i <= 3u) {
        let step = (f32(i) - 2.0) * 0.38 * rise_r;
        s.center = rk_rotate_toward(rise, along, step);
        if (abs(step) < 1.0e-4) {
            s.center = rise;
        }
        s.height_km = (13.0 + 3.0 * rk_rand(prm.seed, 70u + i)) * scale;
    } else {
        s.center = rk_direction(prm.seed, 80u + i);
        s.height_km = (8.0 + 5.0 * rk_rand(prm.seed, 90u + i)) * scale;
    }
    // Hawaiian-to-Martian shields: ~300 km base radius for 21 km of height.
    s.radius = (s.height_km / 21.0) * 300.0 / prm.radius_km / max(rk_relief_scale(prm), 0.5);
    s.height_km = min(s.height_km, 0.012 * prm.radius_km);
    return s;
}

fn rk_shield_height(p: vec3<f32>, s: RockyShield, i: u32) -> f32 {
    let r = rk_angle(p, s.center) / max(s.radius, 1.0e-5);
    if (r >= 1.0) {
        return 0.0;
    }
    var h = s.height_km * pow(1.0 - pow(r, 1.2), 1.4);
    // Nested summit caldera.
    let cr = r / 0.1;
    h -= 0.1 * s.height_km * (1.0 - rk_ramp(0.8, 1.0, cr));
    // Radial lava flows on the flanks.
    let tangent = p - s.center * dot(p, s.center);
    let dir = tangent / max(length(tangent), 1.0e-6);
    h += 0.025 * s.height_km * snoise(dir * 24.0 + vec3<f32>(f32(i) * 3.1)) * r * (1.0 - r);
    return h;
}

// Rift canyon radial to a large volcanic rise, cracked by its load
// (Valles Marineris: ~4000 km long, up to ~600 km wide and 7-10 km deep),
// with a parallel side chasma. x = depth (km, positive down), y = floor.
fn rk_rift(p: vec3<f32>, prm: RockyParams) -> vec2<f32> {
    let strength = rk_ramp(0.3, 0.8, prm.volcanism);
    if (strength <= 0.0) {
        return vec2<f32>(0.0);
    }
    let rise = rk_volcanic_center(prm);
    let rise_r = rk_rise_radius(prm);
    // Run roughly along the rise's latitude circle, away from the lowlands.
    let east = normalize(cross(vec3<f32>(0.0, 1.0, 0.0), rise) + vec3<f32>(0.0, 0.0, 1.0e-4));
    let heading = select(-1.0, 1.0, rk_rand(prm.seed, 53u) < 0.5);
    let start = rk_rotate_toward(rise, east * heading, 0.25 * rise_r);
    let length_rad = min(4000.0 / prm.radius_km, 1.4) * strength;
    let end = rk_rotate_toward(start, east * heading, length_rad);
    let normal = normalize(cross(start, end));
    let off = noise_seed_offset(prm.seed, 201u);
    let across = asin(clamp(dot(p, normal), -1.0, 1.0))
        + 0.012 * snoise(p * 9.0 + off) + 0.005 * snoise(p * 31.0 + off * 1.7);
    let projected = normalize(p - normal * dot(p, normal));
    let along = atan2(dot(cross(start, projected), normal), dot(start, projected)) / length_rad;
    if (along < -0.1 || along > 1.1) {
        return vec2<f32>(0.0);
    }
    let taper = rk_ramp(-0.05, 0.12, along) * (1.0 - rk_ramp(0.8, 1.05, along));
    let width = (60.0 + 220.0 * sin(clamp(along, 0.0, 1.0) * 3.14159)) / prm.radius_km;
    let depth_km = 8.0 * strength * rk_relief_scale(prm) * taper;
    let main = 1.0 - rk_ramp(0.55, 1.0, abs(across) / max(width * taper, 1.0e-5));
    let side_offset = 2.2 * width;
    let side_t = rk_ramp(0.2, 0.35, along) * (1.0 - rk_ramp(0.55, 0.7, along));
    let side = (1.0 - rk_ramp(0.5, 1.0, abs(across - side_offset) / max(0.35 * width, 1.0e-5)))
        * side_t;
    let trough = max(main, 0.7 * side);
    return vec2<f32>(depth_km * trough, rk_ramp(0.9, 1.0, trough) * taper);
}

// Volcanic relief in km: the broad rise, its shields and rift.
fn rk_volcanic_height(p: vec3<f32>, prm: RockyParams) -> f32 {
    if (prm.volcanism <= 0.0) {
        return 0.0;
    }
    let rise_r = rk_rise_radius(prm);
    let a = rk_angle(p, rk_volcanic_center(prm)) / rise_r;
    var h = 8.0 * prm.volcanism * rk_relief_scale(prm) * exp(-2.0 * a * a);
    h -= rk_rift(p, prm).x;
    let count = rk_shield_count(prm);
    for (var i = 0u; i < 6u; i++) {
        if (i >= count) {
            break;
        }
        h += rk_shield_height(p, rk_shield(prm, i), i);
    }
    return h;
}

// Basin catalogue entry: center, diameter (km), flood level as a fraction of
// depth (0 = dry), degradation.
struct RockyBasin {
    center: vec3<f32>,
    diameter: f32,
    flood: f32,
    degrade: f32,
}

fn rk_basin(prm: RockyParams, i: u32) -> RockyBasin {
    var b: RockyBasin;
    let stream = 7000u + i * 8u;
    b.center = rk_direction(prm.seed, stream);
    let floor_km = rk_basin_floor_km(prm);
    let top_km = max(1.45 * prm.radius_km, floor_km * 1.01);
    let u = rk_rand(prm.seed, stream + 1u);
    let span = 1.0 - (floor_km * floor_km) / (top_km * top_km);
    b.diameter = floor_km / sqrt(1.0 - u * span);
    b.degrade = 0.25 + 0.6 * rk_rand(prm.seed, stream + 2u);
    let bias = 0.2 + 0.8 * rk_ramp(-0.3, 0.75, dot(b.center, rk_mare_pole(prm)));
    let flood_chance = clamp(prm.mare * 4.0 * bias * bias, 0.0, 1.0);
    b.flood = select(0.0, 0.55 + 0.4 * rk_rand(prm.seed, stream + 4u),
        rk_rand(prm.seed, stream + 3u) < flood_chance);
    return b;
}

// Basin depth (km): nearly independent of gravity, ~5-6 km for a 1000 km
// lunar basin and ~7 km for Hellas, shallower as basins relax with age.
fn rk_basin_depth(diameter: f32, degrade: f32) -> f32 {
    return 1.1 * pow(diameter, 0.25) * (1.0 - 0.3 * degrade);
}

// Multi-ring basin relief (km) at radius r in basin radii: flat floor, inner
// peak ring, main rim and an outer scarp ring.
fn rk_basin_shape(r: f32, diameter: f32, degrade: f32) -> f32 {
    let depth = rk_basin_depth(diameter, degrade);
    let rim = depth * 0.3 * (1.0 - 0.5 * degrade);
    var inner: f32;
    if (r < 1.0) {
        inner = -depth + (depth + rim) * pow(rk_ramp(0.42, 1.0, r), 1.3);
        let ring_d = (r - 0.55) / 0.05;
        inner += 0.22 * depth * (1.0 - 0.6 * degrade) * exp(-ring_d * ring_d);
    } else {
        inner = rim + 2.0 * depth * (r - 1.0);
    }
    let scarp_d = (r - 1.4) / 0.06;
    let outer = rim * pow(max(r, 0.5), -2.5) * (1.0 - rk_ramp(1.8, 2.3, r))
        + 0.1 * depth * exp(-scarp_d * scarp_d);
    return rk_smin(inner, outer, (0.3 + 0.5 * degrade) * rim);
}

// Basin relief (km) with maria filling flooded basins; y = mare mask.
// Later catalogue entries are younger: a basin excavates most of the relief
// of older basins inside its cavity instead of stacking on it, and lowland
// plains bury basins as they bury Utopia and Isidis on Mars.
fn rk_basins(p: vec3<f32>, prm: RockyParams, lowland: f32) -> vec2<f32> {
    var out = vec2<f32>(0.0);
    if (prm.crater_age <= 0.0) {
        return out;
    }
    let count = rk_basin_count(prm);
    let shore = 0.6 * snoise(p * 5.0 + noise_seed_offset(prm.seed, 161u))
        + 0.3 * snoise(p * 14.0 + noise_seed_offset(prm.seed, 162u))
        + 0.12 * snoise(p * 38.0 + noise_seed_offset(prm.seed, 163u));
    for (var i = 0u; i < RK_MAX_BASINS; i++) {
        if (f32(i) >= count) {
            break;
        }
        let b = rk_basin(prm, i);
        let radius = 0.5 * b.diameter / prm.radius_km;
        let r = rk_angle(p, b.center) / radius;
        if (r > 2.3) {
            continue;
        }
        let shape = rk_basin_shape(r, b.diameter, b.degrade);
        out.x *= 1.0 - 0.6 * (1.0 - rk_ramp(0.7, 1.0, r));
        if (b.flood > 0.0) {
            let depth = rk_basin_depth(b.diameter, b.degrade);
            let level = -depth * (1.0 - b.flood);
            // Lava fills the basin up to an irregular shore inside its rim,
            // embaying the ring structure and craters on the walls.
            let reach = 0.62 + 0.3 * b.flood + 0.16 * shore;
            let flooded = 1.0 - rk_ramp(reach - 0.03, reach + 0.03, r);
            out.x += mix(shape, max(shape, level), flooded);
            out.y = max(out.y, flooded);
        } else {
            out.x += shape;
        }
    }
    out.x *= 1.0 - 0.75 * lowland;
    return out;
}

// Irregular volcanic plains beyond the basins on the thin-crust hemisphere,
// like Oceanus Procellarum: lava overflowing into low ground between basins.
fn rk_mare_plains(p: vec3<f32>, prm: RockyParams) -> f32 {
    if (prm.mare <= 0.0) {
        return 0.0;
    }
    let off = noise_seed_offset(prm.seed, 211u);
    let n = snoise(p * 1.6 + off) * 0.6 + snoise(p * 4.2 + off * 1.3) * 0.3
        + snoise(p * 10.5 + off * 0.7) * 0.15 + snoise(p * 27.0 + off * 1.9) * 0.06;
    let value = n + 0.55 * dot(p, rk_mare_pole(prm));
    let threshold = 0.95 - 1.9 * prm.mare;
    return rk_ramp(threshold, threshold + 0.05, value);
}

fn rk_provinces(p: vec3<f32>, prm: RockyParams, mare: f32, lowland: f32) -> RockyProvinces {
    var prov: RockyProvinces;
    prov.mare = max(mare, rk_mare_plains(p, prm));
    prov.plains = lowland;
    prov.volcanic = rk_volcanic_province(p, prm);
    return prov;
}

// Long-wavelength crustal roughness (km) between the impact features.
fn rk_base_noise(p: vec3<f32>, prm: RockyParams) -> f32 {
    let off = noise_seed_offset(prm.seed, 171u);
    var value = 0.0;
    var amp = 1.0;
    var freq = 1.3;
    for (var i = 0u; i < 5u; i++) {
        value += amp * snoise(p * freq + off);
        freq *= 2.1;
        amp *= 0.5;
    }
    return value * 0.9 * rk_relief_scale(prm);
}

struct RockyStructure {
    height: f32, // endogenic relief (km): crust, dichotomy, volcanoes
    basins: f32, // impact basin relief including mare fill (km)
    provinces: RockyProvinces,
}

// Crustal structure and the basin catalogue. Plate worlds blend the
// endogenic part by the lid; impacts apply in full.
fn rk_structure(p: vec3<f32>, prm: RockyParams) -> RockyStructure {
    var s: RockyStructure;
    let lowland = rk_lowland(p, prm);
    let basins = rk_basins(p, prm, lowland);
    s.basins = basins.x;
    s.provinces = rk_provinces(p, prm, basins.y, lowland);
    let mare = s.provinces.mare;
    let smoothing = (1.0 - 0.7 * lowland) * (1.0 - 0.85 * mare)
        * (1.0 - 0.5 * s.provinces.volcanic);
    let dichotomy_km = 5.5 * prm.dichotomy * rk_relief_scale(prm) * (0.35 - lowland);
    // Plains lava outside basins ponds ~1-2 km below the highlands.
    let ponded = -1.5 * max(mare - basins.y, 0.0);
    s.height = rk_base_noise(p, prm) * smoothing + dichotomy_km + rk_volcanic_height(p, prm)
        + ponded;
    return s;
}

// Impact relief (km): basins, the crater octaves and the large rayed craters.
fn rk_impacts(p: vec3<f32>, prm: RockyParams, s: RockyStructure) -> RockyCraters {
    var out = rk_craters(p, prm, s.provinces, false);
    let rayed = rk_rayed(p, prm);
    out.height += s.basins + rayed.x;
    out.fresh = max(out.fresh, rayed.y);
    out.ray = rayed.z;
    return out;
}

// Surface units for colouring: provinces plus fresh ejecta and rays, without
// the mature crater relief.
fn rk_material(p: vec3<f32>, prm: RockyParams) -> RockyTerrain {
    var t: RockyTerrain;
    let lowland = rk_lowland(p, prm);
    let basins = rk_basins(p, prm, lowland);
    t.provinces = rk_provinces(p, prm, basins.y, lowland);
    t.height = 0.0;
    // Bright halos of young craters stay visible a few octaves below the
    // relief's resolution.
    var fine = prm;
    fine.min_crater_km = prm.min_crater_km * 0.3;
    let craters = rk_craters(p, fine, t.provinces, true);
    let rayed = rk_rayed(p, prm);
    t.fresh = max(craters.fresh, rayed.y);
    t.ray = rayed.z;
    return t;
}

// Linear albedo of rocky regolith. Airless bodies: space-weathered gray
// highlands (~0.14 at 550 nm), dark maria (~0.07, titanium-rich ones
// bluer), immature crater ejecta and rays about twice as bright and less
// red. Close to the star (Mercury) iron-poor volcanic plains are brighter
// than the highlands instead of dark maria.
// Air-bearing dusty worlds: Mars-like oxidized dust (~0.30 red / 0.16
// green / 0.075 blue) over darker basalt and sand that collects on crater
// floors. `slope` is the surface gradient (rise over run), `valley` a
// 0-1 concavity, `height_km` the elevation.
fn rk_albedo(
    p: vec3<f32>, prm: RockyParams, units: RockyTerrain, slope: f32, valley: f32,
    height_km: f32, insolation: f32,
) -> vec3<f32> {
    let off = noise_seed_offset(prm.seed, 181u);
    let regional = snoise(p * 2.2 + off) * 0.6 + snoise(p * 6.5 + off * 1.3) * 0.4;
    let grain = snoise(p * 90.0 + off) * 0.6 + snoise(p * 260.0 + off) * 0.4;
    let titanium = rk_ramp(-0.4, 0.6, snoise(p * 3.1 + off * 0.7));
    let mare = units.provinces.mare;
    let fresh = clamp(units.fresh + units.ray * 0.8, 0.0, 1.0);
    let steep = rk_ramp(0.04, 0.3, slope);
    let hot = rk_ramp(1.5, 6.7, insolation);

    // Space-weathered regolith (Moon, Mercury). On Mercury dark, bluish
    // low-reflectance material excavated from depth mottles the
    // intercrater terrain and the smooth volcanic plains are brighter and
    // redder (MESSENGER's LRM and high-reflectance red plains).
    let weathered = vec3<f32>(0.160, 0.145, 0.128) * (1.0 + 0.08 * regional) * (1.0 - 0.05 * hot);
    let lrm_field = snoise(p * 1.8 + off * 1.9) * 0.7 + snoise(p * 5.0 + off * 0.4) * 0.3;
    let lrm = hot * rk_ramp(0.05, 0.6, lrm_field);
    let highland = weathered * mix(vec3<f32>(1.0), vec3<f32>(0.72, 0.75, 0.82), lrm);
    let basalt = mix(vec3<f32>(0.090, 0.080, 0.068), vec3<f32>(0.060, 0.058, 0.056), titanium);
    let plains = mix(basalt * (1.0 + 0.08 * regional), weathered * vec3<f32>(1.10, 1.05, 0.99), hot);
    var lunar = mix(highland, plains, mare);
    // Steep walls shed weathered soil and expose brighter rock; floors
    // collect darker fines.
    lunar *= (1.0 + 0.35 * steep) * (1.0 - 0.12 * valley);
    let immature = mix(vec3<f32>(0.29, 0.28, 0.265), vec3<f32>(0.15, 0.148, 0.145), mare * (1.0 - hot) * 0.7);
    lunar = mix(lunar, immature * (1.0 - 0.15 * hot), fresh);

    // Oxidized regolith (Mars): dust settles on high, smooth volcanic and
    // lowland plains; rough cratered highlands and crater floors stay dark.
    let off2 = noise_seed_offset(prm.seed, 182u);
    // Wind smears albedo features east-west: warp, then stretch along latitude.
    let warp = vec3<f32>(snoise(p * 2.0 + off2 * 0.3), snoise(p * 2.0 + off2 * 0.5),
        snoise(p * 2.0 + off2 * 0.9)) * 0.12;
    let q = (p + warp) * vec3<f32>(0.75, 1.5, 0.75);
    let dust_field = 0.35 + 0.55 * snoise(q * 1.4 + off2) + 0.3 * snoise(q * 3.8 + off2 * 1.7)
        + 0.15 * snoise(q * 9.0 + off2 * 0.6)
        + 0.45 * units.provinces.volcanic + 0.25 * units.provinces.plains
        + 0.04 * height_km - 0.1 * mare;
    let cover = clamp(prm.dust * rk_ramp(0.0, 0.6, dust_field), 0.0, 1.0)
        * (1.0 - 0.4 * valley) * (1.0 - 0.5 * steep);
    let dust_color = vec3<f32>(0.30, 0.165, 0.078) * (1.0 + 0.08 * regional);
    let dark = mix(vec3<f32>(0.115, 0.083, 0.062), vec3<f32>(0.10, 0.076, 0.06), mare);
    var oxidized = mix(dark * (1.0 + 0.1 * regional), dust_color, cover);
    oxidized *= 1.0 + 0.15 * steep * (1.0 - cover);

    let color = mix(oxidized, lunar, prm.airless);
    return clamp(color * (1.0 + 0.04 * grain), vec3<f32>(0.01), vec3<f32>(0.6));
}

// Polar frost on cold worlds whose atmosphere moves volatiles: a residual
// water-ice cap sized by the frozen inventory and a seasonal (CO2-like)
// cap reaching toward the winter pole. Mars: residual caps to ~80-85°,
// seasonal frost to ~55-60° in winter. Airless worlds keep their ice in
// permanently shadowed craters, invisible at this scale.
fn rk_frost(
    p: vec3<f32>, prm: RockyParams, latitude: f32, tilt: f32, season: f32,
    pole_temp_c: f32, water_ice: f32,
) -> f32 {
    let air = 1.0 - prm.airless;
    if (air <= 0.0) {
        return 0.0;
    }
    let off = noise_seed_offset(prm.seed, 191u);
    let colat = 90.0 - abs(degrees(latitude));
    let wobble = 2.5 * snoise(p * 5.0 + off) + 1.0 * snoise(p * 13.0 + off * 1.3);
    let residual_r = min(4.0 + 80.0 * water_ice, 25.0) * rk_ramp(-15.0, -40.0, pole_temp_c);
    let winter = clamp(select(season - 0.5, 0.5 - season, latitude > 0.0) * 2.0, 0.0, 1.0)
        * min(abs(degrees(tilt)) / 25.0, 1.0);
    let seasonal_r = (8.0 + 27.0 * winter) * rk_ramp(-60.0, -100.0, pole_temp_c);
    var residual = 1.0 - rk_ramp(residual_r - 1.5, residual_r + 1.5, colat + wobble);
    // Spiral troughs cut the residual cap (Mars' north polar layered
    // deposits): sun-facing scarps sublimate into curved dark lanes.
    // Azimuth about the same (tilted) pole the latitude is measured from.
    let across = vec3<f32>(0.0, -sin(tilt), cos(tilt));
    let azimuth = atan2(dot(p, across), p.x) * select(-1.0, 1.0, latitude > 0.0);
    let spiral = sin(3.0 * (azimuth + 2.2 * log(max(colat, 0.5))) + off.x);
    residual *= 1.0 - 0.6 * rk_ramp(0.55, 0.95, spiral) * rk_ramp(1.0, 3.0, colat);
    let seasonal = 1.0 - rk_ramp(seasonal_r - 3.0, seasonal_r + 3.0, colat + wobble);
    return max(residual * step(0.5, residual_r), 0.8 * seasonal * step(0.5, seasonal_r)) * air;
}

// Lunar-Lambert photometry (McEwen 1991, 1996) replacing Lambert's mu0:
// Lommel-Seeliger behaviour near opposition, so a full Moon is evenly
// bright to its limb, fading to Lambert at high phase; plus the opposition
// surge of shadow hiding (Hapke 1986).
fn rk_lunar_lambert(mu0: f32, mu: f32, phase_deg: f32) -> f32 {
    let a = clamp(phase_deg, 0.0, 180.0);
    let l = clamp(1.0 - 0.019 * a + 2.42e-4 * a * a - 1.46e-6 * a * a * a, 0.0, 1.0);
    let lommel = 2.0 * mu0 / max(mu0 + mu, 1.0e-4);
    let surge = 1.0 + 0.4 / (1.0 + tan(radians(a) * 0.5) / 0.06);
    return (l * lommel + (1.0 - l) * mu0) * surge;
}
