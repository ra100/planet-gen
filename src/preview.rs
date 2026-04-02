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
    pub season: f32, // 0=winter, 0.5=equinox, 1=summer
    pub atmosphere_density: f32, // 0.0 = none, 1.0 = Earth-like (reserved for future)
    pub atmosphere_height: f32,  // scale height in planet radii (reserved for future)
    pub height_scale: f32,       // normal map height exaggeration (1.0 = subtle, 5.0 = dramatic)
    pub zoom: f32,               // viewport zoom (1.0 = default, >1 = zoomed in)
    pub pan_x: f32,              // viewport pan in NDC units
    pub pan_y: f32,
    pub cloud_coverage: f32,     // 0.0 = clear, 1.0 = overcast
    pub cloud_seed: f32,         // noise seed for cloud pattern
    pub cloud_altitude: f32,     // cloud shell altitude above surface (planet radii)
    pub cloud_type: f32,         // 0.0 = smooth stratus, 1.0 = puffy cumulus
    pub storm_count: f32,        // 0-8 cyclone storm systems
    pub storm_size: f32,         // storm radius multiplier (0.5 = small, 1.0 = default, 2.0 = large)
    pub night_lights: f32,       // 0.0 = pristine, 1.0 = heavily urbanized
    pub star_color_temp: f32,    // 0.0 = blue hot star, 0.5 = sun-like, 1.0 = red dwarf
    pub city_light_hue: f32,    // 0.0 = warm amber, 0.5 = white, 1.0 = cool blue
    pub show_ao: f32,           // 1.0 = AO enabled, 0.0 = disabled
    pub _pad4: [f32; 3],
}

pub struct PreviewRenderer {
    pipeline: wgpu::RenderPipeline,
    bind_group_layout: wgpu::BindGroupLayout,
    sampler: wgpu::Sampler,
    pub size: u32,
}

impl PreviewRenderer {
    pub fn new(gpu: &GpuContext) -> Self {
        let shader_source = format!(
            "{}\n{}",
            include_str!("shaders/noise.wgsl"),
            include_str!("shaders/preview_cubemap.wgsl"),
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
                    ],
                });

        let pipeline_layout =
            gpu.device
                .create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
                    label: Some("preview pipeline layout"),
                    bind_group_layouts: &[&bind_group_layout],
                    push_constant_ranges: &[],
                });

        let pipeline = gpu
            .device
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
                        format: wgpu::TextureFormat::Rgba8UnormSrgb,
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
            });

        let sampler = gpu.device.create_sampler(&wgpu::SamplerDescriptor {
            label: Some("height sampler"),
            mag_filter: wgpu::FilterMode::Linear,
            min_filter: wgpu::FilterMode::Linear,
            ..Default::default()
        });

        Self {
            pipeline,
            bind_group_layout,
            sampler,
            size: DEFAULT_PREVIEW_SIZE,
        }
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
                    origin: wgpu::Origin3d { x: 0, y: 0, z: i as u32 },
                    aspect: wgpu::TextureAspect::All,
                },
                bytemuck::cast_slice(&f16_data),
                wgpu::TexelCopyBufferLayout {
                    offset: 0,
                    bytes_per_row: Some(res * 2),
                    rows_per_image: Some(res),
                },
                wgpu::Extent3d { width: res, height: res, depth_or_array_layers: 1 },
            );
        }

        cubemap.create_view(&wgpu::TextureViewDescriptor {
            dimension: Some(wgpu::TextureViewDimension::Cube),
            ..Default::default()
        })
    }

    /// Render the planet preview to an RGBA pixel buffer using a pre-computed cubemap.
    pub fn render(
        &self,
        gpu: &GpuContext,
        uniforms: &PreviewUniforms,
        cubemap_view: &wgpu::TextureView,
        render_size: u32,
    ) -> Vec<u8> {
        let size = render_size;

        let uniform_buffer =
            gpu.device
                .create_buffer_init(&wgpu::util::BufferInitDescriptor {
                    label: Some("preview uniforms"),
                    contents: bytemuck::bytes_of(uniforms),
                    usage: wgpu::BufferUsages::UNIFORM,
                });

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
            ],
        });

        let render_target = gpu.device.create_texture(&wgpu::TextureDescriptor {
            label: Some("preview render target"),
            size: wgpu::Extent3d { width: size, height: size, depth_or_array_layers: 1 },
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
            .create_command_encoder(&wgpu::CommandEncoderDescriptor { label: Some("preview encoder") });

        {
            let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("preview pass"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view: &render_view,
                    resolve_target: None,
                    depth_slice: None,
                    ops: wgpu::Operations {
                        load: wgpu::LoadOp::Clear(wgpu::Color { r: 0.02, g: 0.02, b: 0.05, a: 1.0 }),
                        store: wgpu::StoreOp::Store,
                    },
                })],
                depth_stencil_attachment: None,
                timestamp_writes: None,
                occlusion_query_set: None,
            });
            pass.set_pipeline(&self.pipeline);
            pass.set_bind_group(0, &bind_group, &[]);
            pass.draw(0..3, 0..1);
        }

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
            wgpu::Extent3d { width: size, height: size, depth_or_array_layers: 1 },
        );

        gpu.queue.submit(Some(encoder.finish()));

        readback.slice(..).map_async(wgpu::MapMode::Read, |_| {});
        let _ = gpu.device.poll(wgpu::PollType::Wait);

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

/// Compute a deterministic seed offset using golden ratio hash.
pub fn seed_to_offset(seed: u32) -> [f32; 3] {
    let s = seed as f64;
    let phi = 1.618033988749895_f64;
    let x = ((s * phi) % 97.0) as f32;
    let y = ((s * phi * phi) % 89.0) as f32;
    let z = ((s * phi * phi * phi) % 83.0) as f32;
    [x, y, z]
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::gpu::GpuContext;
    use crate::plates::{generate_plates, PlateGenParams};
    use crate::terrain_compute::TerrainComputePipeline;

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
        });
        let terrain = compute.generate(&gpu, &plates, 64, 42, 1.0, 1.2, 8, 0.5, 2.0, 1.0, 0.10, 1.0, 1.0);

        // Upload and render
        let renderer = PreviewRenderer::new(&gpu);
        let cubemap_view = renderer.upload_terrain(&gpu, &terrain);

        let uniforms = PreviewUniforms {
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
            cloud_seed: 42.0,
            cloud_altitude: 0.008,
            cloud_type: 0.5,
            storm_count: 0.0,
            storm_size: 1.0,
            night_lights: 0.0,
            star_color_temp: 0.5,
            city_light_hue: 0.0,
            show_ao: 1.0,
            _pad4: [0.0; 3],
        };

        let size = 256;
        let pixels = renderer.render(&gpu, &uniforms, &cubemap_view, size);
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
    fn test_seed_offset_distinct_for_different_seeds() {
        let seeds = [0u32, 1, 42, 100_000, 999_999, u32::MAX];
        let offsets: Vec<_> = seeds.iter().map(|&s| seed_to_offset(s)).collect();
        for i in 0..offsets.len() {
            for j in (i + 1)..offsets.len() {
                let diff = (offsets[i][0] - offsets[j][0]).abs()
                    + (offsets[i][1] - offsets[j][1]).abs()
                    + (offsets[i][2] - offsets[j][2]).abs();
                assert!(diff > 0.1, "seeds {} and {} too similar", seeds[i], seeds[j]);
            }
        }
    }

    #[test]
    fn test_seed_offset_in_range() {
        for seed in [0u32, 1, 42, 100_000, 999_999, u32::MAX] {
            let off = seed_to_offset(seed);
            for (i, &v) in off.iter().enumerate() {
                assert!(v >= 0.0 && v < 100.0, "seed {seed} offset[{i}] = {v} out of range");
            }
        }
    }
}
