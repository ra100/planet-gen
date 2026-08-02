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
    tile_offset_x: u32,
    tile_offset_y: u32,
    full_resolution: u32,
    local_height: u32,
    _pad1: u32,
    _pad2: u32,
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
    if (id.x >= res || id.y >= params.local_height) {
        return;
    }

    // Global coordinates are only for planet-space calculations.
    let full_res = params.full_resolution;
    let global_x = params.tile_offset_x + id.x;
    let global_y = params.tile_offset_y + id.y;
    let height = heightmap[id.y * res + id.x];
    let is_ocean = height < params.ocean_level;

    let uv = vec2<f32>(
        f32(global_x) / f32(full_res - 1u),
        f32(global_y) / f32(full_res - 1u)
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

    // Slope & moisture-dependent roughness for land only
    if (!is_ocean) {
        // Compute slope from neighboring heights (central differences)
        let x_left = select(global_x - 1u, 0u, global_x == 0u);
        let x_right = min(global_x + 1u, full_res - 1u);
        let y_up = select(global_y - 1u, 0u, global_y == 0u);
        let y_down = min(global_y + 1u, full_res - 1u);

        let local_x_left = u32(clamp(i32(x_left) - i32(params.tile_offset_x), 0, i32(res) - 1));
        let local_x_right = u32(clamp(i32(x_right) - i32(params.tile_offset_x), 0, i32(res) - 1));
        let local_y_up = u32(clamp(i32(y_up) - i32(params.tile_offset_y), 0, i32(params.local_height) - 1));
        let local_y_down = u32(clamp(i32(y_down) - i32(params.tile_offset_y), 0, i32(params.local_height) - 1));
        let h_left = heightmap[id.y * res + local_x_left];
        let h_right = heightmap[id.y * res + local_x_right];
        let h_up = heightmap[local_y_up * res + id.x];
        let h_down = heightmap[local_y_down * res + id.x];

        let step_size = 2.0 / f32(full_res);
        let dx = (h_right - h_left) / step_size;
        let dy = (h_down - h_up) / step_size;
        let slope = length(vec2<f32>(dx, dy));

        // Steep slopes → rougher (exposed rock, scree)
        r = mix(r, 0.85, smoothstep(0.4, 2.0, slope));

        // Wet surfaces slightly smoother
        let moisture_norm = clamp(moisture / 400.0, 0.0, 1.0);
        r *= mix(1.0, 0.75, moisture_norm);

        // Add noise variation
        let noise_var = snoise(sphere_pos * 10.0 + seed_offset) * 0.1;
        r = clamp(r + noise_var, 0.05, 1.0);
    }

    // Write to tile-local index
    let idx = id.y * res + id.x;
    roughness[idx] = r;
}
