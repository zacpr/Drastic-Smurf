use eframe::NativeOptions;

mod app;
mod core;
mod modules;
mod ui;

#[tokio::main]
async fn main() -> eframe::Result<()> {
    tracing_subscriber::fmt::init();

    let config = crate::core::config::AppConfig::load().unwrap_or_default();

    let mut viewport = egui::ViewportBuilder::default()
        .with_inner_size([
            config.window_width.unwrap_or(1280.0),
            config.window_height.unwrap_or(800.0),
        ])
        .with_min_inner_size([800.0, 600.0]);

    if let (Some(x), Some(y)) = (config.window_pos_x, config.window_pos_y) {
        viewport = viewport.with_position([x, y]);
    }

    let options = NativeOptions {
        viewport,
        ..Default::default()
    };

    eframe::run_native(
        "DRASTIC SMURF",
        options,
        Box::new(|cc| Ok(Box::new(app::DrasticSmurfApp::new(cc)))),
    )
}
