// Cube-to-sphere mapping for 6 cube faces.
// Face indices: 0=+X, 1=-X, 2=+Y, 3=-Y, 4=+Z, 5=-Z
//
// UV coordinates are in [0, 1] range, mapped to [-1, 1] on the cube face.

fn cube_to_sphere(face: u32, uv: vec2<f32>) -> vec3<f32> {
    // Map UV from [0,1] to [-1,1]
    let s = uv.x * 2.0 - 1.0;
    let t = uv.y * 2.0 - 1.0;

    var p: vec3<f32>;
    switch (face) {
        case 0u: { p = vec3<f32>( 1.0,    t,   -s); } // +X
        case 1u: { p = vec3<f32>(-1.0,    t,    s); } // -X
        case 2u: { p = vec3<f32>(   s,  1.0,   -t); } // +Y
        case 3u: { p = vec3<f32>(   s, -1.0,    t); } // -Y
        case 4u: { p = vec3<f32>(   s,    t,  1.0); } // +Z
        case 5u: { p = vec3<f32>(  -s,    t, -1.0); } // -Z
        default: { p = vec3<f32>(0.0, 0.0, 1.0); }
    }

    return normalize(p);
}
