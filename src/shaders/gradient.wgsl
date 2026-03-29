@group(0) @binding(0) var output: texture_storage_2d<rgba8unorm, write>;

@compute @workgroup_size(16, 16)
fn main(@builtin(global_invocation_id) id: vec3<u32>) {
    let dims = textureDimensions(output);
    if (id.x >= dims.x || id.y >= dims.y) {
        return;
    }

    let u = f32(id.x) / f32(dims.x - 1u);
    let v = f32(id.y) / f32(dims.y - 1u);

    let color = vec4<f32>(u, v, 0.5, 1.0);
    textureStore(output, vec2<u32>(id.x, id.y), color);
}
