use std::sync::Arc;

/// Holds the wgpu device, queue, and adapter info for the application lifetime.
pub struct GpuContext {
    pub device: wgpu::Device,
    pub queue: wgpu::Queue,
    pub adapter_info: wgpu::AdapterInfo,
}

impl GpuContext {
    /// Initialize the GPU context. Call once at app startup.
    pub fn new() -> Result<Self, GpuError> {
        let instance = wgpu::Instance::new(&wgpu::InstanceDescriptor {
            backends: wgpu::Backends::all(),
            ..Default::default()
        });

        let adapter = pollster::block_on(instance.request_adapter(&wgpu::RequestAdapterOptions {
            power_preference: wgpu::PowerPreference::HighPerformance,
            compatible_surface: None,
            force_fallback_adapter: false,
        }))
        .map_err(|_| GpuError::NoAdapter)?;

        let adapter_info = adapter.get_info();
        log::info!(
            "GPU adapter: {} ({:?})",
            adapter_info.name,
            adapter_info.backend
        );

        let (device, queue) = pollster::block_on(adapter.request_device(&wgpu::DeviceDescriptor {
            label: Some("planet-gen"),
            required_features: wgpu::Features::empty(),
            required_limits: wgpu::Limits::default(),
            memory_hints: wgpu::MemoryHints::Performance,
            trace: wgpu::Trace::Off,
        }))
        .map_err(|e: wgpu::RequestDeviceError| GpuError::DeviceRequest(e.to_string()))?;

        Ok(Self {
            device,
            queue,
            adapter_info,
        })
    }

    pub fn adapter_name(&self) -> &str {
        &self.adapter_info.name
    }
}

/// Shared GPU context wrapped in Arc for use across the app.
pub type SharedGpuContext = Arc<GpuContext>;

#[derive(Debug, thiserror::Error)]
pub enum GpuError {
    #[error("no suitable GPU adapter found")]
    NoAdapter,
    #[error("failed to request GPU device: {0}")]
    DeviceRequest(String),
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_gpu_context_initializes() {
        let ctx = GpuContext::new().expect("GPU context should initialize");
        let name = ctx.adapter_name();
        assert!(!name.is_empty(), "adapter name should not be empty");
        println!("GPU adapter: {name}");
    }
}
