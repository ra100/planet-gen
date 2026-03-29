// Test shader: samples simplex noise at grid positions and writes to a storage buffer.
// Used to verify the noise function produces values in [-1, 1] and is non-uniform.

@group(0) @binding(0) var<storage, read_write> output: array<f32>;
@group(0) @binding(1) var<uniform> params: NoiseTestParams;

struct NoiseTestParams {
    width: u32,
    height: u32,
    scale: f32,
    _pad: u32,
}

@compute @workgroup_size(64)
fn main(@builtin(global_invocation_id) id: vec3<u32>) {
    let idx = id.x;
    let total = params.width * params.height;
    if (idx >= total) {
        return;
    }

    let x = idx % params.width;
    let y = idx / params.width;

    let pos = vec3<f32>(
        f32(x) / f32(params.width) * params.scale,
        f32(y) / f32(params.height) * params.scale,
        0.0
    );

    output[idx] = snoise(pos);
}
