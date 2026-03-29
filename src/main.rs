use std::sync::Arc;

use planet_gen::app::PlanetGenApp;
use planet_gen::gpu::GpuContext;

fn main() -> eframe::Result {
    env_logger::init();

    let gpu = Arc::new(GpuContext::new().expect("Failed to initialize GPU"));
    log::info!("Planet Gen started with GPU: {}", gpu.adapter_name());

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
