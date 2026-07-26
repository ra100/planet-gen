use planet_gen::app::PlanetGenApp;
use planet_gen::terrain_artifact::{CliMode, TerrainSource, load_approved_terrain, parse_cli_args};

fn main() -> eframe::Result {
    let source = match parse_cli_args(std::env::args_os().skip(1)) {
        Ok(CliMode::Procedural) => TerrainSource::Procedural,
        Ok(CliMode::Import { artifact, approval }) => {
            match load_approved_terrain(&artifact, &approval) {
                Ok(artifact) => TerrainSource::Imported {
                    terrain: artifact.terrain,
                    control_ocean_level: artifact.approval.control_ocean_level,
                },
                Err(error) => {
                    eprintln!("terrain import failed: {error}");
                    std::process::exit(2);
                }
            }
        }
        Ok(CliMode::Help) => {
            println!("Usage: planet-gen [--terrain-artifact DIR --terrain-approval FILE]");
            return Ok(());
        }
        Err(error) => {
            eprintln!(
                "Usage: planet-gen [--terrain-artifact DIR --terrain-approval FILE] ({error})"
            );
            std::process::exit(2);
        }
    };
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
        Box::new(move |cc| Ok(Box::new(PlanetGenApp::new(cc, source)?))),
    )
}
