// Cube-to-sphere mapping for 6 cube faces.
// Face indices match wgpu/WebGPU cubemap layer ordering.
// UV coordinates are in [0, 1] range, mapped to [-1, 1] on the cube face.
//
// Standard cubemap convention (OpenGL/Vulkan/WebGPU):
//   layer 0 (+X): ( 1, -t, -s)
//   layer 1 (-X): (-1, -t,  s)
//   layer 2 (+Y): ( s,  1,  t)
//   layer 3 (-Y): ( s, -1, -t)
//   layer 4 (+Z): ( s, -t,  1)
//   layer 5 (-Z): (-s, -t, -1)

fn cube_to_sphere(face: u32, uv: vec2<f32>) -> vec3<f32> {
    let s = uv.x * 2.0 - 1.0;
    let t = uv.y * 2.0 - 1.0;

    var p: vec3<f32>;
    switch (face) {
        case 0u: { p = vec3<f32>( 1.0,   -t,   -s); } // +X
        case 1u: { p = vec3<f32>(-1.0,   -t,    s); } // -X
        case 2u: { p = vec3<f32>(   s,  1.0,    t); } // +Y
        case 3u: { p = vec3<f32>(   s, -1.0,   -t); } // -Y
        case 4u: { p = vec3<f32>(   s,   -t,  1.0); } // +Z
        case 5u: { p = vec3<f32>(  -s,   -t, -1.0); } // -Z
        default: { p = vec3<f32>(0.0, 0.0, 1.0); }
    }

    return normalize(p);
}
