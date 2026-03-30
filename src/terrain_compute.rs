use crate::gpu::GpuContext;
use crate::plates::PlateGpu;
use bytemuck::{Pod, Zeroable};
use wgpu::util::DeviceExt;

#[repr(C)]
#[derive(Copy, Clone, Pod, Zeroable)]
pub struct TerrainGenParams {
    pub face: u32,
    pub resolution: u32,
    pub num_plates: u32,
    pub seed: u32,
    pub amplitude: f32,
    pub frequency: f32,
    pub octaves: u32,
    pub gain: f32,
    pub lacunarity: f32,
    pub _pad0: f32,
    pub _pad1: f32,
    pub _pad2: f32,
}

/// Generated tectonic heightmap for all 6 cube faces.
pub struct TectonicTerrain {
    pub faces: [Vec<f32>; 6],
    pub resolution: u32,
}

pub struct TerrainComputePipeline {
    pipeline: wgpu::ComputePipeline,
    bind_group_layout: wgpu::BindGroupLayout,
}

impl TerrainComputePipeline {
    pub fn new(gpu: &GpuContext) -> Self {
        let shader_source = format!(
            "{}\n{}\n{}",
            include_str!("shaders/cube_sphere.wgsl"),
            include_str!("shaders/noise.wgsl"),
            include_str!("shaders/plates.wgsl"),
        );

        let shader = gpu
            .device
            .create_shader_module(wgpu::ShaderModuleDescriptor {
                label: Some("tectonic terrain shader"),
                source: wgpu::ShaderSource::Wgsl(shader_source.into()),
            });

        let bind_group_layout =
            gpu.device
                .create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
                    label: Some("terrain compute bgl"),
                    entries: &[
                        // Plates buffer (read-only)
                        wgpu::BindGroupLayoutEntry {
                            binding: 0,
                            visibility: wgpu::ShaderStages::COMPUTE,
                            ty: wgpu::BindingType::Buffer {
                                ty: wgpu::BufferBindingType::Storage { read_only: true },
                                has_dynamic_offset: false,
                                min_binding_size: None,
                            },
                            count: None,
                        },
                        // Params uniform
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
                        // Heightmap output (read-write)
                        wgpu::BindGroupLayoutEntry {
                            binding: 2,
                            visibility: wgpu::ShaderStages::COMPUTE,
                            ty: wgpu::BindingType::Buffer {
                                ty: wgpu::BufferBindingType::Storage { read_only: false },
                                has_dynamic_offset: false,
                                min_binding_size: None,
                            },
                            count: None,
                        },
                    ],
                });

        let pipeline_layout =
            gpu.device
                .create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
                    label: Some("terrain compute pipeline layout"),
                    bind_group_layouts: &[&bind_group_layout],
                    push_constant_ranges: &[],
                });

        let pipeline = gpu
            .device
            .create_compute_pipeline(&wgpu::ComputePipelineDescriptor {
                label: Some("terrain compute pipeline"),
                layout: Some(&pipeline_layout),
                module: &shader,
                entry_point: Some("main"),
                compilation_options: Default::default(),
                cache: None,
            });

        Self {
            pipeline,
            bind_group_layout,
        }
    }

    /// Generate tectonic terrain for all 6 cube faces.
    pub fn generate(
        &self,
        gpu: &GpuContext,
        plates: &[PlateGpu],
        resolution: u32,
        seed: u32,
        amplitude: f32,
        frequency: f32,
        octaves: u32,
        gain: f32,
        lacunarity: f32,
    ) -> TectonicTerrain {
        let total_pixels = (resolution * resolution) as usize;
        let buffer_size = (total_pixels * std::mem::size_of::<f32>()) as u64;

        // Upload plates buffer (shared across all faces)
        let plates_buffer =
            gpu.device
                .create_buffer_init(&wgpu::util::BufferInitDescriptor {
                    label: Some("plates buffer"),
                    contents: bytemuck::cast_slice(plates),
                    usage: wgpu::BufferUsages::STORAGE,
                });

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

        let mut faces: [Vec<f32>; 6] = Default::default();

        for face_idx in 0..6u32 {
            let params = TerrainGenParams {
                face: face_idx,
                resolution,
                num_plates: plates.len() as u32,
                seed,
                amplitude,
                frequency,
                octaves,
                gain,
                lacunarity,
                _pad0: 0.0,
                _pad1: 0.0,
                _pad2: 0.0,
            };

            let params_buffer =
                gpu.device
                    .create_buffer_init(&wgpu::util::BufferInitDescriptor {
                        label: Some("terrain params"),
                        contents: bytemuck::bytes_of(&params),
                        usage: wgpu::BufferUsages::UNIFORM,
                    });

            let bind_group = gpu.device.create_bind_group(&wgpu::BindGroupDescriptor {
                label: Some("terrain compute bind group"),
                layout: &self.bind_group_layout,
                entries: &[
                    wgpu::BindGroupEntry {
                        binding: 0,
                        resource: plates_buffer.as_entire_binding(),
                    },
                    wgpu::BindGroupEntry {
                        binding: 1,
                        resource: params_buffer.as_entire_binding(),
                    },
                    wgpu::BindGroupEntry {
                        binding: 2,
                        resource: output_buffer.as_entire_binding(),
                    },
                ],
            });

            let mut encoder = gpu
                .device
                .create_command_encoder(&wgpu::CommandEncoderDescriptor {
                    label: Some("terrain compute encoder"),
                });

            {
                let mut pass = encoder.begin_compute_pass(&wgpu::ComputePassDescriptor {
                    label: Some("terrain compute pass"),
                    timestamp_writes: None,
                });
                pass.set_pipeline(&self.pipeline);
                pass.set_bind_group(0, &bind_group, &[]);
                pass.dispatch_workgroups(
                    (resolution + 15) / 16,
                    (resolution + 15) / 16,
                    1,
                );
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

        TectonicTerrain { faces, resolution }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::gpu::GpuContext;
    use crate::plates::{generate_plates, PlateGenParams};

    #[test]
    fn test_tectonic_terrain_generates() {
        let gpu = GpuContext::new().expect("GPU init failed");
        let pipeline = TerrainComputePipeline::new(&gpu);

        let plates = generate_plates(&PlateGenParams {
            seed: 42,
            mass_earth: 1.0,
            ocean_fraction: 0.7,
            tectonics_factor: 0.85,
        });

        let terrain = pipeline.generate(
            &gpu, &plates, 64, 42, 1.0, 1.2, 8, 0.5, 2.0,
        );

        assert_eq!(terrain.faces.len(), 6);
        for (i, face) in terrain.faces.iter().enumerate() {
            assert_eq!(face.len(), 64 * 64, "face {i} wrong size");
            assert!(face.iter().all(|v| !v.is_nan()), "face {i} has NaN");
        }
    }

    #[test]
    fn test_tectonic_terrain_has_bimodal_distribution() {
        let gpu = GpuContext::new().expect("GPU init failed");
        let pipeline = TerrainComputePipeline::new(&gpu);

        let plates = generate_plates(&PlateGenParams {
            seed: 42,
            mass_earth: 1.0,
            ocean_fraction: 0.7,
            tectonics_factor: 0.85,
        });

        let terrain = pipeline.generate(
            &gpu, &plates, 64, 42, 1.0, 1.2, 8, 0.5, 2.0,
        );

        // Collect all heights
        let all_heights: Vec<f32> = terrain.faces.iter().flat_map(|f| f.iter().copied()).collect();

        // Count heights above 0 (land) vs below 0 (ocean)
        let above = all_heights.iter().filter(|&&h| h > 0.0).count();
        let below = all_heights.iter().filter(|&&h| h <= 0.0).count();
        let total = all_heights.len();

        let land_fraction = above as f32 / total as f32;

        // With 70% ocean fraction, expect roughly 30% land (±15% tolerance)
        assert!(
            land_fraction > 0.15 && land_fraction < 0.55,
            "Land fraction should be roughly 30%, got {:.1}% ({} above / {} total)",
            land_fraction * 100.0, above, total
        );
    }

    #[test]
    fn test_tectonic_terrain_has_mountains() {
        let gpu = GpuContext::new().expect("GPU init failed");
        let pipeline = TerrainComputePipeline::new(&gpu);

        let plates = generate_plates(&PlateGenParams {
            seed: 42,
            mass_earth: 1.0,
            ocean_fraction: 0.7,
            tectonics_factor: 0.85,
        });

        let terrain = pipeline.generate(
            &gpu, &plates, 64, 42, 1.0, 1.2, 8, 0.5, 2.0,
        );

        let all_heights: Vec<f32> = terrain.faces.iter().flat_map(|f| f.iter().copied()).collect();
        let max_height = all_heights.iter().cloned().fold(f32::NEG_INFINITY, f32::max);

        // Should have mountain peaks significantly above continental shelf (0.25)
        assert!(
            max_height > 0.4,
            "Should have mountain peaks > 0.4, max is {}",
            max_height
        );
    }
}
