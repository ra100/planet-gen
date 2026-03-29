use crate::gpu::GpuContext;
use bytemuck::{Pod, Zeroable};
use wgpu::util::DeviceExt;

#[repr(C)]
#[derive(Copy, Clone, Pod, Zeroable)]
pub struct TerrainParams {
    pub face: u32,
    pub resolution: u32,
    pub octaves: u32,
    pub seed: u32,
    pub frequency: f32,
    pub lacunarity: f32,
    pub gain: f32,
    pub amplitude: f32,
}

impl Default for TerrainParams {
    fn default() -> Self {
        Self {
            face: 0,
            resolution: 256,
            octaves: 8,
            seed: 42,
            frequency: 1.0,
            lacunarity: 2.0,
            gain: 0.5,
            amplitude: 1.0,
        }
    }
}

/// Generated heightmap data for all 6 cube faces.
pub struct TerrainData {
    /// Heightmap per face, each Vec<f32> has resolution² elements.
    pub faces: [Vec<f32>; 6],
    pub resolution: u32,
}

/// Generate terrain heightmaps for all 6 cube faces.
pub fn generate_terrain(gpu: &GpuContext, params: &TerrainParams) -> TerrainData {
    let shader_source = format!(
        "{}\n{}\n{}",
        include_str!("shaders/cube_sphere.wgsl"),
        include_str!("shaders/noise.wgsl"),
        include_str!("shaders/terrain.wgsl"),
    );

    let shader = gpu
        .device
        .create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("terrain shader"),
            source: wgpu::ShaderSource::Wgsl(shader_source.into()),
        });

    let res = params.resolution;
    let total = (res * res) as usize;
    let buffer_size = (total * std::mem::size_of::<f32>()) as u64;

    let output_buffer = gpu.device.create_buffer(&wgpu::BufferDescriptor {
        label: Some("terrain output"),
        size: buffer_size,
        usage: wgpu::BufferUsages::STORAGE | wgpu::BufferUsages::COPY_SRC,
        mapped_at_creation: false,
    });

    let staging_buffer = gpu.device.create_buffer(&wgpu::BufferDescriptor {
        label: Some("terrain staging"),
        size: buffer_size,
        usage: wgpu::BufferUsages::MAP_READ | wgpu::BufferUsages::COPY_DST,
        mapped_at_creation: false,
    });

    let bind_group_layout =
        gpu.device
            .create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
                label: Some("terrain bgl"),
                entries: &[
                    wgpu::BindGroupLayoutEntry {
                        binding: 0,
                        visibility: wgpu::ShaderStages::COMPUTE,
                        ty: wgpu::BindingType::Buffer {
                            ty: wgpu::BufferBindingType::Storage { read_only: false },
                            has_dynamic_offset: false,
                            min_binding_size: None,
                        },
                        count: None,
                    },
                    wgpu::BindGroupLayoutEntry {
                        binding: 1,
                        visibility: wgpu::ShaderStages::COMPUTE,
                        ty: wgpu::BindingType::Buffer {
                            ty: wgpu::BufferBindingType::Uniform,
                            has_dynamic_offset: false,
                            min_binding_size: None,
                        },
                        count: None,
                    },
                ],
            });

    let pipeline_layout = gpu
        .device
        .create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("terrain pipeline layout"),
            bind_group_layouts: &[&bind_group_layout],
            push_constant_ranges: &[],
        });

    let pipeline = gpu
        .device
        .create_compute_pipeline(&wgpu::ComputePipelineDescriptor {
            label: Some("terrain pipeline"),
            layout: Some(&pipeline_layout),
            module: &shader,
            entry_point: Some("main"),
            compilation_options: Default::default(),
            cache: None,
        });

    let mut faces: [Vec<f32>; 6] = Default::default();

    for face_idx in 0..6u32 {
        let mut face_params = *params;
        face_params.face = face_idx;

        let uniform_buffer =
            gpu.device
                .create_buffer_init(&wgpu::util::BufferInitDescriptor {
                    label: Some("terrain params"),
                    contents: bytemuck::bytes_of(&face_params),
                    usage: wgpu::BufferUsages::UNIFORM,
                });

        let bind_group = gpu.device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("terrain bind group"),
            layout: &bind_group_layout,
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: output_buffer.as_entire_binding(),
                },
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: uniform_buffer.as_entire_binding(),
                },
            ],
        });

        let mut encoder = gpu
            .device
            .create_command_encoder(&wgpu::CommandEncoderDescriptor {
                label: Some("terrain encoder"),
            });

        {
            let mut pass = encoder.begin_compute_pass(&wgpu::ComputePassDescriptor {
                label: Some("terrain pass"),
                timestamp_writes: None,
            });
            pass.set_pipeline(&pipeline);
            pass.set_bind_group(0, &bind_group, &[]);
            pass.dispatch_workgroups((res + 15) / 16, (res + 15) / 16, 1);
        }

        encoder.copy_buffer_to_buffer(&output_buffer, 0, &staging_buffer, 0, buffer_size);
        gpu.queue.submit(Some(encoder.finish()));

        staging_buffer
            .slice(..)
            .map_async(wgpu::MapMode::Read, |_| {});
        let _ = gpu.device.poll(wgpu::PollType::Wait);

        let mapped = staging_buffer.slice(..).get_mapped_range();
        faces[face_idx as usize] = bytemuck::cast_slice(&mapped).to_vec();
        drop(mapped);
        staging_buffer.unmap();
    }

    TerrainData {
        faces,
        resolution: res,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::gpu::GpuContext;

    #[test]
    fn test_terrain_generates_six_faces() {
        let gpu = GpuContext::new().expect("GPU init failed");
        let params = TerrainParams {
            resolution: 64,
            ..Default::default()
        };

        let terrain = generate_terrain(&gpu, &params);

        assert_eq!(terrain.faces.len(), 6);
        for (i, face) in terrain.faces.iter().enumerate() {
            assert_eq!(face.len(), 64 * 64, "face {i} should have 64x64 values");
            assert!(
                face.iter().all(|v| !v.is_nan()),
                "face {i} should have no NaN"
            );
        }
    }

    #[test]
    fn test_terrain_has_variation() {
        let gpu = GpuContext::new().expect("GPU init failed");
        let params = TerrainParams {
            resolution: 64,
            ..Default::default()
        };

        let terrain = generate_terrain(&gpu, &params);

        for (i, face) in terrain.faces.iter().enumerate() {
            let min = face.iter().cloned().fold(f32::INFINITY, f32::min);
            let max = face.iter().cloned().fold(f32::NEG_INFINITY, f32::max);
            let range = max - min;
            assert!(
                range > 0.01,
                "face {i} should show variation (range={range})"
            );
        }
    }

    #[test]
    fn test_terrain_edge_continuity() {
        let gpu = GpuContext::new().expect("GPU init failed");
        let res = 64u32;
        let params = TerrainParams {
            resolution: res,
            ..Default::default()
        };

        let terrain = generate_terrain(&gpu, &params);

        // Check continuity between +X (face 0) and +Z (face 4) at their shared edge.
        // +X face: u=0 edge → cube point (1, t, 1)
        // +Z face: u=1 edge → cube point (1, t, 1)
        // They should produce similar heightmap values along the shared edge.
        let face_px = &terrain.faces[0]; // +X
        let face_pz = &terrain.faces[4]; // +Z

        let mut max_diff = 0.0f32;
        for y in 0..res {
            let val_px = face_px[(y * res + 0) as usize]; // u=0 column of +X
            let val_pz = face_pz[(y * res + (res - 1)) as usize]; // u=res-1 column of +Z
            let diff = (val_px - val_pz).abs();
            max_diff = max_diff.max(diff);
        }

        assert!(
            max_diff < 0.1,
            "shared edge between +X and +Z should be continuous (max_diff={max_diff})"
        );
    }

    #[test]
    fn test_different_seeds_produce_different_terrain() {
        let gpu = GpuContext::new().expect("GPU init failed");

        let terrain_a = generate_terrain(
            &gpu,
            &TerrainParams {
                resolution: 32,
                seed: 1,
                ..Default::default()
            },
        );
        let terrain_b = generate_terrain(
            &gpu,
            &TerrainParams {
                resolution: 32,
                seed: 999,
                ..Default::default()
            },
        );

        // Compare face 0 — different seeds should produce different heightmaps
        let diff: f32 = terrain_a.faces[0]
            .iter()
            .zip(terrain_b.faces[0].iter())
            .map(|(a, b)| (a - b).abs())
            .sum::<f32>()
            / terrain_a.faces[0].len() as f32;

        assert!(
            diff > 0.01,
            "different seeds should produce different terrain (avg_diff={diff})"
        );
    }
}
