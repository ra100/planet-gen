use crate::gpu::GpuContext;
use crate::terrain_compute::{DynamicsTextures, TectonicTerrain};
use bytemuck::{Pod, Zeroable};
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

pub struct WeatherTextures {
    _mass_texture: wgpu::Texture,
    _geometry_texture: wgpu::Texture,
    mass_storage: wgpu::TextureView,
    geometry_storage: wgpu::TextureView,
    pub mass: wgpu::TextureView,
    pub geometry: wgpu::TextureView,
    pub resolution: u32,
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
                    | wgpu::TextureUsages::COPY_SRC,
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
                        resource: wgpu::BindingResource::TextureView(&weather.mass_storage),
                    },
                    wgpu::BindGroupEntry {
                        binding: 6,
                        resource: wgpu::BindingResource::TextureView(&weather.geometry_storage),
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
            (0.25..=0.6).contains(&occupied_fraction),
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
                        0.0,
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
        assert!((east_asymmetry - calm_asymmetry) * (west_asymmetry - calm_asymmetry) < 0.0);
    }

    #[test]
    fn fronts_use_signed_gradient_alignment() {
        let resolution = 32;
        let gpu = GpuContext::new().expect("GPU init failed");
        let terrain = terrain_from(resolution, |_| -0.1);
        let wind_pipeline = WindFieldPipeline::new(&gpu).expect("dynamics unavailable");
        let pipeline = WeatherFieldPipeline::new(&gpu).expect("weather unavailable");
        let generate = |pressure_sign: f32| {
            let dynamics = wind_pipeline.create_test_textures(&gpu, resolution, |pos| {
                ([0.0; 4], 1013.0 + pressure_sign * pos[1] * 40.0)
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
                        ..snapshot(resolution)
                    },
                )
                ._mass_texture,
                resolution,
            )
        };
        let aligned = generate(1.0);
        let opposed = generate(-1.0);
        let northern_high = |values: &[f32]| {
            channel_sum_where(values, resolution, 2, |pos| pos[1] > 0.25 && pos[1] < 0.85)
        };
        assert!(northern_high(&aligned) > northern_high(&opposed) * 1.25);
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
}
