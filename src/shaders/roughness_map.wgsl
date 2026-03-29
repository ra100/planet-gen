// Roughness map generation from biome data.
// Uses temperature + moisture → biome → roughness value.
// Includes cube_sphere.wgsl and noise.wgsl at load time.

struct RoughnessParams {
    face: u32,
    resolution: u32,
    seed: u32,
    base_temp_c: f32,
    ocean_level: f32,
    ocean_fraction: f32,
    _pad0: u32,
    _pad1: u32,
}

@group(0) @binding(0) var<storage, read> heightmap: array<f32>;
@group(0) @binding(1) var<storage, read_write> roughness: array<f32>;
@group(0) @binding(2) var<uniform> params: RoughnessParams;

fn biome_roughness_value(temp_c: f32, moisture_cm: f32, is_ocean: bool) -> f32 {
    if (is_ocean) { return 0.05; } // Water is very smooth

    // Simplified Whittaker → roughness
    if (temp_c < 0.0) { return 0.15; } // Ice
    if (temp_c < 10.0) { return 0.55; } // Tundra/Taiga
    if (moisture_cm < 25.0) { return 0.85; } // Desert
    if (moisture_cm < 100.0) { return 0.60; } // Grassland/Savanna
    return 0.50; // Forest
}

@compute @workgroup_size(16, 16)
fn main(@builtin(global_invocation_id) id: vec3<u32>) {
    let res = params.resolution;
    if (id.x >= res || id.y >= res) {
        return;
    }

    let idx = id.y * res + id.x;
    let height = heightmap[idx];
    let is_ocean = height < params.ocean_level;

    let uv = vec2<f32>(
        f32(id.x) / f32(res - 1u),
        f32(id.y) / f32(res - 1u)
    );
    let sphere_pos = cube_to_sphere(params.face, uv);

    // Compute temperature/moisture (simplified version of preview shader)
    let latitude = asin(clamp(sphere_pos.y, -1.0, 1.0));
    let lat_deg = abs(latitude) * 180.0 / 3.14159;
    let temp_scale = params.base_temp_c / 15.0;
    let temp = 30.0 * temp_scale - lat_deg * (60.0 * temp_scale / 90.0);

    let seed_offset = vec3<f32>(f32(params.seed % 1000u) * 0.1, f32((params.seed / 1000u) % 1000u) * 0.1, 0.0);
    let moisture_noise = snoise(sphere_pos * 2.5 + seed_offset);
    let moisture = clamp((moisture_noise * 0.5 + 0.5) * 200.0 * (0.5 + params.ocean_fraction), 0.0, 400.0);

    var r = biome_roughness_value(temp, moisture, is_ocean);

    // Add noise variation ±10%
    let noise_var = snoise(sphere_pos * 10.0 + seed_offset) * 0.1;
    r = clamp(r + noise_var, 0.0, 1.0);

    roughness[idx] = r;
}
