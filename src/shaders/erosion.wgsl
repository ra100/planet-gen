// Multiple-flow drainage with cached routing, annual runoff, and wave reworking.
// Concatenate after noise.wgsl, climate.wgsl, and cube_sphere.wgsl.
struct ErosionParams {
    width: u32,
    height: u32,
    full_resolution: u32,
    row_offset: u32,
    erosion_rate: f32,
    deposition_rate: f32,
    min_slope: f32,
    channel_threshold: f32,
    ocean_level: f32,
    seed: u32,
    face: u32,
    base_temp_c: f32,
    axial_tilt_rad: f32,
    moisture: f32,
    ocean_fraction: f32,
    _pad0: u32,
}

@group(0) @binding(0) var<storage, read> input_height: array<f32>;
@group(0) @binding(1) var<storage, read_write> output_height: array<f32>;
@group(0) @binding(2) var<uniform> params: ErosionParams;
@group(0) @binding(3) var<storage, read> water_in: array<f32>;
@group(0) @binding(4) var<storage, read_write> water_out: array<f32>;
// x: inverse sum of downhill slopes; y: rainfall remaining after infiltration.
@group(0) @binding(5) var<storage, read_write> routing: array<vec2<f32>>;

const OFFSETS = array<vec2<i32>, 8>(
    vec2<i32>(-1, -1), vec2<i32>(0, -1), vec2<i32>(1, -1),
    vec2<i32>(-1,  0),                    vec2<i32>(1,  0),
    vec2<i32>(-1,  1), vec2<i32>(0,  1), vec2<i32>(1,  1)
);
const INV_DISTANCE = array<f32, 8>(
    0.70710678, 1.0, 0.70710678, 1.0, 1.0, 0.70710678, 1.0, 0.70710678
);

fn sample_index(x: i32, y: i32) -> u32 {
    let cx = clamp(x, 0, i32(params.width) - 1);
    var cy = clamp(y, 0, i32(params.height) - 1);
    if (y <= 0 && params.row_offset == 0u) { cy = 1; }
    if (y >= i32(params.height) - 1 && params.row_offset + params.height - 2u == params.full_resolution) {
        cy = i32(params.height) - 2;
    }
    return u32(cy) * params.width + u32(cx);
}

fn sphere_position(id: vec3<u32>) -> vec3<f32> {
    let global_y = id.y - 1u + params.row_offset;
    let uv = vec2<f32>(f32(id.x), f32(global_y)) / max(f32(params.full_resolution - 1u), 1.0);
    return cube_to_sphere(params.face, uv);
}

fn annual_runoff(p: vec3<f32>, h: f32) -> f32 {
    if (h <= params.ocean_level || params.moisture <= 0.0 || params.ocean_fraction <= 0.0) {
        return 0.0;
    }
    // Annual belts remain stationary when the displayed season changes.
    let latitude = abs(climate_latitude(p, params.axial_tilt_rad)) * 57.2957795;
    let tropical = 200.0 * exp(-latitude * latitude / 200.0);
    let dry = -80.0 * exp(-(latitude - 30.0) * (latitude - 30.0) / 60.0);
    let temperate = 90.0 * exp(-(latitude - 45.0) * (latitude - 45.0) / 200.0);
    let belts = max(10.0, tropical + dry + temperate + 90.0 - 60.0 * smoothstep(65.0, 85.0, latitude));
    let regional = snoise(p * 3.0 + noise_seed_offset(params.seed, 73u)) * 0.5;
    let ocean_scale = 0.25 + 0.75 * clamp(params.ocean_fraction, 0.0, 1.0);
    var rain = belts * (0.775 + 0.45 * regional) + 50.0 * (regional + 0.5);
    rain *= ocean_scale * (0.5 + params.ocean_fraction) * clamp(params.moisture, 0.0, 1.0);
    rain *= 1.0 + snoise(p * 0.7 + noise_seed_offset(params.seed, 74u)) * 0.25;
    // Elevation reduces marine supply inland; warm dry soil absorbs weak rain.
    let land_height = max(h - params.ocean_level, 0.0);
    rain *= 1.0 - 0.55 * smoothstep(0.1, 0.7, land_height);
    let temp = climate_temperature(p, h, params.ocean_level, params.base_temp_c, params.axial_tilt_rad, 0.5);
    let liquid = smoothstep(-5.0, 8.0, temp);
    let effective_rain = max(rain - mix(8.0, 28.0, smoothstep(5.0, 30.0, temp)), 0.0);
    return clamp(effective_rain / 100.0, 0.0, 2.5) * liquid;
}

fn valid_neighbor(n: vec2<i32>) -> bool {
    return n.x >= 0 && n.x < i32(params.width) &&
        !(n.y < 1 && params.row_offset == 0u) &&
        !(n.y + 1 >= i32(params.height) && params.row_offset + params.height - 2u == params.full_resolution);
}

@compute @workgroup_size(16, 16)
fn prepare_flow(@builtin(global_invocation_id) id: vec3<u32>) {
    if (id.x >= params.width || id.y == 0u || id.y + 1u >= params.height) { return; }
    let x = i32(id.x);
    let y = i32(id.y);
    let idx = id.y * params.width + id.x;
    let h = input_height[idx];
    var sum = 0.0;
    if (h > params.ocean_level && params.moisture > 0.0) {
        for (var i = 0u; i < 8u; i++) {
            let n = vec2<i32>(x, y) + OFFSETS[i];
            // Missing face-edge neighbors are not copies of the edge pixel.
            if (!valid_neighbor(n)) { continue; }
            sum += max(h - input_height[sample_index(n.x, n.y)], 0.0) * INV_DISTANCE[i];
        }
    }
    let runoff = annual_runoff(sphere_position(id), h);
    routing[idx] = vec2<f32>(select(0.0, 1.0 / max(sum, 1e-12), sum > 0.0), runoff);
    water_out[idx] = runoff;
}

@compute @workgroup_size(16, 16)
fn accumulate_flow(@builtin(global_invocation_id) id: vec3<u32>) {
    if (id.x >= params.width || id.y == 0u || id.y + 1u >= params.height) { return; }
    let x = i32(id.x);
    let y = i32(id.y);
    let idx = id.y * params.width + id.x;
    let h = input_height[idx];
    // Ocean absorbs drainage; it cannot supply freshwater to coastal pixels.
    if (h <= params.ocean_level) {
        water_out[idx] = 0.0;
        return;
    }
    var water = routing[idx].y;
    for (var i = 0u; i < 8u; i++) {
        let n = vec2<i32>(x, y) + OFFSETS[i];
        if (!valid_neighbor(n)) { continue; }
        let ni = sample_index(n.x, n.y);
        let drop = max(input_height[ni] - h, 0.0);
        water += water_in[ni] * drop * INV_DISTANCE[i] * routing[ni].x;
    }
    water_out[idx] = water;
}

@compute @workgroup_size(16, 16)
fn erode(@builtin(global_invocation_id) id: vec3<u32>) {
    if (id.x >= params.width || id.y == 0u || id.y + 1u >= params.height) { return; }
    let x = i32(id.x);
    let y = i32(id.y);
    let idx = id.y * params.width + id.x;
    let h = input_height[idx];
    let land_height = h - params.ocean_level;
    if (land_height < -0.018) {
        output_height[idx] = h;
        return;
    }

    var min_neighbor = h;
    var max_neighbor = h;
    var average = 0.0;
    var slope = 0.0;
    for (var i = 0u; i < 8u; i++) {
        let n = vec2<i32>(x, y) + OFFSETS[i];
        let nh = input_height[sample_index(n.x, n.y)];
        min_neighbor = min(min_neighbor, nh);
        max_neighbor = max(max_neighbor, nh);
        average += nh;
        slope = max(slope, max(h - nh, 0.0) * INV_DISTANCE[i]);
    }
    average /= 8.0;
    let drainage = max(water_in[idx], 0.0);
    let runoff = routing[idx].y;
    let channel = smoothstep(params.channel_threshold, params.channel_threshold * 3.0, drainage);
    // Rivers lose incision power approaching sea level. Upstream discharge
    // can still cross dry lowlands even when local rainfall is negligible.
    let base_level = smoothstep(0.0, 0.025, land_height);
    var new_h = h;
    if (land_height > 0.0) {
        let stream_power = sqrt(drainage) * slope;
        let incision = min(max(stream_power - params.min_slope, 0.0) * params.erosion_rate * channel,
            max(h - max(min_neighbor, params.ocean_level), 0.0) * 0.25);
        new_h -= incision * base_level;
        // Wet hillslopes relax gently; dry terrain retains its original relief.
        new_h -= max(h - average, 0.0) * 0.004 * min(runoff, 1.0) * (1.0 - channel) * base_level;
        if (channel > 0.0 && slope < 0.003) {
            new_h += max(average - h, 0.0) * params.deposition_rate * channel * base_level;
        }
    }

    // Wave reworking moves loose sediment both ways on shallow, gentle shores.
    // Steep headlands retain relief; concentrated river mouths resist filling.
    let gradient = vec2<f32>(
        input_height[sample_index(x + 1, y)] - input_height[sample_index(x - 1, y)],
        input_height[sample_index(x, y + 1)] - input_height[sample_index(x, y - 1)]
    ) * f32(params.full_resolution) * 0.31830989;
    let sand = 1.0 - smoothstep(0.35, 1.6, length(gradient));
    let shore = 1.0 - smoothstep(0.004, 0.018, abs(land_height));
    let marine_contact = select(0.0, 1.0, min_neighbor <= params.ocean_level);
    if (sand * shore * marine_contact > 0.0 && params.ocean_fraction > 0.0) {
        let p = sphere_position(id);
        let temp = climate_temperature(p, h, params.ocean_level, params.base_temp_c, params.axial_tilt_rad, 0.5);
        let waves = sand * shore * marine_contact * smoothstep(-3.0, 5.0, temp)
            / (1.0 + drainage / params.channel_threshold);
        let delta = (average - h) * 0.18 * waves;
        // Bound redistribution and retain the original shoreline classification.
        let limit = min((max_neighbor - min_neighbor) * 0.12, abs(land_height) * 0.25);
        new_h += clamp(delta, -limit, limit);
    }
    if (land_height > 0.0) {
        new_h = max(new_h, params.ocean_level);
    }
    output_height[idx] = new_h;
}
