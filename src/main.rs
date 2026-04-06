use std::sync::Arc;

use planet_gen::app::PlanetGenApp;
use planet_gen::gpu::GpuContext;

fn main() -> eframe::Result {
    env_logger::init();

    let gpu = match GpuContext::new() {
        Ok(ctx) => {
            log::info!("Planet Gen started with GPU: {}", ctx.adapter_name());
            Arc::new(ctx)
        }
        Err(e) => {
            eprintln!("ERROR: GPU initialization failed: {e}");
            eprintln!();
            eprintln!("Planet Gen requires a GPU with WebGPU support.");
            eprintln!("  macOS:   Metal is used automatically.");
            eprintln!("  Linux:   Install Vulkan drivers (mesa-vulkan-drivers).");
            eprintln!("  Windows: Update your GPU drivers.");
            std::process::exit(1);
        }
    };

    let options = eframe::NativeOptions {
        viewport: eframe::egui::ViewportBuilder::default()
            .with_inner_size([1024.0, 768.0])
            .with_title("Planet Gen"),
        ..Default::default()
    };

    eframe::run_native(
        "Planet Gen",
        options,
        Box::new(move |_cc| Ok(Box::new(PlanetGenApp::new(gpu)))),
    )
}
