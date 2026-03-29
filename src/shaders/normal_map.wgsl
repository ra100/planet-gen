// Normal map generation from heightmap via central differences.
// Input: heightmap (storage buffer, resolution²)
// Output: normal map (storage buffer, resolution² × 4 floats RGBA)

struct NormalParams {
    resolution: u32,
    height_scale: f32,
    _pad0: u32,
    _pad1: u32,
}

@group(0) @binding(0) var<storage, read> heightmap: array<f32>;
@group(0) @binding(1) var<storage, read_write> normal_map: array<vec4<f32>>;
@group(0) @binding(2) var<uniform> params: NormalParams;

@compute @workgroup_size(16, 16)
fn main(@builtin(global_invocation_id) id: vec3<u32>) {
    let res = params.resolution;
    if (id.x >= res || id.y >= res) {
        return;
    }

    let x = id.x;
    let y = id.y;

    // Sample neighboring heights with clamping at edges
    let x_left = select(x - 1u, 0u, x == 0u);
    let x_right = select(x + 1u, res - 1u, x >= res - 1u);
    let y_up = select(y - 1u, 0u, y == 0u);
    let y_down = select(y + 1u, res - 1u, y >= res - 1u);

    let h_left = heightmap[y * res + x_left];
    let h_right = heightmap[y * res + x_right];
    let h_up = heightmap[y_up * res + x];
    let h_down = heightmap[y_down * res + x];

    // Central differences
    let dx = (h_right - h_left) * params.height_scale;
    let dy = (h_down - h_up) * params.height_scale;

    // Normal in tangent space: cross product of (2/res, 0, dx) and (0, 2/res, dy)
    let step = 2.0 / f32(res);
    var n = normalize(vec3<f32>(-dx / step, -dy / step, 1.0));

    // Encode to [0, 1] range for storage (normal map convention)
    let encoded = n * 0.5 + 0.5;

    let idx = y * res + x;
    normal_map[idx] = vec4<f32>(encoded, 1.0);
}
