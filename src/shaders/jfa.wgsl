// Pass 2: Jump Flooding Algorithm — one iteration per dispatch.
// Called multiple times with decreasing step sizes: res/2, res/4, ..., 2, 1.
// Propagates nearest boundary seed positions across the grid.

struct JfaSeed {
    seed_x: i32,
    seed_y: i32,
    plate_a: i32,
    plate_b: i32,
}

struct JfaParams {
    resolution: u32,
    step_size: u32,
    _pad0: u32,
    _pad1: u32,
}

@group(0) @binding(0) var<storage, read> jfa_src: array<JfaSeed>;
@group(0) @binding(1) var<storage, read_write> jfa_dst: array<JfaSeed>;
@group(0) @binding(2) var<uniform> params: JfaParams;

@compute @workgroup_size(16, 16)
fn main(@builtin(global_invocation_id) id: vec3<u32>) {
    let res = params.resolution;
    if (id.x >= res || id.y >= res) { return; }

    let idx = id.y * res + id.x;
    let step = i32(params.step_size);
    let my_pos = vec2<f32>(f32(id.x), f32(id.y));

    // Start with current best seed
    var best = jfa_src[idx];
    var best_dist = 1e20;
    if (best.seed_x >= 0) {
        let d = my_pos - vec2<f32>(f32(best.seed_x), f32(best.seed_y));
        best_dist = dot(d, d); // squared distance
    }

    // Check 9 neighbors at ±step offset
    for (var dy = -1; dy <= 1; dy++) {
        for (var dx = -1; dx <= 1; dx++) {
            let nx = i32(id.x) + dx * step;
            let ny = i32(id.y) + dy * step;

            if (nx >= 0 && nx < i32(res) && ny >= 0 && ny < i32(res)) {
                let neighbor = jfa_src[u32(ny) * res + u32(nx)];
                if (neighbor.seed_x >= 0) {
                    let d = my_pos - vec2<f32>(f32(neighbor.seed_x), f32(neighbor.seed_y));
                    let dist = dot(d, d);
                    if (dist < best_dist) {
                        best_dist = dist;
                        best = neighbor;
                    }
                }
            }
        }
    }

    jfa_dst[idx] = best;
}
