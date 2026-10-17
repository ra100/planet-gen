//! Broad terrain ambient occlusion.
//!
//! A smooth, hill-scale occlusion field computed on the CPU from low-passed
//! copies of the export heightmap. Each scale compares a ring of the terrain at
//! that scale's own resolution with the centre at the same resolution, so every
//! term is a band-pass and pixel-level bumps never reach the result. The field
//! is resolution independent: the same planet produces the same occlusion at
//! every export size, and it is bilinearly upsampled when baked.

use std::sync::atomic::{AtomicBool, Ordering};

use crate::export::{direction_to_face_uv, face_uv_to_direction};
use crate::terrain_compute::TectonicTerrain;

/// Face resolution of the occlusion field (about 0.18° per texel).
pub const BROAD_AO_RESOLUTION: u32 = 512;
/// Terrain level of each scale, as a divisor of the field resolution. Rings
/// sit at about 8, 16 and 32 field texels (roughly 1.5°–6° of arc), so the AO
/// follows whole hills and basins; bumps smaller than the finest level never
/// reach it.
const LEVEL_DIVISORS: [u32; 3] = [4, 8, 16];
const SCALE_WEIGHTS: [f32; 3] = [0.3, 0.4, 0.3];
/// Ring radius in texels of each scale's own level.
const RING_RADIUS_TEXELS: f32 = 2.0;
const RING_DIRECTIONS: usize = 16;
/// Height (terrain units) a ring must rise above the centre for that scale to
/// count as fully occluded. Wider rings span more relief, so they need more.
const OCCLUSION_DEPTH: [f32; 3] = [0.05, 0.08, 0.11];
/// AO value of a point fully enclosed at every scale.
const MIN_AO: f32 = 0.25;
const MIN_LEVEL_RESOLUTION: u32 = 4;

pub struct BroadAoField {
    resolution: u32,
    faces: [Vec<f32>; 6],
}

impl BroadAoField {
    /// Builds the field from the export terrain. Heights below `ocean_level`
    /// are flattened to it so the sea floor never occludes the coast.
    pub fn from_terrain(
        terrain: &TectonicTerrain,
        ocean_level: f32,
        cancel: &AtomicBool,
    ) -> Result<Self, String> {
        if terrain.resolution < 2 {
            return Err("broad AO requires terrain faces of at least 2 pixels".into());
        }
        let resolution = BROAD_AO_RESOLUTION
            .min(terrain.resolution)
            .max(MIN_LEVEL_RESOLUTION);
        let base = downsample(terrain, resolution, ocean_level);
        let mut levels = Vec::with_capacity(LEVEL_DIVISORS.len());
        for divisor in LEVEL_DIVISORS {
            if cancel.load(Ordering::Relaxed) {
                return Err("Cancelled while building broad AO".into());
            }
            let level_resolution = (resolution / divisor).max(MIN_LEVEL_RESOLUTION);
            levels.push(spherical_downsample(&base, level_resolution));
        }

        let faces = std::thread::scope(|scope| {
            let handles: Vec<_> = (0..6)
                .map(|face| {
                    let levels = &levels;
                    scope.spawn(move || occlusion_face(levels, face, resolution, cancel))
                })
                .collect();
            let mut faces: [Vec<f32>; 6] = Default::default();
            for (slot, handle) in faces.iter_mut().zip(handles) {
                *slot = handle
                    .join()
                    .map_err(|_| "broad AO worker panicked".to_string())??;
            }
            Ok::<_, String>(faces)
        })?;
        let faces = faces.map(|face| blur_face(&face, resolution as usize));
        Ok(Self { resolution, faces })
    }

    /// Bilinear sample at face-local `u`, `v` in `[0, 1]` (corner aligned, as
    /// the export tiles address pixels).
    pub fn sample(&self, face: usize, u: f32, v: f32) -> f32 {
        let resolution = self.resolution as usize;
        let x = u.clamp(0.0, 1.0) * (resolution - 1) as f32;
        let y = v.clamp(0.0, 1.0) * (resolution - 1) as f32;
        let x0 = (x as usize).min(resolution - 2);
        let y0 = (y as usize).min(resolution - 2);
        let tx = x - x0 as f32;
        let ty = y - y0 as f32;
        let values = &self.faces[face];
        let at = |x: usize, y: usize| values[y * resolution + x];
        let top = at(x0, y0) + (at(x0 + 1, y0) - at(x0, y0)) * tx;
        let bottom = at(x0, y0 + 1) + (at(x0 + 1, y0 + 1) - at(x0, y0 + 1)) * tx;
        top + (bottom - top) * ty
    }
}

/// Multiplier applied to albedo for a given AO value and bake strength.
pub fn albedo_bake_factor(ao: f32, strength: f32) -> f32 {
    1.0 - strength.clamp(0.0, 1.0) * (1.0 - ao.clamp(0.0, 1.0))
}

/// Box-filtered copy of the terrain at `resolution`, with the ocean flattened.
fn downsample(terrain: &TectonicTerrain, resolution: u32, ocean_level: f32) -> TectonicTerrain {
    let source = terrain.resolution as usize;
    let target = resolution as usize;
    let ratio = (source - 1) as f32 / (target - 1) as f32;
    let half = (ratio * 0.5).max(0.0);
    let spans: Vec<(usize, usize)> = (0..target)
        .map(|i| {
            let centre = i as f32 * ratio;
            let start = (centre - half).ceil().max(0.0) as usize;
            let end = ((centre + half).floor() as usize).min(source - 1);
            (start.min(end), end)
        })
        .collect();
    let faces = std::array::from_fn(|face| {
        let values = &terrain.faces[face];
        let mut out = Vec::with_capacity(target * target);
        for &(y0, y1) in &spans {
            for &(x0, x1) in &spans {
                let mut sum = 0.0;
                for y in y0..=y1 {
                    let row = &values[y * source..y * source + source];
                    for height in &row[x0..=x1] {
                        sum += height.max(ocean_level);
                    }
                }
                out.push(sum / ((y1 - y0 + 1) * (x1 - x0 + 1)) as f32);
            }
        }
        out
    });
    TectonicTerrain { faces, resolution }
}

/// Coarser level built by averaging `base` over each texel's footprint on the
/// sphere, so footprints cross cube-face edges instead of stopping at them.
fn spherical_downsample(base: &TectonicTerrain, resolution: u32) -> TectonicTerrain {
    let res = resolution as usize;
    let taps = (base.resolution - 1).div_ceil(resolution - 1).max(1) as i32;
    let step = 2.0 / (base.resolution - 1) as f32;
    let faces = std::array::from_fn(|face| {
        let mut out = Vec::with_capacity(res * res);
        for y in 0..res {
            for x in 0..res {
                let direction = normalize(face_uv_to_direction(
                    face,
                    x as f32 / (res - 1) as f32,
                    y as f32 / (res - 1) as f32,
                ));
                let (t1, t2) = tangent_basis(direction);
                let mut sum = 0.0;
                for j in 0..taps {
                    let b = (j as f32 - (taps - 1) as f32 * 0.5) * step;
                    for i in 0..taps {
                        let a = (i as f32 - (taps - 1) as f32 * 0.5) * step;
                        let sample = normalize(std::array::from_fn(|k| {
                            direction[k] + t1[k] * a + t2[k] * b
                        }));
                        sum += sample_height(base, sample);
                    }
                }
                out.push(sum / (taps * taps) as f32);
            }
        }
        out
    });
    TectonicTerrain { faces, resolution }
}

fn tangent_basis(direction: [f32; 3]) -> ([f32; 3], [f32; 3]) {
    let up = if direction[1].abs() < 0.99 {
        [0.0, 1.0, 0.0]
    } else {
        [1.0, 0.0, 0.0]
    };
    let t1 = normalize(cross(up, direction));
    (t1, cross(direction, t1))
}

fn sample_height(level: &TectonicTerrain, direction: [f32; 3]) -> f32 {
    let (face, u, v) = direction_to_face_uv(direction[0], direction[1], direction[2]);
    let resolution = level.resolution as usize;
    let x = u * (resolution - 1) as f32;
    let y = v * (resolution - 1) as f32;
    let x0 = (x as usize).min(resolution - 2);
    let y0 = (y as usize).min(resolution - 2);
    let tx = x - x0 as f32;
    let ty = y - y0 as f32;
    let values = &level.faces[face];
    let at = |x: usize, y: usize| values[y * resolution + x];
    let top = at(x0, y0) + (at(x0 + 1, y0) - at(x0, y0)) * tx;
    let bottom = at(x0, y0 + 1) + (at(x0 + 1, y0 + 1) - at(x0, y0 + 1)) * tx;
    top + (bottom - top) * ty
}

fn occlusion_face(
    levels: &[TectonicTerrain],
    face: usize,
    resolution: u32,
    cancel: &AtomicBool,
) -> Result<Vec<f32>, String> {
    let res = resolution as usize;
    let mut out = Vec::with_capacity(res * res);
    for y in 0..res {
        if cancel.load(Ordering::Relaxed) {
            return Err("Cancelled while building broad AO".into());
        }
        for x in 0..res {
            let direction = normalize(face_uv_to_direction(
                face,
                x as f32 / (res - 1) as f32,
                y as f32 / (res - 1) as f32,
            ));
            out.push(occlusion_at(levels, direction));
        }
    }
    Ok(out)
}

fn occlusion_at(levels: &[TectonicTerrain], direction: [f32; 3]) -> f32 {
    let (t1, t2) = tangent_basis(direction);
    let mut occlusion = 0.0;
    for (scale, (level, weight)) in levels.iter().zip(SCALE_WEIGHTS).enumerate() {
        // Texels near a face centre span 2/(res-1) in tangent units ≈ radians.
        let radius = RING_RADIUS_TEXELS * 2.0 / (level.resolution - 1) as f32;
        let (sin_r, cos_r) = radius.sin_cos();
        let offset = if scale % 2 == 0 { 0.0 } else { 0.5 };
        let mut ring = 0.0;
        for k in 0..RING_DIRECTIONS {
            let angle = (k as f32 + offset) / RING_DIRECTIONS as f32 * std::f32::consts::TAU;
            let (sin_a, cos_a) = angle.sin_cos();
            let sample = std::array::from_fn(|i| {
                direction[i] * cos_r + (t1[i] * cos_a + t2[i] * sin_a) * sin_r
            });
            ring += sample_height(level, sample);
        }
        let rise = ring / RING_DIRECTIONS as f32 - sample_height(level, direction);
        occlusion += weight * smoothstep(0.0, OCCLUSION_DEPTH[scale], rise);
    }
    1.0 - occlusion * (1.0 - MIN_AO)
}

/// One 3×3 binomial pass, clamped at the face edge.
fn blur_face(values: &[f32], res: usize) -> Vec<f32> {
    let weight = [1.0, 2.0, 1.0];
    let mut out = Vec::with_capacity(values.len());
    for y in 0..res {
        for x in 0..res {
            let mut sum = 0.0;
            for (dy, wy) in weight.iter().enumerate() {
                let sy = (y + dy).saturating_sub(1).min(res - 1);
                for (dx, wx) in weight.iter().enumerate() {
                    let sx = (x + dx).saturating_sub(1).min(res - 1);
                    sum += values[sy * res + sx] * wx * wy;
                }
            }
            out.push(sum / 16.0);
        }
    }
    out
}

fn smoothstep(edge0: f32, edge1: f32, x: f32) -> f32 {
    let t = ((x - edge0) / (edge1 - edge0)).clamp(0.0, 1.0);
    t * t * (3.0 - 2.0 * t)
}

fn cross(a: [f32; 3], b: [f32; 3]) -> [f32; 3] {
    [
        a[1] * b[2] - a[2] * b[1],
        a[2] * b[0] - a[0] * b[2],
        a[0] * b[1] - a[1] * b[0],
    ]
}

fn normalize(v: [f32; 3]) -> [f32; 3] {
    let length = (v[0] * v[0] + v[1] * v[1] + v[2] * v[2]).sqrt();
    [v[0] / length, v[1] / length, v[2] / length]
}

#[cfg(test)]
mod tests {
    use super::*;

    fn terrain(resolution: u32, height: impl Fn([f32; 3]) -> f32) -> TectonicTerrain {
        let res = resolution as usize;
        let faces = std::array::from_fn(|face| {
            (0..res * res)
                .map(|i| {
                    let u = (i % res) as f32 / (res - 1) as f32;
                    let v = (i / res) as f32 / (res - 1) as f32;
                    height(normalize(face_uv_to_direction(face, u, v)))
                })
                .collect()
        });
        TectonicTerrain { faces, resolution }
    }

    #[test]
    fn flat_terrain_is_unoccluded() {
        let field = BroadAoField::from_terrain(&terrain(64, |_| 0.4), 0.0, &AtomicBool::new(false))
            .unwrap();
        for face in 0..6 {
            assert!((field.sample(face, 0.3, 0.7) - 1.0).abs() < 1e-5);
        }
    }

    #[test]
    fn basin_is_darker_than_surrounding_hills() {
        // A broad bowl centred on +Z: deep at the pole, high on its rim.
        let bowl = |p: [f32; 3]| {
            let angle = p[2].clamp(-1.0, 1.0).acos();
            0.2 + 0.4 * smoothstep(0.0, 0.3, angle)
        };
        let field =
            BroadAoField::from_terrain(&terrain(256, bowl), -1.0, &AtomicBool::new(false)).unwrap();
        let floor = field.sample(4, 0.5, 0.5);
        let plain = field.sample(0, 0.5, 0.5);
        assert!(floor < 0.9, "basin floor should be occluded: {floor}");
        assert!((plain - 1.0).abs() < 1e-3, "open plain stays lit: {plain}");
    }

    #[test]
    fn pixel_noise_does_not_reach_the_field() {
        // High-frequency checkerboard bumps on a flat plain average out in the
        // low-passed levels, so the field stays nearly uniform.
        let res = 512usize;
        let mut flat = terrain(res as u32, |_| 0.3);
        for face in &mut flat.faces {
            for (i, height) in face.iter_mut().enumerate() {
                if (i % res + i / res).is_multiple_of(2) {
                    *height += 0.2;
                }
            }
        }
        let field = BroadAoField::from_terrain(&flat, 0.0, &AtomicBool::new(false)).unwrap();
        let samples: Vec<f32> = (0..32)
            .map(|i| field.sample(2, 0.2 + i as f32 * 0.019, 0.5))
            .collect();
        let min = samples.iter().copied().fold(f32::MAX, f32::min);
        let max = samples.iter().copied().fold(f32::MIN, f32::max);
        assert!(max - min < 0.02, "field should be smooth: {min}..{max}");
    }

    #[test]
    fn ocean_is_flattened_before_occlusion() {
        // A deep ocean basin below sea level does not darken the coast beside it.
        let basin = |p: [f32; 3]| if p[0] < 0.0 { -0.9 } else { 0.1 };
        let field =
            BroadAoField::from_terrain(&terrain(128, basin), 0.0, &AtomicBool::new(false)).unwrap();
        // +Z face, just on the land side of the x = 0 coastline.
        let coast = field.sample(4, 0.53, 0.5);
        assert!((coast - 1.0).abs() < 1e-3, "coast stays lit: {coast}");
    }

    #[test]
    fn bake_factor_scales_with_strength() {
        assert_eq!(albedo_bake_factor(0.4, 0.0), 1.0);
        assert!((albedo_bake_factor(0.4, 1.0) - 0.4).abs() < 1e-6);
        assert!((albedo_bake_factor(0.4, 0.5) - 0.7).abs() < 1e-6);
    }
}
