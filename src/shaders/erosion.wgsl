// Hydraulic erosion with drainage accumulation.
// Two entry points:
//   accumulate_flow: propagate water downhill to compute drainage area per pixel
//   erode: use drainage area + slope for stream-power erosion

struct ErosionParams {
    resolution: u32,
    erosion_rate: f32,
    deposition_rate: f32,
    min_slope: f32,
    talus_angle: f32,
    ocean_level: f32,
    _pad0: u32,
    _pad1: u32,
}

@group(0) @binding(0) var<storage, read> input_height: array<f32>;
@group(0) @binding(1) var<storage, read_write> output_height: array<f32>;
@group(0) @binding(2) var<uniform> params: ErosionParams;
@group(0) @binding(3) var<storage, read_write> water: array<f32>; // drainage accumulation

fn get_h(x: i32, y: i32) -> f32 {
    let res = i32(params.resolution);
    let cx = clamp(x, 0, res - 1);
    let cy = clamp(y, 0, res - 1);
    return input_height[u32(cy) * params.resolution + u32(cx)];
}

fn get_water(x: i32, y: i32) -> f32 {
    let res = i32(params.resolution);
    let cx = clamp(x, 0, res - 1);
    let cy = clamp(y, 0, res - 1);
    return water[u32(cy) * params.resolution + u32(cx)];
}

// Pass 1: Flow accumulation — each pixel receives water from uphill neighbors
// Run this ~10 times to propagate water from ridgelines to valleys.
@compute @workgroup_size(16, 16)
fn accumulate_flow(@builtin(global_invocation_id) id: vec3<u32>) {
    let res = params.resolution;
    if (id.x >= res || id.y >= res) { return; }

    let x = i32(id.x);
    let y = i32(id.y);
    let idx = id.y * res + id.x;
    let h = input_height[idx];

    // Start with 1.0 unit of rainfall per pixel
    var w = 1.0;

    // Receive water from each neighbor that drains toward us
    // (neighbor's lowest point is us)
    let neighbors = array<vec2<i32>, 4>(
        vec2<i32>(x - 1, y), vec2<i32>(x + 1, y),
        vec2<i32>(x, y - 1), vec2<i32>(x, y + 1)
    );

    for (var i = 0; i < 4; i++) {
        let nx = neighbors[i].x;
        let ny = neighbors[i].y;
        let nh = get_h(nx, ny);

        if (nh > h) {
            // This neighbor is higher — check if WE are its lowest neighbor
            let n_left  = get_h(nx - 1, ny);
            let n_right = get_h(nx + 1, ny);
            let n_up    = get_h(nx, ny - 1);
            let n_down  = get_h(nx, ny + 1);

            let n_min = min(min(n_left, n_right), min(n_up, n_down));

            // If we're the lowest neighbor of this uphill pixel, receive its water
            if (abs(h - n_min) < 0.0001) {
                w += get_water(nx, ny) * 0.25; // Partial transfer per iteration
            }
        }
    }

    water[idx] = w;
}

// Pass 2: Stream-power erosion using accumulated drainage area
@compute @workgroup_size(16, 16)
fn erode(@builtin(global_invocation_id) id: vec3<u32>) {
    let res = params.resolution;
    if (id.x >= res || id.y >= res) { return; }

    let x = i32(id.x);
    let y = i32(id.y);
    let idx = id.y * res + id.x;
    let h = input_height[idx];

    // Skip deep ocean
    if (h < params.ocean_level - 0.05) {
        output_height[idx] = h;
        return;
    }

    // Find lowest neighbor
    let h_left  = get_h(x - 1, y);
    let h_right = get_h(x + 1, y);
    let h_up    = get_h(x, y - 1);
    let h_down  = get_h(x, y + 1);

    var min_neighbor = min(min(h_left, h_right), min(h_up, h_down));
    let slope = max(h - min_neighbor, 0.0);

    var new_h = h;

    // Stream power erosion: E = K * A^0.5 * S^1.0
    // A = drainage area (water accumulation), S = slope
    let drainage = water[idx];
    let stream_power = sqrt(drainage) * slope;

    if (stream_power > params.min_slope) {
        // Erosion proportional to stream power — more water = deeper cut
        let erosion = min(stream_power * params.erosion_rate, slope * 0.4);
        new_h -= erosion;
    }

    // Thermal erosion: talus slope collapse
    if (slope > params.talus_angle) {
        let excess = (slope - params.talus_angle) * 0.3;
        new_h -= excess;
    }

    // Deposition at local minima (valley floors)
    if (h <= min_neighbor && h > params.ocean_level) {
        let avg_neighbor = (h_left + h_right + h_up + h_down) * 0.25;
        let fill = (avg_neighbor - h) * params.deposition_rate;
        new_h += fill;
    }

    output_height[idx] = new_h;
}
