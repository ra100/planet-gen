use crate::gpu::GpuContext;
use crate::terrain::TerrainData;
use bytemuck::{Pod, Zeroable};
use wgpu::util::DeviceExt;

const PREVIEW_SIZE: u32 = 768;

#[repr(C)]
#[derive(Copy, Clone, Pod, Zeroable)]
struct PreviewUniforms {
    rotation: [[f32; 4]; 4],
    light_dir: [f32; 3],
    ocean_level: f32,
}

pub struct PreviewRenderer {
    pipeline: wgpu::RenderPipeline,
    bind_group_layout: wgpu::BindGroupLayout,
    sampler: wgpu::Sampler,
    pub size: u32,
}

impl PreviewRenderer {
    pub fn new(gpu: &GpuContext) -> Self {
        let shader = gpu
            .device
            .create_shader_module(wgpu::ShaderModuleDescriptor {
                label: Some("preview shader"),
                source: wgpu::ShaderSource::Wgsl(
                    include_str!("shaders/preview.wgsl").into(),
                ),
            });

        let bind_group_layout =
            gpu.device
                .create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
                    label: Some("preview bgl"),
                    entries: &[
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
            label: Some("preview sampler"),
            mag_filter: wgpu::FilterMode::Linear,
            min_filter: wgpu::FilterMode::Linear,
            ..Default::default()
        });

        Self {
            pipeline,
            bind_group_layout,
            sampler,
            size: PREVIEW_SIZE,
        }
    }

    /// Render the terrain data to an RGBA pixel buffer.
    pub fn render(
        &self,
        gpu: &GpuContext,
        terrain: &TerrainData,
        rotation_y: f32,
        rotation_x: f32,
        ocean_fraction: f32,
    ) -> Vec<u8> {
        let res = terrain.resolution;

        // Create cubemap texture from terrain heightmap data
        // Use R16Float for linear filtering support (R32Float is not filterable)
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

        // Upload each face (convert f32 → f16)
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
                    bytes_per_row: Some(res * 2), // f16 = 2 bytes
                    rows_per_image: Some(res),
                },
                wgpu::Extent3d {
                    width: res,
                    height: res,
                    depth_or_array_layers: 1,
                },
            );
        }

        let cubemap_view = cubemap.create_view(&wgpu::TextureViewDescriptor {
            dimension: Some(wgpu::TextureViewDimension::Cube),
            ..Default::default()
        });

        // Combined Y then X rotation matrix
        let cy = rotation_y.cos();
        let sy = rotation_y.sin();
        let cx = rotation_x.cos();
        let sx = rotation_x.sin();
        // Ry * Rx
        let uniforms = PreviewUniforms {
            rotation: [
                [cy, sy * sx, sy * cx, 0.0],
                [0.0, cx, -sx, 0.0],
                [-sy, cy * sx, cy * cx, 0.0],
                [0.0, 0.0, 0.0, 1.0],
            ],
            light_dir: [0.5, 0.7, -1.0],
            // Map ocean_fraction to a height threshold: more ocean → higher sea level
            // Normalized terrain is [-1, 1], so ocean_level near 0 means ~50% coverage
            ocean_level: -1.0 + 2.0 * ocean_fraction,
        };

        let uniform_buffer =
            gpu.device
                .create_buffer_init(&wgpu::util::BufferInitDescriptor {
                    label: Some("preview uniforms"),
                    contents: bytemuck::bytes_of(&uniforms),
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
                    resource: wgpu::BindingResource::TextureView(&cubemap_view),
                },
                wgpu::BindGroupEntry {
                    binding: 2,
                    resource: wgpu::BindingResource::Sampler(&self.sampler),
                },
            ],
        });

        // Render target
        let render_target = gpu.device.create_texture(&wgpu::TextureDescriptor {
            label: Some("preview render target"),
            size: wgpu::Extent3d {
                width: self.size,
                height: self.size,
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

        {
            let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("preview pass"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view: &render_view,
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
            pass.set_pipeline(&self.pipeline);
            pass.set_bind_group(0, &bind_group, &[]);
            pass.draw(0..3, 0..1); // fullscreen triangle
        }

        // Readback
        let bytes_per_row = self.size * 4;
        let padded_bytes_per_row = (bytes_per_row + 255) & !255;
        let readback = gpu.device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("preview readback"),
            size: (padded_bytes_per_row * self.size) as u64,
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
                    rows_per_image: Some(self.size),
                },
            },
            wgpu::Extent3d {
                width: self.size,
                height: self.size,
                depth_or_array_layers: 1,
            },
        );

        gpu.queue.submit(Some(encoder.finish()));

        readback.slice(..).map_async(wgpu::MapMode::Read, |_| {});
        let _ = gpu.device.poll(wgpu::PollType::Wait);

        let mapped = readback.slice(..).get_mapped_range();
        let mut pixels = Vec::with_capacity((self.size * self.size * 4) as usize);
        for row in 0..self.size {
            let start = (row * padded_bytes_per_row) as usize;
            let end = start + (self.size * 4) as usize;
            pixels.extend_from_slice(&mapped[start..end]);
        }
        pixels
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::gpu::GpuContext;
    use crate::terrain::{generate_terrain, TerrainParams};

    #[test]
    fn test_preview_renders_non_empty() {
        let gpu = GpuContext::new().expect("GPU init failed");
        let renderer = PreviewRenderer::new(&gpu);

        let terrain = generate_terrain(
            &gpu,
            &TerrainParams {
                resolution: 64,
                ..Default::default()
            },
        );

        let pixels = renderer.render(&gpu, &terrain, 0.0, 0.0, 0.7);
        let size = renderer.size;
        assert_eq!(pixels.len(), (size * size * 4) as usize);

        // Should not be all black — the sphere should have colored pixels
        let non_background: usize = pixels
            .chunks(4)
            .filter(|px| px[0] > 10 || px[1] > 10 || px[2] > 10)
            .count();

        let total_pixels = (size * size) as usize;
        assert!(
            non_background > total_pixels / 10,
            "preview should have visible sphere pixels ({non_background}/{total_pixels})"
        );
    }
}
