use std::collections::HashSet;

use egui::Ui;
use serde_json::Value;

use crate::modules::pipeline_engine::{
    ConvertType, ExecutionResult, Processor, ProcessorType, default_processor, execute_pipeline,
};
use crate::ui::theme::Theme;

#[derive(Debug, Clone, Default)]
pub struct PipelineState {
    pub processors: Vec<Processor>,
    pub document_text: String,
    pub new_processor_type: ProcessorType,
    pub run_result: Option<ExecutionResult>,
    pub input_error: Option<String>,
    #[allow(dead_code)]
    pub expanded_steps: HashSet<usize>,
    pub output_text: String,
}

impl PipelineState {
    pub fn with_defaults() -> Self {
        Self {
            processors: vec![
                Processor::Json {
                    id: "json-1".to_string(),
                    field: "payload".to_string(),
                    target_field: Some("payload".to_string()),
                },
                Processor::Remove {
                    id: "remove-1".to_string(),
                    fields: vec!["payload.remove_me".to_string()],
                },
                Processor::Convert {
                    id: "convert-1".to_string(),
                    field: "payload.status".to_string(),
                    target_field: Some("payload.status".to_string()),
                    convert_to: ConvertType::Integer,
                },
                Processor::Lowercase {
                    id: "lowercase-1".to_string(),
                    field: "payload.level".to_string(),
                    target_field: Some("payload.level".to_string()),
                },
                Processor::Trim {
                    id: "trim-1".to_string(),
                    field: "payload.message".to_string(),
                    target_field: Some("payload.message".to_string()),
                },
                Processor::Uppercase {
                    id: "uppercase-1".to_string(),
                    field: "payload.service".to_string(),
                    target_field: Some("payload.service".to_string()),
                },
                Processor::Set {
                    id: "set-1".to_string(),
                    field: "meta.source".to_string(),
                    value: Value::String("simulator".to_string()),
                },
                Processor::Reroute {
                    id: "reroute-1".to_string(),
                    dataset: "app".to_string(),
                },
            ],
            document_text: serde_json::to_string_pretty(&serde_json::json!({
                "payload": "{\"message\":\" hello \",\"remove_me\":\"temp\",\"status\":\"200\",\"level\":\"INFO\",\"service\":\"edge\"}"
            }))
            .unwrap_or_default(),
            output_text: String::new(),
            ..Default::default()
        }
    }
}

pub fn render_pipeline_module(ui: &mut Ui, state: &mut PipelineState) {
    ui.heading("Ingest Pipeline Simulator");
    ui.add_space(16.0);

    let available_width = ui.available_width();
    let remaining_height = ui.available_height();
    let left_width = available_width * 0.45;
    let right_width = available_width * 0.55 - 16.0;

    ui.allocate_ui_with_layout(
        egui::Vec2::new(available_width, remaining_height),
        egui::Layout::left_to_right(egui::Align::TOP),
        |ui| {
            ui.allocate_ui_with_layout(
                egui::Vec2::new(left_width, remaining_height),
                egui::Layout::top_down(egui::Align::Min),
                |ui| {
                    render_pipeline_builder(ui, state);
                },
            );
            ui.add_space(16.0);
            ui.allocate_ui_with_layout(
                egui::Vec2::new(right_width, remaining_height),
                egui::Layout::top_down(egui::Align::Min),
                |ui| {
                    render_right_panel(ui, state);
                },
            );
        },
    );
}

fn render_right_panel(ui: &mut Ui, state: &mut PipelineState) {
    egui::Frame::new()
        .fill(Theme::bg_card())
        .corner_radius(Theme::CARD_ROUNDING)
        .inner_margin(Theme::CARD_PADDING)
        .show(ui, |ui| {
            ui.label(
                egui::RichText::new("Input Document")
                    .strong()
                    .size(13.0)
                    .color(Theme::text_primary()),
            );
            ui.add_space(4.0);

            ui.add(
                egui::TextEdit::multiline(&mut state.document_text)
                    .font(egui::TextStyle::Monospace)
                    .code_editor()
                    .desired_rows(5),
            );

            if let Some(ref err) = state.input_error {
                ui.add_space(4.0);
                ui.label(
                    egui::RichText::new(format!("Error: {}", err))
                        .color(Theme::danger())
                        .size(12.0),
                );
            }

            ui.add_space(8.0);
            ui.horizontal(|ui| {
                if ui.button("🔄 Run simulation").clicked() {
                    run_simulation(state);
                }
            });
            ui.add_space(12.0);

            ui.label(
                egui::RichText::new("Pipeline Output")
                    .strong()
                    .size(13.0)
                    .color(Theme::text_primary()),
            );
            ui.add_space(4.0);

            egui::ScrollArea::vertical()
                .id_salt("pipeline_output")
                .show(ui, |ui| {
                    let mut output = state.output_text.clone();
                    ui.add(
                        egui::TextEdit::multiline(&mut output)
                            .font(egui::TextStyle::Monospace)
                            .code_editor()
                            .desired_rows(10)
                            .interactive(false),
                    );
                    state.output_text = output;
                });
        });
}

fn render_pipeline_builder(ui: &mut Ui, state: &mut PipelineState) {
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
            ui.add_space(8.0);

            ui.horizontal(|ui| {
                egui::ComboBox::from_id_salt("new_processor_type")
                    .selected_text(state.new_processor_type.as_str())
                    .show_ui(ui, |ui| {
                        for ptype in ProcessorType::ALL {
                            ui.selectable_value(&mut state.new_processor_type, *ptype, ptype.as_str());
                        }
                    });

                if ui.button("Add processor").clicked() {
                    state.processors.push(default_processor(state.new_processor_type));
                }
            });

            ui.add_space(8.0);

            ui.allocate_ui_with_layout(
                egui::Vec2::new(ui.available_width(), ui.available_height()),
                egui::Layout::top_down(egui::Align::Min),
                |ui| {
                    let max_h = ui.available_height();
                    egui::ScrollArea::vertical()
                        .id_salt("pipeline_processors")
                        .max_height(max_h)
                        .show(ui, |ui| {
                            let mut remove_idx = None;
                            let mut move_dir = None;
                            let proc_len = state.processors.len();

                            for (index, processor) in state.processors.iter_mut().enumerate() {
                                render_processor_card(
                                    ui,
                                    index,
                                    processor,
                                    proc_len,
                                    &mut remove_idx,
                                    &mut move_dir,
                                );
                            }

                            if let Some(idx) = remove_idx {
                                state.processors.remove(idx);
                                state.output_text.clear();
                            }
                            if let Some((idx, dir)) = move_dir {
                                let new_idx = (idx as isize + dir) as usize;
                                if new_idx < state.processors.len() {
                                    state.processors.swap(idx, new_idx);
                                    state.output_text.clear();
                                }
                            }
                        });
                });
        });
}

fn render_processor_card(
    ui: &mut Ui,
    index: usize,
    processor: &mut Processor,
    proc_len: usize,
    remove_idx: &mut Option<usize>,
    move_dir: &mut Option<(usize, isize)>,
) {
    egui::Frame::new()
        .fill(Theme::bg_input())
        .corner_radius(egui::CornerRadius::same(6))
        .inner_margin(egui::Vec2::new(10.0, 8.0))
        .show(ui, |ui| {
            ui.set_width(ui.available_width());

            // Header
            ui.horizontal(|ui| {
                ui.label(
                    egui::RichText::new(format!(
                        "{}. {}",
                        index + 1,
                        processor.processor_type().as_str()
                    ))
                    .strong()
                    .size(12.0)
                    .color(Theme::text_primary()),
                );

                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    if ui
                        .add(
                            egui::Label::new(
                                egui::RichText::new("×").size(14.0).color(Theme::danger()),
                            )
                            .sense(egui::Sense::click()),
                        )
                        .clicked()
                    {
                        *remove_idx = Some(index);
                    }

                    if ui
                        .add_enabled(
                            index < proc_len - 1,
                            egui::Label::new(
                                egui::RichText::new("↓")
                                    .size(12.0)
                                    .color(Theme::text_secondary()),
                            )
                            .sense(egui::Sense::click()),
                        )
                        .clicked()
                    {
                        *move_dir = Some((index, 1));
                    }

                    if ui
                        .add_enabled(
                            index > 0,
                            egui::Label::new(
                                egui::RichText::new("↑")
                                    .size(12.0)
                                    .color(Theme::text_secondary()),
                            )
                            .sense(egui::Sense::click()),
                        )
                        .clicked()
                    {
                        *move_dir = Some((index, -1));
                    }
                });
            });

            ui.add_space(4.0);

            // Type-specific fields
            match processor {
                Processor::Set { field, value, .. } => {
                    ui.horizontal(|ui| {
                        ui.label("Field:");
                        ui.text_edit_singleline(field);
                    });
                    ui.horizontal(|ui| {
                        ui.label("Value:");
                        let mut raw = match value {
                            Value::String(s) => s.clone(),
                            _ => value.to_string(),
                        };
                        if ui.text_edit_singleline(&mut raw).changed() {
                            *value = raw.trim().parse::<Value>().unwrap_or(Value::String(raw));
                        }
                    });
                }
                Processor::Remove { fields, .. } => {
                    let mut raw = fields.join(", ");
                    ui.horizontal(|ui| {
                        ui.label("Fields:");
                        if ui.text_edit_singleline(&mut raw).changed() {
                            *fields = raw
                                .split(',')
                                .map(|s| s.trim().to_string())
                                .filter(|s| !s.is_empty())
                                .collect();
                        }
                    });
                }
                Processor::Json {
                    field,
                    target_field,
                    ..
                } => {
                    ui.horizontal(|ui| {
                        ui.label("Source:");
                        ui.text_edit_singleline(field);
                    });
                    ui.horizontal(|ui| {
                        ui.label("Target:");
                        let mut tf = target_field.clone().unwrap_or_default();
                        if ui.text_edit_singleline(&mut tf).changed() {
                            *target_field = if tf.is_empty() { None } else { Some(tf) };
                        }
                    });
                }
                Processor::Reroute { dataset, .. } => {
                    ui.horizontal(|ui| {
                        ui.label("Dataset:");
                        ui.text_edit_singleline(dataset);
                    });
                }
                Processor::Convert {
                    field,
                    target_field,
                    convert_to,
                    ..
                } => {
                    ui.horizontal(|ui| {
                        ui.label("Source:");
                        ui.text_edit_singleline(field);
                    });
                    ui.horizontal(|ui| {
                        ui.label("Target:");
                        let mut tf = target_field.clone().unwrap_or_default();
                        if ui.text_edit_singleline(&mut tf).changed() {
                            *target_field = if tf.is_empty() { None } else { Some(tf) };
                        }
                    });
                    ui.horizontal(|ui| {
                        ui.label("Convert to:");
                        egui::ComboBox::from_id_salt(format!("convert-{}-type", index))
                            .selected_text(convert_to.as_str())
                            .show_ui(ui, |ui| {
                                for ct in ConvertType::ALL {
                                    ui.selectable_value(convert_to, *ct, ct.as_str());
                                }
                            });
                    });
                }
                Processor::Lowercase {
                    field,
                    target_field,
                    ..
                }
                | Processor::Uppercase {
                    field,
                    target_field,
                    ..
                }
                | Processor::Trim {
                    field,
                    target_field,
                    ..
                } => {
                    ui.horizontal(|ui| {
                        ui.label("Source:");
                        ui.text_edit_singleline(field);
                    });
                    ui.horizontal(|ui| {
                        ui.label("Target:");
                        let mut tf = target_field.clone().unwrap_or_default();
                        if ui.text_edit_singleline(&mut tf).changed() {
                            *target_field = if tf.is_empty() { None } else { Some(tf) };
                        }
                    });
                }
            }
        });

    ui.add_space(4.0);
}

fn run_simulation(state: &mut PipelineState) {
    let parsed: Value = match serde_json::from_str(&state.document_text) {
        Ok(v) => v,
        Err(e) => {
            state.input_error = Some(format!("invalid JSON: {}", e));
            state.output_text = format!("Error: {}", e);
            return;
        }
    };

    if !parsed.is_object() {
        state.input_error = Some("document must be a JSON object".to_string());
        state.output_text = String::from("Error: document must be a JSON object");
        return;
    }

    state.input_error = None;
    let result = execute_pipeline(&parsed, &state.processors);

    match &result.error {
        Some(err) => {
            state.output_text = format!("Pipeline error: {}\n\nFinal document:\n{}",
                err,
                serde_json::to_string_pretty(&result.final_document).unwrap_or_default()
            );
        }
        None => {
            state.output_text = serde_json::to_string_pretty(&result.final_document)
                .unwrap_or_default();
        }
    }

    state.run_result = Some(result);
}
