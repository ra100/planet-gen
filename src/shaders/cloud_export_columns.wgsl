// Radial cloud columns on cube faces. Concatenate after cloud_export.wgsl.
@group(0) @binding(7) var<storage, read_write> output: array<CloudColumn>;

@compute @workgroup_size(8, 8)
fn main(@builtin(global_invocation_id) id: vec3<u32>) {
    if (id.x >= params.tile_width || id.y >= params.tile_height) { return; }
    let pixel = id.y * params.tile_width + id.x;
    let uv = vec2<f32>(
        f32(params.tile_offset_x + id.x) / f32(params.full_resolution - 1u),
        f32(params.tile_offset_y + id.y) / f32(params.full_resolution - 1u),
    );
    let direction = cube_to_sphere(params.face, uv);
    output[pixel] = cloud_column_sample(direction, 2.0 / f32(params.full_resolution));
}
