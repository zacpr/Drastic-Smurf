use egui::Ui;
use serde_json::{Value, json};

use crate::core::config::{PipelineMode, PipelineTargetKind, PipelineTestPreset};
use crate::ui::theme::Theme;

#[derive(Debug, Clone)]
pub struct OnlinePipelineState {
    pub selected_cluster: String,
    pub target_kind: PipelineTargetKind,
    pub target_name: String,
    pub target_filter: String,
    pub indices: Vec<String>,
    pub datastreams: Vec<String>,
    pub pipeline_mode: PipelineMode,
    pub pipeline_id: String,
    pub pipeline_filter: String,
    pub pipeline_ids: Vec<String>,
    pub pipeline_text: String,
    pub docs_text: String,
    pub docs_error: Option<String>,
    pub pipeline_error: Option<String>,
    pub result_text: String,
    pub is_loading: bool,
    pub indices_loading: bool,
    pub pipelines_loading: bool,
    pub last_selected_cluster: String,
    pub show_doc_modal: bool,
    pub doc_modal_index: String,
    pub doc_modal_id: String,
    pub show_save_modal: bool,
    pub preset_name_input: String,
    pub show_load_modal: bool,
}

impl Default for OnlinePipelineState {
    fn default() -> Self {
        let default_docs = serde_json::to_string_pretty(&json!([
            { "_source": { "message": "hello world", "level": "info" } }
        ]))
        .unwrap_or_default();
        Self {
            selected_cluster: String::new(),
            target_kind: PipelineTargetKind::Index,
            target_name: String::new(),
            target_filter: String::new(),
            indices: Vec::new(),
            datastreams: Vec::new(),
            pipeline_mode: PipelineMode::Default,
            pipeline_id: String::new(),
            pipeline_filter: String::new(),
            pipeline_ids: Vec::new(),
            pipeline_text: String::new(),
            docs_text: default_docs,
            docs_error: None,
            pipeline_error: None,
            result_text: String::new(),
            is_loading: false,
            indices_loading: false,
            pipelines_loading: false,
            last_selected_cluster: String::new(),
            show_doc_modal: false,
            doc_modal_index: String::new(),
            doc_modal_id: String::new(),
            show_save_modal: false,
            preset_name_input: String::new(),
            show_load_modal: false,
        }
    }
}

#[derive(Debug, Clone)]
pub enum OnlineAction {
    FetchTargets(String),
    FetchPipelines(String),
    LoadPipelineDef(String, String),
    FetchDoc(String, String, String),
    PickFile,
    Simulate {
        cluster: String,
        target: String,
        mode: PipelineMode,
        pipeline_id: String,
        pipeline_text: String,
        docs_text: String,
    },
    SavePreset(String),
    LoadPreset(String),
    DeletePreset(String),
}

/// Build the request body for the simulate ingest API.
/// In Default mode the cluster's stored pipeline is used (no substitutions).
/// In Loaded mode the supplied definition is sent via `pipeline_substitutions` keyed by id.
pub fn build_simulate_body(
    docs_text: &str,
    mode: &PipelineMode,
    pipeline_id: &str,
    pipeline_text: &str,
) -> Result<Value, String> {
    let docs_value: Value = serde_json::from_str(docs_text)
        .map_err(|e| format!("Documents must be valid JSON: {}", e))?;
    let docs_array = match docs_value {
        Value::Array(arr) => Value::Array(arr),
        Value::Object(_) => Value::Array(vec![docs_value]),
        other => {
            return Err(format!(
                "Documents must be a JSON array or object, got {}",
                type_of(&other)
            ));
        }
    };

    match mode {
        PipelineMode::Default => Ok(json!({ "docs": docs_array })),
        PipelineMode::Loaded => {
            if pipeline_id.trim().is_empty() {
                return Err("Pipeline id is required in Loaded mode".to_string());
            }
            let def: Value = serde_json::from_str(pipeline_text)
                .map_err(|e| format!("Pipeline definition must be valid JSON: {}", e))?;
            let mut map = serde_json::Map::new();
            map.insert(pipeline_id.to_string(), def);
            Ok(json!({
                "docs": docs_array,
                "pipeline_substitutions": Value::Object(map),
            }))
        }
    }
}

fn type_of(v: &Value) -> &'static str {
    match v {
        Value::Null => "null",
        Value::Bool(_) => "boolean",
        Value::Number(_) => "number",
        Value::String(_) => "string",
        Value::Array(_) => "array",
        Value::Object(_) => "object",
    }
}

pub fn apply_preset(state: &mut OnlinePipelineState, preset: &PipelineTestPreset) {
    state.selected_cluster = preset.cluster.clone();
    state.target_kind = preset.target_kind.clone();
    state.target_name = preset.target_name.clone();
    state.pipeline_mode = preset.pipeline_mode.clone();
    state.pipeline_id = preset.pipeline_id.clone();
    state.pipeline_text = preset.pipeline_text.clone();
    state.docs_text = preset.docs_text.clone();
    state.target_filter.clear();
    state.pipeline_filter.clear();
}

pub fn collect_preset(state: &OnlinePipelineState, name: String) -> PipelineTestPreset {
    PipelineTestPreset {
        name,
        cluster: state.selected_cluster.clone(),
        target_kind: state.target_kind.clone(),
        target_name: state.target_name.clone(),
        pipeline_mode: state.pipeline_mode.clone(),
        pipeline_id: state.pipeline_id.clone(),
        pipeline_text: state.pipeline_text.clone(),
        docs_text: state.docs_text.clone(),
    }
}

/// Append a fetched `_source` to the docs JSON array, preserving existing contents.
pub fn append_doc_source(docs_text: &str, source: Value) -> Result<String, String> {
    let parsed: Value = serde_json::from_str(docs_text)
        .map_err(|e| format!("Existing documents must be valid JSON: {}", e))?;
    let mut arr = match parsed {
        Value::Array(a) => a,
        Value::Object(_) => vec![parsed],
        _ => return Err("Existing documents must be a JSON array or object".to_string()),
    };
    arr.push(json!({ "_source": source }));
    serde_json::to_string_pretty(&Value::Array(arr))
        .map_err(|e| format!("Failed to serialize documents: {}", e))
}

/// Append docs from file text. If the file is a JSON array, each element is appended.
/// If it is a single JSON object, it is appended as one doc. Otherwise the raw text
/// is wrapped as a string `_source`.
pub fn append_file_docs(docs_text: &str, file_text: &str) -> Result<String, String> {
    let parsed = serde_json::from_str::<Value>(file_text).ok();

    let mut arr: Vec<Value> = if docs_text.trim().is_empty() {
        Vec::new()
    } else {
        let v: Value = serde_json::from_str(docs_text)
            .map_err(|e| format!("Existing documents must be valid JSON: {}", e))?;
        match v {
            Value::Array(a) => a,
            Value::Object(_) => vec![v],
            _ => return Err("Existing documents must be a JSON array or object".to_string()),
        }
    };

    match parsed {
        Some(Value::Array(items)) => {
            for item in items {
                if item.get("_source").is_some() {
                    arr.push(item);
                } else {
                    arr.push(json!({ "_source": item }));
                }
            }
        }
        Some(Value::Object(_)) => {
            if let Ok(v) = serde_json::from_str::<Value>(file_text) {
                arr.push(json!({ "_source": v }));
            } else {
                arr.push(json!({ "_source": { "raw": file_text } }));
            }
        }
        _ => {
            arr.push(json!({ "_source": { "raw": file_text } }));
        }
    }

    serde_json::to_string_pretty(&Value::Array(arr))
        .map_err(|e| format!("Failed to serialize documents: {}", e))
}

pub fn render_online_pipeline_module(
    ui: &mut Ui,
    state: &mut OnlinePipelineState,
    cluster_names: &[String],
    presets: &[PipelineTestPreset],
    on_action: &mut Vec<OnlineAction>,
) {
    if !cluster_names.is_empty() && state.selected_cluster.is_empty() {
        state.selected_cluster = cluster_names[0].clone();
    }

    let available_width = ui.available_width();
    let remaining_height = ui.available_height();

    let col1_width = available_width * 0.36;
    let col2_width = available_width * 0.32;
    let col3_width = available_width * 0.32 - 16.0;

    // --- Top bar: cluster / target type / target picker / refresh spinner ---
    let target_list: Vec<String> = match state.target_kind {
        PipelineTargetKind::Index => state.indices.clone(),
        PipelineTargetKind::DataStream => state.datastreams.clone(),
    };
    ui.horizontal(|ui| {
        ui.label("Cluster:");
        let prev = state.selected_cluster.clone();
        egui::ComboBox::from_id_salt("online_pipeline_cluster")
            .selected_text(&state.selected_cluster)
            .width(180.0)
            .show_ui(ui, |ui| {
                for c in cluster_names {
                    ui.selectable_value(&mut state.selected_cluster, c.clone(), c);
                }
            });
        if prev != state.selected_cluster {
            state.last_selected_cluster = prev;
            on_action.push(OnlineAction::FetchTargets(state.selected_cluster.clone()));
            on_action.push(OnlineAction::FetchPipelines(state.selected_cluster.clone()));
        }

        ui.add_space(8.0);
        ui.radio_value(
            &mut state.target_kind,
            PipelineTargetKind::Index,
            "📁 Indices",
        );
        ui.radio_value(
            &mut state.target_kind,
            PipelineTargetKind::DataStream,
            "🌊 Data Streams",
        );

        ui.add_space(8.0);

        // Target picker fills the rest of the row.
        if state.indices_loading && target_list.is_empty() {
            // Animated spinner while initial targets are loading. We request a
            // continuous repaint below so the spinner keeps animating.
            let _ = ui.add(
                egui::Spinner::new()
                    .color(Theme::info())
                    .size(16.0),
            );
            ui.label(
                egui::RichText::new(format!(
                    "Loading {}...",
                    match state.target_kind {
                        PipelineTargetKind::Index => "indices",
                        PipelineTargetKind::DataStream => "data streams",
                    }
                ))
                .italics()
                .color(Theme::text_muted()),
            );
        } else {
            crate::ui::widgets::filterable_select(
                ui,
                "online_target_select",
                &mut state.target_name,
                &mut state.target_filter,
                &target_list,
                if target_list.is_empty() {
                    "(no targets — click 🔄 to fetch)"
                } else {
                    "(select target)"
                },
            );
        }

        // Small refresh button to manually re-fetch targets.
        if ui
            .add_enabled(
                !state.selected_cluster.is_empty() && !state.indices_loading,
                egui::Button::new(
                    egui::RichText::new("🔄").color(Theme::text_primary()),
                )
                .fill(Theme::bg_input())
                .stroke(egui::Stroke::new(1.0, Theme::border())),
            )
            .on_hover_text("Re-fetch indices / data streams from the cluster")
            .clicked()
        {
            on_action.push(OnlineAction::FetchTargets(state.selected_cluster.clone()));
        }
    });

    // Keep the spinner animating while we're fetching targets.
    if state.indices_loading {
        ui.ctx().request_repaint();
    }

    // --- Preset bar: Save / Load sit on their own row so they never overlap the target picker. ---
    ui.horizontal(|ui| {
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            if ui.button("📂 Load Settings").clicked() {
                state.show_load_modal = true;
            }
            if ui.button("💾 Save Settings").clicked() {
                state.show_save_modal = true;
            }
        });
    });

    ui.add_space(8.0);

    ui.allocate_ui_with_layout(
        egui::Vec2::new(available_width, remaining_height - 36.0),
        egui::Layout::left_to_right(egui::Align::TOP),
        |ui| {
            ui.allocate_ui_with_layout(
                egui::Vec2::new(col1_width, remaining_height),
                egui::Layout::top_down(egui::Align::Min),
                |ui| {
                    render_pipeline_column(ui, state, on_action);
                },
            );
            ui.add_space(8.0);
            ui.allocate_ui_with_layout(
                egui::Vec2::new(col2_width, remaining_height),
                egui::Layout::top_down(egui::Align::Min),
                |ui| {
                    render_documents_column(ui, state, on_action);
                },
            );
            ui.add_space(8.0);
            ui.allocate_ui_with_layout(
                egui::Vec2::new(col3_width, remaining_height),
                egui::Layout::top_down(egui::Align::Min),
                |ui| {
                    render_result_column(ui, state);
                },
            );
        },
    );

    // --- Doc modal ---
    if state.show_doc_modal {
        egui::Window::new("🔎 Load Document from Elastic")
            .collapsible(false)
            .resizable(false)
            .anchor(egui::Align2::CENTER_CENTER, egui::Vec2::ZERO)
            .show(ui.ctx(), |ui| {
                ui.set_min_width(380.0);
                ui.label("Index/Data Stream:");
                ui.text_edit_singleline(&mut state.doc_modal_index);
                ui.add_space(4.0);
                ui.label("Document _id:");
                ui.text_edit_singleline(&mut state.doc_modal_id);
                ui.add_space(8.0);
                ui.horizontal(|ui| {
                    if ui.button("Load").clicked() {
                        if !state.doc_modal_index.is_empty() && !state.doc_modal_id.is_empty() {
                            on_action.push(OnlineAction::FetchDoc(
                                state.selected_cluster.clone(),
                                state.doc_modal_index.clone(),
                                state.doc_modal_id.clone(),
                            ));
                            state.show_doc_modal = false;
                        }
                    }
                    if ui.button("Cancel").clicked() {
                        state.show_doc_modal = false;
                    }
                });
            });
    }

    // --- Save preset modal ---
    if state.show_save_modal {
        egui::Window::new("💾 Save Test Settings")
            .collapsible(false)
            .resizable(false)
            .anchor(egui::Align2::CENTER_CENTER, egui::Vec2::ZERO)
            .show(ui.ctx(), |ui| {
                ui.set_min_width(360.0);
                ui.label("Preset name:");
                ui.text_edit_singleline(&mut state.preset_name_input);
                ui.add_space(8.0);
                ui.horizontal(|ui| {
                    if ui.button("Save").clicked() {
                        let name = state.preset_name_input.trim().to_string();
                        if !name.is_empty() {
                            on_action.push(OnlineAction::SavePreset(name));
                            state.show_save_modal = false;
                        }
                    }
                    if ui.button("Cancel").clicked() {
                        state.show_save_modal = false;
                    }
                });
            });
    }

    // --- Load preset modal ---
    if state.show_load_modal {
        egui::Window::new("📂 Load Test Settings")
            .collapsible(false)
            .resizable(true)
            .default_size([480.0, 320.0])
            .anchor(egui::Align2::CENTER_CENTER, egui::Vec2::ZERO)
            .show(ui.ctx(), |ui| {
                if presets.is_empty() {
                    ui.label(
                        egui::RichText::new("No saved presets yet.")
                            .color(Theme::text_muted()),
                    );
                } else {
                    egui::ScrollArea::vertical()
                        .max_height(260.0)
                        .show(ui, |ui| {
                            let mut to_delete: Option<String> = None;
                            for p in presets {
                                egui::Frame::new()
                                    .fill(Theme::bg_input())
                                    .corner_radius(Theme::CARD_ROUNDING)
                                    .inner_margin(8.0)
                                    .show(ui, |ui| {
                                        ui.horizontal(|ui| {
                                            ui.vertical(|ui| {
                                                ui.label(
                                                    egui::RichText::new(&p.name)
                                                        .strong()
                                                        .color(Theme::text_primary()),
                                                );
                                                ui.label(
                                                    egui::RichText::new(format!(
                                                        "{} · {} · {} · {}",
                                                        p.cluster,
                                                        match p.target_kind {
                                                            PipelineTargetKind::Index => "Index",
                                                            PipelineTargetKind::DataStream =>
                                                                "DataStream",
                                                        },
                                                        p.target_name,
                                                        match p.pipeline_mode {
                                                            PipelineMode::Default => "default",
                                                            PipelineMode::Loaded => "loaded",
                                                        },
                                                    ))
                                                    .color(Theme::text_muted())
                                                    .size(10.5),
                                                );
                                            });
                                            ui.with_layout(
                                                egui::Layout::right_to_left(egui::Align::Center),
                                                |ui| {
                                                    if ui.button("🗑").clicked() {
                                                        to_delete = Some(p.name.clone());
                                                    }
                                                    if ui.button("Load").clicked() {
                                                        on_action.push(OnlineAction::LoadPreset(
                                                            p.name.clone(),
                                                        ));
                                                        state.show_load_modal = false;
                                                    }
                                                },
                                            );
                                        });
                                    });
                                ui.add_space(4.0);
                            }
                            if let Some(name) = to_delete {
                                on_action.push(OnlineAction::DeletePreset(name));
                            }
                        });
                }
                ui.add_space(8.0);
                ui.horizontal(|ui| {
                    if ui.button("Close").clicked() {
                        state.show_load_modal = false;
                    }
                });
            });
    }
}

fn render_pipeline_column(
    ui: &mut Ui,
    state: &mut OnlinePipelineState,
    on_action: &mut Vec<OnlineAction>,
) {
    egui::Frame::new()
        .fill(Theme::bg_card())
        .corner_radius(Theme::CARD_ROUNDING)
        .inner_margin(Theme::CARD_PADDING)
        .show(ui, |ui| {
            ui.label(
                egui::RichText::new("Pipeline")
                    .strong()
                    .size(14.0)
                    .color(Theme::text_primary()),
            );
            ui.add_space(4.0);

            ui.horizontal(|ui| {
                ui.radio_value(&mut state.pipeline_mode, PipelineMode::Default, "Default (use stored pipeline)");
                ui.radio_value(&mut state.pipeline_mode, PipelineMode::Loaded, "Loaded (substitute)");
            });

            if state.pipeline_mode == PipelineMode::Loaded {
                ui.add_space(4.0);
                ui.horizontal(|ui| {
                    ui.label("Pipeline:");
                    if state.pipelines_loading && state.pipeline_ids.is_empty() {
                        ui.label(
                            egui::RichText::new("Loading...")
                                .italics()
                                .color(Theme::text_muted()),
                        );
                    } else {
                        crate::ui::widgets::filterable_select(
                            ui,
                            "online_pipeline_select",
                            &mut state.pipeline_id,
                            &mut state.pipeline_filter,
                            &state.pipeline_ids,
                            "(select pipeline)",
                        );
                    }
                    if ui.button("Load Definition").clicked() && !state.pipeline_id.is_empty() {
                        on_action.push(OnlineAction::LoadPipelineDef(
                            state.selected_cluster.clone(),
                            state.pipeline_id.clone(),
                        ));
                    }
                });
            }

            ui.add_space(6.0);
            let enabled = state.pipeline_mode == PipelineMode::Loaded;
            ui.add_enabled_ui(enabled, |ui| {
                ui.add(
                    egui::TextEdit::multiline(&mut state.pipeline_text)
                        .font(egui::TextStyle::Monospace)
                        .code_editor()
                        .hint_text("Pipeline definition JSON (used as a request-scoped substitution)")
                        .desired_rows(10)
                        .desired_width(ui.available_width())
                        .layouter(&mut |ui, text, wrap_width| {
                            crate::ui::widgets::json_layouter(ui, text, wrap_width)
                        }),
                );
            });
            if let Some(ref err) = state.pipeline_error {
                ui.add_space(4.0);
                ui.label(
                    egui::RichText::new(format!("Error: {}", err))
                        .color(Theme::danger())
                        .size(11.0),
                );
            }
        });
}

fn render_documents_column(
    ui: &mut Ui,
    state: &mut OnlinePipelineState,
    on_action: &mut Vec<OnlineAction>,
) {
    egui::Frame::new()
        .fill(Theme::bg_card())
        .corner_radius(Theme::CARD_ROUNDING)
        .inner_margin(Theme::CARD_PADDING)
        .show(ui, |ui| {
            ui.label(
                egui::RichText::new("Test Documents")
                    .strong()
                    .size(14.0)
                    .color(Theme::text_primary()),
            );
            ui.add_space(4.0);

            ui.horizontal(|ui| {
                if ui.button("📁 Load from File").clicked() {
                    on_action.push(OnlineAction::PickFile);
                }
                if ui.button("🔎 Load from Elastic").clicked() {
                    state.doc_modal_index = state.target_name.clone();
                    state.doc_modal_id.clear();
                    state.show_doc_modal = true;
                }
                if ui.button("🗑 Clear").clicked() {
                    state.docs_text = "[]".to_string();
                    state.docs_error = None;
                }
            });

            ui.add_space(4.0);
            ui.add(
                egui::TextEdit::multiline(&mut state.docs_text)
                    .font(egui::TextStyle::Monospace)
                    .code_editor()
                    .hint_text(r#"[ { "_source": { ... } } ]"#)
                    .desired_rows(14)
                    .desired_width(ui.available_width())
                    .layouter(&mut |ui, text, wrap_width| {
                        crate::ui::widgets::json_layouter(ui, text, wrap_width)
                    }),
            );

            if let Some(ref err) = state.docs_error {
                ui.add_space(4.0);
                ui.label(
                    egui::RichText::new(format!("Error: {}", err))
                        .color(Theme::danger())
                        .size(11.0),
                );
            }

            ui.add_space(12.0);

            let can_run = !state.is_loading
                && !state.selected_cluster.is_empty()
                && !state.target_name.is_empty();
            ui.add_enabled_ui(can_run, |ui| {
                let success_color = Theme::success();
                let btn = egui::Button::new(
                    egui::RichText::new(if state.is_loading {
                        "⏳ Running..."
                    } else {
                        "🔄 Run Simulation"
                    })
                    .color(Theme::contrast_text_color(success_color))
                    .strong(),
                )
                .fill(success_color);
                if ui
                    .add_sized([ui.available_width(), 32.0], btn)
                    .clicked()
                {
                    on_action.push(OnlineAction::Simulate {
                        cluster: state.selected_cluster.clone(),
                        target: state.target_name.clone(),
                        mode: state.pipeline_mode.clone(),
                        pipeline_id: state.pipeline_id.clone(),
                        pipeline_text: state.pipeline_text.clone(),
                        docs_text: state.docs_text.clone(),
                    });
                    state.is_loading = true;
                }
            });

            if can_run == false {
                let hint = if state.selected_cluster.is_empty() {
                    "Select a cluster to run."
                } else if state.target_name.is_empty() {
                    "Select a target index or data stream to run."
                } else {
                    ""
                };
                if !hint.is_empty() {
                    ui.label(
                        egui::RichText::new(hint)
                            .color(Theme::text_muted())
                            .size(11.0),
                    );
                }
            }
        });
}

fn render_result_column(ui: &mut Ui, state: &mut OnlinePipelineState) {
    egui::Frame::new()
        .fill(Theme::bg_card())
        .corner_radius(Theme::CARD_ROUNDING)
        .inner_margin(Theme::CARD_PADDING)
        .show(ui, |ui| {
            ui.label(
                egui::RichText::new("Pipeline Result")
                    .strong()
                    .size(14.0)
                    .color(Theme::text_primary()),
            );
            ui.add_space(4.0);
            egui::ScrollArea::vertical()
                .id_salt("online_pipeline_result_scroll")
                .show(ui, |ui| {
                    let mut text = state.result_text.clone();
                    let resp = ui.add(
                        egui::TextEdit::multiline(&mut text)
                            .font(egui::TextStyle::Monospace)
                            .code_editor()
                            .desired_rows(20)
                            .desired_width(ui.available_width())
                            .interactive(false)
                            .layouter(&mut |ui, t, wrap_width| {
                                crate::ui::widgets::json_layouter(ui, t, wrap_width)
                            }),
                    );
                    if resp.lost_focus() || resp.ctx.input(|i| i.key_pressed(egui::Key::Escape))
                    {
                        // No-op: keeps text in sync but doesn't capture keys.
                    }
                    state.result_text = text;
                });
        });
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::core::config::PipelineMode;

    #[test]
    fn build_body_default_mode() {
        let body = build_simulate_body(
            r#"[{"_source":{"a":1}}]"#,
            &PipelineMode::Default,
            "",
            "",
        )
        .unwrap();
        assert_eq!(
            body,
            json!({
                "docs": [{"_source":{"a":1}}]
            })
        );
        assert!(body.get("pipeline_substitutions").is_none());
    }

    #[test]
    fn build_body_loaded_mode_includes_substitution() {
        let body = build_simulate_body(
            r#"[{"_source":{"a":1}}]"#,
            &PipelineMode::Loaded,
            "my-pipeline",
            r#"{"description":"test","processors":[{"set":{"field":"x","value":1}}]}"#,
        )
        .unwrap();
        assert_eq!(
            body,
            json!({
                "docs": [{"_source":{"a":1}}],
                "pipeline_substitutions": {
                    "my-pipeline": {
                        "description": "test",
                        "processors": [{"set":{"field":"x","value":1}}]
                    }
                }
            })
        );
    }

    #[test]
    fn build_body_loaded_mode_requires_id() {
        let res = build_simulate_body(
            r#"[{"_source":{}}]"#,
            &PipelineMode::Loaded,
            "",
            "{}",
        );
        assert!(res.is_err());
    }

    #[test]
    fn build_body_invalid_docs() {
        let res = build_simulate_body("not json", &PipelineMode::Default, "", "");
        assert!(res.is_err());
    }

    #[test]
    fn append_file_docs_with_array() {
        let docs = append_file_docs("[]", r#"[{"a":1},{"a":2}]"#).unwrap();
        let v: Value = serde_json::from_str(&docs).unwrap();
        assert_eq!(v.as_array().unwrap().len(), 2);
    }

    #[test]
    fn append_file_docs_with_object() {
        let docs = append_file_docs("[]", r#"{"foo":"bar"}"#).unwrap();
        let v: Value = serde_json::from_str(&docs).unwrap();
        let arr = v.as_array().unwrap();
        assert_eq!(arr.len(), 1);
        assert_eq!(arr[0]["_source"]["foo"], "bar");
    }

    #[test]
    fn append_file_docs_with_plain_text() {
        let docs = append_file_docs("[]", "hello world").unwrap();
        let v: Value = serde_json::from_str(&docs).unwrap();
        let arr = v.as_array().unwrap();
        assert_eq!(arr[0]["_source"]["raw"], "hello world");
    }
}
