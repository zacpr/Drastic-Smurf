use eframe::NativeOptions;

mod app;
mod core;
mod modules;
mod ui;

#[tokio::main]
async fn main() -> eframe::Result<()> {
    tracing_subscriber::fmt::init();

    let options = NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_inner_size([1280.0, 800.0])
            .with_min_inner_size([800.0, 600.0]),
        ..Default::default()
    };

    eframe::run_native(
        "DRASTIC SMURF",
        options,
        Box::new(|cc| Ok(Box::new(app::DrasticSmurfApp::new(cc)))),
    )
}
