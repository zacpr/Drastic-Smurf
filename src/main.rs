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

    #[cfg(target_os = "windows")]
    let options = {
        let wgpu_setup = eframe::egui_wgpu::WgpuSetup::CreateNew(
            eframe::egui_wgpu::WgpuSetupCreateNew {
                power_preference: eframe::wgpu::PowerPreference::LowPower,
                instance_descriptor: eframe::wgpu::InstanceDescriptor {
                    backends: eframe::wgpu::Backends::all(),
                    flags: eframe::wgpu::InstanceFlags::default()
                        | eframe::wgpu::InstanceFlags::ALLOW_UNDERLYING_NONCOMPLIANT_ADAPTER,
                    ..Default::default()
                },
                native_adapter_selector: Some(std::sync::Arc::new(|adapters, _surface| {
                    tracing::info!("Available graphics adapters:");
                    println!("Available graphics adapters:");
                    for (i, adapter) in adapters.iter().enumerate() {
                        let info = adapter.get_info();
                        let msg = format!(
                            "  Adapter [{}]: name={:?}, backend={:?}, device_type={:?}, driver={:?}, driver_info={:?}",
                            i,
                            info.name,
                            info.backend,
                            info.device_type,
                            info.driver,
                            info.driver_info
                        );
                        tracing::info!("{}", msg);
                        println!("{}", msg);
                    }

                    if adapters.is_empty() {
                        return Err("No graphics adapters found!".to_string());
                    }

                    // First preference: Discrete GPU
                    if let Some(adapter) = adapters
                        .iter()
                        .find(|a| a.get_info().device_type == eframe::wgpu::DeviceType::DiscreteGpu)
                    {
                        let info = adapter.get_info();
                        tracing::info!("Selected Discrete GPU: {:?}", info.name);
                        println!("Selected Discrete GPU: {:?}", info.name);
                        return Ok(adapter.clone());
                    }

                    // Second preference: Integrated GPU
                    if let Some(adapter) = adapters.iter().find(|a| {
                        a.get_info().device_type == eframe::wgpu::DeviceType::IntegratedGpu
                    }) {
                        let info = adapter.get_info();
                        tracing::info!("Selected Integrated GPU: {:?}", info.name);
                        println!("Selected Integrated GPU: {:?}", info.name);
                        return Ok(adapter.clone());
                    }

                    // Third preference: CPU/Software/Other
                    if let Some(adapter) = adapters
                        .iter()
                        .find(|a| a.get_info().device_type == eframe::wgpu::DeviceType::Cpu)
                    {
                        let info = adapter.get_info();
                        tracing::info!("Selected CPU/Software adapter: {:?}", info.name);
                        println!("Selected CPU/Software adapter: {:?}", info.name);
                        return Ok(adapter.clone());
                    }

                    // Fallback: just pick the first one
                    let adapter = &adapters[0];
                    let info = adapter.get_info();
                    tracing::info!("Selected fallback adapter: {:?}", info.name);
                    println!("Selected fallback adapter: {:?}", info.name);
                    Ok(adapter.clone())
                })),
                ..Default::default()
            },
        );

        NativeOptions {
            viewport,
            wgpu_options: eframe::egui_wgpu::WgpuConfiguration {
                wgpu_setup,
                ..Default::default()
            },
            ..Default::default()
        }
    };

    #[cfg(not(target_os = "windows"))]
    let options = NativeOptions {
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
