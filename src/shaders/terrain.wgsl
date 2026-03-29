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

fn fbm(pos: vec3<f32>) -> f32 {
    var value = 0.0;
    var freq = params.frequency;
    var amp = params.amplitude;
    var p = pos;

    // Offset by seed to get different terrain per seed
    let seed_offset = vec3<f32>(
        f32(params.seed) * 13.37,
        f32(params.seed) * 7.19,
        f32(params.seed) * 23.71
    );
    p = p + seed_offset;

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
