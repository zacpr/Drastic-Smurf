use crate::core::es_client::{CatIndex, DataStream};
use crate::ui::theme::Theme;
use egui::{Color32, RichText, Ui};
use std::collections::HashSet;

#[derive(Debug, Clone)]
pub enum IndicesSubTab {
    Indices,
    DataStreams,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum IndicesSortField {
    Name,
    Docs,
    Size,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SortOrder {
    Ascending,
    Descending,
}

#[derive(Debug, Clone)]
pub struct IndexDetail {
    pub name: String,
    pub is_datastream: bool,
    pub ilm_policy: Option<String>,
    pub ilm_explain: Option<serde_json::Value>,
    pub index_template: Option<String>,
    pub settings: Option<serde_json::Value>,
}

pub struct IndicesState {
    pub selected_cluster: String,
    pub active_sub_tab: IndicesSubTab,
    pub filter: String,
    pub indices: Vec<CatIndex>,
    pub datastreams: Vec<DataStream>,
    pub is_loading: bool,
    pub error: Option<String>,
    pub selected_indices: HashSet<String>,
    pub selected_datastreams: HashSet<String>,
    pub filter_only_selected: bool,
    pub sort_field: IndicesSortField,
    pub sort_order: SortOrder,
    pub previous_doc_counts: std::collections::HashMap<String, i64>,
    pub previous_sizes: std::collections::HashMap<String, u64>,
    pub selected_detail: Option<IndexDetail>,
    pub detail_loading: bool,
}

impl IndicesState {
    pub fn new() -> Self {
        Self {
            selected_cluster: String::new(),
            active_sub_tab: IndicesSubTab::Indices,
            filter: String::new(),
            indices: Vec::new(),
            datastreams: Vec::new(),
            is_loading: false,
            error: None,
            selected_indices: HashSet::new(),
            selected_datastreams: HashSet::new(),
            filter_only_selected: false,
            sort_field: IndicesSortField::Name,
            sort_order: SortOrder::Ascending,
            previous_doc_counts: std::collections::HashMap::new(),
            previous_sizes: std::collections::HashMap::new(),
            selected_detail: None,
            detail_loading: false,
        }
    }

    pub fn update_data(&mut self, new_indices: Vec<CatIndex>, new_datastreams: Vec<DataStream>) {
        // Store previous values before replacing them
        self.previous_doc_counts.clear();
        self.previous_sizes.clear();

        for idx in &self.indices {
            let docs = idx
                .docs_count
                .as_deref()
                .unwrap_or("0")
                .parse::<i64>()
                .unwrap_or(0);
            let size = idx
                .store_size
                .as_deref()
                .unwrap_or("0")
                .parse::<u64>()
                .unwrap_or(0);
            self.previous_doc_counts.insert(idx.index.clone(), docs);
            self.previous_sizes.insert(idx.index.clone(), size);
        }

        for ds in &self.datastreams {
            let size = ds.store_size_bytes.unwrap_or(0) as u64;
            self.previous_sizes.insert(ds.name.clone(), size);
        }

        self.indices = new_indices;
        self.datastreams = new_datastreams;
    }
}

pub fn render_indices_module(
    ui: &mut Ui,
    state: &mut IndicesState,
    clusters: &[String],
    on_refresh: &mut Option<(String, bool)>, // (cluster_name, is_datastream)
    on_fetch_detail: &mut Option<(String, bool)>, // (target_name, is_datastream)
) {
    ui.heading("Datastreams & Indices");
    ui.add_space(8.0);

    if clusters.is_empty() {
        ui.label("No clusters configured. Add a cluster first.");
        return;
    }

    if state.selected_cluster.is_empty() || !clusters.contains(&state.selected_cluster) {
        state.selected_cluster = clusters[0].clone();
        *on_refresh = Some((state.selected_cluster.clone(), true));
    }

    // Top Control Bar: Cluster Selection & Sub-Tab buttons & Refresh button
    ui.horizontal(|ui| {
        ui.label("Cluster:");
        let prev_cluster = state.selected_cluster.clone();
        egui::ComboBox::from_id_salt("indices_cluster_select")
            .selected_text(&state.selected_cluster)
            .show_ui(ui, |ui| {
                for c in clusters {
                    ui.selectable_value(&mut state.selected_cluster, c.clone(), c);
                }
            });

        if state.selected_cluster != prev_cluster {
            state.is_loading = true;
            state.previous_doc_counts.clear();
            state.previous_sizes.clear();
            *on_refresh = Some((state.selected_cluster.clone(), true));
        }

        ui.add_space(8.0);
        if state.is_loading {
            ui.spinner();
        } else {
            crate::ui::animations::pulsing_dot(ui, Theme::success(), 2.0);
            ui.label(RichText::new("Live").size(10.0).color(Theme::text_muted()));
        }

        ui.add_space(16.0);

        // Sub-tab selectors
        let indices_btn = ui.selectable_label(
            matches!(state.active_sub_tab, IndicesSubTab::Indices),
            "📦 Indices",
        );
        if indices_btn.clicked() {
            state.active_sub_tab = IndicesSubTab::Indices;
        }

        let ds_btn = ui.selectable_label(
            matches!(state.active_sub_tab, IndicesSubTab::DataStreams),
            "🌊 Data Streams",
        );
        if ds_btn.clicked() {
            state.active_sub_tab = IndicesSubTab::DataStreams;
        }

        ui.add_space(16.0);

        if ui.button("🔄 Refresh").clicked() {
            state.is_loading = true;
            *on_refresh = Some((state.selected_cluster.clone(), true));
        }

        if state.is_loading {
            ui.spinner();
        }
    });

    ui.add_space(8.0);

    // Search and filter textbox + "Filter Selected" toggle button
    ui.horizontal(|ui| {
        ui.label("🔍 Filter:");
        ui.text_edit_singleline(&mut state.filter);
        if !state.filter.is_empty() {
            if ui.small_button("Clear").clicked() {
                state.filter.clear();
            }
        }

        ui.add_space(8.0);

        let selected_count = match state.active_sub_tab {
            IndicesSubTab::Indices => state.selected_indices.len(),
            IndicesSubTab::DataStreams => state.selected_datastreams.len(),
        };

        let button_text = if state.filter_only_selected {
            format!("🎯 Show All (Filtering {} Selected)", selected_count)
        } else {
            format!("🎯 Filter Selected ({})", selected_count)
        };

        let btn = if state.filter_only_selected {
            egui::Button::new(
                RichText::new(&button_text)
                    .color(Theme::text_primary())
                    .strong(),
            )
            .fill(Theme::accent())
        } else {
            egui::Button::new(RichText::new(&button_text).color(Theme::text_secondary()))
                .fill(Theme::bg_input())
        };

        if ui.add(btn).clicked() {
            state.filter_only_selected = !state.filter_only_selected;
        }
    });

    ui.add_space(12.0);

    if let Some(err) = &state.error {
        ui.colored_label(Theme::danger(), format!("Error: {}", err));
        ui.add_space(8.0);
    }

    // Main scroll area for listing items
    egui::Frame::new()
        .fill(Theme::bg_card())
        .corner_radius(Theme::CARD_ROUNDING)
        .inner_margin(Theme::CARD_PADDING)
        .show(ui, |ui| {
            let height = ui.available_height() - 16.0;
            egui::ScrollArea::vertical()
                .id_salt("indices_list_scroll")
                .max_height(height)
                .auto_shrink([false, false])
                .show(ui, |ui| match state.active_sub_tab {
                    IndicesSubTab::Indices => render_indices_table(ui, state, on_fetch_detail),
                    IndicesSubTab::DataStreams => {
                        render_datastreams_table(ui, state, on_fetch_detail)
                    }
                });
        });
}

fn render_indices_table(
    ui: &mut Ui,
    state: &mut IndicesState,
    on_fetch_detail: &mut Option<(String, bool)>,
) {
    // 1. First filter by search text (cloned to avoid borrow conflicts)
    let text_filtered_indices: Vec<CatIndex> = state
        .indices
        .iter()
        .filter(|idx| {
            state.filter.is_empty()
                || idx
                    .index
                    .to_lowercase()
                    .contains(&state.filter.to_lowercase())
        })
        .cloned()
        .collect();

    // 2. Then filter by selection if active
    let mut display_indices: Vec<CatIndex> = text_filtered_indices
        .iter()
        .filter(|idx| !state.filter_only_selected || state.selected_indices.contains(&idx.index))
        .cloned()
        .collect();

    if display_indices.is_empty() {
        if state.filter_only_selected {
            ui.label(
                RichText::new("No selected indices match the active filters.")
                    .color(Theme::text_muted()),
            );
        } else {
            ui.label(RichText::new("No indices found").color(Theme::text_muted()));
        }
        return;
    }

    // Sort display indices
    display_indices.sort_by(|a, b| {
        let cmp = match state.sort_field {
            IndicesSortField::Name => a.index.to_lowercase().cmp(&b.index.to_lowercase()),
            IndicesSortField::Docs => {
                let docs_a = a
                    .docs_count
                    .as_deref()
                    .unwrap_or("0")
                    .parse::<i64>()
                    .unwrap_or(0);
                let docs_b = b
                    .docs_count
                    .as_deref()
                    .unwrap_or("0")
                    .parse::<i64>()
                    .unwrap_or(0);
                docs_a.cmp(&docs_b)
            }
            IndicesSortField::Size => {
                let size_a = a
                    .store_size
                    .as_deref()
                    .unwrap_or("0")
                    .parse::<u64>()
                    .unwrap_or(0);
                let size_b = b
                    .store_size
                    .as_deref()
                    .unwrap_or("0")
                    .parse::<u64>()
                    .unwrap_or(0);
                size_a.cmp(&size_b)
            }
        };

        let final_cmp = if cmp == std::cmp::Ordering::Equal {
            a.index.to_lowercase().cmp(&b.index.to_lowercase())
        } else {
            cmp
        };

        if state.sort_order == SortOrder::Descending {
            final_cmp.reverse()
        } else {
            final_cmp
        }
    });

    egui::Grid::new("indices_table_grid")
        .num_columns(5)
        .spacing([24.0, 10.0])
        .striped(true)
        .show(ui, |ui| {
            // Header Row
            // Column 0: Checkbox for Select All
            let all_visible_selected = !text_filtered_indices.is_empty()
                && text_filtered_indices
                    .iter()
                    .all(|idx| state.selected_indices.contains(&idx.index));
            let mut select_all = all_visible_selected;

            if ui.checkbox(&mut select_all, "").clicked() {
                if select_all {
                    for idx in &text_filtered_indices {
                        state.selected_indices.insert(idx.index.clone());
                    }
                } else {
                    for idx in &text_filtered_indices {
                        state.selected_indices.remove(&idx.index);
                    }
                }
            }

            // Helper function for sortable headers
            let header_btn = |ui: &mut Ui,
                              label: &str,
                              field: IndicesSortField,
                              state: &mut IndicesState| {
                let is_active = state.sort_field == field;
                let text = if is_active {
                    let arrow = match state.sort_order {
                        SortOrder::Ascending => " ⏶",
                        SortOrder::Descending => " ⏷",
                    };
                    format!("{}{}", label, arrow)
                } else {
                    label.to_string()
                };

                let text_color = if is_active {
                    Theme::accent()
                } else {
                    Theme::text_secondary()
                };
                let response =
                    ui.selectable_label(is_active, RichText::new(text).strong().color(text_color));
                if response.clicked() {
                    if is_active {
                        state.sort_order = match state.sort_order {
                            SortOrder::Ascending => SortOrder::Descending,
                            SortOrder::Descending => SortOrder::Ascending,
                        };
                    } else {
                        state.sort_field = field;
                        state.sort_order = SortOrder::Ascending;
                    }
                }
            };

            // Column 1: Index Name
            header_btn(ui, "Index Name", IndicesSortField::Name, state);

            // Column 2: Health / Status
            ui.label(
                RichText::new("Health / Status")
                    .strong()
                    .color(Theme::text_secondary()),
            );

            // Column 3: Docs
            header_btn(ui, "Docs", IndicesSortField::Docs, state);

            // Column 4: Size
            header_btn(ui, "Size", IndicesSortField::Size, state);

            ui.end_row();

            // Data Rows
            for idx in display_indices {
                // Column 0: Checkbox for individual selection
                let mut is_selected = state.selected_indices.contains(&idx.index);
                if ui.checkbox(&mut is_selected, "").changed() {
                    if is_selected {
                        state.selected_indices.insert(idx.index.clone());
                    } else {
                        state.selected_indices.remove(&idx.index);
                    }
                }

                // Column 1: Health dot & Name
                ui.horizontal(|ui| {
                    let dot_color = match idx.health.as_deref() {
                        Some("green") => Theme::success(),
                        Some("yellow") => Color32::from_rgb(235, 179, 41),
                        Some("red") => Theme::danger(),
                        _ => Color32::from_rgb(100, 100, 100),
                    };
                    ui.add(
                        crate::ui::widgets::ConnectionDot::new(true)
                            .color(dot_color)
                            .size(6.0),
                    );
                    let name_btn = ui
                        .add(egui::Link::new(
                            egui::RichText::new(&idx.index)
                                .strong()
                                .color(Theme::text_primary()),
                        ))
                        .on_hover_text(
                            "Click to view settings, index template, and ILM policy details",
                        );
                    if name_btn.clicked() {
                        *on_fetch_detail = Some((idx.index.clone(), false));
                    }
                });

                // Column 2: Status
                let status = idx.status.clone().unwrap_or_else(|| "open".to_string());
                ui.label(RichText::new(&status).color(Theme::text_muted()).size(11.0));

                // Column 3: Docs Count
                let docs = idx.docs_count.as_deref().unwrap_or("0");
                let docs_formatted = format_number(docs);
                ui.horizontal(|ui| {
                    ui.label(
                        RichText::new(docs_formatted)
                            .color(Theme::text_muted())
                            .size(11.0),
                    );
                    let current_docs = docs.parse::<i64>().unwrap_or(0);
                    if let Some(&prev_docs) = state.previous_doc_counts.get(&idx.index) {
                        let diff = current_docs - prev_docs;
                        if diff > 0 {
                            let diff_str = format!(" (+{})", format_number(&diff.to_string()));
                            ui.label(
                                RichText::new(diff_str)
                                    .color(Theme::success())
                                    .size(10.0)
                                    .strong(),
                            );
                        } else if diff < 0 {
                            let diff_str = format!(" ({})", format_number(&diff.to_string()));
                            ui.label(
                                RichText::new(diff_str)
                                    .color(Theme::danger())
                                    .size(10.0)
                                    .strong(),
                            );
                        }
                    }
                });

                // Column 4: Size
                let size = idx.store_size.as_deref().unwrap_or("0");
                let size_formatted = format_size(&size);
                ui.horizontal(|ui| {
                    ui.label(
                        RichText::new(size_formatted)
                            .color(Theme::text_muted())
                            .size(11.0),
                    );
                    let current_size = size.parse::<u64>().unwrap_or(0);
                    if let Some(&prev_size) = state.previous_sizes.get(&idx.index) {
                        if current_size > prev_size {
                            let diff = current_size - prev_size;
                            let diff_str = format!(" (+{})", human_bytes(diff as f64));
                            ui.label(
                                RichText::new(diff_str)
                                    .color(Theme::success())
                                    .size(10.0)
                                    .strong(),
                            );
                        } else if current_size < prev_size {
                            let diff = prev_size - current_size;
                            let diff_str = format!(" (-{})", human_bytes(diff as f64));
                            ui.label(
                                RichText::new(diff_str)
                                    .color(Theme::danger())
                                    .size(10.0)
                                    .strong(),
                            );
                        }
                    }
                });

                ui.end_row();
            }
        });
}

fn render_datastreams_table(
    ui: &mut Ui,
    state: &mut IndicesState,
    on_fetch_detail: &mut Option<(String, bool)>,
) {
    // 1. First filter by search text (cloned to avoid borrow conflicts)
    let text_filtered_streams: Vec<DataStream> = state
        .datastreams
        .iter()
        .filter(|ds| {
            state.filter.is_empty()
                || ds
                    .name
                    .to_lowercase()
                    .contains(&state.filter.to_lowercase())
        })
        .cloned()
        .collect();

    // 2. Then filter by selection if active
    let mut display_streams: Vec<DataStream> = text_filtered_streams
        .iter()
        .filter(|ds| !state.filter_only_selected || state.selected_datastreams.contains(&ds.name))
        .cloned()
        .collect();

    if display_streams.is_empty() {
        if state.filter_only_selected {
            ui.label(
                RichText::new("No selected datastreams match the active filters.")
                    .color(Theme::text_muted()),
            );
        } else {
            ui.label(RichText::new("No data streams found").color(Theme::text_muted()));
        }
        return;
    }

    // Sort display streams
    display_streams.sort_by(|a, b| {
        let cmp = match state.sort_field {
            IndicesSortField::Name => a.name.to_lowercase().cmp(&b.name.to_lowercase()),
            IndicesSortField::Docs => a.indices.len().cmp(&b.indices.len()),
            IndicesSortField::Size => {
                let size_a = a.store_size_bytes.unwrap_or(0);
                let size_b = b.store_size_bytes.unwrap_or(0);
                size_a.cmp(&size_b)
            }
        };

        let final_cmp = if cmp == std::cmp::Ordering::Equal {
            a.name.to_lowercase().cmp(&b.name.to_lowercase())
        } else {
            cmp
        };

        if state.sort_order == SortOrder::Descending {
            final_cmp.reverse()
        } else {
            final_cmp
        }
    });

    egui::Grid::new("datastreams_table_grid")
        .num_columns(5)
        .spacing([24.0, 10.0])
        .striped(true)
        .show(ui, |ui| {
            // Header Row
            // Column 0: Checkbox for Select All
            let all_visible_selected = !text_filtered_streams.is_empty()
                && text_filtered_streams
                    .iter()
                    .all(|ds| state.selected_datastreams.contains(&ds.name));
            let mut select_all = all_visible_selected;

            if ui.checkbox(&mut select_all, "").clicked() {
                if select_all {
                    for ds in &text_filtered_streams {
                        state.selected_datastreams.insert(ds.name.clone());
                    }
                } else {
                    for ds in &text_filtered_streams {
                        state.selected_datastreams.remove(&ds.name);
                    }
                }
            }

            // Helper function for sortable headers
            let header_btn = |ui: &mut Ui,
                              label: &str,
                              field: IndicesSortField,
                              state: &mut IndicesState| {
                let is_active = state.sort_field == field;
                let text = if is_active {
                    let arrow = match state.sort_order {
                        SortOrder::Ascending => " ⏶",
                        SortOrder::Descending => " ⏷",
                    };
                    format!("{}{}", label, arrow)
                } else {
                    label.to_string()
                };

                let text_color = if is_active {
                    Theme::accent()
                } else {
                    Theme::text_secondary()
                };
                let response =
                    ui.selectable_label(is_active, RichText::new(text).strong().color(text_color));
                if response.clicked() {
                    if is_active {
                        state.sort_order = match state.sort_order {
                            SortOrder::Ascending => SortOrder::Descending,
                            SortOrder::Descending => SortOrder::Ascending,
                        };
                    } else {
                        state.sort_field = field;
                        state.sort_order = SortOrder::Ascending;
                    }
                }
            };

            // Column 1: Data Stream Name
            header_btn(ui, "Data Stream Name", IndicesSortField::Name, state);

            // Column 2: Backing Indices
            header_btn(ui, "Backing Indices", IndicesSortField::Docs, state);

            // Column 3: Status
            ui.label(
                RichText::new("Status")
                    .strong()
                    .color(Theme::text_secondary()),
            );

            // Column 4: Total Size
            header_btn(ui, "Total Size", IndicesSortField::Size, state);

            ui.end_row();

            // Data Rows
            for ds in display_streams {
                // Column 0: Checkbox for individual selection
                let mut is_selected = state.selected_datastreams.contains(&ds.name);
                if ui.checkbox(&mut is_selected, "").changed() {
                    if is_selected {
                        state.selected_datastreams.insert(ds.name.clone());
                    } else {
                        state.selected_datastreams.remove(&ds.name);
                    }
                }

                // Column 1: Health dot & Name
                ui.horizontal(|ui| {
                    let dot_color = match ds.status.to_lowercase().as_str() {
                        "green" => Theme::success(),
                        "yellow" => Color32::from_rgb(235, 179, 41),
                        "red" => Theme::danger(),
                        _ => Color32::from_rgb(100, 100, 100),
                    };
                    ui.add(
                        crate::ui::widgets::ConnectionDot::new(true)
                            .color(dot_color)
                            .size(6.0),
                    );
                    let name_btn = ui
                        .add(egui::Link::new(
                            egui::RichText::new(&ds.name)
                                .strong()
                                .color(Theme::text_primary()),
                        ))
                        .on_hover_text(
                            "Click to view settings, index template, and ILM policy details",
                        );
                    if name_btn.clicked() {
                        *on_fetch_detail = Some((ds.name.clone(), true));
                    }
                });

                // Column 2: Backing Indices
                let indices_count = ds.indices.len().to_string();
                ui.label(
                    RichText::new(indices_count)
                        .color(Theme::text_muted())
                        .size(11.0),
                );

                // Column 3: Status
                ui.label(
                    RichText::new(&ds.status)
                        .color(Theme::text_muted())
                        .size(11.0),
                );

                // Column 4: Total Size
                let total_bytes = ds.store_size_bytes.unwrap_or(0);
                let size_formatted = human_bytes(total_bytes as f64);
                ui.horizontal(|ui| {
                    ui.label(
                        RichText::new(size_formatted)
                            .color(Theme::text_muted())
                            .size(11.0),
                    );
                    let current_size = total_bytes as u64;
                    if let Some(&prev_size) = state.previous_sizes.get(&ds.name) {
                        if current_size > prev_size {
                            let diff = current_size - prev_size;
                            let diff_str = format!(" (+{})", human_bytes(diff as f64));
                            ui.label(
                                RichText::new(diff_str)
                                    .color(Theme::success())
                                    .size(10.0)
                                    .strong(),
                            );
                        } else if current_size < prev_size {
                            let diff = prev_size - current_size;
                            let diff_str = format!(" (-{})", human_bytes(diff as f64));
                            ui.label(
                                RichText::new(diff_str)
                                    .color(Theme::danger())
                                    .size(10.0)
                                    .strong(),
                            );
                        }
                    }
                });

                ui.end_row();
            }
        });
}

fn format_number(val: &str) -> String {
    if let Ok(n) = val.parse::<i64>() {
        let s = n.to_string();
        let mut formatted = String::new();
        let bytes = s.as_bytes();
        let len = bytes.len();
        for (i, &byte) in bytes.iter().enumerate() {
            formatted.push(byte as char);
            if byte != b'-' && i + 1 < len && (len - i - 1) % 3 == 0 {
                formatted.push(',');
            }
        }
        formatted
    } else {
        val.to_string()
    }
}

fn format_size(size_str: &str) -> String {
    if let Ok(bytes) = size_str.parse::<u64>() {
        human_bytes(bytes as f64)
    } else {
        size_str.to_string()
    }
}

fn human_bytes(bytes: f64) -> String {
    let suffix = ["B", "KB", "MB", "GB", "TB", "PB"];
    let mut i = 0;
    let mut val = bytes;
    while val >= 1024.0 && i < suffix.len() - 1 {
        val /= 1024.0;
        i += 1;
    }
    format!("{:.1} {}", val, suffix[i])
}

pub fn index_pattern_matches(pattern: &str, index_name: &str) -> bool {
    let pattern = pattern.trim();
    if pattern == "*" || pattern == index_name {
        return true;
    }

    let parts: Vec<&str> = pattern.split('*').collect();
    if parts.len() == 1 {
        return pattern == index_name;
    }

    let mut current_idx = 0;
    for (i, part) in parts.iter().enumerate() {
        if part.is_empty() {
            continue;
        }
        if i == 0 {
            if !index_name.starts_with(part) {
                return false;
            }
            current_idx = part.len();
        } else if i == parts.len() - 1 {
            return index_name[current_idx..].ends_with(part);
        } else {
            if let Some(pos) = index_name[current_idx..].find(part) {
                current_idx += pos + part.len();
            } else {
                return false;
            }
        }
    }
    true
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_index_pattern_matches() {
        assert!(index_pattern_matches("logs-*", "logs-mysql-000001"));
        assert!(index_pattern_matches(
            "kibana_sample_data_*",
            "kibana_sample_data_flights"
        ));
        assert!(index_pattern_matches("*.logs", "system.logs"));
        assert!(index_pattern_matches("test-*-prod", "test-database-prod"));
        assert!(!index_pattern_matches("logs-*", "metrics-cpu"));
        assert!(!index_pattern_matches("*-prod", "test-prod-old"));
    }

    #[test]
    fn test_indices_state_new() {
        let state = IndicesState::new();
        assert!(state.selected_indices.is_empty());
        assert!(state.selected_datastreams.is_empty());
        assert!(!state.filter_only_selected);
        assert_eq!(state.sort_field, IndicesSortField::Name);
        assert_eq!(state.sort_order, SortOrder::Ascending);
    }

    #[test]
    fn test_format_number() {
        assert_eq!(format_number("123"), "123");
        assert_eq!(format_number("1500"), "1,500");
        assert_eq!(format_number("2300000"), "2,300,000");
        assert_eq!(format_number("4500000000"), "4,500,000,000");
        assert_eq!(format_number("invalid"), "invalid");
    }

    #[test]
    fn test_human_bytes() {
        assert_eq!(human_bytes(500.0), "500.0 B");
        assert_eq!(human_bytes(1536.0), "1.5 KB");
        assert_eq!(human_bytes(1024.0 * 1024.0 * 2.5), "2.5 MB");
    }

    #[test]
    fn test_indices_update_data_diff_tracking() {
        let mut state = IndicesState::new();

        let idx1 = CatIndex {
            index: "logs-1".to_string(),
            docs_count: Some("100".to_string()),
            docs_deleted: None,
            store_size: Some("1024".to_string()),
            pri_store_size: None,
            health: None,
            status: None,
            pri: None,
            rep: None,
        };

        // Initial insert
        state.update_data(vec![idx1], vec![]);
        assert!(state.previous_doc_counts.is_empty());
        assert!(state.previous_sizes.is_empty());

        // Refresh with changed values
        let idx2 = CatIndex {
            index: "logs-1".to_string(),
            docs_count: Some("150".to_string()),
            docs_deleted: None,
            store_size: Some("2048".to_string()),
            pri_store_size: None,
            health: None,
            status: None,
            pri: None,
            rep: None,
        };

        state.update_data(vec![idx2], vec![]);

        assert_eq!(*state.previous_doc_counts.get("logs-1").unwrap(), 100);
        assert_eq!(*state.previous_sizes.get("logs-1").unwrap(), 1024);
    }
}
