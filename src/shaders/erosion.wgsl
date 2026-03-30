// Hydraulic erosion compute shader.
// Single iteration: for each pixel, find steepest descent, erode proportional to slope,
// deposit material at lowest neighbor.
// Uses double-buffering: reads from input, writes to output.

struct ErosionParams {
    resolution: u32,
    erosion_rate: f32,
    deposition_rate: f32,
    min_slope: f32,
    talus_angle: f32, // tangent of max stable slope (~0.7 for ~35°)
    ocean_level: f32,
    _pad0: u32,
    _pad1: u32,
}

@group(0) @binding(0) var<storage, read> input_height: array<f32>;
@group(0) @binding(1) var<storage, read_write> output_height: array<f32>;
@group(0) @binding(2) var<uniform> params: ErosionParams;

fn get_height(x: i32, y: i32) -> f32 {
    let res = i32(params.resolution);
    let cx = clamp(x, 0, res - 1);
    let cy = clamp(y, 0, res - 1);
    return input_height[u32(cy) * params.resolution + u32(cx)];
}

@compute @workgroup_size(16, 16)
fn main(@builtin(global_invocation_id) id: vec3<u32>) {
    let res = params.resolution;
    if (id.x >= res || id.y >= res) {
        return;
    }

    let x = i32(id.x);
    let y = i32(id.y);
    let idx = id.y * res + id.x;
    let h = input_height[idx];

    // Skip ocean floor (don't erode underwater terrain)
    if (h < params.ocean_level - 0.05) {
        output_height[idx] = h;
        return;
    }

    // Find lowest neighbor (von Neumann: 4 neighbors)
    let h_left  = get_height(x - 1, y);
    let h_right = get_height(x + 1, y);
    let h_up    = get_height(x, y - 1);
    let h_down  = get_height(x, y + 1);

    var min_neighbor = h_left;
    min_neighbor = min(min_neighbor, h_right);
    min_neighbor = min(min_neighbor, h_up);
    min_neighbor = min(min_neighbor, h_down);

    let slope = h - min_neighbor;
    var new_h = h;

    // Hydraulic erosion: remove material proportional to slope
    if (slope > params.min_slope) {
        let erosion = min(slope * params.erosion_rate, slope * 0.5); // Don't erode more than half the slope
        new_h -= erosion;

        // Deposition happens implicitly: eroded material from neighbors raises this pixel
        // (handled by neighbors eroding toward us in their computation)
    }

    // Thermal erosion: if slope exceeds talus angle, material slides downhill
    if (slope > params.talus_angle) {
        let excess = (slope - params.talus_angle) * 0.5;
        new_h -= excess * 0.3; // Remove some material (slides to neighbor)
    }

    // Sediment deposition: if this pixel is a local minimum (all neighbors higher),
    // it accumulates sediment from uphill erosion
    if (h <= min_neighbor && h > params.ocean_level) {
        // Low point: accumulate some fill (simulates sediment deposition)
        let avg_neighbor = (h_left + h_right + h_up + h_down) * 0.25;
        let fill = (avg_neighbor - h) * params.deposition_rate;
        new_h += fill;
    }

    output_height[idx] = new_h;
}
