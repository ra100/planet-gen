use planet_gen::app::PlanetGenApp;

fn main() -> eframe::Result {
    env_logger::init();

    let options = eframe::NativeOptions {
        viewport: eframe::egui::ViewportBuilder::default()
            .with_inner_size([1024.0, 768.0])
            .with_title("Planet Gen"),
        renderer: eframe::Renderer::Wgpu,
        ..Default::default()
    };

    eframe::run_native(
        "Planet Gen",
        options,
        Box::new(|cc| Ok(Box::new(PlanetGenApp::new(cc)?))),
    )
}
