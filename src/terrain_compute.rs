use crate::gpu::GpuContext;
use crate::plates::PlateGpu;
use bytemuck::{Pod, Zeroable};
use wgpu::util::DeviceExt;

/// Convert a physical angular velocity to the wind model's documented
/// Earth-relative convention (1.0 = the app's 24-hour Earth reference).
pub fn earth_relative_rotation_rate(rotation_rate_rad_s: f32) -> f32 {
    rotation_rate_rad_s / (std::f32::consts::TAU / 86_400.0)
}

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
    pub mountain_scale: f32, // multiplier for tectonic mountain height (1.0 = default)
    pub boundary_width: f32, // sigma for boundary influence spread (0.10 = default)
    pub warp_strength: f32,  // domain warp intensity (1.0 = default)
    pub detail_scale: f32,   // fBm detail noise intensity (1.0 = default)
    pub surface_gravity: f32, // m/s² (9.81 for Earth, 3.72 for Mars)
    pub tectonics_factor: f32, // [0,1]: 0=stagnant lid, 1=vigorous tectonics
    pub surface_age: f32,    // [0,1]: 0=young/sharp, 1=old/smooth
    pub continental_scale: f32, // noise frequency multiplier for continent size (1.0 = default)
}

#[derive(Copy, Clone, Debug, PartialEq)]
pub struct TerrainGenerationParams {
    pub seed: u32,
    pub amplitude: f32,
    pub frequency: f32,
    pub octaves: u32,
    pub gain: f32,
    pub lacunarity: f32,
    pub mountain_scale: f32,
    pub boundary_width: f32,
    pub warp_strength: f32,
    pub detail_scale: f32,
    pub surface_gravity: f32,
    pub tectonics_factor: f32,
    pub surface_age: f32,
    pub continental_scale: f32,
    pub num_plates_override: u32,
    pub num_continents: u32,
    pub continent_size_variety: f32,
}

impl TerrainGenParams {
    pub fn for_tile(
        terrain: &TerrainGenerationParams,
        face: u32,
        resolution: u32,
        num_plates: u32,
        tile_offset_x: u32,
        tile_offset_y: u32,
        full_resolution: u32,
    ) -> Self {
        Self {
            face,
            resolution,
            num_plates,
            seed: terrain.seed,
            amplitude: terrain.amplitude,
            frequency: terrain.frequency,
            octaves: terrain.octaves,
            gain: terrain.gain,
            lacunarity: terrain.lacunarity,
            tile_offset_x,
            tile_offset_y,
            full_resolution,
            mountain_scale: terrain.mountain_scale,
            boundary_width: terrain.boundary_width,
            warp_strength: terrain.warp_strength,
            detail_scale: terrain.detail_scale,
            surface_gravity: terrain.surface_gravity,
            tectonics_factor: terrain.tectonics_factor,
            surface_age: terrain.surface_age,
            continental_scale: terrain.continental_scale,
        }
    }
}

/// Generated tectonic heightmap for all 6 cube faces.
#[derive(Clone)]
pub struct TectonicTerrain {
    pub faces: [Vec<f32>; 6],
    pub resolution: u32,
}

impl TectonicTerrain {
    /// Fraction of the sphere below `ocean_level`, weighted by cubemap texel
    /// solid angle rather than treating all face pixels as equal-area samples.
    pub fn solid_angle_ocean_coverage(&self, ocean_level: f32) -> f32 {
        if self.resolution == 0 {
            return 0.0;
        }
        let denominator = self.resolution.saturating_sub(1).max(1) as f64;
        let mut wet_weight = 0.0_f64;
        let mut total_weight = 0.0_f64;
        // Each face uses the same solid-angle weights; evaluate the expensive
        // weight once per face coordinate, not six times per terrain readback.
        for index in 0..(self.resolution as usize).pow(2) {
            let x = 2.0 * (index % self.resolution as usize) as f64 / denominator - 1.0;
            let y = 2.0 * (index / self.resolution as usize) as f64 / denominator - 1.0;
            let weight = (1.0 + x * x + y * y).powf(-1.5);
            for face in &self.faces {
                if let Some(height) = face.get(index) {
                    total_weight += weight;
                    if *height < ocean_level {
                        wet_weight += weight;
                    }
                }
            }
        }
        if total_weight > 0.0 {
            (wet_weight / total_weight) as f32
        } else {
            0.0
        }
    }
}

pub struct TerrainComputePipeline {
    pipeline: wgpu::ComputePipeline,
    bind_group_layout: wgpu::BindGroupLayout,
}

impl TerrainComputePipeline {
    pub fn new(gpu: &GpuContext) -> Self {
        let shader_source = format!(
            "{}\n{}\n{}\n{}",
            include_str!("shaders/cube_sphere.wgsl"),
            include_str!("shaders/noise.wgsl"),
            include_str!("shaders/terrain_profiles.wgsl"),
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

        let pipeline_layout = gpu
            .device
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
        cancel: &std::sync::atomic::AtomicBool,
    ) -> Result<Vec<f32>, String> {
        let tile_size = params.resolution;
        let total_pixels = (tile_size * tile_size) as usize;
        let buffer_size = (total_pixels * std::mem::size_of::<f32>()) as u64;

        let params_buffer = gpu
            .device
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
            pass.dispatch_workgroups(tile_size.div_ceil(16), tile_size.div_ceil(16), 1);
        }

        encoder.copy_buffer_to_buffer(&output_buffer, 0, &staging_buffer, 0, buffer_size);
        gpu.queue.submit(Some(encoder.finish()));

        let slice = staging_buffer.slice(..);
        let (sender, receiver) = std::sync::mpsc::channel();
        slice.map_async(wgpu::MapMode::Read, move |result| {
            let _ = sender.send(result.map_err(|error| error.to_string()));
        });
        loop {
            if cancel.load(std::sync::atomic::Ordering::Relaxed) {
                return Err("Cancelled while waiting for terrain readback".into());
            }
            match receiver.try_recv() {
                Ok(result) => {
                    result.map_err(|error| format!("terrain readback failed: {error}"))?
                }
                Err(std::sync::mpsc::TryRecvError::Disconnected) => {
                    return Err("terrain readback callback disconnected".into());
                }
                Err(std::sync::mpsc::TryRecvError::Empty) => {
                    let _ = gpu.device.poll(wgpu::PollType::Poll);
                    std::thread::sleep(std::time::Duration::from_millis(1));
                    continue;
                }
            }
            break;
        }

        let mapped = staging_buffer.slice(..).get_mapped_range();
        let result: Vec<f32> = bytemuck::cast_slice(&mapped).to_vec();
        drop(mapped);
        staging_buffer.unmap();

        Ok(result)
    }

    /// Generate tectonic terrain for all 6 cube faces.
    #[allow(clippy::too_many_arguments)]
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
        let plates_buffer = gpu
            .device
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

            let params_buffer = gpu
                .device
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
                pass.dispatch_workgroups(resolution.div_ceil(16), resolution.div_ceil(16), 1);
            }

            encoder.copy_buffer_to_buffer(&output_buffer, 0, &staging_buffer, 0, buffer_size);
            gpu.queue.submit(Some(encoder.finish()));

            staging_buffer
                .slice(..)
                .map_async(wgpu::MapMode::Read, |_| {});
            let _ = gpu.device.poll(wgpu::PollType::Wait {
                submission_index: None,
                timeout: None,
            });

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
        let assign_src = format!(
            "{cube_sphere}\n{noise}\n{}",
            include_str!("shaders/plate_assign.wgsl")
        );
        let assign_shader = gpu
            .device
            .create_shader_module(wgpu::ShaderModuleDescriptor {
                label: Some("plate assign shader"),
                source: wgpu::ShaderSource::Wgsl(assign_src.into()),
            });
        let assign_bgl = gpu
            .device
            .create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
                label: Some("assign bgl"),
                entries: &[
                    create_storage_entry(0, true),  // plates
                    create_uniform_entry(1),        // params
                    create_storage_entry(2, false), // plate_idx output
                    create_storage_entry(3, false), // jfa_seeds output
                ],
            });
        let assign_layout = gpu
            .device
            .create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
                label: Some("assign layout"),
                bind_group_layouts: &[&assign_bgl],
                push_constant_ranges: &[],
            });
        let assign_pipeline =
            gpu.device
                .create_compute_pipeline(&wgpu::ComputePipelineDescriptor {
                    label: Some("assign pipeline"),
                    layout: Some(&assign_layout),
                    module: &assign_shader,
                    entry_point: Some("main"),
                    compilation_options: Default::default(),
                    cache: None,
                });

        // Pass 2: JFA
        let jfa_shader = gpu
            .device
            .create_shader_module(wgpu::ShaderModuleDescriptor {
                label: Some("jfa shader"),
                source: wgpu::ShaderSource::Wgsl(include_str!("shaders/jfa.wgsl").into()),
            });
        let jfa_bgl = gpu
            .device
            .create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
                label: Some("jfa bgl"),
                entries: &[
                    create_storage_entry(0, true),  // jfa_src
                    create_storage_entry(1, false), // jfa_dst
                    create_uniform_entry(2),        // params
                ],
            });
        let jfa_layout = gpu
            .device
            .create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
                label: Some("jfa layout"),
                bind_group_layouts: &[&jfa_bgl],
                push_constant_ranges: &[],
            });
        let jfa_pipeline = gpu
            .device
            .create_compute_pipeline(&wgpu::ComputePipelineDescriptor {
                label: Some("jfa pipeline"),
                layout: Some(&jfa_layout),
                module: &jfa_shader,
                entry_point: Some("main"),
                compilation_options: Default::default(),
                cache: None,
            });

        // Pass 3: terrain from plates
        let terrain_src = format!(
            "{cube_sphere}\n{noise}\n{}\n{}",
            include_str!("shaders/terrain_profiles.wgsl"),
            include_str!("shaders/terrain_from_plates.wgsl")
        );
        let terrain_shader = gpu
            .device
            .create_shader_module(wgpu::ShaderModuleDescriptor {
                label: Some("terrain from plates shader"),
                source: wgpu::ShaderSource::Wgsl(terrain_src.into()),
            });
        let terrain_bgl = gpu
            .device
            .create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
                label: Some("terrain from plates bgl"),
                entries: &[
                    create_storage_entry(0, true),  // plates
                    create_uniform_entry(1),        // params
                    create_storage_entry(2, true),  // plate_idx
                    create_storage_entry(3, true),  // jfa_data
                    create_storage_entry(4, false), // heightmap output
                ],
            });
        let terrain_layout = gpu
            .device
            .create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
                label: Some("terrain from plates layout"),
                bind_group_layouts: &[&terrain_bgl],
                push_constant_ranges: &[],
            });
        let terrain_pipeline =
            gpu.device
                .create_compute_pipeline(&wgpu::ComputePipelineDescriptor {
                    label: Some("terrain from plates pipeline"),
                    layout: Some(&terrain_layout),
                    module: &terrain_shader,
                    entry_point: Some("main"),
                    compilation_options: Default::default(),
                    cache: None,
                });

        Self {
            assign_pipeline,
            assign_bgl,
            jfa_pipeline,
            jfa_bgl,
            terrain_pipeline,
            terrain_bgl,
        }
    }

    #[allow(clippy::too_many_arguments)]
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

        let plates_buffer = gpu
            .device
            .create_buffer_init(&wgpu::util::BufferInitDescriptor {
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

        let workgroups = resolution.div_ceil(16);
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
                _pad0: 0,
                _pad1: 0,
                _pad2: 0,
            };
            let assign_params_buf =
                gpu.device
                    .create_buffer_init(&wgpu::util::BufferInitDescriptor {
                        label: Some("assign params"),
                        contents: bytemuck::bytes_of(&assign_params),
                        usage: wgpu::BufferUsages::UNIFORM,
                    });
            let assign_bg = gpu.device.create_bind_group(&wgpu::BindGroupDescriptor {
                label: Some("assign bg"),
                layout: &self.assign_bgl,
                entries: &[
                    wgpu::BindGroupEntry {
                        binding: 0,
                        resource: plates_buffer.as_entire_binding(),
                    },
                    wgpu::BindGroupEntry {
                        binding: 1,
                        resource: assign_params_buf.as_entire_binding(),
                    },
                    wgpu::BindGroupEntry {
                        binding: 2,
                        resource: plate_idx_buf.as_entire_binding(),
                    },
                    wgpu::BindGroupEntry {
                        binding: 3,
                        resource: jfa_buf_a.as_entire_binding(),
                    },
                ],
            });

            let mut encoder = gpu
                .device
                .create_command_encoder(&wgpu::CommandEncoderDescriptor {
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
                    _pad0: 0,
                    _pad1: 0,
                };
                let jfa_params_buf =
                    gpu.device
                        .create_buffer_init(&wgpu::util::BufferInitDescriptor {
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
                        wgpu::BindGroupEntry {
                            binding: 0,
                            resource: src_buf.as_entire_binding(),
                        },
                        wgpu::BindGroupEntry {
                            binding: 1,
                            resource: dst_buf.as_entire_binding(),
                        },
                        wgpu::BindGroupEntry {
                            binding: 2,
                            resource: jfa_params_buf.as_entire_binding(),
                        },
                    ],
                });

                let mut encoder =
                    gpu.device
                        .create_command_encoder(&wgpu::CommandEncoderDescriptor {
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
            let terrain_params_buf =
                gpu.device
                    .create_buffer_init(&wgpu::util::BufferInitDescriptor {
                        label: Some("terrain params"),
                        contents: bytemuck::bytes_of(&terrain_params),
                        usage: wgpu::BufferUsages::UNIFORM,
                    });
            let terrain_bg = gpu.device.create_bind_group(&wgpu::BindGroupDescriptor {
                label: Some("terrain bg"),
                layout: &self.terrain_bgl,
                entries: &[
                    wgpu::BindGroupEntry {
                        binding: 0,
                        resource: plates_buffer.as_entire_binding(),
                    },
                    wgpu::BindGroupEntry {
                        binding: 1,
                        resource: terrain_params_buf.as_entire_binding(),
                    },
                    wgpu::BindGroupEntry {
                        binding: 2,
                        resource: plate_idx_buf.as_entire_binding(),
                    },
                    wgpu::BindGroupEntry {
                        binding: 3,
                        resource: final_jfa_buf.as_entire_binding(),
                    },
                    wgpu::BindGroupEntry {
                        binding: 4,
                        resource: heightmap_buf.as_entire_binding(),
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
                    label: Some("pass3: terrain"),
                    timestamp_writes: None,
                });
                pass.set_pipeline(&self.terrain_pipeline);
                pass.set_bind_group(0, &terrain_bg, &[]);
                pass.dispatch_workgroups(workgroups, workgroups, 1);
            }

            // Readback
            encoder.copy_buffer_to_buffer(
                &heightmap_buf,
                0,
                &staging_buf,
                0,
                total_pixels as u64 * f32_size,
            );
            gpu.queue.submit(Some(encoder.finish()));

            staging_buf.slice(..).map_async(wgpu::MapMode::Read, |_| {});
            let _ = gpu.device.poll(wgpu::PollType::Wait {
                submission_index: None,
                timeout: None,
            });
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
    pub width: u32,
    pub height: u32,
    pub full_resolution: u32,
    pub row_offset: u32,
    pub erosion_rate: f32,
    pub deposition_rate: f32,
    pub min_slope: f32,
    pub channel_threshold: f32,
    pub ocean_level: f32,
    pub seed: u32,
    pub _pad0: u32,
}

#[derive(Copy, Clone, Debug, PartialEq, Eq)]
struct ErosionTile {
    row_offset: u32,
    interior_rows: u32,
}

struct ErosionTileBuffers {
    tile: ErosionTile,
    height_a: wgpu::Buffer,
    height_b: wgpu::Buffer,
    water_a: wgpu::Buffer,
    water_b: wgpu::Buffer,
    params: wgpu::Buffer,
}

#[derive(Debug, thiserror::Error)]
pub enum ErosionError {
    #[error(
        "erosion at {resolution}px requires a storage binding larger than the device limit of {max_binding_bytes} bytes"
    )]
    InsufficientStorageBinding {
        resolution: u32,
        max_binding_bytes: u64,
    },
    #[error("erosion readback cancelled")]
    ReadbackCancelled,
    #[error("erosion readback failed: {0}")]
    ReadbackFailed(String),
    #[error("erosion memory estimate overflowed")]
    MemoryEstimateOverflow,
}

fn erosion_tiles(
    resolution: u32,
    max_binding_bytes: u64,
) -> Result<Vec<ErosionTile>, ErosionError> {
    let row_bytes = u64::from(resolution)
        .checked_mul(std::mem::size_of::<f32>() as u64)
        .ok_or(ErosionError::MemoryEstimateOverflow)?;
    let rows_with_halo = max_binding_bytes / row_bytes;
    let max_interior_rows =
        rows_with_halo
            .checked_sub(2)
            .ok_or(ErosionError::InsufficientStorageBinding {
                resolution,
                max_binding_bytes,
            })? as u32;
    if max_interior_rows == 0 {
        return Err(ErosionError::InsufficientStorageBinding {
            resolution,
            max_binding_bytes,
        });
    }

    let tile_count = resolution.div_ceil(max_interior_rows);
    let rows_per_tile = resolution.div_ceil(tile_count);
    Ok((0..resolution)
        .step_by(rows_per_tile as usize)
        .map(|row_offset| ErosionTile {
            row_offset,
            interior_rows: (resolution - row_offset).min(rows_per_tile),
        })
        .collect())
}

pub struct ErosionPipeline {
    flow_pipeline: wgpu::ComputePipeline,
    erode_pipeline: wgpu::ComputePipeline,
    bind_group_layout: wgpu::BindGroupLayout,
}

impl ErosionPipeline {
    pub fn aggregate_tiled_bytes(
        resolution: u32,
        limits: &wgpu::Limits,
    ) -> Result<u64, ErosionError> {
        let max_binding_bytes =
            u64::from(limits.max_storage_buffer_binding_size).min(limits.max_buffer_size);
        let tiles = erosion_tiles(resolution, max_binding_bytes)?;
        let rows_with_halos = tiles.iter().try_fold(0_u64, |total, tile| {
            total
                .checked_add(u64::from(tile.interior_rows) + 2)
                .ok_or(ErosionError::MemoryEstimateOverflow)
        })?;
        u64::from(resolution)
            .checked_mul(rows_with_halos)
            .and_then(|values| values.checked_mul(std::mem::size_of::<f32>() as u64))
            // Four concurrent storage buffers plus one staging buffer per tile.
            .and_then(|tile_bytes| tile_bytes.checked_mul(5))
            .ok_or(ErosionError::MemoryEstimateOverflow)
    }

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

        let pipeline_layout = gpu
            .device
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
    ) -> Result<(), ErosionError> {
        let cancel = std::sync::atomic::AtomicBool::new(false);
        self.erode_with_cancel(gpu, terrain, iterations, ocean_level, &cancel)
    }

    pub fn erode_with_cancel(
        &self,
        gpu: &GpuContext,
        terrain: &mut TectonicTerrain,
        iterations: u32,
        ocean_level: f32,
        cancel: &std::sync::atomic::AtomicBool,
    ) -> Result<(), ErosionError> {
        if iterations == 0 {
            return Ok(());
        }

        let res = terrain.resolution;
        let limits = gpu.device.limits();
        let tiles = erosion_tiles(
            res,
            u64::from(limits.max_storage_buffer_binding_size).min(limits.max_buffer_size),
        )?;
        // Resolution-adaptive: longer propagation for higher resolution
        let flow_sub_iterations = (res / 8).max(16);

        for face_idx in 0..6usize {
            let tile_buffers: Vec<_> = tiles
                .iter()
                .copied()
                .map(|tile| {
                    let height = (0..tile.interior_rows + 2)
                        .flat_map(|local_y| {
                            let global_y =
                                (tile.row_offset + local_y).saturating_sub(1).min(res - 1);
                            let start = (global_y * res) as usize;
                            terrain.faces[face_idx][start..start + res as usize]
                                .iter()
                                .copied()
                        })
                        .collect::<Vec<_>>();
                    let tile_pixels = u64::from(res) * u64::from(tile.interior_rows + 2);
                    let usage = wgpu::BufferUsages::STORAGE
                        | wgpu::BufferUsages::COPY_SRC
                        | wgpu::BufferUsages::COPY_DST;
                    let params = ErosionParams {
                        width: res,
                        height: tile.interior_rows + 2,
                        full_resolution: res,
                        row_offset: tile.row_offset,
                        erosion_rate: 0.08,
                        deposition_rate: 0.05,
                        min_slope: 0.001,
                        channel_threshold: 8.0,
                        ocean_level,
                        seed: 42,
                        _pad0: 0,
                    };
                    ErosionTileBuffers {
                        tile,
                        height_a: gpu.device.create_buffer_init(
                            &wgpu::util::BufferInitDescriptor {
                                label: Some("erosion height A"),
                                contents: bytemuck::cast_slice(&height),
                                usage,
                            },
                        ),
                        height_b: gpu.device.create_buffer_init(
                            &wgpu::util::BufferInitDescriptor {
                                label: Some("erosion height B"),
                                contents: bytemuck::cast_slice(&height),
                                usage,
                            },
                        ),
                        water_a: gpu
                            .device
                            .create_buffer_init(&wgpu::util::BufferInitDescriptor {
                                label: Some("erosion water A"),
                                contents: bytemuck::cast_slice(&vec![
                                    1.0_f32;
                                    tile_pixels as usize
                                ]),
                                usage,
                            }),
                        water_b: gpu
                            .device
                            .create_buffer_init(&wgpu::util::BufferInitDescriptor {
                                label: Some("erosion water B"),
                                contents: bytemuck::cast_slice(&vec![
                                    1.0_f32;
                                    tile_pixels as usize
                                ]),
                                usage,
                            }),
                        params: gpu
                            .device
                            .create_buffer_init(&wgpu::util::BufferInitDescriptor {
                                label: Some("erosion params"),
                                contents: bytemuck::bytes_of(&params),
                                usage: wgpu::BufferUsages::UNIFORM,
                            }),
                    }
                })
                .collect();

            let bind_group = |label,
                              tile: &ErosionTileBuffers,
                              input_height: &wgpu::Buffer,
                              output_height: &wgpu::Buffer,
                              water_in: &wgpu::Buffer,
                              water_out: &wgpu::Buffer| {
                gpu.device.create_bind_group(&wgpu::BindGroupDescriptor {
                    label: Some(label),
                    layout: &self.bind_group_layout,
                    entries: &[
                        wgpu::BindGroupEntry {
                            binding: 0,
                            resource: input_height.as_entire_binding(),
                        },
                        wgpu::BindGroupEntry {
                            binding: 1,
                            resource: output_height.as_entire_binding(),
                        },
                        wgpu::BindGroupEntry {
                            binding: 2,
                            resource: tile.params.as_entire_binding(),
                        },
                        wgpu::BindGroupEntry {
                            binding: 3,
                            resource: water_in.as_entire_binding(),
                        },
                        wgpu::BindGroupEntry {
                            binding: 4,
                            resource: water_out.as_entire_binding(),
                        },
                    ],
                })
            };
            let mut encoder = gpu
                .device
                .create_command_encoder(&wgpu::CommandEncoderDescriptor {
                    label: Some("erosion encoder"),
                });

            macro_rules! synchronize_halos {
                ($field:ident) => {
                    let row_bytes = u64::from(res) * std::mem::size_of::<f32>() as u64;
                    for pair in tile_buffers.windows(2) {
                        let upper = &pair[0];
                        let lower = &pair[1];
                        encoder.copy_buffer_to_buffer(
                            &upper.$field,
                            u64::from(upper.tile.interior_rows) * row_bytes,
                            &lower.$field,
                            0,
                            row_bytes,
                        );
                        encoder.copy_buffer_to_buffer(
                            &lower.$field,
                            row_bytes,
                            &upper.$field,
                            u64::from(upper.tile.interior_rows + 1) * row_bytes,
                            row_bytes,
                        );
                    }
                };
            }

            for iter in 0..iterations {
                if iter.is_multiple_of(2) {
                    synchronize_halos!(height_a);
                } else {
                    synchronize_halos!(height_b);
                }
                // Phase 1: Flow accumulation with the current height and water buffers.
                for sub in 0..flow_sub_iterations {
                    if sub.is_multiple_of(2) {
                        synchronize_halos!(water_a);
                    } else {
                        synchronize_halos!(water_b);
                    }
                    for tile in &tile_buffers {
                        let (input_height, output_height, water_in, water_out) =
                            match (iter.is_multiple_of(2), sub.is_multiple_of(2)) {
                                (true, true) => {
                                    (&tile.height_a, &tile.height_b, &tile.water_a, &tile.water_b)
                                }
                                (true, false) => {
                                    (&tile.height_a, &tile.height_b, &tile.water_b, &tile.water_a)
                                }
                                (false, true) => {
                                    (&tile.height_b, &tile.height_a, &tile.water_a, &tile.water_b)
                                }
                                (false, false) => {
                                    (&tile.height_b, &tile.height_a, &tile.water_b, &tile.water_a)
                                }
                            };
                        let flow_bg = bind_group(
                            "flow tile",
                            tile,
                            input_height,
                            output_height,
                            water_in,
                            water_out,
                        );
                        let mut pass = encoder.begin_compute_pass(&wgpu::ComputePassDescriptor {
                            label: Some("flow pass"),
                            timestamp_writes: None,
                        });
                        pass.set_pipeline(&self.flow_pipeline);
                        pass.set_bind_group(0, &flow_bg, &[]);
                        pass.dispatch_workgroups(
                            res.div_ceil(16),
                            (tile.tile.interior_rows + 2).div_ceil(16),
                            1,
                        );
                    }
                }

                // Phase 2: Erosion — reads final water, writes new height
                for tile in &tile_buffers {
                    let (input_height, output_height, water_in, water_out) = match (
                        iter.is_multiple_of(2),
                        flow_sub_iterations.is_multiple_of(2),
                    ) {
                        (true, true) => {
                            (&tile.height_a, &tile.height_b, &tile.water_a, &tile.water_b)
                        }
                        (true, false) => {
                            (&tile.height_a, &tile.height_b, &tile.water_b, &tile.water_a)
                        }
                        (false, true) => {
                            (&tile.height_b, &tile.height_a, &tile.water_a, &tile.water_b)
                        }
                        (false, false) => {
                            (&tile.height_b, &tile.height_a, &tile.water_b, &tile.water_a)
                        }
                    };
                    let erode_bg = bind_group(
                        "erode tile",
                        tile,
                        input_height,
                        output_height,
                        water_in,
                        water_out,
                    );
                    let mut pass = encoder.begin_compute_pass(&wgpu::ComputePassDescriptor {
                        label: Some("erode pass"),
                        timestamp_writes: None,
                    });
                    pass.set_pipeline(&self.erode_pipeline);
                    pass.set_bind_group(0, &erode_bg, &[]);
                    pass.dispatch_workgroups(
                        res.div_ceil(16),
                        (tile.tile.interior_rows + 2).div_ceil(16),
                        1,
                    );
                }
            }

            let staging: Vec<_> = tile_buffers
                .iter()
                .map(|tile| {
                    let size = u64::from(res)
                        * u64::from(tile.tile.interior_rows + 2)
                        * std::mem::size_of::<f32>() as u64;
                    let staging = gpu.device.create_buffer(&wgpu::BufferDescriptor {
                        label: Some("erosion staging"),
                        size,
                        usage: wgpu::BufferUsages::MAP_READ | wgpu::BufferUsages::COPY_DST,
                        mapped_at_creation: false,
                    });
                    let result = if iterations.is_multiple_of(2) {
                        &tile.height_a
                    } else {
                        &tile.height_b
                    };
                    encoder.copy_buffer_to_buffer(result, 0, &staging, 0, size);
                    staging
                })
                .collect();
            gpu.queue.submit(Some(encoder.finish()));

            let (sender, receiver) = std::sync::mpsc::channel();
            for buffer in &staging {
                let sender = sender.clone();
                buffer
                    .slice(..)
                    .map_async(wgpu::MapMode::Read, move |result| {
                        let _ = sender.send(result.map_err(|error| error.to_string()));
                    });
            }
            drop(sender);
            for _ in &staging {
                loop {
                    if cancel.load(std::sync::atomic::Ordering::Relaxed) {
                        return Err(ErosionError::ReadbackCancelled);
                    }
                    match receiver.try_recv() {
                        Ok(Ok(())) => break,
                        Ok(Err(error)) => return Err(ErosionError::ReadbackFailed(error)),
                        Err(std::sync::mpsc::TryRecvError::Disconnected) => {
                            return Err(ErosionError::ReadbackFailed(
                                "callback disconnected".into(),
                            ));
                        }
                        Err(std::sync::mpsc::TryRecvError::Empty) => {
                            let _ = gpu.device.poll(wgpu::PollType::Poll);
                            std::thread::sleep(std::time::Duration::from_millis(1));
                        }
                    }
                }
            }

            let mut face = vec![0.0; (res * res) as usize];
            for (tile, staging) in tile_buffers.iter().zip(&staging) {
                let mapped = staging.slice(..).get_mapped_range();
                let values: &[f32] = bytemuck::cast_slice(&mapped);
                for row in 0..tile.tile.interior_rows as usize {
                    let source = (row + 1) * res as usize;
                    let destination = (tile.tile.row_offset as usize + row) * res as usize;
                    face[destination..destination + res as usize]
                        .copy_from_slice(&values[source..source + res as usize]);
                }
                drop(mapped);
                staging.unmap();
            }
            terrain.faces[face_idx] = face;
        }
        Ok(())
    }
}

// ============ Wind Field Pipeline ============
// Analytical circulation and pressure from terrain + continentality.
// 4-mode compute shader: init_cont → smooth_cont → pressure → wind.

#[repr(C)]
#[derive(Copy, Clone, Pod, Zeroable)]
pub struct WindFieldParams {
    pub face: u32,
    pub resolution: u32,
    pub mode: u32,
    pub seed: u32,
    pub ocean_level: f32,
    pub axial_tilt_rad: f32,
    pub season: f32,
    pub smooth_weight: f32,
    pub rotation_rate: f32, // relative to Earth (1.0 = 24h)
    pub base_temp_c: f32,   // planet mean temperature °C
    pub atm_pressure: f32,  // atmospheric pressure in bar (1.0 = Earth)
    /// FE-084: wind speed multiplier; scales mesoscale steering above ws=1.
    pub wind_scale: f32,
}

pub struct WindField {
    pub wind: Vec<f32>, // 3 * 6 * res² floats (3D tangent vectors, all faces packed)
    pub continentality: [Vec<f32>; 6], // per-face continentality [0,1]
    pub pressure: [Vec<f32>; 6], // per-face pressure (hPa deviation)
    pub resolution: u32,
}

pub struct WindFieldPipeline {
    pipeline: wgpu::ComputePipeline,
    bind_group_layout: wgpu::BindGroupLayout,
}

pub struct DynamicsTextures {
    _wind_continentality: wgpu::Texture,
    _pressure: wgpu::Texture,
    wind_continentality_storage: wgpu::TextureView,
    pressure_storage: wgpu::TextureView,
    pub wind_continentality: wgpu::TextureView,
    pub pressure: wgpu::TextureView,
    pub resolution: u32,
    nearly_all_ocean: std::cell::Cell<bool>,
}

impl DynamicsTextures {
    pub(crate) fn is_nearly_all_ocean(&self) -> bool {
        self.nearly_all_ocean.get()
    }
}

fn terrain_is_nearly_all_ocean(terrain: &TectonicTerrain, ocean_level: f32) -> bool {
    terrain
        .faces
        .iter()
        .flatten()
        .all(|height| *height <= ocean_level)
}

impl WindFieldPipeline {
    pub fn new(gpu: &GpuContext) -> Result<Self, String> {
        let features = gpu.rgba16float_features;
        let required = wgpu::TextureUsages::STORAGE_BINDING | wgpu::TextureUsages::TEXTURE_BINDING;
        if !features.allowed_usages.contains(required)
            || !features
                .flags
                .contains(wgpu::TextureFormatFeatureFlags::FILTERABLE)
        {
            return Err(format!(
                "GPU adapter '{}' does not support filterable Rgba16Float storage cubemaps",
                gpu.adapter_name()
            ));
        }
        let src = format!(
            "{}\n{}\n{}",
            include_str!("shaders/cube_sphere.wgsl"),
            include_str!("shaders/noise.wgsl"),
            include_str!("shaders/wind_field.wgsl"),
        );
        let shader = gpu
            .device
            .create_shader_module(wgpu::ShaderModuleDescriptor {
                label: Some("wind field shader"),
                source: wgpu::ShaderSource::Wgsl(src.into()),
            });

        let bgl = gpu
            .device
            .create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
                label: Some("wind field bgl"),
                entries: &[
                    wgpu::BindGroupLayoutEntry {
                        binding: 0,
                        visibility: wgpu::ShaderStages::COMPUTE,
                        ty: wgpu::BindingType::Buffer {
                            ty: wgpu::BufferBindingType::Uniform,
                            has_dynamic_offset: false,
                            min_binding_size: None,
                        },
                        count: None,
                    },
                    wgpu::BindGroupLayoutEntry {
                        binding: 1,
                        visibility: wgpu::ShaderStages::COMPUTE,
                        ty: wgpu::BindingType::Buffer {
                            ty: wgpu::BufferBindingType::Storage { read_only: true },
                            has_dynamic_offset: false,
                            min_binding_size: None,
                        },
                        count: None,
                    },
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
                    wgpu::BindGroupLayoutEntry {
                        binding: 4,
                        visibility: wgpu::ShaderStages::COMPUTE,
                        ty: wgpu::BindingType::StorageTexture {
                            access: wgpu::StorageTextureAccess::WriteOnly,
                            format: wgpu::TextureFormat::Rgba16Float,
                            view_dimension: wgpu::TextureViewDimension::D2Array,
                        },
                        count: None,
                    },
                ],
            });
        let layout = gpu
            .device
            .create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
                label: Some("wind field layout"),
                bind_group_layouts: &[&bgl],
                push_constant_ranges: &[],
            });
        let pipeline = gpu
            .device
            .create_compute_pipeline(&wgpu::ComputePipelineDescriptor {
                label: Some("wind field pipeline"),
                layout: Some(&layout),
                module: &shader,
                entry_point: Some("main"),
                compilation_options: Default::default(),
                cache: None,
            });
        Ok(Self {
            pipeline,
            bind_group_layout: bgl,
        })
    }

    pub fn create_textures(
        &self,
        gpu: &GpuContext,
        resolution: u32,
        terrain: &TectonicTerrain,
        ocean_level: f32,
    ) -> DynamicsTextures {
        let textures = self.create_empty_textures(gpu, resolution);
        textures
            .nearly_all_ocean
            .set(terrain_is_nearly_all_ocean(terrain, ocean_level));
        textures
    }

    fn create_empty_textures(&self, gpu: &GpuContext, resolution: u32) -> DynamicsTextures {
        let create = |label| {
            gpu.device.create_texture(&wgpu::TextureDescriptor {
                label: Some(label),
                size: wgpu::Extent3d {
                    width: resolution,
                    height: resolution,
                    depth_or_array_layers: 6,
                },
                mip_level_count: 1,
                sample_count: 1,
                dimension: wgpu::TextureDimension::D2,
                format: wgpu::TextureFormat::Rgba16Float,
                usage: wgpu::TextureUsages::STORAGE_BINDING
                    | wgpu::TextureUsages::TEXTURE_BINDING
                    | wgpu::TextureUsages::COPY_SRC
                    | wgpu::TextureUsages::COPY_DST,
                view_formats: &[],
            })
        };
        let wind = create("wind and continentality cubemap");
        let pressure = create("pressure cubemap");
        let storage = |texture: &wgpu::Texture| {
            texture.create_view(&wgpu::TextureViewDescriptor {
                dimension: Some(wgpu::TextureViewDimension::D2Array),
                ..Default::default()
            })
        };
        let cube = |texture: &wgpu::Texture| {
            texture.create_view(&wgpu::TextureViewDescriptor {
                dimension: Some(wgpu::TextureViewDimension::Cube),
                ..Default::default()
            })
        };
        DynamicsTextures {
            wind_continentality_storage: storage(&wind),
            pressure_storage: storage(&pressure),
            wind_continentality: cube(&wind),
            pressure: cube(&pressure),
            _wind_continentality: wind,
            _pressure: pressure,
            resolution,
            nearly_all_ocean: std::cell::Cell::new(false),
        }
    }

    // ponytail: narrow diagnostics path used by sweep validation and weather tests only
    pub fn create_test_textures(
        &self,
        gpu: &GpuContext,
        resolution: u32,
        field: impl Fn([f32; 3]) -> ([f32; 4], f32),
    ) -> DynamicsTextures {
        let textures = self.create_empty_textures(gpu, resolution);
        let mut nearly_all_ocean = true;
        for face in 0..6 {
            let mut wind = Vec::with_capacity((resolution * resolution * 4) as usize);
            let mut pressure = Vec::with_capacity((resolution * resolution * 4) as usize);
            for y in 0..resolution {
                for x in 0..resolution {
                    let pos = crate::cube_sphere::cube_to_sphere(
                        face,
                        x as f32 / (resolution - 1) as f32,
                        y as f32 / (resolution - 1) as f32,
                    );
                    let (wind_value, pressure_value) = field(pos);
                    nearly_all_ocean &= wind_value[3] <= 0.01;
                    wind.extend(wind_value.map(|value| half::f16::from_f32(value).to_bits()));
                    pressure.extend(
                        [pressure_value, 0.0, 0.0, 0.0]
                            .map(|value| half::f16::from_f32(value).to_bits()),
                    );
                }
            }
            for (texture, data) in [
                (&textures._wind_continentality, &wind),
                (&textures._pressure, &pressure),
            ] {
                gpu.queue.write_texture(
                    wgpu::TexelCopyTextureInfo {
                        texture,
                        mip_level: 0,
                        origin: wgpu::Origin3d {
                            x: 0,
                            y: 0,
                            z: face,
                        },
                        aspect: wgpu::TextureAspect::All,
                    },
                    bytemuck::cast_slice(data),
                    wgpu::TexelCopyBufferLayout {
                        offset: 0,
                        bytes_per_row: Some(resolution * 8),
                        rows_per_image: Some(resolution),
                    },
                    wgpu::Extent3d {
                        width: resolution,
                        height: resolution,
                        depth_or_array_layers: 1,
                    },
                );
            }
        }
        textures.nearly_all_ocean.set(nearly_all_ocean);
        textures
    }

    #[allow(clippy::too_many_arguments)]
    fn dispatch_mode(
        &self,
        gpu: &GpuContext,
        mode: u32,
        face: u32,
        resolution: u32,
        seed: u32,
        ocean_level: f32,
        axial_tilt_rad: f32,
        season: f32,
        smooth_weight: f32,
        earth_relative_rotation_rate: f32,
        base_temp_c: f32,
        atm_pressure: f32,
        wind_scale: f32,
        src_buf: &wgpu::Buffer,
        dst_buf: &wgpu::Buffer,
        height_buf: &wgpu::Buffer,
        output_view: &wgpu::TextureView,
    ) {
        let p = WindFieldParams {
            face,
            resolution,
            mode,
            seed,
            ocean_level,
            axial_tilt_rad,
            season,
            smooth_weight,
            rotation_rate: earth_relative_rotation_rate,
            base_temp_c,
            atm_pressure,
            wind_scale,
        };
        let p_buf = gpu
            .device
            .create_buffer_init(&wgpu::util::BufferInitDescriptor {
                label: Some("wind p"),
                contents: bytemuck::bytes_of(&p),
                usage: wgpu::BufferUsages::UNIFORM,
            });
        let bg = gpu.device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: None,
            layout: &self.bind_group_layout,
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: p_buf.as_entire_binding(),
                },
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: src_buf.as_entire_binding(),
                },
                wgpu::BindGroupEntry {
                    binding: 2,
                    resource: dst_buf.as_entire_binding(),
                },
                wgpu::BindGroupEntry {
                    binding: 3,
                    resource: height_buf.as_entire_binding(),
                },
                wgpu::BindGroupEntry {
                    binding: 4,
                    resource: wgpu::BindingResource::TextureView(output_view),
                },
            ],
        });
        let wg = resolution.div_ceil(16);
        let mut enc = gpu.device.create_command_encoder(&Default::default());
        {
            let mut pass = enc.begin_compute_pass(&Default::default());
            pass.set_pipeline(&self.pipeline);
            pass.set_bind_group(0, &bg, &[]);
            pass.dispatch_workgroups(wg, wg, 1);
        }
        gpu.queue.submit(std::iter::once(enc.finish()));
    }

    #[allow(clippy::too_many_arguments)]
    pub fn generate(
        &self,
        gpu: &GpuContext,
        terrain: &TectonicTerrain,
        resolution: u32,
        seed: u32,
        ocean_level: f32,
        axial_tilt_rad: f32,
        season: f32,
        earth_relative_rotation_rate: f32,
        base_temp_c: f32,
        atm_pressure: f32,
        wind_scale: f32,
    ) -> WindField {
        let ppf = (resolution * resolution) as usize;
        let total_1c = 6 * ppf; // 1-component buffer (continentality, pressure)
        let total_3c = 3 * total_1c; // 3-component buffer (wind vectors)
        let textures = self.create_textures(gpu, resolution, terrain, ocean_level);

        // Pack all 6 faces of height into one buffer
        let mut all_height = vec![0.0f32; total_1c];
        for (i, face) in terrain.faces.iter().enumerate() {
            // Resample terrain to wind field resolution if needed
            let src_res = (face.len() as f32).sqrt() as usize;
            for y in 0..resolution as usize {
                for x in 0..resolution as usize {
                    let sx = x * src_res / resolution as usize;
                    let sy = y * src_res / resolution as usize;
                    all_height[i * ppf + y * resolution as usize + x] = face[sy * src_res + sx];
                }
            }
        }
        let height_buf = gpu
            .device
            .create_buffer_init(&wgpu::util::BufferInitDescriptor {
                label: Some("wind height"),
                contents: bytemuck::cast_slice(&all_height),
                usage: wgpu::BufferUsages::STORAGE,
            });

        let buf_a = gpu.device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("wind A"),
            size: (total_1c * 4) as u64,
            usage: wgpu::BufferUsages::STORAGE
                | wgpu::BufferUsages::COPY_SRC
                | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        let buf_b = gpu.device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("wind B"),
            size: (total_1c * 4) as u64,
            usage: wgpu::BufferUsages::STORAGE
                | wgpu::BufferUsages::COPY_SRC
                | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        // Wind output needs 3× space (3D vectors)
        let wind_buf = gpu.device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("wind out"),
            size: (total_3c * 4) as u64,
            usage: wgpu::BufferUsages::STORAGE | wgpu::BufferUsages::COPY_SRC,
            mapped_at_creation: false,
        });

        // === Phase 1: Continentality ===
        // Mode 0: Init (ocean=0, land=1) → buf_a
        for face in 0..6u32 {
            self.dispatch_mode(
                gpu,
                0,
                face,
                resolution,
                seed,
                ocean_level,
                axial_tilt_rad,
                season,
                0.0,
                earth_relative_rotation_rate,
                base_temp_c,
                atm_pressure,
                wind_scale,
                &buf_b,
                &buf_a,
                &height_buf,
                &textures.wind_continentality_storage,
            );
        }

        // Mode 1: Smooth (80 iterations at weight 0.22, ping-pong buf_a ↔ buf_b)
        // Wider diffusion (was 40 @ 0.15) prevents hard cloud edges at coastlines.
        // Effective gradient width ≈ sqrt(80 * 0.22 * 2) ≈ 6 texels → ~1.4° on 384px cubemap.
        let mut src_is_a = true;
        for _ in 0..80 {
            for face in 0..6u32 {
                let (s, d) = if src_is_a {
                    (&buf_a, &buf_b)
                } else {
                    (&buf_b, &buf_a)
                };
                self.dispatch_mode(
                    gpu,
                    1,
                    face,
                    resolution,
                    seed,
                    ocean_level,
                    axial_tilt_rad,
                    season,
                    0.22,
                    earth_relative_rotation_rate,
                    base_temp_c,
                    atm_pressure,
                    wind_scale,
                    s,
                    d,
                    &height_buf,
                    &textures.wind_continentality_storage,
                );
            }
            src_is_a = !src_is_a;
        }

        // Read back continentality
        let cont_result = if src_is_a { &buf_a } else { &buf_b };
        let cont_data = self.readback_1c(gpu, cont_result, total_1c);

        // === Phase 2: Pressure ===
        // Continentality is in cont_result, pressure goes to the other buffer
        let pressure_dst = if src_is_a { &buf_b } else { &buf_a };
        for face in 0..6u32 {
            self.dispatch_mode(
                gpu,
                2,
                face,
                resolution,
                seed,
                ocean_level,
                axial_tilt_rad,
                season,
                0.0,
                earth_relative_rotation_rate,
                base_temp_c,
                atm_pressure,
                wind_scale,
                cont_result,
                pressure_dst,
                &height_buf,
                &textures.pressure_storage,
            );
        }

        // Read back pressure
        let pressure_data = self.readback_1c(gpu, pressure_dst, total_1c);

        // === Phase 3: Direct analytical wind (reads continentality for monsoon effects) ===
        for face in 0..6u32 {
            self.dispatch_mode(
                gpu,
                3,
                face,
                resolution,
                seed,
                ocean_level,
                axial_tilt_rad,
                season,
                0.0,
                earth_relative_rotation_rate,
                base_temp_c,
                atm_pressure,
                wind_scale,
                cont_result,
                &wind_buf,
                &height_buf,
                &textures.wind_continentality_storage,
            );
        }

        // Read back wind (3-component)
        let wind_data = self.readback_3c(gpu, &wind_buf, total_3c);

        // Split into per-face arrays
        let mut cont_faces: [Vec<f32>; 6] = Default::default();
        let mut press_faces: [Vec<f32>; 6] = Default::default();
        for i in 0..6 {
            cont_faces[i] = cont_data[i * ppf..(i + 1) * ppf].to_vec();
            press_faces[i] = pressure_data[i * ppf..(i + 1) * ppf].to_vec();
        }

        WindField {
            wind: wind_data,
            continentality: cont_faces,
            pressure: press_faces,
            resolution,
        }
    }

    #[allow(clippy::too_many_arguments)]
    pub fn generate_gpu(
        &self,
        gpu: &GpuContext,
        terrain: &TectonicTerrain,
        textures: &DynamicsTextures,
        seed: u32,
        ocean_level: f32,
        axial_tilt_rad: f32,
        season: f32,
        earth_relative_rotation_rate: f32,
        base_temp_c: f32,
        atm_pressure: f32,
        wind_scale: f32,
    ) {
        textures
            .nearly_all_ocean
            .set(terrain_is_nearly_all_ocean(terrain, ocean_level));
        let resolution = textures.resolution;
        let ppf = (resolution * resolution) as usize;
        let total = 6 * ppf;
        let mut all_height = vec![0.0f32; total];
        for (face_index, face) in terrain.faces.iter().enumerate() {
            let source_resolution = (face.len() as f32).sqrt() as usize;
            for y in 0..resolution as usize {
                for x in 0..resolution as usize {
                    let source_x = x * source_resolution / resolution as usize;
                    let source_y = y * source_resolution / resolution as usize;
                    all_height[face_index * ppf + y * resolution as usize + x] =
                        face[source_y * source_resolution + source_x];
                }
            }
        }
        let height = gpu
            .device
            .create_buffer_init(&wgpu::util::BufferInitDescriptor {
                label: Some("wind height"),
                contents: bytemuck::cast_slice(&all_height),
                usage: wgpu::BufferUsages::STORAGE,
            });
        let buffer = |label| {
            gpu.device.create_buffer(&wgpu::BufferDescriptor {
                label: Some(label),
                size: (total * std::mem::size_of::<f32>()) as u64,
                usage: wgpu::BufferUsages::STORAGE,
                mapped_at_creation: false,
            })
        };
        let a = buffer("wind A");
        let b = buffer("wind B");
        let wind = gpu.device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("wind output"),
            size: (total * 3 * std::mem::size_of::<f32>()) as u64,
            usage: wgpu::BufferUsages::STORAGE,
            mapped_at_creation: false,
        });

        for face in 0..6 {
            self.dispatch_mode(
                gpu,
                0,
                face,
                resolution,
                seed,
                ocean_level,
                axial_tilt_rad,
                season,
                0.0,
                earth_relative_rotation_rate,
                base_temp_c,
                atm_pressure,
                wind_scale,
                &b,
                &a,
                &height,
                &textures.wind_continentality_storage,
            );
        }
        let mut source_is_a = true;
        for _ in 0..80 {
            for face in 0..6 {
                let (source, destination) = if source_is_a { (&a, &b) } else { (&b, &a) };
                self.dispatch_mode(
                    gpu,
                    1,
                    face,
                    resolution,
                    seed,
                    ocean_level,
                    axial_tilt_rad,
                    season,
                    0.22,
                    earth_relative_rotation_rate,
                    base_temp_c,
                    atm_pressure,
                    wind_scale,
                    source,
                    destination,
                    &height,
                    &textures.wind_continentality_storage,
                );
            }
            source_is_a = !source_is_a;
        }
        let continentality = if source_is_a { &a } else { &b };
        let pressure = if source_is_a { &b } else { &a };
        for face in 0..6 {
            self.dispatch_mode(
                gpu,
                2,
                face,
                resolution,
                seed,
                ocean_level,
                axial_tilt_rad,
                season,
                0.0,
                earth_relative_rotation_rate,
                base_temp_c,
                atm_pressure,
                wind_scale,
                continentality,
                pressure,
                &height,
                &textures.pressure_storage,
            );
            self.dispatch_mode(
                gpu,
                3,
                face,
                resolution,
                seed,
                ocean_level,
                axial_tilt_rad,
                season,
                0.0,
                earth_relative_rotation_rate,
                base_temp_c,
                atm_pressure,
                wind_scale,
                continentality,
                &wind,
                &height,
                &textures.wind_continentality_storage,
            );
        }
    }

    fn readback_1c(&self, gpu: &GpuContext, buf: &wgpu::Buffer, total: usize) -> Vec<f32> {
        let size = (total * 4) as u64;
        let staging = gpu.device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("wind staging"),
            size,
            usage: wgpu::BufferUsages::MAP_READ | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        let mut enc = gpu.device.create_command_encoder(&Default::default());
        enc.copy_buffer_to_buffer(buf, 0, &staging, 0, size);
        gpu.queue.submit(std::iter::once(enc.finish()));

        let slice = staging.slice(..);
        let (tx, rx) = std::sync::mpsc::channel();
        slice.map_async(wgpu::MapMode::Read, move |r| {
            let _ = tx.send(r);
        });
        let _ = gpu.device.poll(wgpu::PollType::Wait {
            submission_index: None,
            timeout: None,
        });
        rx.recv().unwrap().unwrap();

        let data = slice.get_mapped_range();
        bytemuck::cast_slice::<u8, f32>(&data).to_vec()
    }

    fn readback_3c(&self, gpu: &GpuContext, buf: &wgpu::Buffer, total: usize) -> Vec<f32> {
        self.readback_1c(gpu, buf, total) // same logic, different size
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::gpu::GpuContext;
    use crate::plates::{PlateGenParams, generate_plates};

    fn flat_terrain(resolution: u32) -> TectonicTerrain {
        TectonicTerrain {
            faces: std::array::from_fn(|face| {
                vec![if face % 2 == 0 { 0.2 } else { -0.2 }; (resolution * resolution) as usize]
            }),
            resolution,
        }
    }

    fn sloped_terrain(resolution: u32) -> TectonicTerrain {
        TectonicTerrain {
            faces: std::array::from_fn(|_| {
                (0..resolution * resolution)
                    .map(|index| (index / resolution + index % resolution) as f32 * 0.02)
                    .collect()
            }),
            resolution,
        }
    }

    #[test]
    fn erosion_tiles_keep_halo_bindings_within_device_limit() {
        let limit = 134_217_728;
        let tiles = erosion_tiles(8192, limit).expect("128 MiB should support tiled 8K erosion");
        assert!(tiles.len() > 1);
        assert_eq!(tiles.first().unwrap().row_offset, 0);
        assert_eq!(
            tiles.iter().map(|tile| tile.interior_rows).sum::<u32>(),
            8192
        );
        assert!(
            tiles.iter().all(|tile| {
                u64::from(8192_u32) * u64::from(tile.interior_rows + 2) * 4 <= limit
            })
        );

        let limits = wgpu::Limits {
            max_storage_buffer_binding_size: limit as u32,
            max_buffer_size: limit,
            ..Default::default()
        };
        let expected = tiles
            .iter()
            .map(|tile| u64::from(8192_u32) * u64::from(tile.interior_rows + 2) * 4 * 5)
            .sum::<u64>();
        assert_eq!(
            ErosionPipeline::aggregate_tiled_bytes(8192, &limits).unwrap(),
            expected
        );
    }

    #[test]
    fn erosion_tiles_return_a_structured_error_when_one_row_cannot_fit() {
        assert!(matches!(
            erosion_tiles(8192, 8192 * 4),
            Err(ErosionError::InsufficientStorageBinding { .. })
        ));
    }

    #[test]
    fn erosion_readback_observes_cancellation() {
        let gpu = GpuContext::new().expect("GPU init failed");
        let pipeline = ErosionPipeline::new(&gpu);
        let mut terrain = sloped_terrain(16);
        let cancel = std::sync::atomic::AtomicBool::new(true);
        assert!(matches!(
            pipeline.erode_with_cancel(&gpu, &mut terrain, 1, -1.0, &cancel),
            Err(ErosionError::ReadbackCancelled)
        ));
    }

    #[test]
    fn erosion_ping_pong_uses_latest_iteration() {
        let gpu = GpuContext::new().expect("GPU init failed");
        let pipeline = ErosionPipeline::new(&gpu);
        let mut once = sloped_terrain(16);
        let mut twice = sloped_terrain(16);

        pipeline.erode(&gpu, &mut once, 1, -1.0).unwrap();
        pipeline.erode(&gpu, &mut twice, 2, -1.0).unwrap();

        assert_ne!(once.faces, twice.faces);
        assert!(
            twice
                .faces
                .iter()
                .flatten()
                .zip(sloped_terrain(16).faces.iter().flatten())
                .any(|(eroded, original)| eroded != original)
        );
    }

    #[test]
    fn zero_erosion_iterations_preserve_terrain() {
        let gpu = GpuContext::new().expect("GPU init failed");
        let pipeline = ErosionPipeline::new(&gpu);
        let mut terrain = sloped_terrain(16);
        let original = sloped_terrain(16);

        pipeline.erode(&gpu, &mut terrain, 0, -1.0).unwrap();

        assert_eq!(terrain.faces, original.faces);
    }

    fn read_texture(gpu: &GpuContext, texture: &wgpu::Texture, resolution: u32) -> Vec<f32> {
        let unpadded_bytes_per_row = resolution * 8;
        let bytes_per_row = unpadded_bytes_per_row.div_ceil(256) * 256;
        let buffer = gpu.device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("dynamics test readback"),
            size: (bytes_per_row * resolution * 6) as u64,
            usage: wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::MAP_READ,
            mapped_at_creation: false,
        });
        let mut encoder = gpu.device.create_command_encoder(&Default::default());
        encoder.copy_texture_to_buffer(
            texture.as_image_copy(),
            wgpu::TexelCopyBufferInfo {
                buffer: &buffer,
                layout: wgpu::TexelCopyBufferLayout {
                    offset: 0,
                    bytes_per_row: Some(bytes_per_row),
                    rows_per_image: Some(resolution),
                },
            },
            wgpu::Extent3d {
                width: resolution,
                height: resolution,
                depth_or_array_layers: 6,
            },
        );
        gpu.queue.submit(Some(encoder.finish()));
        buffer.slice(..).map_async(wgpu::MapMode::Read, |_| {});
        gpu.device
            .poll(wgpu::PollType::Wait {
                submission_index: None,
                timeout: None,
            })
            .unwrap();
        let mapped = buffer.slice(..).get_mapped_range();
        let mut values = Vec::with_capacity((resolution * resolution * 6 * 4) as usize);
        for row in mapped.chunks_exact(bytes_per_row as usize) {
            values.extend(
                row[..unpadded_bytes_per_row as usize]
                    .chunks_exact(2)
                    .map(|bytes| {
                        half::f16::from_bits(u16::from_le_bytes([bytes[0], bytes[1]])).to_f32()
                    }),
            );
        }
        values
    }

    #[test]
    fn test_gpu_dynamics_are_packed_deterministic_and_finite() {
        let gpu = GpuContext::new().expect("GPU init failed");
        let pipeline = WindFieldPipeline::new(&gpu).expect("Rgba16Float dynamics unsupported");
        let terrain = flat_terrain(16);
        let generate = |textures: &DynamicsTextures| {
            pipeline.generate_gpu(
                &gpu, &terrain, textures, 42, 0.0, 0.4, 0.5, 1.0, 15.0, 1.0, 1.0,
            );
        };
        let first = pipeline.create_textures(&gpu, 16, &terrain, 0.0);
        let second = pipeline.create_textures(&gpu, 16, &terrain, 0.0);
        generate(&first);
        generate(&second);

        let first_wind = read_texture(&gpu, &first._wind_continentality, 16);
        let second_wind = read_texture(&gpu, &second._wind_continentality, 16);
        let pressure = read_texture(&gpu, &first._pressure, 16);
        assert_eq!(first_wind, second_wind);
        assert!(first_wind.iter().all(|value| value.is_finite()));
        assert!(pressure.iter().all(|value| value.is_finite()));
        assert!(
            first_wind
                .chunks_exact(4)
                .all(|pixel| (0.0..=1.0).contains(&pixel[3]))
        );
        assert!(
            pressure
                .chunks_exact(4)
                .all(|pixel| (900.0..1100.0).contains(&pixel[0]))
        );
    }

    #[test]
    fn test_season_transition_is_continuous_around_equinox() {
        let gpu = GpuContext::new().expect("GPU init failed");
        let pipeline = WindFieldPipeline::new(&gpu).expect("Rgba16Float dynamics unsupported");
        let terrain = flat_terrain(8);
        let fields: Vec<_> = [0.49, 0.50, 0.51]
            .into_iter()
            .map(|season| {
                pipeline.generate(&gpu, &terrain, 8, 42, 0.0, 0.4, season, 1.0, 15.0, 1.0, 1.0)
            })
            .collect();
        let mean_delta = |a: &WindField, b: &WindField| {
            a.pressure
                .iter()
                .flatten()
                .zip(b.pressure.iter().flatten())
                .map(|(a, b)| (a - b).abs())
                .sum::<f32>()
                / (6 * 8 * 8) as f32
        };
        assert!(mean_delta(&fields[0], &fields[1]) < 2.0);
        assert!(mean_delta(&fields[1], &fields[2]) < 2.0);
    }

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

        let all_heights: Vec<f32> = terrain
            .faces
            .iter()
            .flat_map(|f| f.iter().copied())
            .collect();
        let min_h = all_heights.iter().cloned().fold(f32::INFINITY, f32::min);
        let max_h = all_heights
            .iter()
            .cloned()
            .fold(f32::NEG_INFINITY, f32::max);

        // Continuous model should produce a meaningful height range
        assert!(
            max_h - min_h > 0.3,
            "Height range should be > 0.3, got {:.3} (min={:.3}, max={:.3})",
            max_h - min_h,
            min_h,
            max_h
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

        let all_heights: Vec<f32> = terrain
            .faces
            .iter()
            .flat_map(|f| f.iter().copied())
            .collect();
        let max_height = all_heights
            .iter()
            .cloned()
            .fold(f32::NEG_INFINITY, f32::max);

        // Should have elevated peaks from convergent boundary mountains
        assert!(
            max_height > 0.2,
            "Should have peaks > 0.2, max is {}",
            max_height
        );
    }

    #[test]
    fn terrain_profiles_have_continuous_contours_and_preserve_underlying_relief() {
        let gpu = GpuContext::new().unwrap();
        // Also compile the alternate JFA path with the shared profiles.
        let _multi_pass = MultiPassTerrainPipeline::new(&gpu);
        let source = format!(
            "{}\n{}",
            include_str!("shaders/terrain_profiles.wgsl"),
            r#"
@group(0) @binding(0) var<storage, read_write> result: array<vec4<f32>>;
@compute @workgroup_size(64)
fn main(@builtin(global_invocation_id) id: vec3<u32>) {
    if (id.x >= 257u) { return; }
    let x = f32(id.x) / 128.0 - 1.0;
    let distance = f32(id.x) / 128.0;
    result[id.x] = vec4<f32>(continental_elevation(x),
        hotspot_elevation(-0.5, distance, 1.0, 0.4),
        hotspot_elevation(0.2, distance, 1.0, 0.4),
        hotspot_elevation(-0.49, distance, 1.0, 0.4));
}"#
        );
        let shader = gpu
            .device
            .create_shader_module(wgpu::ShaderModuleDescriptor {
                label: Some("terrain profile continuity oracle"),
                source: wgpu::ShaderSource::Wgsl(source.into()),
            });
        let pipeline = gpu
            .device
            .create_compute_pipeline(&wgpu::ComputePipelineDescriptor {
                label: Some("terrain profile oracle"),
                layout: None,
                module: &shader,
                entry_point: Some("main"),
                compilation_options: Default::default(),
                cache: None,
            });
        let bytes = 257 * 16;
        let output = gpu.device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("terrain profile output"),
            size: bytes,
            usage: wgpu::BufferUsages::STORAGE | wgpu::BufferUsages::COPY_SRC,
            mapped_at_creation: false,
        });
        let readback = gpu.device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("terrain profile readback"),
            size: bytes,
            usage: wgpu::BufferUsages::MAP_READ | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        let bind = gpu.device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: None,
            layout: &pipeline.get_bind_group_layout(0),
            entries: &[wgpu::BindGroupEntry {
                binding: 0,
                resource: output.as_entire_binding(),
            }],
        });
        let mut encoder = gpu.device.create_command_encoder(&Default::default());
        {
            let mut pass = encoder.begin_compute_pass(&Default::default());
            pass.set_pipeline(&pipeline);
            pass.set_bind_group(0, &bind, &[]);
            pass.dispatch_workgroups(5, 1, 1);
        }
        encoder.copy_buffer_to_buffer(&output, 0, &readback, 0, bytes);
        gpu.queue.submit(Some(encoder.finish()));
        readback.slice(..).map_async(wgpu::MapMode::Read, |_| {});
        gpu.device
            .poll(wgpu::PollType::Wait {
                submission_index: None,
                timeout: None,
            })
            .unwrap();
        let mapped = readback.slice(..).get_mapped_range();
        let values: &[[f32; 4]] = bytemuck::cast_slice(&mapped);
        for (i, v) in values.iter().enumerate() {
            assert!(v.iter().all(|x| x.is_finite()));
            assert!(
                (v[0] + values[256 - i][0]).abs() < 1e-5,
                "odd continental curve"
            );
            assert!((v[3] - v[1] - 0.01).abs() < 1e-6, "retain seabed detail");
            assert!(
                (v[2] - v[1] - 0.7).abs() < 1e-6,
                "same uplift on land and sea"
            );
            if i >= 128 {
                assert_eq!(v[1], -0.5, "no uplift outside footprint");
            }
            if i > 0 {
                let slope = (v[0] - values[i - 1][0]) * 128.0;
                assert!(
                    (0.0..3.0).contains(&slope),
                    "bounded monotone slope: {slope}"
                );
            }
        }
        assert_eq!(values[128][0], 0.0);
        assert!((values[256][0] - 1.0).abs() < 1e-6);
        assert!((values[0][1] + 0.1).abs() < 1e-6, "peak is relative uplift");
        assert!(
            (values[127][1] - values[128][1]).abs() < 0.0001,
            "no cliff at hotspot perimeter"
        );
        for source in [
            include_str!("shaders/plates.wgsl"),
            include_str!("shaders/terrain_from_plates.wgsl"),
        ] {
            assert!(source.contains("continental_elevation(continental_raw)"));
            assert!(source.contains("height = hotspot_elevation("));
            assert!(!source.contains("height = max(height, volcano_h)"));
        }
    }

    #[test]
    fn wind_steering_preserves_weak_eddies_and_thermal_trough_meanders() {
        let gpu = GpuContext::new().unwrap();
        let source = format!(
            "{}\n{}\n{}\n{}",
            include_str!("shaders/cube_sphere.wgsl"),
            include_str!("shaders/noise.wgsl"),
            include_str!("shaders/wind_field.wgsl"),
            r#"
@compute @workgroup_size(64)
fn circulation_oracle(@builtin(global_invocation_id) id: vec3<u32>) {
    let i = id.x;
    let angle = f32(i) * 6.2831853 / 64.0;
    let pos = vec3<f32>(cos(angle), 0.0, sin(angle));
    dst[i * 4u] = thermal_trough_latitude(pos, 0.0);
    dst[i * 4u + 1u] = thermal_trough_latitude(pos, 1.0);
    let strength = f32(i) / 63.0;
    dst[i * 4u + 2u] = length(bounded_stream_steering(vec3<f32>(strength * 0.001, 0.0, 0.0)));
    dst[i * 4u + 3u] = length(bounded_stream_steering(vec3<f32>(strength * 100.0, 0.0, 0.0)));
}"#
        );
        let shader = gpu
            .device
            .create_shader_module(wgpu::ShaderModuleDescriptor {
                label: Some("circulation oracle"),
                source: wgpu::ShaderSource::Wgsl(source.into()),
            });
        let pipeline = gpu
            .device
            .create_compute_pipeline(&wgpu::ComputePipelineDescriptor {
                label: None,
                layout: None,
                module: &shader,
                entry_point: Some("circulation_oracle"),
                compilation_options: Default::default(),
                cache: None,
            });
        let output = gpu.device.create_buffer(&wgpu::BufferDescriptor {
            label: None,
            size: 1024,
            usage: wgpu::BufferUsages::STORAGE | wgpu::BufferUsages::COPY_SRC,
            mapped_at_creation: false,
        });
        let wind = WindFieldPipeline::new(&gpu).unwrap();
        let mut equinox = Vec::new();
        for season in [0.5, 1.0, 0.0] {
            let params = WindFieldParams {
                seed: 1042,
                axial_tilt_rad: 23.4_f32.to_radians(),
                season,
                ..WindFieldParams::zeroed()
            };
            let uniform = gpu
                .device
                .create_buffer_init(&wgpu::util::BufferInitDescriptor {
                    label: None,
                    contents: bytemuck::bytes_of(&params),
                    usage: wgpu::BufferUsages::UNIFORM,
                });
            let bind = gpu.device.create_bind_group(&wgpu::BindGroupDescriptor {
                label: None,
                layout: &pipeline.get_bind_group_layout(0),
                entries: &[
                    wgpu::BindGroupEntry {
                        binding: 0,
                        resource: uniform.as_entire_binding(),
                    },
                    wgpu::BindGroupEntry {
                        binding: 2,
                        resource: output.as_entire_binding(),
                    },
                ],
            });
            let mut encoder = gpu.device.create_command_encoder(&Default::default());
            {
                let mut pass = encoder.begin_compute_pass(&Default::default());
                pass.set_pipeline(&pipeline);
                pass.set_bind_group(0, &bind, &[]);
                pass.dispatch_workgroups(1, 1, 1);
            }
            gpu.queue.submit(Some(encoder.finish()));
            let values = wind.readback_1c(&gpu, &output, 256);
            assert!(values.iter().all(|v| v.is_finite()));
            let rows: Vec<_> = values.chunks_exact(4).collect();
            assert_eq!(rows[0][2], 0.0);
            assert_eq!(rows[0][3], 0.0);
            assert!(
                rows[63][2] < 0.00034,
                "weak eddy must not become unit strength"
            );
            assert!(rows[63][3] < 1.0 && rows[63][3] > 0.99);
            assert!(
                rows.windows(2)
                    .all(|p| p[1][2] > p[0][2] && p[1][3] > p[0][3])
            );
            if season == 0.5 {
                equinox = rows.iter().map(|r| r[0]).collect();
                let lo = equinox.iter().copied().fold(f32::INFINITY, f32::min);
                let hi = equinox.iter().copied().fold(f32::NEG_INFINITY, f32::max);
                assert!(hi - lo > 1.0, "trough must vary with longitude");
                assert!(rows.iter().all(|r| (r[0] - r[1]).abs() < 1e-5));
            } else {
                let sign = if season == 1.0 { 1.0 } else { -1.0 };
                for (i, row) in rows.iter().enumerate() {
                    assert!((row[0] - equinox[i] - sign * 5.0).abs() < 1e-4);
                    assert!((row[1] - row[0] - sign * 15.0).abs() < 1e-4);
                }
            }
        }
    }

    #[test]
    fn generated_wind_is_deterministic_tangent_and_continuous_at_cube_edges() {
        let gpu = GpuContext::new().unwrap();
        let pipeline = WindFieldPipeline::new(&gpu).unwrap();
        let res = 16;
        let terrain = TectonicTerrain {
            faces: std::array::from_fn(|_| vec![-0.2; (res * res) as usize]),
            resolution: res,
        };
        let generate = |seed| {
            pipeline.generate(
                &gpu,
                &terrain,
                res,
                seed,
                0.0,
                23.4_f32.to_radians(),
                0.5,
                1.0,
                15.0,
                1.0,
                1.0,
            )
        };
        let field = generate(1042);
        assert_eq!(field.wind, generate(1042).wind);
        assert_ne!(field.wind, generate(1137).wind);
        let mut seams = std::collections::HashMap::new();
        let mut seam_pairs = 0;
        for face in 0..6 {
            for y in 0..res {
                for x in 0..res {
                    let pos = crate::cube_sphere::cube_to_sphere(
                        face,
                        x as f32 / (res - 1) as f32,
                        y as f32 / (res - 1) as f32,
                    );
                    let index = ((face * res * res + y * res + x) * 3) as usize;
                    let v: [f32; 3] = field.wind[index..index + 3].try_into().unwrap();
                    assert!(v.iter().all(|c| c.is_finite()));
                    let radial: f32 = v.iter().zip(pos).map(|(a, b)| a * b).sum();
                    assert!(radial.abs() < 1e-5, "non-tangent wind: {radial}");
                    assert!(
                        v.iter().map(|c| c * c).sum::<f32>() < 4.0,
                        "unbounded speed"
                    );
                    if x == 0 || y == 0 || x == res - 1 || y == res - 1 {
                        let key = pos.map(|c| (c * 100000.0).round() as i32);
                        if let Some(previous) = seams.insert(key, v) {
                            seam_pairs += 1;
                            assert!(
                                previous.iter().zip(v).all(|(a, b)| (a - b).abs() < 1e-5),
                                "wind seam at {pos:?}"
                            );
                        }
                    }
                }
            }
        }
        assert!(seam_pairs > 100);
    }

    #[test]
    fn realism_rotation_units_and_area_weighting() {
        for (hours, ratio) in [(12.0, 2.0), (24.0, 1.0), (48.0, 0.5)] {
            assert!(
                (earth_relative_rotation_rate(std::f32::consts::TAU / (hours * 3600.0)) - ratio)
                    .abs()
                    < 1e-6
            );
        }
        let mut terrain = TectonicTerrain {
            resolution: 3,
            faces: std::array::from_fn(|_| vec![1.0; 9]),
        };
        assert_eq!(terrain.solid_angle_ocean_coverage(0.0), 0.0);
        assert_eq!(terrain.solid_angle_ocean_coverage(2.0), 1.0);
        for face in &mut terrain.faces {
            face[4] = -1.0;
        }
        assert!(
            terrain.solid_angle_ocean_coverage(0.0) > 1.0 / 9.0,
            "face centers cover more solid angle than corners"
        );
    }

    #[test]
    fn realism_active_tectonics_respects_motion_width_and_zero_activity() {
        let gpu = GpuContext::new().unwrap();
        let compute = TerrainComputePipeline::new(&gpu);
        let plates = generate_plates(&PlateGenParams {
            seed: 42,
            mass_earth: 1.0,
            ocean_fraction: 0.6,
            tectonics_factor: 0.85,
            continental_scale: 1.0,
            num_plates_override: 0,
            num_continents: 4,
            continent_size_variety: 0.35,
        });
        let mut stopped = plates.clone();
        for plate in &mut stopped {
            plate.velocity = [0.0; 3];
        }
        let generate = |plates: &[PlateGpu], width, activity| {
            compute.generate(
                &gpu, plates, 32, 42, 1.2, 1.5, 6, 0.6, 2.1, 1.0, width, 1.0, 1.0, 9.81, activity,
                0.2, 1.0,
            )
        };
        let active = generate(&plates, 0.1, 0.85);
        assert_eq!(active.faces, generate(&plates, 0.1, 0.85).faces);
        assert_ne!(active.faces, generate(&stopped, 0.1, 0.85).faces);
        assert_ne!(active.faces, generate(&plates, 0.25, 0.85).faces);
        assert_eq!(
            generate(&plates, 0.1, 0.0).faces,
            generate(&stopped, 0.25, 0.0).faces
        );
        assert!(active.faces.iter().flatten().all(|v| v.is_finite()));
    }
}
