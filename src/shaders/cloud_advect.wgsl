// Cloud advection compute shader.
// Semi-Lagrangian advection on cubemap: trace back along wind, sample source density.
// Includes noise.wgsl and cube_sphere.wgsl (concatenated at load time).

struct CloudParams {
    face: u32,
    resolution: u32,
    seed: u32,
    mode: u32,         // 0 = init (noise), 1 = advect step
    dt: f32,           // advection time step
    decay: f32,        // per-step dissipation (0.99 = slow, 0.95 = fast)
    ocean_level: f32,
    ocean_fraction: f32,
    axial_tilt_rad: f32,
    season: f32,
    condensation_rate: f32,
    _pad0: u32,
}

@group(0) @binding(0) var<uniform> params: CloudParams;
@group(0) @binding(1) var<storage, read> src_density: array<f32>;    // source (read)
@group(0) @binding(2) var<storage, read_write> dst_density: array<f32>; // destination (write)
@group(0) @binding(3) var<storage, read> height_data: array<f32>;    // terrain heightmap (same face)
@group(0) @binding(4) var<storage, read> wind_field: array<f32>;     // 3D wind vectors (3 * 6 * res²)
// For cross-face sampling, we pack all 6 faces into src_density:
// face i starts at offset i * resolution * resolution

fn pcg_hash(input: u32) -> u32 {
    var h = input * 747796405u + 2891336453u;
    h = ((h >> ((h >> 28u) + 4u)) ^ h) * 277803737u;
    h = (h >> 22u) ^ h;
    return h;
}

fn seed_offset(s: u32) -> vec3<f32> {
    let h1 = pcg_hash(s);
    let h2 = pcg_hash(s + 1u);
    let h3 = pcg_hash(s + 2u);
    return vec3<f32>(
        f32(h1 & 0xFFFFu) / 655.35,
        f32(h2 & 0xFFFFu) / 655.35,
        f32(h3 & 0xFFFFu) / 655.35
    );
}

fn smooth_step(edge0: f32, edge1: f32, x: f32) -> f32 {
    let t = clamp((x - edge0) / (edge1 - edge0), 0.0, 1.0);
    return t * t * (3.0 - 2.0 * t);
}

// Inverse cube mapping: 3D direction → face index + UV
fn sphere_to_face_uv(dir: vec3<f32>) -> vec3<f32> {
    let abs_dir = abs(dir);
    var face_idx: f32;
    var u: f32;
    var v: f32;

    if (abs_dir.x >= abs_dir.y && abs_dir.x >= abs_dir.z) {
        if (dir.x > 0.0) {
            face_idx = 0.0; u = -dir.z / abs_dir.x; v = -dir.y / abs_dir.x;
        } else {
            face_idx = 1.0; u = dir.z / abs_dir.x; v = -dir.y / abs_dir.x;
        }
    } else if (abs_dir.y >= abs_dir.x && abs_dir.y >= abs_dir.z) {
        if (dir.y > 0.0) {
            face_idx = 2.0; u = dir.x / abs_dir.y; v = dir.z / abs_dir.y;
        } else {
            face_idx = 3.0; u = dir.x / abs_dir.y; v = -dir.z / abs_dir.y;
        }
    } else {
        if (dir.z > 0.0) {
            face_idx = 4.0; u = dir.x / abs_dir.z; v = -dir.y / abs_dir.z;
        } else {
            face_idx = 5.0; u = -dir.x / abs_dir.z; v = -dir.y / abs_dir.z;
        }
    }
    // Map [-1,1] → [0,1]
    return vec3<f32>(face_idx, u * 0.5 + 0.5, v * 0.5 + 0.5);
}

// Sample density from the full 6-face buffer using a 3D direction
fn sample_density(dir: vec3<f32>) -> f32 {
    let fuv = sphere_to_face_uv(dir);
    let face = u32(fuv.x);
    let res = params.resolution;
    let px = clamp(u32(fuv.y * f32(res - 1u)), 0u, res - 1u);
    let py = clamp(u32(fuv.z * f32(res - 1u)), 0u, res - 1u);
    let idx = face * res * res + py * res + px;
    return src_density[idx];
}

// Sample pressure-derived wind from precomputed wind field buffer.
// Wind field contains 3D tangent vectors: 3 floats per texel, all 6 faces packed.
// Falls back to basic latitude-based wind if wind field is empty (size < 6).
fn wind_at(pos: vec3<f32>) -> vec3<f32> {
    // Sample wind from precomputed buffer via cross-face lookup
    let fuv = sphere_to_face_uv(pos);
    let face = u32(fuv.x);
    let res = params.resolution;
    let px = clamp(u32(fuv.y * f32(res - 1u)), 0u, res - 1u);
    let py = clamp(u32(fuv.z * f32(res - 1u)), 0u, res - 1u);
    let base = (face * res * res + py * res + px) * 3u;

    let wx = wind_field[base];
    let wy = wind_field[base + 1u];
    let wz = wind_field[base + 2u];
    let wind = vec3<f32>(wx, wy, wz);

    let speed = length(wind);
    if (speed < 0.001) {
        // Fallback: tiny tangent nudge to prevent zero advection
        var up = vec3<f32>(0.0, 1.0, 0.0);
        if (abs(pos.y) > 0.95) { up = vec3<f32>(1.0, 0.0, 0.0); }
        return normalize(cross(up, pos)) * 0.01;
    }

    // Normalize to reasonable advection speed (scale from pressure gradient units)
    return normalize(wind) * min(speed * 0.5, 1.0);
}

// Moisture-based condensation rate
fn condensation_at(pos: vec3<f32>, height: f32) -> f32 {
    let tilted_y = pos.y * cos(params.axial_tilt_rad) + pos.z * sin(params.axial_tilt_rad);
    let lat = asin(clamp(tilted_y, -1.0, 1.0));
    let lat_deg = abs(lat) * 180.0 / 3.14159;

    // ITCZ convergence: strong condensation at equator
    let itcz = exp(-lat_deg * lat_deg / 150.0) * 0.5;

    // Moisture from ocean proximity (simple: below ocean_level = ocean)
    let is_ocean = height < params.ocean_level;
    let ocean_boost = select(0.0, 0.25, is_ocean);

    // Subtropical suppression
    let subtropical = smooth_step(15.0, 25.0, lat_deg) * smooth_step(40.0, 30.0, lat_deg) * 0.2;

    // Mid-latitude frontal
    let midlat = smooth_step(35.0, 50.0, lat_deg) * smooth_step(65.0, 55.0, lat_deg) * 0.15;

    // Baseline condensation everywhere (clouds form globally, just denser at ITCZ)
    let baseline = 0.08;

    return (baseline + itcz + ocean_boost + midlat - subtropical) * params.condensation_rate;
}

@compute @workgroup_size(16, 16)
fn main(@builtin(global_invocation_id) id: vec3<u32>) {
    let res = params.resolution;
    if (id.x >= res || id.y >= res) { return; }

    let uv = vec2<f32>(
        f32(id.x) / f32(res - 1u),
        f32(id.y) / f32(res - 1u)
    );
    let pos = cube_to_sphere(params.face, uv);
    let face_offset = params.face * res * res;
    let idx = face_offset + id.y * res + id.x;
    let local_idx = id.y * res + id.x;

    if (params.mode == 0u) {
        // === INIT MODE: generate noise-based seed density ===
        let so = seed_offset(params.seed + 8000u);
        let p = pos * 7.0 + so;
        let warp = vec3<f32>(
            snoise(p * 0.5 + vec3<f32>(31.7, 0.0, 0.0)),
            snoise(p * 0.5 + vec3<f32>(0.0, 47.3, 0.0)),
            snoise(p * 0.5 + vec3<f32>(0.0, 0.0, 73.1))
        ) * 0.4;
        var noise = snoise(p + warp) * 0.5 + snoise((p + warp) * 2.1) * 0.25
                  + snoise((p + warp) * 4.2) * 0.13;
        noise = noise * 0.5 + 0.4; // bias toward some density

        // Weather-scale regions
        let weather = snoise(pos * 1.5 + so * 0.3 + vec3<f32>(77.0, 0.0, 0.0));
        noise *= 0.7 + 0.3 * (weather * 0.5 + 0.5);

        dst_density[idx] = clamp(noise, 0.0, 1.0);
    } else {
        // === ADVECT MODE: semi-Lagrangian advection ===
        let wind = wind_at(pos);

        // Turbulent diffusion: small random displacement per-texel per-step
        // prevents density from organizing into perfect latitude bands.
        let turb_hash = pcg_hash(idx + params.seed * 7u + params.mode * 31u);
        let turb_angle = f32(turb_hash & 0xFFFFu) / 10430.0; // [0, 2π)
        let turb_mag = f32((turb_hash >> 16u) & 0xFFu) / 255.0 * 0.008;
        // Build tangent-plane displacement
        var up_t = vec3<f32>(0.0, 1.0, 0.0);
        if (abs(pos.y) > 0.95) { up_t = vec3<f32>(1.0, 0.0, 0.0); }
        let te = normalize(cross(up_t, pos));
        let tn = normalize(cross(pos, te));
        let turb = (te * cos(turb_angle) + tn * sin(turb_angle)) * turb_mag;

        // Trace back along wind + turbulence to find source position
        let source_pos = normalize(pos - wind * params.dt + turb);
        var density = sample_density(source_pos);

        // Dissipation
        density *= params.decay;

        // Condensation source (where moisture is high)
        let h = height_data[local_idx];
        let cond = condensation_at(pos, h);
        density += cond * params.dt;

        // Rain shadow sink: check if terrain is upwind and tall
        let upwind = normalize(pos + wind * 0.05);
        let upwind_fuv = sphere_to_face_uv(upwind);
        let uf = u32(upwind_fuv.x);
        let upx = clamp(u32(upwind_fuv.y * f32(res - 1u)), 0u, res - 1u);
        let upy = clamp(u32(upwind_fuv.z * f32(res - 1u)), 0u, res - 1u);
        // Only sample if on same face (cross-face would need height for all faces)
        if (uf == params.face) {
            let upwind_h = height_data[upy * res + upx];
            if (upwind_h > h + 0.08) {
                // Leeward dissipation
                density *= 0.92;
            }
        }

        dst_density[idx] = clamp(density, 0.0, 1.0);
    }
}
