// Ambient occlusion map generation from heightmap curvature.
// Multi-scale approach: samples at 3 radii and blends.
// Ocean pixels receive AO = 1.0 (no occlusion on water).

struct AoParams {
    face: u32,
    full_resolution: u32,
    ao_strength: f32,
    ocean_level: f32,
    tile_offset_x: u32,
    tile_offset_y: u32,
    resolution: u32,
    _pad0: u32,
}

@group(0) @binding(0) var<storage, read> heightmap: array<f32>;
@group(0) @binding(1) var<storage, read_write> ao_output: array<f32>;
@group(0) @binding(2) var<uniform> params: AoParams;

fn read_height(gx: i32, gy: i32) -> f32 {
    let full = i32(params.full_resolution);
    let cx = clamp(gx, 0, full - 1);
    let cy = clamp(gy, 0, full - 1);
    return heightmap[u32(cy) * params.full_resolution + u32(cx)];
}

// Compute curvature at a given radius by comparing center to ring average.
fn curvature_at_radius(gx: i32, gy: i32, center_h: f32, radius: i32) -> f32 {
    var sum = 0.0;
    var count = 0.0;

    // Sample the 8 neighbors at the given radius step
    for (var dy = -radius; dy <= radius; dy++) {
        for (var dx = -radius; dx <= radius; dx++) {
            if (dx == 0 && dy == 0) { continue; }
            // Only sample at approximately the given radius (avoid interior pixels)
            if (abs(dx) < radius && abs(dy) < radius) { continue; }
            sum += read_height(gx + dx, gy + dy);
            count += 1.0;
        }
    }

    if (count == 0.0) { return 0.0; }
    let neighbor_avg = sum / count;
    // Positive curvature = ridge/convex = bright (AO = 1.0)
    // Negative curvature = valley/concave = dark (AO < 1.0)
    return center_h - neighbor_avg;
}

@compute @workgroup_size(8, 8, 1)
fn main(@builtin(global_invocation_id) id: vec3<u32>) {
    let res = params.resolution;
    if (id.x >= res || id.y >= res) {
        return;
    }

    let gx = i32(params.tile_offset_x + id.x);
    let gy = i32(params.tile_offset_y + id.y);
    let center_h = read_height(gx, gy);

    // Ocean gets full AO (no occlusion)
    if (center_h < params.ocean_level) {
        let idx = id.y * res + id.x;
        ao_output[idx] = 1.0;
        return;
    }

    // Multi-scale curvature: small (r=1), medium (r=3), large (r=6)
    let curv_small  = curvature_at_radius(gx, gy, center_h, 1);
    let curv_medium = curvature_at_radius(gx, gy, center_h, 3);
    let curv_large  = curvature_at_radius(gx, gy, center_h, 6);

    // Weighted blend: small details matter most for AO
    let blended_curv = curv_small * 0.5 + curv_medium * 0.35 + curv_large * 0.15;

    // Map curvature to AO: negative = darker, positive = stays at 1.0
    let ao = clamp(0.5 + blended_curv * params.ao_strength, 0.0, 1.0);

    let idx = id.y * res + id.x;
    ao_output[idx] = ao;
}
