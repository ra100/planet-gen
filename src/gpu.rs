use std::sync::Arc;

#[cfg(test)]
use std::sync::{Condvar, Mutex};

#[cfg(test)]
static GPU_TEST_GATE: (Mutex<bool>, Condvar) = (Mutex::new(false), Condvar::new());

#[cfg(test)]
struct GpuTestPermit;

#[cfg(test)]
impl GpuTestPermit {
    fn acquire() -> Self {
        let (lock, available) = &GPU_TEST_GATE;
        let mut in_use = lock.lock().unwrap_or_else(|poisoned| poisoned.into_inner());
        while *in_use {
            in_use = available
                .wait(in_use)
                .unwrap_or_else(|poisoned| poisoned.into_inner());
        }
        *in_use = true;
        Self
    }
}

#[cfg(test)]
impl Drop for GpuTestPermit {
    fn drop(&mut self) {
        let (lock, available) = &GPU_TEST_GATE;
        let mut in_use = lock.lock().unwrap_or_else(|poisoned| poisoned.into_inner());
        *in_use = false;
        available.notify_one();
    }
}

/// Holds the wgpu device, queue, and adapter info for the application lifetime.
pub struct GpuContext {
    pub device: wgpu::Device,
    pub queue: wgpu::Queue,
    pub adapter_info: wgpu::AdapterInfo,
    pub rgba16float_features: wgpu::TextureFormatFeatures,
    // Test-only: Vulkan is not reliable when independent test contexts overlap.
    #[cfg(test)]
    _test_permit: GpuTestPermit,
}

impl GpuContext {
    /// Initialize the GPU context. Call once at app startup.
    pub fn new() -> Result<Self, GpuError> {
        #[cfg(test)]
        let test_permit = GpuTestPermit::acquire();

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
        let rgba16float_features =
            adapter.get_texture_format_features(wgpu::TextureFormat::Rgba16Float);
        log::info!(
            "GPU adapter: {} ({:?})",
            adapter_info.name,
            adapter_info.backend
        );

        let (device, queue) = pollster::block_on(adapter.request_device(&wgpu::DeviceDescriptor {
            label: Some("planet-gen"),
            required_features: wgpu::Features::empty(),
            experimental_features: wgpu::ExperimentalFeatures::disabled(),
            required_limits: wgpu::Limits::default(),
            memory_hints: wgpu::MemoryHints::Performance,
            trace: wgpu::Trace::Off,
        }))
        .map_err(|e: wgpu::RequestDeviceError| GpuError::DeviceRequest(e.to_string()))?;

        Ok(Self {
            device,
            queue,
            adapter_info,
            rgba16float_features,
            #[cfg(test)]
            _test_permit: test_permit,
        })
    }

    /// Borrow eframe's GPU allocation for the interactive application.
    pub fn from_eframe(render_state: &eframe::egui_wgpu::RenderState) -> Self {
        #[cfg(test)]
        let test_permit = GpuTestPermit::acquire();

        Self {
            device: render_state.device.clone(),
            queue: render_state.queue.clone(),
            adapter_info: render_state.adapter.get_info(),
            rgba16float_features: render_state
                .adapter
                .get_texture_format_features(wgpu::TextureFormat::Rgba16Float),
            #[cfg(test)]
            _test_permit: test_permit,
        }
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
