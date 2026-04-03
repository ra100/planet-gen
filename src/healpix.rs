//! HEALPix nested-scheme spherical pixelization.
//!
//! Implements the Hierarchical Equal Area isoLatitude Pixelization (Gorski et al. 2005)
//! using the NESTED indexing scheme for cache-friendly spatial locality.
//!
//! Key properties:
//! - 12 base pixels, each subdivided into nside×nside sub-pixels
//! - Total pixels: npix = 12 * nside²
//! - Equal area per pixel
//! - nside must be a power of 2 for nested scheme

use std::f64::consts::PI;

// ─── Face geometry lookup tables (Gorski et al. 2005, Table 2) ────────────────
//
// jrll[f]: ring-number offset for face f (multiply by nside, subtract ix+iy+1)
// jpll[f]: phi offset for face f
const JRLL: [i32; 12] = [2, 2, 2, 2, 3, 3, 3, 3, 4, 4, 4, 4];
const JPLL: [i32; 12] = [1, 3, 5, 7, 0, 2, 4, 6, 1, 3, 5, 7];

// ─── Z-order (Morton) curve helpers ───────────────────────────────────────────

/// Spread bits of x into even bit positions: 0b1011 → 0b01_00_01_01
fn spread_bits(mut v: u32) -> u32 {
    v &= 0x0000_FFFF;
    v = (v | (v << 8)) & 0x00FF_00FF;
    v = (v | (v << 4)) & 0x0F0F_0F0F;
    v = (v | (v << 2)) & 0x3333_3333;
    v = (v | (v << 1)) & 0x5555_5555;
    v
}

/// Extract bits from even bit positions.
fn compact_bits(mut v: u32) -> u32 {
    v &= 0x5555_5555;
    v = (v | (v >> 1)) & 0x3333_3333;
    v = (v | (v >> 2)) & 0x0F0F_0F0F;
    v = (v | (v >> 4)) & 0x00FF_00FF;
    v = (v | (v >> 8)) & 0x0000_FFFF;
    v
}

/// Encode (x, y) to Z-order index.
fn xy2z(x: u32, y: u32) -> u32 {
    spread_bits(x) | (spread_bits(y) << 1)
}

/// Decode Z-order index to (x, y).
fn z2xy(z: u32) -> (u32, u32) {
    (compact_bits(z), compact_bits(z >> 1))
}

// ─── Nested index ↔ (face, ix, iy) ───────────────────────────────────────────

/// Convert nested pixel index to (ix, iy, face).
fn nest2xyf(nside: u32, ipix: u32) -> (u32, u32, u32) {
    let npface = nside * nside;
    let face = ipix / npface;
    let (ix, iy) = z2xy(ipix % npface);
    (ix, iy, face)
}

/// Convert (ix, iy, face) to nested pixel index.
fn xyf2nest(nside: u32, ix: u32, iy: u32, face: u32) -> u32 {
    face * nside * nside + xy2z(ix, iy)
}

// ─── Core coordinate conversions ──────────────────────────────────────────────

/// Convert (face, ix, iy) to (z, phi) using jrll/jpll tables.
/// z = cos(theta), phi ∈ [0, 2π)
/// Accepts signed coordinates for neighbor extrapolation across face boundaries.
fn xyf2zphi(nside: u32, ix: i64, iy: i64, face: u32) -> (f64, f64) {
    let ns = nside as i64;
    let f = face as usize;

    // Ring number counted from north pole (1-indexed)
    let jr = (JRLL[f] as i64) * ns - ix - iy - 1;

    let z: f64;
    let nr: i64;
    let kshift: i64;

    if jr < 1 {
        // Beyond north pole — clamp
        nr = 1;
        z = 1.0 - 1.0 / (3.0 * (ns * ns) as f64);
        kshift = 0;
    } else if jr < ns {
        // North polar cap
        nr = jr;
        z = 1.0 - (jr * jr) as f64 / (3.0 * (ns * ns) as f64);
        kshift = 0;
    } else if jr <= 3 * ns {
        // Equatorial belt
        nr = ns;
        z = (2 * ns - jr) as f64 * 2.0 / (3.0 * ns as f64);
        kshift = (jr - ns) & 1;
    } else if jr < 4 * ns {
        // South polar cap
        nr = 4 * ns - jr;
        z = -1.0 + (nr * nr) as f64 / (3.0 * (ns * ns) as f64);
        kshift = 0;
    } else {
        // Beyond south pole — clamp
        nr = 1;
        z = -1.0 + 1.0 / (3.0 * (ns * ns) as f64);
        kshift = 0;
    }

    // Pixel-in-ring
    let mut jp = ((JPLL[f] as i64) * nr + ix - iy + 1 + kshift) / 2;
    if jp > 4 * nr {
        jp -= 4 * nr;
    }
    if jp < 1 {
        jp += 4 * nr;
    }

    let phi = (jp as f64 - (kshift + 1) as f64 * 0.5) * PI / (2.0 * nr as f64);
    let phi = phi.rem_euclid(2.0 * PI);

    (z.clamp(-1.0, 1.0), phi)
}

/// Convert (z, phi) to (ix, iy, face). z = cos(theta).
fn zphi2xyf(nside: u32, z: f64, phi: f64) -> (u32, u32, u32) {
    let ns = nside as i64;
    let za = z.abs();
    let phi_pos = phi.rem_euclid(2.0 * PI);
    let tt = phi_pos * 2.0 / PI; // ∈ [0, 4)

    if za <= 2.0 / 3.0 + 1e-14 {
        // Equatorial belt
        let temp1 = ns as f64 * (0.5 + tt);
        let temp2 = ns as f64 * z * 0.75;

        let jp = (temp1 - temp2) as i64; // ascending
        let jm = (temp1 + temp2) as i64; // descending

        let ifp = jp / ns; // in [0, 4] (can be 4 at phi ≈ 2π)
        let ifm = jm / ns;

        // Handle wrap-around: ifp or ifm can be 4 when phi ≈ 2π
        let face = if ifp == ifm {
            ((ifp & 3) + 4) as u32
        } else if ifp < ifm {
            (ifp & 3) as u32
        } else {
            ((ifm & 3) + 8) as u32
        };

        let ix = (jm & (ns - 1)) as u32;
        let iy = (ns - 1 - (jp & (ns - 1))) as u32;

        (ix, iy, face)
    } else {
        // Polar cap
        let ntt = (tt.floor() as i64).min(3);
        let tp = tt - ntt as f64;
        let tmp = ns as f64 * (3.0 * (1.0 - za)).sqrt();

        let mut jp = (tp * tmp) as i64;
        let mut jm = ((1.0 - tp) * tmp) as i64;

        jp = jp.min(ns - 1);
        jm = jm.min(ns - 1);

        if z >= 0.0 {
            // North polar cap
            let face = ntt as u32;
            let ix = (ns - jm - 1) as u32;
            let iy = (ns - jp - 1) as u32;
            (ix, iy, face)
        } else {
            // South polar cap
            let face = (ntt + 8) as u32;
            let ix = jp as u32;
            let iy = jm as u32;
            (ix, iy, face)
        }
    }
}

// ─── Public API ───────────────────────────────────────────────────────────────

/// Total number of pixels for a given nside.
pub fn npix(nside: u32) -> u32 {
    12 * nside * nside
}

/// Convert nested pixel index to 3D unit vector (x, y, z).
pub fn pix2vec(nside: u32, ipix: u32) -> [f64; 3] {
    let (ix, iy, face) = nest2xyf(nside, ipix);
    let (z, phi) = xyf2zphi(nside, ix as i64, iy as i64, face);
    let sin_theta = (1.0 - z * z).max(0.0).sqrt();
    [sin_theta * phi.cos(), sin_theta * phi.sin(), z]
}

/// Convert 3D unit vector to nested pixel index.
pub fn vec2pix(nside: u32, v: &[f64; 3]) -> u32 {
    let len = (v[0] * v[0] + v[1] * v[1] + v[2] * v[2]).sqrt();
    let z = v[2] / len;
    let phi = v[1].atan2(v[0]);
    let (ix, iy, face) = zphi2xyf(nside, z, phi);
    xyf2nest(nside, ix, iy, face)
}

/// 8 neighbors of a pixel in order: SW, W, NW, N, NE, E, SE, S.
/// For edge/corner pixels that cross face boundaries, the neighbor is found
/// by extrapolating face coordinates and mapping back via sphere lookup.
/// Returns u32::MAX for invalid neighbors (rare, only at nside=1 polar vertices).
pub fn neighbors(nside: u32, ipix: u32) -> [u32; 8] {
    let (ix, iy, face) = nest2xyf(nside, ipix);
    let ix = ix as i32;
    let iy = iy as i32;
    let ns = nside as i32;

    // Direction offsets: SW, W, NW, N, NE, E, SE, S
    let dx: [i32; 8] = [-1, -1, -1, 0, 1, 1, 1, 0];
    let dy: [i32; 8] = [-1, 0, 1, 1, 1, 0, -1, -1];

    let mut result = [u32::MAX; 8];

    for i in 0..8 {
        let nx = ix + dx[i];
        let ny = iy + dy[i];

        if nx >= 0 && nx < ns && ny >= 0 && ny < ns {
            // Interior pixel — direct indexing
            result[i] = xyf2nest(nside, nx as u32, ny as u32, face);
        } else {
            // Edge/corner — extrapolate using signed face coordinates.
            // The jrll/jpll formulas naturally extend beyond face boundaries,
            // giving valid (z, phi) on the adjacent face's territory.
            let (z, phi) = xyf2zphi(nside, nx as i64, ny as i64, face);
            let sin_theta = (1.0 - z * z).max(0.0).sqrt();
            let v = [sin_theta * phi.cos(), sin_theta * phi.sin(), z];
            let nbr = vec2pix(nside, &v);
            if nbr != ipix {
                result[i] = nbr;
            }
        }
    }

    result
}

/// Resample a HEALPix buffer onto a 6-face cubemap with inverse-distance interpolation.
///
/// `data`: HEALPix pixel values indexed by nested pixel index (length = 12*nside²).
/// `nside`: HEALPix resolution parameter.
/// `face_res`: Resolution of each cubemap face (face_res × face_res pixels).
///
/// Returns 6 face buffers in the same format as `TectonicTerrain::faces`.
/// Face indices: 0=+X, 1=-X, 2=+Y, 3=-Y, 4=+Z, 5=-Z
pub fn to_cubemap(data: &[f32], nside: u32, face_res: u32) -> [Vec<f32>; 6] {
    let mut faces: [Vec<f32>; 6] =
        std::array::from_fn(|_| vec![0.0; (face_res * face_res) as usize]);
    let pixel_radius = (4.0 * PI / (12.0 * (nside as f64).powi(2))).sqrt();

    for face in 0..6u32 {
        for py in 0..face_res {
            for px in 0..face_res {
                let u = (px as f64 + 0.5) / face_res as f64;
                let v = (py as f64 + 0.5) / face_res as f64;
                let dir = cube_to_sphere_f64(face, u, v);

                let center_pix = vec2pix(nside, &dir);
                let center_vec = pix2vec(nside, center_pix);
                let center_val = data[center_pix as usize] as f64;

                // Inverse-distance weighted interpolation with center + neighbors
                let nbrs = neighbors(nside, center_pix);
                let mut weighted_sum = 0.0;
                let mut weight_total = 0.0;
                let min_dist = 1e-12;

                // Center pixel
                let center_dist = angular_dist(&dir, &center_vec).max(min_dist);
                let center_w = 1.0 / center_dist;
                weighted_sum += center_w * center_val;
                weight_total += center_w;

                for &nbr in &nbrs {
                    if nbr == u32::MAX || nbr as usize >= data.len() {
                        continue;
                    }
                    let nbr_vec = pix2vec(nside, nbr);
                    let dist = angular_dist(&dir, &nbr_vec).max(min_dist);
                    if dist < 3.0 * pixel_radius {
                        let w = 1.0 / dist;
                        weighted_sum += w * data[nbr as usize] as f64;
                        weight_total += w;
                    }
                }

                let value = if weight_total > 0.0 {
                    (weighted_sum / weight_total) as f32
                } else {
                    data[center_pix as usize]
                };

                faces[face as usize][(py * face_res + px) as usize] = value;
            }
        }
    }

    faces
}

// ─── Helpers ──────────────────────────────────────────────────────────────────

fn dot(a: &[f64; 3], b: &[f64; 3]) -> f64 {
    a[0] * b[0] + a[1] * b[1] + a[2] * b[2]
}

fn angular_dist(a: &[f64; 3], b: &[f64; 3]) -> f64 {
    dot(a, b).clamp(-1.0, 1.0).acos()
}

/// Cube face to sphere direction (f64 version matching cube_sphere.rs conventions).
/// Face indices: 0=+X, 1=-X, 2=+Y, 3=-Y, 4=+Z, 5=-Z
fn cube_to_sphere_f64(face: u32, u: f64, v: f64) -> [f64; 3] {
    let s = u * 2.0 - 1.0;
    let t = v * 2.0 - 1.0;

    let p = match face {
        0 => [1.0, -t, -s],
        1 => [-1.0, -t, s],
        2 => [s, 1.0, t],
        3 => [s, -1.0, -t],
        4 => [s, -t, 1.0],
        5 => [-s, -t, -1.0],
        _ => [0.0, 0.0, 1.0],
    };

    let len = (p[0] * p[0] + p[1] * p[1] + p[2] * p[2]).sqrt();
    [p[0] / len, p[1] / len, p[2] / len]
}

#[cfg(test)]
mod tests {
    use super::*;

    fn vec_len(v: &[f64; 3]) -> f64 {
        (v[0] * v[0] + v[1] * v[1] + v[2] * v[2]).sqrt()
    }

    #[test]
    fn z_order_roundtrip() {
        for x in 0..16 {
            for y in 0..16 {
                let z = xy2z(x, y);
                let (rx, ry) = z2xy(z);
                assert_eq!((x, y), (rx, ry), "Z-order roundtrip failed for ({x},{y})");
            }
        }
    }

    #[test]
    fn nest2xyf_roundtrip() {
        for nside in [1, 2, 4, 8] {
            let np = npix(nside);
            for ipix in 0..np {
                let (x, y, f) = nest2xyf(nside, ipix);
                let recovered = xyf2nest(nside, x, y, f);
                assert_eq!(
                    ipix, recovered,
                    "nside={nside}: nest roundtrip failed for {ipix}"
                );
            }
        }
    }

    #[test]
    fn npix_formula() {
        assert_eq!(npix(1), 12);
        assert_eq!(npix(2), 48);
        assert_eq!(npix(4), 192);
        assert_eq!(npix(8), 768);
        assert_eq!(npix(16), 3072);
    }

    #[test]
    fn pix2vec_all_pixels_are_unit_vectors() {
        for nside in [1, 2, 4, 8] {
            let np = npix(nside);
            for ipix in 0..np {
                let v = pix2vec(nside, ipix);
                let len = vec_len(&v);
                assert!(
                    (len - 1.0).abs() < 1e-10,
                    "nside={nside} ipix={ipix}: length={len}"
                );
            }
        }
    }

    #[test]
    fn vec2pix_roundtrip_all_pixels() {
        for nside in [1, 2, 4, 8, 16] {
            let np = npix(nside);
            for ipix in 0..np {
                let v = pix2vec(nside, ipix);
                let recovered = vec2pix(nside, &v);
                assert_eq!(
                    ipix, recovered,
                    "nside={nside}: pix2vec→vec2pix roundtrip failed for ipix={ipix}"
                );
            }
        }
    }

    #[test]
    fn pix2vec_covers_full_sphere() {
        let nside = 8;
        let np = npix(nside);
        let mut min_z = f64::MAX;
        let mut max_z = f64::MIN;

        for ipix in 0..np {
            let v = pix2vec(nside, ipix);
            min_z = min_z.min(v[2]);
            max_z = max_z.max(v[2]);
        }

        assert!(
            max_z > 0.9,
            "Should have pixels near north pole, max_z={max_z}"
        );
        assert!(
            min_z < -0.9,
            "Should have pixels near south pole, min_z={min_z}"
        );
    }

    #[test]
    fn pix2vec_no_duplicate_positions() {
        let nside = 4;
        let np = npix(nside);
        let vecs: Vec<_> = (0..np).map(|i| pix2vec(nside, i)).collect();

        for i in 0..np as usize {
            for j in (i + 1)..np as usize {
                let d = dot(&vecs[i], &vecs[j]);
                assert!(
                    d < 0.9999,
                    "Pixels {i} and {j} are nearly identical: dot={d}"
                );
            }
        }
    }

    #[test]
    fn pix2vec_equal_area_approximate() {
        // HEALPix is iso-latitude: pixels in the same ring share the same z.
        // Check that UNIQUE z-values are well-distributed across [-1, 1].
        let nside = 16;
        let np = npix(nside);
        let mut z_vals: Vec<f64> = (0..np).map(|i| pix2vec(nside, i)[2]).collect();
        z_vals.sort_by(|a, b| a.partial_cmp(b).unwrap());
        z_vals.dedup_by(|a, b| (*a - *b).abs() < 1e-12);

        // nside=16 has 4*nside - 1 = 63 unique rings
        let num_rings = z_vals.len();
        assert!(
            num_rings >= 4 * nside as usize - 2,
            "Expected ~{} rings, got {num_rings}",
            4 * nside - 1
        );

        let expected_gap = 2.0 / num_rings as f64;
        let mut max_gap = 0.0_f64;
        for w in z_vals.windows(2) {
            max_gap = max_gap.max(w[1] - w[0]);
        }
        assert!(
            max_gap < 3.0 * expected_gap,
            "z-gap too large: {max_gap} vs expected ~{expected_gap}"
        );
    }

    #[test]
    fn neighbors_interior_pixel_has_8_valid() {
        let nside = 8;
        // Pick a pixel well inside a face (face 4, x=4, y=4)
        let ipix = xyf2nest(nside, 4, 4, 4);
        let nbrs = neighbors(nside, ipix);
        for (i, &n) in nbrs.iter().enumerate() {
            assert_ne!(n, u32::MAX, "Interior neighbor {i} should be valid");
            assert!(n < npix(nside), "Neighbor {i} out of range: {n}");
        }
        // All 8 should be distinct
        let mut sorted = nbrs;
        sorted.sort();
        for i in 0..7 {
            assert_ne!(sorted[i], sorted[i + 1], "Duplicate neighbor found");
        }
    }

    #[test]
    fn neighbors_are_spatially_close() {
        let nside = 16;
        let np = npix(nside);
        let pixel_radius = (4.0 * PI / (12.0 * (nside as f64).powi(2))).sqrt();

        for ipix in (0..np).step_by(17) {
            let v = pix2vec(nside, ipix);
            let nbrs = neighbors(nside, ipix);
            for (i, &n) in nbrs.iter().enumerate() {
                if n == u32::MAX || n >= np {
                    continue;
                }
                let nv = pix2vec(nside, n);
                let d = angular_dist(&v, &nv);
                assert!(
                    d < 4.0 * pixel_radius,
                    "nside={nside} ipix={ipix}: neighbor {i} (pix {n}) too far: {d:.4} > {:.4}",
                    4.0 * pixel_radius
                );
            }
        }
    }

    #[test]
    fn neighbors_edge_pixel_crosses_faces() {
        let nside = 4;
        // Pixel at edge of face 0: ix=0, iy=2
        let ipix = xyf2nest(nside, 0, 2, 0);
        let nbrs = neighbors(nside, ipix);
        let (_, _, own_face) = nest2xyf(nside, ipix);

        let mut found_other_face = false;
        for &n in &nbrs {
            if n != u32::MAX && n < npix(nside) {
                let (_, _, f) = nest2xyf(nside, n);
                if f != own_face {
                    found_other_face = true;
                    break;
                }
            }
        }
        assert!(
            found_other_face,
            "Edge pixel should have neighbors on other faces"
        );
    }

    #[test]
    fn to_cubemap_uniform_value() {
        let nside = 4;
        let np = npix(nside);
        let data: Vec<f32> = vec![0.5; np as usize];
        let face_res = 8;
        let faces = to_cubemap(&data, nside, face_res);

        for (fi, face) in faces.iter().enumerate() {
            assert_eq!(face.len(), (face_res * face_res) as usize);
            for (pi, &val) in face.iter().enumerate() {
                assert!(
                    (val - 0.5).abs() < 0.01,
                    "Face {fi} pixel {pi}: expected ~0.5, got {val}"
                );
            }
        }
    }

    #[test]
    fn to_cubemap_preserves_gradient() {
        // HEALPix buffer where value = z coordinate (latitude gradient)
        let nside = 16;
        let np = npix(nside);
        let data: Vec<f32> = (0..np)
            .map(|i| {
                let v = pix2vec(nside, i);
                v[2] as f32
            })
            .collect();

        let face_res = 32;
        let faces = to_cubemap(&data, nside, face_res);

        // +Z face (face 4) center points to [0,0,1] = north pole, z ≈ 1
        let center_idx = (face_res / 2 * face_res + face_res / 2) as usize;
        assert!(
            faces[4][center_idx] > 0.5,
            "+Z face center should be positive (north), got {}",
            faces[4][center_idx]
        );

        // -Z face (face 5) center points to [0,0,-1] = south pole, z ≈ -1
        assert!(
            faces[5][center_idx] < -0.5,
            "-Z face center should be negative (south), got {}",
            faces[5][center_idx]
        );
    }

    #[test]
    fn to_cubemap_no_face_boundary_seams() {
        // z-gradient: check adjacent face edge pixels are close in value
        let nside = 16;
        let np = npix(nside);
        let data: Vec<f32> = (0..np).map(|i| pix2vec(nside, i)[2] as f32).collect();

        let face_res = 16;
        let faces = to_cubemap(&data, nside, face_res);

        // Check +X (face 0) u=0 edge vs +Z (face 4) u=1 edge
        let mut max_diff: f32 = 0.0;
        for py in 0..face_res {
            let val_x = faces[0][(py * face_res) as usize];
            let val_z = faces[4][(py * face_res + face_res - 1) as usize];
            max_diff = max_diff.max((val_x - val_z).abs());
        }

        assert!(
            max_diff < 0.3,
            "Face boundary seam too large: max_diff={max_diff}"
        );
    }
}
