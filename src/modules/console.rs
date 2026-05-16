use egui::Ui;

use crate::core::config::SavedQuery;

#[derive(Debug, Clone, Default)]
pub struct ConsoleState {
    pub selected_cluster: String,
    pub method: String,
    pub path: String,
    pub body: String,
    pub response: String,
    pub history: Vec<(String, String, String, String)>,
    pub is_loading: bool,
    pub saved_queries: Vec<SavedQuery>,
    pub query_name_input: String,
    pub show_save_dialog: bool,
}

impl ConsoleState {
    pub fn new() -> Self {
        Self {
            method: "GET".to_string(),
            path: "/_cluster/health".to_string(),
            ..Default::default()
        }
    }
}

pub fn render_console_module(
    ui: &mut Ui,
    state: &mut ConsoleState,
    clusters: &[String],
    on_send: &mut Option<(String, String, String, Option<String>)>,
    on_save_query: &mut Option<SavedQuery>,
) {
    ui.heading("Elastic Console");
    ui.add_space(16.0);

    if clusters.is_empty() {
        ui.label("No clusters configured. Add a cluster first.");
        return;
    }

    egui::Frame::new()
        .fill(crate::ui::theme::Theme::bg_card())
        .corner_radius(crate::ui::theme::Theme::CARD_ROUNDING)
        .inner_margin(crate::ui::theme::Theme::CARD_PADDING)
        .show(ui, |ui| {
            ui.horizontal(|ui| {
                ui.label("Cluster:");
                egui::ComboBox::from_id_salt("console_cluster")
                    .selected_text(&state.selected_cluster)
                    .show_ui(ui, |ui| {
                        for cluster in clusters {
                            ui.selectable_value(
                                &mut state.selected_cluster,
                                cluster.clone(),
                                cluster,
                            );
                        }
                    });

                ui.label("Method:");
                egui::ComboBox::from_id_salt("console_method")
                    .selected_text(&state.method)
                    .show_ui(ui, |ui| {
                        for m in ["GET", "POST", "PUT", "DELETE", "HEAD"] {
                            ui.selectable_value(&mut state.method, m.to_string(), m);
                        }
                    });
            });

            // Saved queries
            if !state.saved_queries.is_empty() {
                ui.add_space(4.0);
                ui.horizontal(|ui| {
                    ui.label("Saved:");
                    egui::ComboBox::from_id_salt("console_saved_queries")
                        .selected_text("Select query...")
                        .width(200.0)
                        .show_ui(ui, |ui| {
                            for query in &state.saved_queries {
                                if ui
                                    .selectable_label(false, format!(
                                        "{} {} {}",
                                        query.name, query.method, query.path
                                    ))
                                    .clicked()
                                {
                                    state.method = query.method.clone();
                                    state.path = query.path.clone();
                                    state.body = query.body.clone().unwrap_or_default();
                                }
                            }
                        });
                });
            }

            ui.add_space(8.0);
            ui.horizontal(|ui| {
                ui.label("Path:");
                ui.text_edit_singleline(&mut state.path);
                let button = ui.button("Send");
                if state.is_loading {
                    ui.spinner();
                }
                if button.clicked() && !state.is_loading {
                    state.is_loading = true;
                    state.history.push((
                        state.selected_cluster.clone(),
                        state.method.clone(),
                        state.path.clone(),
                        state.body.clone(),
                    ));
                    let body = if state.body.trim().is_empty() {
                        None
                    } else {
                        Some(state.body.clone())
                    };
                    *on_send = Some((
                        state.selected_cluster.clone(),
                        state.method.clone(),
                        state.path.clone(),
                        body,
                    ));
                }
                if ui.button("💾 Save").clicked() {
                    state.show_save_dialog = true;
                    state.query_name_input.clear();
                }
            });

            // Save query dialog
            if state.show_save_dialog {
                ui.add_space(4.0);
                ui.horizontal(|ui| {
                    ui.label("Name:");
                    ui.text_edit_singleline(&mut state.query_name_input);
                    if ui.button("Save").clicked() && !state.query_name_input.is_empty() {
                        *on_save_query = Some(SavedQuery {
                            name: state.query_name_input.clone(),
                            method: state.method.clone(),
                            path: state.path.clone(),
                            body: if state.body.trim().is_empty() {
                                None
                            } else {
                                Some(state.body.clone())
                            },
                        });
                        state.show_save_dialog = false;
                        state.query_name_input.clear();
                    }
                    if ui.button("Cancel").clicked() {
                        state.show_save_dialog = false;
                        state.query_name_input.clear();
                    }
                });
            }

            ui.add_space(8.0);
            ui.label("Body:");
            ui.add_sized(
                [ui.available_width(), 120.0],
                egui::TextEdit::multiline(&mut state.body).code_editor(),
            );

            ui.add_space(8.0);
            ui.horizontal(|ui| {
                ui.label("Response:");
                if ui.small_button("Clear").clicked() {
                    state.response.clear();
                }
            });
            ui.add_sized(
                [ui.available_width(), 200.0],
                egui::TextEdit::multiline(&mut state.response).code_editor(),
            );
        });
}
