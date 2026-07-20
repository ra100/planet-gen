use crate::gpu::GpuContext;
use crate::terrain_compute::TectonicTerrain;
use bytemuck::{Pod, Zeroable};
use wgpu::util::DeviceExt;

pub const DEFAULT_PREVIEW_SIZE: u32 = 768;

#[repr(C)]
#[derive(Copy, Clone, Pod, Zeroable)]
pub struct PreviewUniforms {
    pub rotation: [[f32; 4]; 4],
    pub light_dir: [f32; 3],
    pub ocean_level: f32,
    // Planet properties
    pub base_temp_c: f32,
    pub ocean_fraction: f32,
    pub axial_tilt_rad: f32,
    pub view_mode: u32,
    pub season: f32,             // 0=winter, 0.5=equinox, 1=summer
    pub atmosphere_density: f32, // 0.0 = none, 1.0 = Earth-like (reserved for future)
    pub atmosphere_height: f32,  // scale height in planet radii (reserved for future)
    pub height_scale: f32,       // normal map height exaggeration (1.0 = subtle, 5.0 = dramatic)
    pub zoom: f32,               // viewport zoom (1.0 = default, >1 = zoomed in)
    pub pan_x: f32,              // viewport pan in NDC units
    pub pan_y: f32,
    pub cloud_coverage: f32,  // 0.0 = clear, 1.0 = overcast
    pub cloud_seed: u32,      // noise seed for cloud pattern
    pub night_lights: f32,    // 0.0 = pristine, 1.0 = heavily urbanized
    pub star_color_temp: f32, // 0.0 = blue hot star, 0.5 = sun-like, 1.0 = red dwarf
    pub city_light_hue: f32,  // 0.0 = warm amber, 0.5 = white, 1.0 = cool blue
    pub show_ao: f32,         // 1.0 = AO enabled, 0.0 = disabled
    // Layer toggles (1.0 = enabled, 0.0 = disabled)
    pub show_water: f32,
    pub show_ice: f32,
    pub show_biomes: f32,
    pub show_clouds: f32,
    pub show_atmosphere_layer: f32,
    pub show_cities: f32,
    pub cloud_opacity: f32,
    pub cloud_advection: f32, // 1.0 = advected cubemap modulates clouds, 0.0 = per-pixel only
    pub rotation_rate: f32,   // relative to Earth (1.0 = 24h day)
    pub atm_pressure: f32,    // atmospheric pressure in bar (1.0 = Earth)
    pub _pad4: f32,           // retained uniform slot after U15 removes render-only wind strength
    pub lava_glow: f32,       // tectonic emission intensity (0.0-1.0)
    pub ring_inner: f32,      // ring system inner radius (planet radii, 0 = no rings)
    pub ring_outer: f32,      // ring system outer radius
    pub ring_tilt: f32,       // ring plane tilt angle (radians)
    pub ring_opacity: f32,    // ring opacity (0-1)
    pub planet_radius_km: f32,
    pub show_cloud_shadows: f32,
    pub _pad5: f32,
}

pub struct PreviewRenderer {
    pipeline: wgpu::RenderPipeline,
    interactive_pipeline: wgpu::RenderPipeline,
    bind_group_layout: wgpu::BindGroupLayout,
    sampler: wgpu::Sampler,
    target: wgpu::Texture,
    target_view: wgpu::TextureView,
    pub size: u32,
}

impl PreviewRenderer {
    pub fn new(gpu: &GpuContext) -> Self {
        Self::new_with_cloud_config(gpu, [1.0; 3], 8)
    }

    pub fn new_with_cloud_detail(gpu: &GpuContext, detail_strength: f32) -> Self {
        Self::new_with_cloud_config(gpu, [detail_strength; 3], 8)
    }

    pub fn new_with_cloud_detail_layers(gpu: &GpuContext, detail_strength: [f32; 3]) -> Self {
        Self::new_with_cloud_config(gpu, detail_strength, 8)
    }

    pub fn new_with_cloud_samples(gpu: &GpuContext, samples: u32) -> Self {
        Self::new_with_cloud_config(gpu, [1.0; 3], samples)
    }

    fn new_with_cloud_config(gpu: &GpuContext, detail_strength: [f32; 3], samples: u32) -> Self {
        let cloud_density = include_str!("shaders/cloud_density.wgsl")
            .replace(
                "const LOW_DETAIL_STRENGTH: f32 = 1.0;",
                &format!("const LOW_DETAIL_STRENGTH: f32 = {};", detail_strength[0]),
            )
            .replace(
                "const DEEP_DETAIL_STRENGTH: f32 = 1.0;",
                &format!("const DEEP_DETAIL_STRENGTH: f32 = {};", detail_strength[1]),
            )
            .replace(
                "const HIGH_DETAIL_STRENGTH: f32 = 1.0;",
                &format!("const HIGH_DETAIL_STRENGTH: f32 = {};", detail_strength[2]),
            );
        let preview_shader = include_str!("shaders/preview_cubemap.wgsl").replace(
            "const CLOUD_RAY_SAMPLES: u32 = 8u;",
            &format!("const CLOUD_RAY_SAMPLES: u32 = {samples}u;"),
        );
        let shader_source = format!(
            "{}\n{}\n{}",
            include_str!("shaders/noise.wgsl"),
            cloud_density,
            preview_shader,
        );

        let shader = gpu
            .device
            .create_shader_module(wgpu::ShaderModuleDescriptor {
                label: Some("preview cubemap shader"),
                source: wgpu::ShaderSource::Wgsl(shader_source.into()),
            });

        let bind_group_layout =
            gpu.device
                .create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
                    label: Some("preview bgl"),
                    entries: &[
                        // Uniforms
                        wgpu::BindGroupLayoutEntry {
                            binding: 0,
                            visibility: wgpu::ShaderStages::VERTEX | wgpu::ShaderStages::FRAGMENT,
                            ty: wgpu::BindingType::Buffer {
                                ty: wgpu::BufferBindingType::Uniform,
                                has_dynamic_offset: false,
                                min_binding_size: None,
                            },
                            count: None,
                        },
                        // Height cubemap
                        wgpu::BindGroupLayoutEntry {
                            binding: 1,
                            visibility: wgpu::ShaderStages::FRAGMENT,
                            ty: wgpu::BindingType::Texture {
                                sample_type: wgpu::TextureSampleType::Float { filterable: true },
                                view_dimension: wgpu::TextureViewDimension::Cube,
                                multisampled: false,
                            },
                            count: None,
                        },
                        // Sampler
                        wgpu::BindGroupLayoutEntry {
                            binding: 2,
                            visibility: wgpu::ShaderStages::FRAGMENT,
                            ty: wgpu::BindingType::Sampler(wgpu::SamplerBindingType::Filtering),
                            count: None,
                        },
                        // Cloud density cubemap
                        wgpu::BindGroupLayoutEntry {
                            binding: 3,
                            visibility: wgpu::ShaderStages::FRAGMENT,
                            ty: wgpu::BindingType::Texture {
                                sample_type: wgpu::TextureSampleType::Float { filterable: true },
                                view_dimension: wgpu::TextureViewDimension::Cube,
                                multisampled: false,
                            },
                            count: None,
                        },
                        wgpu::BindGroupLayoutEntry {
                            binding: 4,
                            visibility: wgpu::ShaderStages::FRAGMENT,
                            ty: wgpu::BindingType::Texture {
                                sample_type: wgpu::TextureSampleType::Float { filterable: true },
                                view_dimension: wgpu::TextureViewDimension::Cube,
                                multisampled: false,
                            },
                            count: None,
                        },
                        wgpu::BindGroupLayoutEntry {
                            binding: 5,
                            visibility: wgpu::ShaderStages::FRAGMENT,
                            ty: wgpu::BindingType::Texture {
                                sample_type: wgpu::TextureSampleType::Float { filterable: true },
                                view_dimension: wgpu::TextureViewDimension::Cube,
                                multisampled: false,
                            },
                            count: None,
                        },
                    ],
                });

        let pipeline_layout = gpu
            .device
            .create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
                label: Some("preview pipeline layout"),
                bind_group_layouts: &[&bind_group_layout],
                push_constant_ranges: &[],
            });

        let create_pipeline = |format| {
            gpu.device
                .create_render_pipeline(&wgpu::RenderPipelineDescriptor {
                    label: Some("preview pipeline"),
                    layout: Some(&pipeline_layout),
                    vertex: wgpu::VertexState {
                        module: &shader,
                        entry_point: Some("vs_main"),
                        buffers: &[],
                        compilation_options: Default::default(),
                    },
                    fragment: Some(wgpu::FragmentState {
                        module: &shader,
                        entry_point: Some("fs_main"),
                        targets: &[Some(wgpu::ColorTargetState {
                            format,
                            blend: None,
                            write_mask: wgpu::ColorWrites::ALL,
                        })],
                        compilation_options: Default::default(),
                    }),
                    primitive: wgpu::PrimitiveState {
                        topology: wgpu::PrimitiveTopology::TriangleList,
                        ..Default::default()
                    },
                    depth_stencil: None,
                    multisample: wgpu::MultisampleState::default(),
                    multiview: None,
                    cache: None,
                })
        };
        let pipeline = create_pipeline(wgpu::TextureFormat::Rgba8UnormSrgb);
        let interactive_pipeline = create_pipeline(wgpu::TextureFormat::Rgba8Unorm);

        let sampler = gpu.device.create_sampler(&wgpu::SamplerDescriptor {
            label: Some("height sampler"),
            mag_filter: wgpu::FilterMode::Linear,
            min_filter: wgpu::FilterMode::Linear,
            ..Default::default()
        });

        let (target, target_view) = Self::create_target(gpu, DEFAULT_PREVIEW_SIZE);

        Self {
            pipeline,
            interactive_pipeline,
            bind_group_layout,
            sampler,
            target,
            target_view,
            size: DEFAULT_PREVIEW_SIZE,
        }
    }

    fn create_target(gpu: &GpuContext, size: u32) -> (wgpu::Texture, wgpu::TextureView) {
        let target = gpu.device.create_texture(&wgpu::TextureDescriptor {
            label: Some("preview render target"),
            size: wgpu::Extent3d {
                width: size,
                height: size,
                depth_or_array_layers: 1,
            },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: wgpu::TextureFormat::Rgba8Unorm,
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT
                | wgpu::TextureUsages::TEXTURE_BINDING
                | wgpu::TextureUsages::COPY_SRC,
            view_formats: &[],
        });
        let view = target.create_view(&Default::default());
        (target, view)
    }

    pub fn target_view(&self) -> &wgpu::TextureView {
        &self.target_view
    }

    /// Rebind the new view before dropping the old target.
    pub fn resize_target(
        &mut self,
        gpu: &GpuContext,
        size: u32,
        register: impl FnOnce(&wgpu::TextureView),
    ) {
        if self.size == size {
            return;
        }
        let (target, target_view) = Self::create_target(gpu, size);
        register(&target_view);
        self.target = target;
        self.target_view = target_view;
        self.size = size;
    }

    pub fn render_interactive(
        &self,
        gpu: &GpuContext,
        uniforms: &PreviewUniforms,
        cubemap_view: &wgpu::TextureView,
        cloud_view: Option<&wgpu::TextureView>,
        weather_views: Option<(&wgpu::TextureView, &wgpu::TextureView)>,
    ) {
        let mut encoder = gpu
            .device
            .create_command_encoder(&wgpu::CommandEncoderDescriptor {
                label: Some("preview encoder"),
            });
        self.encode_render(
            gpu,
            &mut encoder,
            &self.interactive_pipeline,
            uniforms,
            cubemap_view,
            cloud_view,
            weather_views,
            &self.target_view,
        );
        gpu.queue.submit(Some(encoder.finish()));
    }

    #[allow(clippy::too_many_arguments)]
    fn encode_render(
        &self,
        gpu: &GpuContext,
        encoder: &mut wgpu::CommandEncoder,
        pipeline: &wgpu::RenderPipeline,
        uniforms: &PreviewUniforms,
        cubemap_view: &wgpu::TextureView,
        cloud_view: Option<&wgpu::TextureView>,
        weather_views: Option<(&wgpu::TextureView, &wgpu::TextureView)>,
        render_view: &wgpu::TextureView,
    ) {
        let uniform_buffer = gpu
            .device
            .create_buffer_init(&wgpu::util::BufferInitDescriptor {
                label: Some("preview uniforms"),
                contents: bytemuck::bytes_of(uniforms),
                usage: wgpu::BufferUsages::UNIFORM,
            });

        let dummy_cloud_tex;
        let dummy_cloud_view;
        let effective_cloud_view = match cloud_view {
            Some(view) => view,
            None => {
                dummy_cloud_tex = gpu.device.create_texture(&wgpu::TextureDescriptor {
                    label: Some("dummy cloud"),
                    size: wgpu::Extent3d {
                        width: 1,
                        height: 1,
                        depth_or_array_layers: 6,
                    },
                    mip_level_count: 1,
                    sample_count: 1,
                    dimension: wgpu::TextureDimension::D2,
                    format: wgpu::TextureFormat::R16Float,
                    usage: wgpu::TextureUsages::TEXTURE_BINDING,
                    view_formats: &[],
                });
                dummy_cloud_view = dummy_cloud_tex.create_view(&wgpu::TextureViewDescriptor {
                    dimension: Some(wgpu::TextureViewDimension::Cube),
                    ..Default::default()
                });
                &dummy_cloud_view
            }
        };
        let dummy_weather_mass_tex;
        let dummy_weather_geometry_tex;
        let dummy_weather_mass_view;
        let dummy_weather_geometry_view;
        let (effective_mass_view, effective_geometry_view) = if let Some(views) = weather_views {
            views
        } else {
            let create_zero_weather = |label| {
                gpu.device.create_texture(&wgpu::TextureDescriptor {
                    label: Some(label),
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
                })
            };
            dummy_weather_mass_tex = create_zero_weather("zero weather mass");
            dummy_weather_geometry_tex = create_zero_weather("zero weather geometry");
            dummy_weather_mass_view =
                dummy_weather_mass_tex.create_view(&wgpu::TextureViewDescriptor {
                    dimension: Some(wgpu::TextureViewDimension::Cube),
                    ..Default::default()
                });
            dummy_weather_geometry_view =
                dummy_weather_geometry_tex.create_view(&wgpu::TextureViewDescriptor {
                    dimension: Some(wgpu::TextureViewDimension::Cube),
                    ..Default::default()
                });
            (&dummy_weather_mass_view, &dummy_weather_geometry_view)
        };
        let bind_group = gpu.device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("preview bind group"),
            layout: &self.bind_group_layout,
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: uniform_buffer.as_entire_binding(),
                },
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: wgpu::BindingResource::TextureView(cubemap_view),
                },
                wgpu::BindGroupEntry {
                    binding: 2,
                    resource: wgpu::BindingResource::Sampler(&self.sampler),
                },
                wgpu::BindGroupEntry {
                    binding: 3,
                    resource: wgpu::BindingResource::TextureView(effective_cloud_view),
                },
                wgpu::BindGroupEntry {
                    binding: 4,
                    resource: wgpu::BindingResource::TextureView(effective_mass_view),
                },
                wgpu::BindGroupEntry {
                    binding: 5,
                    resource: wgpu::BindingResource::TextureView(effective_geometry_view),
                },
            ],
        });

        let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
            label: Some("preview pass"),
            color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                view: render_view,
                resolve_target: None,
                depth_slice: None,
                ops: wgpu::Operations {
                    load: wgpu::LoadOp::Clear(wgpu::Color {
                        r: 0.02,
                        g: 0.02,
                        b: 0.05,
                        a: 1.0,
                    }),
                    store: wgpu::StoreOp::Store,
                },
            })],
            depth_stencil_attachment: None,
            timestamp_writes: None,
            occlusion_query_set: None,
        });
        pass.set_pipeline(pipeline);
        pass.set_bind_group(0, &bind_group, &[]);
        pass.draw(0..3, 0..1);
    }

    /// Upload terrain data to a cubemap texture (R16Float for filterability).
    pub fn upload_terrain(&self, gpu: &GpuContext, terrain: &TectonicTerrain) -> wgpu::TextureView {
        let res = terrain.resolution;

        let cubemap = gpu.device.create_texture(&wgpu::TextureDescriptor {
            label: Some("height cubemap"),
            size: wgpu::Extent3d {
                width: res,
                height: res,
                depth_or_array_layers: 6,
            },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: wgpu::TextureFormat::R16Float,
            usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
            view_formats: &[],
        });

        for (i, face_data) in terrain.faces.iter().enumerate() {
            let f16_data: Vec<u16> = face_data
                .iter()
                .map(|&v| half::f16::from_f32(v).to_bits())
                .collect();
            gpu.queue.write_texture(
                wgpu::TexelCopyTextureInfo {
                    texture: &cubemap,
                    mip_level: 0,
                    origin: wgpu::Origin3d {
                        x: 0,
                        y: 0,
                        z: i as u32,
                    },
                    aspect: wgpu::TextureAspect::All,
                },
                bytemuck::cast_slice(&f16_data),
                wgpu::TexelCopyBufferLayout {
                    offset: 0,
                    bytes_per_row: Some(res * 2),
                    rows_per_image: Some(res),
                },
                wgpu::Extent3d {
                    width: res,
                    height: res,
                    depth_or_array_layers: 1,
                },
            );
        }

        cubemap.create_view(&wgpu::TextureViewDescriptor {
            dimension: Some(wgpu::TextureViewDimension::Cube),
            ..Default::default()
        })
    }

    /// Render the planet preview to an RGBA pixel buffer using a pre-computed cubemap.
    /// Upload arbitrary 6-face f32 data as R16Float cubemap.
    pub fn upload_cubemap_r16(
        &self,
        gpu: &GpuContext,
        faces: &[Vec<f32>; 6],
        res: u32,
    ) -> wgpu::TextureView {
        let cubemap = gpu.device.create_texture(&wgpu::TextureDescriptor {
            label: Some("data cubemap"),
            size: wgpu::Extent3d {
                width: res,
                height: res,
                depth_or_array_layers: 6,
            },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: wgpu::TextureFormat::R16Float,
            usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
            view_formats: &[],
        });
        for (i, face_data) in faces.iter().enumerate() {
            let f16_data: Vec<u16> = face_data
                .iter()
                .map(|&v| half::f16::from_f32(v).to_bits())
                .collect();
            gpu.queue.write_texture(
                wgpu::TexelCopyTextureInfo {
                    texture: &cubemap,
                    mip_level: 0,
                    origin: wgpu::Origin3d {
                        x: 0,
                        y: 0,
                        z: i as u32,
                    },
                    aspect: wgpu::TextureAspect::All,
                },
                bytemuck::cast_slice(&f16_data),
                wgpu::TexelCopyBufferLayout {
                    offset: 0,
                    bytes_per_row: Some(res * 2),
                    rows_per_image: Some(res),
                },
                wgpu::Extent3d {
                    width: res,
                    height: res,
                    depth_or_array_layers: 1,
                },
            );
        }
        cubemap.create_view(&wgpu::TextureViewDescriptor {
            dimension: Some(wgpu::TextureViewDimension::Cube),
            ..Default::default()
        })
    }

    /// Upload 6-face RGBA f32 data as Rgba16Float cubemap (4 channels per texel).
    pub fn upload_cubemap_rgba16(
        &self,
        gpu: &GpuContext,
        faces: &[Vec<f32>; 6],
        res: u32,
    ) -> wgpu::TextureView {
        let cubemap = gpu.device.create_texture(&wgpu::TextureDescriptor {
            label: Some("rgba data cubemap"),
            size: wgpu::Extent3d {
                width: res,
                height: res,
                depth_or_array_layers: 6,
            },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: wgpu::TextureFormat::Rgba16Float,
            usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
            view_formats: &[],
        });
        for (i, face_data) in faces.iter().enumerate() {
            let f16_data: Vec<u16> = face_data
                .iter()
                .map(|&v| half::f16::from_f32(v).to_bits())
                .collect();
            gpu.queue.write_texture(
                wgpu::TexelCopyTextureInfo {
                    texture: &cubemap,
                    mip_level: 0,
                    origin: wgpu::Origin3d {
                        x: 0,
                        y: 0,
                        z: i as u32,
                    },
                    aspect: wgpu::TextureAspect::All,
                },
                bytemuck::cast_slice(&f16_data),
                wgpu::TexelCopyBufferLayout {
                    offset: 0,
                    bytes_per_row: Some(res * 4 * 2),
                    rows_per_image: Some(res),
                },
                wgpu::Extent3d {
                    width: res,
                    height: res,
                    depth_or_array_layers: 1,
                },
            );
        }
        cubemap.create_view(&wgpu::TextureViewDescriptor {
            dimension: Some(wgpu::TextureViewDimension::Cube),
            ..Default::default()
        })
    }

    pub fn render(
        &self,
        gpu: &GpuContext,
        uniforms: &PreviewUniforms,
        cubemap_view: &wgpu::TextureView,
        cloud_view: Option<&wgpu::TextureView>,
        weather_views: Option<(&wgpu::TextureView, &wgpu::TextureView)>,
        render_size: u32,
    ) -> Vec<u8> {
        let size = render_size;

        let render_target = gpu.device.create_texture(&wgpu::TextureDescriptor {
            label: Some("preview readback target"),
            size: wgpu::Extent3d {
                width: size,
                height: size,
                depth_or_array_layers: 1,
            },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: wgpu::TextureFormat::Rgba8UnormSrgb,
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::COPY_SRC,
            view_formats: &[],
        });
        let render_view = render_target.create_view(&Default::default());

        let mut encoder = gpu
            .device
            .create_command_encoder(&wgpu::CommandEncoderDescriptor {
                label: Some("preview encoder"),
            });

        self.encode_render(
            gpu,
            &mut encoder,
            &self.pipeline,
            uniforms,
            cubemap_view,
            cloud_view,
            weather_views,
            &render_view,
        );

        // Readback
        let bytes_per_row = size * 4;
        let padded_bytes_per_row = (bytes_per_row + 255) & !255;
        let readback = gpu.device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("preview readback"),
            size: (padded_bytes_per_row * size) as u64,
            usage: wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::MAP_READ,
            mapped_at_creation: false,
        });

        encoder.copy_texture_to_buffer(
            wgpu::TexelCopyTextureInfo {
                texture: &render_target,
                mip_level: 0,
                origin: wgpu::Origin3d::ZERO,
                aspect: wgpu::TextureAspect::All,
            },
            wgpu::TexelCopyBufferInfo {
                buffer: &readback,
                layout: wgpu::TexelCopyBufferLayout {
                    offset: 0,
                    bytes_per_row: Some(padded_bytes_per_row),
                    rows_per_image: Some(size),
                },
            },
            wgpu::Extent3d {
                width: size,
                height: size,
                depth_or_array_layers: 1,
            },
        );

        gpu.queue.submit(Some(encoder.finish()));

        readback.slice(..).map_async(wgpu::MapMode::Read, |_| {});
        let _ = gpu.device.poll(wgpu::PollType::Wait {
            submission_index: None,
            timeout: None,
        });

        let mapped = readback.slice(..).get_mapped_range();
        let mut pixels = Vec::with_capacity((size * size * 4) as usize);
        for row in 0..size {
            let start = (row * padded_bytes_per_row) as usize;
            let end = start + (size * 4) as usize;
            pixels.extend_from_slice(&mapped[start..end]);
        }
        pixels
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cube_sphere::cube_to_sphere;
    use crate::gpu::GpuContext;
    use crate::plates::{PlateGenParams, generate_plates};
    use crate::terrain_compute::{TectonicTerrain, TerrainComputePipeline, WindFieldPipeline};
    use crate::weather::{WeatherFieldPipeline, WeatherSnapshot};

    fn uniforms() -> PreviewUniforms {
        PreviewUniforms {
            rotation: [
                [1.0, 0.0, 0.0, 0.0],
                [0.0, 1.0, 0.0, 0.0],
                [0.0, 0.0, 1.0, 0.0],
                [0.0, 0.0, 0.0, 1.0],
            ],
            light_dir: [0.5, 0.7, -1.0],
            ocean_level: -0.1,
            base_temp_c: 15.0,
            ocean_fraction: 0.7,
            axial_tilt_rad: 0.41,
            view_mode: 0,
            season: 0.5,
            atmosphere_density: 0.0,
            atmosphere_height: 0.0,
            height_scale: 3.0,
            zoom: 1.0,
            pan_x: 0.0,
            pan_y: 0.0,
            cloud_coverage: 0.5,
            cloud_seed: 42,
            night_lights: 0.0,
            star_color_temp: 0.5,
            city_light_hue: 0.0,
            show_ao: 1.0,
            show_water: 1.0,
            show_ice: 1.0,
            show_biomes: 1.0,
            show_clouds: 0.0,
            show_atmosphere_layer: 0.0,
            show_cities: 0.0,
            cloud_opacity: 1.0,
            cloud_advection: 0.0,
            rotation_rate: 1.0,
            atm_pressure: 0.7,
            _pad4: 0.0,
            lava_glow: 0.0,
            ring_inner: 0.0,
            ring_outer: 0.0,
            ring_tilt: 0.0,
            ring_opacity: 0.0,
            planet_radius_km: 6371.0,
            show_cloud_shadows: 1.0,
            _pad5: 0.0,
        }
    }

    fn contour_fixture(resolution: u32) -> ([Vec<f32>; 6], [Vec<f32>; 6]) {
        let smooth = |edge0: f32, edge1: f32, value: f32| {
            let t = ((value - edge0) / (edge1 - edge0)).clamp(0.0, 1.0);
            t * t * (3.0 - 2.0 * t)
        };
        let mut mass = std::array::from_fn(|_| Vec::new());
        let mut geometry = std::array::from_fn(|_| Vec::new());
        for face in 0..6 {
            for y in 0..resolution {
                for x in 0..resolution {
                    let p = cube_to_sphere(
                        face,
                        x as f32 / (resolution - 1) as f32,
                        y as f32 / (resolution - 1) as f32,
                    );
                    let low_distance =
                        ((p[0] + 0.16).powi(2) + (p[1] - 0.04).powi(2) + (p[2] - 0.98).powi(2))
                            .sqrt();
                    let deep_distance =
                        ((p[0] - 0.27).powi(2) + (p[1] + 0.12).powi(2) + (p[2] - 0.95).powi(2))
                            .sqrt();
                    let low = smooth(0.72, 0.12, low_distance) * 0.78;
                    let deep = smooth(0.42, 0.08, deep_distance) * 0.82;
                    mass[face as usize].extend_from_slice(&[low, deep, 0.0, low.max(deep)]);
                    geometry[face as usize].extend_from_slice(&[0.6, 2.2, 10.0, 12.0]);
                }
            }
        }
        (mass, geometry)
    }

    fn layered_fixture(resolution: u32) -> ([Vec<f32>; 6], [Vec<f32>; 6]) {
        let smooth = |edge0: f32, edge1: f32, value: f32| {
            let t = ((value - edge0) / (edge1 - edge0)).clamp(0.0, 1.0);
            t * t * (3.0 - 2.0 * t)
        };
        let mut mass = std::array::from_fn(|_| Vec::new());
        let geometry = std::array::from_fn(|_| {
            (0..resolution * resolution)
                .flat_map(|_| [0.4, 2.0, 7.0, 11.0])
                .collect()
        });
        for face in 0..6 {
            for y in 0..resolution {
                for x in 0..resolution {
                    let p = cube_to_sphere(
                        face,
                        x as f32 / (resolution - 1) as f32,
                        y as f32 / (resolution - 1) as f32,
                    );
                    let low = smooth(
                        0.62,
                        0.12,
                        ((p[0] + 0.28).powi(2) + (p[1] + 0.08).powi(2) + (p[2] - 0.95).powi(2))
                            .sqrt(),
                    ) * 0.82;
                    let deep = smooth(
                        0.46,
                        0.10,
                        ((p[0] - 0.28).powi(2) + (p[1] + 0.08).powi(2) + (p[2] - 0.95).powi(2))
                            .sqrt(),
                    ) * 0.86;
                    let high = smooth(
                        0.50,
                        0.10,
                        (p[0].powi(2) + (p[1] - 0.33).powi(2) + (p[2] - 0.94).powi(2)).sqrt(),
                    ) * 0.90;
                    // Alpha is low/deep occupancy only: high must remain renderable without it.
                    mass[face as usize].extend_from_slice(&[low, deep, high, low.max(deep)]);
                }
            }
        }
        (mass, geometry)
    }

    fn srgb_to_linear(value: u8) -> f32 {
        let value = value as f32 / 255.0;
        if value <= 0.04045 {
            value / 12.92
        } else {
            ((value + 0.055) / 1.055).powf(2.4)
        }
    }

    fn density_values(pixels: &[u8]) -> Vec<f32> {
        pixels
            .chunks_exact(4)
            .map(|pixel| srgb_to_linear(pixel[0]))
            .collect()
    }

    fn optical_depth(opacity: f32) -> f32 {
        -(1.0 - opacity).max(f32::MIN_POSITIVE).ln()
    }

    fn mean_color_difference(a: &[u8], b: &[u8], size: usize) -> f32 {
        let (difference, count) = a
            .chunks_exact(4)
            .zip(b.chunks_exact(4))
            .enumerate()
            .filter(|(index, _)| sphere_mask(size, *index))
            .fold((0.0, 0usize), |(difference, count), (_, (a, b))| {
                let delta = (0..3)
                    .map(|channel| (srgb_to_linear(a[channel]) - srgb_to_linear(b[channel])).abs())
                    .sum::<f32>()
                    / 3.0;
                (difference + delta, count + 1)
            });
        difference / count.max(1) as f32
    }

    fn percentile(mut values: Vec<f32>, percentile: f32) -> f32 {
        values.sort_by(f32::total_cmp);
        values[((values.len() - 1) as f32 * percentile).round() as usize]
    }

    fn mean_optical_depth_difference(a: &[u8], b: &[u8], size: usize) -> f32 {
        let mut total = 0.0;
        let mut count = 0usize;
        for (index, (a, b)) in a.chunks_exact(4).zip(b.chunks_exact(4)).enumerate() {
            if sphere_mask(size, index) {
                total += (optical_depth(srgb_to_linear(a[0]))
                    - optical_depth(srgb_to_linear(b[0])))
                .abs();
                count += 1;
            }
        }
        total / count.max(1) as f32
    }

    fn sphere_mask(size: usize, index: usize) -> bool {
        let x = index % size;
        let y = index / size;
        let ndc_x = ((x as f32 + 0.5) / size as f32 - 0.5) * 2.0 / 0.85;
        let ndc_y = ((y as f32 + 0.5) / size as f32 - 0.5) * 2.0 / 0.85;
        ndc_x * ndc_x + ndc_y * ndc_y <= 1.0
    }

    fn box_blur(values: &[f32], size: usize, radius: usize) -> Vec<f32> {
        (0..values.len())
            .map(|index| {
                let x = index % size;
                let y = index / size;
                let mut sum = 0.0;
                let mut count = 0;
                for sample_y in y.saturating_sub(radius)..=(y + radius).min(size - 1) {
                    for sample_x in x.saturating_sub(radius)..=(x + radius).min(size - 1) {
                        sum += values[sample_y * size + sample_x];
                        count += 1;
                    }
                }
                sum / count as f32
            })
            .collect()
    }

    fn correlation(a: &[f32], b: &[f32], size: usize) -> f32 {
        let samples: Vec<_> = a
            .iter()
            .zip(b)
            .enumerate()
            .filter(|(index, _)| sphere_mask(size, *index))
            .map(|(_, (&a, &b))| (a, b))
            .collect();
        let mean_a = samples.iter().map(|sample| sample.0).sum::<f32>() / samples.len() as f32;
        let mean_b = samples.iter().map(|sample| sample.1).sum::<f32>() / samples.len() as f32;
        let covariance = samples
            .iter()
            .map(|sample| (sample.0 - mean_a) * (sample.1 - mean_b))
            .sum::<f32>();
        let variance_a = samples
            .iter()
            .map(|sample| (sample.0 - mean_a).powi(2))
            .sum::<f32>();
        let variance_b = samples
            .iter()
            .map(|sample| (sample.1 - mean_b).powi(2))
            .sum::<f32>();
        covariance / (variance_a * variance_b).sqrt().max(f32::EPSILON)
    }

    fn edge_energy(values: &[f32], size: usize) -> f32 {
        let mut energy = 0.0;
        let mut count = 0;
        for y in 1..size - 1 {
            for x in 1..size - 1 {
                let index = y * size + x;
                if sphere_mask(size, index) {
                    energy += (values[index + 1] - values[index - 1]).abs()
                        + (values[index + size] - values[index - size]).abs();
                    count += 1;
                }
            }
        }
        energy / count as f32
    }

    fn source_support(
        mass: &[Vec<f32>; 6],
        channel: usize,
        size: usize,
        resolution: usize,
    ) -> Vec<bool> {
        let face_pixel = |direction: [f32; 3]| {
            let (axis, value) = direction
                .iter()
                .enumerate()
                .max_by(|(_, a), (_, b)| a.abs().total_cmp(&b.abs()))
                .map(|(axis, value)| (axis, *value))
                .unwrap();
            let (face, s, t) = match (axis, value.is_sign_positive()) {
                (0, true) => (0, -direction[2] / value, -direction[1] / value),
                (0, false) => (1, direction[2] / -value, -direction[1] / -value),
                (1, true) => (2, direction[0] / value, direction[2] / value),
                (1, false) => (3, direction[0] / -value, -direction[2] / -value),
                (2, true) => (4, direction[0] / value, -direction[1] / value),
                (2, false) => (5, -direction[0] / -value, -direction[1] / -value),
                _ => unreachable!(),
            };
            let x =
                (((s + 1.0) * 0.5 * (resolution - 1) as f32).round() as usize).min(resolution - 1);
            let y =
                (((t + 1.0) * 0.5 * (resolution - 1) as f32).round() as usize).min(resolution - 1);
            (face, x, y)
        };
        (0..size * size)
            .map(|index| {
                if !sphere_mask(size, index) {
                    return false;
                }
                let x = index % size;
                let y = index / size;
                let ndc_x = ((x as f32 + 0.5) / size as f32 - 0.5) * 2.0 / 0.85;
                let ndc_y = ((y as f32 + 0.5) / size as f32 - 0.5) * 2.0 / 0.85;
                let direction = [ndc_x, ndc_y, (1.0 - ndc_x * ndc_x - ndc_y * ndc_y).sqrt()];
                let (face, x, y) = face_pixel(direction);
                (y.saturating_sub(3)..=(y + 3).min(resolution - 1)).any(|sample_y| {
                    (x.saturating_sub(3)..=(x + 3).min(resolution - 1)).any(|sample_x| {
                        mass[face][(sample_y * resolution + sample_x) * 4 + channel] > 0.0
                    })
                })
            })
            .collect()
    }

    fn masked_density_mae(a: &[f32], b: &[f32], mask: &[bool]) -> f32 {
        let (sum, count) = a
            .iter()
            .zip(b)
            .zip(mask)
            .filter(|(_, supported)| **supported)
            .fold((0.0, 0usize), |(sum, count), ((a, b), _)| {
                (sum + (a - b).abs(), count + 1)
            });
        sum / count.max(1) as f32
    }

    fn masked_edge_energy(values: &[f32], mask: &[bool], size: usize) -> f32 {
        let mut energy = 0.0;
        let mut count = 0;
        for y in 1..size - 1 {
            for x in 1..size - 1 {
                let index = y * size + x;
                if mask[index]
                    && mask[index + 1]
                    && mask[index - 1]
                    && mask[index + size]
                    && mask[index - size]
                {
                    energy += (values[index + 1] - values[index - 1]).abs()
                        + (values[index + size] - values[index - size]).abs();
                    count += 1;
                }
            }
        }
        energy / count.max(1) as f32
    }

    fn masked_centroid(values: &[f32], mask: &[bool], size: usize) -> (f32, f32) {
        let (weight, x_sum, y_sum) = values.iter().zip(mask).enumerate().fold(
            (0.0, 0.0, 0.0),
            |(weight, x_sum, y_sum), (index, (value, supported))| {
                if !*supported {
                    return (weight, x_sum, y_sum);
                }
                let x = (index % size) as f32;
                let y = (index / size) as f32;
                (weight + value, x_sum + x * value, y_sum + y * value)
            },
        );
        (
            x_sum / weight.max(f32::EPSILON),
            y_sum / weight.max(f32::EPSILON),
        )
    }

    #[derive(Debug)]
    struct LocalDetailMetrics {
        anisotropy_median: f32,
        axis_median_degrees: f32,
        axis_p90_degrees: f32,
        curvature_p95_degrees: f32,
        closed_winding_fraction: f32,
        autocorrelation: f32,
    }

    fn axis_delta(left: f32, right: f32) -> f32 {
        let delta = (left - right).abs().rem_euclid(std::f32::consts::PI);
        delta.min(std::f32::consts::PI - delta)
    }

    fn local_detail_metrics(values: &[f32], mask: &[bool], size: usize) -> LocalDetailMetrics {
        const RADIUS: usize = 12;
        let mut patches = Vec::new();
        for y in (RADIUS..size - RADIUS).step_by(RADIUS) {
            for x in (RADIUS..size - RADIUS).step_by(RADIUS) {
                let mut tensor = [0.0; 3];
                let mut valid = true;
                for sample_y in y - RADIUS..=y + RADIUS {
                    for sample_x in x - RADIUS..=x + RADIUS {
                        let index = sample_y * size + sample_x;
                        if !mask[index]
                            || sample_x == 0
                            || sample_x + 1 == size
                            || sample_y == 0
                            || sample_y + 1 == size
                        {
                            valid = false;
                            break;
                        }
                        let dx = values[index + 1] - values[index - 1];
                        let dy = values[index + size] - values[index - size];
                        tensor[0] += dx * dx;
                        tensor[1] += dx * dy;
                        tensor[2] += dy * dy;
                    }
                    if !valid {
                        break;
                    }
                }
                if valid {
                    patches.push((
                        0.5 * (2.0 * tensor[1]).atan2(tensor[0] - tensor[2]),
                        ((tensor[0] - tensor[2]).powi(2) + 4.0 * tensor[1] * tensor[1]).sqrt()
                            / (tensor[0] + tensor[2]).max(f32::EPSILON),
                    ));
                }
            }
        }
        let reference = patches.iter().fold([0.0; 2], |sum, (angle, anisotropy)| {
            [
                sum[0] + anisotropy * (2.0 * angle).cos(),
                sum[1] + anisotropy * (2.0 * angle).sin(),
            ]
        });
        let reference = 0.5 * reference[1].atan2(reference[0]);
        let mut anisotropy: Vec<_> = patches.iter().map(|(_, value)| *value).collect();
        let mut axes: Vec<_> = patches
            .iter()
            .map(|(angle, _)| axis_delta(*angle, reference).to_degrees())
            .collect();
        let mut curvature = Vec::new();
        for pair in patches.windows(2) {
            curvature.push(axis_delta(pair[0].0, pair[1].0).to_degrees());
        }
        let mut closed = 0usize;
        let mut windows = 0usize;
        for y in RADIUS..size - RADIUS {
            for x in RADIUS..size - RADIUS {
                let ring = [
                    (x - RADIUS, y - RADIUS),
                    (x, y - RADIUS),
                    (x + RADIUS, y - RADIUS),
                    (x + RADIUS, y),
                    (x + RADIUS, y + RADIUS),
                    (x, y + RADIUS),
                    (x - RADIUS, y + RADIUS),
                    (x - RADIUS, y),
                ];
                let gradients: Option<Vec<_>> = ring
                    .into_iter()
                    .map(|(sample_x, sample_y)| {
                        let index = sample_y * size + sample_x;
                        mask[index].then(|| {
                            (values[index + 1] - values[index - 1])
                                .atan2(values[index + size] - values[index - size])
                        })
                    })
                    .collect();
                let Some(gradients) = gradients else { continue };
                let winding: f32 = gradients
                    .iter()
                    .zip(gradients.iter().cycle().skip(1))
                    .take(gradients.len())
                    .map(|(a, b)| {
                        (b - a + std::f32::consts::PI).rem_euclid(std::f32::consts::TAU)
                            - std::f32::consts::PI
                    })
                    .sum();
                windows += 1;
                closed += usize::from(winding.abs() >= std::f32::consts::PI);
            }
        }
        let mut correlation = Vec::new();
        for y in 1..size - 1 {
            for x in 1..size - 1 {
                let index = y * size + x;
                if mask[index] && mask[index + 1] && mask[index + size] {
                    correlation.push((
                        values[index],
                        (values[index + 1] + values[index + size]) * 0.5,
                    ));
                }
            }
        }
        let mean = |index: usize| {
            correlation
                .iter()
                .map(|pair| [pair.0, pair.1][index])
                .sum::<f32>()
                / correlation.len().max(1) as f32
        };
        let mean_a = mean(0);
        let mean_b = mean(1);
        let covariance = correlation
            .iter()
            .map(|(a, b)| (a - mean_a) * (b - mean_b))
            .sum::<f32>();
        let variance_a = correlation
            .iter()
            .map(|(a, _)| (a - mean_a).powi(2))
            .sum::<f32>();
        let variance_b = correlation
            .iter()
            .map(|(_, b)| (b - mean_b).powi(2))
            .sum::<f32>();
        let percentile = |values: &mut Vec<f32>, percent: f32| {
            values.sort_by(f32::total_cmp);
            values[((values.len().saturating_sub(1) as f32 * percent).round() as usize)
                .min(values.len().saturating_sub(1))]
        };
        LocalDetailMetrics {
            anisotropy_median: percentile(&mut anisotropy, 0.50),
            axis_median_degrees: percentile(&mut axes, 0.50),
            axis_p90_degrees: percentile(&mut axes, 0.90),
            curvature_p95_degrees: percentile(&mut curvature, 0.95),
            closed_winding_fraction: closed as f32 / windows.max(1) as f32,
            autocorrelation: covariance / (variance_a * variance_b).sqrt().max(f32::EPSILON),
        }
    }

    fn centroid(values: &[f32], size: usize) -> (f32, f32) {
        let (weight, x_sum, y_sum) = values.iter().enumerate().fold(
            (0.0, 0.0, 0.0),
            |(weight, x_sum, y_sum), (index, value)| {
                if !sphere_mask(size, index) || *value <= 0.05 {
                    return (weight, x_sum, y_sum);
                }
                let x = (index % size) as f32;
                let y = (index / size) as f32;
                (weight + value, x_sum + x * value, y_sum + y * value)
            },
        );
        (
            x_sum / weight.max(f32::EPSILON),
            y_sum / weight.max(f32::EPSILON),
        )
    }

    #[test]
    fn cloud_detail_erodes_edges_without_replacing_weather_systems() {
        let gpu = GpuContext::new().expect("GPU init failed");
        let detail_on = PreviewRenderer::new_with_cloud_detail(&gpu, 1.0);
        let detail_off = PreviewRenderer::new_with_cloud_detail(&gpu, 0.0);
        let resolution = 64;
        let size = 128usize;
        let terrain = TectonicTerrain {
            faces: std::array::from_fn(|_| vec![0.0; (resolution * resolution) as usize]),
            resolution,
        };
        let terrain_view = detail_on.upload_terrain(&gpu, &terrain);
        let (mass, geometry) = contour_fixture(resolution);
        let mass_view = detail_on.upload_cubemap_rgba16(&gpu, &mass, resolution);
        let geometry_view = detail_on.upload_cubemap_rgba16(&gpu, &geometry, resolution);
        let zero_mass = std::array::from_fn(|_| vec![0.0; (resolution * resolution * 4) as usize]);
        let zero_mass_view = detail_on.upload_cubemap_rgba16(&gpu, &zero_mass, resolution);
        let mut settings = uniforms();
        settings.view_mode = 9;
        settings.show_clouds = 1.0;
        settings.cloud_coverage = 1.0;

        let render = |renderer: &PreviewRenderer, seed, mass_view, render_size| {
            let settings = PreviewUniforms {
                cloud_seed: seed,
                ..settings
            };
            renderer.render(
                &gpu,
                &settings,
                &terrain_view,
                None,
                Some((mass_view, &geometry_view)),
                render_size,
            )
        };
        let leaked = render(&detail_on, u32::MAX, &zero_mass_view, size as u32);
        assert!(
            leaked
                .chunks_exact(4)
                .enumerate()
                .all(|(index, pixel)| { !sphere_mask(size, index) || pixel[0..3] == [0, 0, 0] }),
            "detail must not create density outside authored weather mass"
        );

        let coarse = density_values(&render(&detail_off, 42, &mass_view, size as u32));
        let occupied = |values: &[f32]| {
            values
                .iter()
                .enumerate()
                .filter(|(index, value)| sphere_mask(size, *index) && **value > 0.05)
                .count()
        };
        let coarse_energy = edge_energy(&coarse, size);
        let coarse_centroid = centroid(&coarse, size);
        for seed in [7, 19, 37, 73, 101, 211, 509, 997] {
            let detailed = density_values(&render(&detail_on, seed, &mass_view, size as u32));
            let blurred_correlation = correlation(
                &box_blur(&coarse, size, 4),
                &box_blur(&detailed, size, 4),
                size,
            );
            let occupied_ratio = occupied(&detailed) as f32 / occupied(&coarse) as f32;
            let detailed_centroid = centroid(&detailed, size);
            let centroid_drift = ((detailed_centroid.0 - coarse_centroid.0).powi(2)
                + (detailed_centroid.1 - coarse_centroid.1).powi(2))
            .sqrt()
                / size as f32;
            let detailed_energy = edge_energy(&detailed, size);
            let isolated = detailed
                .iter()
                .enumerate()
                .filter(|(index, value)| {
                    if **value <= 0.05 || !sphere_mask(size, *index) {
                        return false;
                    }
                    let x = *index % size;
                    let y = *index / size;
                    x > 0
                        && x + 1 < size
                        && y > 0
                        && y + 1 < size
                        && (y - 1..=y + 1).all(|sample_y| {
                            (x - 1..=x + 1).all(|sample_x| {
                                sample_x == x && sample_y == y
                                    || detailed[sample_y * size + sample_x] <= 0.05
                            })
                        })
                })
                .count();
            let (coarse_dense_sum, detailed_dense_sum, dense_count) = coarse
                .iter()
                .zip(&detailed)
                .filter(|(coarse, _)| optical_depth(**coarse) > 0.6)
                .map(|(coarse, detailed)| (optical_depth(*coarse), optical_depth(*detailed)))
                .fold(
                    (0.0, 0.0, 0usize),
                    |(coarse_sum, detailed_sum, count), (coarse, detailed)| {
                        (coarse_sum + coarse, detailed_sum + detailed, count + 1)
                    },
                );
            let dense_mean = coarse_dense_sum / dense_count.max(1) as f32;
            let detailed_dense_mean = detailed_dense_sum / dense_count.max(1) as f32;
            let dense_drift =
                (dense_mean - detailed_dense_mean).abs() / dense_mean.max(f32::EPSILON);
            let (core_residual_sq, core_count, fringe_residual_sq, fringe_count) = coarse
                .iter()
                .zip(&detailed)
                .fold((0.0, 0usize, 0.0, 0usize), |metrics, (coarse, detailed)| {
                    let residual = optical_depth(*coarse) - optical_depth(*detailed);
                    if optical_depth(*coarse) > 0.6 {
                        (
                            metrics.0 + residual * residual,
                            metrics.1 + 1,
                            metrics.2,
                            metrics.3,
                        )
                    } else if *coarse > 0.05 {
                        (
                            metrics.0,
                            metrics.1,
                            metrics.2 + residual * residual,
                            metrics.3 + 1,
                        )
                    } else {
                        metrics
                    }
                });
            let core_rms = (core_residual_sq / core_count.max(1) as f32).sqrt();
            let fringe_rms = (fringe_residual_sq / fringe_count.max(1) as f32).sqrt();
            println!(
                "cloud contour metrics seed={seed}: correlation={blurred_correlation:.4}, occupied={occupied_ratio:.4}, centroid={centroid_drift:.4}, edge={coarse_energy:.5}->{detailed_energy:.5}, isolated={isolated}, dense_tau_mean={dense_mean:.4}->{detailed_dense_mean:.4}, dense_tau_drift={dense_drift:.4}, core_tau_rms={core_rms:.4}, fringe_tau_rms={fringe_rms:.4}"
            );
            assert!(
                blurred_correlation >= 0.95,
                "seed={seed}: coarse systems changed"
            );
            assert!(
                (0.95..=1.05).contains(&occupied_ratio),
                "seed={seed}: occupied={occupied_ratio}"
            );
            let edge_ratio = detailed_energy / coarse_energy.max(f32::EPSILON);
            assert!(
                (0.95..=1.25).contains(&edge_ratio),
                "seed={seed}: edge ratio={edge_ratio}"
            );
            assert!(
                isolated == 0,
                "seed={seed}: detail created one-pixel speckle"
            );
            assert!(
                dense_drift <= 0.05,
                "seed={seed}: dense cores drifted by {dense_drift}"
            );
            assert!(
                core_rms <= 0.25,
                "seed={seed}: core optical-depth RMS={core_rms}"
            );
            assert!(
                centroid_drift < 0.05,
                "seed={seed}: centroid drifted by {centroid_drift}"
            );
            assert!(
                fringe_rms.is_finite(),
                "seed={seed}: fringe RMS is non-finite"
            );
        }

        if let Ok(output_dir) = std::env::var("CLOUD_CONTOUR_OUTPUT_DIR") {
            std::fs::create_dir_all(&output_dir).expect("create contour output directory");
            let render_size = 512;
            for seed in [7, 19, 37, 73, 101, 211, 509, 997] {
                let close_settings = PreviewUniforms {
                    cloud_seed: seed,
                    zoom: 1.55,
                    ..settings
                };
                let render_close = |renderer: &PreviewRenderer| {
                    renderer.render(
                        &gpu,
                        &close_settings,
                        &terrain_view,
                        None,
                        Some((&mass_view, &geometry_view)),
                        render_size,
                    )
                };
                let before = render_close(&detail_off);
                let after = render_close(&detail_on);
                let mut comparison = Vec::with_capacity((render_size * render_size * 8) as usize);
                for row in 0..render_size as usize {
                    let start = row * render_size as usize * 4;
                    let end = start + render_size as usize * 4;
                    comparison.extend_from_slice(&before[start..end]);
                    comparison.extend_from_slice(&after[start..end]);
                }
                image::save_buffer(
                    std::path::Path::new(&output_dir)
                        .join(format!("cloud_contours_seed_{seed}_before_after.png")),
                    &comparison,
                    render_size * 2,
                    render_size,
                    image::ColorType::Rgba8,
                )
                .expect("save contour comparison");
            }
        }

        let high_seed_outputs: Vec<_> = [(1u32 << 24) + 1, 714_003_000, u32::MAX]
            .map(|seed| {
                let first = render(&detail_on, seed, &mass_view, size as u32);
                assert_eq!(
                    first,
                    render(&detail_on, seed, &mass_view, size as u32),
                    "seed={seed}"
                );
                first
            })
            .into_iter()
            .collect();
        assert!(
            high_seed_outputs.windows(2).all(|pair| pair[0] != pair[1]),
            "high cloud seeds must select distinct stable detail streams"
        );
    }

    #[test]
    fn cloud_eight_samples_match_six_and_ten_sample_references() {
        let gpu = GpuContext::new().expect("GPU init failed");
        let resolution = 64;
        let size = 128;
        let terrain = TectonicTerrain {
            faces: std::array::from_fn(|_| vec![0.0; (resolution * resolution) as usize]),
            resolution,
        };
        let reference = PreviewRenderer::new_with_cloud_samples(&gpu, 8);
        let terrain_view = reference.upload_terrain(&gpu, &terrain);
        let (mass, geometry) = contour_fixture(resolution);
        let mass_view = reference.upload_cubemap_rgba16(&gpu, &mass, resolution);
        let geometry_view = reference.upload_cubemap_rgba16(&gpu, &geometry, resolution);
        let settings = PreviewUniforms {
            view_mode: 9,
            show_clouds: 1.0,
            cloud_coverage: 1.0,
            ..uniforms()
        };
        let render = |renderer: &PreviewRenderer| {
            renderer.render(
                &gpu,
                &settings,
                &terrain_view,
                None,
                Some((&mass_view, &geometry_view)),
                size as u32,
            )
        };
        let six = render(&PreviewRenderer::new_with_cloud_samples(&gpu, 6));
        let eight = render(&reference);
        let ten = render(&PreviewRenderer::new_with_cloud_samples(&gpu, 10));
        let six_to_eight = mean_optical_depth_difference(&six, &eight, size);
        let eight_to_ten = mean_optical_depth_difference(&eight, &ten, size);
        println!(
            "cloud sample metrics: 6_to_8_tau_mae={six_to_eight:.4}, 8_to_10_tau_mae={eight_to_ten:.4}"
        );
        assert!(
            eight_to_ten < six_to_eight,
            "8 samples do not converge toward the 10-sample reference: {six_to_eight} -> {eight_to_ten}"
        );
    }

    #[test]
    fn cloud_low_mass_optical_depth_is_near_linear_and_zero_supported() {
        let gpu = GpuContext::new().expect("GPU init failed");
        let resolution = 32;
        let size = 192usize;
        let terrain = TectonicTerrain {
            faces: std::array::from_fn(|_| vec![0.0; (resolution * resolution) as usize]),
            resolution,
        };
        let renderer = PreviewRenderer::new(&gpu);
        let terrain_view = renderer.upload_terrain(&gpu, &terrain);
        let geometry = std::array::from_fn(|_| {
            (0..resolution * resolution)
                .flat_map(|_| [0.2, 2.2, 2.2, 2.2])
                .collect()
        });
        let render = |mass: &[Vec<f32>; 6]| {
            let mass_view = renderer.upload_cubemap_rgba16(&gpu, mass, resolution);
            let geometry_view = renderer.upload_cubemap_rgba16(&gpu, &geometry, resolution);
            let settings = PreviewUniforms {
                view_mode: 9,
                show_clouds: 1.0,
                cloud_coverage: 1.0,
                planet_radius_km: 500.0,
                ..uniforms()
            };
            density_values(&renderer.render(
                &gpu,
                &settings,
                &terrain_view,
                None,
                Some((&mass_view, &geometry_view)),
                size as u32,
            ))
        };
        let mean_tau = |mass: f32| {
            let mass = std::array::from_fn(|_| {
                (0..resolution * resolution)
                    .flat_map(|_| [mass, 0.0, 0.0, mass])
                    .collect()
            });
            let image = render(&mass);
            let samples: Vec<_> = image
                .into_iter()
                .enumerate()
                .filter(|(index, _)| sphere_mask(size, *index))
                .map(|(_, value)| optical_depth(value))
                .collect();
            percentile(samples, 0.50)
        };
        let zero = std::array::from_fn(|_| vec![0.0; (resolution * resolution * 4) as usize]);
        assert!(
            render(&zero)
                .iter()
                .enumerate()
                .filter(|(index, _)| sphere_mask(size, *index))
                .all(|(_, value)| *value == 0.0)
        );
        let tau_01 = mean_tau(0.01);
        let tau_02 = mean_tau(0.02);
        let tau_04 = mean_tau(0.04);
        let ratio_01_02 = tau_02 / tau_01.max(f32::EPSILON);
        let ratio_02_04 = tau_04 / tau_02.max(f32::EPSILON);
        println!(
            "cloud low-mass optical metrics: tau(0.01)={tau_01:.5}, tau(0.02)={tau_02:.5}, tau(0.04)={tau_04:.5}, ratios={ratio_01_02:.4}/{ratio_02_04:.4}"
        );
        assert!((1.8..=2.2).contains(&ratio_01_02));
        assert!((1.8..=2.2).contains(&ratio_02_04));
    }

    #[test]
    fn cloud_sparse_partial_mass_has_exactly_zero_density_without_causal_support() {
        let gpu = GpuContext::new().expect("GPU init failed");
        let resolution = 64;
        let size = 192usize;
        let renderer = PreviewRenderer::new(&gpu);
        let terrain = TectonicTerrain {
            faces: std::array::from_fn(|_| vec![0.0; (resolution * resolution) as usize]),
            resolution,
        };
        let terrain_view = renderer.upload_terrain(&gpu, &terrain);
        let geometry = std::array::from_fn(|_| {
            (0..resolution * resolution)
                .flat_map(|_| [0.4, 2.0, 7.0, 11.0])
                .collect()
        });
        let mut mass = std::array::from_fn(|_| vec![0.0; (resolution * resolution * 4) as usize]);
        for y in 20..44 {
            for x in 20..44 {
                let pixel = (y * resolution + x) as usize * 4;
                mass[4][pixel..pixel + 4].copy_from_slice(&[0.2, 0.1, 0.15, 0.2]);
            }
        }
        let mass_view = renderer.upload_cubemap_rgba16(&gpu, &mass, resolution);
        let geometry_view = renderer.upload_cubemap_rgba16(&gpu, &geometry, resolution);
        let settings = PreviewUniforms {
            view_mode: 9,
            show_clouds: 1.0,
            cloud_coverage: 1.0,
            planet_radius_km: 500.0,
            ..uniforms()
        };
        let density = density_values(&renderer.render(
            &gpu,
            &settings,
            &terrain_view,
            None,
            Some((&mass_view, &geometry_view)),
            size as u32,
        ));
        let support = [0, 1, 2]
            .map(|channel| source_support(&mass, channel, size, resolution as usize))
            .into_iter()
            .reduce(|mut combined, family| {
                for (combined, family) in combined.iter_mut().zip(family) {
                    *combined |= family;
                }
                combined
            })
            .unwrap();
        assert!(support.iter().any(|supported| *supported));
        assert!(
            density
                .iter()
                .zip(&support)
                .enumerate()
                .all(|(index, (density, supported))| !sphere_mask(size, index)
                    || *supported
                    || *density == 0.0)
        );
    }

    #[test]
    fn cloud_layers_keep_high_mass_visible_and_materially_distinct() {
        let gpu = GpuContext::new().expect("GPU init failed");
        let resolution = 64;
        let size = 192usize;
        let renderer = PreviewRenderer::new(&gpu);
        let terrain = TectonicTerrain {
            faces: std::array::from_fn(|_| vec![0.0; (resolution * resolution) as usize]),
            resolution,
        };
        let terrain_view = renderer.upload_terrain(&gpu, &terrain);
        let (mass, geometry) = layered_fixture(resolution);
        let geometry_view = renderer.upload_cubemap_rgba16(&gpu, &geometry, resolution);
        let render = |mass: &[Vec<f32>; 6], view_mode| {
            let mass_view = renderer.upload_cubemap_rgba16(&gpu, mass, resolution);
            let settings = PreviewUniforms {
                view_mode,
                show_clouds: 1.0,
                cloud_coverage: 1.0,
                planet_radius_km: 500.0,
                ..uniforms()
            };
            renderer.render(
                &gpu,
                &settings,
                &terrain_view,
                None,
                Some((&mass_view, &geometry_view)),
                size as u32,
            )
        };
        let family_mass = |channel| {
            std::array::from_fn(|face| {
                mass[face]
                    .chunks_exact(4)
                    .flat_map(|sample| {
                        let value = sample[channel];
                        [
                            if channel == 0 { value } else { 0.0 },
                            if channel == 1 { value } else { 0.0 },
                            if channel == 2 { value } else { 0.0 },
                            if channel < 2 { value } else { 0.0 },
                        ]
                    })
                    .collect()
            })
        };
        let clear = std::array::from_fn(|_| vec![0.0; (resolution * resolution * 4) as usize]);
        let low_mass = family_mass(0);
        let deep_mass = family_mass(1);
        let high_mass = family_mass(2);
        let low_density = density_values(&render(&low_mass, 9));
        let deep_density = density_values(&render(&deep_mass, 9));
        let high_density = density_values(&render(&high_mass, 9));
        let low_centroid = centroid(&low_density, size);
        let deep_centroid = centroid(&deep_density, size);
        let high_centroid = centroid(&high_density, size);
        let separation = |a: (f32, f32), b: (f32, f32)| {
            ((a.0 - b.0).powi(2) + (a.1 - b.1).powi(2)).sqrt() / size as f32
        };
        let high_tau = high_density
            .iter()
            .map(|value| optical_depth(*value))
            .sum::<f32>()
            / high_density.len() as f32;
        let low_color = render(&low_mass, 0);
        let deep_color = render(&deep_mass, 0);
        let high_color = render(&high_mass, 0);
        let clear_color = render(&clear, 0);
        let low_material = mean_color_difference(&clear_color, &low_color, size);
        let deep_material = mean_color_difference(&clear_color, &deep_color, size);
        let high_material = mean_color_difference(&clear_color, &high_color, size);
        println!(
            "U3 layered metrics: high_tau={high_tau:.5}, separation low/deep={:.4}, low/high={:.4}, deep/high={:.4}, material low/deep/high={low_material:.5}/{deep_material:.5}/{high_material:.5}",
            separation(low_centroid, deep_centroid),
            separation(low_centroid, high_centroid),
            separation(deep_centroid, high_centroid),
        );
        assert!(
            high_tau > 0.001,
            "high-only mass with zero alpha must render"
        );
        assert!(separation(low_centroid, deep_centroid) > 0.10);
        assert!(separation(low_centroid, high_centroid) > 0.10);
        assert!(separation(deep_centroid, high_centroid) > 0.10);
        assert!(low_material > 0.001 && deep_material > 0.001 && high_material > 0.001);
        assert!((low_material - deep_material).abs() > 0.0005);
        assert!((high_material - deep_material).abs() > 0.0005);
    }

    #[test]
    fn u3_wind_detail_rotation_and_wind2_preserve_each_family() {
        let gpu = GpuContext::new().expect("GPU init failed");
        let resolution = 64;
        let size = 192usize;
        let renderer = PreviewRenderer::new(&gpu);
        let detail_off = PreviewRenderer::new_with_cloud_detail(&gpu, 0.0);
        let terrain = TectonicTerrain {
            faces: std::array::from_fn(|_| vec![0.0; (resolution * resolution) as usize]),
            resolution,
        };
        let terrain_view = renderer.upload_terrain(&gpu, &terrain);
        let (mass, geometry) = layered_fixture(resolution);
        let geometry_view = renderer.upload_cubemap_rgba16(&gpu, &geometry, resolution);
        let wind_pipeline = WindFieldPipeline::new(&gpu).expect("wind init failed");
        let wind = |direction: [f32; 3], scale: f32| {
            wind_pipeline.create_test_textures(&gpu, resolution, move |_| {
                (
                    [
                        direction[0] * scale,
                        direction[1] * scale,
                        direction[2] * scale,
                        0.0,
                    ],
                    1013.0,
                )
            })
        };
        let wind_x = wind([1.0, 0.0, 0.0], 1.0);
        let wind_y = wind([0.0, 1.0, 0.0], 1.0);
        let wind_2 = wind([1.0, 0.0, 0.0], 2.0);
        let family_mass = |channel: usize| {
            std::array::from_fn(|face| {
                mass[face]
                    .chunks_exact(4)
                    .flat_map(|sample| {
                        let value = sample[channel];
                        [
                            if channel == 0 { value } else { 0.0 },
                            if channel == 1 { value } else { 0.0 },
                            if channel == 2 { value } else { 0.0 },
                            if channel < 2 { value } else { 0.0 },
                        ]
                    })
                    .collect()
            })
        };
        let render = |renderer: &PreviewRenderer,
                      mass: &[Vec<f32>; 6],
                      wind: &crate::terrain_compute::DynamicsTextures| {
            let mass_view = renderer.upload_cubemap_rgba16(&gpu, mass, resolution);
            let settings = PreviewUniforms {
                view_mode: 9,
                show_clouds: 1.0,
                cloud_coverage: 1.0,
                cloud_advection: 1.0,
                planet_radius_km: 500.0,
                ..uniforms()
            };
            density_values(&renderer.render(
                &gpu,
                &settings,
                &terrain_view,
                Some(&wind.wind_continentality),
                Some((&mass_view, &geometry_view)),
                size as u32,
            ))
        };
        let mut failures = Vec::new();
        for (family, name) in [(0, "low"), (1, "deep"), (2, "high")] {
            let mass = family_mass(family);
            let coarse_x = render(&detail_off, &mass, &wind_x);
            let coarse_y = render(&detail_off, &mass, &wind_y);
            let source_support = source_support(&mass, family, size, resolution as usize);
            let x = render(&renderer, &mass, &wind_x);
            let y = render(&renderer, &mass, &wind_y);
            let strong = render(&renderer, &mass, &wind_2);
            let x_residual: Vec<_> = x
                .iter()
                .zip(&coarse_x)
                .map(|(detail, coarse)| detail - coarse)
                .collect();
            let y_residual: Vec<_> = y
                .iter()
                .zip(&coarse_y)
                .map(|(detail, coarse)| detail - coarse)
                .collect();
            let x_metrics = local_detail_metrics(&x_residual, &source_support, size);
            let y_metrics = local_detail_metrics(&y_residual, &source_support, size);
            let coarse_x = box_blur(&coarse_x, size, 4);
            let coarse_y = box_blur(&coarse_y, size, 4);
            let macro_correlation = correlation(&coarse_x, &coarse_y, size);
            let macro_mae = masked_density_mae(&coarse_x, &coarse_y, &source_support);
            let occupied_delta = (x.iter().filter(|value| **value > 0.05).count() as f32
                - y.iter().filter(|value| **value > 0.05).count() as f32)
                .abs()
                / source_support
                    .iter()
                    .filter(|supported| **supported)
                    .count()
                    .max(1) as f32;
            let edge_ratio = masked_edge_energy(&strong, &source_support, size)
                / masked_edge_energy(&x, &source_support, size).max(f32::EPSILON);
            let centroid_distance = |a: (f32, f32), b: (f32, f32)| {
                ((a.0 - b.0).powi(2) + (a.1 - b.1).powi(2)).sqrt() / size as f32
            };
            let centroid_drift = centroid_distance(
                masked_centroid(&x, &source_support, size),
                masked_centroid(&y, &source_support, size),
            );
            println!(
                "U3 {name} local metrics: anisotropy={:.5}/{:.5}, axis_median={:.3}/{:.3}, axis_p90={:.3}/{:.3}, curvature_p95={:.3}/{:.3}, closed_winding={:.4}/{:.4}, autocorrelation={:.5}/{:.5}, macro_corr={macro_correlation:.5}, macro_mae={macro_mae:.5}, occupied_delta={occupied_delta:.5}, centroid_drift={centroid_drift:.5}, wind2_edge_ratio={edge_ratio:.5}",
                x_metrics.anisotropy_median,
                y_metrics.anisotropy_median,
                x_metrics.axis_median_degrees,
                y_metrics.axis_median_degrees,
                x_metrics.axis_p90_degrees,
                y_metrics.axis_p90_degrees,
                x_metrics.curvature_p95_degrees,
                y_metrics.curvature_p95_degrees,
                x_metrics.closed_winding_fraction,
                y_metrics.closed_winding_fraction,
                x_metrics.autocorrelation,
                y_metrics.autocorrelation,
            );
            let metrics = [&x_metrics, &y_metrics];
            if metrics.iter().any(|metrics| {
                !metrics.anisotropy_median.is_finite()
                    || !metrics.axis_median_degrees.is_finite()
                    || !metrics.axis_p90_degrees.is_finite()
                    || !metrics.curvature_p95_degrees.is_finite()
                    || !metrics.closed_winding_fraction.is_finite()
                    || !metrics.autocorrelation.is_finite()
            }) {
                failures.push(format!("{name}: non-finite local patch metric"));
            }
            if macro_correlation < 0.995
                || macro_mae > 0.01
                || occupied_delta > 0.02
                || centroid_drift >= 0.005
            {
                failures.push(format!("{name}: macro support changed"));
            }
            if !(0.80..=1.15).contains(&edge_ratio) {
                failures.push(format!("{name}: wind2 edge ratio={edge_ratio:.5}"));
            }
        }
        assert!(failures.is_empty(), "{}", failures.join("; "));
    }

    #[test]
    fn u3_wind_basis_handles_calm_pole_and_cube_edge() {
        let gpu = GpuContext::new().expect("GPU init failed");
        let resolution = 32;
        let size = 96;
        let renderer = PreviewRenderer::new(&gpu);
        let terrain = TectonicTerrain {
            faces: std::array::from_fn(|_| vec![0.0; (resolution * resolution) as usize]),
            resolution,
        };
        let terrain_view = renderer.upload_terrain(&gpu, &terrain);
        let mass = std::array::from_fn(|_| {
            (0..resolution * resolution)
                .flat_map(|_| [0.4, 0.25, 0.15, 0.4])
                .collect()
        });
        let geometry = std::array::from_fn(|_| {
            (0..resolution * resolution)
                .flat_map(|_| [0.4, 2.0, 7.0, 11.0])
                .collect()
        });
        let mass_view = renderer.upload_cubemap_rgba16(&gpu, &mass, resolution);
        let geometry_view = renderer.upload_cubemap_rgba16(&gpu, &geometry, resolution);
        let wind_pipeline = WindFieldPipeline::new(&gpu).expect("wind init failed");
        let cases = [
            ("calm", [0.0, 0.0, 0.0], uniforms().rotation),
            (
                "pole",
                [0.0, 1.0, 0.000_001],
                [
                    [1.0, 0.0, 0.0, 0.0],
                    [0.0, 0.0, 1.0, 0.0],
                    [0.0, -1.0, 0.0, 0.0],
                    [0.0, 0.0, 0.0, 1.0],
                ],
            ),
            (
                "cube_edge",
                [1.0, 0.0, 1.0],
                [
                    [
                        std::f32::consts::FRAC_1_SQRT_2,
                        0.0,
                        std::f32::consts::FRAC_1_SQRT_2,
                        0.0,
                    ],
                    [0.0, 1.0, 0.0, 0.0],
                    [
                        -std::f32::consts::FRAC_1_SQRT_2,
                        0.0,
                        std::f32::consts::FRAC_1_SQRT_2,
                        0.0,
                    ],
                    [0.0, 0.0, 0.0, 1.0],
                ],
            ),
        ];
        for (name, wind, rotation) in cases {
            let dynamics = wind_pipeline.create_test_textures(&gpu, resolution, move |_| {
                ([wind[0], wind[1], wind[2], 0.0], 1013.0)
            });
            let settings = PreviewUniforms {
                rotation,
                view_mode: 9,
                show_clouds: 1.0,
                cloud_coverage: 1.0,
                cloud_advection: 1.0,
                planet_radius_km: 500.0,
                ..uniforms()
            };
            let first = renderer.render(
                &gpu,
                &settings,
                &terrain_view,
                Some(&dynamics.wind_continentality),
                Some((&mass_view, &geometry_view)),
                size,
            );
            assert_eq!(
                first,
                renderer.render(
                    &gpu,
                    &settings,
                    &terrain_view,
                    Some(&dynamics.wind_continentality),
                    Some((&mass_view, &geometry_view)),
                    size,
                ),
                "{name}"
            );
            assert!(first.chunks_exact(4).any(|pixel| pixel[0] > 0), "{name}");
        }
    }

    #[test]
    fn test_preview_renders_non_empty() {
        let gpu = GpuContext::new().expect("GPU init failed");

        // Generate terrain via compute pipeline
        let compute = TerrainComputePipeline::new(&gpu);
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
        let terrain = compute.generate(
            &gpu, &plates, 64, 42, 1.0, 1.2, 8, 0.5, 2.0, 1.0, 0.10, 1.0, 1.0, 9.81, 0.85, 0.2, 1.0,
        );

        // Upload and render
        let renderer = PreviewRenderer::new(&gpu);
        let cubemap_view = renderer.upload_terrain(&gpu, &terrain);

        let uniforms = uniforms();

        let size = 256;
        let pixels = renderer.render(&gpu, &uniforms, &cubemap_view, None, None, size);
        assert_eq!(pixels.len(), (size * size * 4) as usize);

        let non_background: usize = pixels
            .chunks(4)
            .filter(|px| px[0] > 10 || px[1] > 10 || px[2] > 10)
            .count();

        let total_pixels = (size * size) as usize;
        assert!(
            non_background > total_pixels / 4,
            "preview should have visible sphere pixels ({non_background}/{total_pixels})"
        );
    }

    #[test]
    fn cloud_visibility_and_shadow_toggle_plumbing_are_independent() {
        let gpu = GpuContext::new().expect("GPU init failed");
        let terrain = TectonicTerrain {
            faces: std::array::from_fn(|_| vec![0.0; 16 * 16]),
            resolution: 16,
        };
        let renderer = PreviewRenderer::new(&gpu);
        let terrain_view = renderer.upload_terrain(&gpu, &terrain);
        let wind = WindFieldPipeline::new(&gpu).expect("dynamics unavailable");
        let dynamics = wind.create_textures(&gpu, 16);
        wind.generate_gpu(&gpu, &terrain, &dynamics, 42, 0.0, 0.4, 0.5, 1.0, 15.0, 1.0);
        let weather_pipeline = WeatherFieldPipeline::new(&gpu).expect("weather unavailable");

        let mut cloud_off = uniforms();
        cloud_off.cloud_coverage = 1.0;
        cloud_off.show_cities = 1.0;
        cloud_off.night_lights = 1.0;
        cloud_off.light_dir = [-1.0, 0.0, -0.2];
        let expected = renderer.render(
            &gpu,
            &cloud_off,
            &terrain_view,
            Some(&dynamics.wind_continentality),
            None,
            64,
        );

        let mut cloud_on_without_weather = cloud_off;
        cloud_on_without_weather.show_clouds = 1.0;
        assert_eq!(
            renderer.render(
                &gpu,
                &cloud_on_without_weather,
                &terrain_view,
                Some(&dynamics.wind_continentality),
                None,
                64,
            ),
            expected
        );

        for (moisture, coverage) in [(0.0, 1.0), (1.0, 0.0)] {
            let weather = weather_pipeline.create_textures(&gpu, 16);
            weather_pipeline.generate(
                &gpu,
                WeatherSnapshot {
                    face: 0,
                    resolution: 16,
                    seed: 42,
                    storm_count: 8,
                    coverage,
                    moisture,
                    surface_pressure_bar: 1.0,
                    base_temp_c: 15.0,
                    ocean_level: 0.0,
                    axial_tilt_rad: 0.4,
                    season: 0.5,
                    storm_size: 2.0,
                    radius_km: 6371.0,
                    rotation_rate_rad_s: std::f32::consts::TAU / 86400.0,
                    wind_scale: 1.0,
                },
                &terrain,
                &dynamics,
                &weather,
            );
            let mut cloud_on = cloud_off;
            cloud_on.show_clouds = 1.0;
            cloud_on.cloud_coverage = coverage;
            let actual = renderer.render(
                &gpu,
                &cloud_on,
                &terrain_view,
                Some(&dynamics.wind_continentality),
                Some((&weather.mass, &weather.geometry)),
                64,
            );
            assert_eq!(actual, expected, "moisture={moisture}, coverage={coverage}");
        }

        let dense_snapshot = WeatherSnapshot {
            face: 0,
            resolution: 16,
            seed: 42,
            storm_count: 8,
            coverage: 1.0,
            moisture: 1.0,
            surface_pressure_bar: 1.0,
            base_temp_c: 15.0,
            ocean_level: -0.1,
            axial_tilt_rad: 0.4,
            season: 0.5,
            storm_size: 2.0,
            radius_km: 500.0,
            rotation_rate_rad_s: std::f32::consts::TAU / 86400.0,
            wind_scale: 1.0,
        };
        let dense_weather = weather_pipeline.create_textures(&gpu, 16);
        weather_pipeline.generate(&gpu, dense_snapshot, &terrain, &dynamics, &dense_weather);

        assert_eq!(
            renderer.render(
                &gpu,
                &cloud_off,
                &terrain_view,
                Some(&dynamics.wind_continentality),
                Some((&dense_weather.mass, &dense_weather.geometry)),
                64,
            ),
            expected,
            "hidden clouds must not block city lights"
        );

        let mut transparent_clouds = cloud_off;
        transparent_clouds.show_clouds = 1.0;
        transparent_clouds.cloud_opacity = 0.0;
        assert_eq!(
            renderer.render(
                &gpu,
                &transparent_clouds,
                &terrain_view,
                Some(&dynamics.wind_continentality),
                Some((&dense_weather.mass, &dense_weather.geometry)),
                64,
            ),
            expected,
            "zero-opacity clouds must not cast shadows or block city lights"
        );

        let mut limb_clear = cloud_off;
        limb_clear.planet_radius_km = 500.0;
        limb_clear.atmosphere_density = 1.0;
        limb_clear.atmosphere_height = 0.05;
        limb_clear.show_atmosphere_layer = 1.0;
        let size = 128;
        let clear_limb = renderer.render(
            &gpu,
            &limb_clear,
            &terrain_view,
            Some(&dynamics.wind_continentality),
            Some((&dense_weather.mass, &dense_weather.geometry)),
            size,
        );
        let mut cloudy_uniforms = limb_clear;
        cloudy_uniforms.show_clouds = 1.0;
        let cloudy_limb = renderer.render(
            &gpu,
            &cloudy_uniforms,
            &terrain_view,
            Some(&dynamics.wind_continentality),
            Some((&dense_weather.mass, &dense_weather.geometry)),
            size,
        );
        let limb_radius_squared = |index: usize| {
            let x = (index % size as usize) as f32 + 0.5;
            let y = (index / size as usize) as f32 + 0.5;
            let ndc = [
                ((x / size as f32 - 0.5) * 2.0) / 0.85,
                ((y / size as f32 - 0.5) * 2.0) / 0.85,
            ];
            ndc[0] * ndc[0] + ndc[1] * ndc[1]
        };
        let changed_limb_pixels = clear_limb
            .chunks_exact(4)
            .zip(cloudy_limb.chunks_exact(4))
            .enumerate()
            .filter(|(index, (clear, cloudy))| limb_radius_squared(*index) > 1.0 && clear != cloudy)
            .count();
        assert!(
            changed_limb_pixels > 0,
            "clouds must contribute outside the solid planet limb"
        );

        cloudy_uniforms.show_cloud_shadows = 0.0;
        let no_surface_shadows = renderer.render(
            &gpu,
            &cloudy_uniforms,
            &terrain_view,
            Some(&dynamics.wind_continentality),
            Some((&dense_weather.mass, &dense_weather.geometry)),
            size,
        );
        let surface_changes = cloudy_limb
            .chunks_exact(4)
            .zip(no_surface_shadows.chunks_exact(4))
            .enumerate()
            .filter(|(index, (with_shadows, without_shadows))| {
                limb_radius_squared(*index) < 1.0 && with_shadows != without_shadows
            })
            .count();
        assert!(
            surface_changes > 0,
            "cloud shadow toggle must have a bounded nonzero surface effect; strength and low-sun quality belong to U4"
        );

        for (index, (with_shadows, without_shadows)) in cloudy_limb
            .chunks_exact(4)
            .zip(no_surface_shadows.chunks_exact(4))
            .enumerate()
        {
            if limb_radius_squared(index) > 1.0 {
                assert_eq!(with_shadows, without_shadows, "limb clouds changed");
            }
        }
    }

    #[test]
    fn test_preview_target_resize_rebinds_once() {
        let gpu = GpuContext::new().expect("GPU init failed");
        let mut renderer = PreviewRenderer::new(&gpu);
        let mut rebinds = 0;

        renderer.resize_target(&gpu, DEFAULT_PREVIEW_SIZE, |_| rebinds += 1);
        renderer.resize_target(&gpu, 256, |_| rebinds += 1);

        assert_eq!(renderer.size, 256);
        assert_eq!(rebinds, 1);
    }
}
