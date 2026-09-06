// Preview renderer: samples pre-computed height cubemap + applies biome pipeline.
// Height comes from the tectonic compute pipeline (plates.wgsl).
// Temperature, moisture, biomes computed per-pixel in fragment shader.

struct Uniforms {
    rotation: mat4x4<f32>,
    light_dir: vec3<f32>,
    ocean_level: f32,
    base_temp_c: f32,
    ocean_fraction: f32,
    axial_tilt_rad: f32,
    view_mode: u32,
    season: f32, // 0=winter, 0.5=equinox, 1=summer
    atmosphere_density: f32, // 0.0 = none, 1.0 = Earth-like (reserved)
    atmosphere_height: f32,  // shell cutoff in planet radii, 12 molecular scale heights
    height_scale: f32,       // normal map height exaggeration
    zoom: f32,               // viewport zoom (1.0 = default)
    pan_x: f32,              // viewport pan in NDC units
    pan_y: f32,
    cloud_coverage: f32,     // 0.0 = clear, 1.0 = overcast
    cloud_seed: u32,
    night_lights: f32,       // 0.0 = pristine, 1.0 = urbanized
    star_color_temp: f32,    // 0.0 = blue, 0.5 = sun, 1.0 = red dwarf
    city_light_hue: f32,    // 0.0 = warm amber, 0.5 = white, 1.0 = cool blue
    show_ao: f32,           // 1.0 = enabled, 0.0 = disabled
    // Layer toggles (1.0 = enabled, 0.0 = disabled)
    show_water: f32,
    show_ice: f32,
    show_biomes: f32,
    show_clouds: f32,
    show_atmosphere_layer: f32,
    show_cities: f32,
    cloud_opacity: f32,    // 0.0 = transparent, 1.0 = full opacity
    // Historical name: selects persistent GPU wind/continentality textures;
    // zero uses the analytical fallback for no-wind cases.
    cloud_advection: f32,
    rotation_rate: f32,    // relative to Earth (1.0 = 24h day)
    atm_pressure: f32,     // atmospheric pressure in bar (1.0 = Earth)
    _pad4: f32,
    lava_glow: f32,        // tectonic emission intensity (0.0-1.0)
    ring_inner: f32,       // ring inner radius (planet radii, 0 = disabled)
    ring_outer: f32,       // ring outer radius
    ring_tilt: f32,        // ring plane tilt (radians)
    ring_opacity: f32,     // ring opacity (0-1)
    planet_radius_km: f32,
    show_cloud_shadows: f32,
    _pad5: f32,
}

const CLOUD_RAY_SAMPLES: u32 = 8u;

@group(0) @binding(0) var<uniform> uniforms: Uniforms;
@group(0) @binding(1) var height_tex: texture_cube<f32>;
@group(0) @binding(2) var height_sampler: sampler;
@group(0) @binding(3) var cloud_tex: texture_cube<f32>;
@group(0) @binding(4) var weather_mass_tex: texture_cube<f32>;
@group(0) @binding(5) var weather_geometry_tex: texture_cube<f32>;

// Sample wind+continentality cubemap: RGBA = (wind.x, wind.y, wind.z, continentality)
fn sample_wind_cont(dir: vec3<f32>) -> vec4<f32> {
    return textureSample(cloud_tex, height_sampler, dir);
}

// Sample pressure-derived 3D wind vector from cubemap
fn sample_wind_field(dir: vec3<f32>) -> vec3<f32> {
    return textureSample(cloud_tex, height_sampler, dir).xyz;
}

struct WindTangent {
    direction: vec3<f32>,
    magnitude: f32,
}

fn tangent_basis(direction: vec3<f32>) -> mat2x3<f32> {
    let reference = select(
        vec3<f32>(0.0, 1.0, 0.0),
        vec3<f32>(1.0, 0.0, 0.0),
        abs(direction.y) > 0.9,
    );
    let first = cross(reference, direction);
    let first_length = max(length(first), 1.0e-6);
    let tangent_x = first / first_length;
    return mat2x3<f32>(tangent_x, cross(direction, tangent_x));
}

// Uses GPU-computed pressure wind when available, falling back to analytical flow.
// The magnitude is preserved for cloud-detail filtering; callers needing only direction
// should use sample_wind_tangent.
fn sample_wind_tangent_data(sphere_pos: vec3<f32>) -> WindTangent {
    if (uniforms.cloud_advection > 0.5) {
        let basis = tangent_basis(sphere_pos);
        let t1 = basis[0];
        let t2 = basis[1];
        let r = 0.05; // ~320km blur radius
        let w = sample_wind_field(sphere_pos) * 2.0
              + sample_wind_field(normalize(sphere_pos + (t1 + t2) * r))
              + sample_wind_field(normalize(sphere_pos - (t1 + t2) * r));
        let tangent = w - sphere_pos * dot(w, sphere_pos);
        let speed = length(tangent);
        if (speed > 0.003) { return WindTangent(tangent / speed, speed * 0.25); }
    }
    let tilt = uniforms.axial_tilt_rad;
    let tilted_y = sphere_pos.y * cos(tilt) + sphere_pos.z * sin(tilt);
    let lat = asin(clamp(tilted_y, -1.0, 1.0));
    return WindTangent(wind_direction_at(sphere_pos, lat), 0.0);
}

fn sample_wind_tangent(sphere_pos: vec3<f32>) -> vec3<f32> {
    return sample_wind_tangent_data(sphere_pos).direction;
}

// Sample continentality (0=coast, 1=deep interior) from cubemap alpha
fn sample_continentality(dir: vec3<f32>) -> f32 {
    return textureSample(cloud_tex, height_sampler, dir).a;
}

// Wide-blur continentality for climate-scale effects (clouds, monsoon).
// Averages over ~350km radius to prevent cloud edges from tracing fine coastline detail.
// Uses tangent-plane diagonal offsets per cubemap blur best practice.
fn sample_continentality_wide(pos: vec3<f32>) -> f32 {
    var up_ref = vec3<f32>(0.0, 1.0, 0.0);
    if (abs(pos.y) > 0.95) { up_ref = vec3<f32>(1.0, 0.0, 0.0); }
    let t1 = normalize(cross(up_ref, pos));
    let t2 = normalize(cross(pos, t1));
    let r = 0.06; // ~350km on Earth-sized sphere
    return (sample_continentality(pos)
          + sample_continentality(normalize(pos + (t1 + t2) * r))
          + sample_continentality(normalize(pos + (t1 - t2) * r))
          + sample_continentality(normalize(pos - (t1 - t2) * r))
          + sample_continentality(normalize(pos - (t1 + t2) * r))) * 0.2;
}


struct VertexOutput {
    @builtin(position) position: vec4<f32>,
    @location(0) uv: vec2<f32>,
}

// Star color from temperature slider: 0=blue O-star, 0.5=sun G-star, 1.0=red M-dwarf
fn star_color(temp: f32) -> vec3<f32> {
    // Blue (O/B) → White (A/F) → Yellow (G) → Orange (K) → Red (M)
    let blue = vec3<f32>(0.6, 0.7, 1.0);
    let white = vec3<f32>(1.0, 1.0, 1.0);
    // Daylight white balance: the Sun is near-white from space, not amber.
    let yellow = vec3<f32>(1.0, 0.98, 0.96);
    let orange = vec3<f32>(1.0, 0.75, 0.5);
    let red = vec3<f32>(1.0, 0.5, 0.3);

    if (temp < 0.25) {
        return mix(blue, white, temp * 4.0);
    } else if (temp < 0.5) {
        return mix(white, yellow, (temp - 0.25) * 4.0);
    } else if (temp < 0.75) {
        return mix(yellow, orange, (temp - 0.5) * 4.0);
    }
    return mix(orange, red, (temp - 0.75) * 4.0);
}

fn smooth_step(edge0: f32, edge1: f32, x: f32) -> f32 {
    let t = clamp((x - edge0) / (edge1 - edge0), 0.0, 1.0);
    return t * t * (3.0 - 2.0 * t);
}

@vertex
fn vs_main(@builtin(vertex_index) idx: u32) -> VertexOutput {
    var out: VertexOutput;
    let x = f32(i32(idx) / 2) * 4.0 - 1.0;
    let y = f32(i32(idx) % 2) * 4.0 - 1.0;
    out.position = vec4<f32>(x, y, 0.0, 1.0);
    out.uv = vec2<f32>(x, -y) * 0.5 + 0.5;
    return out;
}

// ---- Ray-marched atmosphere ----

struct ScatterResult {
    in_scatter: vec3<f32>,
    transmittance: vec3<f32>,
}

// Shared incident illumination/exposure for surface and participating media.
const SUN_IRRADIANCE: f32 = 3.14159;

// Henyey-Greenstein phase function for Mie scattering.
// g > 0: strong forward scattering (bright glow around sun).
fn henyey_greenstein(cos_theta: f32, g: f32) -> f32 {
    let g2 = g * g;
    return (1.0 - g2) / (4.0 * 3.14159 * pow(1.0 + g2 - 2.0 * g * cos_theta, 1.5));
}

// Coefficients are per planet-radius distance; density is relative surface pressure.
fn atmosphere_scale_height() -> f32 {
    return max(uniforms.atmosphere_height / 12.0, 1.0e-6);
}

fn atmosphere_extinction(pos: vec3<f32>) -> vec3<f32> {
    let altitude = max(length(pos) - 1.0, 0.0);
    let scale_h = atmosphere_scale_height();
    let molecular = exp(-altitude / scale_h);
    let aerosol = exp(-altitude / (scale_h * 0.15));
    return (vec3<f32>(0.005802, 0.013558, 0.033100) * molecular
        + vec3<f32>(0.004440) * aerosol)
        * max(uniforms.atmosphere_density, 0.0) * max(uniforms.planet_radius_km, 1.0);
}

fn atmosphere_sun_transmittance(pos: vec3<f32>, sun: vec3<f32>) -> vec3<f32> {
    if (ray_sphere_positive_intersection(pos, sun, 1.0) >= 0.0) {
        return vec3<f32>(0.0);
    }
    if (uniforms.atmosphere_density <= 0.0 || uniforms.show_atmosphere_layer < 0.5) {
        return vec3<f32>(1.0);
    }
    let radius = 1.0 + uniforms.atmosphere_height;
    let b = dot(pos, sun);
    let exit_distance = max(-b + sqrt(max(b * b + radius * radius - dot(pos, pos), 0.0)), 0.0);
    var optical_depth = vec3<f32>(0.0);
    // Quadratic spacing resolves dense air near the sample, including curved
    // twilight paths. No secant clamp or horizon brightness floor.
    for (var i = 0u; i < 6u; i++) {
        let a = f32(i) / 6.0;
        let bstep = f32(i + 1u) / 6.0;
        let start = exit_distance * a * a;
        let end = exit_distance * bstep * bstep;
        optical_depth += atmosphere_extinction(pos + sun * ((start + end) * 0.5)) * (end - start);
    }
    return exp(-optical_depth);
}

fn atmosphere_segment(pos: vec3<f32>, distance: f32, sun: vec3<f32>) -> ScatterResult {
    var result = ScatterResult(vec3<f32>(0.0), vec3<f32>(1.0));
    if (uniforms.atmosphere_density <= 0.0 || uniforms.show_atmosphere_layer < 0.5
        || uniforms.atmosphere_height <= 0.0 || length(pos) < 1.0
        || length(pos) > 1.0 + uniforms.atmosphere_height) { return result; }
    let altitude = max(length(pos) - 1.0, 0.0);
    let scale_h = atmosphere_scale_height();
    let factor = uniforms.atmosphere_density * max(uniforms.planet_radius_km, 1.0);
    let beta_r = vec3<f32>(0.005802, 0.013558, 0.033100) * factor * exp(-altitude / scale_h);
    let beta_m = vec3<f32>(0.003996) * factor * exp(-altitude / (scale_h * 0.15));
    let extinction = atmosphere_extinction(pos);
    let cos_theta = -sun.z;
    let source = (beta_r * (0.0596831 * (1.0 + cos_theta * cos_theta))
        + beta_m * henyey_greenstein(cos_theta, 0.76))
        * atmosphere_sun_transmittance(pos, sun) * star_color(uniforms.star_color_temp) * SUN_IRRADIANCE;
    result.transmittance = exp(-extinction * abs(distance));
    result.in_scatter = source / max(extinction, vec3<f32>(1.0e-8)) * (vec3<f32>(1.0) - result.transmittance);
    return result;
}

fn ray_march_atmosphere(ndc: vec2<f32>, z_start: f32, z_end: f32, sun_dir: vec3<f32>) -> ScatterResult {
    var result = ScatterResult(vec3<f32>(0.0), vec3<f32>(1.0));
    let span = max(z_start - z_end, 0.0);
    for (var i = 0u; i < 12u; i++) {
        let a = f32(i) / 12.0;
        let b = f32(i + 1u) / 12.0;
        // Concentrate near the surface for front hemisphere rays. Limb paths
        // remain symmetric to resolve their densest air near the tangent.
        let start_t = select(a, 1.0 - (1.0 - a) * (1.0 - a), z_end >= 0.0);
        let end_t = select(b, 1.0 - (1.0 - b) * (1.0 - b), z_end >= 0.0);
        let segment = atmosphere_segment(vec3<f32>(ndc, z_start - (start_t + end_t) * span * 0.5), (end_t - start_t) * span, sun_dir);
        result.in_scatter += result.transmittance * segment.in_scatter;
        result.transmittance *= segment.transmittance;
    }
    return result;
}

fn ray_march_clouds(
    ndc: vec2<f32>,
    z_start: f32,
    z_end: f32,
    sun_dir: vec3<f32>,
    angular_pixel_footprint: f32,
) -> ScatterResult {
    let step_len = (z_start - z_end) / f32(CLOUD_RAY_SAMPLES);
    let radius_km = max(uniforms.planet_radius_km, 1.0);
    let display_scale = cloud_display_scale();
    // Ray-anchor jitter is deterministic in world space, avoiding camera-space shimmer.
    let ray_anchor = normalize((uniforms.rotation * vec4<f32>(normalize(vec3<f32>(ndc, 0.5)), 0.0)).xyz);
    let start_jitter = 0.5 + snoise(ray_anchor * 47.0 + noise_seed_offset(uniforms.cloud_seed, 45u)) * 0.005;
    let sun_world = normalize((uniforms.rotation * vec4<f32>(sun_dir, 0.0)).xyz);
    var transmittance = vec3<f32>(1.0);
    var in_scatter = vec3<f32>(0.0);

    for (var i = 0u; i < CLOUD_RAY_SAMPLES; i++) {
        let segment_start = vec3<f32>(ndc, z_start - f32(i) * step_len);
        let segment_end = vec3<f32>(ndc, z_start - f32(i + 1u) * step_len);
        let z = z_start - (f32(i) + start_jitter) * step_len;
        let pos = vec3<f32>(ndc, z);
        let altitude_km = max((length(pos) - 1.0) * radius_km, 0.0);
        let direction = normalize(pos);
        let world = (uniforms.rotation * vec4<f32>(direction, 0.0)).xyz;
        let sample = weather_cloud_layers_land_segment(
            world, altitude_km, segment_start, segment_end, radius_km, angular_pixel_footprint,
        );
        let layers = sample.layers;
        let extinction = (layers.low * 0.90 + layers.deep * 1.65 + layers.high * 0.32) * display_scale;


        let world_pos = world * (1.0 + altitude_km / radius_km);
        let light_transmittance = cloud_sun_path_transmittance(world_pos, sun_world, radius_km, layers, sample.geometry);
        let sun_transmit = atmosphere_sun_transmittance(pos, sun_dir);
        let sun_facing = smooth_step(-0.025, 0.16, dot(direction, sun_dir));
        let star = star_color(uniforms.star_color_temp);
        // Orthographic rays are all +Z to the camera; phase must not vary with
        // screen position.  Incident energy is zero in the planet's shadow,
        // leaving only a deliberately tiny blue night ambient.
        let phase = cloud_phase(-sun_dir.z) * (4.0 * 3.14159);
        let direct_energy = sun_facing * light_transmittance * (0.8 + 0.2 * min(phase, 2.5)) * SUN_IRRADIANCE;
        let night_ambient = vec3<f32>(0.0015, 0.0020, 0.0035);
        let low_color = night_ambient + vec3<f32>(1.0, 1.0, 0.98) * star * sun_transmit * direct_energy * 0.72;
        let deep_color = night_ambient * 0.55 + vec3<f32>(0.92, 0.94, 0.96) * star * sun_transmit * direct_energy * 0.48;
        let high_color = night_ambient * 1.3 + vec3<f32>(0.88, 0.94, 1.0) * star * sun_transmit * direct_energy * 0.78;
        let cloud_color = (low_color * layers.low * 0.90
            + deep_color * layers.deep * 1.65
            + high_color * layers.high * 0.32) / max(extinction / display_scale, 0.0001);
        let segment_transmittance = exp(-extinction * abs(step_len) * radius_km * CLOUD_LIGHT_EXTINCTION);
        let segment_alpha = 1.0 - segment_transmittance;
        // Symmetric air/cloud/air integration, all in linear radiance. Air below
        // an opaque cloud is attenuated by that cloud, never added over it.
        let air_half = atmosphere_segment(pos, abs(step_len) * 0.5, sun_dir);
        in_scatter += transmittance * air_half.in_scatter;
        transmittance *= air_half.transmittance;
        in_scatter += cloud_color * transmittance * segment_alpha;
        transmittance *= segment_transmittance;
        in_scatter += transmittance * air_half.in_scatter;
        transmittance *= air_half.transmittance;
        if (max(max(transmittance.r, transmittance.g), transmittance.b) < 0.01) { break; }
    }

    var result: ScatterResult;
    result.in_scatter = in_scatter;
    result.transmittance = transmittance;
    return result;
}

// Composite from the background toward the camera. The cloud interval includes
// its own air; integrate only the remaining atmosphere outside that interval.
fn composite_volumes(background: vec3<f32>, ndc: vec2<f32>, surface_z: f32,
    cloud_radius: f32, sun: vec3<f32>, footprint: f32) -> vec3<f32> {
    let r2 = dot(ndc, ndc);
    let atm_radius = 1.0 + max(uniforms.atmosphere_height, 0.0);
    let za = sqrt(max(atm_radius * atm_radius - r2, 0.0));
    let back = select(-za, surface_z, surface_z >= 0.0);
    var color = background;
    if (cloud_display_scale() > 0.0 && r2 < cloud_radius * cloud_radius) {
        let zc = sqrt(max(cloud_radius * cloud_radius - r2, 0.0));
        let cloud_back = select(-zc, surface_z, surface_z >= 0.0);
        if (surface_z < 0.0 && za > zc) {
            let far_air = ray_march_atmosphere(ndc, -zc, -za, sun);
            color = color * far_air.transmittance + far_air.in_scatter;
        }
        let clouds = ray_march_clouds(ndc, zc, cloud_back, sun, footprint);
        color = color * clouds.transmittance + clouds.in_scatter;
        if (za > zc) {
            let near_air = ray_march_atmosphere(ndc, za, zc, sun);
            color = color * near_air.transmittance + near_air.in_scatter;
        }
    } else {
        let air = ray_march_atmosphere(ndc, za, back, sun);
        color = color * air.transmittance + air.in_scatter;
    }
    return color;
}

// ---- Temperature ----
fn compute_temperature(sphere_pos: vec3<f32>, height: f32, season: f32) -> f32 {
    let effective_lat = climate_latitude(sphere_pos, uniforms.axial_tilt_rad);
    let lat_normalized = min(abs(climate_thermal_latitude(sphere_pos, uniforms.axial_tilt_rad, season)) / 1.5707963, 1.0);
    let baseline = climate_temperature(sphere_pos, height, uniforms.ocean_level, uniforms.base_temp_c, uniforms.axial_tilt_rad, season);

    // Preview-only diagnostic ocean-current anomaly, separate from shared baseline.
    // === Ocean current approximation ===
    // Western coasts → warm poleward currents (Gulf Stream, Kuroshio)
    // Eastern coasts → cold equatorward currents (California, Benguela)
    // Wind-derived "east" direction: Ekman transport deflects surface water 90° from wind.
    // Using wind direction makes currents respond to pressure-derived wind patterns.
    var current_temp = 0.0;
    let is_ocean = height < uniforms.ocean_level;
    if (is_ocean) {
        // Wind-derived east: perpendicular to wind on the tangent plane
        // This approximates Ekman transport direction
        let wind_tang = sample_wind_tangent(sphere_pos);
        let east_dir = normalize(cross(sphere_pos, wind_tang));

        // Detect land proximity using CONTINENTALITY cubemap — no ghost rings.
        // Continentality is smoothly diffused (80 iterations), so probing east/west
        // gives a gradual signal instead of the discrete steps from height probes.
        var land_east_score: f32;
        var land_west_score: f32;
        if (uniforms.cloud_advection > 0.5) {
            // Probe continentality at two distances for broad + near detection
            let cont_e1 = sample_continentality(normalize(sphere_pos + east_dir * 0.08));
            let cont_w1 = sample_continentality(normalize(sphere_pos - east_dir * 0.08));
            let cont_e2 = sample_continentality(normalize(sphere_pos + east_dir * 0.18));
            let cont_w2 = sample_continentality(normalize(sphere_pos - east_dir * 0.18));
            // Smooth ramp: higher continentality = more definitely land in that direction
            land_east_score = smooth_step(0.0, 0.4, cont_e1 * 0.6 + cont_e2 * 0.4);
            land_west_score = smooth_step(0.0, 0.4, cont_w1 * 0.6 + cont_w2 * 0.4);
        } else {
            // Fallback without cubemap: single smooth probe
            let ol = uniforms.ocean_level;
            let he = textureSample(height_tex, height_sampler, normalize(sphere_pos + east_dir * 0.12)).r;
            let hw = textureSample(height_tex, height_sampler, normalize(sphere_pos - east_dir * 0.12)).r;
            land_east_score = smooth_step(ol - 0.03, ol + 0.10, he);
            land_west_score = smooth_step(ol - 0.03, ol + 0.10, hw);
        }

        let season_angle = (uniforms.season - 0.5) * 2.0;
        let winter_boost = 1.0 + clamp(-effective_lat * season_angle * 2.0, 0.0, 0.5);
        let lat_strength = 1.0 - abs(lat_normalized);

        // Western boundary current (Gulf Stream): warm where land is to the WEST
        current_temp += land_west_score * 4.0 * lat_strength * winter_boost;
        // Eastern boundary current (California): cold upwelling where land is to the EAST
        current_temp -= land_east_score * 3.0 * lat_strength * winter_boost;
    }

    return baseline + current_temp;
}

fn thermal_latitude(sphere_pos: vec3<f32>, season: f32) -> f32 {
    return climate_thermal_latitude(sphere_pos, uniforms.axial_tilt_rad, season);
}

fn compute_sea_level_temperature(sphere_pos: vec3<f32>, season: f32) -> f32 {
    return climate_sea_level_temperature(sphere_pos, uniforms.base_temp_c, uniforms.axial_tilt_rad, season);
}

// Polar caps are a sea-level climate feature. Keeping altitude out of this
// mask leaves mountain snow to its separate altitude treatment.
fn polar_ice_coverage(sphere_pos: vec3<f32>, is_ocean: bool) -> f32 {
    let annual_temp = compute_sea_level_temperature(sphere_pos, 0.5);
    let seasonal_temp = compute_sea_level_temperature(sphere_pos, uniforms.season);
    let climate_temp = mix(annual_temp, seasonal_temp, 0.35);
    let thermal_lat = abs(thermal_latitude(sphere_pos, uniforms.season));
    let polar_domain = smooth_step(0.80, 1.00, thermal_lat);
    // Low-frequency breakup only; this is bounded to ±0.55°C at the edge.
    let edge_temp = 0.5 + snoise(sphere_pos * 0.7 + vec3<f32>(31.0, 0.0, 0.0)) * 0.55;
    var coverage = polar_domain * smooth_step(edge_temp, edge_temp - 5.0, climate_temp);

    if (!is_ocean) {
        let height = textureSample(height_tex, height_sampler, sphere_pos).r;
        let land_height = clamp(
            (height - uniforms.ocean_level),
            0.0,
            1.0,
        );
        let lowland = 1.0 - smooth_step(0.15, 0.45, land_height);
        // Polar-cap coverage ends below the alpine terrain bands; DS-027 owns those materials.
        coverage *= lowland;
        let moisture = compute_moisture(sphere_pos, height, 0.5);
        // Dry lowlands remain polar desert/rock instead of becoming a white cap.
        coverage *= mix(1.0, smooth_step(25.0, 65.0, moisture), lowland);
    }
    return coverage;
}

fn wind_height_sample(direction: vec3<f32>) -> f32 {
    return textureSample(height_tex, height_sampler, direction).r;
}

// ---- Hadley cell moisture ----
fn hadley_cell_moisture(latitude_rad: f32) -> f32 {
    let lat_deg = abs(latitude_rad) * 180.0 / 3.14159;
    let hadley_lat = preview_hadley_top();
    let polar_lat = preview_subpolar_lat();

    // ITCZ: tropical wet belt (always centered near equator)
    let itcz_wet = exp(-lat_deg * lat_deg / 200.0) * 200.0;
    // Subtropical dry: centered at Hadley cell top (rotation-dependent)
    let subtropical_dry = -80.0 * exp(-((lat_deg - hadley_lat) * (lat_deg - hadley_lat)) / 60.0);
    // Mid-latitude wet belt: between Hadley top and subpolar low
    let midlat_center = (hadley_lat + polar_lat) * 0.5;
    let polar_front_wet = 90.0 * exp(-((lat_deg - midlat_center) * (lat_deg - midlat_center)) / 200.0);
    // Polar drying
    let polar_dry = -60.0 * smooth_step(polar_lat + 5.0, polar_lat + 25.0, lat_deg);
    // Higher base ensures most temperate land has enough moisture for vegetation
    return max(itcz_wet + subtropical_dry + polar_front_wet + polar_dry + 90.0, 10.0);
}


fn compute_moisture(sphere_pos: vec3<f32>, height: f32, season: f32) -> f32 {
    let tilt = uniforms.axial_tilt_rad;
    let tilted_y = sphere_pos.y * cos(tilt) + sphere_pos.z * sin(tilt);
    let effective_lat = asin(clamp(tilted_y, -1.0, 1.0));

    // Shift Hadley cells with thermal equator (same sub-solar shift as temperature)
    let season_angle = (season - 0.5) * 2.0;
    let sub_solar_lat = tilt * season_angle;

    // Monsoon: ITCZ shifts poleward over large continents in summer.
    // Uses smooth land detection to avoid continent-shaped artifacts in cloud coverage.
    var land_score: f32;
    if (uniforms.cloud_advection > 0.5) {
        land_score = sample_continentality_wide(sphere_pos);
    } else {
        // Smooth height transition — NOT binary select which creates continent outlines
        let local_h = textureSample(height_tex, height_sampler, sphere_pos).r;
        land_score = smooth_step(uniforms.ocean_level - 0.05, uniforms.ocean_level + 0.20, local_h) * 0.5;
    }
    // Reduced magnitude (was 15°): only deep interior (land_score > 0.5) shifts ITCZ noticeably
    let monsoon_pull = land_score * 8.0 * 3.14159 / 180.0 * season_angle;
    let thermal_lat = effective_lat - sub_solar_lat - monsoon_pull;

    // Hadley cell base moisture — scaled by ocean fraction FIRST.
    // Softened ocean scaling: low-water worlds still get some moisture
    let ocean_scale = 0.25 + 0.75 * uniforms.ocean_fraction;
    let hadley_base = hadley_cell_moisture(thermal_lat) * ocean_scale;

    // Local noise variation (breaks latitude bands)
    let noise1 = snoise(sphere_pos * 3.0 + vec3<f32>(100.0, 0.0, 0.0));
    let local_var = noise1 * 0.5;
    var moisture = hadley_base * (0.55 + 0.45 * (local_var + 0.5));
    moisture += 50.0 * (local_var + 0.5) * ocean_scale;

    // === Coast/interior moisture gradient ===
    // GPU-computed continentality (0=coast/ocean, ~0.8=deep interior) from cubemap
    // provides a much better signal than inline neighbor sampling, using 80 iterations
    // of diffusion. Wide-blur sample prevents moisture from tracking fine coastline detail.
    let is_land = height > uniforms.ocean_level;
    if (is_land) {
        var continentality: f32;
        if (uniforms.cloud_advection > 0.5) {
            continentality = sample_continentality_wide(sphere_pos);
        } else {
            // Fallback: 4-neighbor coast detection
            let step = 0.06;
            let h_e = textureSample(height_tex, height_sampler, sphere_pos + vec3<f32>(step, 0.0, 0.0)).r;
            let h_w = textureSample(height_tex, height_sampler, sphere_pos + vec3<f32>(-step, 0.0, 0.0)).r;
            let h_n = textureSample(height_tex, height_sampler, sphere_pos + vec3<f32>(0.0, step, 0.0)).r;
            let h_s = textureSample(height_tex, height_sampler, sphere_pos + vec3<f32>(0.0, -step, 0.0)).r;
            var ocean_count = 0.0;
            if (h_e < uniforms.ocean_level) { ocean_count += 1.0; }
            if (h_w < uniforms.ocean_level) { ocean_count += 1.0; }
            if (h_n < uniforms.ocean_level) { ocean_count += 1.0; }
            if (h_s < uniforms.ocean_level) { ocean_count += 1.0; }
            continentality = 1.0 - ocean_count / 4.0;
        }

        // Coast stays moist, deep interior dries out
        // continentality 0=coast → penetration 1.0, continentality 0.8+ → penetration 0.55
        let penetration = mix(1.0, 0.55, smooth_step(0.1, 0.7, continentality));
        moisture *= penetration;

        // === Rain shadow from mountains (>2km relief) ===
        let tangent_wind = sample_wind_tangent(sphere_pos);
        let upwind_pos = normalize(sphere_pos - tangent_wind * 0.08);
        let upwind_h = textureSample(height_tex, height_sampler, upwind_pos).r;
        let upwind_elev = max(upwind_h - uniforms.ocean_level, 0.0);
        let my_elevation = max(height - uniforms.ocean_level, 0.0);
        if (upwind_elev > my_elevation + 0.02) {
            let relief = upwind_elev - my_elevation;
            let shadow_strength = smooth_step(0.02, 0.06, relief) * 0.7;
            moisture *= (1.0 - shadow_strength);
        }
    } else {
        moisture *= 1.3; // Over ocean
    }

    // === Regional moisture character ===
    // Low-frequency noise gives each region a wet or dry personality.
    // This creates "jungle continents" vs "desert continents" at similar latitudes.
    let region_moisture_bias = snoise(sphere_pos * 0.7 + vec3<f32>(500.0, 0.0, 0.0));
    moisture *= 1.0 + region_moisture_bias * 0.25; // ±25% regional variation

    moisture *= 0.5 + uniforms.ocean_fraction;

    // Pressure-dependent precipitation scaling (ExoPlaSim: precip ~ P^(-0.5))
    // Thinner atmospheres cycle water faster → more precipitation per unit moisture
    // Thicker atmospheres suppress evaporation → less precipitation
    let atm_p = max(uniforms.atm_pressure, 0.05);
    moisture *= pow(atm_p, -0.5);

    return clamp(moisture, 0.0, 400.0);
}

// Clean grayscale elevation — pure height visualization
// Maps terrain range (~-0.5 to ~0.8) to full 0..1 grayscale
fn height_color(h: f32, ocean_level: f32) -> vec3<f32> {
    let v = clamp((h + 0.5) / 1.3, 0.0, 1.0);
    return vec3<f32>(v, v, v);
}

// ---- Continuous gradient biome coloring ----
// Replaces discrete Whittaker lookup with smooth 2D interpolation.
// Temperature × moisture → color via 3×2 anchor grid.

fn gradient_color(mean_temp: f32, mean_moisture: f32, seasonal_temp: f32, variation: f32, region_noise: f32) -> vec3<f32> {
    // 12-biome system: 4 temperature bands × 3 moisture levels
    // Biome classification uses MEAN ANNUAL values for stability
    let r = region_noise; // [0,1] regional sub-variant selector

    // Temperature bands (smooth interpolation weights)
    let t_polar   = 1.0 - smooth_step(-15.0, 0.0, mean_temp);    // <0°C: ice/tundra
    let t_boreal  = smooth_step(-10.0, 2.0, mean_temp) * (1.0 - smooth_step(8.0, 18.0, mean_temp));
    let t_temperate = smooth_step(5.0, 15.0, mean_temp) * (1.0 - smooth_step(20.0, 30.0, mean_temp));
    let t_tropical = smooth_step(18.0, 28.0, mean_temp);

    // Moisture bands
    let m_arid = 1.0 - smooth_step(15.0, 40.0, mean_moisture);   // <25mm: desert
    let m_semi = smooth_step(15.0, 35.0, mean_moisture) * (1.0 - smooth_step(55.0, 90.0, mean_moisture));
    let m_wet  = smooth_step(50.0, 90.0, mean_moisture);          // >70mm: forest/jungle

    // === 12 biome anchor colors with regional sub-variants ===
    // Polar
    let ice_desert    = mix(vec3<f32>(0.72, 0.75, 0.80), vec3<f32>(0.60, 0.58, 0.55), r); // cold dry
    let tundra        = mix(vec3<f32>(0.55, 0.58, 0.45), vec3<f32>(0.48, 0.52, 0.38), r); // cold semi: lichen/moss
    let polar_wet     = mix(vec3<f32>(0.62, 0.68, 0.65), vec3<f32>(0.52, 0.60, 0.50), r); // cold wet: boggy tundra

    // Boreal
    let cold_steppe   = mix(vec3<f32>(0.58, 0.48, 0.32), vec3<f32>(0.52, 0.42, 0.28), r); // cool dry steppe
    let boreal_forest = mix(vec3<f32>(0.12, 0.28, 0.10), vec3<f32>(0.18, 0.32, 0.14), r); // dark conifer
    let boreal_bog    = mix(vec3<f32>(0.15, 0.30, 0.12), vec3<f32>(0.22, 0.35, 0.18), r); // wet taiga

    // Temperate
    let med_scrub     = mix(mix(vec3<f32>(0.55, 0.50, 0.30), vec3<f32>(0.62, 0.42, 0.24), r),
                             vec3<f32>(0.48, 0.44, 0.28), smooth_step(0.7, 1.0, r)); // Mediterranean
    let temp_forest   = mix(mix(vec3<f32>(0.14, 0.38, 0.10), vec3<f32>(0.22, 0.42, 0.15), r),
                             vec3<f32>(0.10, 0.30, 0.08), smooth_step(0.6, 1.0, r)); // deciduous/mixed
    let temp_rain     = mix(vec3<f32>(0.08, 0.34, 0.08), vec3<f32>(0.12, 0.38, 0.10), r); // temperate rainforest

    // Tropical
    let hot_desert    = mix(mix(vec3<f32>(0.85, 0.75, 0.55), vec3<f32>(0.75, 0.45, 0.25), r),
                             vec3<f32>(0.40, 0.32, 0.25), smooth_step(0.7, 1.0, r)); // sand/red/volcanic
    let savanna       = mix(vec3<f32>(0.52, 0.48, 0.22), vec3<f32>(0.42, 0.40, 0.18), r); // dry grassland
    let tropical_rain = mix(mix(vec3<f32>(0.06, 0.30, 0.04), vec3<f32>(0.04, 0.24, 0.03), r),
                             vec3<f32>(0.10, 0.28, 0.06), smooth_step(0.5, 1.0, r)); // deep jungle

    // Blend across moisture within each temperature band
    let polar_color = m_arid * ice_desert + m_semi * tundra + m_wet * polar_wet;
    let boreal_color = m_arid * cold_steppe + m_semi * boreal_forest + m_wet * boreal_bog;
    let temp_color = m_arid * med_scrub + m_semi * temp_forest + m_wet * temp_rain;
    let trop_color = m_arid * hot_desert + m_semi * savanna + m_wet * tropical_rain;

    // Blend across temperature bands
    var base = t_polar * polar_color + t_boreal * boreal_color
             + t_temperate * temp_color + t_tropical * trop_color;
    // Normalize blending weights (they don't always sum to 1 due to overlapping smooth_steps)
    let w_sum = t_polar + t_boreal + t_temperate + t_tropical;
    base /= max(w_sum, 0.25); // floor at 0.25 prevents color spikes at band boundaries

    // === Seasonal color modulation ===
    let temp_deviation = seasonal_temp - mean_temp;
    let green_amount = max(base.g - max(base.r, base.b), 0.0);
    if (green_amount > 0.05) {
        let winter_factor = clamp(-temp_deviation / 20.0, 0.0, 1.0);
        let summer_factor = clamp(temp_deviation / 20.0, 0.0, 1.0);
        base += vec3<f32>(0.06, -0.02, -0.03) * winter_factor * green_amount * 2.0;
        base += vec3<f32>(-0.01, 0.02, 0.0) * summer_factor * green_amount;
    }
    if (seasonal_temp < 5.0 && mean_temp < 15.0) {
        let cold_winter = clamp(-temp_deviation / 15.0, 0.0, 1.0);
        base = mix(base, vec3<f32>(0.80, 0.82, 0.85), cold_winter * 0.25 * t_polar);
    }

    // Per-pixel noise for natural texture
    base += base * variation * 0.12;

    return base;
}

// ---- Terrain normal from height cubemap ----
fn compute_terrain_normal(sphere_pos: vec3<f32>, geo_normal: vec3<f32>, footprint: f32) -> vec3<f32> {
    // A geodesic central difference is stable at cube seams and does not turn
    // an arbitrary texture-value delta into an unbounded limb normal.
    let step = max(2.0 / f32(textureDimensions(height_tex).x), footprint);

    // Build tangent frame in CUBEMAP space (consistent with height sampling)
    var up = vec3<f32>(0.0, 1.0, 0.0);
    if (abs(dot(sphere_pos, up)) > 0.99) { up = vec3<f32>(1.0, 0.0, 0.0); }
    let tan_world = normalize(cross(up, sphere_pos));
    let bitan_world = normalize(cross(sphere_pos, tan_world));

    // Sample 4 neighbors in cubemap space
    let h_right = textureSample(height_tex, height_sampler, normalize(sphere_pos + tan_world * step)).r;
    let h_left  = textureSample(height_tex, height_sampler, normalize(sphere_pos - tan_world * step)).r;
    let h_up    = textureSample(height_tex, height_sampler, normalize(sphere_pos + bitan_world * step)).r;
    let h_down  = textureSample(height_tex, height_sampler, normalize(sphere_pos - bitan_world * step)).r;

    // Central differences → height gradient in cubemap space
    let height_scale = clamp(uniforms.height_scale, 0.0, 5.0) * 5.0 / max(uniforms.planet_radius_km, 1.0);
    let dx = (h_right - h_left) * height_scale / (2.0 * step);
    let dy = (h_up - h_down) * height_scale / (2.0 * step);

    // Perturbed normal in cubemap/world space
    let perturbed_world = normalize(sphere_pos - tan_world * dx - bitan_world * dy);

    // Transform back to view space using inverse rotation (transpose of orthogonal matrix)
    let inv_rot = transpose(uniforms.rotation);
    let perturbed_view = normalize((inv_rot * vec4<f32>(perturbed_world, 0.0)).xyz);
    return perturbed_view;
}

fn terrain_slope(sphere_pos: vec3<f32>) -> f32 {
    let step = 0.015;
    let east = textureSample(height_tex, height_sampler, sphere_pos + vec3<f32>(step, 0.0, 0.0)).r;
    let west = textureSample(height_tex, height_sampler, sphere_pos - vec3<f32>(step, 0.0, 0.0)).r;
    let north = textureSample(height_tex, height_sampler, sphere_pos + vec3<f32>(0.0, step, 0.0)).r;
    let south = textureSample(height_tex, height_sampler, sphere_pos - vec3<f32>(0.0, step, 0.0)).r;
    return max(abs(east - west), abs(north - south)) / (2.0 * step);
}

// Mountain snow needs elevation, sustained cold, terrain that can retain it,
// and precipitation. Dry summits still get a sparse residual dusting.
fn mountain_snow_coverage(
    land_height: f32,
    seasonal_temp: f32,
    moisture_cm: f32,
    slope: f32,
) -> f32 {
    let altitude = smooth_step(0.34, 0.82, land_height);
    let thermal = smooth_step(2.0, -14.0, seasonal_temp);
    let slope_retention = smooth_step(5.5, 1.5, slope);
    let moisture_supply = mix(0.12, 1.0, smooth_step(15.0, 75.0, moisture_cm));
    return altitude * thermal * slope_retention * moisture_supply;
}

// ---- Surface roughness from climate and terrain material ----
fn compute_roughness(
    temp_c: f32,
    moisture_cm: f32,
    is_ocean: bool,
    is_ice: bool,
    land_height: f32,
    slope: f32,
    mountain_snow: f32,
) -> f32 {
    if (is_ocean) {
        if (is_ice) { return 0.15; }
        return 0.10;
    }
    // Climate supplies the base material; terrain exposes rough alpine rock on
    // steep high ground, while retained snow restores a smoother surface.
    let desert_rough = smooth_step(50.0, 15.0, moisture_cm) * smooth_step(5.0, 20.0, temp_c); // dry+warm
    let vegetation = smooth_step(30.0, 120.0, moisture_cm) * smooth_step(5.0, 15.0, temp_c); // wet+warm
    let highland_rough = smooth_step(0.35, 0.70, land_height) * 0.08;
    let exposed_rock = smooth_step(0.55, 0.85, land_height) * (1.0 - mountain_snow) * 0.16;
    let slope_rough = smooth_step(0.5, 4.0, slope) * (1.0 - mountain_snow) * 0.18;
    var roughness = 0.55 + highland_rough + exposed_rock + slope_rough;
    roughness += desert_rough * 0.25; // desert roughens
    roughness -= vegetation * 0.15; // dense vegetation slightly smoother
    roughness = mix(roughness, 0.22, mountain_snow);
    // Per-pixel noise from spatial position (NOT temp/moisture which creates sharp biome edges)
    roughness += snoise(vec3<f32>(temp_c * 0.02 + moisture_cm * 0.005, moisture_cm * 0.01 - temp_c * 0.01, temp_c * 0.015)) * 0.06;
    return clamp(roughness, 0.15, 0.85);
}

// ---- Terrain ambient occlusion ----
// Samples height neighbors to darken valleys and crevices.
fn compute_ao(sphere_pos: vec3<f32>) -> f32 {
    let h_center = textureSample(height_tex, height_sampler, sphere_pos).r;

    // Two-radius sampling: wide for broad valleys, narrow for crevices
    // Both use soft thresholds to avoid pixelated edges
    var occlusion = 0.0;

    // Wide radius: catches broad valley shading (smooth)
    let wide = 0.012;
    let w_offsets = array<vec3<f32>, 4>(
        vec3<f32>(wide, 0.0, 0.0), vec3<f32>(-wide, 0.0, 0.0),
        vec3<f32>(0.0, wide, 0.0), vec3<f32>(0.0, -wide, 0.0)
    );
    for (var i = 0; i < 4; i++) {
        let neighbor = textureSample(height_tex, height_sampler, sphere_pos + w_offsets[i]).r;
        let height_diff = max(neighbor - h_center, 0.0);
        occlusion += smooth_step(0.0, 0.15, height_diff) * 0.5; // gentle, wide contribution
    }

    // Narrow radius: catches local detail (subtle)
    let narrow = 0.005;
    let n_offsets = array<vec3<f32>, 4>(
        vec3<f32>(narrow, narrow, 0.0) * 0.707, vec3<f32>(-narrow, narrow, 0.0) * 0.707,
        vec3<f32>(narrow, -narrow, 0.0) * 0.707, vec3<f32>(-narrow, -narrow, 0.0) * 0.707
    );
    for (var j = 0; j < 4; j++) {
        let neighbor = textureSample(height_tex, height_sampler, sphere_pos + n_offsets[j]).r;
        let height_diff = max(neighbor - h_center, 0.0);
        occlusion += smooth_step(0.0, 0.10, height_diff) * 0.3; // subtle, tight contribution
    }

    // Softer darkening: max ~60% darken (was 80%), higher floor
    let ao = 1.0 - occlusion * 0.08;
    return clamp(ao, 0.4, 1.0);
}

// ---- PBR: GGX normal distribution ----
fn ggx_distribution(n_dot_h: f32, roughness: f32) -> f32 {
    let a = roughness * roughness;
    let a2 = a * a;
    let d = n_dot_h * n_dot_h * (a2 - 1.0) + 1.0;
    return a2 / max(3.14159 * d * d, 1.0e-9);
}

// Smith's separable GGX visibility term prevents grazing-angle energy spikes.
fn ggx_smith_g1(n_dot_x: f32, roughness: f32) -> f32 {
    let a = roughness * roughness;
    let nd = clamp(n_dot_x, 0.0, 1.0);
    return 2.0 * nd / max(nd + sqrt(a * a + (1.0 - a * a) * nd * nd), 1.0e-4);
}

fn ggx_smith_visibility(n_dot_l: f32, n_dot_v: f32, roughness: f32) -> f32 {
    return ggx_smith_g1(n_dot_l, roughness) * ggx_smith_g1(n_dot_v, roughness);
}

// ---- PBR: Schlick Fresnel ----
fn fresnel_schlick(h_dot_v: f32, f0: f32) -> f32 {
    return f0 + (1.0 - f0) * pow(1.0 - h_dot_v, 5.0);
}

// ---- Urban density: procedural city placement ----
// Returns 0.0 (wilderness) to 1.0 (dense urban). Based on climate habitability.
fn compute_urban_density(sphere_pos: vec3<f32>, height: f32) -> f32 {
    let dev = uniforms.night_lights;
    if (dev <= 0.0) { return 0.0; }

    // Only on land
    if (height <= uniforms.ocean_level) { return 0.0; }

    let land_h = (height - uniforms.ocean_level) / max(1.0 - uniforms.ocean_level, 0.01);
    let temp = compute_temperature(sphere_pos, height, 0.5);

    // Habitability score — restrict to temperate zones (no arctic/cold cities)
    var score = 0.0;
    // Temperate climate: must be warm enough (8-25°C sweet spot)
    score += smooth_step(8.0, 18.0, temp) * smooth_step(35.0, 22.0, temp) * 0.4;
    // Low elevation preferred
    score += (1.0 - smooth_step(0.0, 0.25, land_h)) * 0.2;
    // Coastal boost
    let stp = 0.06;
    let h_e = textureSample(height_tex, height_sampler, sphere_pos + vec3<f32>(stp, 0.0, 0.0)).r;
    let h_w = textureSample(height_tex, height_sampler, sphere_pos + vec3<f32>(-stp, 0.0, 0.0)).r;
    let h_n = textureSample(height_tex, height_sampler, sphere_pos + vec3<f32>(0.0, stp, 0.0)).r;
    let h_s = textureSample(height_tex, height_sampler, sphere_pos + vec3<f32>(0.0, -stp, 0.0)).r;
    var ocean_near = 0.0;
    if (h_e < uniforms.ocean_level) { ocean_near += 1.0; }
    if (h_w < uniforms.ocean_level) { ocean_near += 1.0; }
    if (h_n < uniforms.ocean_level) { ocean_near += 1.0; }
    if (h_s < uniforms.ocean_level) { ocean_near += 1.0; }
    score += min(ocean_near / 2.0, 1.0) * 0.25;
    // Hard cutoff: no cities below 5°C mean annual
    score *= smooth_step(3.0, 10.0, temp);

    // City pattern: web/dot network instead of blobs
    // Very high frequency for tiny dots
    let dots = snoise(sphere_pos * 120.0) * 0.5 + 0.5;
    let dots2 = snoise(sphere_pos * 250.0 + vec3<f32>(7.3, 2.1, 5.9)) * 0.5 + 0.5;
    // Web-like connections: abs(noise) creates thin lines at zero crossings
    let web1 = 1.0 - abs(snoise(sphere_pos * 60.0 + vec3<f32>(3.1, 8.7, 1.3))) * 2.0;
    let web2 = 1.0 - abs(snoise(sphere_pos * 130.0 + vec3<f32>(11.3, 4.7, 7.1))) * 2.0;
    let webs = max(max(web1, 0.0), max(web2, 0.0));
    // Combine dots + webs
    let city_pattern = max(dots * dots2 * 1.5, webs * 0.6);

    // Cubic dev ramp: very sparse at low values, rapidly grows near 1.0
    // 0.01→0.000001, 0.1→0.001, 0.5→0.125, 1.0→1.0
    let dev_scaled = dev * dev * dev;
    let urban_raw = score * city_pattern;
    let threshold = (1.0 - dev_scaled) * 0.45;
    return smooth_step(threshold, threshold + 0.04, urban_raw);
}

// ---- Starfield + sun orb background ----
fn starfield(ndc: vec2<f32>, sun_dir: vec3<f32>, sun_color: vec3<f32>) -> vec3<f32> {
    var bg = vec3<f32>(0.0, 0.0, 0.0); // pure black space

    // Stars: hash-based bright dots at pseudo-random positions
    // Quantize ndc to a grid, hash each cell to decide if it has a star
    let star_scale = 120.0; // density: higher = more stars
    let cell = floor(ndc * star_scale);
    let cell_uv = fract(ndc * star_scale); // position within cell [0,1]

    // Hash cell coordinates to get pseudo-random star position + brightness
    let h1 = fract(sin(dot(cell, vec2<f32>(127.1, 311.7))) * 43758.5453);
    let h2 = fract(sin(dot(cell, vec2<f32>(269.5, 183.3))) * 28461.6432);
    let h3 = fract(sin(dot(cell, vec2<f32>(419.2, 371.9))) * 59182.7314);

    // Star exists if hash exceeds threshold (~15% of cells have a star)
    if (h1 > 0.85) {
        let star_pos = vec2<f32>(h2, h3); // random position in cell
        let dist = length(cell_uv - star_pos);
        let star_size = 0.03 + h1 * 0.04; // tiny points
        let brightness = (1.0 - smooth_step(0.0, star_size, dist)) * (0.4 + h2 * 0.6);
        // Slight color variation: warm (h3<0.3), blue (h3>0.7), white (middle)
        var star_color = vec3<f32>(1.0);
        if (h3 < 0.3) { star_color = vec3<f32>(1.0, 0.9, 0.7); }
        else if (h3 > 0.7) { star_color = vec3<f32>(0.7, 0.85, 1.0); }
        bg += star_color * brightness;
    }

    // Sun orb: project sun direction to screen space for perfect circle
    if (sun_dir.z < -0.01) { // sun is behind the planet (visible in background)
        let sun_screen = vec2<f32>(sun_dir.x, sun_dir.y) / (-sun_dir.z);
        let sun_dist = length(ndc - sun_screen);

        // Sun disc — colored by star type
        let sun_radius = 0.06;
        let sun_core = 1.0 - smooth_step(0.0, sun_radius, sun_dist);
        bg += sun_color * 3.0 * sun_core;

        // Tight glow halo
        let glow = exp(-sun_dist * sun_dist * 30.0) * 0.2;
        bg += sun_color * glow;
    }

    return bg;
}

// ---- Main fragment shader ----
fn shade_planet(in: VertexOutput) -> vec4<f32> {
    let pan = vec2<f32>(uniforms.pan_x, uniforms.pan_y);
    let ndc = ((in.uv - 0.5) * 2.0 / 0.85 - pan) / uniforms.zoom;
    // Compute derivatives before branch divergence; shared density receives this explicitly.
    let pixel_ray = normalize(vec3<f32>(ndc, 0.5));
    let angular_pixel_footprint = max(length(dpdx(pixel_ray)), length(dpdy(pixel_ray)));
    let r2 = dot(ndc, ndc);

    let sun_dir = normalize(uniforms.light_dir);
    let s_color = star_color(uniforms.star_color_temp);

    // Shell cutoff is supplied from temperature/gravity, separate from density.
    let atm_h = max(uniforms.atmosphere_height, 0.0);
    let atm_radius = 1.0 + atm_h;
    let has_atm = uniforms.atmosphere_density > 0.001 && atm_h > 0.000001;
    let limb_direction = (uniforms.rotation * vec4<f32>(normalize(vec3<f32>(ndc, 0.0001)), 0.0)).xyz;
    let limb_geometry = textureSample(weather_geometry_tex, height_sampler, limb_direction);
    let cloud_top_radius = 1.0 + limb_geometry.a / max(uniforms.planet_radius_km, 1.0);
    let visible_cloud_top = select(1.0, cloud_top_radius, cloud_display_scale() > 0.0);
    let outer_r = max(select(1.005, atm_radius + 0.015, has_atm), visible_cloud_top);

    // ---- Ring system: flat disc intersected by view ray ----
    // Ring sits in a tilted plane through the planet center.
    // View ray: origin (ndc.x, ndc.y, z_far) direction (0, 0, -1) in view space.
    // Ring plane: y * cos(tilt) + z * sin(tilt) = 0 (tilted around X-axis)
    var ring_color_accum = vec3<f32>(0.0);
    var ring_alpha_accum = 0.0;
    let has_rings = uniforms.ring_inner > 0.01 && uniforms.ring_outer > uniforms.ring_inner;
    if (has_rings) {
        let rt = uniforms.ring_tilt;
        let ct = cos(rt);
        let st = sin(rt);
        // Ray: P = (ndc.x, ndc.y, t) for t along view. Plane: y*ct + t*st = 0
        // Solve: t = -ndc.y * ct / st (if st != 0)
        if (abs(st) > 0.001) {
            let t_hit = -ndc.y * ct / st;
            let hit_x = ndc.x;
            let hit_y = ndc.y;
            let ring_r = sqrt(hit_x * hit_x + t_hit * t_hit);

            if (ring_r >= uniforms.ring_inner && ring_r <= uniforms.ring_outer) {
                // Ring hit! Check if it's behind the planet
                let behind_planet = r2 < 1.0 && t_hit < 0.0;
                if (!behind_planet) {
                    // Radial position within ring (0=inner, 1=outer)
                    let ring_frac = (ring_r - uniforms.ring_inner) / (uniforms.ring_outer - uniforms.ring_inner);
                    // Color gradient: inner bright, gaps in middle, outer faint
                    let ring_density = (1.0 - ring_frac) * 0.8 + 0.2;
                    // Procedural ring gaps (Cassini-division-like)
                    let gap1 = 1.0 - smooth_step(0.35, 0.38, ring_frac) * smooth_step(0.42, 0.39, ring_frac) * 0.7;
                    let gap2 = 1.0 - smooth_step(0.65, 0.67, ring_frac) * smooth_step(0.70, 0.68, ring_frac) * 0.5;
                    let ring_band = ring_density * gap1 * gap2;
                    // Lighting: ring is lit by sun on front, shadowed on back
                    let ring_normal = vec3<f32>(0.0, ct, st);
                    let ring_lit = max(dot(ring_normal, sun_dir), 0.0) * 0.7 + 0.3;
                    // Ring color: warm ice/dust tones
                    let base_ring = mix(vec3<f32>(0.75, 0.68, 0.55), vec3<f32>(0.9, 0.85, 0.75), ring_frac);
                    ring_color_accum = base_ring * ring_lit * ring_band * s_color;
                    ring_alpha_accum = ring_band * uniforms.ring_opacity;

                    // Planet shadow on ring: check if ring point is in planet's shadow
                    let shadow_proj = hit_x * sun_dir.x + t_hit * sun_dir.z;
                    if (shadow_proj < 0.0) { // on shadow side
                        let perp_dist = abs(hit_y * ct + t_hit * st - (hit_x * sun_dir.x + hit_y * sun_dir.y + t_hit * sun_dir.z) * sun_dir.y);
                        // Approximate: in shadow if perpendicular distance to sun ray < 1 (planet radius)
                        let shadow_r = sqrt(hit_x * hit_x * (1.0 - sun_dir.x * sun_dir.x) + t_hit * t_hit * (1.0 - sun_dir.z * sun_dir.z));
                        if (shadow_r < 1.05) {
                            ring_color_accum *= 0.15; // deep shadow
                        }
                    }
                }
            }
        }
    }

    // Miss everything — outside both planet and atmosphere → show starfield (+ rings)
    if (r2 > outer_r * outer_r) {
        var bg = starfield(ndc, sun_dir, s_color);
        let bg_tm = bg / (bg + vec3<f32>(1.0)); // tonemap sun HDR
        if (ring_alpha_accum > 0.01) {
            let ring_tm = ring_color_accum / (ring_color_accum + vec3<f32>(1.0));
            return vec4<f32>(mix(bg_tm, ring_tm, ring_alpha_accum), 1.0);
        }
        return vec4<f32>(bg_tm, 1.0);
    }

    let hit_planet = r2 < 1.0;

    // Atmosphere-only ring (between planet edge and outer atmosphere boundary)
    if (!hit_planet) {
        let bg = starfield(ndc, sun_dir, s_color);
        let bg_tm = bg / (bg + vec3<f32>(1.0));
        if (uniforms.view_mode != 0u) {
            return vec4<f32>(bg_tm, 1.0);
        }
        var limb_color = composite_volumes(bg, ndc, -1.0, visible_cloud_top, sun_dir, angular_pixel_footprint);
        limb_color = limb_color / (limb_color + vec3<f32>(1.0));
        return vec4<f32>(limb_color, 1.0);
    }

    // Planet surface hit
    let normal = normalize(vec3<f32>(ndc.x, ndc.y, sqrt(1.0 - r2)));
    let rotated = (uniforms.rotation * vec4<f32>(normal, 0.0)).xyz;

    // Sample height from pre-computed cubemap
    let height = textureSample(height_tex, height_sampler, rotated).r;
    let is_ocean = height < uniforms.ocean_level;

    let color_var = snoise(rotated * 8.0);
    // Regional color variance: low-freq noise for spatially coherent biome sub-variants
    let region_noise = snoise(rotated * 0.8 + vec3<f32>(200.0, 0.0, 0.0)) * 0.5
                     + snoise(rotated * 1.6 + vec3<f32>(0.0, 300.0, 0.0)) * 0.25;
    let region_val = clamp(region_noise + 0.5, 0.0, 1.0);

    // Compute effective latitude for altitude zonation (consistent tilt model)
    let tilt_main = uniforms.axial_tilt_rad;
    let tilted_y_main = rotated.y * cos(tilt_main) + rotated.z * sin(tilt_main);
    let effective_lat = asin(clamp(tilted_y_main, -1.0, 1.0));

    let pure_elevation = uniforms.show_biomes < 0.5 && uniforms.show_water < 0.5;
    var surface_color: vec3<f32>;
    let polar_ice = select(0.0, polar_ice_coverage(rotated, is_ocean), uniforms.show_ice > 0.5);
    var terrain_land_height = 0.0;
    var terrain_slope_value = 0.0;
    var mountain_snow = 0.0;

    // Base layer: when biomes OFF, always show clean grayscale elevation for everything
    if (pure_elevation) {
        // Pure elevation mode — no ocean/land distinction, just height
        surface_color = height_color(height, uniforms.ocean_level);
    } else if (is_ocean && uniforms.show_water > 0.5) {
        // Smooth ocean gradient: shallow → deep with continuous depth color
        let raw_depth = (uniforms.ocean_level - height) / max(uniforms.ocean_level + 1.0, 0.5);
        let depth = clamp(raw_depth, 0.0, 1.0);
        let depth_noise = snoise(rotated * 8.0) * 0.02;

        let near_shore = vec3<f32>(0.07, 0.22, 0.38);
        let mid_ocean  = vec3<f32>(0.04, 0.14, 0.36);
        let deep_ocean = vec3<f32>(0.02, 0.06, 0.22);
        let shelf = smoothstep(0.02, 0.18, depth + depth_noise);
        let abyss = smoothstep(0.18, 0.55, depth);
        var ocean_color = mix(near_shore, mix(mid_ocean, deep_ocean, abyss), shelf);
        ocean_color += vec3<f32>(0.0, 0.015, 0.02) * color_var;

        surface_color = ocean_color;
    } else if (is_ocean) {
        // Water OFF but biomes ON: show height-based grayscale for below-sea-level
        surface_color = height_color(height, uniforms.ocean_level);
    } else {
        // Land: biome coloring or height ramp
        let seasonal_temp = compute_temperature(rotated, height, uniforms.season);
        if (uniforms.show_biomes > 0.5) {
            let mean_temp = compute_temperature(rotated, height, 0.5);
            let mean_moisture = compute_moisture(rotated, height, 0.5);
            surface_color = gradient_color(mean_temp, mean_moisture, seasonal_temp, color_var, region_val);
        } else {
            surface_color = height_color(height, uniforms.ocean_level);
        }

        // Elevation tinting: darken lowlands, lighten highlands
        // Uses raw height for strong contrast on dry worlds (Mars, Venus)
        terrain_land_height = clamp(
            (height - uniforms.ocean_level) / max(1.0 - uniforms.ocean_level, 0.01),
            0.0,
            1.0,
        );
        let h_for_tint = clamp((height + 0.5) / 1.0, 0.0, 1.0);
        let elev_tint = mix(0.65, 1.35, h_for_tint);
        surface_color *= elev_tint;
        terrain_slope_value = terrain_slope(rotated);

        // Altitude zonation — derived from 6.5°C/km lapse rate
        // 1km altitude ~ 8° poleward for vegetation/snow lines
        // Compute sea-level temperature to derive where each biome zone starts
        let sea_level_temp = compute_temperature(rotated, uniforms.ocean_level, 0.5);
        // Convert threshold temperatures to altitude via lapse rate: alt_km = (T_sealevel - T_threshold) / 6.5
        // Then to land_height units: land_height = alt_km / 5.0
        let snow_elev_km = max(sea_level_temp / 6.5, 0.0);         // 0°C line
        let rock_elev_km = max((sea_level_temp - 5.0) / 6.5, 0.0); // 5°C line
        let alpine_elev_km = max((sea_level_temp - 10.0) / 6.5, 0.0); // 10°C treeline
        let highland_elev_km = max((sea_level_temp - 18.0) / 6.5, 0.0); // 18°C highland start
        let snow_line = clamp(snow_elev_km / 5.0, 0.05, 0.95);
        let rock_line = clamp(rock_elev_km / 5.0, 0.04, snow_line - 0.03);
        let alpine_line = clamp(alpine_elev_km / 5.0, 0.03, rock_line - 0.03);
        let highland_line = clamp(highland_elev_km / 5.0, 0.02, alpine_line - 0.02);

        let seasonal_temp_local = compute_temperature(rotated, height, uniforms.season);
        let mean_moisture_local = compute_moisture(rotated, height, 0.5);
        // Bounded material bands: highland vegetation, alpine meadow/scree,
        // then exposed rock. Snow is a separate final deposit over these bands.
        let highland_material = smooth_step(highland_line, highland_line + 0.08, terrain_land_height)
            * (1.0 - smooth_step(alpine_line - 0.04, alpine_line + 0.02, terrain_land_height));
        let alpine_material = smooth_step(alpine_line, alpine_line + 0.06, terrain_land_height)
            * (1.0 - smooth_step(rock_line - 0.03, rock_line + 0.02, terrain_land_height));
        let rock_material = smooth_step(rock_line, rock_line + 0.05, terrain_land_height)
            * (1.0 - smooth_step(snow_line - 0.03, snow_line + 0.02, terrain_land_height));

        if (highland_material > 0.0) {
            let highland_arid = surface_color * vec3<f32>(0.90, 0.82, 0.72);
            let highland_wet = surface_color * vec3<f32>(0.78, 0.75, 0.65);
            let highland_color = mix(highland_wet, highland_arid, smooth_step(30.0, 15.0, mean_moisture_local));
            surface_color = mix(surface_color, highland_color, highland_material * 0.6);
        }

        if (alpine_material > 0.0) {
            let alpine_tropical = vec3<f32>(0.38, 0.48, 0.28);
            let alpine_temperate = vec3<f32>(0.42, 0.45, 0.32);
            let alpine_arid = vec3<f32>(0.52, 0.46, 0.36);
            var alpine_color = mix(alpine_temperate, alpine_tropical, smooth_step(15.0, 25.0, seasonal_temp_local));
            alpine_color = mix(alpine_color, alpine_arid, smooth_step(30.0, 12.0, mean_moisture_local));
            surface_color = mix(surface_color, alpine_color, alpine_material);
        }

        if (rock_material > 0.0) {
            let rock_color = vec3<f32>(0.48, 0.46, 0.42) + vec3<f32>(0.04) * color_var;
            surface_color = mix(surface_color, rock_color, rock_material);
        }

        mountain_snow = select(
            0.0,
            mountain_snow_coverage(
                terrain_land_height,
                seasonal_temp_local,
                mean_moisture_local,
                terrain_slope_value,
            ),
            uniforms.show_ice > 0.5,
        );
        if (mountain_snow > 0.0) {
            let fresh_snow = vec3<f32>(0.90, 0.93, 0.96) + vec3<f32>(0.012) * color_var;
            let glacier_blue = vec3<f32>(0.78, 0.86, 0.93);
            let snow_color = mix(fresh_snow, glacier_blue, terrain_land_height * mountain_snow);
            surface_color = mix(surface_color, snow_color, mountain_snow);
        }

        // Beach transition — very subtle, only at close zoom
        if (terrain_land_height < 0.015) {
            let beach_blend = smooth_step(0.015, 0.0, terrain_land_height);
            surface_color = mix(surface_color, vec3<f32>(0.55, 0.52, 0.42), beach_blend * 0.3);
        }
    }

    // One blue-white albedo for the shared ocean/land polar cap boundary.
    if (!pure_elevation && polar_ice > 0.0) {
        let polar_ice_albedo = vec3<f32>(0.72, 0.82, 0.89) + vec3<f32>(color_var * 0.015);
        surface_color = mix(surface_color, polar_ice_albedo, polar_ice);
    }

    // Polar coastline softening (gated by show_ice)
    if (!pure_elevation && uniforms.show_ice > 0.5 && polar_ice > 0.3 && is_ocean == false) {
        let coast_dist = abs(height - uniforms.ocean_level);
        let coast_soften = 1.0 - smooth_step(0.0, 0.06, coast_dist);
        let uniform_ice = vec3<f32>(0.72, 0.82, 0.89);
        surface_color = mix(surface_color, uniform_ice, coast_soften * polar_ice * 0.8);
    }

    // Day-side urban grey patches (gated by show_cities)
    if (uniforms.show_cities > 0.5 && uniforms.night_lights > 0.0 && !is_ocean) {
        let urban = compute_urban_density(rotated, height);
        if (urban > 0.01) {
            let concrete = vec3<f32>(0.30, 0.30, 0.31); // cool dark grey
            // Darken toward concrete rather than full replace — preserves some surface variation
            let darkened = mix(surface_color * 0.5, concrete, 0.6);
            surface_color = mix(surface_color, darkened, urban * uniforms.night_lights);
        }
    }

    // Debug views
    if (uniforms.view_mode > 0u) {
        let temp = compute_temperature(rotated, height, uniforms.season);
        let moisture = compute_moisture(rotated, height, uniforms.season);
        var debug_color: vec3<f32>;

        switch (uniforms.view_mode) {
            case 1u: { let h = (height + 1.0) * 0.5; debug_color = vec3<f32>(h, h, h); }
            case 2u: {
                let t = clamp(temp / 50.0, -1.0, 1.0);
                if (t < 0.0) { debug_color = mix(vec3<f32>(0.0, 0.0, 1.0), vec3<f32>(1.0), t + 1.0); }
                else { debug_color = mix(vec3<f32>(1.0), vec3<f32>(1.0, 0.0, 0.0), t); }
            }
            case 3u: {
                let m = clamp(moisture / 300.0, 0.0, 1.0);
                if (m < 0.5) { debug_color = mix(vec3<f32>(0.6, 0.4, 0.1), vec3<f32>(0.1, 0.6, 0.1), m * 2.0); }
                else { debug_color = mix(vec3<f32>(0.1, 0.6, 0.1), vec3<f32>(0.1, 0.2, 0.8), (m - 0.5) * 2.0); }
            }
            case 4u: {
                let mean_t = compute_temperature(rotated, height, 0.5);
                let mean_m = compute_moisture(rotated, height, 0.5);
                debug_color = gradient_color(mean_t, mean_m, mean_t, 0.0, region_val) * 1.3;
            }
            case 5u: {
                if (is_ocean) {
                    if (compute_temperature(rotated, height, uniforms.season) < -2.0) { debug_color = vec3<f32>(1.0); }
                    else { debug_color = vec3<f32>(0.0, 0.2, 0.8); }
                } else {
                    if (temp < -15.0) { debug_color = vec3<f32>(0.9, 0.95, 1.0); }
                    else {
                        let lh = clamp((height - uniforms.ocean_level) / max(1.0 - uniforms.ocean_level, 0.01), 0.0, 1.0);
                        debug_color = mix(vec3<f32>(0.2, 0.6, 0.1), vec3<f32>(0.5, 0.3, 0.1), lh);
                    }
                }
            }
            case 6u: {
                // Plate structure: height with contour lines at boundaries
                // Sample neighboring heights to detect edges (plate boundaries)
                let step = 0.01;
                let h_r = textureSample(height_tex, height_sampler, rotated + vec3<f32>(step, 0.0, 0.0)).r;
                let h_u = textureSample(height_tex, height_sampler, rotated + vec3<f32>(0.0, step, 0.0)).r;
                let gradient = abs(h_r - height) + abs(h_u - height);

                // Base: color by elevation (blue ocean, tan/green land)
                let h = (height + 0.5) / 1.0;
                if (height < uniforms.ocean_level) {
                    debug_color = mix(vec3<f32>(0.05, 0.1, 0.3), vec3<f32>(0.1, 0.2, 0.5), clamp(h + 0.5, 0.0, 1.0));
                } else {
                    debug_color = mix(vec3<f32>(0.3, 0.5, 0.2), vec3<f32>(0.7, 0.6, 0.4), clamp((height - uniforms.ocean_level) * 3.0, 0.0, 1.0));
                }

                // Overlay bright lines at plate boundaries (sharp height gradients)
                if (gradient > 0.02) {
                    let edge_strength = clamp((gradient - 0.02) * 20.0, 0.0, 1.0);
                    debug_color = mix(debug_color, vec3<f32>(1.0, 0.3, 0.1), edge_strength);
                }

                // Contour lines at regular height intervals
                let contour = fract(height * 8.0);
                if (contour < 0.05 || contour > 0.95) {
                    debug_color *= 0.7;
                }
            }
            case 7u: {
                // Roughness visualization (uses actual compute_roughness)
                let rt = compute_temperature(rotated, height, uniforms.season);
                let rm = compute_moisture(rotated, height, uniforms.season);
                let r = compute_roughness(
                    rt,
                    rm,
                    is_ocean,
                    is_ocean && rt < -2.0,
                    terrain_land_height,
                    terrain_slope_value,
                    mountain_snow,
                );
                debug_color = vec3<f32>(r, r, r);
            }
            case 8u: {
                // AO visualization
                let ao_val = select(1.0, compute_ao(rotated), !is_ocean);
                debug_color = vec3<f32>(ao_val, ao_val, ao_val);
            }
            case 9u: {
                // Integrated density, so debug visualizes the same volume as preview.
                let debug_geometry = textureSample(weather_geometry_tex, height_sampler, rotated);
                let debug_top = 1.0 + debug_geometry.a / max(uniforms.planet_radius_km, 1.0);
                let z_debug_top = sqrt(max(debug_top * debug_top - r2, 0.0));
                let z_surface = sqrt(max(1.0 - r2, 0.0));
                let clouds = ray_march_clouds(ndc, z_debug_top, z_surface, sun_dir, angular_pixel_footprint);
                let cd = 1.0 - clouds.transmittance.x;
                debug_color = vec3<f32>(cd, cd, cd);
            }
            case 10u: {
                // Emission export: city lights only, everything else black
                let urban = compute_urban_density(rotated, height);
                debug_color = vec3<f32>(urban, urban * 0.8, urban * 0.3);
            }
            case 11u: {
                // Boundary type proxy: use height gradient magnitude and sign to visualize
                // tectonic boundary character. Red=convergent (steep rise), blue=divergent
                // (rift drop), green=transform (lateral offset, low gradient).
                // Samples neighbours to compute gradient vector, then classifies.
                let bstep = 0.008;
                let h_r = textureSample(height_tex, height_sampler, rotated + vec3<f32>(bstep, 0.0, 0.0)).r;
                let h_l = textureSample(height_tex, height_sampler, rotated - vec3<f32>(bstep, 0.0, 0.0)).r;
                let h_u = textureSample(height_tex, height_sampler, rotated + vec3<f32>(0.0, bstep, 0.0)).r;
                let h_d = textureSample(height_tex, height_sampler, rotated - vec3<f32>(0.0, bstep, 0.0)).r;
                let grad_x = (h_r - h_l) * 0.5;
                let grad_y = (h_u - h_d) * 0.5;
                let grad_mag = sqrt(grad_x * grad_x + grad_y * grad_y);
                // Classify: strong positive rise → convergent (red), strong negative → divergent (blue),
                // high grad_mag but mixed sign → transform (green).
                let rise = (h_r - h_l + h_u - h_d) * 0.25; // net rise
                let convergent_str = clamp(rise * 20.0, 0.0, 1.0);
                let divergent_str  = clamp(-rise * 20.0, 0.0, 1.0);
                let transform_str  = clamp(grad_mag * 15.0 - convergent_str - divergent_str, 0.0, 1.0);
                debug_color = vec3<f32>(convergent_str, transform_str, divergent_str);
            }
            case 12u: {
                // Unified climate-driven polar coverage; altitude snow is separate.
                debug_color = vec3<f32>(polar_ice, polar_ice, polar_ice);
            }
            case 13u: {
                // Terrain normals — visualize shading_normal as RGB (normal map style)
                var n: vec3<f32>;
                if (is_ocean) {
                    n = normal;
                } else {
                    n = compute_terrain_normal(rotated, normal, angular_pixel_footprint / max(normal.z, 0.15));
                }
                // Remap from [-1,1] to [0,1] for display
                debug_color = n * 0.5 + vec3<f32>(0.5);
            }
            case 14u: {
                // Wind direction: unified view — cubemap wind when available, analytical fallback.
                let tangent_w = sample_wind_tangent(rotated);

                // Local east/north with smooth pole blend (no ring artifact)
                let ct_w = cos(uniforms.axial_tilt_rad);
                let st_w = sin(uniforms.axial_tilt_rad);
                let tilted_pole = vec3<f32>(0.0, ct_w, st_w);
                let pc = abs(dot(rotated, tilted_pole));
                let ub = smooth_step(0.80, 0.99, pc);
                let up_ref_w = normalize(mix(tilted_pole, vec3<f32>(1.0, 0.0, 0.0), ub));
                let local_east = normalize(cross(up_ref_w, rotated));
                let local_north = normalize(cross(rotated, local_east));

                let wind_east = dot(tangent_w, local_east);
                let wind_north = dot(tangent_w, local_north);
                let speed = length(vec2<f32>(wind_east, wind_north));

                // Wider color ramp: full 0→1 range instead of narrow smooth_step band.
                // Prevents the visualization from making cell boundaries look sharper than they are.
                let east_frac = (wind_east / max(speed, 0.01) + 1.0) * 0.5;
                let merid_frac = abs(wind_north) / max(speed, 0.01);
                debug_color = vec3<f32>(
                    east_frac,                          // R: east (0=west, 1=east) — linear, no threshold
                    merid_frac * 0.4 + speed * 0.3,     // G: meridional + speed
                    1.0 - east_frac                     // B: west — linear complement
                ) * (0.5 + speed * 0.5);
            }
            case 15u: {
                // Ocean currents: warm (red) vs cold (blue) current zones
                if (is_ocean) {
                    let oc_temp = compute_temperature(rotated, height, uniforms.season);
                    let oc_lat = asin(clamp(rotated.y, -1.0, 1.0));
                    let oc_lat_norm = abs(oc_lat) / 1.5708;
                    // Expected temp at this latitude without currents
                    let expected_temp = compute_sea_level_temperature(rotated, uniforms.season);
                    let anomaly = oc_temp - expected_temp;
                    // Warm anomaly → red, cold → blue, neutral → grey
                    let warm = clamp(anomaly / 8.0, 0.0, 1.0);
                    let cold = clamp(-anomaly / 6.0, 0.0, 1.0);
                    debug_color = mix(vec3<f32>(0.3, 0.3, 0.4), vec3<f32>(0.9, 0.2, 0.1), warm);
                    debug_color = mix(debug_color, vec3<f32>(0.1, 0.3, 0.9), cold);
                } else {
                    let lh = clamp((height - uniforms.ocean_level) * 3.0, 0.0, 1.0);
                    debug_color = vec3<f32>(lh * 0.3 + 0.1);
                }
            }
            case 16u: {
                let cont = textureSample(cloud_tex, height_sampler, rotated).a;
                debug_color = mix(vec3<f32>(0.1, 0.2, 0.5), vec3<f32>(0.8, 0.5, 0.2), cont);
            }
            case 17u: {
                // Pressure: sampled from cloud_tex (swapped to pressure cubemap for this view)
                // Pressure stored as raw hPa; map deviation from 1013 to color
                let p = textureSample(cloud_tex, height_sampler, rotated).r;
                let dev = (p - 1013.0) / 20.0; // ±20 hPa range
                let low = clamp(-dev, 0.0, 1.0);
                let high = clamp(dev, 0.0, 1.0);
                debug_color = mix(vec3<f32>(0.3, 0.3, 0.3), vec3<f32>(0.2, 0.4, 0.9), low);
                debug_color = mix(debug_color, vec3<f32>(0.9, 0.3, 0.1), high);
            }
            case 18u: {
                // Legacy: merged into view 14 (sample_wind_tangent shows cubemap when available)
                debug_color = vec3<f32>(0.3, 0.3, 0.3);
            }
            default: { debug_color = surface_color; }
        }
        return vec4<f32>(debug_color, 1.0);
    }

    // ---- PBR Lighting ----
    let light = normalize(uniforms.light_dir);
    let view_dir = vec3<f32>(0.0, 0.0, 1.0); // Camera looks along -Z, view = +Z
    let half_sum = light + view_dir;
    let half_vec = half_sum / max(length(half_sum), 1.0e-6);

    // Compute terrain-perturbed normal (flat for ocean — water surface is smooth)
    var shading_normal: vec3<f32>;
    if (is_ocean) {
        shading_normal = normal; // Geometric sphere normal — flat water
    } else {
        shading_normal = compute_terrain_normal(rotated, normal, angular_pixel_footprint / max(normal.z, 0.15));
    }

    // PBR inputs
    let n_dot_l = max(dot(shading_normal, light), 0.0) * smooth_step(0.0, 0.002, dot(normal, light));
    let n_dot_v = max(dot(shading_normal, view_dir), 0.001);
    let n_dot_h = max(dot(shading_normal, half_vec), 0.0);
    let h_dot_v = max(dot(half_vec, view_dir), 0.0);

    // Roughness and Fresnel base reflectance
    let ocean_ice = is_ocean && !pure_elevation && polar_ice > 0.01;
    let temp_for_rough = compute_temperature(rotated, height, uniforms.season);
    let moist_for_rough = compute_moisture(rotated, height, uniforms.season);
    let roughness = select(
        0.55,
        compute_roughness(
            temp_for_rough,
            moist_for_rough,
            is_ocean,
            ocean_ice || mountain_snow > 0.01,
            terrain_land_height,
            terrain_slope_value,
            mountain_snow,
        ),
        !pure_elevation,
    );
    // Air/water normal-incidence reflectance for n=1.333 is 0.0204.
    let f0 = select(0.04, 0.0204, is_ocean);

    // GGX specular
    let d = ggx_distribution(n_dot_h, roughness);
    let f = fresnel_schlick(h_dot_v, f0);
    let visibility = ggx_smith_visibility(n_dot_l, n_dot_v, roughness);
    let specular = d * f * visibility / max(4.0 * n_dot_v * n_dot_l, 0.001);

    // Diffuse (energy-conserving: reduce diffuse where specular is strong)
    let diffuse = surface_color * (1.0 - f) / 3.14159;

    // Ambient (subtle, directional — slightly brighter on the lit hemisphere)
    let ao = select(1.0, compute_ao(rotated), !is_ocean && uniforms.show_ao > 0.5);
    let ambient = surface_color * (0.002 + 0.04 * max(dot(normal, light), 0.0)) * ao;

    // Combine — tint direct light by star color
    var lit_color = ambient + (diffuse + specular) * n_dot_l * s_color * SUN_IRRADIANCE
        * atmosphere_sun_transmittance(normal * 1.000001, light);

    // Cloud shadow on surface (independent from visible cloud self-shading)
    if (uniforms.show_cloud_shadows > 0.5) {
        let sun_world = normalize((uniforms.rotation * vec4<f32>(sun_dir, 0.0)).xyz);
        let surface_shadow = cloud_surface_shadow(rotated, sun_world, max(uniforms.planet_radius_km, 1.0), angular_pixel_footprint);
        lit_color *= mix(1.0, surface_shadow, 0.85);
    }

    // ---- Lava glow at tectonic boundaries ----
    if (uniforms.lava_glow > 0.0 && !is_ocean) {
        // Detect plate boundaries via height gradient (same logic as debug view 11)
        let lstep = 0.006;
        let lh_r = textureSample(height_tex, height_sampler, rotated + vec3<f32>(lstep, 0.0, 0.0)).r;
        let lh_l = textureSample(height_tex, height_sampler, rotated - vec3<f32>(lstep, 0.0, 0.0)).r;
        let lh_u = textureSample(height_tex, height_sampler, rotated + vec3<f32>(0.0, lstep, 0.0)).r;
        let lh_d = textureSample(height_tex, height_sampler, rotated - vec3<f32>(0.0, lstep, 0.0)).r;
        let lgrad = sqrt(pow(lh_r - lh_l, 2.0) + pow(lh_u - lh_d, 2.0));
        // Strong gradient = plate boundary → lava emission
        let boundary = smooth_step(0.015, 0.06, lgrad);
        if (boundary > 0.01) {
            // Flickering lava noise
            let lava_noise = snoise(rotated * 80.0) * 0.3 + snoise(rotated * 160.0) * 0.2 + 0.5;
            let lava_strength = boundary * uniforms.lava_glow * max(lava_noise, 0.2);
            let lava_color = mix(vec3<f32>(1.0, 0.3, 0.0), vec3<f32>(1.0, 0.8, 0.1), lava_noise);
            lit_color += lava_color * lava_strength * 3.0; // HDR emission
        }
    }

    // ---- Night-side city lights (gated by show_cities) ----
    var city_glow_through = vec3<f32>(0.0);
    var city_glow_amount = 0.0;
    if (uniforms.show_cities > 0.5 && uniforms.night_lights > 0.0 && !is_ocean) {
        let night_factor = smooth_step(0.05, -0.1, dot(normal, light));
        if (night_factor > 0.01) {
            let urban = compute_urban_density(rotated, height);
            if (urban > 0.01) {
                let sparkle = snoise(rotated * 300.0) * 0.3 + snoise(rotated * 600.0) * 0.2 + 0.5;
                let light_intensity = urban * night_factor * uniforms.night_lights * max(sparkle, 0.3);
                // City color from hue slider
                let amber = vec3<f32>(1.2, 0.85, 0.3);
                let white_led = vec3<f32>(1.1, 1.05, 1.0);
                let cool_blue = vec3<f32>(0.5, 0.7, 1.2);
                let hue = uniforms.city_light_hue;
                var city_col: vec3<f32>;
                if (hue < 0.5) {
                    city_col = mix(amber, white_led, hue * 2.0);
                } else {
                    city_col = mix(white_led, cool_blue, (hue - 0.5) * 2.0);
                }
                // Dim lights under cloud cover
                let cloud_above = weather_column_density(rotated, angular_pixel_footprint);
                let cloud_block = exp(-cloud_above * 4.0); // thick clouds block most light
                lit_color += city_col * light_intensity * 1.2 * cloud_block;
                // Save glow for scatter through clouds
                city_glow_through = city_col * light_intensity * 0.3;
                city_glow_amount = cloud_above;
            }
        }
    }

    // City light scatter through clouds (needs both cities and clouds)
    if (uniforms.show_cities > 0.5 && uniforms.show_clouds > 0.5 && city_glow_amount > 0.05) {
        let scatter_strength = (1.0 - exp(-city_glow_amount * 2.0)) * 0.4; // thicker clouds scatter more
        lit_color += city_glow_through * scatter_strength;
    }

    let geometry = textureSample(weather_geometry_tex, height_sampler, rotated);
    let top_radius = 1.0 + geometry.a / max(uniforms.planet_radius_km, 1.0);
    lit_color = composite_volumes(lit_color, ndc, sqrt(max(1.0 - r2, 0.0)), top_radius, sun_dir, angular_pixel_footprint);

    // Tonemap (Reinhard)
    lit_color = lit_color / (lit_color + vec3<f32>(1.0));

    // Edge AA at planet boundary (when no atmosphere provides the transition)
    if (!has_atm) {
        let edge_bg = starfield(ndc, sun_dir, s_color);
        let edge_bg_tm = edge_bg / (edge_bg + vec3<f32>(1.0));
        let edge_aa = 1.0 - smooth_step(0.99, 1.0, sqrt(r2));
        lit_color = mix(edge_bg_tm, lit_color, edge_aa);
    }

    return vec4<f32>(lit_color, 1.0);
}


// Offline sRGB attachments encode automatically. egui samples a gamma-encoded
// UNORM user texture, so its entry point must encode explicitly exactly once.
@fragment
fn fs_main(in: VertexOutput) -> @location(0) vec4<f32> {
    return shade_planet(in);
}

@fragment
fn fs_main_display(in: VertexOutput) -> @location(0) vec4<f32> {
    let color = shade_planet(in);
    let linear = max(color.rgb, vec3<f32>(0.0));
    let encoded = select(1.055 * pow(linear, vec3<f32>(1.0 / 2.4)) - 0.055,
        12.92 * linear, linear <= vec3<f32>(0.0031308));
    return vec4<f32>(encoded, color.a);
}
