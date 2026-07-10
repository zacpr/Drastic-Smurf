use egui::Ui;
use serde_json::Value;

use crate::ui::theme::Theme;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum PainlessContext {
    #[default]
    PainlessTest,
    Score,
    Filter,
    Custom,
}

impl PainlessContext {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::PainlessTest => "painless_test",
            Self::Score => "score",
            Self::Filter => "filter",
            Self::Custom => "custom",
        }
    }
}

#[derive(Debug, Clone)]
pub struct PainlessTemplate {
    pub name: &'static str,
    pub description: &'static str,
    pub context: PainlessContext,
    pub custom_context: &'static str,
    pub script: &'static str,
    pub params: &'static str,
    pub index: &'static str,
    pub document: &'static str,
}

pub const TEMPLATES: &[PainlessTemplate] = &[
    PainlessTemplate {
        name: "Simple Math",
        description: "Standard painless_test context doing a basic addition of parameters.",
        context: PainlessContext::PainlessTest,
        custom_context: "",
        script: "params.a + params.b",
        params: r#"{
  "a": 10,
  "b": 20
}"#,
        index: "",
        document: "{}",
    },
    PainlessTemplate {
        name: "Score Calculation",
        description: "Simulates custom scoring against an in-memory document with a 'views' field.",
        context: PainlessContext::Score,
        custom_context: "",
        script: "doc['views'].value * params.multiplier",
        params: r#"{
  "multiplier": 1.5
}"#,
        index: "my-index",
        document: r#"{
  "views": 100
}"#,
    },
    PainlessTemplate {
        name: "Document Filter",
        description: "Tests filter scripts that check if document fields match specific query params.",
        context: PainlessContext::Filter,
        custom_context: "",
        script: "doc['age'].value >= params.min_age && doc['status'].value == 'active'",
        params: r#"{
  "min_age": 21
}"#,
        index: "users",
        document: r#"{
  "age": 25,
  "status": "active"
}"#,
    },
    PainlessTemplate {
        name: "String Manipulation",
        description: "Uses Java String functions to format and return a new string in the default test environment.",
        context: PainlessContext::PainlessTest,
        custom_context: "",
        script: "params.prefix + params.text.toUpperCase() + params.suffix",
        params: r#"{
  "prefix": "[LOG] ",
  "text": "error encountered",
  "suffix": "!"
}"#,
        index: "",
        document: "{}",
    },
];

#[derive(Debug, Clone)]
pub struct PainlessState {
    pub selected_cluster: String,
    pub selected_template_index: Option<usize>,
    pub context: PainlessContext,
    pub custom_context: String,
    pub script_source: String,
    pub params_json: String,
    pub index: String,
    pub document_json: String,
    pub response: String,
    pub full_response: Option<String>,
    pub is_loading: bool,
    pub params_error: Option<String>,
    pub doc_error: Option<String>,
    pub show_context_setup: bool,
}

impl Default for PainlessState {
    fn default() -> Self {
        Self {
            selected_cluster: String::new(),
            selected_template_index: Some(0),
            context: PainlessContext::PainlessTest,
            custom_context: String::new(),
            script_source: "params.a + params.b".to_string(),
            params_json: "{\n  \"a\": 10,\n  \"b\": 20\n}".to_string(),
            index: String::new(),
            document_json: "{}".to_string(),
            response: String::new(),
            full_response: None,
            is_loading: false,
            params_error: None,
            doc_error: None,
            show_context_setup: false,
        }
    }
}

pub fn render_painless_module(
    ui: &mut Ui,
    state: &mut PainlessState,
    clusters: &[String],
    on_send: &mut Option<(String, String)>,
) {
    ui.heading("Painless Script Playground");
    ui.add_space(8.0);

    if clusters.is_empty() {
        ui.label("No clusters configured. Add a cluster first.");
        return;
    }

    if state.selected_cluster.is_empty() && !clusters.is_empty() {
        state.selected_cluster = clusters[0].clone();
    }

    let total_available = ui.available_size() - egui::vec2(0.0, 12.0);
    ui.allocate_ui_with_layout(
        total_available,
        egui::Layout::left_to_right(egui::Align::Min),
        |ui| {
            let col_width = (ui.available_width() - 16.0) / 2.0;

            // --- LEFT COLUMN: Editor & Context ---
            ui.allocate_ui_with_layout(
                egui::Vec2::new(col_width, ui.available_height()),
                egui::Layout::top_down(egui::Align::Min),
                |ui| {
                    egui::ScrollArea::vertical()
                        .id_salt("painless_left_scroll")
                        .show(ui, |ui| {
                            render_left_pane(ui, state, clusters, on_send);
                        });
                },
            );

            ui.add_space(8.0);

            // --- RIGHT COLUMN: Response ---
            ui.allocate_ui_with_layout(
                egui::Vec2::new(col_width, ui.available_height()),
                egui::Layout::top_down(egui::Align::Min),
                |ui| {
                    render_right_pane(ui, state);
                },
            );
        },
    );
}

fn render_left_pane(
    ui: &mut Ui,
    state: &mut PainlessState,
    clusters: &[String],
    on_send: &mut Option<(String, String)>,
) {
    // Card 1: Configuration & Templates
    egui::Frame::new()
        .fill(Theme::bg_card())
        .corner_radius(Theme::CARD_ROUNDING)
        .inner_margin(Theme::CARD_PADDING)
        .show(ui, |ui| {
            ui.label(
                egui::RichText::new("CONFIGURATION & TEMPLATES")
                    .strong()
                    .color(Theme::text_secondary())
                    .size(11.0),
            );
            ui.add_space(8.0);

            ui.horizontal(|ui| {
                ui.label("Cluster:");
                egui::ComboBox::from_id_salt("painless_cluster")
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

                ui.add_space(16.0);

                ui.label("Template:");
                let template_text = match state.selected_template_index {
                    Some(idx) => TEMPLATES[idx].name,
                    None => "Custom",
                };
                egui::ComboBox::from_id_salt("painless_template")
                    .selected_text(template_text)
                    .show_ui(ui, |ui| {
                        if ui.selectable_label(state.selected_template_index.is_none(), "Custom").clicked() {
                            state.selected_template_index = None;
                        }
                        for (idx, t) in TEMPLATES.iter().enumerate() {
                            if ui.selectable_label(state.selected_template_index == Some(idx), t.name).clicked() {
                                state.selected_template_index = Some(idx);
                                state.context = t.context;
                                state.custom_context = t.custom_context.to_string();
                                state.script_source = t.script.to_string();
                                state.params_json = t.params.to_string();
                                state.index = t.index.to_string();
                                state.document_json = t.document.to_string();
                                state.params_error = None;
                                state.doc_error = None;
                            }
                        }
                    });
            });

            if let Some(idx) = state.selected_template_index {
                ui.add_space(4.0);
                ui.label(
                    egui::RichText::new(TEMPLATES[idx].description)
                        .size(11.0)
                        .italics()
                        .color(Theme::text_secondary()),
                );
            }
        });

    ui.add_space(8.0);

    // Card 2: Painless Script Source Code
    egui::Frame::new()
        .fill(Theme::bg_card())
        .corner_radius(Theme::CARD_ROUNDING)
        .inner_margin(Theme::CARD_PADDING)
        .show(ui, |ui| {
            ui.horizontal(|ui| {
                ui.label(
                    egui::RichText::new("PAINLESS SCRIPT")
                        .strong()
                        .color(Theme::text_secondary())
                        .size(11.0),
                );
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    ui.label("Context:");
                    egui::ComboBox::from_id_salt("painless_context_selector")
                        .selected_text(state.context.as_str())
                        .show_ui(ui, |ui| {
                            for ctx_type in [
                                PainlessContext::PainlessTest,
                                PainlessContext::Score,
                                PainlessContext::Filter,
                                PainlessContext::Custom,
                            ] {
                                if ui.selectable_value(&mut state.context, ctx_type, ctx_type.as_str()).clicked() {
                                    state.selected_template_index = None;
                                }
                            }
                        });
                });
            });

            if state.context == PainlessContext::Custom {
                ui.add_space(4.0);
                ui.horizontal(|ui| {
                    ui.label("Custom Context:");
                    let res = ui.text_edit_singleline(&mut state.custom_context);
                    if res.changed() {
                        state.selected_template_index = None;
                    }
                });
            }

            ui.add_space(8.0);

            let res = ui.add(
                egui::TextEdit::multiline(&mut state.script_source)
                    .font(egui::TextStyle::Monospace)
                    .code_editor()
                    .desired_rows(12)
                    .desired_width(ui.available_width())
            );
            if res.changed() {
                state.selected_template_index = None;
            }
        });

    ui.add_space(8.0);

    // Card 3: Parameters (JSON)
    egui::Frame::new()
        .fill(Theme::bg_card())
        .corner_radius(Theme::CARD_ROUNDING)
        .inner_margin(Theme::CARD_PADDING)
        .show(ui, |ui| {
            ui.horizontal(|ui| {
                ui.label(
                    egui::RichText::new("SCRIPT PARAMETERS (JSON)")
                        .strong()
                        .color(Theme::text_secondary())
                        .size(11.0),
                );
                if let Some(ref err) = state.params_error {
                    ui.colored_label(Theme::danger(), format!("⚠️ {}", err));
                }
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    if ui.button("✨ Format").clicked() && !state.params_json.trim().is_empty() {
                        match crate::modules::console::prettify_json_body(&state.params_json) {
                            Ok(formatted) => {
                                state.params_json = formatted;
                                state.params_error = None;
                            }
                            Err(e) => {
                                state.params_error = Some(e);
                            }
                        }
                    }
                });
            });
            ui.add_space(6.0);

            let res = ui.add(
                egui::TextEdit::multiline(&mut state.params_json)
                    .font(egui::TextStyle::Monospace)
                    .code_editor()
                    .desired_rows(6)
                    .desired_width(ui.available_width())
                    .layouter(&mut |ui, text, wrap_width| {
                        crate::ui::widgets::json_layouter(ui, text, wrap_width)
                    }),
            );
            if res.changed() {
                state.selected_template_index = None;
                if state.params_json.trim().is_empty() {
                    state.params_error = None;
                } else {
                    match serde_json::from_str::<Value>(&state.params_json) {
                        Ok(_) => state.params_error = None,
                        Err(e) => state.params_error = Some(e.to_string()),
                    }
                }
            }
        });

    ui.add_space(8.0);

    // Card 4: Context Setup (Index & Document)
    let show_setup = state.context == PainlessContext::Score
        || state.context == PainlessContext::Filter
        || state.context == PainlessContext::Custom
        || state.show_context_setup;

    if show_setup {
        egui::Frame::new()
            .fill(Theme::bg_card())
            .corner_radius(Theme::CARD_ROUNDING)
            .inner_margin(Theme::CARD_PADDING)
            .show(ui, |ui| {
                ui.horizontal(|ui| {
                    ui.label(
                        egui::RichText::new("CONTEXT SETUP")
                            .strong()
                            .color(Theme::text_secondary())
                            .size(11.0),
                    );
                    if let Some(ref err) = state.doc_error {
                        ui.colored_label(Theme::danger(), format!("⚠️ {}", err));
                    }
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        if ui.button("✨ Format").clicked() && !state.document_json.trim().is_empty() {
                            match crate::modules::console::prettify_json_body(&state.document_json) {
                                Ok(formatted) => {
                                    state.document_json = formatted;
                                    state.doc_error = None;
                                }
                                Err(e) => {
                                    state.doc_error = Some(e);
                                }
                            }
                        }
                    });
                });
                ui.add_space(6.0);

                ui.horizontal(|ui| {
                    ui.label("Mock Index:");
                    let res = ui.text_edit_singleline(&mut state.index);
                    if res.changed() {
                        state.selected_template_index = None;
                    }
                });
                ui.add_space(6.0);

                ui.label("Mock Document (JSON):");
                let res = ui.add(
                    egui::TextEdit::multiline(&mut state.document_json)
                        .font(egui::TextStyle::Monospace)
                        .code_editor()
                        .desired_rows(6)
                        .desired_width(ui.available_width())
                        .layouter(&mut |ui, text, wrap_width| {
                            crate::ui::widgets::json_layouter(ui, text, wrap_width)
                        }),
                );
                if res.changed() {
                    state.selected_template_index = None;
                    if state.document_json.trim().is_empty() {
                        state.doc_error = None;
                    } else {
                        match serde_json::from_str::<Value>(&state.document_json) {
                            Ok(_) => state.doc_error = None,
                            Err(e) => state.doc_error = Some(e.to_string()),
                        }
                    }
                }
            });

        ui.add_space(8.0);
    } else {
        ui.checkbox(&mut state.show_context_setup, "Show Context Setup (Mock mappings)");
        ui.add_space(8.0);
    }

    // Run Button
    let has_errors = state.params_error.is_some() || state.doc_error.is_some();
    let accent_color = Theme::accent();
    let run_btn = ui.add_enabled(
        !state.is_loading && !has_errors,
        egui::Button::new(
            egui::RichText::new("⚡ Run Painless Script")
                .color(Theme::contrast_text_color(accent_color))
                .strong(),
        )
        .fill(accent_color),
    );

    if run_btn.clicked() {
        state.is_loading = true;

        let mut script_payload = serde_json::json!({
            "source": state.script_source,
        });

        if let Ok(params_val) = serde_json::from_str::<Value>(&state.params_json) {
            script_payload["params"] = params_val;
        }

        let context_str = if state.context == PainlessContext::Custom {
            state.custom_context.trim().to_string()
        } else {
            state.context.as_str().to_string()
        };

        let mut request_body = serde_json::json!({
            "script": script_payload,
            "context": context_str,
        });

        let has_index = !state.index.trim().is_empty();
        let has_doc = !state.document_json.trim().is_empty() && state.document_json.trim() != "{}";

        if has_index || has_doc {
            let mut context_setup = serde_json::json!({});
            if has_index {
                context_setup["index"] = Value::String(state.index.trim().to_string());
            }
            let doc_parsed = serde_json::from_str::<Value>(&state.document_json);
            if let Some(doc_val) = doc_parsed.ok().filter(|_| has_doc) {
                context_setup["document"] = doc_val;
            }
            request_body["context_setup"] = context_setup;
        }

        if let Ok(payload_str) = serde_json::to_string(&request_body) {
            *on_send = Some((state.selected_cluster.clone(), payload_str));
        } else {
            state.response = "Failed to serialize request payload".to_string();
            state.is_loading = false;
        }
    }
}

fn render_right_pane(ui: &mut Ui, state: &mut PainlessState) {
    let height = ui.available_height();

    egui::Frame::new()
        .fill(Theme::bg_card())
        .corner_radius(Theme::CARD_ROUNDING)
        .inner_margin(Theme::CARD_PADDING)
        .show(ui, |ui| {
            ui.set_height(height - 32.0);

            ui.horizontal(|ui| {
                ui.label(
                    egui::RichText::new("EXECUTION RESULT")
                        .strong()
                        .color(Theme::text_secondary())
                        .size(11.0),
                );
                if state.is_loading {
                    ui.spinner();
                }
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    if ui.small_button("📋 Copy Response").clicked() {
                        let text_to_copy = state.full_response.as_ref().unwrap_or(&state.response).clone();
                        ui.ctx().copy_text(text_to_copy);
                    }
                    if ui.small_button("Clear").clicked() {
                        state.response.clear();
                        state.full_response = None;
                    }
                });
            });
            ui.add_space(8.0);

            let scroll_height = ui.available_height() - 16.0;
            egui::ScrollArea::vertical()
                .id_salt("painless_response_scroll")
                .max_height(scroll_height)
                .auto_shrink([false, false])
                .show(ui, |ui| {
                    ui.add(
                        egui::TextEdit::multiline(&mut state.response)
                            .font(egui::TextStyle::Monospace)
                            .code_editor()
                            .desired_width(ui.available_width())
                            .desired_rows(24)
                            .layouter(&mut |ui, text, wrap_width| {
                                crate::ui::widgets::json_layouter(ui, text, wrap_width)
                            }),
                    );
                });
        });
}
