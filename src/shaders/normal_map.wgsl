// Normal map generation from heightmap via central differences.
// Input: heightmap (full-resolution storage buffer)
// Output: normal map (tile-sized storage buffer, resolution² × 4 floats RGBA)

struct NormalParams {
    resolution: u32,
    height_scale: f32,
    tile_offset_x: u32,
    tile_offset_y: u32,
    full_resolution: u32,
    local_height: u32,
    _pad1: u32,
    _pad2: u32,
}

@group(0) @binding(0) var<storage, read> heightmap: array<f32>;
@group(0) @binding(1) var<storage, read_write> normal_map: array<vec4<f32>>;
@group(0) @binding(2) var<uniform> params: NormalParams;

fn read_height(lx: u32, ly: u32) -> f32 {
    let cx = min(lx, params.resolution - 1u);
    let cy = min(ly, params.local_height - 1u);
    return heightmap[cy * params.resolution + cx];
}

@compute @workgroup_size(16, 16)
fn main(@builtin(global_invocation_id) id: vec3<u32>) {
    let res = params.resolution;
    if (id.x >= res || id.y >= params.local_height) {
        return;
    }

    let x_left = select(id.x - 1u, 0u, id.x == 0u);
    let x_right = min(id.x + 1u, res - 1u);
    let y_up = select(id.y - 1u, 0u, id.y == 0u);
    let y_down = min(id.y + 1u, params.local_height - 1u);

    let h_left = read_height(x_left, id.y);
    let h_right = read_height(x_right, id.y);
    let h_up = read_height(id.x, y_up);
    let h_down = read_height(id.x, y_down);

    // Central differences
    let dx = (h_right - h_left) * params.height_scale;
    let dy = (h_down - h_up) * params.height_scale;

    // Normal in tangent space
    let step = 2.0 / f32(params.full_resolution);
    var n = normalize(vec3<f32>(-dx / step, -dy / step, 1.0));

    // Encode to [0, 1] range for storage (normal map convention)
    let encoded = n * 0.5 + 0.5;

    let idx = id.y * res + id.x;
    normal_map[idx] = vec4<f32>(encoded, 1.0);
}
