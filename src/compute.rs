use crate::gpu::GpuContext;

/// Run the gradient compute shader, writing to a 256×256 RGBA texture.
/// Returns the pixel data as a Vec<u8> in RGBA format.
pub fn run_gradient(gpu: &GpuContext, width: u32, height: u32) -> Vec<u8> {
    let shader = gpu
        .device
        .create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("gradient shader"),
            source: wgpu::ShaderSource::Wgsl(
                include_str!("shaders/gradient.wgsl").into(),
            ),
        });

    let texture = gpu.device.create_texture(&wgpu::TextureDescriptor {
        label: Some("gradient output"),
        size: wgpu::Extent3d {
            width,
            height,
            depth_or_array_layers: 1,
        },
        mip_level_count: 1,
        sample_count: 1,
        dimension: wgpu::TextureDimension::D2,
        format: wgpu::TextureFormat::Rgba8Unorm,
        usage: wgpu::TextureUsages::STORAGE_BINDING | wgpu::TextureUsages::COPY_SRC,
        view_formats: &[],
    });

    let texture_view = texture.create_view(&wgpu::TextureViewDescriptor::default());

    let bind_group_layout =
        gpu.device
            .create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
                label: Some("gradient bgl"),
                entries: &[wgpu::BindGroupLayoutEntry {
                    binding: 0,
                    visibility: wgpu::ShaderStages::COMPUTE,
                    ty: wgpu::BindingType::StorageTexture {
                        access: wgpu::StorageTextureAccess::WriteOnly,
                        format: wgpu::TextureFormat::Rgba8Unorm,
                        view_dimension: wgpu::TextureViewDimension::D2,
                    },
                    count: None,
                }],
            });

    let pipeline_layout = gpu
        .device
        .create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("gradient pipeline layout"),
            bind_group_layouts: &[&bind_group_layout],
            push_constant_ranges: &[],
        });

    let pipeline = gpu
        .device
        .create_compute_pipeline(&wgpu::ComputePipelineDescriptor {
            label: Some("gradient pipeline"),
            layout: Some(&pipeline_layout),
            module: &shader,
            entry_point: Some("main"),
            compilation_options: Default::default(),
            cache: None,
        });

    let bind_group = gpu.device.create_bind_group(&wgpu::BindGroupDescriptor {
        label: Some("gradient bind group"),
        layout: &bind_group_layout,
        entries: &[wgpu::BindGroupEntry {
            binding: 0,
            resource: wgpu::BindingResource::TextureView(&texture_view),
        }],
    });

    // Dispatch compute
    let mut encoder = gpu
        .device
        .create_command_encoder(&wgpu::CommandEncoderDescriptor {
            label: Some("gradient encoder"),
        });

    {
        let mut pass = encoder.begin_compute_pass(&wgpu::ComputePassDescriptor {
            label: Some("gradient pass"),
            timestamp_writes: None,
        });
        pass.set_pipeline(&pipeline);
        pass.set_bind_group(0, &bind_group, &[]);
        pass.dispatch_workgroups(
            (width + 15) / 16,
            (height + 15) / 16,
            1,
        );
    }

    // Copy texture to readback buffer
    let bytes_per_row = width * 4;
    // wgpu requires rows aligned to 256 bytes
    let padded_bytes_per_row = (bytes_per_row + 255) & !255;
    let buffer_size = (padded_bytes_per_row * height) as u64;

    let readback_buffer = gpu.device.create_buffer(&wgpu::BufferDescriptor {
        label: Some("gradient readback"),
        size: buffer_size,
        usage: wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::MAP_READ,
        mapped_at_creation: false,
    });

    encoder.copy_texture_to_buffer(
        wgpu::TexelCopyTextureInfo {
            texture: &texture,
            mip_level: 0,
            origin: wgpu::Origin3d::ZERO,
            aspect: wgpu::TextureAspect::All,
        },
        wgpu::TexelCopyBufferInfo {
            buffer: &readback_buffer,
            layout: wgpu::TexelCopyBufferLayout {
                offset: 0,
                bytes_per_row: Some(padded_bytes_per_row),
                rows_per_image: Some(height),
            },
        },
        wgpu::Extent3d {
            width,
            height,
            depth_or_array_layers: 1,
        },
    );

    gpu.queue.submit(Some(encoder.finish()));

    // Map and read back
    let buffer_slice = readback_buffer.slice(..);
    buffer_slice.map_async(wgpu::MapMode::Read, |_| {});
    let _ = gpu.device.poll(wgpu::PollType::Wait);

    let mapped = buffer_slice.get_mapped_range();

    // Remove row padding
    let mut pixels = Vec::with_capacity((width * height * 4) as usize);
    for row in 0..height {
        let start = (row * padded_bytes_per_row) as usize;
        let end = start + (width * 4) as usize;
        pixels.extend_from_slice(&mapped[start..end]);
    }

    pixels
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::gpu::GpuContext;

    #[test]
    fn test_gradient_shader() {
        let gpu = GpuContext::new().expect("GPU init failed");
        let width = 256u32;
        let height = 256u32;
        let pixels = run_gradient(&gpu, width, height);

        assert_eq!(pixels.len(), (width * height * 4) as usize);

        // Top-left corner (0,0): u=0, v=0 → (0, 0, 128, 255)
        assert_eq!(pixels[0], 0, "top-left R should be 0");
        assert_eq!(pixels[1], 0, "top-left G should be 0");
        assert!(pixels[2] > 100 && pixels[2] < 150, "top-left B should be ~128, got {}", pixels[2]);
        assert_eq!(pixels[3], 255, "top-left A should be 255");

        // Bottom-right corner (255,255): u≈1, v≈1 → (255, 255, 128, 255)
        let br = ((height - 1) * width + (width - 1)) as usize * 4;
        assert!(pixels[br] > 250, "bottom-right R should be ~255, got {}", pixels[br]);
        assert!(pixels[br + 1] > 250, "bottom-right G should be ~255, got {}", pixels[br + 1]);
        assert!(pixels[br + 2] > 100 && pixels[br + 2] < 150, "bottom-right B should be ~128");

        // Mid-point check: center pixel should have R≈128, G≈128
        let center = (128 * width + 128) as usize * 4;
        assert!(pixels[center] > 100 && pixels[center] < 160, "center R should be ~128, got {}", pixels[center]);
        assert!(pixels[center + 1] > 100 && pixels[center + 1] < 160, "center G should be ~128");
    }
}
