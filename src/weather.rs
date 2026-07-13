use crate::gpu::GpuContext;
use crate::terrain_compute::{DynamicsTextures, TectonicTerrain};
use bytemuck::{Pod, Zeroable};
use half::f16;
use std::sync::mpsc;
use wgpu::util::DeviceExt;

/// Preview weather stays independent of viewport and export resolution.
pub const DEFAULT_WEATHER_RESOLUTION: u32 = 384;

/// Each `Rgba16Float` cubemap uses `6 * resolution^2 * 8` bytes.
/// Mass channels are low, deep, high, occupancy. Geometry channels are
/// base, low top, deep top, high top, all in kilometers.
#[repr(C)]
#[derive(Copy, Clone, Pod, Zeroable)]
pub struct WeatherSnapshot {
    pub face: u32,
    pub resolution: u32,
    pub seed: u32,
    pub storm_count: u32,
    pub coverage: f32,
    pub moisture: f32,
    pub surface_pressure_bar: f32,
    pub base_temp_c: f32,
    pub ocean_level: f32,
    pub axial_tilt_rad: f32,
    pub season: f32,
    pub storm_size: f32,
    pub radius_km: f32,
    pub rotation_rate_rad_s: f32,
    pub _pad0: f32,
}

const SPINUP_RESOLUTION: u32 = 128;
const SPINUP_ITERATIONS: usize = 16;

#[repr(C)]
#[derive(Copy, Clone, Pod, Zeroable)]
struct SpinupParams {
    spin_resolution: u32,
    output_resolution: u32,
    seed: u32,
    storm_count: u32,
    coverage: f32,
    moisture: f32,
    surface_pressure_bar: f32,
    base_temp_c: f32,
    ocean_level: f32,
    axial_tilt_rad: f32,
    season: f32,
    storm_size: f32,
    radius_km: f32,
    rotation_rate_rad_s: f32,
    diagnostic_flags: u32,
    _pad0: u32,
}

struct SpinupTexture {
    _texture: wgpu::Texture,
    sampled: wgpu::TextureView,
    storage: wgpu::TextureView,
}

pub struct WeatherTextures {
    _mass_texture: wgpu::Texture,
    _geometry_texture: wgpu::Texture,
    mass_storage: wgpu::TextureView,
    geometry_storage: wgpu::TextureView,
    pub mass: wgpu::TextureView,
    pub geometry: wgpu::TextureView,
    pub resolution: u32,
}

fn read_weather_cube_texture(
    gpu: &GpuContext,
    texture: &wgpu::Texture,
    resolution: u32,
) -> Vec<f32> {
    let unpadded_bytes_per_row = resolution * 8;
    let bytes_per_row = unpadded_bytes_per_row.div_ceil(256) * 256;
    let buffer = gpu.device.create_buffer(&wgpu::BufferDescriptor {
        label: Some("weather texture readback"),
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
    let slice = buffer.slice(..);
    let (tx, rx) = mpsc::channel();
    slice.map_async(wgpu::MapMode::Read, move |result| {
        let _ = tx.send(result);
    });
    gpu.device
        .poll(wgpu::PollType::Wait {
            submission_index: None,
            timeout: None,
        })
        .expect("weather texture readback poll failed");
    rx.recv()
        .expect("weather texture readback callback dropped")
        .expect("weather texture readback failed");
    let mapped = slice.get_mapped_range();
    mapped
        .chunks_exact(bytes_per_row as usize)
        .flat_map(|row| row[..unpadded_bytes_per_row as usize].chunks_exact(2))
        .map(|bytes| f16::from_bits(u16::from_le_bytes([bytes[0], bytes[1]])).to_f32())
        .collect()
}

pub fn read_weather_cube_texture_for_sweep(
    gpu: &GpuContext,
    texture: &wgpu::Texture,
    resolution: u32,
) -> Vec<f32> {
    read_weather_cube_texture(gpu, texture, resolution)
}

impl WeatherTextures {
    pub fn read_mass(&self, gpu: &GpuContext) -> Vec<f32> {
        read_weather_cube_texture(gpu, &self._mass_texture, self.resolution)
    }

    pub fn read_geometry(&self, gpu: &GpuContext) -> Vec<f32> {
        read_weather_cube_texture(gpu, &self._geometry_texture, self.resolution)
    }
}

#[derive(Default)]
struct RevisionState {
    requested: u64,
    submitted: Option<(u64, WeatherSnapshot)>,
    ready: Option<u64>,
    pending: Option<WeatherSnapshot>,
}

impl RevisionState {
    fn request(&mut self, snapshot: WeatherSnapshot) {
        self.requested += 1;
        self.pending = Some(snapshot);
    }

    fn next_submission(&mut self) -> Option<(u64, WeatherSnapshot)> {
        if self.submitted.is_some() {
            return None;
        }
        let snapshot = self.pending.take()?;
        self.submitted = Some((self.requested, snapshot));
        Some((self.requested, snapshot))
    }

    fn complete(&mut self, revision: u64) -> Option<WeatherSnapshot> {
        let (submitted_revision, snapshot) = self.submitted?;
        if submitted_revision != revision {
            return None;
        }
        self.submitted = None;
        if self.ready.is_some_and(|ready| revision <= ready) {
            return None;
        }
        self.ready = Some(revision);
        Some(snapshot)
    }

    fn is_busy(&self) -> bool {
        self.submitted.is_some() || self.pending.is_some()
    }
}

/// Double-buffered weather generation with a latest-wins publication gate.
pub struct WeatherLifecycle {
    front: WeatherTextures,
    back: WeatherTextures,
    front_is_a: bool,
    front_snapshot: Option<WeatherSnapshot>,
    revisions: RevisionState,
    completed_tx: mpsc::Sender<u64>,
    completed_rx: mpsc::Receiver<u64>,
}

impl WeatherLifecycle {
    pub fn new(pipeline: &WeatherFieldPipeline, gpu: &GpuContext, resolution: u32) -> Self {
        let (completed_tx, completed_rx) = mpsc::channel();
        Self {
            front: pipeline.create_textures(gpu, resolution),
            back: pipeline.create_textures(gpu, resolution),
            front_is_a: true,
            front_snapshot: None,
            revisions: RevisionState::default(),
            completed_tx,
            completed_rx,
        }
    }

    pub fn request(&mut self, snapshot: WeatherSnapshot) {
        self.revisions.request(snapshot);
    }

    pub fn next_submission(&mut self) -> Option<(u64, WeatherSnapshot)> {
        self.revisions.next_submission()
    }

    pub fn back(&self) -> &WeatherTextures {
        if self.front_is_a {
            &self.back
        } else {
            &self.front
        }
    }

    pub fn front(&self) -> &WeatherTextures {
        if self.front_is_a {
            &self.front
        } else {
            &self.back
        }
    }

    pub fn front_snapshot(&self) -> Option<WeatherSnapshot> {
        self.front_snapshot
    }

    pub fn mark_submitted(&self, queue: &wgpu::Queue, revision: u64) {
        let completed_tx = self.completed_tx.clone();
        queue.on_submitted_work_done(move || {
            let _ = completed_tx.send(revision);
        });
    }

    /// Poll completion callbacks without waiting; every newer completion publishes atomically.
    pub fn poll(&mut self) -> bool {
        let mut published = false;
        while let Ok(revision) = self.completed_rx.try_recv() {
            if let Some(snapshot) = self.revisions.complete(revision) {
                self.front_is_a = !self.front_is_a;
                self.front_snapshot = Some(snapshot);
                published = true;
            }
        }
        published
    }

    pub fn is_busy(&self) -> bool {
        self.revisions.is_busy()
    }
}

pub struct WeatherFieldPipeline {
    pipeline: wgpu::ComputePipeline,
    diagnose_pipeline: wgpu::ComputePipeline,
    bind_group_layout: wgpu::BindGroupLayout,
    spinup_init_pipeline: wgpu::ComputePipeline,
    spinup_init_layout: wgpu::BindGroupLayout,
    spinup_transport_pipeline: wgpu::ComputePipeline,
    spinup_transport_layout: wgpu::BindGroupLayout,
    spinup_finalize_pipeline: wgpu::ComputePipeline,
    spinup_finalize_layout: wgpu::BindGroupLayout,
    sampler: wgpu::Sampler,
    spinup_iterations: usize,
}

impl WeatherFieldPipeline {
    pub fn new(gpu: &GpuContext) -> Result<Self, String> {
        let features = gpu.rgba16float_features;
        let required = wgpu::TextureUsages::STORAGE_BINDING | wgpu::TextureUsages::TEXTURE_BINDING;
        if !features.allowed_usages.contains(required)
            || !features
                .flags
                .contains(wgpu::TextureFormatFeatureFlags::FILTERABLE)
        {
            return Err(format!(
                "GPU adapter '{}' does not support filterable Rgba16Float weather cubemaps",
                gpu.adapter_name()
            ));
        }
        let shader = gpu
            .device
            .create_shader_module(wgpu::ShaderModuleDescriptor {
                label: Some("weather field shader"),
                source: wgpu::ShaderSource::Wgsl(
                    format!(
                        "{}\n{}\n{}",
                        include_str!("shaders/cube_sphere.wgsl"),
                        include_str!("shaders/noise.wgsl"),
                        include_str!("shaders/weather_field.wgsl"),
                    )
                    .into(),
                ),
            });
        let bind_group_layout =
            gpu.device
                .create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
                    label: Some("weather field bgl"),
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
                            ty: wgpu::BindingType::Texture {
                                sample_type: wgpu::TextureSampleType::Float { filterable: true },
                                view_dimension: wgpu::TextureViewDimension::Cube,
                                multisampled: false,
                            },
                            count: None,
                        },
                        wgpu::BindGroupLayoutEntry {
                            binding: 2,
                            visibility: wgpu::ShaderStages::COMPUTE,
                            ty: wgpu::BindingType::Texture {
                                sample_type: wgpu::TextureSampleType::Float { filterable: true },
                                view_dimension: wgpu::TextureViewDimension::Cube,
                                multisampled: false,
                            },
                            count: None,
                        },
                        wgpu::BindGroupLayoutEntry {
                            binding: 3,
                            visibility: wgpu::ShaderStages::COMPUTE,
                            ty: wgpu::BindingType::Sampler(wgpu::SamplerBindingType::Filtering),
                            count: None,
                        },
                        wgpu::BindGroupLayoutEntry {
                            binding: 4,
                            visibility: wgpu::ShaderStages::COMPUTE,
                            ty: wgpu::BindingType::Buffer {
                                ty: wgpu::BufferBindingType::Storage { read_only: true },
                                has_dynamic_offset: false,
                                min_binding_size: None,
                            },
                            count: None,
                        },
                        wgpu::BindGroupLayoutEntry {
                            binding: 5,
                            visibility: wgpu::ShaderStages::COMPUTE,
                            ty: wgpu::BindingType::StorageTexture {
                                access: wgpu::StorageTextureAccess::WriteOnly,
                                format: wgpu::TextureFormat::Rgba16Float,
                                view_dimension: wgpu::TextureViewDimension::D2Array,
                            },
                            count: None,
                        },
                        wgpu::BindGroupLayoutEntry {
                            binding: 6,
                            visibility: wgpu::ShaderStages::COMPUTE,
                            ty: wgpu::BindingType::StorageTexture {
                                access: wgpu::StorageTextureAccess::WriteOnly,
                                format: wgpu::TextureFormat::Rgba16Float,
                                view_dimension: wgpu::TextureViewDimension::D2Array,
                            },
                            count: None,
                        },
                        wgpu::BindGroupLayoutEntry {
                            binding: 7,
                            visibility: wgpu::ShaderStages::COMPUTE,
                            ty: wgpu::BindingType::Texture {
                                sample_type: wgpu::TextureSampleType::Float { filterable: true },
                                view_dimension: wgpu::TextureViewDimension::Cube,
                                multisampled: false,
                            },
                            count: None,
                        },
                    ],
                });
        let layout = gpu
            .device
            .create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
                label: Some("weather field layout"),
                bind_group_layouts: &[&bind_group_layout],
                push_constant_ranges: &[],
            });
        let pipeline = gpu
            .device
            .create_compute_pipeline(&wgpu::ComputePipelineDescriptor {
                label: Some("weather field pipeline"),
                layout: Some(&layout),
                module: &shader,
                entry_point: Some("main"),
                compilation_options: Default::default(),
                cache: None,
            });
        let diagnose_pipeline =
            gpu.device
                .create_compute_pipeline(&wgpu::ComputePipelineDescriptor {
                    label: Some("weather final diagnosis pipeline"),
                    layout: Some(&layout),
                    module: &shader,
                    entry_point: Some("diagnose"),
                    compilation_options: Default::default(),
                    cache: None,
                });
        let spinup_shader = gpu
            .device
            .create_shader_module(wgpu::ShaderModuleDescriptor {
                label: Some("weather spin-up shader"),
                source: wgpu::ShaderSource::Wgsl(
                    format!(
                        "{}\n{}",
                        include_str!("shaders/cube_sphere.wgsl"),
                        include_str!("shaders/weather_spinup.wgsl"),
                    )
                    .into(),
                ),
            });
        let create_spinup_pipeline = |label, entry_point| {
            gpu.device
                .create_compute_pipeline(&wgpu::ComputePipelineDescriptor {
                    label: Some(label),
                    layout: None,
                    module: &spinup_shader,
                    entry_point: Some(entry_point),
                    compilation_options: Default::default(),
                    cache: None,
                })
        };
        let spinup_init_pipeline = create_spinup_pipeline("weather spin-up init", "init");
        let spinup_init_layout = spinup_init_pipeline.get_bind_group_layout(0);
        let spinup_transport_pipeline =
            create_spinup_pipeline("weather spin-up transport", "transport");
        let spinup_transport_layout = spinup_transport_pipeline.get_bind_group_layout(0);
        let spinup_finalize_pipeline =
            create_spinup_pipeline("weather spin-up finalize", "finalize");
        let spinup_finalize_layout = spinup_finalize_pipeline.get_bind_group_layout(0);
        let sampler = gpu.device.create_sampler(&wgpu::SamplerDescriptor {
            label: Some("weather field sampler"),
            mag_filter: wgpu::FilterMode::Linear,
            min_filter: wgpu::FilterMode::Linear,
            ..Default::default()
        });
        Ok(Self {
            pipeline,
            diagnose_pipeline,
            bind_group_layout,
            spinup_init_pipeline,
            spinup_init_layout,
            spinup_transport_pipeline,
            spinup_transport_layout,
            spinup_finalize_pipeline,
            spinup_finalize_layout,
            sampler,
            spinup_iterations: SPINUP_ITERATIONS,
        })
    }

    pub fn create_textures(&self, gpu: &GpuContext, resolution: u32) -> WeatherTextures {
        let create_texture = |label| {
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
        let mass_texture = create_texture("weather mass cubemap");
        let geometry_texture = create_texture("weather geometry cubemap");
        let storage = |texture: &wgpu::Texture| {
            texture.create_view(&wgpu::TextureViewDescriptor {
                dimension: Some(wgpu::TextureViewDimension::D2Array),
                ..Default::default()
            })
        };
        let sampled = |texture: &wgpu::Texture| {
            texture.create_view(&wgpu::TextureViewDescriptor {
                dimension: Some(wgpu::TextureViewDimension::Cube),
                ..Default::default()
            })
        };
        WeatherTextures {
            mass_storage: storage(&mass_texture),
            geometry_storage: storage(&geometry_texture),
            mass: sampled(&mass_texture),
            geometry: sampled(&geometry_texture),
            _mass_texture: mass_texture,
            _geometry_texture: geometry_texture,
            resolution,
        }
    }

    pub fn generate(
        &self,
        gpu: &GpuContext,
        snapshot: WeatherSnapshot,
        terrain: &TectonicTerrain,
        dynamics: &DynamicsTextures,
        weather: &WeatherTextures,
    ) {
        assert_eq!(snapshot.resolution, weather.resolution);
        let pixels_per_face = (weather.resolution * weather.resolution) as usize;
        let mut heights = vec![0.0; pixels_per_face * 6];
        for (face_index, face) in terrain.faces.iter().enumerate() {
            let source_resolution = (face.len() as f32).sqrt() as usize;
            for y in 0..weather.resolution as usize {
                for x in 0..weather.resolution as usize {
                    heights[face_index * pixels_per_face + y * weather.resolution as usize + x] =
                        face[y * source_resolution / weather.resolution as usize
                            * source_resolution
                            + x * source_resolution / weather.resolution as usize];
                }
            }
        }
        let height = gpu
            .device
            .create_buffer_init(&wgpu::util::BufferInitDescriptor {
                label: Some("weather height"),
                contents: bytemuck::cast_slice(&heights),
                usage: wgpu::BufferUsages::STORAGE,
            });
        // `main` does not read the post-spin-up state, but the shared diagnosis layout
        // requires a non-overlapping cube binding for this pre-spin-up baseline pass.
        let diagnostic_placeholder = gpu.device.create_texture(&wgpu::TextureDescriptor {
            label: Some("weather diagnosis placeholder"),
            size: wgpu::Extent3d {
                width: 1,
                height: 1,
                depth_or_array_layers: 6,
            },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: wgpu::TextureFormat::Rgba16Float,
            usage: wgpu::TextureUsages::TEXTURE_BINDING,
            view_formats: &[],
        });
        let diagnostic_placeholder =
            diagnostic_placeholder.create_view(&wgpu::TextureViewDescriptor {
                dimension: Some(wgpu::TextureViewDimension::Cube),
                ..Default::default()
            });
        let workgroups = weather.resolution.div_ceil(16);
        let mut encoder = gpu.device.create_command_encoder(&Default::default());
        for face in 0..6 {
            let params = WeatherSnapshot { face, ..snapshot };
            let uniform = gpu
                .device
                .create_buffer_init(&wgpu::util::BufferInitDescriptor {
                    label: Some("weather params"),
                    contents: bytemuck::bytes_of(&params),
                    usage: wgpu::BufferUsages::UNIFORM,
                });
            let bind_group = gpu.device.create_bind_group(&wgpu::BindGroupDescriptor {
                label: Some("weather field bind group"),
                layout: &self.bind_group_layout,
                entries: &[
                    wgpu::BindGroupEntry {
                        binding: 0,
                        resource: uniform.as_entire_binding(),
                    },
                    wgpu::BindGroupEntry {
                        binding: 1,
                        resource: wgpu::BindingResource::TextureView(&dynamics.wind_continentality),
                    },
                    wgpu::BindGroupEntry {
                        binding: 2,
                        resource: wgpu::BindingResource::TextureView(&dynamics.pressure),
                    },
                    wgpu::BindGroupEntry {
                        binding: 3,
                        resource: wgpu::BindingResource::Sampler(&self.sampler),
                    },
                    wgpu::BindGroupEntry {
                        binding: 4,
                        resource: height.as_entire_binding(),
                    },
                    wgpu::BindGroupEntry {
                        binding: 5,
                        resource: wgpu::BindingResource::TextureView(&weather.mass_storage),
                    },
                    wgpu::BindGroupEntry {
                        binding: 6,
                        resource: wgpu::BindingResource::TextureView(&weather.geometry_storage),
                    },
                    wgpu::BindGroupEntry {
                        binding: 7,
                        resource: wgpu::BindingResource::TextureView(&diagnostic_placeholder),
                    },
                ],
            });
            {
                let mut pass = encoder.begin_compute_pass(&Default::default());
                pass.set_pipeline(&self.pipeline);
                pass.set_bind_group(0, &bind_group, &[]);
                pass.dispatch_workgroups(workgroups, workgroups, 1);
            }
        }

        let spin_resolution = weather.resolution.clamp(2, SPINUP_RESOLUTION);
        let spinup_params = SpinupParams {
            spin_resolution,
            output_resolution: weather.resolution,
            seed: snapshot.seed,
            storm_count: snapshot.storm_count,
            coverage: snapshot.coverage,
            moisture: snapshot.moisture,
            surface_pressure_bar: snapshot.surface_pressure_bar,
            base_temp_c: snapshot.base_temp_c,
            ocean_level: snapshot.ocean_level,
            axial_tilt_rad: snapshot.axial_tilt_rad,
            season: snapshot.season,
            storm_size: snapshot.storm_size,
            radius_km: snapshot.radius_km,
            rotation_rate_rad_s: snapshot.rotation_rate_rad_s,
            diagnostic_flags: 0,
            _pad0: 0,
        };
        let spinup_uniform = gpu
            .device
            .create_buffer_init(&wgpu::util::BufferInitDescriptor {
                label: Some("weather spin-up params"),
                contents: bytemuck::bytes_of(&spinup_params),
                usage: wgpu::BufferUsages::UNIFORM,
            });
        let create_state = |label| {
            let texture = gpu.device.create_texture(&wgpu::TextureDescriptor {
                label: Some(label),
                size: wgpu::Extent3d {
                    width: spin_resolution,
                    height: spin_resolution,
                    depth_or_array_layers: 6,
                },
                mip_level_count: 1,
                sample_count: 1,
                dimension: wgpu::TextureDimension::D2,
                format: wgpu::TextureFormat::Rgba16Float,
                usage: wgpu::TextureUsages::STORAGE_BINDING | wgpu::TextureUsages::TEXTURE_BINDING,
                view_formats: &[],
            });
            SpinupTexture {
                sampled: texture.create_view(&wgpu::TextureViewDescriptor {
                    dimension: Some(wgpu::TextureViewDimension::Cube),
                    ..Default::default()
                }),
                storage: texture.create_view(&wgpu::TextureViewDescriptor {
                    dimension: Some(wgpu::TextureViewDimension::D2Array),
                    ..Default::default()
                }),
                _texture: texture,
            }
        };
        let state_a = create_state("weather spin-up state A");
        let state_b = create_state("weather spin-up state B");
        let init_bind_group = gpu.device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("weather spin-up init bind group"),
            layout: &self.spinup_init_layout,
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: spinup_uniform.as_entire_binding(),
                },
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: wgpu::BindingResource::TextureView(&dynamics.wind_continentality),
                },
                wgpu::BindGroupEntry {
                    binding: 3,
                    resource: wgpu::BindingResource::Sampler(&self.sampler),
                },
                wgpu::BindGroupEntry {
                    binding: 5,
                    resource: wgpu::BindingResource::TextureView(&weather.mass),
                },
                wgpu::BindGroupEntry {
                    binding: 7,
                    resource: wgpu::BindingResource::TextureView(&state_a.storage),
                },
            ],
        });
        let transport_bind_group = |label, source: &SpinupTexture, target: &SpinupTexture| {
            gpu.device.create_bind_group(&wgpu::BindGroupDescriptor {
                label: Some(label),
                layout: &self.spinup_transport_layout,
                entries: &[
                    wgpu::BindGroupEntry {
                        binding: 0,
                        resource: spinup_uniform.as_entire_binding(),
                    },
                    wgpu::BindGroupEntry {
                        binding: 1,
                        resource: wgpu::BindingResource::TextureView(&dynamics.wind_continentality),
                    },
                    wgpu::BindGroupEntry {
                        binding: 2,
                        resource: wgpu::BindingResource::TextureView(&dynamics.pressure),
                    },
                    wgpu::BindGroupEntry {
                        binding: 3,
                        resource: wgpu::BindingResource::Sampler(&self.sampler),
                    },
                    wgpu::BindGroupEntry {
                        binding: 4,
                        resource: height.as_entire_binding(),
                    },
                    wgpu::BindGroupEntry {
                        binding: 6,
                        resource: wgpu::BindingResource::TextureView(&source.sampled),
                    },
                    wgpu::BindGroupEntry {
                        binding: 7,
                        resource: wgpu::BindingResource::TextureView(&target.storage),
                    },
                ],
            })
        };
        let a_to_b = transport_bind_group("weather spin-up A to B", &state_a, &state_b);
        let b_to_a = transport_bind_group("weather spin-up B to A", &state_b, &state_a);
        let final_state = if self.spinup_iterations.is_multiple_of(2) {
            &state_a
        } else {
            &state_b
        };
        let finalize_bind_group = gpu.device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("weather spin-up finalize bind group"),
            layout: &self.spinup_finalize_layout,
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: spinup_uniform.as_entire_binding(),
                },
                wgpu::BindGroupEntry {
                    binding: 3,
                    resource: wgpu::BindingResource::Sampler(&self.sampler),
                },
                wgpu::BindGroupEntry {
                    binding: 6,
                    resource: wgpu::BindingResource::TextureView(&final_state.sampled),
                },
                wgpu::BindGroupEntry {
                    binding: 8,
                    resource: wgpu::BindingResource::TextureView(&weather.mass_storage),
                },
            ],
        });
        let spin_workgroups = spin_resolution.div_ceil(8);
        {
            let mut pass = encoder.begin_compute_pass(&Default::default());
            pass.set_pipeline(&self.spinup_init_pipeline);
            pass.set_bind_group(0, &init_bind_group, &[]);
            pass.dispatch_workgroups(spin_workgroups, spin_workgroups, 6);
        }
        for iteration in 0..self.spinup_iterations {
            let mut pass = encoder.begin_compute_pass(&Default::default());
            pass.set_pipeline(&self.spinup_transport_pipeline);
            pass.set_bind_group(0, if iteration % 2 == 0 { &a_to_b } else { &b_to_a }, &[]);
            pass.dispatch_workgroups(spin_workgroups, spin_workgroups, 6);
        }
        {
            let mut pass = encoder.begin_compute_pass(&Default::default());
            pass.set_pipeline(&self.spinup_finalize_pipeline);
            pass.set_bind_group(0, &finalize_bind_group, &[]);
            pass.dispatch_workgroups(
                weather.resolution.div_ceil(8),
                weather.resolution.div_ceil(8),
                6,
            );
        }
        // The spin-up state is transient. Diagnose and pack both published cubemaps together.
        for face in 0..6 {
            let params = WeatherSnapshot { face, ..snapshot };
            let uniform = gpu
                .device
                .create_buffer_init(&wgpu::util::BufferInitDescriptor {
                    label: Some("weather final diagnosis params"),
                    contents: bytemuck::bytes_of(&params),
                    usage: wgpu::BufferUsages::UNIFORM,
                });
            let bind_group = gpu.device.create_bind_group(&wgpu::BindGroupDescriptor {
                label: Some("weather final diagnosis bind group"),
                layout: &self.bind_group_layout,
                entries: &[
                    wgpu::BindGroupEntry {
                        binding: 0,
                        resource: uniform.as_entire_binding(),
                    },
                    wgpu::BindGroupEntry {
                        binding: 1,
                        resource: wgpu::BindingResource::TextureView(&dynamics.wind_continentality),
                    },
                    wgpu::BindGroupEntry {
                        binding: 2,
                        resource: wgpu::BindingResource::TextureView(&dynamics.pressure),
                    },
                    wgpu::BindGroupEntry {
                        binding: 3,
                        resource: wgpu::BindingResource::Sampler(&self.sampler),
                    },
                    wgpu::BindGroupEntry {
                        binding: 4,
                        resource: height.as_entire_binding(),
                    },
                    wgpu::BindGroupEntry {
                        binding: 5,
                        resource: wgpu::BindingResource::TextureView(&weather.mass_storage),
                    },
                    wgpu::BindGroupEntry {
                        binding: 6,
                        resource: wgpu::BindingResource::TextureView(&weather.geometry_storage),
                    },
                    wgpu::BindGroupEntry {
                        binding: 7,
                        resource: wgpu::BindingResource::TextureView(&final_state.sampled),
                    },
                ],
            });
            let mut pass = encoder.begin_compute_pass(&Default::default());
            pass.set_pipeline(&self.diagnose_pipeline);
            pass.set_bind_group(0, &bind_group, &[]);
            pass.dispatch_workgroups(workgroups, workgroups, 1);
        }
        gpu.queue.submit(Some(encoder.finish()));
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::terrain_compute::WindFieldPipeline;

    const SPINUP_DIAGNOSTIC_NO_SOURCE: u32 = 1;
    const SPINUP_DIAGNOSTIC_NO_SINK: u32 = 2;
    const SPINUP_DIAGNOSTIC_NO_PHASE_CHANGE: u32 = 4;
    const SPINUP_DIAGNOSTIC_NO_RELAXATION: u32 = 8;

    fn terrain(resolution: u32) -> TectonicTerrain {
        terrain_from(resolution, |pos| {
            0.2 * (pos[0] * pos[2] + pos[1] * 0.3) - 0.05
        })
    }

    fn terrain_from(resolution: u32, height: impl Fn([f32; 3]) -> f32) -> TectonicTerrain {
        TectonicTerrain {
            faces: std::array::from_fn(|face| {
                (0..resolution * resolution)
                    .map(|index| {
                        crate::cube_sphere::cube_to_sphere(
                            face as u32,
                            (index % resolution) as f32 / (resolution - 1) as f32,
                            (index / resolution) as f32 / (resolution - 1) as f32,
                        )
                    })
                    .map(&height)
                    .collect()
            }),
            resolution,
        }
    }

    fn snapshot(resolution: u32) -> WeatherSnapshot {
        WeatherSnapshot {
            face: 0,
            resolution,
            seed: 42,
            storm_count: 2,
            coverage: 0.5,
            moisture: 1.0,
            surface_pressure_bar: 1.0,
            base_temp_c: 15.0,
            ocean_level: 0.0,
            axial_tilt_rad: 0.4,
            season: 0.5,
            storm_size: 1.0,
            radius_km: 6371.0,
            rotation_rate_rad_s: std::f32::consts::TAU / 86400.0,
            _pad0: 0.0,
        }
    }

    fn read_texture(gpu: &GpuContext, texture: &wgpu::Texture, resolution: u32) -> Vec<f32> {
        read_weather_cube_texture(gpu, texture, resolution)
    }

    fn write_texture_rgba16f(
        gpu: &GpuContext,
        texture: &wgpu::Texture,
        resolution: u32,
        value: [f32; 4],
    ) {
        write_texture_rgba16f_from(gpu, texture, resolution, |_| value);
    }

    fn write_texture_rgba16f_from(
        gpu: &GpuContext,
        texture: &wgpu::Texture,
        resolution: u32,
        value_at: impl Fn([f32; 3]) -> [f32; 4],
    ) {
        let unpadded_row_bytes = (resolution * 4 * 2) as usize;
        let row_bytes = unpadded_row_bytes.div_ceil(256) * 256;
        for face in 0..6 {
            let mut full_face = vec![0u8; row_bytes * resolution as usize];
            for y in 0..resolution {
                for x in 0..resolution {
                    let pos = crate::cube_sphere::cube_to_sphere(
                        face,
                        x as f32 / (resolution - 1) as f32,
                        y as f32 / (resolution - 1) as f32,
                    );
                    let texel =
                        value_at(pos).map(|component| half::f16::from_f32(component).to_bits());
                    let offset = y as usize * row_bytes + x as usize * 8;
                    full_face[offset..offset + 8].copy_from_slice(bytemuck::bytes_of(&texel));
                }
            }
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
                &full_face,
                wgpu::TexelCopyBufferLayout {
                    offset: 0,
                    bytes_per_row: Some(row_bytes as u32),
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

    #[derive(Clone, Copy, Default)]
    struct SpinupTestConfig {
        iterations: usize,
        diagnostic_flags: u32,
    }

    struct SpinupPass {
        state: Vec<f32>,
        mass: Vec<f32>,
    }

    fn run_spinup_pass(
        gpu: &GpuContext,
        pipeline: &WeatherFieldPipeline,
        terrain: &TectonicTerrain,
        dynamics: &DynamicsTextures,
        snapshot: WeatherSnapshot,
        weather: &WeatherTextures,
        iterations: usize,
    ) -> SpinupPass {
        run_spinup_pass_with_config(
            gpu,
            pipeline,
            terrain,
            dynamics,
            snapshot,
            weather,
            SpinupTestConfig {
                iterations,
                ..Default::default()
            },
        )
    }

    fn run_spinup_pass_with_config(
        gpu: &GpuContext,
        pipeline: &WeatherFieldPipeline,
        terrain: &TectonicTerrain,
        dynamics: &DynamicsTextures,
        snapshot: WeatherSnapshot,
        weather: &WeatherTextures,
        config: SpinupTestConfig,
    ) -> SpinupPass {
        assert_eq!(snapshot.resolution, weather.resolution);
        let spin_resolution = weather.resolution.clamp(2, SPINUP_RESOLUTION);
        let spinup_params = SpinupParams {
            spin_resolution,
            output_resolution: weather.resolution,
            seed: snapshot.seed,
            storm_count: snapshot.storm_count,
            coverage: snapshot.coverage,
            moisture: snapshot.moisture,
            surface_pressure_bar: snapshot.surface_pressure_bar,
            base_temp_c: snapshot.base_temp_c,
            ocean_level: snapshot.ocean_level,
            axial_tilt_rad: snapshot.axial_tilt_rad,
            season: snapshot.season,
            storm_size: snapshot.storm_size,
            radius_km: snapshot.radius_km,
            rotation_rate_rad_s: snapshot.rotation_rate_rad_s,
            diagnostic_flags: config.diagnostic_flags,
            _pad0: 0,
        };
        let spinup_uniform = gpu
            .device
            .create_buffer_init(&wgpu::util::BufferInitDescriptor {
                label: Some("weather spin-up params"),
                contents: bytemuck::bytes_of(&spinup_params),
                usage: wgpu::BufferUsages::UNIFORM,
            });

        let pixel_per_face = (terrain.resolution * terrain.resolution) as usize;
        let mut heights = vec![0.0; pixel_per_face * 6];
        for (face_index, face) in terrain.faces.iter().enumerate() {
            let source_resolution = (face.len() as f32).sqrt() as usize;
            for y in 0..snapshot.resolution as usize {
                for x in 0..snapshot.resolution as usize {
                    heights[face_index * pixel_per_face + y * snapshot.resolution as usize + x] =
                        face[y * source_resolution / snapshot.resolution as usize
                            * source_resolution
                            + x * source_resolution / snapshot.resolution as usize];
                }
            }
        }
        let height = gpu
            .device
            .create_buffer_init(&wgpu::util::BufferInitDescriptor {
                label: Some("weather height"),
                contents: bytemuck::cast_slice(&heights),
                usage: wgpu::BufferUsages::STORAGE,
            });

        let create_state = |label| {
            let texture = gpu.device.create_texture(&wgpu::TextureDescriptor {
                label: Some(label),
                size: wgpu::Extent3d {
                    width: spin_resolution,
                    height: spin_resolution,
                    depth_or_array_layers: 6,
                },
                mip_level_count: 1,
                sample_count: 1,
                dimension: wgpu::TextureDimension::D2,
                format: wgpu::TextureFormat::Rgba16Float,
                usage: wgpu::TextureUsages::STORAGE_BINDING
                    | wgpu::TextureUsages::TEXTURE_BINDING
                    | wgpu::TextureUsages::COPY_SRC,
                view_formats: &[],
            });
            SpinupTexture {
                sampled: texture.create_view(&wgpu::TextureViewDescriptor {
                    dimension: Some(wgpu::TextureViewDimension::Cube),
                    ..Default::default()
                }),
                storage: texture.create_view(&wgpu::TextureViewDescriptor {
                    dimension: Some(wgpu::TextureViewDimension::D2Array),
                    ..Default::default()
                }),
                _texture: texture,
            }
        };

        let state_a = create_state("weather spin-up state A");
        let state_b = create_state("weather spin-up state B");
        let init_bind_group = gpu.device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("weather spin-up init bind group"),
            layout: &pipeline.spinup_init_layout,
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: spinup_uniform.as_entire_binding(),
                },
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: wgpu::BindingResource::TextureView(&dynamics.wind_continentality),
                },
                wgpu::BindGroupEntry {
                    binding: 3,
                    resource: wgpu::BindingResource::Sampler(&pipeline.sampler),
                },
                wgpu::BindGroupEntry {
                    binding: 5,
                    resource: wgpu::BindingResource::TextureView(&weather.mass),
                },
                wgpu::BindGroupEntry {
                    binding: 7,
                    resource: wgpu::BindingResource::TextureView(&state_a.storage),
                },
            ],
        });

        let spin_workgroups = spin_resolution.div_ceil(8);
        let mut encoder = gpu.device.create_command_encoder(&Default::default());
        {
            let mut pass = encoder.begin_compute_pass(&Default::default());
            pass.set_pipeline(&pipeline.spinup_init_pipeline);
            pass.set_bind_group(0, &init_bind_group, &[]);
            pass.dispatch_workgroups(spin_workgroups, spin_workgroups, 6);
        }
        let transport_bind_group = |label, source: &SpinupTexture, target: &SpinupTexture| {
            gpu.device.create_bind_group(&wgpu::BindGroupDescriptor {
                label: Some(label),
                layout: &pipeline.spinup_transport_layout,
                entries: &[
                    wgpu::BindGroupEntry {
                        binding: 0,
                        resource: spinup_uniform.as_entire_binding(),
                    },
                    wgpu::BindGroupEntry {
                        binding: 1,
                        resource: wgpu::BindingResource::TextureView(&dynamics.wind_continentality),
                    },
                    wgpu::BindGroupEntry {
                        binding: 2,
                        resource: wgpu::BindingResource::TextureView(&dynamics.pressure),
                    },
                    wgpu::BindGroupEntry {
                        binding: 3,
                        resource: wgpu::BindingResource::Sampler(&pipeline.sampler),
                    },
                    wgpu::BindGroupEntry {
                        binding: 4,
                        resource: height.as_entire_binding(),
                    },
                    wgpu::BindGroupEntry {
                        binding: 6,
                        resource: wgpu::BindingResource::TextureView(&source.sampled),
                    },
                    wgpu::BindGroupEntry {
                        binding: 7,
                        resource: wgpu::BindingResource::TextureView(&target.storage),
                    },
                ],
            })
        };
        let a_to_b = transport_bind_group("weather spin-up A to B", &state_a, &state_b);
        let b_to_a = transport_bind_group("weather spin-up B to A", &state_b, &state_a);
        for iteration in 0..config.iterations {
            let mut pass = encoder.begin_compute_pass(&Default::default());
            pass.set_pipeline(&pipeline.spinup_transport_pipeline);
            pass.set_bind_group(0, if iteration % 2 == 0 { &a_to_b } else { &b_to_a }, &[]);
            pass.dispatch_workgroups(spin_workgroups, spin_workgroups, 6);
        }

        let final_state = if config.iterations.is_multiple_of(2) {
            &state_a
        } else {
            &state_b
        };
        let finalize_bind_group = gpu.device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("weather spin-up debug finalize bind group"),
            layout: &pipeline.spinup_finalize_layout,
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: spinup_uniform.as_entire_binding(),
                },
                wgpu::BindGroupEntry {
                    binding: 3,
                    resource: wgpu::BindingResource::Sampler(&pipeline.sampler),
                },
                wgpu::BindGroupEntry {
                    binding: 6,
                    resource: wgpu::BindingResource::TextureView(&final_state.sampled),
                },
                wgpu::BindGroupEntry {
                    binding: 8,
                    resource: wgpu::BindingResource::TextureView(&weather.mass_storage),
                },
            ],
        });
        {
            let mut pass = encoder.begin_compute_pass(&Default::default());
            pass.set_pipeline(&pipeline.spinup_finalize_pipeline);
            pass.set_bind_group(0, &finalize_bind_group, &[]);
            pass.dispatch_workgroups(
                weather.resolution.div_ceil(8),
                weather.resolution.div_ceil(8),
                6,
            );
        }
        gpu.queue.submit(Some(encoder.finish()));
        SpinupPass {
            state: read_weather_cube_texture(gpu, &final_state._texture, spin_resolution),
            mass: read_weather_cube_texture(gpu, &weather._mass_texture, weather.resolution),
        }
    }

    fn generate_weather(
        gpu: &GpuContext,
        pipeline: &WeatherFieldPipeline,
        dynamics: &DynamicsTextures,
        terrain: &TectonicTerrain,
        snapshot: WeatherSnapshot,
    ) -> WeatherTextures {
        let weather = pipeline.create_textures(gpu, snapshot.resolution);
        pipeline.generate(gpu, snapshot, terrain, dynamics, &weather);
        weather
    }

    fn channel_sum_where(
        values: &[f32],
        resolution: u32,
        channel: usize,
        include: impl Fn([f32; 3]) -> bool,
    ) -> f32 {
        let mut sum = 0.0;
        for face in 0..6 {
            for y in 0..resolution {
                for x in 0..resolution {
                    let pos = crate::cube_sphere::cube_to_sphere(
                        face,
                        x as f32 / (resolution - 1) as f32,
                        y as f32 / (resolution - 1) as f32,
                    );
                    if include(pos) {
                        let pixel =
                            ((face * resolution * resolution + y * resolution + x) * 4) as usize;
                        sum += values[pixel + channel];
                    }
                }
            }
        }
        sum
    }

    #[derive(Clone, Copy, Debug)]
    struct DeepMetrics {
        mass: f32,
        area: usize,
    }

    fn deep_metrics(mass: &[f32]) -> DeepMetrics {
        let deep_mass = mass.chunks_exact(4).map(|pixel| pixel[1]);
        DeepMetrics {
            mass: deep_mass.clone().sum(),
            area: deep_mass.clone().filter(|value| *value > 0.1).count(),
        }
    }

    #[test]
    fn weather_is_deterministic_finite_and_channel_ordered() {
        let gpu = GpuContext::new().expect("GPU init failed");
        let terrain = terrain(16);
        let wind = WindFieldPipeline::new(&gpu).expect("dynamics unavailable");
        let dynamics = wind.create_textures(&gpu, 16);
        wind.generate_gpu(&gpu, &terrain, &dynamics, 42, 0.0, 0.4, 0.5, 1.0, 15.0, 1.0);
        let pipeline = WeatherFieldPipeline::new(&gpu).expect("weather unavailable");
        let first = generate_weather(&gpu, &pipeline, &dynamics, &terrain, snapshot(16));
        let second = generate_weather(&gpu, &pipeline, &dynamics, &terrain, snapshot(16));
        let a = read_texture(&gpu, &first._mass_texture, 16);
        let b = read_texture(&gpu, &second._mass_texture, 16);
        let geometry_a = read_texture(&gpu, &first._geometry_texture, 16);
        let geometry_b = read_texture(&gpu, &second._geometry_texture, 16);
        assert_eq!(a, b);
        assert_eq!(geometry_a, geometry_b);
        assert!(a.iter().all(|value| value.is_finite()));
        assert!(geometry_a.iter().all(|value| value.is_finite()));
        assert!(
            a.chunks_exact(4)
                .all(|pixel| pixel.iter().all(|value| (0.0..=1.0).contains(value)))
        );
        assert!(a.chunks_exact(4).all(|pixel| {
            (pixel[3] <= 0.001 || (pixel[0] + pixel[1] - pixel[3]).abs() <= 0.002)
                && pixel[2] <= pixel[3] + 0.002
        }));
        assert!(geometry_a.chunks_exact(4).all(|pixel| {
            pixel[0] >= 0.0
                && pixel[0] <= pixel[1]
                && pixel[1] <= pixel[2]
                && pixel[2] <= pixel[3]
                && pixel[3] <= 20.0
        }));
        let occupied = a.chunks_exact(4).filter(|pixel| pixel[3] > 0.05).count();
        let occupied_fraction = occupied as f32 / (16 * 16 * 6) as f32;
        assert!(
            (0.15..=0.65).contains(&occupied_fraction),
            "half coverage should leave coherent clear sky, occupied={occupied_fraction:.2}"
        );
        let mut overcast = snapshot(16);
        overcast.coverage = 1.0;
        let overcast = read_texture(
            &gpu,
            &generate_weather(&gpu, &pipeline, &dynamics, &terrain, overcast)._mass_texture,
            16,
        );
        let pixel =
            |face: usize, x: usize, y: usize| &overcast[((face * 16 + y) * 16 + x) * 4..][..4];
        let mut populated_seams = 0;
        for corners in [
            [(0, 0, 0), (2, 15, 15), (4, 15, 0)],
            [(0, 15, 0), (2, 15, 0), (5, 0, 0)],
            [(0, 0, 15), (3, 15, 0), (4, 15, 15)],
            [(0, 15, 15), (3, 15, 15), (5, 0, 15)],
            [(1, 15, 0), (2, 0, 15), (4, 0, 0)],
            [(1, 0, 0), (2, 0, 0), (5, 15, 0)],
            [(1, 15, 15), (3, 0, 0), (4, 0, 15)],
            [(1, 0, 15), (3, 0, 15), (5, 15, 15)],
        ] {
            let reference = pixel(corners[0].0, corners[0].1, corners[0].2);
            populated_seams += usize::from(reference[3] > 0.01);
            for &(face, x, y) in &corners[1..] {
                assert!(
                    reference
                        .iter()
                        .zip(pixel(face, x, y))
                        .all(|(a, b)| (a - b).abs() <= 0.01),
                    "weather corner {corners:?} is discontinuous"
                );
            }
        }
        assert!(
            populated_seams > 0,
            "seam checks must include populated weather"
        );

        let mut clear = snapshot(16);
        clear.moisture = 0.0;
        let clear = generate_weather(&gpu, &pipeline, &dynamics, &terrain, clear);
        assert!(
            read_texture(&gpu, &clear._mass_texture, 16)
                .chunks_exact(4)
                .all(|pixel| pixel == [0.0; 4])
        );

        let mut clear = snapshot(16);
        clear.coverage = 0.0;
        let clear = generate_weather(&gpu, &pipeline, &dynamics, &terrain, clear);
        assert!(
            read_texture(&gpu, &clear._mass_texture, 16)
                .chunks_exact(4)
                .all(|pixel| pixel == [0.0; 4])
        );
    }

    #[test]
    fn weather_seed_temperature_and_season_change_the_field() {
        let gpu = GpuContext::new().expect("GPU init failed");
        let terrain = terrain(16);
        let wind = WindFieldPipeline::new(&gpu).expect("dynamics unavailable");
        let dynamics = wind.create_textures(&gpu, 16);
        wind.generate_gpu(&gpu, &terrain, &dynamics, 42, 0.0, 0.4, 0.5, 1.0, 15.0, 1.0);
        let pipeline = WeatherFieldPipeline::new(&gpu).expect("weather unavailable");
        let baseline = read_texture(
            &gpu,
            &generate_weather(&gpu, &pipeline, &dynamics, &terrain, snapshot(16))._mass_texture,
            16,
        );
        let mut seeded = snapshot(16);
        seeded.seed = 43;
        let seeded = read_texture(
            &gpu,
            &generate_weather(&gpu, &pipeline, &dynamics, &terrain, seeded)._mass_texture,
            16,
        );
        assert_ne!(baseline, seeded);
        let mut warm = snapshot(16);
        warm.base_temp_c = 35.0;
        let warm = read_texture(
            &gpu,
            &generate_weather(&gpu, &pipeline, &dynamics, &terrain, warm)._mass_texture,
            16,
        );
        assert_ne!(baseline, warm);

        let mut winter = snapshot(16);
        winter.season = 0.0;
        let winter = read_texture(
            &gpu,
            &generate_weather(&gpu, &pipeline, &dynamics, &terrain, winter)._mass_texture,
            16,
        );
        let mut summer = snapshot(16);
        summer.season = 1.0;
        let summer = read_texture(
            &gpu,
            &generate_weather(&gpu, &pipeline, &dynamics, &terrain, summer)._mass_texture,
            16,
        );
        assert_ne!(winter, summer);
    }

    #[test]
    fn coverage_is_continuous_from_zero() {
        let gpu = GpuContext::new().expect("GPU init failed");
        let terrain = terrain(16);
        let wind = WindFieldPipeline::new(&gpu).expect("dynamics unavailable");
        let dynamics = wind.create_textures(&gpu, 16);
        wind.generate_gpu(&gpu, &terrain, &dynamics, 42, 0.0, 0.4, 0.5, 1.0, 15.0, 1.0);
        let pipeline = WeatherFieldPipeline::new(&gpu).expect("weather unavailable");
        let mut totals = Vec::new();
        for coverage in [0.0, half::f16::EPSILON.to_f32(), 0.25, 0.5, 0.75, 1.0] {
            let mut params = snapshot(16);
            params.coverage = coverage;
            let values = read_texture(
                &gpu,
                &generate_weather(&gpu, &pipeline, &dynamics, &terrain, params)._mass_texture,
                16,
            );
            totals.push(values.chunks_exact(4).map(|pixel| pixel[3]).sum::<f32>());
        }
        assert_eq!(totals[0], 0.0);
        assert!(
            totals.windows(2).all(|pair| pair[1] > pair[0]),
            "{totals:?}"
        );
    }

    #[test]
    fn marine_forcing_drives_cool_decks_warm_trades_and_continuous_coverage() {
        let resolution = 32;
        let gpu = GpuContext::new().expect("GPU init failed");
        let terrain = terrain_from(resolution, |_| -0.1);
        let wind_pipeline = WindFieldPipeline::new(&gpu).expect("dynamics unavailable");
        let pipeline = WeatherFieldPipeline::new(&gpu).expect("weather unavailable");
        let generate = |continentality: f32, base_temp_c: f32, coverage: f32| {
            let dynamics = wind_pipeline.create_test_textures(&gpu, resolution, |_| {
                ([0.0, 0.0, 0.0, continentality], 1025.0)
            });
            let weather = generate_weather(
                &gpu,
                &pipeline,
                &dynamics,
                &terrain,
                WeatherSnapshot {
                    storm_count: 0,
                    base_temp_c,
                    coverage,
                    ..snapshot(resolution)
                },
            );
            (weather.read_mass(&gpu), weather.read_geometry(&gpu))
        };
        let mean = |values: &[f32], channel: usize| {
            values
                .chunks_exact(4)
                .map(|pixel| pixel[channel])
                .sum::<f32>()
                / (values.len() / 4) as f32
        };

        let (cool_ocean, cool_geometry) = generate(0.0, -10.0, 0.75);
        let (cool_inland, _) = generate(1.0, -10.0, 0.75);
        let cool_low = mean(&cool_ocean, 0);
        let inland_low = mean(&cool_inland, 0);
        let cool_deep = mean(&cool_ocean, 1);
        let deck_thickness = cool_geometry
            .chunks_exact(4)
            .map(|geometry| geometry[1] - geometry[0])
            .sum::<f32>()
            / (cool_geometry.len() / 4) as f32;
        assert!(
            cool_low >= inland_low * 1.5,
            "ocean={cool_low} inland={inland_low}"
        );
        assert!(inland_low >= 0.02, "inland low mass={inland_low}");
        assert!(
            cool_low >= cool_deep * 4.0,
            "low={cool_low} deep={cool_deep}"
        );
        assert!(cool_deep >= 0.005, "cool deep mass={cool_deep}");
        assert!(
            (0.3..=1.2).contains(&deck_thickness),
            "deck thickness={deck_thickness}km"
        );

        let (warm_ocean, warm_geometry) = generate(0.0, 28.0, 0.75);
        let warm_top = mean(&warm_geometry, 1);
        let warm_low = mean(&warm_ocean, 0);
        let clear_gaps = warm_ocean
            .chunks_exact(4)
            .filter(|mass| mass[0] <= 0.05)
            .count() as f32
            / (warm_ocean.len() / 4) as f32;
        assert!(
            (1.0..=3.0).contains(&warm_top),
            "warm marine top={warm_top}km"
        );
        assert!(warm_low >= 0.02, "warm low mass={warm_low}");
        assert!(
            (0.15..=0.85).contains(&clear_gaps),
            "warm clear gaps={clear_gaps}"
        );

        let totals: Vec<f32> = [0.0, 0.25, 0.5, 0.75, 1.0]
            .into_iter()
            .map(|coverage| mean(&generate(0.0, 15.0, coverage).0, 3))
            .collect();
        let increments: Vec<f32> = totals.windows(2).map(|pair| pair[1] - pair[0]).collect();
        let mut sorted = increments.clone();
        sorted.sort_by(f32::total_cmp);
        let median_increment = sorted[sorted.len() / 2];
        assert!(
            totals.windows(2).all(|pair| pair[1] >= pair[0]),
            "{totals:?}"
        );
        assert!(
            increments
                .iter()
                .all(|increment| *increment <= median_increment * 2.0),
            "coverage increments={increments:?}, median={median_increment}"
        );
    }

    #[test]
    fn orography_follows_wind_and_stops_in_calm_air() {
        let resolution = 32;
        let gpu = GpuContext::new().expect("GPU init failed");
        let terrain = terrain_from(resolution, |pos| {
            let ridge = (-((pos[2] / 0.14).powi(2))).exp() * pos[0].max(0.0).powi(8);
            -0.15 + ridge * 0.45
        });
        let wind_pipeline = WindFieldPipeline::new(&gpu).expect("dynamics unavailable");
        let pipeline = WeatherFieldPipeline::new(&gpu).expect("weather unavailable");
        let generate = |speed: f32| {
            let dynamics = wind_pipeline.create_test_textures(&gpu, resolution, |pos| {
                let tangent = [pos[2], 0.0, -pos[0]];
                let length = (tangent[0] * tangent[0] + tangent[2] * tangent[2])
                    .sqrt()
                    .max(0.0001);
                (
                    [
                        tangent[0] / length * speed,
                        0.0,
                        tangent[2] / length * speed,
                        1.0,
                    ],
                    1013.0,
                )
            });
            read_texture(
                &gpu,
                &generate_weather(
                    &gpu,
                    &pipeline,
                    &dynamics,
                    &terrain,
                    WeatherSnapshot {
                        storm_count: 0,
                        coverage: 1.0,
                        base_temp_c: 5.0,
                        ..snapshot(resolution)
                    },
                )
                ._mass_texture,
                resolution,
            )
        };
        let side = |values: &[f32], positive: bool| {
            channel_sum_where(values, resolution, 0, |pos| {
                pos[0] > 0.65 && (pos[2] > 0.04) == positive && pos[2].abs() < 0.45
            })
        };
        let eastward = generate(1.0);
        let westward = generate(-1.0);
        let calm = generate(0.0);
        let whisper = generate(0.001);
        let east_asymmetry = side(&eastward, true) - side(&eastward, false);
        let west_asymmetry = side(&westward, true) - side(&westward, false);
        let calm_asymmetry = side(&calm, true) - side(&calm, false);
        assert_eq!(calm, whisper);
        assert!(
            (east_asymmetry - calm_asymmetry).abs() > 0.15,
            "windward land enhancement={}",
            east_asymmetry - calm_asymmetry
        );
        assert!(
            (east_asymmetry - calm_asymmetry) * (west_asymmetry - calm_asymmetry) < 0.0,
            "east={east_asymmetry}, west={west_asymmetry}, calm={calm_asymmetry}"
        );
    }

    #[test]
    fn fronts_use_signed_gradient_alignment() {
        let resolution = 32;
        let gpu = GpuContext::new().expect("GPU init failed");
        let terrain = terrain_from(resolution, |_| -0.1);
        let wind_pipeline = WindFieldPipeline::new(&gpu).expect("dynamics unavailable");
        let pipeline = WeatherFieldPipeline::new(&gpu).expect("weather unavailable");
        let generate = |pressure_sign: f32, convergent: bool| {
            let dynamics = wind_pipeline.create_test_textures(&gpu, resolution, |pos| {
                let speed = if convergent { 0.8 } else { 0.0 };
                let projection = pos[1];
                (
                    [
                        -pos[0] * projection * speed,
                        (1.0 - pos[1] * projection) * speed,
                        -pos[2] * projection * speed,
                        0.0,
                    ],
                    1013.0 + pressure_sign * pos[1] * 40.0,
                )
            });
            read_texture(
                &gpu,
                &generate_weather(
                    &gpu,
                    &pipeline,
                    &dynamics,
                    &terrain,
                    WeatherSnapshot {
                        coverage: 1.0,
                        base_temp_c: -10.0,
                        ..snapshot(resolution)
                    },
                )
                ._mass_texture,
                resolution,
            )
        };
        let aligned = generate(-1.0, true);
        let opposed = generate(1.0, true);
        let no_convergence = generate(-1.0, false);
        let northern_high = |values: &[f32]| {
            channel_sum_where(values, resolution, 2, |pos| pos[1] > 0.25 && pos[1] < 0.85)
        };
        let high = [
            northern_high(&aligned),
            northern_high(&opposed),
            northern_high(&no_convergence),
        ];
        assert!(high[0] > high[1] * 1.02, "high={high:?}");
        assert!(high[0] > high[2] * 1.25, "high={high:?}");
    }

    #[test]
    fn storm_count_and_size_localize_only_physically_eligible_deep_clouds() {
        let resolution = 32;
        let gpu = GpuContext::new().expect("GPU init failed");
        let terrain = terrain_from(resolution, |_| -0.1);
        let wind_pipeline = WindFieldPipeline::new(&gpu).expect("dynamics unavailable");
        let pipeline = WeatherFieldPipeline::new(&gpu).expect("weather unavailable");
        let dynamics = wind_pipeline.create_test_textures(&gpu, resolution, |pos| {
            let target = [1.0, 0.0, 0.0];
            let projection = target[0] * pos[0] + target[1] * pos[1] + target[2] * pos[2];
            (
                [
                    (target[0] - pos[0] * projection) * 0.8,
                    (target[1] - pos[1] * projection) * 0.8,
                    (target[2] - pos[2] * projection) * 0.8,
                    0.0,
                ],
                1000.0,
            )
        });
        let generate = |storm_count, storm_size, moisture, base_temp_c, surface_pressure_bar| {
            let weather = generate_weather(
                &gpu,
                &pipeline,
                &dynamics,
                &terrain,
                WeatherSnapshot {
                    storm_count,
                    storm_size,
                    coverage: 0.5,
                    moisture,
                    base_temp_c,
                    surface_pressure_bar,
                    ..snapshot(resolution)
                },
            );
            read_texture(&gpu, &weather._mass_texture, resolution)
        };

        let eligible = |pos: [f32; 3]| pos[0] > 0.2;
        let ineligible = |pos: [f32; 3]| pos[0] <= 0.2;
        let deep_mass_area_where = |values: &[f32], include: &dyn Fn([f32; 3]) -> bool| {
            let mut mass = 0.0f32;
            let mut area = 0usize;
            for face in 0..6 {
                for y in 0..resolution {
                    for x in 0..resolution {
                        let pos = crate::cube_sphere::cube_to_sphere(
                            face,
                            x as f32 / (resolution - 1) as f32,
                            y as f32 / (resolution - 1) as f32,
                        );
                        if !include(pos) {
                            continue;
                        }
                        let pixel =
                            ((face * resolution * resolution + y * resolution + x) * 4) as usize;
                        let deep = values[pixel + 1];
                        mass += deep;
                        if deep > 0.1 {
                            area += 1;
                        }
                    }
                }
            }
            (mass, area)
        };

        let baseline_values = generate(0, 0.3, 1.0, 35.0, 1.03);
        let dry_base = deep_metrics(&baseline_values);

        let wet = [0, 4, 8].map(|count| {
            let values = generate(count, 1.0, 1.0, 35.0, 1.0);
            let metrics = deep_metrics(&values);
            let eligible = deep_mass_area_where(&values, &eligible);
            let ineligible = deep_mass_area_where(&values, &ineligible);
            (metrics, eligible, ineligible)
        });

        assert!(
            wet.windows(2).all(|pair| pair[1].0.mass > pair[0].0.mass),
            "{wet:?}"
        );
        let mass_ratio = wet[2].0.mass / wet[0].0.mass;
        assert!(mass_ratio > 1.01, "{wet:?}");
        assert!(wet[2].0.area > wet[0].0.area, "{wet:?}");
        let wet_eligible_mass_gain = wet[2].1.0 - wet[0].1.0;
        let wet_ineligible_mass_gain = wet[2].2.0 - wet[0].2.0;
        assert!(
            wet_eligible_mass_gain > wet_ineligible_mass_gain * 5.0,
            "{wet:?}"
        );
        assert!(wet[2].1.1 > wet[0].1.1, "{wet:?}");

        let sized = [0.3, 1.0, 3.0].map(|size| {
            let values = generate(4, size, 1.0, 35.0, 1.0);
            let metrics = deep_metrics(&values);
            let eligible = deep_mass_area_where(&values, &eligible);
            let ineligible = deep_mass_area_where(&values, &ineligible);
            (metrics, eligible, ineligible)
        });
        assert!(
            sized.windows(2).all(|pair| pair[1].0.mass > pair[0].0.mass),
            "{sized:?}"
        );
        assert!(sized[2].0.area > sized[0].0.area, "{sized:?}");
        assert!(
            (sized[2].1.0 - sized[0].1.0) > (sized[2].2.0 - sized[0].2.0) * 5.0,
            "{sized:?}"
        );

        let dry = [0, 8].map(|count| {
            let values = generate(count, 3.0, 0.0, 35.0, 1.03);
            let metrics = deep_metrics(&values);
            let eligible = deep_mass_area_where(&values, &eligible);
            let ineligible = deep_mass_area_where(&values, &ineligible);
            (metrics, eligible, ineligible)
        });
        assert!(dry.iter().all(|tuple| tuple.0.mass <= 0.001), "{dry:?}");
        assert!(dry.iter().all(|tuple| tuple.0.area == 0), "{dry:?}");
        assert!(
            dry.iter().all(|tuple| tuple.1.0 <= dry_base.mass),
            "{dry:?}"
        );
        assert!(
            dry.iter().all(|tuple| tuple.2.0 <= dry_base.mass * 0.05),
            "{dry:?}"
        );

        let stable = [
            {
                let values = generate(0, 0.3, 1.0, -35.0, 1.03);
                deep_metrics(&values)
            },
            {
                let values = generate(8, 3.0, 1.0, -35.0, 1.03);
                deep_metrics(&values)
            },
        ];
        assert!(
            (stable[1].mass - stable[0].mass).abs() <= 0.001,
            "{stable:?}"
        );
        assert_eq!(stable[0].area, stable[1].area, "{stable:?}");
        println!(
            "storm controls: count mass={:?}, count area={:?}, size mass={:?}, size area={:?}, dry mass={:?}, stable mass delta={:.6}",
            wet.map(|metrics| metrics.0.mass),
            wet.map(|metrics| metrics.0.area),
            sized.map(|metrics| metrics.0.mass),
            sized.map(|metrics| metrics.0.area),
            dry.map(|metrics| metrics.0.mass),
            stable[1].mass - stable[0].mass,
        );
    }

    #[test]
    fn divergence_suppresses_occupancy() {
        let resolution = 32;
        let gpu = GpuContext::new().expect("GPU init failed");
        let terrain = terrain_from(resolution, |_| -0.1);
        let wind_pipeline = WindFieldPipeline::new(&gpu).expect("dynamics unavailable");
        let dynamics = wind_pipeline.create_test_textures(&gpu, resolution, |pos| {
            let projection = pos[0];
            (
                [
                    (1.0 - pos[0] * projection) * 0.8,
                    -pos[1] * projection * 0.8,
                    -pos[2] * projection * 0.8,
                    0.0,
                ],
                1013.0,
            )
        });
        let pipeline = WeatherFieldPipeline::new(&gpu).expect("weather unavailable");
        let mut params = snapshot(resolution);
        params.coverage = 0.75;
        let mass = read_texture(
            &gpu,
            &generate_weather(&gpu, &pipeline, &dynamics, &terrain, params)._mass_texture,
            resolution,
        );
        let convergent = channel_sum_where(&mass, resolution, 3, |pos| pos[0] > 0.55);
        let divergent = channel_sum_where(&mass, resolution, 3, |pos| pos[0] < -0.55);
        assert!(convergent > divergent * 1.1, "{convergent} <= {divergent}");
    }

    #[test]
    fn cloud_seed_continuously_erodes_eligible_mass_without_replacing_structure() {
        let resolution = 32;
        let gpu = GpuContext::new().expect("GPU init failed");
        let terrain = terrain(resolution);
        let wind = WindFieldPipeline::new(&gpu).expect("dynamics unavailable");
        let dynamics = wind.create_textures(&gpu, resolution);
        wind.generate_gpu(&gpu, &terrain, &dynamics, 42, 0.0, 0.4, 0.5, 1.0, 15.0, 1.0);
        let pipeline = WeatherFieldPipeline::new(&gpu).expect("weather unavailable");
        let generate = |seed| {
            let mut params = snapshot(resolution);
            params.seed = seed;
            params.storm_count = 0;
            read_texture(
                &gpu,
                &generate_weather(&gpu, &pipeline, &dynamics, &terrain, params)._mass_texture,
                resolution,
            )
        };
        let first = generate(1 << 24);
        let second = generate((1 << 24) + 1);
        let pixels = (resolution * resolution * 6) as usize;
        let changed = first
            .chunks_exact(4)
            .zip(second.chunks_exact(4))
            .filter(|(a, b)| (a[3] - b[3]).abs() > 0.05)
            .count();
        let core_union = first
            .chunks_exact(4)
            .zip(second.chunks_exact(4))
            .filter(|(a, b)| a[3] > 0.15 || b[3] > 0.15)
            .count();
        let core_overlap = first
            .chunks_exact(4)
            .zip(second.chunks_exact(4))
            .filter(|(a, b)| a[3] > 0.15 && b[3] > 0.15)
            .count();
        let totals =
            [&first, &second].map(|mass| mass.chunks_exact(4).map(|pixel| pixel[3]).sum::<f32>());
        assert!(
            changed > pixels / 14,
            "adjacent high seeds changed only {changed}/{pixels} eligible mass pixels"
        );
        assert!(
            core_overlap * 5 > core_union * 2,
            "core overlap={core_overlap}/{core_union}"
        );
        assert!(
            (0.8..=1.25).contains(&(totals[1] / totals[0])),
            "totals={totals:?}"
        );
    }

    #[test]
    fn revisions_publish_monotonically_while_coalescing_pending_requests() {
        let mut revisions = RevisionState::default();
        let mut first_snapshot = snapshot(16);
        first_snapshot.seed = 1;
        first_snapshot.radius_km = 5000.0;
        revisions.request(first_snapshot);
        let (first, _) = revisions.next_submission().unwrap();
        let mut latest_snapshot = snapshot(16);
        latest_snapshot.seed = 3;
        latest_snapshot.radius_km = 7000.0;
        revisions.request(snapshot(16));
        revisions.request(latest_snapshot);
        let published_first = revisions.complete(first).unwrap();
        assert_eq!(published_first.seed, 1);
        assert_eq!(published_first.radius_km, 5000.0);
        assert_eq!(revisions.ready, Some(1));
        let (latest, submitted_snapshot) = revisions.next_submission().unwrap();
        assert_eq!(latest, 3);
        assert_eq!(submitted_snapshot.seed, 3);
        let published_snapshot = revisions.complete(latest).unwrap();
        assert_eq!(published_snapshot.seed, 3);
        assert_eq!(published_snapshot.radius_km, 7000.0);
        assert_eq!(revisions.ready, Some(3));
    }

    #[test]
    fn spinup_zero_iterations_uses_init_then_finalize_with_initialized_baseline() {
        let resolution = 8;
        let gpu = GpuContext::new().expect("GPU init failed");
        let terrain = terrain(resolution);
        let wind_pipeline = WindFieldPipeline::new(&gpu).expect("dynamics unavailable");
        let dynamics = wind_pipeline
            .create_test_textures(&gpu, resolution, |_| ([0.0, 0.0, 0.0, 0.0], 1013.0));
        let mut pipeline = WeatherFieldPipeline::new(&gpu).expect("weather unavailable");
        pipeline.spinup_iterations = 0;
        let weather = pipeline.create_textures(&gpu, resolution);

        // Half-float quantization is the storage path, so pick values with stable rounding.
        let baseline = [0.4_f32, 0.20_f32, 0.0_f32, 0.0_f32];
        write_texture_rgba16f(&gpu, &weather._mass_texture, resolution, baseline);

        let pre = read_texture(&gpu, &weather._mass_texture, resolution);
        println!("pre pixel 0 {:?}", pre[0..4].to_vec());
        assert!(
            pre.chunks_exact(4)
                .all(|pixel| (0..4)
                    .all(|channel| (pixel[channel] - baseline[channel]).abs() <= 0.0001)),
            "baseline write failed: first pixel={:?}",
            pre.chunks_exact(4).next().unwrap()
        );

        let mut params = snapshot(resolution);
        params.coverage = 1.0;
        params.moisture = 1.0;

        let output =
            run_spinup_pass(&gpu, &pipeline, &terrain, &dynamics, params, &weather, 0).mass;
        println!("output pixel 0 {:?}", output[0..4].to_vec());
        assert!(
            output[0] > 0.0 || output[1] > 0.0 || output[2] > 0.0 || output[3] > 0.0,
            "zero-iter spin-up did not produce cloud mass"
        );
        let reference = &output[0..4];
        let occupancy = reference[3];
        assert!(
            (reference[0] + reference[1] - occupancy).abs() <= 0.02
                && reference[2] <= occupancy + 0.02,
            "spinup finalize consistency failed: first pixel={:?}",
            reference
        );
        assert!(
            output
                .chunks_exact(4)
                .all(|pixel| (0..4)
                    .all(|channel| (pixel[channel] - reference[channel]).abs() <= 0.03)),
            "zero-iter spin-up produced non-uniform pixel distribution"
        );
        assert!(
            output
                .chunks_exact(4)
                .all(|pixel| (0..4)
                    .all(|channel| pixel[channel].is_finite() && pixel[channel] >= 0.0)),
            "zero-iter spin-up produced non-finite values"
        );
        assert!(
            output
                .chunks_exact(4)
                .all(|pixel| (0..4).all(|channel| pixel[channel] <= 1.0)),
            "zero-iter spin-up produced clipped values"
        );
    }

    #[test]
    fn spinup_transport_conserves_private_diagnostic_moisture_state() {
        const TRANSPORT_STEPS: usize = 16;
        let resolution = 16;
        let gpu = GpuContext::new().expect("GPU init failed");
        let terrain = terrain(resolution);
        let wind_pipeline = WindFieldPipeline::new(&gpu).expect("dynamics unavailable");
        let dynamics = wind_pipeline.create_test_textures(&gpu, resolution, |pos| {
            ([pos[2] * 0.4, 0.0, -pos[0] * 0.4, 0.0], 1013.0)
        });
        let pipeline = WeatherFieldPipeline::new(&gpu).expect("weather unavailable");
        let flags = SPINUP_DIAGNOSTIC_NO_SOURCE
            | SPINUP_DIAGNOSTIC_NO_SINK
            | SPINUP_DIAGNOSTIC_NO_PHASE_CHANGE
            | SPINUP_DIAGNOSTIC_NO_RELAXATION;
        let run = |iterations| {
            let weather = pipeline.create_textures(&gpu, resolution);
            write_texture_rgba16f_from(&gpu, &weather._mass_texture, resolution, |pos| {
                // An asymmetric, bounded moisture pulse near +Z travels toward +X.
                let center = [-0.30, 0.15, 0.94];
                let alignment = pos[0] * center[0] + pos[1] * center[1] + pos[2] * center[2];
                let pulse = (-18.0 * (1.0 - alignment)).exp();
                let gradient = (0.08 + 0.04 * pos[1] + 0.03 * pos[0]).max(0.01);
                [
                    gradient + pulse * 0.24,
                    gradient * 0.45 + pulse * 0.15,
                    gradient * 0.20 + pulse * 0.08,
                    gradient * 0.70 + pulse * 0.42,
                ]
            });
            run_spinup_pass_with_config(
                &gpu,
                &pipeline,
                &terrain,
                &dynamics,
                snapshot(resolution),
                &weather,
                SpinupTestConfig {
                    iterations,
                    diagnostic_flags: flags,
                },
            )
            .state
        };
        let initial = run(0);
        let final_state = run(TRANSPORT_STEPS);
        let total = |state: &[f32]| {
            state
                .chunks_exact(4)
                .map(|pixel| pixel.iter().sum::<f32>())
                .sum::<f32>()
        };
        let initial_total = total(&initial);
        let final_total = total(&final_state);
        let drift = (final_total - initial_total).abs() / initial_total;
        println!(
            "transport-only moisture drift={:.4}% ({initial_total:.4} -> {final_total:.4})",
            drift * 100.0
        );
        assert!(
            drift <= 0.02,
            "transport-only moisture drift={drift:.4}, initial={initial_total:.4}, final={final_total:.4}"
        );

        let centroid_x = |state: &[f32]| {
            let mut weighted_x = 0.0;
            let mut total_mass = 0.0;
            for face in 0..6 {
                for y in 0..resolution {
                    for x in 0..resolution {
                        let pos = crate::cube_sphere::cube_to_sphere(
                            face,
                            x as f32 / (resolution - 1) as f32,
                            y as f32 / (resolution - 1) as f32,
                        );
                        let index =
                            ((face * resolution * resolution + y * resolution + x) * 4) as usize;
                        let mass = state[index + 1..index + 4].iter().sum::<f32>();
                        weighted_x += pos[0] * mass;
                        total_mass += mass;
                    }
                }
            }
            weighted_x / total_mass
        };
        let initial_x = centroid_x(&initial);
        let final_x = centroid_x(&final_state);
        println!(
            "transport-only redistribution: centroid_x={initial_x:.4} -> {final_x:.4}, delta={:.4}",
            final_x - initial_x
        );
        assert!(
            final_x > initial_x + 0.005,
            "transport did not move the asymmetric pulse downwind: {initial_x:.4} -> {final_x:.4}"
        );

        assert!(
            initial
                .iter()
                .chain(&final_state)
                .all(|value| value.is_finite() && *value >= 0.0),
            "transport-only state contains non-finite or negative moisture"
        );
    }

    #[test]
    fn spinup_parity_switches_pingpong_target_between_even_and_odd_iterations() {
        let resolution = 12;
        let gpu = GpuContext::new().expect("GPU init failed");
        let terrain = terrain_from(resolution, |pos| pos[0] * 0.15 - 0.1);
        let wind_pipeline = WindFieldPipeline::new(&gpu).expect("dynamics unavailable");
        let dynamics = wind_pipeline.create_test_textures(&gpu, resolution, |pos| {
            let _ = pos;
            let east_wind = [0.0, 0.0, 0.0, 1.0];
            (east_wind, 1013.0)
        });

        let baseline = [0.6_f32, 0.2_f32, 0.0_f32, 1.0_f32];
        let diff_count = |a: &[f32], b: &[f32]| {
            a.chunks_exact(4)
                .zip(b.chunks_exact(4))
                .filter(|(lhs, rhs)| {
                    (0..4).any(|channel| (lhs[channel] - rhs[channel]).abs() > 0.01)
                })
                .count()
        };

        let base_snapshot = snapshot(resolution);
        let inputs = [base_snapshot; 4];
        assert!(
            inputs
                .iter()
                .all(|input| bytemuck::bytes_of(input) == bytemuck::bytes_of(&base_snapshot))
        );
        let mut snapshots = Vec::new();
        for (spinup_iterations, input) in inputs.iter().enumerate() {
            let mut pipeline = WeatherFieldPipeline::new(&gpu).expect("weather unavailable");
            pipeline.spinup_iterations = spinup_iterations;
            let weather = pipeline.create_textures(&gpu, resolution);
            write_texture_rgba16f(&gpu, &weather._mass_texture, resolution, baseline);
            let pixels = run_spinup_pass(
                &gpu,
                &pipeline,
                &terrain,
                &dynamics,
                *input,
                &weather,
                spinup_iterations,
            );
            snapshots.push(pixels.mass);
        }

        assert!(
            diff_count(&snapshots[0], &snapshots[1]) >= (resolution * resolution * 6) as usize / 16,
            "odd iteration did not switch output source: changed_0_1={}",
            diff_count(&snapshots[0], &snapshots[1])
        );
        assert!(
            diff_count(&snapshots[2], &snapshots[3]) >= (resolution * resolution * 6) as usize / 16,
            "odd iteration did not switch output source: changed_2_3={}",
            diff_count(&snapshots[2], &snapshots[3])
        );
    }
}
