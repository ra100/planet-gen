use planet_gen::gpu;

fn main() {
    env_logger::init();

    let gpu_ctx = gpu::GpuContext::new().expect("Failed to initialize GPU");
    log::info!("Planet Gen started with GPU: {}", gpu_ctx.adapter_name());
}
