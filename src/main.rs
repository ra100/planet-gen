use planet_gen::app::PlanetGenApp;

fn main() -> eframe::Result {
    env_logger::init();

    let icon = image::load_from_memory(include_bytes!("../assets/planet-gen-icon.png"))
        .expect("failed to decode application icon");
    let (icon_width, icon_height) = (icon.width(), icon.height());
    let icon = icon.into_rgba8();

    let options = eframe::NativeOptions {
        viewport: eframe::egui::ViewportBuilder::default()
            .with_inner_size([1460.0, 920.0])
            .with_min_inner_size([980.0, 640.0])
            .with_title("Planet Gen")
            .with_icon(eframe::egui::IconData {
                rgba: icon.into_raw(),
                width: icon_width,
                height: icon_height,
            }),
        renderer: eframe::Renderer::Wgpu,
        // Persistence only stores app settings (export folder); keep window
        // geometry behaving as before.
        persist_window: false,
        ..Default::default()
    };

    eframe::run_native(
        "Planet Gen",
        options,
        Box::new(|cc| Ok(Box::new(PlanetGenApp::new(cc)?))),
    )
}
