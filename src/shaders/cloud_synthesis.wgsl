// Cloud synthesis: Earth-like cloud structure on top of the weather solver.
// Concatenate after noise.wgsl, climate.wgsl and cloud_density.wgsl. Each stage
// declares `var<uniform> cloud_synthesis: CloudSynthesisData` at its own
// binding, next to the shared weather textures.
//
// The solver resolves condensate on a coarse grid and has no weather systems,
// so on its own every region shows the same wind-streaked texture. Synthesis
// rebuilds each column's cloud from three scales:
//
//  1. Synoptic systems (`cloud_synthesis.systems`, built by
//     `cloud_synthesis.rs`): extratropical cyclones as an open frontal wave
//     (cold-front band, warm-front shield, warm and cold sectors) wound up by
//     their own circulation into a comma and, when occluded, a spiral with a
//     dry slot; tropical cyclones with eye, dense overcast and rainbands.
//  2. Climatological regimes from the circulation belts, surface and
//     temperature: ITCZ convective clusters, trade cumulus, west-coast
//     stratocumulus decks, storm-track open cells and stratiform, polar
//     stratus, dry subtropical land, jet-stream cirrus. The solver's own
//     condensate modulates coverage regionally and adds its convective storms
//     and ice.
//  3. Mesoscale morphology: fBm in three bands (synoptic, mesoscale, fine;
//     fractal edges with the area-perimeter dimension of real clouds),
//     sheared zonally, domain-warped and advected through a steady flow of
//     the systems' circulation and the storm-track jets (cs_advect), blended
//     with Worley closed and open cells, cumulus "flowers" and convective
//     systems.
//
// Coverage is a two-level probability remap: the macro field is thresholded
// at the quantile of cover / fill (where decks, cells and clusters are), the
// fine band at the quantile of the fill (how solid each is), and structure
// finer than the pixel turns into edge softness or brightness instead of
// aliasing. The expected cloud fraction therefore does not depend on
// resolution: a 2K and an 8K map agree once averaged, the 8K one resolving
// cells, clusters and filaments the 2K one shows as gray.
//
// Output is the column's low, deep and high mass in solver units: physical
// optical depth is mapped back through the preview/bridge curve
// (tau = 36 (d - 0.05) t^0.6), so every existing consumer of the masses and
// of `cloud_density.exr` renders the synthesized cloud unchanged.

const CS_MAX_SYSTEMS: u32 = 48u;
const CS_OCTAVES: i32 = 11;
// Largest fBm feature, km; each octave halves it, down to ~2.3 km.
const CS_LARGEST_KM: f32 = 2400.0;
// First octaves of the mesoscale (150-37 km) and fine (19-2.3 km) bands.
const CS_MID_OCTAVE: i32 = 4;
const CS_FINE_OCTAVE: i32 = 7;
// fBm gain for the cloud boundary's area-perimeter dimension D ~ 1.35
// (Lovejoy 1982), i.e. Hurst exponent 0.65: gain 2^-0.65. The largest octave
// is damped (regimes and systems own that scale); 1200-300 km stay full.
const CS_GAIN: f32 = 0.64;
// Standard deviation of one `snoise` octave (measured over 400k samples).
const CS_SIMPLEX_SIGMA: f32 = 0.373;
// Mean and standard deviation of the cs_blobs fields below on a sphere slice
// of unit cells (measured offline, ragged = 0).
const CS_MCS_STATS: vec2<f32> = vec2<f32>(0.117, 0.226);
const CS_FLOWER_STATS: vec2<f32> = vec2<f32>(0.149, 0.260);
// Meridional compression of the noise domain by the zonal jets.
const CS_ZONAL_STRETCH: f32 = 2.2;
const CS_CIRRUS_STRETCH: f32 = 6.0;

struct CloudSystem {
    center: vec4<f32>,
    frame: vec4<f32>,
    shape: vec4<f32>,
    extra: vec4<f32>,
}

struct CloudSynthesisData {
    control: vec4<f32>,
    climate: vec4<f32>,
    systems: array<CloudSystem, 48>,
}

fn cs_strength() -> f32 {
    return clamp(cloud_synthesis.control.x, 0.0, 1.0);
}

// Logistic approximation of the standard normal CDF and its exact inverse.
fn cs_logistic(x: f32) -> f32 {
    return 1.0 / (1.0 + exp(-1.702 * clamp(x, -30.0, 30.0)));
}

fn cs_probit(p: f32) -> f32 {
    let q = clamp(p, 1.0e-4, 1.0 - 1.0e-4);
    return log(q / (1.0 - q)) / 1.702;
}

// Circulation pole, matching `climate_latitude` and the wind field.
fn cs_pole() -> vec3<f32> {
    return vec3<f32>(0.0, cos(uniforms.axial_tilt_rad), sin(uniforms.axial_tilt_rad));
}

fn cs_rotate(v: vec3<f32>, axis: vec3<f32>, angle: f32) -> vec3<f32> {
    let c = cos(angle);
    let s = sin(angle);
    return v * c + cross(axis, v) * s + axis * dot(axis, v) * (1.0 - c);
}

fn cs_pcg3d(input: vec3<u32>) -> vec3<u32> {
    var v = input * 1664525u + 1013904223u;
    v.x += v.y * v.z;
    v.y += v.z * v.x;
    v.z += v.x * v.y;
    v ^= v >> vec3<u32>(16u);
    v.x += v.y * v.z;
    v.y += v.z * v.x;
    v.z += v.x * v.y;
    return v;
}

// Worley F1, F2 and a random value (0-1) owned by the nearest feature point.
fn cs_worley(p: vec3<f32>, seed: u32) -> vec3<f32> {
    let cell = floor(p);
    let local = p - cell;
    let base = vec3<i32>(cell);
    var f1 = 9.0;
    var f2 = 9.0;
    var id = 0.0;
    for (var z = -1; z <= 1; z++) {
        for (var y = -1; y <= 1; y++) {
            for (var x = -1; x <= 1; x++) {
                let offset = vec3<i32>(x, y, z);
                let h = cs_pcg3d(bitcast<vec3<u32>>(base + offset)
                    ^ vec3<u32>(seed, seed * 3u + 1u, seed * 7u + 2u));
                let jitter = vec3<f32>(h & vec3<u32>(0xffffu)) / 65535.0;
                let d = vec3<f32>(offset) + jitter - local;
                let distance_sq = dot(d, d);
                if (distance_sq < f1) {
                    f2 = f1;
                    f1 = distance_sq;
                    id = f32(h.x >> 16u) / 65535.0;
                } else if (distance_sq < f2) {
                    f2 = distance_sq;
                }
            }
        }
    }
    return vec3<f32>(sqrt(f1), sqrt(f2), id);
}

// Union of random elliptical blobs, one per firing cell (probability
// `firing`), radius `radius.x`-`radius.y` cells, soft inner core `inner`.
// Every neighbour cell contributes, so a blob reaching past its own Voronoi
// cell is never cut along the boundary. Blobs two cells away are never
// evaluated, so each one fades out before one cell from its centre;
// otherwise elongated or swollen blobs would be cut along the lattice planes.
// `ragged` > 0 swells a blob and < 0 shrinks it. It must follow the noise the
// blob is added to: with the opposite sign, blobs eroded exactly where the
// noise rose around them and read as rings (the "soap bubbles"). Callers use
// mostly the fine band, which frays the edges without doubling the bands
// the macro shape already has.
fn cs_blobs(p: vec3<f32>, seed: u32, firing: f32, radius: vec2<f32>, inner: f32, ragged: f32) -> f32 {
    let cell = floor(p);
    let local = p - cell;
    let base = vec3<i32>(cell);
    var clear = 1.0;
    for (var z = -1; z <= 1; z++) {
        for (var y = -1; y <= 1; y++) {
            for (var x = -1; x <= 1; x++) {
                let offset = vec3<i32>(x, y, z);
                let h = cs_pcg3d(bitcast<vec3<u32>>(base + offset)
                    ^ vec3<u32>(seed, seed * 3u + 1u, seed * 7u + 2u));
                let id = f32(h.x >> 16u) / 65535.0;
                if (id >= firing) { continue; }
                let jitter = vec3<f32>(h & vec3<u32>(0xffffu)) / 65535.0;
                let shape = f32(h.y >> 16u) / 65535.0;
                let d = vec3<f32>(offset) + jitter - local;
                // Elongate along a random axis, up to 1.8:1.
                let axis = normalize(vec3<f32>(h.zxy >> vec3<u32>(16u)) / 32767.5 - 1.0 + vec3<f32>(1.0e-3));
                let along = dot(d, axis);
                let stretched = d + axis * along * (1.0 / (1.0 + 0.8 * shape) - 1.0);
                let r = mix(radius.x, radius.y, fract(id * 7.31 + shape));
                let distance = length(stretched) * max(1.0 - ragged, 0.6);
                let reach = 1.0 - smooth_step(0.8, 1.0, length(d));
                // Probabilistic union: overlapping blobs merge without creases.
                clear *= 1.0 - reach * (1.0 - smooth_step(r * inner, r, distance));
            }
        }
    }
    return 1.0 - clear;
}

// Share of a noise octave with lattice spacing `size` (domain units) the
// pixel resolves; simplex features are about half the spacing. Unresolved
// octaves fall back to their mean and become edge softness instead.
fn cs_resolved(footprint: f32, size: f32) -> f32 {
    return 1.0 - smooth_step(0.15, 0.5, footprint / size);
}

// The same for Worley cells of spacing `size`, whose walls are far thinner.
fn cs_cells_resolved(footprint: f32, size: f32) -> f32 {
    return 1.0 - smooth_step(0.06, 0.2, footprint / size);
}

// Compresses the domain along the circulation pole so features elongate
// east-west, most at mid latitudes and not at all at the poles. The added
// offset w(y) has slope (1 - y^2)(0.3 + 2 y^2), y = sin(latitude), which
// vanishes at the poles and keeps the map seamless there.
fn cs_zonal_stretch(q: vec3<f32>, pole: vec3<f32>, stretch: f32) -> vec3<f32> {
    // Largest meridional compression: see cs_stretch_footprint.
    let y = clamp(dot(q, pole), -1.0, 1.0);
    let y2 = y * y;
    let w = y * (0.3 + y2 * (0.56666667 - 0.4 * y2));
    return q + pole * ((stretch - 1.0) * w);
}

// Pixel footprint in a stretched domain: the compression peaks at
// 1 + 0.661 (stretch - 1), where (1 - y^2)(0.3 + 2 y^2) is largest.
fn cs_stretch_footprint(footprint: f32, stretch: f32) -> f32 {
    return footprint * (1.0 + 0.661 * (stretch - 1.0));
}

// ---------------------------------------------------------------------------
// Synoptic systems

struct CsTemplate {
    // Influence of the system at this point (0-1).
    env: f32,
    // Cloud fraction the system sets here.
    cover: f32,
    // In-cloud optical depth relative to a 14 tau stratiform deck.
    thick: f32,
    // Share of the liquid column in the deep family.
    deep: f32,
    // Upper cloud fraction (warm conveyor outflow, eyewall outflow).
    cirrus: f32,
    // Morphology weights: stratiform, closed cells, open cells, cumulus.
    strat: f32,
    closed: f32,
    open: f32,
    cu: f32,
    // How strongly the system's own structure (shield, fronts, sectors, dry
    // slot) sets the cloud here; elsewhere the background regime holds.
    feature: f32,
}

fn cs_empty_template() -> CsTemplate {
    return CsTemplate(0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0);
}

// Angular reach of a system, radians: everything it does fades to zero
// before this distance (see cs_synoptic), so nothing jumps where its
// evaluation stops.
fn cs_reach(radius: f32) -> f32 {
    return min(radius * 4.6, 1.5);
}

// Local coordinates of `q` in a system's frame (x east, y poleward, system
// radius 1) and, in z, the taper that fades everything the system does to
// zero at its reach (0 outside it).
fn cs_local(q: vec3<f32>, system: CloudSystem) -> vec3<f32> {
    let center = system.center.xyz;
    let radius = max(system.center.w, 1.0e-4);
    let cos_distance = dot(q, center);
    let reach = cs_reach(radius);
    if (cos_distance < cos(reach)) { return vec3<f32>(0.0); }
    let taper = 1.0 - smooth_step(0.75, 1.0, acos(clamp(cos_distance, -1.0, 1.0)) / reach);
    let east = system.frame.xyz;
    let north = cross(center, east);
    let tangent = q - center * cos_distance;
    return vec3<f32>(dot(tangent, east) / radius, dot(tangent, north) * system.shape.z / radius, taper);
}

// Extratropical cyclone frame: tilted with its fronts, east-west aspect.
fn cs_extratropical_frame(local: vec2<f32>, system: CloudSystem) -> vec2<f32> {
    let ct = cos(system.extra.w);
    let st = sin(system.extra.w);
    return vec2<f32>((ct * local.x + st * local.y) / system.extra.z, -st * local.x + ct * local.y);
}

// A system's circulation at a local point, before the taper: x the wind-up
// of its own cloud (radians, cyclonic in the local frame; the noise takes
// 0.45 of it in cs_advect, the template all of it), y the broad circulation
// that bends the surrounding field.
fn cs_circulation(local: vec2<f32>, system: CloudSystem) -> vec2<f32> {
    let intensity = system.shape.y;
    if (system.frame.w > 1.5) {
        let r2 = dot(local, local * vec2<f32>(1.0 / (system.extra.z * system.extra.z), 1.0));
        return vec2<f32>(0.0, -0.9 * intensity * exp(-r2 / (1.1 * 1.1)));
    }
    if (system.frame.w > 0.5) {
        let r2 = dot(local, local);
        return vec2<f32>(3.2 * intensity * exp(-r2 / (0.5 * 0.5)), 0.8 * intensity * exp(-r2 / (1.2 * 1.2)));
    }
    let q = cs_extratropical_frame(local, system);
    let r2 = dot(q, q);
    let stage = system.shape.x;
    return vec2<f32>(
        (0.3 + 1.6 * stage) * (0.6 + 0.4 * intensity) * exp(-r2 / (1.0 * 1.0)),
        0.6 * intensity * exp(-r2 / (1.5 * 1.5)),
    );
}

// Extratropical cyclone in local coordinates (x east, y poleward, radius 1).
// An open frontal wave is wound up by the cyclone's own circulation: the
// twirl grows with life-cycle stage, so young waves keep a straight cold
// front and broad warm-front shield, mature lows curl the shield into a
// broad comma head with a dry slot, and occluded lows wrap it about once
// around the centre, trailing a long cold front. Real lows are not wound
// tighter than that: more wind-up reads as a whirlpool of thin arms.
fn cs_extratropical(local: vec2<f32>, system: CloudSystem) -> CsTemplate {
    let stage = system.shape.x;
    let intensity = system.shape.y;
    let front_length = system.extra.x;
    let curvature = system.extra.y;
    let q = cs_extratropical_frame(local, system);
    let r = length(q);
    var tpl = cs_empty_template();
    // Un-wind: the open-wave pattern that the circulation carried here.
    let twirl = cs_circulation(local, system).x;
    let c = cos(twirl);
    let s = sin(twirl);
    let w = vec2<f32>(c * q.x + s * q.y, -s * q.x + c * q.y);

    // Cold front: from just east of the low, equatorward, bowing east and
    // bending west along its tail, with a gentle frontal wave.
    let y = -w.y;
    let phase = system.shape.w * 6.2831853;
    let front_x = 0.06 - 0.32 * y - curvature * y * y
        + 0.09 * sin(2.3 * y + phase) * smooth_step(0.3, 1.2, y);
    let ahead = w.x - front_x;
    let along = smooth_step(-0.35, 0.0, y) * (1.0 - smooth_step(front_length * 0.65, front_length, y));
    // The band widens toward the low, where the warm conveyor belt rises; its
    // rear (cold) edge is sharp, its leading edge diffuse.
    let band_width = (0.45 + 0.2 * (1.0 - stage)) * (1.0 + 0.6 * (1.0 - smooth_step(0.0, 1.2, y)))
        * (0.85 + 0.3 * sin(1.7 * y + 2.0 * phase));
    let band = smooth_step(-0.05, 0.02, ahead) * (1.0 - smooth_step(band_width * 0.55, band_width, ahead)) * along;
    // Warm front east of the low; its cloud shield lies poleward of it.
    let poleward = w.y - (0.12 * w.x - 0.03);
    let shield = smooth_step(-0.10, 0.06, poleward) * (1.0 - smooth_step(0.9, 1.6, poleward))
        * smooth_step(-1.05, -0.45, w.x) * (1.0 - smooth_step(1.25, 2.2, w.x));
    // Warm sector: ahead of the cold front, equatorward of the warm front.
    // Both sectors abut the band, so the system's weight has no seam there.
    let warm = smooth_step(0.0, 0.2, ahead) * (1.0 - smooth_step(-0.12, 0.04, poleward))
        * (1.0 - smooth_step(1.1, 1.9, w.x)) * along;
    // Cold sector: behind the cold front, outside the shield. Near the low
    // it is the descending dry intrusion that the wind-up curls into a slot.
    let cold = (1.0 - smooth_step(-0.12, 0.0, ahead)) * (1.0 - shield)
        * smooth_step(-0.6, -0.1, y);
    let dry = cold * (1.0 - smooth_step(0.3, 1.0, r)) * (0.35 + 0.65 * stage);

    let frontal = max(shield, band);
    // The system's weight already fades with its features, so the frontal
    // cover itself is not scaled down again (that thinned bands to their core).
    tpl.cover = 0.5;
    tpl.cover = mix(tpl.cover, 0.62, warm);
    tpl.cover = mix(tpl.cover, 0.5, cold);
    tpl.cover = mix(tpl.cover, 0.02, dry);
    tpl.cover = mix(tpl.cover, 0.97, frontal);
    tpl.feature = clamp(max(max(frontal, dry), max(0.8 * warm, 0.75 * cold)), 0.0, 1.0);
    tpl.thick = 0.4 + 2.0 * shield + 1.6 * band + 0.1 * warm;
    tpl.deep = 0.7 * frontal;
    tpl.cirrus = 0.85 * shield * smooth_step(0.2, 0.8, poleward) + 0.35 * band;
    tpl.strat = frontal + 0.4 * warm * (1.0 - frontal);
    tpl.closed = 0.6 * warm * (1.0 - frontal);
    tpl.open = cold * (1.0 - dry) * (1.0 - frontal);
    tpl.cu = max(1.0 - tpl.strat - tpl.closed - tpl.open, 0.0);

    let core = 1.0 - smooth_step(1.35, 2.1, r);
    let tail = along * (1.0 - smooth_step(0.25, 0.7, abs(ahead + 0.05)));
    tpl.env = max(core, tail) * intensity;
    return tpl;
}

// Ridge between lows: subsiding air clears or flattens cloud into a thin
// stratocumulus sheet (its anticyclonic flow is in cs_circulation).
fn cs_anticyclone(local: vec2<f32>, system: CloudSystem) -> CsTemplate {
    let intensity = system.shape.y;
    var q = local;
    q.x /= system.extra.z;
    let r = length(q);
    var tpl = cs_empty_template();
    let core = 1.0 - smooth_step(0.2, 1.1, r);
    tpl.env = core * intensity;
    tpl.feature = 0.55;
    tpl.cover = 0.35;
    tpl.thick = 0.6;
    tpl.closed = 0.5;
    tpl.cu = 0.5;
    return tpl;
}

// Tropical cyclone: eye, central dense overcast and spiral rainbands that wind
// inward cyclonically (theta + k ln r = const), with a cirrus outflow shield.
fn cs_tropical(local: vec2<f32>, system: CloudSystem) -> CsTemplate {
    let stage = system.shape.x;
    let intensity = system.shape.y;
    let r = length(local);
    let theta = atan2(local.y, local.x);
    let tightness = system.extra.y;
    let spiral = theta + tightness * log(max(r, 0.02));
    let arm = 0.5 + 0.5 * cos(2.0 * spiral + system.shape.w * 6.2831853);
    let bands = smooth_step(0.4, 0.85, arm) * smooth_step(0.12, 0.3, r) * (1.0 - smooth_step(0.65, 1.15, r));
    let overcast = 1.0 - smooth_step(0.14 + 0.1 * stage, 0.34, r);
    let eye = (1.0 - smooth_step(0.022, 0.05, r)) * smooth_step(0.6, 0.85, intensity);
    var tpl = cs_empty_template();
    tpl.cover = max(max(overcast, 0.92 * bands), 0.12) * (1.0 - eye);
    tpl.feature = 1.0;
    tpl.thick = 0.8 + 2.4 * overcast + 1.2 * bands;
    tpl.deep = 0.9;
    tpl.cirrus = 0.75 * (1.0 - smooth_step(0.5, 1.5, r)) * (1.0 - eye);
    tpl.strat = max(overcast, bands);
    tpl.cu = 1.0 - tpl.strat;
    tpl.env = (1.0 - smooth_step(0.95, 1.45, r)) * intensity;
    return tpl;
}

struct CsSynoptic {
    tpl: CsTemplate,
    // Noise domain points carried back along the flow (see cs_advect).
    point: vec3<f32>,
    point_meso: vec3<f32>,
    point_fine: vec3<f32>,
}

// Backward steps of the flow that advects the noise domain.
const CS_FLOW_STEPS: i32 = 6;
// Westerly jet along each storm track, radians of rotation about the
// circulation pole over the whole advection, and its latitude scale.
const CS_JET_ANGLE: f32 = 0.6;
const CS_JET_WIDTH_DEG: f32 = 12.0;
// Polar easterlies poleward of the subpolar low, radians.
const CS_EASTERLY_ANGLE: f32 = 0.2;

// Carries a point backward through a steady flow made of every system's
// circulation and the storm-track jets (advected texture, Neyret 2003), so
// the noise is stretched along the streamlines: filaments and tilted bands
// in the saddles between lows and ridges, streaks in the jets' shear. Each
// step rotates about every system centre by its rate at the current point,
// which is exact for an isolated vortex. The flow is seen from the lows,
// which travel with the jet core: the jet's flanks and the easterlies lag
// behind them, and the cores keep their own clean wind-up. Fine detail lives
// too briefly to be wound into coherent spirals: it takes half the first
// step, mesoscale the first step, synoptic structure all of them.
fn cs_advect(p: vec3<f32>) -> CsSynoptic {
    let count = min(u32(cloud_synthesis.control.y), CS_MAX_SYSTEMS);
    let pole = cs_pole();
    let track = cloud_synthesis.climate.z;
    let subpolar = max(cloud_synthesis.climate.y, cloud_synthesis.climate.x + 10.0);
    let dt = 1.0 / f32(CS_FLOW_STEPS);
    var q = p;
    var meso = p;
    var fine = p;
    for (var step = 0; step < CS_FLOW_STEPS; step++) {
        let alat = abs(asin(clamp(dot(q, pole), -1.0, 1.0)) * 57.29578);
        let jet_offset = (alat - track) / CS_JET_WIDTH_DEG;
        let lag = CS_JET_ANGLE * (1.0 - exp(-jet_offset * jet_offset))
            + CS_EASTERLY_ANGLE * smooth_step(subpolar - 4.0, subpolar + 8.0, alat);
        var next = cs_rotate(q, pole, lag * dt);
        for (var i = 0u; i < count; i++) {
            let system = cloud_synthesis.systems[i];
            let local = cs_local(q, system);
            if (local.z <= 0.0) { continue; }
            // Cyclonic in the local frame is counter-clockwise seen from above
            // in the north and clockwise in the south.
            let rate = cs_circulation(local.xy, system) * local.z;
            next = cs_rotate(next, system.center.xyz, -(0.45 * rate.x + rate.y) * system.shape.z * dt);
        }
        q = normalize(next);
        if (step == 0) {
            fine = normalize(p + q);
            meso = q;
        }
    }
    return CsSynoptic(cs_empty_template(), q, meso, fine);
}

fn cs_synoptic(p: vec3<f32>) -> CsSynoptic {
    var acc = cs_empty_template();
    var weight = 0.0;
    var clear = 1.0;
    let count = min(u32(cloud_synthesis.control.y), CS_MAX_SYSTEMS);
    for (var i = 0u; i < count; i++) {
        let system = cloud_synthesis.systems[i];
        let local = cs_local(p, system);
        if (local.z <= 0.0) { continue; }
        var tpl: CsTemplate;
        if (system.frame.w > 1.5) {
            tpl = cs_anticyclone(local.xy, system);
        } else if (system.frame.w > 0.5) {
            tpl = cs_tropical(local.xy, system);
        } else {
            tpl = cs_extratropical(local.xy, system);
        }
        tpl.env *= local.z;
        if (tpl.env > 0.0) {
            let w = tpl.env * tpl.feature;
            weight += w;
            acc.cover += w * tpl.cover;
            acc.thick += w * tpl.thick;
            acc.deep += w * tpl.deep;
            acc.strat += w * tpl.strat;
            acc.closed += w * tpl.closed;
            acc.open += w * tpl.open;
            acc.cu += w * tpl.cu;
            clear *= 1.0 - clamp(w, 0.0, 1.0);
            acc.cirrus = max(acc.cirrus, tpl.env * tpl.cirrus);
        }
    }
    if (weight > 0.0) {
        let inv = 1.0 / weight;
        acc.cover *= inv;
        acc.thick *= inv;
        acc.deep *= inv;
        acc.strat *= inv;
        acc.closed *= inv;
        acc.open *= inv;
        acc.cu *= inv;
    }
    acc.env = 1.0 - clear;
    var result = cs_advect(p);
    result.tpl = acc;
    return result;
}

// ---------------------------------------------------------------------------
// Regimes

struct CsRegime {
    weight: f32,
    cover: f32,
    // In-cloud physical optical depth.
    tau: f32,
    deep: f32,
    // Edge softness, in standard deviations of the shape field.
    soft: f32,
    strat: f32,
    closed: f32,
    open: f32,
    cu: f32,
    conv: f32,
}

fn cs_add(
    m: ptr<function, CsRegime>, w: f32, cover: f32, tau: f32, deep: f32, soft: f32,
    morph: vec4<f32>, conv: f32,
) {
    if (w <= 0.0) { return; }
    (*m).weight += w;
    (*m).cover += w * cover;
    (*m).tau += w * tau;
    (*m).deep += w * deep;
    (*m).soft += w * soft;
    (*m).strat += w * morph.x;
    (*m).closed += w * morph.y;
    (*m).open += w * morph.z;
    (*m).cu += w * morph.w;
    (*m).conv += w * conv;
}

// Land fraction with a shoreline ramp of a few hundred metres of relief.
fn cs_land(direction: vec3<f32>) -> f32 {
    let height = textureSampleLevel(height_tex, height_sampler, direction, 0.0).r;
    return smooth_step(uniforms.ocean_level - 0.01, uniforms.ocean_level + 0.03, height);
}

// Solver depth d whose preview/bridge optical depth 36 (d - 0.05) t^0.6,
// t = clamp((d - 0.05) / 0.3), equals `tau`. Below tau 0.05 it ramps to 0 so
// empty sky stays exactly empty for every consumer.
fn cs_solver_depth(tau: f32) -> f32 {
    let t = max(tau, 0.0);
    if (t < 0.05) { return t * (0.06048 / 0.05); }
    if (t < 10.8) { return 0.05 + pow(0.013489 * t, 0.625); }
    return 0.05 + t / 36.0;
}

// Three fBm bands, each zero-mean with unit variance when fully resolved:
// synoptic (2400-300 km), mesoscale (150-37 km) and fine (19-2.3 km), each
// evaluated at its own domain point.
struct CsOctaves {
    lo: f32,
    mid: f32,
    hi: f32,
    // Unresolved share of each band's variance.
    lo_unresolved: f32,
    mid_unresolved: f32,
    hi_unresolved: f32,
}

fn cs_fbm(
    q_lo: vec3<f32>, q_mid: vec3<f32>, q_hi: vec3<f32>, footprint: vec3<f32>, radius_km: f32, seed: u32,
    stream: u32,
) -> CsOctaves {
    var sum = vec3<f32>(0.0);
    var variance = vec3<f32>(0.0);
    var unresolved = vec3<f32>(0.0);
    for (var k = 0; k < CS_OCTAVES; k++) {
        let size = CS_LARGEST_KM * exp2(-f32(k)) / radius_km;
        // The synoptic band falls off from 1200 km, so thresholding it
        // leaves large clear and cloudy areas for the flow to draw into
        // bands rather than a camouflage of blotches; then the fractal gain.
        var lo_amp = array<f32, 4>(0.7, 0.8, 0.55, 0.38);
        let amp = select(pow(CS_GAIN, f32(k - 3)), lo_amp[min(k, 3)], k < 4);
        var band = 0;
        var q = q_lo;
        if (k >= CS_FINE_OCTAVE) {
            band = 2;
            q = q_hi;
        } else if (k >= CS_MID_OCTAVE) {
            band = 1;
            q = q_mid;
        }
        let resolved = cs_resolved(footprint[band], size);
        var n = 0.0;
        if (resolved > 0.0) {
            n = snoise(q / size + noise_seed_offset(seed, stream + u32(k))) / CS_SIMPLEX_SIGMA;
        }
        let a2 = amp * amp;
        sum[band] += amp * resolved * n;
        variance[band] += a2;
        unresolved[band] += a2 * (1.0 - resolved * resolved);
    }
    let scale = inverseSqrt(variance);
    let share = unresolved / variance;
    return CsOctaves(sum.x * scale.x, sum.y * scale.y, sum.z * scale.z, share.x, share.y, share.z);
}

// Synthesized column mass (low, deep, high, occupancy) in solver units.
fn cs_synthesize(p: vec3<f32>, footprint: f32, solver: vec4<f32>) -> vec4<f32> {
    let radius_km = max(uniforms.planet_radius_km, 100.0);
    let pole = cs_pole();
    let seed = uniforms.cloud_seed;
    let moisture = clamp(cloud_synthesis.control.z, 0.0, 1.0);
    if (moisture <= 0.0) { return vec4<f32>(0.0); }

    // ---- Climate at this column ----
    let lat = asin(clamp(dot(p, pole), -1.0, 1.0)) * 57.29578;
    let alat = abs(lat);
    let hadley = cloud_synthesis.climate.x;
    let subpolar = max(cloud_synthesis.climate.y, hadley + 10.0);
    let height = textureSampleLevel(height_tex, height_sampler, p, 0.0).r;
    // Regimes change over ~150 km at coasts, not at the shoreline texel:
    // two rings of taps, so no single sample steps the fraction much. The
    // ring turns with the circulation pole, continuously everywhere but there.
    let east = normalize(cross(pole, p) + vec3<f32>(1.0e-6, 0.0, 0.0));
    let basis = mat2x3<f32>(east, cross(p, east));
    var land = cs_land(p);
    for (var i = 0; i < 8; i++) {
        let angle = f32(i) * 0.7853982;
        let reach = select(140.0, 70.0, (i & 1) == 0) / radius_km;
        land += cs_land(normalize(p + (basis[0] * cos(angle) + basis[1] * sin(angle)) * reach));
    }
    land /= 9.0;
    let ocean = 1.0 - land;
    let wind = textureSampleLevel(cloud_tex, height_sampler, p, 0.0);
    let continentality = clamp(wind.a, 0.0, 1.0) * land;
    let temperature = climate_temperature(p, height, uniforms.ocean_level, uniforms.base_temp_c,
        uniforms.axial_tilt_rad, uniforms.season);
    let season_sign = clamp((uniforms.season - 0.5) * 2.0, -1.0, 1.0);
    let local_summer = season_sign * sign(lat + 1.0e-4) * clamp(uniforms.axial_tilt_rad / 0.4091, 0.0, 1.5);
    // ITCZ: the wind field's thermal trough (seasonal, monsoonal over land).
    let itcz = 5.0 * season_sign * (uniforms.axial_tilt_rad / 0.4084)
        + snoise(p * 1.5 + noise_seed_offset(seed, 10u)) * 5.0 + continentality * 15.0 * season_sign;
    let d_itcz = lat - itcz;
    let w_itcz = exp(-0.5 * (d_itcz / 2.8) * (d_itcz / 2.8));
    let w_tropics = exp(-0.5 * (d_itcz / 10.0) * (d_itcz / 10.0));
    // Circulation belts as a partition of unity.
    let sub_in = smooth_step(0.25 * hadley, 0.7 * hadley, alat);
    let storm_in = smooth_step(0.95 * hadley, 1.3 * hadley, alat);
    let polar_in = smooth_step(subpolar - 4.0, subpolar + 12.0, alat);
    let b_trop = 1.0 - sub_in;
    let b_sub = sub_in * (1.0 - storm_in);
    let b_storm = storm_in * (1.0 - polar_in);
    let b_polar = polar_in;
    // West-coast stratocumulus: subtropical ocean with land to the east
    // (cold upwelling under the subtropical high's equatorward flank). The
    // deck thins out over ~2000 km as the trades carry it off the cold water,
    // so land to the east is a weighted mean over a fan of taps 250-2000 km
    // out, widening with distance, with a soft shoreline per tap. The max of a
    // few point probes drew the coastline offshore, once per probe.
    let north = basis[1];
    var east_land = 0.0;
    var east_total = 0.0;
    for (var i = 0; i < 6; i++) {
        let along = min((250.0 + 350.0 * f32(i)) / radius_km, 0.5);
        let weight = 1.0 - 0.12 * f32(i);
        for (var j = -1; j <= 1; j++) {
            let probe = normalize(p * cos(along) + east * sin(along) + north * (0.45 * along * f32(j)));
            let probe_height = textureSampleLevel(height_tex, height_sampler, probe, 0.0).r;
            east_land += weight * smooth_step(uniforms.ocean_level - 0.1, uniforms.ocean_level + 0.1, probe_height);
            east_total += weight;
        }
    }
    east_land = smooth_step(0.1, 0.55, east_land / east_total);
    let deck_zone = smooth_step(0.3 * hadley, 0.55 * hadley, alat)
        * (1.0 - smooth_step(1.1 * hadley, 1.45 * hadley, alat));
    let w_deck = ocean * deck_zone * east_land;
    let frozen = 1.0 - smooth_step(-25.0, -5.0, temperature);
    let hot = smooth_step(20.0, 28.0, temperature);

    // ---- Regime mix ----
    var regime = CsRegime(0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0);
    let strat = vec4<f32>(1.0, 0.0, 0.0, 0.0);
    let closed = vec4<f32>(0.0, 1.0, 0.0, 0.0);
    let cumulus = vec4<f32>(0.0, 0.0, 0.0, 1.0);
    // Ocean.
    cs_add(&regime, ocean * w_itcz * (0.6 + 0.4 * hot), 0.42, 45.0, 0.85, 0.3, vec4<f32>(0.0, 0.0, 0.0, 0.2), 1.0);
    cs_add(&regime, ocean * (b_trop + b_sub) * (1.0 - w_itcz) * (1.0 - w_deck), 0.14, 3.0, 0.0, 0.15, cumulus, 0.0);
    cs_add(&regime, ocean * w_deck * 1.6, 0.84, 11.0, 0.0, 0.06, closed, 0.0);
    cs_add(&regime, ocean * b_storm, 0.4, 6.5, 0.2, 0.18, vec4<f32>(0.65, 0.0, 0.35, 0.0), 0.0);
    cs_add(&regime, ocean * b_polar, 0.58, 6.0, 0.1, 0.2, vec4<f32>(0.8, 0.0, 0.2, 0.0), 0.0);
    // Land.
    let wet_tropics = land * w_tropics * (1.0 - 0.7 * continentality * b_sub);
    cs_add(&regime, wet_tropics, 0.48, 30.0, 0.75, 0.3, vec4<f32>(0.0, 0.0, 0.0, 0.5), 0.5);
    cs_add(&regime, land * (b_trop + b_sub) * (1.0 - w_tropics), mix(0.16, 0.02, continentality), 4.0, 0.0, 0.2,
        cumulus, 0.0);
    cs_add(&regime, land * b_storm, mix(0.38, 0.22, continentality) + 0.08 * local_summer, 7.0, 0.2, 0.18,
        vec4<f32>(0.5 - 0.2 * local_summer, 0.0, 0.0, 0.5 + 0.2 * local_summer), 0.0);
    cs_add(&regime, land * b_polar, mix(0.45, 0.3, frozen), 5.0, 0.05, 0.2, strat, 0.0);
    let inv = 1.0 / max(regime.weight, 1.0e-6);
    var cover = regime.cover * inv;
    var tau = regime.tau * inv;
    var deep_share = regime.deep * inv;
    var soft = regime.soft * inv;
    var morph = vec4<f32>(regime.strat, regime.closed, regime.open, regime.cu) * inv;
    var conv = regime.conv * inv;

    // ---- Solver hints ----
    // Regional condensate (~250 km) keeps the solver's orographic cloud, rain
    // shadows and moist and dry air masses; its convective storms and ice
    // become clusters and cirrus.
    let reach = 450.0 / radius_km;
    var regional = solver.r + solver.g + solver.b;
    for (var i = 0; i < 4; i++) {
        let angle = f32(i) * 1.5707963 + 0.4;
        let probe = normalize(p + (basis[0] * cos(angle) + basis[1] * sin(angle)) * reach);
        let m = sample_weather_mass(probe);
        regional += m.r + m.g + m.b;
    }
    regional *= 0.2;
    cover *= mix(0.72, 1.15, smooth_step(0.004, 0.09, regional));
    let storm_hint = smooth_step(0.03, 0.16, solver.g);
    conv = max(conv, storm_hint);
    tau = mix(tau, 45.0, storm_hint * 0.6);
    deep_share = max(deep_share, 0.8 * storm_hint);
    cover = max(cover, 0.5 * storm_hint);

    // ---- Synoptic systems ----
    let synoptic = cs_synoptic(p);
    let env = synoptic.tpl.env;
    let system_open = synoptic.tpl.open * ocean;
    let system_morph = vec4<f32>(synoptic.tpl.strat, synoptic.tpl.closed, system_open,
        synoptic.tpl.cu + synoptic.tpl.open * land);
    let system_cover = synoptic.tpl.cover * (1.0 - 0.3 * continentality);
    cover = mix(cover, system_cover, env);
    tau = mix(tau, 14.0 * synoptic.tpl.thick, env);
    deep_share = mix(deep_share, synoptic.tpl.deep, env);
    soft = mix(soft, 0.1, env);
    morph = mix(morph, system_morph, env);
    conv *= 1.0 - env;

    // Coverage control: 0.5 is the physical baseline; lower values clear
    // broken cloud first and keep the large systems, higher ones fill gaps.
    let gamma = exp2((clamp(uniforms.cloud_coverage, 0.0, 1.0) - 0.5) * 3.0);
    cover = 1.0 - pow(1.0 - clamp(cover * min(moisture * 1.6, 1.0), 0.0, 0.995), gamma);

    // ---- Noise domains ----
    // Footprints in each stretched domain.
    let fp = cs_stretch_footprint(footprint, CS_ZONAL_STRETCH);
    let fp_cells = cs_stretch_footprint(footprint, 1.3);
    let base = cs_zonal_stretch(synoptic.point, pole, CS_ZONAL_STRETCH);
    let warp_size = 1600.0 / radius_km;
    let warp_a = cs_resolved(fp, warp_size) * (160.0 / radius_km);
    let warp_1 = vec3<f32>(
        snoise(base / warp_size + noise_seed_offset(seed, 31u)),
        snoise(base / warp_size + noise_seed_offset(seed, 32u)),
        snoise(base / warp_size + noise_seed_offset(seed, 33u)),
    ) * warp_a;
    let warp_small = 420.0 / radius_km;
    let warp_b = cs_resolved(fp, warp_small) * (25.0 / radius_km);
    var warp_2 = vec3<f32>(0.0);
    if (warp_b > 0.0) {
        let qa = base + warp_1;
        warp_2 = vec3<f32>(
            snoise(qa / warp_small + noise_seed_offset(seed, 34u)),
            snoise(qa / warp_small + noise_seed_offset(seed, 35u)),
            snoise(qa / warp_small + noise_seed_offset(seed, 36u)),
        ) * warp_b;
    }
    let q_lo = base + warp_1 + warp_2;
    let q_mid = cs_zonal_stretch(synoptic.point_meso, pole, 1.8) + warp_1 * 0.7 + warp_2;
    let q_hi = cs_zonal_stretch(synoptic.point_fine, pole, 1.25) + warp_1 * 0.4 + warp_2 * 0.6;
    let q_cells = cs_zonal_stretch(synoptic.point_fine, pole, 1.3) + warp_1 * 0.5 + warp_2 * 0.8;
    let octaves = cs_fbm(q_lo, q_mid, q_hi, vec3<f32>(fp, cs_stretch_footprint(footprint, 1.8),
        cs_stretch_footprint(footprint, 1.25)), radius_km, seed, 40u);

    // Cellular morphologies, each zero-mean and unit variance when resolved.
    var closed_cells = 0.0;
    var closed_unres = 1.0;
    if (morph.y > 0.02) {
        let size = 30.0 / radius_km;
        let resolved = cs_cells_resolved(fp_cells, size);
        if (resolved > 0.0) {
            let cells = cs_worley(q_cells / size, seed ^ 0x51u);
            // Bright polygon cells with narrow, wobbling darker borders.
            // Mean and deviation measured for unit-jitter cells on a slice.
            let edge = cells.y - cells.x + 0.05 * octaves.hi * (1.0 - octaves.hi_unresolved);
            let interior = smooth_step(0.0, 0.22, edge);
            closed_cells = resolved * (interior - 0.65) / 0.385;
            closed_unres = 1.0 - resolved * resolved;
        }
    }
    var open_cells = 0.0;
    var open_unres = 1.0;
    if (morph.z > 0.02) {
        let size = 40.0 / radius_km;
        let resolved = cs_cells_resolved(fp_cells, size);
        if (resolved > 0.0) {
            let cells = cs_worley(q_cells / size + vec3<f32>(17.0), seed ^ 0x6fu);
            // Cloud on the cell walls around clear centres.
            let edge = cells.y - cells.x + 0.05 * octaves.hi * (1.0 - octaves.hi_unresolved);
            let walls = 1.0 - smooth_step(0.03, 0.28, edge);
            open_cells = resolved * (walls - 0.47) / 0.41;
            open_unres = 1.0 - resolved * resolved;
        }
    }
    var clusters = 0.0;
    var clusters_unres = 1.0;
    if (conv > 0.02) {
        let size = 320.0 / radius_km;
        let resolved = cs_cells_resolved(fp, size * 3.0);
        if (resolved > 0.0) {
            let q_cluster = cs_zonal_stretch(synoptic.point_meso, pole, 1.4) + warp_1 * 0.6 + warp_2;
            // Mesoscale convective systems: about half the cells host one,
            // 130-510 km across, elongated, with ragged anvil edges; they
            // swell where the synoptic band is moist, so they gather in
            // groups instead of a string of pearls.
            let ragged = 0.1 * octaves.lo + 0.25 * octaves.hi * (1.0 - octaves.hi_unresolved);
            let blob = cs_blobs(q_cluster / size + vec3<f32>(-23.0), seed ^ 0x77u, 0.42,
                vec2<f32>(0.2, 0.8), 0.15, ragged);
            var small = 0.0;
            let small_size = 85.0 / radius_km;
            let small_resolved = cs_cells_resolved(fp_cells, small_size);
            if (small_resolved > 0.0) {
                // Isolated cumulonimbus between the systems.
                small = small_resolved * cs_blobs(q_cells / small_size + vec3<f32>(5.0), seed ^ 0x99u, 0.2,
                    vec2<f32>(0.2, 0.55), 0.3, ragged);
            }
            // Mean and deviation measured on a sphere slice of unit cells.
            clusters = resolved * (max(blob, 0.7 * small) - CS_MCS_STATS.x) / CS_MCS_STATS.y;
            clusters_unres = 1.0 - resolved * resolved;
        }
    }

    // Cumulus "flowers": roundish clusters separated by clear sky, the
    // commonest mesoscale organization of trade cumulus (Stevens et al. 2020).
    var flowers = 0.0;
    var flowers_unres = 1.0;
    if (morph.w > 0.02) {
        let size = 110.0 / radius_km;
        let resolved = cs_cells_resolved(cs_stretch_footprint(footprint, 1.8), size * 2.0);
        if (resolved > 0.0) {
            // Drawn out along the trades into short streets.
            let q_flowers = cs_zonal_stretch(synoptic.point_meso, pole, 1.8) + warp_1 * 0.5 + warp_2 * 0.8;
            let ragged = 0.3 * octaves.hi * (1.0 - octaves.hi_unresolved);
            let blob = cs_blobs(q_flowers / size + vec3<f32>(41.0), seed ^ 0x3bu, 0.6,
                vec2<f32>(0.25, 0.75), 0.2, ragged);
            flowers = resolved * (blob - CS_FLOWER_STATS.x) / CS_FLOWER_STATS.y;
            flowers_unres = 1.0 - resolved * resolved;
        }
    }

    // ---- Two-level remap ----
    // Cloud fraction = macro fraction x fill. The macro field (synoptic and
    // mesoscale bands plus cells or clusters) decides where decks, cells and
    // cumulus clusters are; the fine band decides how solidly each is filled.
    // Both are thresholded at their quantiles, so the expected cover is exact
    // at any resolution: unresolved cumulus inside a cluster renders as its
    // fill fraction (gray), and resolves into individual clouds at 8K.
    let w_sum = max(morph.x + morph.y + morph.z + morph.w + conv, 1.0e-4);
    let wm = morph / w_sum;
    let wc = conv / w_sum;
    // Macro recipes, each unit variance: stratiform is synoptic-dominated,
    // cumulus clusters follow the synoptic and mesoscale bands (the flowers
    // only texture them: alone, their peaked field reads as a star field),
    // the cellular regimes add their cells.
    let c_lo = wm.x * 0.97 + wm.y * 0.5 + wm.z * 0.45 + wm.w * 0.6 + wc * 0.5;
    let c_mid = wm.x * 0.25 + wm.y * 0.2 + wm.z * 0.2 + wm.w * 0.55 + wc * 0.45;
    let c_cl = wm.y * 0.84;
    let c_op = wm.z * 0.87;
    let c_fl = wm.w * 0.45;
    let c_cv = wc * 0.8;
    let variance = c_lo * c_lo + c_mid * c_mid + c_cl * c_cl + c_op * c_op + c_fl * c_fl + c_cv * c_cv;
    let norm = inverseSqrt(max(variance, 1.0e-6));
    let shape = (c_lo * octaves.lo + c_mid * octaves.mid + c_cl * closed_cells + c_op * open_cells
        + c_fl * flowers + c_cv * clusters) * norm;
    let unresolved = (c_lo * c_lo * octaves.lo_unresolved + c_mid * c_mid * octaves.mid_unresolved
        + c_cl * c_cl * closed_unres + c_op * c_op * open_unres + c_fl * c_fl * flowers_unres
        + c_cv * c_cv * clusters_unres) * norm * norm;
    // Fill of a deck, cell or cluster: decks are solid at this scale (their
    // texture is brightness, not holes); cumulus sits in compact clusters
    // with clear sky between, as satellite composites show it, rather than a
    // half-filled gray veil spread over the whole regime.
    let fill = max(wm.x * 1.0 + wm.y * 0.97 + wm.z * 0.8 + wm.w * 0.8 + wc * 0.9, cover);
    let macro_cover = clamp(cover / max(fill, 1.0e-3), 0.0, 1.0);
    // Cumulus cluster margins are fuzzy even where a system sets the cover.
    let edge = soft + 0.1 * wm.w;
    let threshold = cs_probit(1.0 - macro_cover) * sqrt(1.0 + edge * edge);
    let spread = sqrt(unresolved + edge * edge);
    let clustered = max(cs_logistic((shape - threshold) / spread) - 0.003, 0.0) / 0.997;
    // Individual clouds only become holes once most of the fine band is
    // resolved (16K, or a close preview); a single resolved octave
    // thresholded reads as a regular stipple. Coarser pixels see the fill as
    // brightness, varying with how many clouds each pixel catches.
    let fill_soft = 0.3;
    var filled = fill;
    let fine_resolved = smooth_step(0.4, 0.8, 1.0 - octaves.hi_unresolved);
    if (fill < 0.995 && fine_resolved > 0.0) {
        let fill_threshold = cs_probit(1.0 - fill) * sqrt(1.0 + fill_soft * fill_soft);
        let holes = cs_logistic((octaves.hi - fill_threshold) / sqrt(octaves.hi_unresolved + fill_soft * fill_soft));
        filled = mix(fill, holes, fine_resolved);
    }
    // Broken cumulus margins thin out to nothing rather than a gray veil.
    let mask = clustered * filled * mix(1.0, smooth_step(0.04, 0.25, clustered), wm.w);
    // Optical depth grows with distance above the threshold, so margins are
    // thin and cores thick (roughly lognormal, as observed) instead of flat
    // gray patches; resolved mesoscale and fine structure varies brightness,
    // most in broken cumulus and open cells.
    // Sub-pixel structure keeps its mean: E[exp(1.1 x)] over the unresolved
    // variance adds 1.1^2 / 2 of it.
    let interior = exp(clamp(1.1 * (shape - threshold) - 0.5 + 0.605 * unresolved, -2.5, 1.2));
    let texture = 0.12 + 0.35 * (wm.w + 0.5 * wm.z);
    let tau_in = tau * interior * exp(texture * (
        0.4 * octaves.mid * (1.0 - octaves.mid_unresolved) + 0.6 * octaves.hi * (1.0 - octaves.hi_unresolved)));
    let tau_liquid = mask * tau_in * (1.0 - 0.45 * frozen);

    // ---- Cirrus ----
    var cirrus_cover = 0.03 + 0.12 * b_storm + 0.12 * w_itcz * ocean + 0.05 * wet_tropics
        + 0.2 * smooth_step(0.006, 0.04, solver.b) + 0.15 * conv;
    cirrus_cover = max(cirrus_cover, synoptic.tpl.cirrus);
    cirrus_cover = clamp(cirrus_cover * min(moisture * 1.6, 1.0), 0.0, 0.9);
    var high = 0.0;
    if (cirrus_cover > 0.01) {
        let qc = cs_zonal_stretch(synoptic.point, pole, CS_CIRRUS_STRETCH) + warp_1 * 1.4;
        let qc_fine = cs_zonal_stretch(synoptic.point_meso, pole, CS_CIRRUS_STRETCH) + warp_1;
        let fp_cirrus = cs_stretch_footprint(footprint, CS_CIRRUS_STRETCH);
        let fibres = cs_fbm(qc, qc_fine, qc_fine, vec3<f32>(fp_cirrus), radius_km, seed, 60u);
        let ci_shape = 0.7 * fibres.lo + 0.6 * fibres.mid + 0.4 * fibres.hi;
        let ci_unres = 0.49 * fibres.lo_unresolved + 0.36 * fibres.mid_unresolved + 0.16 * fibres.hi_unresolved;
        let ci_soft = 0.35;
        let ci_threshold = cs_probit(1.0 - cirrus_cover) * sqrt(1.0 + ci_soft * ci_soft);
        let ci_mask = max(cs_logistic((ci_shape - ci_threshold) / sqrt(ci_unres + ci_soft * ci_soft)) - 0.003, 0.0)
            / 0.997;
        let streak = 0.6 + 0.4 * (1.0 - abs(fibres.hi) * (1.0 - fibres.hi_unresolved) * 0.6);
        // Ice mass 0.1-0.3: optical depth ~0.5-2, thin veils over the low cloud.
        high = ci_mask * (0.08 + 0.22 * smooth_step(0.0, 1.5, ci_shape - ci_threshold)) * streak;
    }

    let depth = cs_solver_depth(tau_liquid);
    let deep = clamp(deep_share, 0.0, 1.0);
    let low_mass = (1.0 - deep) * depth / (LOW_OPTICAL_WEIGHT * 0.9 * CLOUD_LIGHT_EXTINCTION);
    let deep_mass = deep * depth / (1.65 * CLOUD_LIGHT_EXTINCTION);
    return vec4<f32>(low_mass, deep_mass, high, max(mask, min(high * 2.0, 1.0)));
}

// Column mass for rendering and export: the solver's, blended toward the
// synthesized cloud by the synthesis strength. Strength 0 is exact.
fn cloud_synthesis_mass(direction: vec3<f32>, footprint: f32) -> vec4<f32> {
    let solver = sample_weather_mass(direction);
    let strength = cs_strength();
    if (strength <= 0.0) { return solver; }
    let synthesized = cs_synthesize(normalize(direction), max(footprint, 1.0e-6), solver);
    return mix(solver, synthesized, strength);
}
