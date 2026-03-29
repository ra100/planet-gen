use crate::gpu::GpuContext;
use bytemuck::{Pod, Zeroable};
use wgpu::util::DeviceExt;

const PREVIEW_SIZE: u32 = 768;

#[repr(C)]
#[derive(Copy, Clone, Pod, Zeroable)]
pub struct PreviewUniforms {
    pub rotation: [[f32; 4]; 4],
    pub light_dir: [f32; 3],
    pub ocean_level: f32,
    // Terrain params
    pub seed_offset: [f32; 3],
    pub frequency: f32,
    pub lacunarity: f32,
    pub gain: f32,
    pub amplitude: f32,
    pub octaves: u32,
    pub _pad: [f32; 3],
    pub _pad2: f32,
}

pub struct PreviewRenderer {
    pipeline: wgpu::RenderPipeline,
    bind_group_layout: wgpu::BindGroupLayout,
    pub size: u32,
}

impl PreviewRenderer {
    pub fn new(gpu: &GpuContext) -> Self {
        // Concatenate noise functions + preview shader
        let shader_source = format!(
            "{}\n{}",
            include_str!("shaders/noise.wgsl"),
            include_str!("shaders/preview.wgsl"),
        );

        let shader = gpu
            .device
            .create_shader_module(wgpu::ShaderModuleDescriptor {
                label: Some("preview shader"),
                source: wgpu::ShaderSource::Wgsl(shader_source.into()),
            });

        let bind_group_layout =
            gpu.device
                .create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
                    label: Some("preview bgl"),
                    entries: &[wgpu::BindGroupLayoutEntry {
                        binding: 0,
                        visibility: wgpu::ShaderStages::VERTEX | wgpu::ShaderStages::FRAGMENT,
                        ty: wgpu::BindingType::Buffer {
                            ty: wgpu::BufferBindingType::Uniform,
                            has_dynamic_offset: false,
                            min_binding_size: None,
                        },
                        count: None,
                    }],
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

        Self {
            pipeline,
            bind_group_layout,
            size: PREVIEW_SIZE,
        }
    }

    /// Render the planet preview to an RGBA pixel buffer.
    /// No cubemap needed — noise is computed directly in the fragment shader.
    pub fn render(&self, gpu: &GpuContext, uniforms: &PreviewUniforms) -> Vec<u8> {
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
            entries: &[wgpu::BindGroupEntry {
                binding: 0,
                resource: uniform_buffer.as_entire_binding(),
            }],
        });

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
            pass.draw(0..3, 0..1);
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

/// Compute a deterministic seed offset using float-based hash.
/// Produces values in [0, 100) range — avoids the WGSL integer overflow issue.
pub fn seed_to_offset(seed: u32) -> [f32; 3] {
    // Use golden ratio-based hash for good distribution
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

    #[test]
    fn test_preview_renders_non_empty() {
        let gpu = GpuContext::new().expect("GPU init failed");
        let renderer = PreviewRenderer::new(&gpu);

        let uniforms = PreviewUniforms {
            rotation: [
                [1.0, 0.0, 0.0, 0.0],
                [0.0, 1.0, 0.0, 0.0],
                [0.0, 0.0, 1.0, 0.0],
                [0.0, 0.0, 0.0, 1.0],
            ],
            light_dir: [0.5, 0.7, -1.0],
            ocean_level: 0.0,
            seed_offset: seed_to_offset(42),
            frequency: 1.5,
            lacunarity: 2.0,
            gain: 0.5,
            amplitude: 1.0,
            octaves: 8,
            _pad: [0.0; 3],
            _pad2: 0.0,
        };

        let pixels = renderer.render(&gpu, &uniforms);
        let size = renderer.size;
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

        // All pairs should be distinct
        for i in 0..offsets.len() {
            for j in (i + 1)..offsets.len() {
                let diff = (offsets[i][0] - offsets[j][0]).abs()
                    + (offsets[i][1] - offsets[j][1]).abs()
                    + (offsets[i][2] - offsets[j][2]).abs();
                assert!(
                    diff > 0.1,
                    "seeds {} and {} should produce different offsets: {:?} vs {:?}",
                    seeds[i], seeds[j], offsets[i], offsets[j]
                );
            }
        }
    }

    #[test]
    fn test_seed_offset_in_range() {
        for seed in [0u32, 1, 42, 100_000, 999_999, u32::MAX] {
            let off = seed_to_offset(seed);
            for (i, &v) in off.iter().enumerate() {
                assert!(
                    v >= 0.0 && v < 100.0,
                    "seed {seed} offset[{i}] = {v} out of range"
                );
            }
        }
    }
}
