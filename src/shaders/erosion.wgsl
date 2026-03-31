// Hydraulic erosion with D8 steepest-descent drainage + channel carving.
// Includes noise.wgsl at load time for roughening.
//
// Two entry points:
//   accumulate_flow: D8 routing — each pixel receives water from uphill neighbors
//                    whose steepest descent leads to it. Ping-pong water buffers.
//   erode: channel carving where drainage concentrates, gentle weathering elsewhere.

struct ErosionParams {
    resolution: u32,
    erosion_rate: f32,
    deposition_rate: f32,
    min_slope: f32,
    channel_threshold: f32, // drainage level above which channel carving activates
    ocean_level: f32,
    seed: u32,
    _pad0: u32,
}

@group(0) @binding(0) var<storage, read> input_height: array<f32>;
@group(0) @binding(1) var<storage, read_write> output_height: array<f32>;
@group(0) @binding(2) var<uniform> params: ErosionParams;
@group(0) @binding(3) var<storage, read> water_in: array<f32>;
@group(0) @binding(4) var<storage, read_write> water_out: array<f32>;

fn get_h(x: i32, y: i32) -> f32 {
    let res = i32(params.resolution);
    let cx = clamp(x, 0, res - 1);
    let cy = clamp(y, 0, res - 1);
    return input_height[u32(cy) * params.resolution + u32(cx)];
}

fn get_water_in(x: i32, y: i32) -> f32 {
    let res = i32(params.resolution);
    let cx = clamp(x, 0, res - 1);
    let cy = clamp(y, 0, res - 1);
    return water_in[u32(cy) * params.resolution + u32(cx)];
}

// Find the steepest-descent neighbor (D8) using slope, not raw height.
// Diagonal neighbors are sqrt(2) away — dividing by distance prevents
// bias toward cardinal directions that creates straight-line artifacts.
// A small noise jitter breaks remaining grid alignment.
fn lowest_neighbor(x: i32, y: i32) -> vec2<i32> {
    let h = get_h(x, y);
    var max_slope = 0.0;
    var min_pos = vec2<i32>(x, y);

    let offsets = array<vec2<i32>, 8>(
        vec2<i32>(-1, -1), vec2<i32>(0, -1), vec2<i32>(1, -1),
        vec2<i32>(-1,  0),                    vec2<i32>(1,  0),
        vec2<i32>(-1,  1), vec2<i32>(0,  1), vec2<i32>(1,  1)
    );
    let dists = array<f32, 8>(
        1.414, 1.0, 1.414,
        1.0,        1.0,
        1.414, 1.0, 1.414
    );

    // Noise jitter to break grid-aligned ties
    let jitter_seed = vec3<f32>(f32(x) * 0.37 + f32(params.seed) * 0.01, f32(y) * 0.41, 0.0);
    let jitter = snoise(jitter_seed) * 0.003;

    for (var i = 0; i < 8; i++) {
        let nx = x + offsets[i].x;
        let ny = y + offsets[i].y;
        let nh = get_h(nx, ny);
        let drop = h - nh;
        if (drop > 0.0) {
            let slope = drop / dists[i] + jitter * (f32(i) - 3.5) * 0.1;
            if (slope > max_slope) {
                max_slope = slope;
                min_pos = vec2<i32>(nx, ny);
            }
        }
    }

    return min_pos;
}

// Compute what fraction of a neighbor's water flows toward (tx, ty).
// Uses MFD (Multiple Flow Direction): water splits proportionally among
// all downhill neighbors based on slope, preventing single-pixel channels.
fn flow_fraction(nx: i32, ny: i32, tx: i32, ty: i32) -> f32 {
    let nh = get_h(nx, ny);
    let offsets = array<vec2<i32>, 8>(
        vec2<i32>(-1, -1), vec2<i32>(0, -1), vec2<i32>(1, -1),
        vec2<i32>(-1,  0),                    vec2<i32>(1,  0),
        vec2<i32>(-1,  1), vec2<i32>(0,  1), vec2<i32>(1,  1)
    );
    let dists = array<f32, 8>(
        1.414, 1.0, 1.414,
        1.0,        1.0,
        1.414, 1.0, 1.414
    );

    var target_slope = 0.0;
    var total_slope = 0.0;

    for (var i = 0; i < 8; i++) {
        let cx = nx + offsets[i].x;
        let cy = ny + offsets[i].y;
        let ch = get_h(cx, cy);
        let drop = nh - ch;
        if (drop > 0.0) {
            let s = drop / dists[i];
            total_slope += s;
            if (cx == tx && cy == ty) {
                target_slope = s;
            }
        }
    }

    if (total_slope <= 0.0 || target_slope <= 0.0) { return 0.0; }
    return target_slope / total_slope;
}

// Pass 1: MFD flow accumulation — each pixel receives a proportional share
// of water from uphill neighbors based on relative slope. Ping-pong buffers.
// Run 64+ times to propagate water from ridgelines to valleys.
@compute @workgroup_size(16, 16)
fn accumulate_flow(@builtin(global_invocation_id) id: vec3<u32>) {
    let res = params.resolution;
    if (id.x >= res || id.y >= res) { return; }

    let x = i32(id.x);
    let y = i32(id.y);
    let idx = id.y * res + id.x;
    let h = input_height[idx];

    // Start with base rainfall
    var w = 1.0;

    let offsets = array<vec2<i32>, 8>(
        vec2<i32>(-1, -1), vec2<i32>(0, -1), vec2<i32>(1, -1),
        vec2<i32>(-1,  0),                    vec2<i32>(1,  0),
        vec2<i32>(-1,  1), vec2<i32>(0,  1), vec2<i32>(1,  1)
    );

    for (var i = 0; i < 8; i++) {
        let nx = x + offsets[i].x;
        let ny = y + offsets[i].y;
        let nh = get_h(nx, ny);

        // Only receive water from uphill neighbors
        if (nh > h) {
            let frac = flow_fraction(nx, ny, x, y);
            w += get_water_in(nx, ny) * frac;
        }
    }

    water_out[idx] = w;
}

// Pass 2: Channel carving + detail-preserving weathering
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

    // Find steepest slope (8 neighbors)
    let offsets = array<vec2<i32>, 8>(
        vec2<i32>(-1, -1), vec2<i32>(0, -1), vec2<i32>(1, -1),
        vec2<i32>(-1,  0),                    vec2<i32>(1,  0),
        vec2<i32>(-1,  1), vec2<i32>(0,  1), vec2<i32>(1,  1)
    );

    var min_neighbor = h;
    var avg_neighbor = 0.0;
    for (var i = 0; i < 8; i++) {
        let nh = get_h(x + offsets[i].x, y + offsets[i].y);
        min_neighbor = min(min_neighbor, nh);
        avg_neighbor += nh;
    }
    avg_neighbor /= 8.0;

    let slope = max(h - min_neighbor, 0.0);
    let drainage = water_in[idx];

    var new_h = h;

    if (drainage > params.channel_threshold) {
        // === Channel carving ===
        // Stream-power: E = K * A^0.5 * S — concentrated flow cuts valleys
        let stream_power = sqrt(drainage) * slope;
        if (stream_power > params.min_slope) {
            let erosion = min(stream_power * params.erosion_rate, slope * 0.5);
            new_h -= erosion;
        }

        // Deposition at convergence points (high drainage + low slope = river floodplain)
        if (slope < 0.01 && h > params.ocean_level) {
            let fill = min((avg_neighbor - h) * params.deposition_rate, 0.02);
            if (fill > 0.0) {
                new_h += fill;
            }
        }
    } else {
        // === Gentle weathering ===
        // Very slight peak softening — move toward neighbor average by tiny amount
        // Only at peaks (higher than average neighbors)
        if (h > avg_neighbor) {
            let soften = (h - avg_neighbor) * 0.02;
            new_h -= soften;
        }
    }

    // Add roughening noise to eroded lowland areas (weathered rock texture)
    let land_height = h - params.ocean_level;
    if (land_height > 0.0 && land_height < 0.3) {
        let erosion_factor = drainage / max(params.channel_threshold, 1.0);
        let rough_amount = min(erosion_factor, 1.0) * 0.008;
        let pos = vec3<f32>(f32(x) * 0.1 + f32(params.seed) * 0.01, f32(y) * 0.1, 0.0);
        let roughness = snoise(pos * 2.0);
        new_h += roughness * rough_amount;
    }

    output_height[idx] = new_h;
}
