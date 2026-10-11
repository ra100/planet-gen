struct IceHeightParams {
    resolution: u32,
    height_scale: f32,
    tile_offset_x: u32,
    tile_offset_y: u32,
    full_resolution: u32,
    local_height: u32,
    pad1: u32,
    pad2: u32,
    ice: IceMapClimate,
}

@group(0) @binding(0) var<storage, read> heightmap: array<f32>;
@group(0) @binding(1) var<storage, read_write> output: array<f32>;
@group(0) @binding(2) var<uniform> params: IceHeightParams;

@compute @workgroup_size(16, 16)
fn main(@builtin(global_invocation_id) id: vec3<u32>) {
    if (id.x >= params.resolution || id.y >= params.local_height) { return; }
    let uv = vec2<f32>(f32(params.tile_offset_x + id.x), f32(params.tile_offset_y + id.y))
        / f32(params.full_resolution - 1u);
    let p = cube_to_sphere(params.ice.face, uv);
    let index = id.y * params.resolution + id.x;
    let c = params.ice;
    output[index] = climate_frozen_height(p, heightmap[index], c.ocean_level,
        c.base_temp_c, c.axial_tilt_rad, c.season, c.seed, c.ocean_fraction);
}
