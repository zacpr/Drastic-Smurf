use eframe::NativeOptions;

mod app;
mod core;
mod modules;
mod ui;

#[tokio::main]
async fn main() -> eframe::Result<()> {
    let log_entries = crate::ui::log_buffer::init_logging();

    let config = crate::core::config::AppConfig::load().unwrap_or_default();

    let num_clusters = config.clusters.len();
    let (default_w, default_h) = if num_clusters <= 2 {
        (1280.0, 800.0)
    } else if num_clusters <= 4 {
        (1280.0, 950.0)
    } else {
        (1650.0, 1000.0)
    };

    let mut viewport = egui::ViewportBuilder::default()
        .with_inner_size([
            config.window_width.unwrap_or(default_w),
            config.window_height.unwrap_or(default_h),
        ])
        .with_min_inner_size([800.0, 600.0]);

    if let (Some(x), Some(y)) = (config.window_pos_x, config.window_pos_y) {
        viewport = viewport.with_position([x, y]);
    }

    let options = NativeOptions {
        renderer: eframe::Renderer::Glow,
        viewport,
        ..Default::default()
    };

    eframe::run_native(
        "DRASTIC SMURF",
        options,
        Box::new(move |cc| {
            egui_extras::install_image_loaders(&cc.egui_ctx);
            Ok(Box::new(app::DrasticSmurfApp::new(cc, log_entries)))
        }),
    )
}
