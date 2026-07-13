use crate::gpu::GpuContext;
use bytemuck::{Pod, Zeroable};
use wgpu::util::DeviceExt;

#[repr(C)]
#[derive(Copy, Clone, Pod, Zeroable)]
struct NoiseTestParams {
    width: u32,
    height: u32,
    scale: f32,
    seed: u32,
    stream: u32,
    _pad0: u32,
    _pad1: u32,
    _pad2: u32,
}

/// Run the noise test shader, sampling a width×height grid at the given scale.
/// Returns noise values as a Vec<f32>.
pub fn run_noise_test(gpu: &GpuContext, width: u32, height: u32, scale: f32) -> Vec<f32> {
    run_seeded_noise_test(gpu, width, height, scale, 0, 0)
}

fn run_seeded_noise_test(
    gpu: &GpuContext,
    width: u32,
    height: u32,
    scale: f32,
    seed: u32,
    stream: u32,
) -> Vec<f32> {
    let shader_source = format!(
        "{}\n{}",
        include_str!("shaders/noise.wgsl"),
        include_str!("shaders/noise_test.wgsl"),
    );

    let shader = gpu
        .device
        .create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("noise test shader"),
            source: wgpu::ShaderSource::Wgsl(shader_source.into()),
        });

    let total = (width * height) as usize;
    let buffer_size = (total * std::mem::size_of::<f32>()) as u64;

    let output_buffer = gpu.device.create_buffer(&wgpu::BufferDescriptor {
        label: Some("noise output"),
        size: buffer_size,
        usage: wgpu::BufferUsages::STORAGE | wgpu::BufferUsages::COPY_SRC,
        mapped_at_creation: false,
    });

    let params = NoiseTestParams {
        width,
        height,
        scale,
        seed,
        stream,
        _pad0: 0,
        _pad1: 0,
        _pad2: 0,
    };
    let uniform_buffer = gpu
        .device
        .create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("noise params"),
            contents: bytemuck::bytes_of(&params),
            usage: wgpu::BufferUsages::UNIFORM,
        });

    let bind_group_layout = gpu
        .device
        .create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("noise test bgl"),
            entries: &[
                wgpu::BindGroupLayoutEntry {
                    binding: 0,
                    visibility: wgpu::ShaderStages::COMPUTE,
                    ty: wgpu::BindingType::Buffer {
                        ty: wgpu::BufferBindingType::Storage { read_only: false },
                        has_dynamic_offset: false,
                        min_binding_size: None,
                    },
                    count: None,
                },
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
            ],
        });

    let pipeline_layout = gpu
        .device
        .create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("noise test pipeline layout"),
            bind_group_layouts: &[&bind_group_layout],
            push_constant_ranges: &[],
        });

    let pipeline = gpu
        .device
        .create_compute_pipeline(&wgpu::ComputePipelineDescriptor {
            label: Some("noise test pipeline"),
            layout: Some(&pipeline_layout),
            module: &shader,
            entry_point: Some("main"),
            compilation_options: Default::default(),
            cache: None,
        });

    let bind_group = gpu.device.create_bind_group(&wgpu::BindGroupDescriptor {
        label: Some("noise test bind group"),
        layout: &bind_group_layout,
        entries: &[
            wgpu::BindGroupEntry {
                binding: 0,
                resource: output_buffer.as_entire_binding(),
            },
            wgpu::BindGroupEntry {
                binding: 1,
                resource: uniform_buffer.as_entire_binding(),
            },
        ],
    });

    let staging_buffer = gpu.device.create_buffer(&wgpu::BufferDescriptor {
        label: Some("noise staging"),
        size: buffer_size,
        usage: wgpu::BufferUsages::MAP_READ | wgpu::BufferUsages::COPY_DST,
        mapped_at_creation: false,
    });

    let mut encoder = gpu
        .device
        .create_command_encoder(&wgpu::CommandEncoderDescriptor {
            label: Some("noise test encoder"),
        });

    {
        let mut pass = encoder.begin_compute_pass(&wgpu::ComputePassDescriptor {
            label: Some("noise test pass"),
            timestamp_writes: None,
        });
        pass.set_pipeline(&pipeline);
        pass.set_bind_group(0, &bind_group, &[]);
        pass.dispatch_workgroups((total as u32).div_ceil(64), 1, 1);
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
    let result: Vec<f32> = bytemuck::cast_slice(&mapped).to_vec();
    result
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::gpu::GpuContext;

    #[test]
    fn test_noise_range_and_variation() {
        let gpu = GpuContext::new().expect("GPU init failed");
        let values = run_noise_test(&gpu, 64, 64, 4.0);

        assert_eq!(values.len(), 64 * 64);

        // All values should be in [-1, 1]
        let min_val = values.iter().cloned().fold(f32::INFINITY, f32::min);
        let max_val = values.iter().cloned().fold(f32::NEG_INFINITY, f32::max);

        assert!(min_val >= -1.0, "noise minimum {min_val} should be >= -1.0");
        assert!(max_val <= 1.0, "noise maximum {max_val} should be <= 1.0");

        // Should have meaningful variation (not all zeros or constant)
        let range = max_val - min_val;
        assert!(
            range > 0.1,
            "noise range {range} should show variation (min={min_val}, max={max_val})"
        );

        // No NaN values
        assert!(
            values.iter().all(|v| !v.is_nan()),
            "noise should not produce NaN"
        );

        // Should not all be the same value
        let first = values[0];
        let all_same = values.iter().all(|v| (*v - first).abs() < 1e-6);
        assert!(!all_same, "noise should not be uniform");
    }

    #[test]
    fn high_seed_noise_is_stable_finite_and_distinct() {
        let gpu = GpuContext::new().expect("GPU init failed");
        let seeds = [
            (1u32 << 24) - 1,
            1u32 << 24,
            (1u32 << 24) + 1,
            714_002_999,
            714_003_000,
            714_003_001,
            u32::MAX - 1,
            u32::MAX,
        ];
        let fields: Vec<_> = seeds
            .iter()
            .map(|&seed| run_seeded_noise_test(&gpu, 16, 16, 4.0, seed, 17))
            .collect();

        for (&seed, values) in seeds.iter().zip(&fields) {
            assert!(values.iter().all(|value| value.is_finite()), "seed {seed}");
            let (min, max) = values
                .iter()
                .fold((f32::INFINITY, f32::NEG_INFINITY), |(min, max), &value| {
                    (min.min(value), max.max(value))
                });
            assert!(max - min > 0.1, "seed {seed} spatially degenerated");
            assert_eq!(
                *values,
                run_seeded_noise_test(&gpu, 16, 16, 4.0, seed, 17),
                "seed {seed} is not deterministic"
            );
        }

        for pair in fields.windows(2) {
            assert_ne!(pair[0], pair[1], "adjacent high seeds aliased");
        }
    }
}
