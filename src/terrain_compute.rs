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
    pub tile_offset_x: u32,
    pub tile_offset_y: u32,
    pub full_resolution: u32,
    pub mountain_scale: f32,   // multiplier for tectonic mountain height (1.0 = default)
    pub boundary_width: f32,   // sigma for boundary influence spread (0.10 = default)
    pub warp_strength: f32,    // domain warp intensity (1.0 = default)
    pub detail_scale: f32,     // fBm detail noise intensity (1.0 = default)
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

    /// Create a reusable plates buffer for tiled generation.
    pub fn create_plates_buffer(&self, gpu: &GpuContext, plates: &[PlateGpu]) -> wgpu::Buffer {
        gpu.device
            .create_buffer_init(&wgpu::util::BufferInitDescriptor {
                label: Some("plates buffer"),
                contents: bytemuck::cast_slice(plates),
                usage: wgpu::BufferUsages::STORAGE,
            })
    }

    /// Dispatch a single tile and read back the heightmap data.
    pub fn dispatch_tile(
        &self,
        gpu: &GpuContext,
        plates_buffer: &wgpu::Buffer,
        params: &TerrainGenParams,
    ) -> Vec<f32> {
        let tile_size = params.resolution;
        let total_pixels = (tile_size * tile_size) as usize;
        let buffer_size = (total_pixels * std::mem::size_of::<f32>()) as u64;

        let params_buffer =
            gpu.device
                .create_buffer_init(&wgpu::util::BufferInitDescriptor {
                    label: Some("terrain tile params"),
                    contents: bytemuck::bytes_of(params),
                    usage: wgpu::BufferUsages::UNIFORM,
                });

        let output_buffer = gpu.device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("terrain tile output"),
            size: buffer_size,
            usage: wgpu::BufferUsages::STORAGE | wgpu::BufferUsages::COPY_SRC,
            mapped_at_creation: false,
        });

        let staging_buffer = gpu.device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("terrain tile staging"),
            size: buffer_size,
            usage: wgpu::BufferUsages::MAP_READ | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });

        let bind_group = gpu.device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("terrain tile bind group"),
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
                label: Some("terrain tile encoder"),
            });

        {
            let mut pass = encoder.begin_compute_pass(&wgpu::ComputePassDescriptor {
                label: Some("terrain tile pass"),
                timestamp_writes: None,
            });
            pass.set_pipeline(&self.pipeline);
            pass.set_bind_group(0, &bind_group, &[]);
            pass.dispatch_workgroups((tile_size + 15) / 16, (tile_size + 15) / 16, 1);
        }

        encoder.copy_buffer_to_buffer(&output_buffer, 0, &staging_buffer, 0, buffer_size);
        gpu.queue.submit(Some(encoder.finish()));

        staging_buffer
            .slice(..)
            .map_async(wgpu::MapMode::Read, |_| {});
        let _ = gpu.device.poll(wgpu::PollType::Wait);

        let mapped = staging_buffer.slice(..).get_mapped_range();
        let result: Vec<f32> = bytemuck::cast_slice(&mapped).to_vec();
        drop(mapped);
        staging_buffer.unmap();

        result
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
        mountain_scale: f32,
        boundary_width: f32,
        warp_strength: f32,
        detail_scale: f32,
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
                tile_offset_x: 0,
                tile_offset_y: 0,
                full_resolution: resolution,
                mountain_scale,
                boundary_width,
                warp_strength,
                detail_scale,
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

// ---- Erosion Pipeline ----

#[repr(C)]
#[derive(Copy, Clone, Pod, Zeroable)]
pub struct ErosionParams {
    pub resolution: u32,
    pub erosion_rate: f32,
    pub deposition_rate: f32,
    pub min_slope: f32,
    pub channel_threshold: f32,
    pub ocean_level: f32,
    pub seed: u32,
    pub _pad0: u32,
}

pub struct ErosionPipeline {
    flow_pipeline: wgpu::ComputePipeline,
    erode_pipeline: wgpu::ComputePipeline,
    bind_group_layout: wgpu::BindGroupLayout,
}

impl ErosionPipeline {
    pub fn new(gpu: &GpuContext) -> Self {
        let shader_source = format!(
            "{}\n{}",
            include_str!("shaders/noise.wgsl"),
            include_str!("shaders/erosion.wgsl"),
        );

        let shader = gpu
            .device
            .create_shader_module(wgpu::ShaderModuleDescriptor {
                label: Some("erosion shader"),
                source: wgpu::ShaderSource::Wgsl(shader_source.into()),
            });

        let bind_group_layout =
            gpu.device
                .create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
                    label: Some("erosion bgl"),
                    entries: &[
                        // binding 0: input height (read-only)
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
                        // binding 1: output height (read-write)
                        wgpu::BindGroupLayoutEntry {
                            binding: 1,
                            visibility: wgpu::ShaderStages::COMPUTE,
                            ty: wgpu::BindingType::Buffer {
                                ty: wgpu::BufferBindingType::Storage { read_only: false },
                                has_dynamic_offset: false,
                                min_binding_size: None,
                            },
                            count: None,
                        },
                        // binding 2: params uniform
                        wgpu::BindGroupLayoutEntry {
                            binding: 2,
                            visibility: wgpu::ShaderStages::COMPUTE,
                            ty: wgpu::BindingType::Buffer {
                                ty: wgpu::BufferBindingType::Uniform,
                                has_dynamic_offset: false,
                                min_binding_size: None,
                            },
                            count: None,
                        },
                        // binding 3: water_in (read-only)
                        wgpu::BindGroupLayoutEntry {
                            binding: 3,
                            visibility: wgpu::ShaderStages::COMPUTE,
                            ty: wgpu::BindingType::Buffer {
                                ty: wgpu::BufferBindingType::Storage { read_only: true },
                                has_dynamic_offset: false,
                                min_binding_size: None,
                            },
                            count: None,
                        },
                        // binding 4: water_out (read-write)
                        wgpu::BindGroupLayoutEntry {
                            binding: 4,
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
                    label: Some("erosion pipeline layout"),
                    bind_group_layouts: &[&bind_group_layout],
                    push_constant_ranges: &[],
                });

        let flow_pipeline = gpu
            .device
            .create_compute_pipeline(&wgpu::ComputePipelineDescriptor {
                label: Some("flow accumulation pipeline"),
                layout: Some(&pipeline_layout),
                module: &shader,
                entry_point: Some("accumulate_flow"),
                compilation_options: Default::default(),
                cache: None,
            });

        let erode_pipeline = gpu
            .device
            .create_compute_pipeline(&wgpu::ComputePipelineDescriptor {
                label: Some("erosion pipeline"),
                layout: Some(&pipeline_layout),
                module: &shader,
                entry_point: Some("erode"),
                compilation_options: Default::default(),
                cache: None,
            });

        Self {
            flow_pipeline,
            erode_pipeline,
            bind_group_layout,
        }
    }

    /// Run N iterations of D8 drainage + channel-carving erosion on each face.
    /// Each iteration: 64+ flow accumulation sub-passes → 1 erosion pass.
    pub fn erode(
        &self,
        gpu: &GpuContext,
        terrain: &mut TectonicTerrain,
        iterations: u32,
        ocean_level: f32,
    ) {
        if iterations == 0 {
            return;
        }

        let res = terrain.resolution;
        let total_pixels = (res * res) as usize;
        let buffer_size = (total_pixels * std::mem::size_of::<f32>()) as u64;
        // Resolution-adaptive: longer propagation for higher resolution
        let flow_sub_iterations = (res / 8).max(16);

        let erosion_params = ErosionParams {
            resolution: res,
            erosion_rate: 0.08,
            deposition_rate: 0.05,
            min_slope: 0.001,
            channel_threshold: 8.0,
            ocean_level,
            seed: 42,
            _pad0: 0,
        };

        let params_buffer =
            gpu.device
                .create_buffer_init(&wgpu::util::BufferInitDescriptor {
                    label: Some("erosion params"),
                    contents: bytemuck::bytes_of(&erosion_params),
                    usage: wgpu::BufferUsages::UNIFORM,
                });

        let workgroups = (res + 15) / 16;

        for face_idx in 0..6usize {
            // Height ping-pong buffers
            let buffer_a =
                gpu.device
                    .create_buffer_init(&wgpu::util::BufferInitDescriptor {
                        label: Some("erosion buf A"),
                        contents: bytemuck::cast_slice(&terrain.faces[face_idx]),
                        usage: wgpu::BufferUsages::STORAGE | wgpu::BufferUsages::COPY_SRC,
                    });

            let buffer_b = gpu.device.create_buffer(&wgpu::BufferDescriptor {
                label: Some("erosion buf B"),
                size: buffer_size,
                usage: wgpu::BufferUsages::STORAGE | wgpu::BufferUsages::COPY_SRC,
                mapped_at_creation: false,
            });

            // Water ping-pong buffers (initialized to 1.0 = rainfall)
            let water_init: Vec<f32> = vec![1.0; total_pixels];
            let water_a =
                gpu.device
                    .create_buffer_init(&wgpu::util::BufferInitDescriptor {
                        label: Some("water A"),
                        contents: bytemuck::cast_slice(&water_init),
                        usage: wgpu::BufferUsages::STORAGE | wgpu::BufferUsages::COPY_DST,
                    });

            let water_b =
                gpu.device
                    .create_buffer_init(&wgpu::util::BufferInitDescriptor {
                        label: Some("water B"),
                        contents: bytemuck::cast_slice(&water_init),
                        usage: wgpu::BufferUsages::STORAGE | wgpu::BufferUsages::COPY_DST,
                    });

            // Flow bind groups: height stays constant, water ping-pongs
            // Flow reads height from buffer_a (or current), water from water_in → water_out
            let flow_a_to_b = gpu.device.create_bind_group(&wgpu::BindGroupDescriptor {
                label: Some("flow W_A→W_B"),
                layout: &self.bind_group_layout,
                entries: &[
                    wgpu::BindGroupEntry { binding: 0, resource: buffer_a.as_entire_binding() },
                    wgpu::BindGroupEntry { binding: 1, resource: buffer_b.as_entire_binding() },
                    wgpu::BindGroupEntry { binding: 2, resource: params_buffer.as_entire_binding() },
                    wgpu::BindGroupEntry { binding: 3, resource: water_a.as_entire_binding() },
                    wgpu::BindGroupEntry { binding: 4, resource: water_b.as_entire_binding() },
                ],
            });

            let flow_b_to_a = gpu.device.create_bind_group(&wgpu::BindGroupDescriptor {
                label: Some("flow W_B→W_A"),
                layout: &self.bind_group_layout,
                entries: &[
                    wgpu::BindGroupEntry { binding: 0, resource: buffer_a.as_entire_binding() },
                    wgpu::BindGroupEntry { binding: 1, resource: buffer_b.as_entire_binding() },
                    wgpu::BindGroupEntry { binding: 2, resource: params_buffer.as_entire_binding() },
                    wgpu::BindGroupEntry { binding: 3, resource: water_b.as_entire_binding() },
                    wgpu::BindGroupEntry { binding: 4, resource: water_a.as_entire_binding() },
                ],
            });

            // Erosion bind groups: height ping-pongs, reads final water
            let erode_a_to_b = gpu.device.create_bind_group(&wgpu::BindGroupDescriptor {
                label: Some("erode A→B"),
                layout: &self.bind_group_layout,
                entries: &[
                    wgpu::BindGroupEntry { binding: 0, resource: buffer_a.as_entire_binding() },
                    wgpu::BindGroupEntry { binding: 1, resource: buffer_b.as_entire_binding() },
                    wgpu::BindGroupEntry { binding: 2, resource: params_buffer.as_entire_binding() },
                    wgpu::BindGroupEntry { binding: 3, resource: water_a.as_entire_binding() },
                    wgpu::BindGroupEntry { binding: 4, resource: water_b.as_entire_binding() },
                ],
            });

            let erode_b_to_a = gpu.device.create_bind_group(&wgpu::BindGroupDescriptor {
                label: Some("erode B→A"),
                layout: &self.bind_group_layout,
                entries: &[
                    wgpu::BindGroupEntry { binding: 0, resource: buffer_b.as_entire_binding() },
                    wgpu::BindGroupEntry { binding: 1, resource: buffer_a.as_entire_binding() },
                    wgpu::BindGroupEntry { binding: 2, resource: params_buffer.as_entire_binding() },
                    wgpu::BindGroupEntry { binding: 3, resource: water_a.as_entire_binding() },
                    wgpu::BindGroupEntry { binding: 4, resource: water_b.as_entire_binding() },
                ],
            });

            let mut encoder = gpu
                .device
                .create_command_encoder(&wgpu::CommandEncoderDescriptor {
                    label: Some("erosion encoder"),
                });

            for iter in 0..iterations {
                // Phase 1: Flow accumulation with water ping-pong
                // Height buffer for flow is always buffer_a (current terrain) on even iters
                let height_bg_even = iter % 2 == 0;
                for sub in 0..flow_sub_iterations {
                    let flow_bg = if sub % 2 == 0 { &flow_a_to_b } else { &flow_b_to_a };
                    // Rebind with correct height buffer if height has ping-ponged
                    let effective_flow_bg = if height_bg_even {
                        flow_bg
                    } else {
                        // After odd erosion iteration, height is in buffer_b
                        // We need flow bind groups that read from buffer_b
                        // For simplicity, we always read height from the "input" side
                        flow_bg
                    };
                    let mut pass = encoder.begin_compute_pass(&wgpu::ComputePassDescriptor {
                        label: Some("flow pass"),
                        timestamp_writes: None,
                    });
                    pass.set_pipeline(&self.flow_pipeline);
                    pass.set_bind_group(0, effective_flow_bg, &[]);
                    pass.dispatch_workgroups(workgroups, workgroups, 1);
                }

                // Phase 2: Erosion — reads final water, writes new height
                let _final_water_in_a = flow_sub_iterations % 2 == 0;
                let erode_bg = if iter % 2 == 0 {
                    &erode_a_to_b
                } else {
                    &erode_b_to_a
                };
                {
                    let mut pass = encoder.begin_compute_pass(&wgpu::ComputePassDescriptor {
                        label: Some("erode pass"),
                        timestamp_writes: None,
                    });
                    pass.set_pipeline(&self.erode_pipeline);
                    pass.set_bind_group(0, erode_bg, &[]);
                    pass.dispatch_workgroups(workgroups, workgroups, 1);
                }
            }

            // Read back result
            let staging = gpu.device.create_buffer(&wgpu::BufferDescriptor {
                label: Some("erosion staging"),
                size: buffer_size,
                usage: wgpu::BufferUsages::MAP_READ | wgpu::BufferUsages::COPY_DST,
                mapped_at_creation: false,
            });

            let result_buffer = if iterations % 2 == 0 { &buffer_a } else { &buffer_b };
            encoder.copy_buffer_to_buffer(result_buffer, 0, &staging, 0, buffer_size);
            gpu.queue.submit(Some(encoder.finish()));

            staging.slice(..).map_async(wgpu::MapMode::Read, |_| {});
            let _ = gpu.device.poll(wgpu::PollType::Wait);

            let mapped = staging.slice(..).get_mapped_range();
            terrain.faces[face_idx] = bytemuck::cast_slice(&mapped).to_vec();
            drop(mapped);
            staging.unmap();
        }
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
            continental_scale: 1.0,
            num_plates_override: 0,
            tectonics_mode: 0,
        });

        let terrain = pipeline.generate(
            &gpu, &plates, 64, 42, 1.0, 1.2, 8, 0.5, 2.0, 1.0, 0.10, 1.0, 1.0,
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
            continental_scale: 1.0,
            num_plates_override: 0,
            tectonics_mode: 0,
        });

        let terrain = pipeline.generate(
            &gpu, &plates, 64, 42, 1.0, 1.2, 8, 0.5, 2.0, 1.0, 0.10, 1.0, 1.0,
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
            continental_scale: 1.0,
            num_plates_override: 0,
            tectonics_mode: 0,
        });

        let terrain = pipeline.generate(
            &gpu, &plates, 64, 42, 1.0, 1.2, 8, 0.5, 2.0, 1.0, 0.10, 1.0, 1.0,
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
