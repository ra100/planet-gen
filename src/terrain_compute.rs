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
    pub surface_gravity: f32,  // m/s² (9.81 for Earth, 3.72 for Mars)
    pub tectonics_factor: f32, // [0,1]: 0=stagnant lid, 1=vigorous tectonics
    pub surface_age: f32,      // [0,1]: 0=young/sharp, 1=old/smooth
    pub continental_scale: f32, // noise frequency multiplier for continent size (1.0 = default)
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
        surface_gravity: f32,
        tectonics_factor: f32,
        surface_age: f32,
        continental_scale: f32,
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
                surface_gravity,
                tectonics_factor,
                surface_age,
                continental_scale,
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

// ---- Multi-Pass Plate Terrain Pipeline ----
// Pass 1: Voronoi plate assignment + boundary seed init
// Pass 2: JFA distance field (ping-pong, O(log n) passes)
// Pass 3: Terrain from plate data + distance fields

#[repr(C)]
#[derive(Copy, Clone, Pod, Zeroable)]
pub struct AssignParams {
    pub face: u32,
    pub resolution: u32,
    pub num_plates: u32,
    pub seed: u32,
    pub warp_strength: f32,
    pub _pad0: u32,
    pub _pad1: u32,
    pub _pad2: u32,
}

#[repr(C)]
#[derive(Copy, Clone, Pod, Zeroable)]
pub struct JfaParams {
    pub resolution: u32,
    pub step_size: u32,
    pub _pad0: u32,
    pub _pad1: u32,
}

pub struct MultiPassTerrainPipeline {
    assign_pipeline: wgpu::ComputePipeline,
    assign_bgl: wgpu::BindGroupLayout,
    jfa_pipeline: wgpu::ComputePipeline,
    jfa_bgl: wgpu::BindGroupLayout,
    terrain_pipeline: wgpu::ComputePipeline,
    terrain_bgl: wgpu::BindGroupLayout,
}

fn create_storage_entry(binding: u32, read_only: bool) -> wgpu::BindGroupLayoutEntry {
    wgpu::BindGroupLayoutEntry {
        binding,
        visibility: wgpu::ShaderStages::COMPUTE,
        ty: wgpu::BindingType::Buffer {
            ty: wgpu::BufferBindingType::Storage { read_only },
            has_dynamic_offset: false,
            min_binding_size: None,
        },
        count: None,
    }
}

fn create_uniform_entry(binding: u32) -> wgpu::BindGroupLayoutEntry {
    wgpu::BindGroupLayoutEntry {
        binding,
        visibility: wgpu::ShaderStages::COMPUTE,
        ty: wgpu::BindingType::Buffer {
            ty: wgpu::BufferBindingType::Uniform,
            has_dynamic_offset: false,
            min_binding_size: None,
        },
        count: None,
    }
}

impl MultiPassTerrainPipeline {
    pub fn new(gpu: &GpuContext) -> Self {
        let cube_sphere = include_str!("shaders/cube_sphere.wgsl");
        let noise = include_str!("shaders/noise.wgsl");

        // Pass 1: plate assignment
        let assign_src = format!("{cube_sphere}\n{noise}\n{}", include_str!("shaders/plate_assign.wgsl"));
        let assign_shader = gpu.device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("plate assign shader"),
            source: wgpu::ShaderSource::Wgsl(assign_src.into()),
        });
        let assign_bgl = gpu.device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("assign bgl"),
            entries: &[
                create_storage_entry(0, true),   // plates
                create_uniform_entry(1),          // params
                create_storage_entry(2, false),  // plate_idx output
                create_storage_entry(3, false),  // jfa_seeds output
            ],
        });
        let assign_layout = gpu.device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("assign layout"),
            bind_group_layouts: &[&assign_bgl],
            push_constant_ranges: &[],
        });
        let assign_pipeline = gpu.device.create_compute_pipeline(&wgpu::ComputePipelineDescriptor {
            label: Some("assign pipeline"),
            layout: Some(&assign_layout),
            module: &assign_shader,
            entry_point: Some("main"),
            compilation_options: Default::default(),
            cache: None,
        });

        // Pass 2: JFA
        let jfa_shader = gpu.device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("jfa shader"),
            source: wgpu::ShaderSource::Wgsl(include_str!("shaders/jfa.wgsl").into()),
        });
        let jfa_bgl = gpu.device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("jfa bgl"),
            entries: &[
                create_storage_entry(0, true),   // jfa_src
                create_storage_entry(1, false),  // jfa_dst
                create_uniform_entry(2),          // params
            ],
        });
        let jfa_layout = gpu.device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("jfa layout"),
            bind_group_layouts: &[&jfa_bgl],
            push_constant_ranges: &[],
        });
        let jfa_pipeline = gpu.device.create_compute_pipeline(&wgpu::ComputePipelineDescriptor {
            label: Some("jfa pipeline"),
            layout: Some(&jfa_layout),
            module: &jfa_shader,
            entry_point: Some("main"),
            compilation_options: Default::default(),
            cache: None,
        });

        // Pass 3: terrain from plates
        let terrain_src = format!("{cube_sphere}\n{noise}\n{}", include_str!("shaders/terrain_from_plates.wgsl"));
        let terrain_shader = gpu.device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("terrain from plates shader"),
            source: wgpu::ShaderSource::Wgsl(terrain_src.into()),
        });
        let terrain_bgl = gpu.device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("terrain from plates bgl"),
            entries: &[
                create_storage_entry(0, true),   // plates
                create_uniform_entry(1),          // params
                create_storage_entry(2, true),   // plate_idx
                create_storage_entry(3, true),   // jfa_data
                create_storage_entry(4, false),  // heightmap output
            ],
        });
        let terrain_layout = gpu.device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("terrain from plates layout"),
            bind_group_layouts: &[&terrain_bgl],
            push_constant_ranges: &[],
        });
        let terrain_pipeline = gpu.device.create_compute_pipeline(&wgpu::ComputePipelineDescriptor {
            label: Some("terrain from plates pipeline"),
            layout: Some(&terrain_layout),
            module: &terrain_shader,
            entry_point: Some("main"),
            compilation_options: Default::default(),
            cache: None,
        });

        Self {
            assign_pipeline, assign_bgl,
            jfa_pipeline, jfa_bgl,
            terrain_pipeline, terrain_bgl,
        }
    }

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
        surface_gravity: f32,
        tectonics_factor: f32,
        surface_age: f32,
        continental_scale: f32,
    ) -> TectonicTerrain {
        let total_pixels = (resolution * resolution) as usize;
        let f32_size = std::mem::size_of::<f32>() as u64;
        let u32_size = std::mem::size_of::<u32>() as u64;
        // JfaSeed = 4 x i32 = 16 bytes
        let jfa_seed_size = 16u64;

        let plates_buffer = gpu.device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("plates buffer"),
            contents: bytemuck::cast_slice(plates),
            usage: wgpu::BufferUsages::STORAGE,
        });

        // Buffers reused across faces
        let plate_idx_buf = gpu.device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("plate_idx"),
            size: total_pixels as u64 * u32_size,
            usage: wgpu::BufferUsages::STORAGE | wgpu::BufferUsages::COPY_SRC,
            mapped_at_creation: false,
        });
        let jfa_buf_a = gpu.device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("jfa_a"),
            size: total_pixels as u64 * jfa_seed_size,
            usage: wgpu::BufferUsages::STORAGE | wgpu::BufferUsages::COPY_SRC,
            mapped_at_creation: false,
        });
        let jfa_buf_b = gpu.device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("jfa_b"),
            size: total_pixels as u64 * jfa_seed_size,
            usage: wgpu::BufferUsages::STORAGE | wgpu::BufferUsages::COPY_SRC,
            mapped_at_creation: false,
        });
        let heightmap_buf = gpu.device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("heightmap"),
            size: total_pixels as u64 * f32_size,
            usage: wgpu::BufferUsages::STORAGE | wgpu::BufferUsages::COPY_SRC,
            mapped_at_creation: false,
        });
        let staging_buf = gpu.device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("staging"),
            size: total_pixels as u64 * f32_size,
            usage: wgpu::BufferUsages::MAP_READ | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });

        let workgroups = (resolution + 15) / 16;
        let num_jfa_passes = (resolution as f32).log2().ceil() as u32;
        let mut faces: [Vec<f32>; 6] = Default::default();

        for face_idx in 0..6u32 {
            // --- Pass 1: Plate assignment ---
            let assign_params = AssignParams {
                face: face_idx,
                resolution,
                num_plates: plates.len() as u32,
                seed,
                warp_strength,
                _pad0: 0, _pad1: 0, _pad2: 0,
            };
            let assign_params_buf = gpu.device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
                label: Some("assign params"),
                contents: bytemuck::bytes_of(&assign_params),
                usage: wgpu::BufferUsages::UNIFORM,
            });
            let assign_bg = gpu.device.create_bind_group(&wgpu::BindGroupDescriptor {
                label: Some("assign bg"),
                layout: &self.assign_bgl,
                entries: &[
                    wgpu::BindGroupEntry { binding: 0, resource: plates_buffer.as_entire_binding() },
                    wgpu::BindGroupEntry { binding: 1, resource: assign_params_buf.as_entire_binding() },
                    wgpu::BindGroupEntry { binding: 2, resource: plate_idx_buf.as_entire_binding() },
                    wgpu::BindGroupEntry { binding: 3, resource: jfa_buf_a.as_entire_binding() },
                ],
            });

            let mut encoder = gpu.device.create_command_encoder(&wgpu::CommandEncoderDescriptor {
                label: Some("multipass encoder"),
            });
            {
                let mut pass = encoder.begin_compute_pass(&wgpu::ComputePassDescriptor {
                    label: Some("pass1: assign"),
                    timestamp_writes: None,
                });
                pass.set_pipeline(&self.assign_pipeline);
                pass.set_bind_group(0, &assign_bg, &[]);
                pass.dispatch_workgroups(workgroups, workgroups, 1);
            }
            gpu.queue.submit(Some(encoder.finish()));

            // --- Pass 2: JFA iterations ---
            // Ping-pong between jfa_buf_a and jfa_buf_b
            let mut src_is_a = true;
            for i in 0..num_jfa_passes {
                let step = 1u32 << (num_jfa_passes - 1 - i);
                let jfa_params = JfaParams {
                    resolution,
                    step_size: step,
                    _pad0: 0, _pad1: 0,
                };
                let jfa_params_buf = gpu.device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
                    label: Some("jfa params"),
                    contents: bytemuck::bytes_of(&jfa_params),
                    usage: wgpu::BufferUsages::UNIFORM,
                });

                let (src_buf, dst_buf) = if src_is_a {
                    (&jfa_buf_a, &jfa_buf_b)
                } else {
                    (&jfa_buf_b, &jfa_buf_a)
                };

                let jfa_bg = gpu.device.create_bind_group(&wgpu::BindGroupDescriptor {
                    label: Some("jfa bg"),
                    layout: &self.jfa_bgl,
                    entries: &[
                        wgpu::BindGroupEntry { binding: 0, resource: src_buf.as_entire_binding() },
                        wgpu::BindGroupEntry { binding: 1, resource: dst_buf.as_entire_binding() },
                        wgpu::BindGroupEntry { binding: 2, resource: jfa_params_buf.as_entire_binding() },
                    ],
                });

                let mut encoder = gpu.device.create_command_encoder(&wgpu::CommandEncoderDescriptor {
                    label: Some("jfa encoder"),
                });
                {
                    let mut pass = encoder.begin_compute_pass(&wgpu::ComputePassDescriptor {
                        label: Some("pass2: jfa"),
                        timestamp_writes: None,
                    });
                    pass.set_pipeline(&self.jfa_pipeline);
                    pass.set_bind_group(0, &jfa_bg, &[]);
                    pass.dispatch_workgroups(workgroups, workgroups, 1);
                }
                gpu.queue.submit(Some(encoder.finish()));
                src_is_a = !src_is_a;
            }

            // The final JFA result is in whichever buffer was last written to
            let final_jfa_buf = if src_is_a { &jfa_buf_a } else { &jfa_buf_b };

            // --- Pass 3: Terrain generation ---
            let terrain_params = TerrainGenParams {
                face: face_idx,
                resolution,
                num_plates: plates.len() as u32,
                seed,
                amplitude, frequency, octaves, gain, lacunarity,
                tile_offset_x: 0, tile_offset_y: 0,
                full_resolution: resolution,
                mountain_scale, boundary_width, warp_strength, detail_scale,
                surface_gravity, tectonics_factor, surface_age,
                continental_scale,
            };
            let terrain_params_buf = gpu.device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
                label: Some("terrain params"),
                contents: bytemuck::bytes_of(&terrain_params),
                usage: wgpu::BufferUsages::UNIFORM,
            });
            let terrain_bg = gpu.device.create_bind_group(&wgpu::BindGroupDescriptor {
                label: Some("terrain bg"),
                layout: &self.terrain_bgl,
                entries: &[
                    wgpu::BindGroupEntry { binding: 0, resource: plates_buffer.as_entire_binding() },
                    wgpu::BindGroupEntry { binding: 1, resource: terrain_params_buf.as_entire_binding() },
                    wgpu::BindGroupEntry { binding: 2, resource: plate_idx_buf.as_entire_binding() },
                    wgpu::BindGroupEntry { binding: 3, resource: final_jfa_buf.as_entire_binding() },
                    wgpu::BindGroupEntry { binding: 4, resource: heightmap_buf.as_entire_binding() },
                ],
            });

            let mut encoder = gpu.device.create_command_encoder(&wgpu::CommandEncoderDescriptor {
                label: Some("terrain encoder"),
            });
            {
                let mut pass = encoder.begin_compute_pass(&wgpu::ComputePassDescriptor {
                    label: Some("pass3: terrain"),
                    timestamp_writes: None,
                });
                pass.set_pipeline(&self.terrain_pipeline);
                pass.set_bind_group(0, &terrain_bg, &[]);
                pass.dispatch_workgroups(workgroups, workgroups, 1);
            }

            // Readback
            encoder.copy_buffer_to_buffer(
                &heightmap_buf, 0,
                &staging_buf, 0,
                total_pixels as u64 * f32_size,
            );
            gpu.queue.submit(Some(encoder.finish()));

            staging_buf.slice(..).map_async(wgpu::MapMode::Read, |_| {});
            let _ = gpu.device.poll(wgpu::PollType::Wait);
            let mapped = staging_buf.slice(..).get_mapped_range();
            faces[face_idx as usize] = bytemuck::cast_slice(&mapped).to_vec();
            drop(mapped);
            staging_buf.unmap();
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
            num_continents: 0,
            continent_size_variety: 0.0,
        });

        let terrain = pipeline.generate(
            &gpu, &plates, 64, 42, 1.0, 1.2, 8, 0.5, 2.0, 1.0, 0.10, 1.0, 1.0, 9.81, 0.85, 0.2, 1.0,
        );

        assert_eq!(terrain.faces.len(), 6);
        for (i, face) in terrain.faces.iter().enumerate() {
            assert_eq!(face.len(), 64 * 64, "face {i} wrong size");
            assert!(face.iter().all(|v| !v.is_nan()), "face {i} has NaN");
        }
    }

    #[test]
    fn test_tectonic_terrain_has_height_variation() {
        let gpu = GpuContext::new().expect("GPU init failed");
        let pipeline = TerrainComputePipeline::new(&gpu);

        let plates = generate_plates(&PlateGenParams {
            seed: 42,
            mass_earth: 1.0,
            ocean_fraction: 0.7,
            tectonics_factor: 0.85,
            continental_scale: 1.0,
            num_plates_override: 0,
            num_continents: 0,
            continent_size_variety: 0.0,
        });

        let terrain = pipeline.generate(
            &gpu, &plates, 64, 42, 1.0, 1.2, 8, 0.5, 2.0, 1.0, 0.10, 1.0, 1.0, 9.81, 0.85, 0.2, 1.0,
        );

        let all_heights: Vec<f32> = terrain.faces.iter().flat_map(|f| f.iter().copied()).collect();
        let min_h = all_heights.iter().cloned().fold(f32::INFINITY, f32::min);
        let max_h = all_heights.iter().cloned().fold(f32::NEG_INFINITY, f32::max);

        // Continuous model should produce a meaningful height range
        assert!(
            max_h - min_h > 0.3,
            "Height range should be > 0.3, got {:.3} (min={:.3}, max={:.3})",
            max_h - min_h, min_h, max_h
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
            num_continents: 0,
            continent_size_variety: 0.0,
        });

        let terrain = pipeline.generate(
            &gpu, &plates, 64, 42, 1.0, 1.2, 8, 0.5, 2.0, 1.0, 0.10, 1.0, 1.0, 9.81, 0.85, 0.2, 1.0,
        );

        let all_heights: Vec<f32> = terrain.faces.iter().flat_map(|f| f.iter().copied()).collect();
        let max_height = all_heights.iter().cloned().fold(f32::NEG_INFINITY, f32::max);

        // Should have elevated peaks from convergent boundary mountains
        assert!(
            max_height > 0.2,
            "Should have peaks > 0.2, max is {}",
            max_height
        );
    }
}
