use egui::{Color32, Stroke, Ui};
use serde_json::Value;
use std::collections::HashSet;

use crate::ui::theme::Theme;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TimeframePreset {
    Last30m,
    Last1h,
    Last6h,
    Last12h,
    Last24h,
    Last72h,
    Custom,
}

impl TimeframePreset {
    pub fn label(&self) -> &'static str {
        match self {
            Self::Last30m => "🕒 Last 30m",
            Self::Last1h => "🕒 Last 1h",
            Self::Last6h => "🕒 Last 6h",
            Self::Last12h => "🕒 Last 12h",
            Self::Last24h => "🕒 Last day",
            Self::Last72h => "🕒 Last 72h",
            Self::Custom => "🕒 Custom Range",
        }
    }
}

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

    // Timeframe Selector State
    pub time_preset: TimeframePreset,
    pub custom_from_year: i32,
    pub custom_from_month: u32,
    pub custom_from_day: u32,
    pub custom_from_hour: u32,
    pub custom_from_minute: u32,
    pub custom_from_second: u32,
    pub custom_to_year: i32,
    pub custom_to_month: u32,
    pub custom_to_day: u32,
    pub custom_to_hour: u32,
    pub custom_to_minute: u32,
    pub custom_to_second: u32,
    pub show_time_selector_popup: bool,
}

impl Default for DiscoverState {
    fn default() -> Self {
        Self {
            selected_cluster: String::new(),
            index_pattern: "logs-elasticsearch*".to_string(),
            search_query: "".to_string(),
            is_loading: false,
            error: None,
            results: Vec::new(),
            expanded_doc_id: None,
            available_fields: Vec::new(),
            selected_fields: vec!["_source".to_string()],

            // Default custom time to a modern reference date
            time_preset: TimeframePreset::Last1h,
            custom_from_year: 2026,
            custom_from_month: 5,
            custom_from_day: 26,
            custom_from_hour: 0,
            custom_from_minute: 0,
            custom_from_second: 0,
            custom_to_year: 2026,
            custom_to_month: 5,
            custom_to_day: 26,
            custom_to_hour: 23,
            custom_to_minute: 59,
            custom_to_second: 59,
            show_time_selector_popup: false,
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

/// Pure math helper to calculate days in month
fn days_in_month(year: i32, month: u32) -> u32 {
    match month {
        1 | 3 | 5 | 7 | 8 | 10 | 12 => 31,
        4 | 6 | 9 | 11 => 30,
        2 => {
            if (year % 4 == 0 && year % 100 != 0) || (year % 400 == 0) {
                29
            } else {
                28
            }
        }
        _ => 30,
    }
}

/// Pure math helper to calculate day of week for 1st of month using Zeller's Congruence.
/// Returns Sunday = 0, Monday = 1, ..., Saturday = 6.
fn day_of_week_first_of_month(year: i32, month: u32) -> u32 {
    let mut y = year;
    let mut m = month;
    if m < 3 {
        m += 12;
        y -= 1;
    }
    let k = y % 100;
    let j = y / 100;
    
    // Zeller's formula for positive inputs
    let h = (1 + (13 * (m as i32 + 1)) / 5 + k + k / 4 + j / 4 + 5 * j) % 7;
    
    // Zeller: 0 = Saturday, 1 = Sunday, ..., 6 = Friday
    match h {
        0 => 6, // Saturday
        1 => 0, // Sunday
        2 => 1, // Monday
        3 => 2, // Tuesday
        4 => 3, // Wednesday
        5 => 4, // Thursday
        6 => 5, // Friday
        _ => 0,
    }
}

/// Beautifully renders a pure stateless date picker grid
fn draw_calendar_picker(
    ui: &mut Ui,
    year: &mut i32,
    month: &mut u32,
    day: &mut u32,
    id_salt: &'static str,
) {
    ui.vertical(|ui| {
        // Month/Year navigation row
        ui.horizontal(|ui| {
            if ui.button("◀").clicked() {
                if *month == 1 {
                    *month = 12;
                    *year -= 1;
                } else {
                    *month -= 1;
                }
            }
            
            let month_name = match *month {
                1 => "Jan", 2 => "Feb", 3 => "Mar", 4 => "Apr",
                5 => "May", 6 => "Jun", 7 => "Jul", 8 => "Aug",
                9 => "Sep", 10 => "Oct", 11 => "Nov", 12 => "Dec",
                _ => "Month"
            };
            ui.allocate_ui(egui::Vec2::new(76.0, 18.0), |ui| {
                ui.vertical_centered(|ui| {
                    ui.label(egui::RichText::new(format!("{} {}", month_name, year)).strong());
                });
            });

            if ui.button("▶").clicked() {
                if *month == 12 {
                    *month = 1;
                    *year += 1;
                } else {
                    *month += 1;
                }
            }
        });

        ui.add_space(4.0);

        // Day of week headers
        ui.horizontal(|ui| {
            let days_headers = ["Su", "Mo", "Tu", "We", "Th", "Fr", "Sa"];
            for header in &days_headers {
                ui.allocate_ui(egui::Vec2::new(20.0, 14.0), |ui| {
                    ui.vertical_centered(|ui| {
                        ui.label(egui::RichText::new(*header).size(10.0).color(Theme::text_muted()));
                    });
                });
            }
        });

        let total_days = days_in_month(*year, *month);
        let first_day_weekday = day_of_week_first_of_month(*year, *month);

        // 6 rows, 7 columns grid
        let mut current_day = 1;
        egui::Grid::new(id_salt)
            .spacing([2.0, 2.0])
            .show(ui, |ui| {
                for r in 0..6 {
                    for c in 0..7 {
                        let cell_idx = r * 7 + c;
                        if cell_idx < first_day_weekday as i32 || current_day > total_days {
                            ui.allocate_ui(egui::Vec2::new(20.0, 20.0), |ui| {
                                ui.label("");
                            });
                        } else {
                            let is_selected = *day == current_day;
                            let btn_text = current_day.to_string();
                            
                            let mut btn = egui::Button::new(egui::RichText::new(btn_text).size(10.0));
                            if is_selected {
                                btn = btn.fill(Theme::accent());
                            } else {
                                btn = btn.fill(Color32::TRANSPARENT);
                            }

                            ui.allocate_ui(egui::Vec2::new(20.0, 20.0), |ui| {
                                if ui.add(btn).clicked() {
                                    *day = current_day;
                                }
                            });
                            current_day += 1;
                        }
                    }
                    ui.end_row();
                }
            });
    });
}

pub fn render_discover_module(
    ui: &mut Ui,
    state: &mut DiscoverState,
    cluster_names: &[String],
    on_search_triggered: &mut Option<(String, String)>,
) {
    ui.heading("Discover");
    ui.add_space(8.0);

    // Initial cluster selection if empty
    if state.selected_cluster.is_empty() && !cluster_names.is_empty() {
        state.selected_cluster = cluster_names[0].clone();
    }

    let mut trigger_search = false;

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

                ui.add_space(4.0);

                // Index Pattern
                ui.label("Index:");
                ui.add(
                    egui::TextEdit::singleline(&mut state.index_pattern)
                        .hint_text("logs-elasticsearch*")
                        .desired_width(140.0),
                );

                ui.add_space(4.0);

                // Time selector button
                let selector_label = if state.time_preset == TimeframePreset::Custom {
                    format!(
                        "🕒 Custom ({:02}/{:02} to {:02}/{:02})",
                        state.custom_from_month, state.custom_from_day,
                        state.custom_to_month, state.custom_to_day
                    )
                } else {
                    state.time_preset.label().to_string()
                };

                let time_btn = ui.button(egui::RichText::new(selector_label).strong());
                if time_btn.clicked() {
                    state.show_time_selector_popup = !state.show_time_selector_popup;
                }

                ui.add_space(4.0);

                // Search query
                ui.label("Search:");
                let search_input = ui.add(
                    egui::TextEdit::singleline(&mut state.search_query)
                        .hint_text("status:500 OR level:error")
                        .desired_width(180.0),
                );

                // Clear button inside search query space
                if !state.search_query.is_empty() {
                    if ui.small_button("Clear").clicked() {
                        state.search_query.clear();
                    }
                }

                ui.add_space(4.0);

                // Search Button with loading indicator
                let search_btn_text = if state.is_loading { "Searching..." } else { "🔍 Search" };
                let accent_color = Theme::accent();
                let btn = ui.add_enabled(
                    !state.is_loading && !state.selected_cluster.is_empty(),
                    egui::Button::new(
                        egui::RichText::new(search_btn_text)
                            .color(Theme::contrast_text_color(accent_color))
                            .strong(),
                    )
                    .fill(accent_color),
                );

                // Trigger query on Enter or Search click
                if (btn.clicked() || (search_input.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter))))
                    && !state.selected_cluster.is_empty()
                {
                    trigger_search = true;
                }
            });
        });

    ui.add_space(4.0);

    // Timeframe selector drawer (slides down if open)
    if state.show_time_selector_popup {
        egui::Frame::new()
            .fill(Theme::bg_input())
            .corner_radius(Theme::CARD_ROUNDING)
            .inner_margin(Theme::CARD_PADDING)
            .stroke(Stroke::new(1.0, Theme::accent()))
            .show(ui, |ui| {
                ui.horizontal_top(|ui| {
                    // Left Column: Presets
                    ui.vertical(|ui| {
                        ui.label(egui::RichText::new("Quick Presets").strong().color(Theme::accent()));
                        ui.add_space(6.0);
                        
                        let presets = [
                            TimeframePreset::Last30m,
                            TimeframePreset::Last1h,
                            TimeframePreset::Last6h,
                            TimeframePreset::Last12h,
                            TimeframePreset::Last24h,
                            TimeframePreset::Last72h,
                        ];

                        for p in presets {
                            let is_active = state.time_preset == p;
                            let mut btn = egui::Button::new(p.label());
                            if is_active {
                                btn = btn.fill(Theme::accent());
                            }
                            if ui.add_sized([120.0, 20.0], btn).clicked() {
                                state.time_preset = p;
                                state.show_time_selector_popup = false;
                                trigger_search = true;
                            }
                            ui.add_space(4.0);
                        }
                    });

                    ui.add_space(16.0);
                    ui.separator();
                    ui.add_space(16.0);

                    // Middle Column: From Date/Time
                    ui.vertical(|ui| {
                        ui.label(egui::RichText::new("From Date & Time").strong().color(Theme::accent()));
                        ui.add_space(6.0);

                        draw_calendar_picker(
                            ui,
                            &mut state.custom_from_year,
                            &mut state.custom_from_month,
                            &mut state.custom_from_day,
                            "calendar_from",
                        );

                        ui.add_space(6.0);
                        ui.horizontal(|ui| {
                            ui.label("Time:");
                            ui.add(egui::DragValue::new(&mut state.custom_from_hour).range(0..=23).suffix("h"));
                            ui.label(":");
                            ui.add(egui::DragValue::new(&mut state.custom_from_minute).range(0..=59).suffix("m"));
                            ui.label(":");
                            ui.add(egui::DragValue::new(&mut state.custom_from_second).range(0..=59).suffix("s"));
                        });
                    });

                    ui.add_space(16.0);
                    ui.separator();
                    ui.add_space(16.0);

                    // Right Column: To Date/Time
                    ui.vertical(|ui| {
                        ui.label(egui::RichText::new("To Date & Time").strong().color(Theme::accent()));
                        ui.add_space(6.0);

                        draw_calendar_picker(
                            ui,
                            &mut state.custom_to_year,
                            &mut state.custom_to_month,
                            &mut state.custom_to_day,
                            "calendar_to",
                        );

                        ui.add_space(6.0);
                        ui.horizontal(|ui| {
                            ui.label("Time:");
                            ui.add(egui::DragValue::new(&mut state.custom_to_hour).range(0..=23).suffix("h"));
                            ui.label(":");
                            ui.add(egui::DragValue::new(&mut state.custom_to_minute).range(0..=59).suffix("m"));
                            ui.label(":");
                            ui.add(egui::DragValue::new(&mut state.custom_to_second).range(0..=59).suffix("s"));
                        });
                    });
                });

                ui.add_space(12.0);
                ui.separator();
                ui.add_space(8.0);

                // Footer Apply / Cancel buttons
                ui.horizontal(|ui| {
                    let success_color = Theme::success();
                    let apply_btn = egui::Button::new(
                        egui::RichText::new("Apply Custom Range")
                            .color(Theme::contrast_text_color(success_color))
                            .strong()
                    ).fill(success_color);
                    
                    if ui.add(apply_btn).clicked() {
                        state.time_preset = TimeframePreset::Custom;
                        state.show_time_selector_popup = false;
                        trigger_search = true;
                    }

                    if ui.button("Cancel").clicked() {
                        state.show_time_selector_popup = false;
                    }
                });
            });
        ui.add_space(8.0);
    }

    // Execute Search if triggered
    if trigger_search && !state.selected_cluster.is_empty() {
        state.is_loading = true;
        state.error = None;
        state.results.clear();

        // Formulate query body
        let range_filter = match state.time_preset {
            TimeframePreset::Last30m => {
                Some(serde_json::json!({
                    "@timestamp": {
                        "gte": "now-30m",
                        "lte": "now"
                    }
                }))
            }
            TimeframePreset::Last1h => {
                Some(serde_json::json!({
                    "@timestamp": {
                        "gte": "now-1h",
                        "lte": "now"
                    }
                }))
            }
            TimeframePreset::Last6h => {
                Some(serde_json::json!({
                    "@timestamp": {
                        "gte": "now-6h",
                        "lte": "now"
                    }
                }))
            }
            TimeframePreset::Last12h => {
                Some(serde_json::json!({
                    "@timestamp": {
                        "gte": "now-12h",
                        "lte": "now"
                    }
                }))
            }
            TimeframePreset::Last24h => {
                Some(serde_json::json!({
                    "@timestamp": {
                        "gte": "now-24h",
                        "lte": "now"
                    }
                }))
            }
            TimeframePreset::Last72h => {
                Some(serde_json::json!({
                    "@timestamp": {
                        "gte": "now-72h",
                        "lte": "now"
                    }
                }))
            }
            TimeframePreset::Custom => {
                let from_str = format!(
                    "{:04}-{:02}-{:02}T{:02}:{:02}:{:02}Z",
                    state.custom_from_year, state.custom_from_month, state.custom_from_day,
                    state.custom_from_hour, state.custom_from_minute, state.custom_from_second
                );
                let to_str = format!(
                    "{:04}-{:02}-{:02}T{:02}:{:02}:{:02}Z",
                    state.custom_to_year, state.custom_to_month, state.custom_to_day,
                    state.custom_to_hour, state.custom_to_minute, state.custom_to_second
                );
                Some(serde_json::json!({
                    "@timestamp": {
                        "gte": from_str,
                        "lte": to_str
                    }
                }))
            }
        };

        let mut must_clauses = vec![];
        if !state.search_query.trim().is_empty() {
            must_clauses.push(serde_json::json!({
                "query_string": {
                    "query": state.search_query
                }
            }));
        } else {
            must_clauses.push(serde_json::json!({
                "match_all": {}
            }));
        }

        let mut filter_clauses = vec![];
        if let Some(rf) = range_filter {
            filter_clauses.push(serde_json::json!({
                "range": rf
            }));
        }

        let body = serde_json::json!({
            "size": 50,
            "sort": [
                { "@timestamp": { "order": "desc", "unmapped_type": "date" } }
            ],
            "query": {
                "bool": {
                    "must": must_clauses,
                    "filter": filter_clauses
                }
            }
        });

        let body_str = serde_json::to_string_pretty(&body).unwrap_or_default();
        let path = format!("/{}/_search", state.index_pattern);

        *on_search_triggered = Some((path, body_str));
    }

    ui.add_space(12.0);

    // Error alert banner
    if let Some(err) = &state.error {
        let danger_color = Theme::danger();
        let text_color = Theme::contrast_text_color(danger_color);
        egui::Frame::new()
            .fill(danger_color)
            .corner_radius(Theme::CARD_ROUNDING)
            .inner_margin(Theme::CARD_PADDING)
            .show(ui, |ui| {
                ui.horizontal(|ui| {
                    ui.label(egui::RichText::new("⚠️").size(16.0).color(text_color));
                    ui.label(
                        egui::RichText::new(format!("Search Error: {}", err))
                            .color(text_color)
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
