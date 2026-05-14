use egui::Ui;

#[derive(Debug, Clone, Default)]
pub struct ConsoleState {
    pub selected_cluster: String,
    pub method: String,
    pub path: String,
    pub body: String,
    pub response: String,
    pub history: Vec<(String, String, String)>,
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

pub fn render_console_module(ui: &mut Ui, state: &mut ConsoleState, clusters: &[String]) {
    ui.heading("Elastic Console");
    ui.add_space(16.0);

    if clusters.is_empty() {
        ui.label("No clusters configured. Add a cluster first.");
        return;
    }

    egui::Frame::none()
        .fill(crate::ui::theme::Theme::BG_CARD)
        .rounding(crate::ui::theme::Theme::CARD_ROUNDING)
        .inner_margin(crate::ui::theme::Theme::CARD_PADDING)
        .show(ui, |ui| {
            ui.horizontal(|ui| {
                ui.label("Cluster:");
                egui::ComboBox::from_id_source("console_cluster")
                    .selected_text(&state.selected_cluster)
                    .show_ui(ui, |ui| {
                        for cluster in clusters {
                            ui.selectable_value(&mut state.selected_cluster, cluster.clone(), cluster);
                        }
                    });
                
                ui.label("Method:");
                egui::ComboBox::from_id_source("console_method")
                    .selected_text(&state.method)
                    .show_ui(ui, |ui| {
                        for m in ["GET", "POST", "PUT", "DELETE", "HEAD"] {
                            ui.selectable_value(&mut state.method, m.to_string(), m);
                        }
                    });
            });

            ui.add_space(8.0);
            ui.horizontal(|ui| {
                ui.label("Path:");
                ui.text_edit_singleline(&mut state.path);
                if ui.button("Send").clicked() {
                    state.history.push((state.method.clone(), state.path.clone(), state.body.clone()));
                    state.response = "Request sent... (not yet implemented)".to_string();
                }
            });

            ui.add_space(8.0);
            ui.label("Body:");
            ui.add_sized(
                [ui.available_width(), 120.0],
                egui::TextEdit::multiline(&mut state.body).code_editor(),
            );

            ui.add_space(8.0);
            ui.label("Response:");
            ui.add_sized(
                [ui.available_width(), 200.0],
                egui::TextEdit::multiline(&mut state.response).code_editor(),
            );
        });
}
