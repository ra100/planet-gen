use crate::gpu::GpuContext;
use crate::terrain_compute::{DynamicsTextures, TectonicTerrain};
use bytemuck::{Pod, Zeroable};
use std::sync::mpsc;
use wgpu::util::DeviceExt;

/// Preview weather stays independent of viewport and export resolution.
pub const DEFAULT_WEATHER_RESOLUTION: u32 = 384;

/// One `Rgba16Float` cubemap uses `6 * resolution^2 * 8` bytes.
/// Channel contract: R low-cloud coverage [0, 1], G base altitude [km],
/// B thickness [km], A cloud character [0, 1]. U3 derives sparse cirrus
/// from character and the same weather inputs instead of allocating a second field.
#[repr(C)]
#[derive(Copy, Clone, Pod, Zeroable)]
pub struct WeatherSnapshot {
    pub face: u32,
    pub resolution: u32,
    pub seed: u32,
    pub storm_count: u32,
    pub coverage: f32,
    pub moisture: f32,
    pub atm_pressure: f32,
    pub base_temp_c: f32,
    pub ocean_level: f32,
    pub axial_tilt_rad: f32,
    pub season: f32,
    pub storm_size: f32,
    pub cloud_character: f32,
    pub _pad0: f32,
    pub _pad1: f32,
    pub _pad2: f32,
}

pub struct WeatherTextures {
    _texture: wgpu::Texture,
    storage: wgpu::TextureView,
    pub weather: wgpu::TextureView,
    pub resolution: u32,
}

#[derive(Default)]
struct RevisionState {
    requested: u64,
    submitted: Option<u64>,
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
        self.submitted = Some(self.requested);
        Some((self.requested, snapshot))
    }

    fn complete(&mut self, revision: u64) -> bool {
        if self.submitted != Some(revision) {
            return false;
        }
        self.submitted = None;
        self.ready = Some(revision);
        revision == self.requested
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

    pub fn mark_submitted(&self, queue: &wgpu::Queue, revision: u64) {
        let completed_tx = self.completed_tx.clone();
        queue.on_submitted_work_done(move || {
            let _ = completed_tx.send(revision);
        });
    }

    /// Poll completion callbacks without waiting; only the latest completed revision publishes.
    pub fn poll(&mut self) -> bool {
        let mut published = false;
        while let Ok(revision) = self.completed_rx.try_recv() {
            if self.revisions.complete(revision) {
                self.front_is_a = !self.front_is_a;
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
    bind_group_layout: wgpu::BindGroupLayout,
    sampler: wgpu::Sampler,
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
        let sampler = gpu.device.create_sampler(&wgpu::SamplerDescriptor {
            label: Some("weather field sampler"),
            mag_filter: wgpu::FilterMode::Linear,
            min_filter: wgpu::FilterMode::Linear,
            ..Default::default()
        });
        Ok(Self {
            pipeline,
            bind_group_layout,
            sampler,
        })
    }

    pub fn create_textures(&self, gpu: &GpuContext, resolution: u32) -> WeatherTextures {
        let texture = gpu.device.create_texture(&wgpu::TextureDescriptor {
            label: Some("weather cubemap"),
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
                | wgpu::TextureUsages::COPY_SRC,
            view_formats: &[],
        });
        let storage = texture.create_view(&wgpu::TextureViewDescriptor {
            dimension: Some(wgpu::TextureViewDimension::D2Array),
            ..Default::default()
        });
        let weather = texture.create_view(&wgpu::TextureViewDescriptor {
            dimension: Some(wgpu::TextureViewDimension::Cube),
            ..Default::default()
        });
        WeatherTextures {
            _texture: texture,
            storage,
            weather,
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
        let workgroups = weather.resolution.div_ceil(16);
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
                        resource: wgpu::BindingResource::TextureView(&weather.storage),
                    },
                ],
            });
            let mut encoder = gpu.device.create_command_encoder(&Default::default());
            {
                let mut pass = encoder.begin_compute_pass(&Default::default());
                pass.set_pipeline(&self.pipeline);
                pass.set_bind_group(0, &bind_group, &[]);
                pass.dispatch_workgroups(workgroups, workgroups, 1);
            }
            gpu.queue.submit(Some(encoder.finish()));
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::terrain_compute::WindFieldPipeline;

    fn terrain(resolution: u32) -> TectonicTerrain {
        TectonicTerrain {
            faces: std::array::from_fn(|face| {
                (0..resolution * resolution)
                    .map(|index| {
                        if (index + face as u32) % 5 == 0 {
                            0.3
                        } else {
                            -0.1
                        }
                    })
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
            atm_pressure: 1.0,
            base_temp_c: 15.0,
            ocean_level: 0.0,
            axial_tilt_rad: 0.4,
            season: 0.5,
            storm_size: 1.0,
            cloud_character: 0.5,
            _pad0: 0.0,
            _pad1: 0.0,
            _pad2: 0.0,
        }
    }

    fn read_texture(gpu: &GpuContext, texture: &wgpu::Texture, resolution: u32) -> Vec<f32> {
        let unpadded_bytes_per_row = resolution * 8;
        let bytes_per_row = unpadded_bytes_per_row.div_ceil(256) * 256;
        let buffer = gpu.device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("weather test readback"),
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
        mapped
            .chunks_exact(bytes_per_row as usize)
            .flat_map(|row| row[..unpadded_bytes_per_row as usize].chunks_exact(2))
            .map(|bytes| half::f16::from_bits(u16::from_le_bytes([bytes[0], bytes[1]])).to_f32())
            .collect()
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

    #[test]
    fn weather_is_deterministic_finite_and_clear_without_moisture() {
        let gpu = GpuContext::new().expect("GPU init failed");
        let terrain = terrain(16);
        let wind = WindFieldPipeline::new(&gpu).expect("dynamics unavailable");
        let dynamics = wind.create_textures(&gpu, 16);
        wind.generate_gpu(&gpu, &terrain, &dynamics, 42, 0.0, 0.4, 0.5, 1.0, 15.0, 1.0);
        let pipeline = WeatherFieldPipeline::new(&gpu).expect("weather unavailable");
        let first = generate_weather(&gpu, &pipeline, &dynamics, &terrain, snapshot(16));
        let second = generate_weather(&gpu, &pipeline, &dynamics, &terrain, snapshot(16));
        let a = read_texture(&gpu, &first._texture, 16);
        let b = read_texture(&gpu, &second._texture, 16);
        assert_eq!(a, b);
        assert!(a.iter().all(|value| value.is_finite()));
        assert!(a.chunks_exact(4).all(|pixel| {
            (0.0..=1.0).contains(&pixel[0])
                && (0.5..=3.0).contains(&pixel[1])
                && (0.2..=5.0).contains(&pixel[2])
                && (0.0..=1.0).contains(&pixel[3])
        }));
        let pixel = |face: usize, x: usize, y: usize| &a[((face * 16 + y) * 16 + x) * 4..][..4];
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

        let mut clear = snapshot(16);
        clear.moisture = 0.0;
        let clear = generate_weather(&gpu, &pipeline, &dynamics, &terrain, clear);
        assert!(
            read_texture(&gpu, &clear._texture, 16)
                .chunks_exact(4)
                .all(|pixel| pixel[0] == 0.0)
        );
    }

    #[test]
    fn weather_seed_and_temperature_change_the_field() {
        let gpu = GpuContext::new().expect("GPU init failed");
        let terrain = terrain(16);
        let wind = WindFieldPipeline::new(&gpu).expect("dynamics unavailable");
        let dynamics = wind.create_textures(&gpu, 16);
        wind.generate_gpu(&gpu, &terrain, &dynamics, 42, 0.0, 0.4, 0.5, 1.0, 15.0, 1.0);
        let pipeline = WeatherFieldPipeline::new(&gpu).expect("weather unavailable");
        let baseline = read_texture(
            &gpu,
            &generate_weather(&gpu, &pipeline, &dynamics, &terrain, snapshot(16))._texture,
            16,
        );
        let mut seeded = snapshot(16);
        seeded.seed = 43;
        let seeded = read_texture(
            &gpu,
            &generate_weather(&gpu, &pipeline, &dynamics, &terrain, seeded)._texture,
            16,
        );
        assert_ne!(baseline, seeded);
        let mut warm = snapshot(16);
        warm.base_temp_c = 35.0;
        let warm = read_texture(
            &gpu,
            &generate_weather(&gpu, &pipeline, &dynamics, &terrain, warm)._texture,
            16,
        );
        assert_ne!(baseline, warm);
    }

    #[test]
    fn revisions_coalesce_and_never_publish_stale_weather() {
        let mut revisions = RevisionState::default();
        revisions.request(snapshot(16));
        let (first, _) = revisions.next_submission().unwrap();
        revisions.request(snapshot(16));
        revisions.request(snapshot(16));
        assert!(!revisions.complete(first));
        let (latest, _) = revisions.next_submission().unwrap();
        assert_eq!(latest, 3);
        assert!(revisions.complete(latest));
        assert_eq!(revisions.ready, Some(3));
    }
}
