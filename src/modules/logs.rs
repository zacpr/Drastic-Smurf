//! Logs tab: query ECS-style `logs-*` indices on a selected cluster,
//! filter by app/service, hostname, severity and free-text query.

use egui::{Color32, Ui};
use crate::core::es_client::{LogEntry, LogFieldNames, LogFilters};
use crate::ui::theme::Theme;

#[derive(Debug, Clone)]
pub struct LogsState {
    pub selected_cluster: String,
    pub filters: LogFilters,
    pub field_names: LogFieldNames,
    pub index_pattern: String,
    pub limit: usize,
    pub results: Vec<LogEntry>,
    pub is_loading: bool,
    pub error: Option<String>,
    pub expanded_id: Option<String>,
    pub show_settings: bool,
}

impl Default for LogsState {
    fn default() -> Self {
        Self {
            selected_cluster: String::new(),
            filters: LogFilters::default(),
            field_names: LogFieldNames {
                timestamp: "@timestamp".to_string(),
                message: "message".to_string(),
                app: "service.name".to_string(),
                hostname: "host.name".to_string(),
                severity: "log.level".to_string(),
            },
            index_pattern: "logs-elasticsearch-*,logs-kibana-*".to_string(),
            limit: 1000,
            results: Vec::new(),
            is_loading: false,
            error: None,
            expanded_id: None,
            show_settings: false,
        }
    }
}

impl LogsState {
    pub fn from_settings(settings: &crate::core::config::LogSettings) -> Self {
        let mut state = Self::default();
        state.index_pattern = settings.index_pattern.clone();
        state.limit = settings.limit;
        state.field_names.timestamp.clone_from(&settings.timestamp_field);
        state.field_names.message.clone_from(&settings.message_field);
        state.field_names.app.clone_from(&settings.app_field);
        state.field_names.hostname.clone_from(&settings.hostname_field);
        state.field_names.severity.clone_from(&settings.severity_field);
        state
    }

    pub fn to_settings(&self) -> crate::core::config::LogSettings {
        crate::core::config::LogSettings {
            index_pattern: self.index_pattern.clone(),
            limit: self.limit,
            timestamp_field: self.field_names.timestamp.clone(),
            message_field: self.field_names.message.clone(),
            app_field: self.field_names.app.clone(),
            hostname_field: self.field_names.hostname.clone(),
            severity_field: self.field_names.severity.clone(),
        }
    }
}

#[allow(clippy::too_many_arguments)]
pub fn render_logs_module(
    ui: &mut Ui,
    state: &mut LogsState,
    cluster_names: &[String],
    on_search: &mut Option<(String, LogFilters, LogFieldNames, String, usize)>,
    on_settings_changed: &mut bool,
) {
    // Ensure a cluster is selected if any exist.
    if state.selected_cluster.is_empty() && !cluster_names.is_empty() {
        state.selected_cluster = cluster_names[0].clone();
    }
    if !cluster_names.contains(&state.selected_cluster) && !cluster_names.is_empty() {
        state.selected_cluster = cluster_names[0].clone();
    }

    ui.horizontal(|ui| {
        ui.heading(egui::RichText::new("Logs").color(Theme::accent()).size(16.0));
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            if ui
                .selectable_label(state.show_settings, "⚙ Settings")
                .clicked()
            {
                state.show_settings = !state.show_settings;
            }
        });
    });
    ui.add_space(8.0);

    if cluster_names.is_empty() {
        ui.label(
            egui::RichText::new("No clusters configured. Add a cluster first.")
                .color(Theme::text_muted()),
        );
        return;
    }

    // Cluster selector
    ui.horizontal(|ui| {
        ui.label("Cluster:");
        egui::ComboBox::from_id_salt("logs_cluster_select")
            .selected_text(&state.selected_cluster)
            .width(ui.available_width().min(300.0))
            .show_ui(ui, |ui| {
                for name in cluster_names {
                    ui.selectable_value(&mut state.selected_cluster, name.clone(), name);
                }
            });
    });
    ui.add_space(8.0);

    // Settings panel
    if state.show_settings {
        egui::Frame::new()
            .fill(Theme::bg_card())
            .corner_radius(6.0)
            .inner_margin(egui::Margin::same(10))
            .show(ui, |ui| {
                ui.strong("Log index & field mapping");
                ui.add_space(6.0);

                ui.horizontal(|ui| {
                    ui.label("Index pattern:");
                    ui.text_edit_singleline(&mut state.index_pattern);
                });
                ui.horizontal(|ui| {
                    ui.label("Timestamp field:");
                    ui.text_edit_singleline(&mut state.field_names.timestamp);
                });
                ui.horizontal(|ui| {
                    ui.label("Message field:");
                    ui.text_edit_singleline(&mut state.field_names.message);
                });
                ui.horizontal(|ui| {
                    ui.label("App field:");
                    ui.text_edit_singleline(&mut state.field_names.app);
                });
                ui.horizontal(|ui| {
                    ui.label("Hostname field:");
                    ui.text_edit_singleline(&mut state.field_names.hostname);
                });
                ui.horizontal(|ui| {
                    ui.label("Severity field:");
                    ui.text_edit_singleline(&mut state.field_names.severity);
                });
                ui.horizontal(|ui| {
                    ui.label("Page size:");
                    ui.add(egui::DragValue::new(&mut state.limit).range(10..=10000).speed(10));
                });

                if ui.button("Reset to ECS defaults").clicked() {
                    state.index_pattern = "logs-*".to_string();
                    state.limit = 1000;
                    state.field_names.timestamp = "@timestamp".to_string();
                    state.field_names.message = "message".to_string();
                    state.field_names.app = "service.name".to_string();
                    state.field_names.hostname = "host.name".to_string();
                    state.field_names.severity = "log.level".to_string();
                }
                *on_settings_changed = true;
            });
        ui.add_space(8.0);
    }

    // Filters
    egui::Frame::new()
        .fill(Theme::bg_card())
        .corner_radius(6.0)
        .inner_margin(egui::Margin::same(10))
        .show(ui, |ui| {
            ui.horizontal(|ui| {
                ui.label("App:");
                ui.add(
                    egui::TextEdit::singleline(&mut state.filters.app)
                        .desired_width(120.0),
                );
                ui.label("Hostname:");
                ui.add(
                    egui::TextEdit::singleline(&mut state.filters.hostname)
                        .desired_width(140.0),
                );
                ui.label("Severity:");
                ui.add(
                    egui::TextEdit::singleline(&mut state.filters.severity)
                        .hint_text("error, warn, info...")
                        .desired_width(100.0),
                );
            });
            ui.horizontal(|ui| {
                ui.label("Query:");
                ui.add(
                    egui::TextEdit::singleline(&mut state.filters.query)
                        .hint_text("KQL / query_string, e.g. cluster:prod")
                        .desired_width(ui.available_width() - 180.0),
                );

                let enter_pressed = ui.input(|i| i.key_pressed(egui::Key::Enter));
                if ui.button("🔍 Search").clicked() || enter_pressed {
                    *on_search = Some((
                        state.selected_cluster.clone(),
                        state.filters.clone(),
                        state.field_names.clone(),
                        state.index_pattern.clone(),
                        state.limit,
                    ));
                    state.is_loading = true;
                    state.error = None;
                }
                if ui.button("Clear").clicked() {
                    state.filters = LogFilters::default();
                }
            });
        });
    ui.add_space(8.0);

    // Status / error
    if state.is_loading {
        ui.label(egui::RichText::new("Loading logs…").color(Theme::text_muted()));
    }
    if let Some(err) = &state.error {
        ui.label(egui::RichText::new(format!("⚠ {}", err)).color(Theme::danger()));
    }

    // Results summary
    if !state.is_loading && state.error.is_none() {
        ui.label(
            egui::RichText::new(format!("{} log entries", state.results.len()))
                .color(Theme::text_muted())
                .size(11.0),
        );
    }

    // Results table
    egui::ScrollArea::vertical()
        .id_salt("logs_results_scroll")
        .auto_shrink([false; 2])
        .show(ui, |ui| {
            let mut expanded_id = state.expanded_id.clone();
            for (idx, entry) in state.results.iter().enumerate() {
                expanded_id = render_log_row(ui, idx, entry, expanded_id);
            }
            state.expanded_id = expanded_id;
        });
}

fn render_log_row(
    ui: &mut Ui,
    idx: usize,
    entry: &LogEntry,
    expanded_id: Option<String>,
) -> Option<String> {
    let row_id = format!("{}-{}", idx, entry.timestamp.as_deref().unwrap_or(""));
    let is_expanded = expanded_id.as_deref() == Some(&row_id);
    let mut new_expanded_id = expanded_id;

    egui::Frame::new()
        .fill(Theme::bg_input())
        .corner_radius(4.0)
        .inner_margin(egui::Margin::same(6))
        .show(ui, |ui| {
            ui.horizontal(|ui| {
                let btn_text = if is_expanded { "▼" } else { "▶" };
                if ui.button(btn_text).clicked() {
                    new_expanded_id = if is_expanded { None } else { Some(row_id.clone()) };
                }

                // Timestamp
                ui.label(
                    egui::RichText::new(entry.timestamp.as_deref().unwrap_or("—"))
                        .color(Theme::text_muted())
                        .size(10.5)
                        .monospace(),
                );

                // Severity badge
                let severity = entry.severity.as_deref().unwrap_or("").to_lowercase();
                let (badge_text, badge_color) = match severity.as_str() {
                    "error" | "fatal" | "crit" | "critical" | "emergency" | "alert" => {
                        ("ERR", Theme::danger())
                    }
                    "warn" | "warning" => ("WRN", Theme::snapshot_partial()),
                    "info" => ("INF", Theme::success()),
                    "debug" => ("DBG", Theme::text_muted()),
                    "trace" => ("TRC", Theme::text_muted()),
                    _ => (entry.severity.as_deref().unwrap_or("?"), Theme::text_secondary()),
                };
                egui::Frame::new()
                    .fill(badge_color)
                    .corner_radius(3.0)
                    .inner_margin(egui::Margin::symmetric(4, 1))
                    .show(ui, |ui| {
                        ui.label(
                            egui::RichText::new(badge_text)
                                .color(Color32::WHITE)
                                .strong()
                                .size(10.0),
                        );
                    })
                    .response
                    .on_hover_text(entry.severity.as_deref().unwrap_or("unknown"));

                // App / host
                ui.label(
                    egui::RichText::new(entry.app.as_deref().unwrap_or("—"))
                        .color(Theme::accent())
                        .size(11.0),
                );
                ui.label(
                    egui::RichText::new(entry.hostname.as_deref().unwrap_or("—"))
                        .color(Theme::text_secondary())
                        .size(11.0),
                );

                // Message
                let msg = entry.message.as_deref().unwrap_or("(no message)");
                let msg_short = if msg.len() > 120 { format!("{}…", &msg[..120]) } else { msg.to_string() };
                ui.label(
                    egui::RichText::new(msg_short)
                        .color(Theme::text_primary())
                        .size(11.0),
                );
            });

            if is_expanded {
                ui.add_space(6.0);
                let pretty = serde_json::to_string_pretty(&entry.raw_source).unwrap_or_default();
                ui.add(
                    egui::TextEdit::multiline(&mut pretty.as_str())
                        .desired_rows(10)
                        .desired_width(f32::INFINITY)
                        .font(egui::TextStyle::Monospace),
                );
            }
        });
    ui.add_space(4.0);
    new_expanded_id
}
