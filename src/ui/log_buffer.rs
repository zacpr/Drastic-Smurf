use std::sync::{Arc, RwLock};

const MAX_ENTRIES: usize = 5000;

#[derive(Debug, Clone)]
pub struct LogEntry {
    pub timestamp: String,
    pub level: String,
    pub target: String,
    pub message: String,
}

pub struct LogBuffer {
    entries: Arc<RwLock<Vec<LogEntry>>>,
}

impl LogBuffer {
    pub fn new() -> Self {
        Self {
            entries: Arc::new(RwLock::new(Vec::with_capacity(MAX_ENTRIES))),
        }
    }

    pub fn shared(&self) -> Arc<RwLock<Vec<LogEntry>>> {
        Arc::clone(&self.entries)
    }

    pub fn push(&self, entry: LogEntry) {
        if let Ok(mut entries) = self.entries.write() {
            entries.push(entry);
            if entries.len() > MAX_ENTRIES {
                let drain = entries.len() - MAX_ENTRIES;
                entries.drain(0..drain);
            }
        }
    }
}

impl<S: tracing::Subscriber> tracing_subscriber::Layer<S> for LogBuffer {
    fn on_event(
        &self,
        event: &tracing::Event<'_>,
        _ctx: tracing_subscriber::layer::Context<'_, S>,
    ) {
        let meta = event.metadata();
        let timestamp = chrono::Local::now().format("%H:%M:%S%.3f").to_string();
        let level = meta.level().to_string();
        let target = meta.target().to_string();

        let mut message = String::new();
        let writer = tracing_subscriber::fmt::format::Writer::new(&mut message);
        let mut visitor = tracing_subscriber::fmt::format::DefaultVisitor::new(writer, false);
        event.record(&mut visitor);

        self.push(LogEntry {
            timestamp,
            level,
            target,
            message,
        });
    }
}

pub fn init_logging() -> Arc<RwLock<Vec<LogEntry>>> {
    use tracing_subscriber::EnvFilter;
    use tracing_subscriber::prelude::*;

    let buffer = LogBuffer::new();
    let shared = buffer.shared();

    let filter = EnvFilter::try_from_default_env()
        .unwrap_or_else(|_| EnvFilter::new("info,drastic_smurf=debug"));

    let subscriber = tracing_subscriber::registry()
        .with(filter)
        .with(buffer)
        .with(tracing_subscriber::fmt::Layer::new().with_writer(std::io::stderr));

    tracing::subscriber::set_global_default(subscriber).expect("Failed to set tracing subscriber");

    shared
}

const LEVELS: &[&str] = &["All", "ERROR", "WARN", "INFO", "DEBUG", "TRACE"];

fn level_priority(level: &str) -> i32 {
    match level {
        "ERROR" => 0,
        "WARN" => 1,
        "INFO" => 2,
        "DEBUG" => 3,
        "TRACE" => 4,
        _ => 5,
    }
}

pub fn render_log_viewer(ctx: &egui::Context, entries: &[LogEntry]) {
    let filter_id = egui::Id::new("log_level_filter");
    let mut selected = ctx.memory_mut(|mem| {
        mem.data
            .get_temp_mut_or_default::<String>(filter_id)
            .clone()
    });

    egui::CentralPanel::default().show(ctx, |ui| {
        ui.horizontal(|ui| {
            ui.heading("Logs");
            ui.add_space(16.0);
            ui.label("Level:");
            egui::ComboBox::new(filter_id, "Level")
                .selected_text(&selected)
                .show_ui(ui, |ui| {
                    for &level in LEVELS {
                        if ui.selectable_label(selected == *level, level).clicked() {
                            selected = level.to_string();
                        }
                    }
                });
            ui.label(format!("({} entries)", entries.len()));
        });
        ui.add_space(8.0);

        let available_height = ui.available_height();

        egui::ScrollArea::vertical()
            .stick_to_bottom(true)
            .max_height(available_height - 8.0)
            .show(ui, |ui| {
                let min_priority = if selected == "All" {
                    i32::MAX
                } else {
                    level_priority(&selected)
                };

                for entry in entries.iter() {
                    let entry_pri = level_priority(&entry.level);
                    if entry_pri > min_priority {
                        continue;
                    }

                    let color = match entry.level.as_str() {
                        "ERROR" => egui::Color32::from_rgb(255, 80, 80),
                        "WARN" => egui::Color32::from_rgb(255, 180, 50),
                        "INFO" => egui::Color32::from_rgb(150, 200, 255),
                        "DEBUG" => egui::Color32::from_rgb(160, 160, 160),
                        "TRACE" => egui::Color32::from_rgb(120, 120, 120),
                        _ => egui::Color32::GRAY,
                    };

                    ui.label(
                        egui::RichText::new(format!(
                            "[{}] {:5} {} — {}",
                            entry.timestamp, entry.level, entry.target, entry.message
                        ))
                        .monospace()
                        .color(color)
                        .size(11.0),
                    );
                }
            });
    });

    ctx.memory_mut(|mem| mem.data.insert_temp(filter_id, selected));
}
