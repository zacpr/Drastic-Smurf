use egui::{Color32, Stroke, Ui};
use serde_json::Value;
use std::collections::HashSet;

use crate::ui::theme::Theme;

#[derive(Debug, Clone)]
pub struct DiscoverState {
    pub selected_cluster: String,
    pub index_pattern: String,
    pub search_query: String,
    pub is_loading: bool,
    pub error: Option<String>,
    pub results: Vec<Value>,
    pub expanded_doc_id: Option<String>,
    pub available_fields: Vec<String>,
    pub selected_fields: Vec<String>,
}

impl Default for DiscoverState {
    fn default() -> Self {
        Self {
            selected_cluster: String::new(),
            index_pattern: "*".to_string(),
            search_query: "".to_string(),
            is_loading: false,
            error: None,
            results: Vec::new(),
            expanded_doc_id: None,
            available_fields: Vec::new(),
            selected_fields: vec!["_source".to_string()],
        }
    }
}

impl DiscoverState {
    /// Extracts all unique dot-separated field paths recursively from a JSON value.
    fn extract_fields(val: &Value, prefix: &str, fields: &mut HashSet<String>) {
        match val {
            Value::Object(map) => {
                for (k, v) in map {
                    let full_path = if prefix.is_empty() {
                        k.clone()
                    } else {
                        format!("{}.{}", prefix, k)
                    };
                    // Only add leaf fields or simple primitives
                    match v {
                        Value::Object(_) => {
                            Self::extract_fields(v, &full_path, fields);
                        }
                        Value::Array(arr) => {
                            if arr.iter().all(|item| !item.is_object() && !item.is_array()) {
                                fields.insert(full_path.clone());
                            } else {
                                for item in arr {
                                    Self::extract_fields(item, &full_path, fields);
                                }
                            }
                        }
                        _ => {
                            fields.insert(full_path);
                        }
                    }
                }
            }
            _ => {}
        }
    }

    /// Refresh the list of available fields based on current results.
    pub fn refresh_fields(&mut self) {
        let mut field_set = HashSet::new();
        for hit in &self.results {
            if let Some(source) = hit.get("_source") {
                Self::extract_fields(source, "", &mut field_set);
            }
        }
        let mut sorted_fields: Vec<String> = field_set.into_iter().collect();
        sorted_fields.sort();
        self.available_fields = sorted_fields;
    }
}

/// Dynamic JSON path lookup helper.
fn get_json_path(value: &Value, path: &str) -> Option<Value> {
    if path == "_source" {
        return Some(value.clone());
    }
    let mut current = value;
    for part in path.split('.') {
        current = current.get(part)?;
    }
    Some(current.clone())
}

pub fn render_discover_module(
    ui: &mut Ui,
    state: &mut DiscoverState,
    cluster_names: &[String],
    on_search_triggered: &mut Option<(String, String)>, // (method, path, body) target return
) {
    ui.heading("Discover");
    ui.add_space(8.0);

    // Initial cluster selection if empty
    if state.selected_cluster.is_empty() && !cluster_names.is_empty() {
        state.selected_cluster = cluster_names[0].clone();
    }

    // Top control bar card
    egui::Frame::new()
        .fill(Theme::bg_card())
        .corner_radius(Theme::CARD_ROUNDING)
        .inner_margin(Theme::CARD_PADDING)
        .show(ui, |ui| {
            ui.horizontal(|ui| {
                // Cluster selector
                ui.label("Cluster:");
                egui::ComboBox::from_id_salt("discover_cluster")
                    .selected_text(&state.selected_cluster)
                    .show_ui(ui, |ui| {
                        for name in cluster_names {
                            ui.selectable_value(&mut state.selected_cluster, name.clone(), name);
                        }
                    });

                ui.add_space(8.0);

                // Index Pattern
                ui.label("Index Pattern:");
                ui.add(
                    egui::TextEdit::singleline(&mut state.index_pattern)
                        .hint_text("e.g. logstash-*")
                        .desired_width(140.0),
                );

                ui.add_space(8.0);

                // Search query
                ui.label("Search:");
                let search_input = ui.add(
                    egui::TextEdit::singleline(&mut state.search_query)
                        .hint_text("e.g. status:500 OR level:error")
                        .desired_width(320.0),
                );

                // Clear button inside search query space
                if !state.search_query.is_empty() {
                    if ui.small_button("Clear").clicked() {
                        state.search_query.clear();
                    }
                }

                ui.add_space(8.0);

                // Search Button with loading indicator
                let search_btn_text = if state.is_loading { "Searching..." } else { "🔍 Search" };
                let btn = ui.add_enabled(
                    !state.is_loading && !state.selected_cluster.is_empty(),
                    egui::Button::new(
                        egui::RichText::new(search_btn_text)
                            .color(Color32::WHITE)
                            .strong(),
                    )
                    .fill(Theme::accent()),
                );

                // Trigger query on Enter or Search click
                if (btn.clicked() || (search_input.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter))))
                    && !state.selected_cluster.is_empty()
                {
                    state.is_loading = true;
                    state.error = None;
                    state.results.clear();

                    // Formulate query body
                    let body = if state.search_query.trim().is_empty() {
                        serde_json::json!({
                            "size": 50,
                            "sort": [
                                { "@timestamp": { "order": "desc", "unmapped_type": "date" } }
                            ]
                        })
                    } else {
                        serde_json::json!({
                            "size": 50,
                            "sort": [
                                { "@timestamp": { "order": "desc", "unmapped_type": "date" } }
                            ],
                            "query": {
                                "query_string": {
                                    "query": state.search_query
                                }
                            }
                        })
                    };

                    let body_str = serde_json::to_string_pretty(&body).unwrap_or_default();
                    let path = format!("/{}/_search", state.index_pattern);

                    *on_search_triggered = Some((path, body_str));
                }
            });
        });

    ui.add_space(12.0);

    // Error alert banner
    if let Some(err) = &state.error {
        egui::Frame::new()
            .fill(Theme::danger())
            .corner_radius(Theme::CARD_ROUNDING)
            .inner_margin(Theme::CARD_PADDING)
            .show(ui, |ui| {
                ui.horizontal(|ui| {
                    ui.label(egui::RichText::new("⚠️").size(16.0).color(Color32::WHITE));
                    ui.label(
                        egui::RichText::new(format!("Search Error: {}", err))
                            .color(Color32::WHITE)
                            .strong(),
                    );
                });
            });
        ui.add_space(12.0);
    }

    // Split workspace panel
    ui.columns(2, |columns| {
        // --- COLUMN 0: Available Fields Sidebar ---
        let ui_sidebar = &mut columns[0];
        ui_sidebar.set_max_width(240.0);

        egui::Frame::new()
            .fill(Theme::bg_card())
            .corner_radius(Theme::CARD_ROUNDING)
            .inner_margin(Theme::CARD_PADDING)
            .show(ui_sidebar, |ui| {
                ui.heading("Available Fields");
                ui.add_space(6.0);
                ui.label(
                    egui::RichText::new("Select fields to build columns:")
                        .size(11.0)
                        .color(Theme::text_muted()),
                );
                ui.add_space(8.0);

                egui::ScrollArea::vertical().id_salt("fields_scroll").show(ui, |ui| {
                    if state.available_fields.is_empty() {
                        ui.label(
                            egui::RichText::new("No fields loaded.\nRun a search to analyze index mapping.")
                                .color(Theme::text_muted())
                                .italics(),
                        );
                    } else {
                        // Dynamic fields toggle list
                        for field in &state.available_fields {
                            let is_selected = state.selected_fields.contains(field);
                            ui.horizontal(|ui| {
                                if is_selected {
                                    if ui.button(egui::RichText::new("❌").color(Theme::danger())).on_hover_text("Remove column").clicked() {
                                        state.selected_fields.retain(|x| x != field);
                                        // Ensure at least _source is left if empty
                                        if state.selected_fields.is_empty() {
                                            state.selected_fields.push("_source".to_string());
                                        }
                                    }
                                    ui.label(
                                        egui::RichText::new(field)
                                            .color(Theme::accent())
                                            .strong(),
                                    );
                                } else {
                                    if ui.button(egui::RichText::new("➕").color(Theme::success())).on_hover_text("Add column").clicked() {
                                        // Remove _source placeholder if adding specific field columns
                                        state.selected_fields.retain(|x| x != "_source");
                                        state.selected_fields.push(field.clone());
                                    }
                                    ui.label(
                                        egui::RichText::new(field)
                                            .color(Theme::text_secondary()),
                                    );
                                }
                            });
                        }
                    }
                });
            });

        // --- COLUMN 1: Search Results Grid ---
        let ui_results = &mut columns[1];

        egui::Frame::new()
            .fill(Theme::bg_card())
            .corner_radius(Theme::CARD_ROUNDING)
            .inner_margin(Theme::CARD_PADDING)
            .show(ui_results, |ui| {
                ui.horizontal(|ui| {
                    ui.heading("Results");
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        ui.label(
                            egui::RichText::new(format!("{} documents", state.results.len()))
                                .color(Theme::text_muted()),
                        );
                    });
                });
                ui.add_space(8.0);

                if state.results.is_empty() {
                    ui.add_space(32.0);
                    ui.vertical_centered(|ui| {
                        ui.label(egui::RichText::new("🔍").size(48.0));
                        ui.add_space(12.0);
                        ui.label(
                            egui::RichText::new("No documents to show.")
                                .size(14.0)
                                .strong(),
                        );
                        ui.label(
                            egui::RichText::new(
                                "Specify an index pattern or query above and hit Search.",
                            )
                            .color(Theme::text_muted()),
                        );
                    });
                    ui.add_space(32.0);
                } else {
                    // Render results table with scrolling
                    egui::ScrollArea::vertical().id_salt("results_scroll").show(ui, |ui| {
                        // Draw header row
                        ui.horizontal(|ui| {
                            ui.set_width(ui.available_width());
                            // Space for expand arrow
                            ui.add_space(20.0);

                            // Timestamp column
                            ui.allocate_ui(egui::Vec2::new(140.0, 18.0), |ui| {
                                ui.label(
                                    egui::RichText::new("Time")
                                        .strong()
                                        .color(Theme::accent()),
                                );
                            });

                            // Custom fields columns
                            for col in &state.selected_fields {
                                ui.label(
                                    egui::RichText::new(col)
                                        .strong()
                                        .color(Theme::accent()),
                                );
                            }
                        });

                        ui.separator();

                        // Draw hits
                        for hit in &state.results {
                            let doc_id = hit.get("_id").and_then(|id| id.as_str()).unwrap_or("unknown").to_string();
                            let is_expanded = state.expanded_doc_id.as_ref() == Some(&doc_id);

                            let source = hit.get("_source").unwrap_or(&Value::Null);

                            // Extract time
                            let timestamp = source.get("@timestamp")
                                .and_then(|t| t.as_str())
                                .unwrap_or("N/A");

                            ui.horizontal(|ui| {
                                ui.set_width(ui.available_width());

                                // Expand/Collapse toggle button
                                let toggle_symbol = if is_expanded { "▼" } else { "▶" };
                                if ui.small_button(toggle_symbol).clicked() {
                                    if is_expanded {
                                        state.expanded_doc_id = None;
                                    } else {
                                        state.expanded_doc_id = Some(doc_id.clone());
                                    }
                                }

                                // Render time
                                ui.allocate_ui(egui::Vec2::new(140.0, 18.0), |ui| {
                                    ui.label(
                                        egui::RichText::new(timestamp)
                                            .color(Theme::text_secondary())
                                            .size(11.0),
                                    );
                                });

                                // Render other columns
                                for col in &state.selected_fields {
                                    let cell_val = get_json_path(source, col)
                                        .unwrap_or(Value::Null);

                                    let cell_text = match &cell_val {
                                        Value::Null => "-".to_string(),
                                        Value::String(s) => s.clone(),
                                        Value::Number(n) => n.to_string(),
                                        Value::Bool(b) => b.to_string(),
                                        other => other.to_string(),
                                    };

                                    // Display value with clean truncation if too long
                                    let truncated = if cell_text.len() > 60 {
                                        format!("{}...", &cell_text[..57])
                                    } else {
                                        cell_text
                                    };

                                    ui.label(
                                        egui::RichText::new(truncated)
                                            .color(Theme::text_primary())
                                            .size(11.0),
                                    );
                                }
                            });

                            // Expanded detail drawer
                            if is_expanded {
                                ui.indent(doc_id.clone(), |ui| {
                                    egui::Frame::new()
                                        .fill(Theme::bg_input())
                                        .corner_radius(Theme::CARD_ROUNDING)
                                        .inner_margin(Theme::CARD_PADDING)
                                        .stroke(Stroke::new(1.0, Theme::border()))
                                        .show(ui, |ui| {
                                            ui.horizontal(|ui| {
                                                ui.label(
                                                    egui::RichText::new(format!("Document Details (ID: {})", doc_id))
                                                        .strong()
                                                        .color(Theme::accent()),
                                                );
                                                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                                                    if ui.small_button("📋 Copy JSON").clicked() {
                                                        let json_pretty = serde_json::to_string_pretty(&source).unwrap_or_default();
                                                        ui.ctx().copy_text(json_pretty);
                                                    }
                                                });
                                            });
                                            ui.add_space(4.0);

                                            // Formatted pretty json output
                                            let json_pretty = serde_json::to_string_pretty(&source).unwrap_or_default();
                                            ui.label(
                                                egui::RichText::new(json_pretty)
                                                    .code()
                                                    .color(Theme::text_primary())
                                                    .size(11.0),
                                            );
                                        });
                                });
                            }

                            ui.separator();
                        }
                    });
                }
            });
    });
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn test_get_json_path() {
        let val = json!({
            "a": {
                "b": {
                    "c": "hello"
                }
            }
        });
        assert_eq!(get_json_path(&val, "a.b.c").unwrap(), Value::String("hello".to_string()));
        assert_eq!(get_json_path(&val, "a.b.nonexistent"), None);
    }

    #[test]
    fn test_extract_fields() {
        let mut state = DiscoverState::default();
        state.results = vec![
            json!({
                "_id": "1",
                "_source": {
                    "@timestamp": "2026-05-26",
                    "level": "error",
                    "nested": {
                        "field": 42,
                        "array": [1, 2, 3]
                    }
                }
            })
        ];
        state.refresh_fields();
        assert!(state.available_fields.contains(&"@timestamp".to_string()));
        assert!(state.available_fields.contains(&"level".to_string()));
        assert!(state.available_fields.contains(&"nested.field".to_string()));
        assert!(state.available_fields.contains(&"nested.array".to_string()));
    }
}
