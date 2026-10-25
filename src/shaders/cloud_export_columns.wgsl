// Radial cloud columns on cube faces. Concatenate after cloud_export.wgsl and
// cloud_synthesis.wgsl.
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
    // The faces are resampled into a 2N x N equirectangular map whose pixels
    // span pi / N radians, up to 1.6x a face-centre texel and 3x a corner
    // one. Structure resolved at the texel would alias into speckle there.
    let footprint = 3.1415927 / f32(params.full_resolution);
    output[pixel] = cloud_column_sample_synthesized(direction, footprint);
}

// Equirectangular cloud columns, one pixel per invocation at the pixel centre,
// for inspecting the exported cloud without staging cube faces. Uses the
// driver band convention: tile_width = map width, full_resolution = map
// height, tile_offset_y = first row, tile_height = row count.
@compute @workgroup_size(8, 8)
fn equirect(@builtin(global_invocation_id) id: vec3<u32>) {
    if (id.x >= params.tile_width || id.y >= params.tile_height) { return; }
    let y = params.tile_offset_y + id.y;
    let lon = 6.2831853 * (f32(id.x) + 0.5) / f32(params.tile_width) - 3.1415927;
    let lat = 3.1415927 * (0.5 - (f32(y) + 0.5) / f32(params.full_resolution));
    let direction = vec3<f32>(cos(lat) * sin(lon), sin(lat), cos(lat) * cos(lon));
    let footprint = 6.2831853 / f32(params.tile_width);
    output[id.y * params.tile_width + id.x] = cloud_column_sample_synthesized(direction, footprint);
}
