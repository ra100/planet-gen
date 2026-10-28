// Giant planets: the banded cloud decks of gas and ice giants. Shared by
// the preview and texture export; concatenate after noise.wgsl and
// rocky.wgsl (hashes). Positions are unit vectors in the planet frame,
// whose spin axis is tilted toward +z by the axial tilt as in climate.wgsl.
// See docs/research/giant-planets-icy-worlds-rings.md.
//
// Bands: zones (anticyclonic upwelling, fresh NH3 ice) are bright, belts
// (cyclonic downwelling, chromophores and deeper cloud) dark. Their number
// comes from the Rhines scale (GiantUniforms::from_derived); they widen
// poleward as beta shrinks, the band phase growing as the integral of
// sqrt(cos lat), and the equatorial pair is broadest (Jupiter's EZ and NEB
// span 0-18°). Above ~60-75° the bands give way to a polar region: Jupiter's
// cyclone clusters (Adriani et al. 2018) or Saturn's hexagon at ~76° N.
//
// Turbulence is sampled in a frame sheared by the zonal jets, stretched
// along longitude, strongest in belts and on jet flanks, and muted by
// upper haze (Saturn). Long-lived anticyclones sit in zones (Great Red
// Spot, white ovals), elongated cyclonic barges in belts; each swirls the
// band lookup so belt and zone material wraps around it.
//
// Palette by effective temperature (Sudarsky et al. 2000): ammonia decks
// below ~150 K, water clouds near 250 K, clear dark-blue skies above
// ~350 K, near-black roasters at 900-1500 K and bright silicate decks
// above. Cold ice giants show the methane-absorbed greenish blue of Uranus
// and Neptune (Irwin et al. 2024), with polar regions depleted in methane.
// Colours are calibrated to Mallama et al. (2017) geometric albedos.

struct GiantParams {
    seed: u32,
    envelope: f32,
    ice: f32,
    temperature: f32, // T_eff, K
    bands: f32,       // belt-zone pairs per hemisphere
    haze: f32,
    storms: f32,
    equator: f32,     // +1 prograde equatorial jet, -1 retrograde
    tilt: f32,        // axial tilt, rad
    season: f32,      // 0 northern winter, 0.5 equinox, 1 northern summer
}

struct GiantVortex {
    center: vec3<f32>, // spin frame
    a: f32,            // half-length along longitude, rad
    b: f32,            // half-width along latitude, rad
    spin: f32,         // swirl at the centre, rad (sign = sense)
    kind: u32,         // 0 white oval, 1 red spot, 2 dark spot, 3 barge
}

const GG_PI: f32 = 3.14159265;
const GG_HALF_PI: f32 = 1.5707963;
const GG_MAX_VORTICES: u32 = 28u;
// Shear of the turbulence by the jets, radians of longitude per unit jet.
const GG_SHEAR: f32 = 2.2;
// Eddies are this many times longer along longitude than across.
const GG_STRETCH: f32 = 3.2;

// Planet frame to spin frame (x, north, z) and back.
fn gg_to_spin(p: vec3<f32>, tilt: f32) -> vec3<f32> {
    let c = cos(tilt);
    let s = sin(tilt);
    return vec3<f32>(p.x, p.y * c + p.z * s, p.z * c - p.y * s);
}

// Fraction of the hemisphere's band phase reached at |latitude| = x * 90°:
// the integral of sqrt(cos lat), normalised (cubic fit, max error 0.008).
fn gg_band_fraction(x: f32) -> f32 {
    let u = clamp(x, 0.0, 1.0);
    return (1.288 * u + 0.108 * u * u - 0.389 * u * u * u) / 1.007;
}

// Band phase at a latitude: zones at integers (the equatorial zone at 0),
// belts at half integers. The power 1.5 widens the equatorial pair so
// Jupiter's belts fall at their observed latitudes.
fn gg_phase(lat: f32, prm: GiantParams) -> f32 {
    return prm.bands * pow(gg_band_fraction(abs(lat) / GG_HALF_PI), 1.5);
}

// Latitude (unsigned) of a band phase; inverse of gg_phase (quartic fit).
fn gg_latitude(phase: f32, prm: GiantParams) -> f32 {
    let f = pow(clamp(phase / max(prm.bands, 0.1), 0.0, 1.0), 2.0 / 3.0);
    let x = 0.6899 * f + 0.6315 * f * f - 1.3386 * f * f * f + 1.0011 * f * f * f * f;
    return clamp(x, 0.0, 1.0) * GG_HALF_PI;
}

// Latitude where the bands give way to the polar region: ~63° on a
// Jupiter-like giant, ~75° with fewer, broader jets (Saturn, ice giants).
fn gg_polar_latitude(prm: GiantParams) -> f32 {
    return mix(1.10, 1.31, clamp((5.5 - prm.bands) / 2.5, 0.0, 1.0));
}

// Zonal wind, prograde positive, of order 1 at the strongest jets. Deep
// jets: alternating jets on the zone-belt boundaries (prograde where a zone
// meets the belt poleward of it) and a broad prograde equatorial jet.
// Shallow jets: a retrograde equator and one prograde jet per hemisphere
// near 70° (Uranus, Neptune).
fn gg_wind(lat: f32, phase: f32, prm: GiantParams) -> f32 {
    let deep = 0.55 * sin(6.2831853 * phase) + (0.6 + 0.6 * prm.haze) * exp(-lat * lat / 0.09);
    let shallow = -cos(2.6 * clamp(abs(lat), 0.0, 1.35));
    return mix(deep, shallow, clamp(0.5 - 0.5 * prm.equator, 0.0, 1.0));
}

fn gg_fbm(q: vec3<f32>, octaves: u32) -> f32 {
    var sum = 0.0;
    var amplitude = 0.5;
    var frequency = 1.0;
    for (var i = 0u; i < octaves; i++) {
        sum += amplitude * snoise(q * frequency);
        frequency *= 2.03;
        amplitude *= 0.52;
    }
    return sum;
}

fn gg_spin_direction(lat: f32, lon: f32) -> vec3<f32> {
    return vec3<f32>(cos(lat) * cos(lon), sin(lat), cos(lat) * sin(lon));
}

fn gg_vortex_count(prm: GiantParams) -> u32 {
    let per_band = mix(3.0, 1.0, prm.ice);
    let count = (mix(4.0, 1.0, prm.ice) + per_band * prm.bands) * clamp(prm.storms * 1.6, 0.0, 1.0);
    return min(u32(count), GG_MAX_VORTICES);
}

// Vortex catalogue. The first is the great spot: a red anticyclone in the
// first southern tropical zone (Jupiter's GRS at 22° S) or, on an ice
// giant, a dark anticyclone at southern mid-latitudes (Neptune's 1989
// Great Dark Spot). Others are white ovals in zones and brown cyclonic
// barges in belts.
fn gg_vortex(prm: GiantParams, i: u32) -> GiantVortex {
    let s = prm.seed * 7919u + 0x1b873593u;
    let stream = i * 8u;
    let lon = rk_rand(s, stream) * 6.2831853;
    let hemisphere = select(-1.0, 1.0, rk_rand(s, stream + 1u) < 0.5);
    let usable = max(prm.bands * 0.8 - 0.5, 0.6);
    var zone = 1.0 + floor(rk_rand(s, stream + 2u) * usable);
    var phase = zone + (rk_rand(s, stream + 3u) - 0.5) * 0.16;
    var kind = 0u;
    var size = 0.35 + 0.35 * rk_rand(s, stream + 4u);
    var aspect = 1.5 + 0.7 * rk_rand(s, stream + 5u);
    var sense = -hemisphere; // anticyclones: clockwise in the north
    var lat_sign = hemisphere;
    if (i == 0u) {
        // Great spot.
        // Red chromophores need the cold ammonia deck; elsewhere a pale oval.
        kind = select(select(0u, 1u, prm.temperature < 200.0), 2u, prm.ice > 0.5);
        lat_sign = -1.0;
        sense = 1.0;
        zone = 1.0;
        phase = select(1.0, min(0.55 * prm.bands, 1.4), prm.ice > 0.5);
        size = 0.85;
        aspect = 2.0;
    } else if (rk_rand(s, stream + 6u) < 0.3 && prm.ice < 0.5) {
        // Cyclonic barge in a belt: long, thin, dark brown.
        kind = 3u;
        phase = zone - 0.5;
        size = 0.25 + 0.15 * rk_rand(s, stream + 4u);
        aspect = 2.6 + rk_rand(s, stream + 5u);
        sense = hemisphere;
    } else if (prm.ice > 0.5) {
        // Small dark spots and bright companions; the bands are too broad
        // to set their size.
        kind = 2u;
        size *= 0.18;
    }
    let lat = gg_latitude(phase, prm) * lat_sign;
    let width = gg_latitude(zone + 0.5, prm) - gg_latitude(max(zone - 0.5, 0.0), prm);
    // Ice-giant bands are too broad to size spots; Neptune's Great Dark
    // Spot was ~13,000 x 6,600 km, ~0.13 rad across in latitude.
    // The Great Red Spot is ~12,000 km across, 0.09 rad on Jupiter; broad
    // bands do not make vortices larger.
    let b = select(min(0.42 * width, 0.13) * size, 0.16 * size, prm.ice > 0.5);
    var v: GiantVortex;
    v.center = gg_spin_direction(lat, lon);
    v.b = b;
    v.a = b * aspect;
    v.spin = sense * mix(2.4, 4.0, size);
    v.kind = kind;
    return v;
}

struct GiantSample {
    q: vec3<f32>,        // displaced spin-frame position for the band lookup
    inside: f32,         // vortex interior weight
    collar: f32,         // vortex rim
    kind: u32,
}

// Swirl the lookup position around the vortices.
fn gg_swirl(q: vec3<f32>, prm: GiantParams) -> GiantSample {
    var out = GiantSample(q, 0.0, 0.0, 0u);
    let count = gg_vortex_count(prm);
    for (var i = 0u; i < count; i++) {
        let v = gg_vortex(prm, i);
        let reach = 3.0 * v.a;
        if (dot(q, v.center) < cos(min(reach, 1.5))) {
            continue;
        }
        let east = normalize(cross(vec3<f32>(0.0, 1.0, 0.0), v.center) + vec3<f32>(1.0e-6, 0.0, 0.0));
        let north = cross(v.center, east);
        let d = out.q - v.center;
        let e = vec2<f32>(dot(d, east) / v.a, dot(d, north) / v.b);
        let r2 = dot(e, e);
        if (r2 > 9.0) {
            continue;
        }
        let angle = v.spin * exp(-r2 * 0.9);
        let c = cos(angle);
        let s = sin(angle);
        let e2 = vec2<f32>(c * e.x - s * e.y, s * e.x + c * e.y);
        out.q = normalize(v.center + east * (e2.x * v.a) + north * (e2.y * v.b));
        let r = sqrt(r2);
        let inside = 1.0 - smoothstep(0.55, 0.95, r);
        if (inside > out.inside) {
            out.inside = inside;
            out.kind = v.kind;
        }
        out.collar = max(out.collar, exp(-(r - 1.0) * (r - 1.0) / 0.02));
    }
    return out;
}

// Zone and belt albedo for the effective temperature (linear RGB).
fn gg_zone_belt(prm: GiantParams) -> array<vec3<f32>, 2> {
    let t = prm.temperature;
    let w2 = smoothstep(150.0, 210.0, t);
    let w3 = smoothstep(330.0, 420.0, t);
    let w4 = smoothstep(800.0, 950.0, t);
    let w5 = smoothstep(1400.0, 1650.0, t);
    // Class I: ammonia ice zones, chromophore-stained belts (Jupiter).
    var zone = vec3<f32>(0.86, 0.89, 0.80);
    var belt = vec3<f32>(0.69, 0.68, 0.55);
    // Upper haze absorbs blue and hides the deck contrast (Saturn).
    zone = mix(zone, vec3<f32>(0.80, 0.71, 0.50), prm.haze);
    belt = mix(belt, vec3<f32>(0.70, 0.59, 0.40), prm.haze);
    // Cold ice giants: methane absorbs red above a deep, hazy deck.
    let methane = prm.ice * (1.0 - smoothstep(90.0, 160.0, t));
    zone = mix(zone, vec3<f32>(0.29, 0.75, 0.93), methane);
    belt = mix(belt, vec3<f32>(0.27, 0.70, 0.88), methane);
    // Class II: water clouds, the brightest giants.
    zone = mix(zone, vec3<f32>(0.86, 0.87, 0.88), w2);
    belt = mix(belt, vec3<f32>(0.78, 0.79, 0.81), w2);
    // Class III: clear, Rayleigh blue over alkali absorption (HD 189733b).
    zone = mix(zone, vec3<f32>(0.05, 0.12, 0.34), w3);
    belt = mix(belt, vec3<f32>(0.04, 0.10, 0.29), w3);
    // Class IV: roasters, darker than charcoal.
    zone = mix(zone, vec3<f32>(0.035, 0.03, 0.035), w4);
    belt = mix(belt, vec3<f32>(0.03, 0.025, 0.03), w4);
    // Class V: high silicate clouds, bright again.
    zone = mix(zone, vec3<f32>(0.62, 0.57, 0.50), w5);
    belt = mix(belt, vec3<f32>(0.50, 0.44, 0.38), w5);
    return array<vec3<f32>, 2>(zone, belt);
}

// Contrast between belts and zones: hazes and the methane column above an
// ice-giant deck flatten it; clear and roaster atmospheres show little.
fn gg_contrast(prm: GiantParams) -> f32 {
    let methane = prm.ice * (1.0 - smoothstep(90.0, 160.0, prm.temperature));
    let warm = smoothstep(150.0, 210.0, prm.temperature);
    return (1.0 - 0.65 * prm.haze) * (1.0 - 0.8 * methane) * (1.0 - 0.6 * warm);
}

// Cloud-deck albedo (linear RGB) at planet-frame direction p.
// `footprint` is the pixel's angular size, which limits the noise octaves.
fn gg_albedo(p: vec3<f32>, prm: GiantParams, footprint: f32) -> vec3<f32> {
    let q0 = gg_to_spin(normalize(p), prm.tilt);
    let seed_offset = noise_seed_offset(prm.seed, 401u);
    let swirl = gg_swirl(q0, prm);
    var q = swirl.q;
    let lat0 = asin(clamp(q.y, -1.0, 1.0));
    let phase0 = gg_phase(lat0, prm);
    let wind = gg_wind(lat0, phase0, prm);

    // Shear the turbulence along the jets, then stretch it along longitude.
    let shear = GG_SHEAR * wind;
    let cs = cos(shear);
    let sn = sin(shear);
    let sheared = vec3<f32>(cs * q.x - sn * q.z, q.y, sn * q.x + cs * q.z);
    let frequency = 1.6 * prm.bands + 3.0;
    let octaves = u32(clamp(log2(0.5 / max(footprint * frequency, 1.0e-5)), 3.0, 8.0));
    let aniso = vec3<f32>(sheared.x, sheared.y * GG_STRETCH, sheared.z) * frequency + seed_offset;
    // Two-level domain warp folds the bands into filaments.
    let warp = vec3<f32>(
        gg_fbm(aniso + vec3<f32>(17.1, 0.0, 3.7), 4u),
        0.0,
        gg_fbm(aniso + vec3<f32>(-5.3, 9.2, 0.0), 4u)
    );
    let turbulence = gg_fbm(aniso + warp * 0.9, octaves);
    let belt_weight = 0.5 - 0.5 * cos(6.2831853 * phase0);
    let flank = abs(sin(6.2831853 * phase0));
    let amplitude = (0.07 + 0.12 * belt_weight + 0.06 * flank) * (1.0 - 0.55 * prm.haze)
        * (1.0 - 0.5 * prm.ice);
    let phase = phase0 + amplitude * turbulence * max(prm.bands, 1.0) * 0.6;

    // Belt/zone profile with per-band character.
    let index = floor(phase + 0.5);
    let hemi = select(0u, 1u, q.y < 0.0);
    let band_seed = prm.seed * 31u + hemi * 977u + u32(index) * 7u;
    let zone_var = rk_rand(band_seed, 11u);
    let belt_var = rk_rand(band_seed, 12u);
    let zoneness = smoothstep(-0.45, 0.45, cos(6.2831853 * phase));
    let colors = gg_zone_belt(prm);
    var zone = colors[0] * (0.94 + 0.12 * zone_var);
    var belt = colors[1] * (0.82 + 0.3 * belt_var);
    // Some belts are redder or paler than others; the equatorial zone
    // carries an ochre tint at times.
    belt *= mix(vec3<f32>(1.0), vec3<f32>(1.06, 0.97, 0.88), belt_var * (1.0 - prm.ice));
    zone = mix(zone, zone * vec3<f32>(1.02, 0.96, 0.86), select(0.0, zone_var * 0.6, index == 0.0) * (1.0 - prm.ice));
    let mean = 0.5 * (colors[0] + colors[1]);
    let contrast = gg_contrast(prm);
    var color = mix(mean, mix(belt, zone, zoneness), contrast);
    // Fine streaks: brighter where turbulence lifts fresh ice.
    color *= 1.0 + 0.10 * turbulence * contrast;

    let lat = abs(lat0);
    let lon = atan2(q.z, q.x);
    // Hot spots on the equatorward edge of the north equatorial belt:
    // ~11 dark, cloud-free projections between bright plumes (Jupiter).
    let hot_band = exp(-pow((phase - 0.27) / 0.06, 2.0)) * select(0.0, 1.0, q.y > 0.0);
    let hot = hot_band * smoothstep(0.55, 0.9, cos(11.0 * lon + 2.0 * turbulence))
        * (1.0 - prm.haze) * (1.0 - prm.ice) * contrast;
    color = mix(color, color * vec3<f32>(0.55, 0.6, 0.72), hot);

    // Bright methane-ice clouds above the deck on ice giants, strung along
    // the mid-latitude jets.
    if (prm.ice > 0.0) {
        let streak = gg_fbm(vec3<f32>(sheared.x, sheared.y * 4.0, sheared.z) * 3.5 + seed_offset * 1.7, 5u);
        let latitudes = smoothstep(0.35, 0.55, lat) * (1.0 - smoothstep(1.0, 1.2, lat));
        let bright = smoothstep(0.42, 0.62, streak) * latitudes * prm.ice * clamp(prm.storms * 1.5, 0.0, 1.0);
        color = mix(color, vec3<f32>(0.90, 0.93, 0.95), bright * 0.8);
        // Methane-depleted poles and the sunlit polar hood (Uranus).
        let summer = select(prm.season - 0.5, 0.5 - prm.season, q.y < 0.0) * 2.0;
        let polar = smoothstep(0.65, 1.0, lat) * (0.55 + 0.45 * clamp(summer, 0.0, 1.0));
        let methane = prm.ice * (1.0 - smoothstep(90.0, 160.0, prm.temperature));
        color = mix(color, color * vec3<f32>(1.35, 1.12, 1.0), polar * methane);
    }

    // Vortex interiors and rims.
    if (swirl.inside > 0.0 || swirl.collar > 0.0) {
        var core = colors[0] * 1.06;
        if (swirl.kind == 1u) {
            core = vec3<f32>(0.72, 0.38, 0.24) * (0.8 + 0.2 * contrast);
        } else if (swirl.kind == 2u) {
            core = colors[1] * vec3<f32>(0.45, 0.5, 0.62);
        } else if (swirl.kind == 3u) {
            core = colors[1] * vec3<f32>(0.72, 0.58, 0.48);
        }
        let interior = swirl.inside * (0.85 + 0.15 * turbulence);
        color = mix(color, core, clamp(interior, 0.0, 1.0) * mix(0.35, 1.0, contrast + prm.ice));
        // White collars (and Neptune's companion clouds) edge the ovals.
        let rim = select(colors[0] * 1.1, vec3<f32>(0.92, 0.94, 0.96), swirl.kind == 2u);
        color = mix(color, rim, swirl.collar * 0.35 * (1.0 - swirl.inside));
    }

    // Polar region: no bands, a mottled, bluish-grey haze with cyclones
    // (Jupiter) or a hexagonal jet around a polar vortex (Saturn).
    let polar_lat = gg_polar_latitude(prm);
    let polar = smoothstep(polar_lat - 0.06, polar_lat + 0.06, lat) * (1.0 - prm.ice);
    if (polar > 0.0) {
        let colatitude = GG_HALF_PI - lat;
        let mottling = gg_fbm(q0 * 14.0 + seed_offset, 5u);
        let mean_deck = 0.5 * (colors[0] + colors[1]);
        var pole = mix(colors[1], mean_deck * vec3<f32>(0.74, 0.8, 0.92), 0.55 - 0.4 * prm.haze)
            * (0.9 + 0.2 * mottling);
        if (prm.bands > 5.0 || q0.y < 0.0 || prm.haze < 0.3) {
            // Cyclone clusters: 8 around the north pole, 5 around the south;
            // a lone polar vortex at Saturn's south pole.
            var cyclone = exp(-colatitude * colatitude / 0.0012);
            if (prm.bands > 5.0) {
                let n = select(5.0, 8.0, q0.y > 0.0);
                let ring_lon = atan2(q0.z, q0.x) + rk_rand(prm.seed, 0x51u) * 6.2831853;
                let sector = (fract(ring_lon * n / 6.2831853) - 0.5) * 6.2831853 / n;
                let ring_d = vec2<f32>(sector * sin(0.13), colatitude - 0.13);
                cyclone += exp(-dot(ring_d, ring_d) / 0.0012);
            }
            pole = mix(pole, pole * vec3<f32>(1.25, 1.22, 1.15), clamp(cyclone, 0.0, 1.0) * 0.7);
        } else {
            // Hexagon (wavenumber 6) near 76°: the jet's meander bounds a
            // darker, bluer polar cap.
            let lon_q = atan2(q0.z, q0.x);
            let sector = fract(lon_q * 6.0 / 6.2831853) - 0.5;
            let hexagon = (GG_HALF_PI - 1.33) * cos(GG_PI / 6.0) / cos(sector * GG_PI / 3.0);
            let edge = exp(-pow((colatitude - hexagon) / 0.012, 2.0));
            let inside = smoothstep(hexagon + 0.01, hexagon - 0.02, colatitude);
            pole = mix(pole, pole * vec3<f32>(0.86, 0.92, 1.0), inside);
            pole = mix(pole, colors[0], edge * 0.6);
        }
        color = mix(color, pole, polar);
    }
    return max(color, vec3<f32>(0.0));
}

// Limb darkening and phase of a cloud deck, in place of Lambert's μ₀,
// normalised to 1 at the disk centre at zero phase. Jupiter's clear deck
// follows μ₀ times a two-term Henyey-Greenstein phase (g 0.8 / -0.38,
// f 0.9; Tomasko et al. 1978, as used by Dyudina et al. 2005); Saturn's
// hazy one I/F ∝ (μ μ₀ / (μ + μ₀))^B / μ with B 1.48 at zero phase
// falling to 1.34 at 150° (Dones et al. 1993), which keeps the limb
// brighter. `haze` blends between them.
fn gg_hg(g: f32, cos_scatter: f32) -> f32 {
    return (1.0 - g * g) / pow(1.0 + g * g - 2.0 * g * cos_scatter, 1.5);
}

fn gg_photometry(mu0: f32, mu: f32, phase_deg: f32, haze: f32) -> f32 {
    if (mu0 <= 0.0) {
        return 0.0;
    }
    let a = clamp(phase_deg, 0.0, 180.0);
    // Scattering angle cosine: -1 at zero phase (backscatter).
    let cos_scatter = -cos(radians(a));
    let jupiter_phase = (0.9 * gg_hg(0.8, cos_scatter) + 0.1 * gg_hg(-0.38, cos_scatter)) / 0.4143;
    let jupiter = mu0 * jupiter_phase;
    let b = mix(1.48, 1.34, clamp(a / 150.0, 0.0, 1.0));
    let amplitude = select(mix(0.79, 1.32, (a - 90.0) / 60.0), mix(1.0, 0.79, a / 90.0), a < 90.0);
    let m = max(mu, 1.0e-3);
    let saturn = amplitude * 2.7895 * pow(mu0 * m / (m + mu0), b) / m;
    return mix(jupiter, saturn, clamp(haze, 0.0, 1.0));
}

// Thermal glow of a hot giant's deck (roasters above ~900 K), linear RGB
// over the reflected-light scale; dull red, brightening toward orange.
fn gg_emission(prm: GiantParams) -> vec3<f32> {
    let t = prm.temperature;
    let glow = smoothstep(900.0, 2200.0, t);
    return mix(vec3<f32>(0.6, 0.08, 0.01), vec3<f32>(1.0, 0.45, 0.12), smoothstep(1500.0, 2500.0, t)) * glow * glow * 0.35;
}
