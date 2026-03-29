// Crater stamping on heightmap.
// Uses deterministic pseudo-random placement based on seed.
// Crater count scales with surface_age (more craters on older surfaces).
// Research: d/D ≈ 0.2 (simple craters), ejecta ±1 radius

struct CraterParams {
    face: u32,
    resolution: u32,
    seed: u32,
    num_craters: u32,
    // Crater data: packed as (x, y, radius, depth) per crater
    // Max 64 craters per face
}

@group(0) @binding(0) var<storage, read_write> heightmap: array<f32>;
@group(0) @binding(1) var<uniform> params: CraterParams;
@group(0) @binding(2) var<storage, read> crater_data: array<vec4<f32>>; // x, y, radius, depth

@compute @workgroup_size(16, 16)
fn main(@builtin(global_invocation_id) id: vec3<u32>) {
    let res = params.resolution;
    if (id.x >= res || id.y >= res) {
        return;
    }

    let idx = id.y * res + id.x;
    let uv = vec2<f32>(
        f32(id.x) / f32(res - 1u),
        f32(id.y) / f32(res - 1u)
    );

    var height_mod = 0.0;

    for (var i = 0u; i < params.num_craters; i++) {
        let crater = crater_data[i];
        let center = vec2<f32>(crater.x, crater.y);
        let radius = crater.z;
        let depth = crater.w;

        let dist = length(uv - center);
        let r = dist / radius;

        if (r < 2.0) {
            if (r < 0.8) {
                // Crater floor: depressed
                height_mod -= depth * (1.0 - r / 0.8);
            } else if (r < 1.0) {
                // Crater rim: raised
                let rim_t = (r - 0.8) / 0.2;
                height_mod += depth * 0.3 * (1.0 - rim_t);
            } else {
                // Ejecta blanket: slight raise that fades
                let ejecta_t = (r - 1.0) / 1.0;
                height_mod += depth * 0.1 * (1.0 - ejecta_t) * (1.0 - ejecta_t);
            }
        }
    }

    heightmap[idx] += height_mod;
}
