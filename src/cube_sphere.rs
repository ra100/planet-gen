/// CPU-side cube-to-sphere mapping, mirroring the WGSL implementation.
/// Face indices: 0=+X, 1=-X, 2=+Y, 3=-Y, 4=+Z, 5=-Z
pub fn cube_to_sphere(face: u32, u: f32, v: f32) -> [f32; 3] {
    let s = u * 2.0 - 1.0;
    let t = v * 2.0 - 1.0;

    let p = match face {
        0 => [1.0, t, -s],  // +X
        1 => [-1.0, t, s],  // -X
        2 => [s, 1.0, -t],  // +Y
        3 => [s, -1.0, t],  // -Y
        4 => [s, t, 1.0],   // +Z
        5 => [-s, t, -1.0], // -Z
        _ => [0.0, 0.0, 1.0],
    };

    let len = (p[0] * p[0] + p[1] * p[1] + p[2] * p[2]).sqrt();
    [p[0] / len, p[1] / len, p[2] / len]
}

#[cfg(test)]
mod tests {
    use super::*;

    fn approx_eq(a: [f32; 3], b: [f32; 3], eps: f32) -> bool {
        (a[0] - b[0]).abs() < eps && (a[1] - b[1]).abs() < eps && (a[2] - b[2]).abs() < eps
    }

    fn is_unit(v: [f32; 3], eps: f32) -> bool {
        let len = (v[0] * v[0] + v[1] * v[1] + v[2] * v[2]).sqrt();
        (len - 1.0).abs() < eps
    }

    #[test]
    fn face_centers_map_to_axis_directions() {
        // Center of each face (UV = 0.5, 0.5) should map to the face normal direction
        let eps = 1e-5;
        assert!(approx_eq(cube_to_sphere(0, 0.5, 0.5), [1.0, 0.0, 0.0], eps), "+X face center");
        assert!(approx_eq(cube_to_sphere(1, 0.5, 0.5), [-1.0, 0.0, 0.0], eps), "-X face center");
        assert!(approx_eq(cube_to_sphere(2, 0.5, 0.5), [0.0, 1.0, 0.0], eps), "+Y face center");
        assert!(approx_eq(cube_to_sphere(3, 0.5, 0.5), [0.0, -1.0, 0.0], eps), "-Y face center");
        assert!(approx_eq(cube_to_sphere(4, 0.5, 0.5), [0.0, 0.0, 1.0], eps), "+Z face center");
        assert!(approx_eq(cube_to_sphere(5, 0.5, 0.5), [0.0, 0.0, -1.0], eps), "-Z face center");
    }

    #[test]
    fn all_points_are_unit_length() {
        let eps = 1e-5;
        for face in 0..6 {
            for &u in &[0.0f32, 0.25, 0.5, 0.75, 1.0] {
                for &v in &[0.0f32, 0.25, 0.5, 0.75, 1.0] {
                    let p = cube_to_sphere(face, u, v);
                    assert!(is_unit(p, eps), "face={face} uv=({u},{v}) not unit: {:?}", p);
                    assert!(!p[0].is_nan() && !p[1].is_nan() && !p[2].is_nan(),
                        "NaN at face={face} uv=({u},{v})");
                }
            }
        }
    }

    #[test]
    fn adjacent_faces_share_edge_points() {
        // +X face right edge (u=0) should meet +Z face left edge (u=1) at same sphere point
        // +X: s = 0*2-1 = -1, so p = (1, t, 1) → normalized
        // +Z: s = 1*2-1 = 1, so p = (1, t, 1) → normalized
        let eps = 1e-5;
        for &v in &[0.0f32, 0.25, 0.5, 0.75, 1.0] {
            let p_xr = cube_to_sphere(0, 0.0, v); // +X face, u=0 edge
            let p_zl = cube_to_sphere(4, 1.0, v); // +Z face, u=1 edge
            assert!(
                approx_eq(p_xr, p_zl, eps),
                "+X(u=0) and +Z(u=1) should match at v={v}: {:?} vs {:?}",
                p_xr, p_zl
            );
        }
    }

    #[test]
    fn corners_are_not_nan() {
        // Corners of the cube are at maximum distortion — verify no NaN
        for face in 0..6 {
            for &u in &[0.0f32, 1.0] {
                for &v in &[0.0f32, 1.0] {
                    let p = cube_to_sphere(face, u, v);
                    assert!(
                        !p[0].is_nan() && !p[1].is_nan() && !p[2].is_nan(),
                        "NaN at corner face={face} uv=({u},{v})"
                    );
                }
            }
        }
    }
}
