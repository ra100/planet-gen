// Terrain heightmap generation via fBm on cube-sphere faces.
// Combines cube_sphere.wgsl and noise.wgsl (concatenated at load time).

struct TerrainParams {
    face: u32,
    resolution: u32,
    octaves: u32,
    seed: u32,
    frequency: f32,
    lacunarity: f32,
    gain: f32,
    amplitude: f32,
}

@group(0) @binding(0) var<storage, read_write> heightmap: array<f32>;
@group(0) @binding(1) var<uniform> params: TerrainParams;

// Hash seed to a small, well-distributed offset in [0, 100) range.
// Avoids floating point precision issues at large coordinates.
fn hash_seed(s: u32) -> vec3<f32> {
    // Simple integer hash (Wang hash variant)
    var h1 = s;
    h1 = (h1 ^ 61u) ^ (h1 >> 16u);
    h1 = h1 * 9u;
    h1 = h1 ^ (h1 >> 4u);
    h1 = h1 * 0x27d4eb2du;
    h1 = h1 ^ (h1 >> 15u);

    var h2 = h1 * 0x85ebca6bu;
    h2 = h2 ^ (h2 >> 13u);

    var h3 = h2 * 0xc2b2ae35u;
    h3 = h3 ^ (h3 >> 16u);

    return vec3<f32>(
        f32(h1 % 10000u) * 0.01,
        f32(h2 % 10000u) * 0.01,
        f32(h3 % 10000u) * 0.01
    );
}

fn fbm(pos: vec3<f32>) -> f32 {
    var value = 0.0;
    var freq = params.frequency;
    var amp = params.amplitude;

    // Offset by hashed seed for deterministic variation
    let p = pos + hash_seed(params.seed);

    for (var i = 0u; i < params.octaves; i++) {
        value += amp * snoise(p * freq);
        freq *= params.lacunarity;
        amp *= params.gain;
    }

    return value;
}

@compute @workgroup_size(16, 16)
fn main(@builtin(global_invocation_id) id: vec3<u32>) {
    let res = params.resolution;
    if (id.x >= res || id.y >= res) {
        return;
    }

    let uv = vec2<f32>(
        f32(id.x) / f32(res - 1u),
        f32(id.y) / f32(res - 1u)
    );

    let sphere_pos = cube_to_sphere(params.face, uv);
    let height = fbm(sphere_pos);

    let idx = id.y * res + id.x;
    heightmap[idx] = height;
}
