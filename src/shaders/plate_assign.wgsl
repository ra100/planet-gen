// Pass 1: Assign each pixel to nearest plate + initialize JFA boundary seeds.
// Output: plate_idx per pixel + JFA seed buffer with boundary pixels marked.
// Includes noise.wgsl and cube_sphere.wgsl (concatenated at load time).

struct Plate {
    center: vec3<f32>,
    plate_type: f32,
    velocity: vec3<f32>,
    _pad: f32,
}

struct AssignParams {
    face: u32,
    resolution: u32,
    num_plates: u32,
    seed: u32,
    warp_strength: f32,
    _pad0: u32,
    _pad1: u32,
    _pad2: u32,
}

// JFA seed data: (seed_x, seed_y, plate_a_idx, plate_b_idx)
// plate_a = the plate on this side, plate_b = the plate on the other side of boundary
struct JfaSeed {
    seed_x: i32,
    seed_y: i32,
    plate_a: i32,  // plate index on this pixel's side
    plate_b: i32,  // plate index on the other side (-1 if not a boundary)
}

@group(0) @binding(0) var<storage, read> plates: array<Plate>;
@group(0) @binding(1) var<uniform> params: AssignParams;
@group(0) @binding(2) var<storage, read_write> plate_idx: array<u32>;
@group(0) @binding(3) var<storage, read_write> jfa_seeds: array<JfaSeed>;

// PCG hash for seed offsets
fn pcg_hash(input: u32) -> u32 {
    var h = input * 747796405u + 2891336453u;
    h = ((h >> ((h >> 28u) + 4u)) ^ h) * 277803737u;
    h = (h >> 22u) ^ h;
    return h;
}

fn seed_offset(s: u32) -> vec3<f32> {
    let h1 = pcg_hash(s);
    let h2 = pcg_hash(s + 1u);
    let h3 = pcg_hash(s + 2u);
    return vec3<f32>(
        f32(h1 & 0xFFFFu) / 655.35,
        f32(h2 & 0xFFFFu) / 655.35,
        f32(h3 & 0xFFFFu) / 655.35
    );
}

// Domain warping (seed-dependent)
fn warp_position(pos: vec3<f32>) -> vec3<f32> {
    let warp1 = vec3<f32>(
        snoise(pos * 1.2 + seed_offset(params.seed + 5000u)),
        snoise(pos * 1.2 + seed_offset(params.seed + 5001u)),
        snoise(pos * 1.2 + seed_offset(params.seed + 5002u))
    ) * 0.20;
    let warp2 = vec3<f32>(
        snoise(pos * 3.0 + seed_offset(params.seed + 5010u)),
        snoise(pos * 3.0 + seed_offset(params.seed + 5011u)),
        snoise(pos * 3.0 + seed_offset(params.seed + 5012u))
    ) * 0.10;
    let w = params.warp_strength;
    return normalize(pos + (warp1 + warp2) * w);
}

// Find nearest plate for a sphere position
fn find_nearest_plate(sphere_pos: vec3<f32>) -> u32 {
    var nearest_idx = 0u;
    var nearest_dist = 100.0;

    for (var i = 0u; i < params.num_plates; i++) {
        var d = 1.0 - dot(sphere_pos, plates[i].center);
        // Per-plate noise bias for organic boundaries
        let plate_offset = vec3<f32>(f32(i) * 17.3, f32(i) * 31.7, f32(i) * 43.1);
        let bias = snoise(sphere_pos * 2.0 + plate_offset) * 0.03
                 + snoise(sphere_pos * 3.5 + plate_offset * 2.0) * 0.01;
        d += bias;

        if (d < nearest_dist) {
            nearest_dist = d;
            nearest_idx = i;
        }
    }
    return nearest_idx;
}

@compute @workgroup_size(16, 16)
fn main(@builtin(global_invocation_id) id: vec3<u32>) {
    let res = params.resolution;
    if (id.x >= res || id.y >= res) { return; }

    let uv = vec2<f32>(
        f32(id.x) / f32(res - 1u),
        f32(id.y) / f32(res - 1u)
    );

    let raw_pos = cube_to_sphere(params.face, uv);
    let sphere_pos = warp_position(raw_pos);

    // Assign this pixel to nearest plate
    let my_plate = find_nearest_plate(sphere_pos);
    let idx = id.y * res + id.x;
    plate_idx[idx] = my_plate;

    // Boundary detection: check 4 neighbors for different plate assignment
    // We sample the sphere positions of neighbors and find their plates
    var is_boundary = false;
    var neighbor_plate = my_plate;

    let offsets = array<vec2<i32>, 4>(
        vec2<i32>(1, 0), vec2<i32>(-1, 0),
        vec2<i32>(0, 1), vec2<i32>(0, -1)
    );

    for (var n = 0; n < 4; n++) {
        let nx = i32(id.x) + offsets[n].x;
        let ny = i32(id.y) + offsets[n].y;
        if (nx >= 0 && nx < i32(res) && ny >= 0 && ny < i32(res)) {
            let nuv = vec2<f32>(
                f32(nx) / f32(res - 1u),
                f32(ny) / f32(res - 1u)
            );
            let npos = warp_position(cube_to_sphere(params.face, nuv));
            let np = find_nearest_plate(npos);
            if (np != my_plate) {
                is_boundary = true;
                neighbor_plate = np;
            }
        }
    }

    // Initialize JFA seed
    if (is_boundary) {
        jfa_seeds[idx] = JfaSeed(i32(id.x), i32(id.y), i32(my_plate), i32(neighbor_plate));
    } else {
        jfa_seeds[idx] = JfaSeed(-1, -1, i32(my_plate), -1);
    }
}
