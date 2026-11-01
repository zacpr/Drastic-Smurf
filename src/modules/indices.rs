use crate::core::es_client::{CatIndex, ComponentTemplateItem, DataStream, IndexTemplateItem};
use crate::ui::theme::Theme;
use egui::{Color32, RichText, Ui};
use std::collections::HashSet;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum IndicesPageTab {
    DatastreamsAndIndices,
    IndexTemplates,
    Components,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
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
pub enum TemplateSortField {
    Name,
    Priority,
    ComposedCount,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ComponentSortField {
    Name,
    UsedCount,
    Version,
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
    pub active_page_tab: IndicesPageTab,
    pub active_sub_tab: IndicesSubTab,
    pub filter: String,
    pub filter_typed_at: Option<std::time::Instant>,
    pub applied_filter: String,
    pub hide_system: bool,
    pub indices: Vec<CatIndex>,
    pub datastreams: Vec<DataStream>,
    pub index_templates: Vec<IndexTemplateItem>,
    pub component_templates: Vec<ComponentTemplateItem>,
    pub is_loading: bool,
    pub error: Option<String>,
    pub selected_indices: HashSet<String>,
    pub selected_datastreams: HashSet<String>,
    pub filter_only_selected: bool,
    pub sort_field: IndicesSortField,
    pub sort_order: SortOrder,
    pub template_sort_field: TemplateSortField,
    pub template_sort_order: SortOrder,
    pub component_sort_field: ComponentSortField,
    pub component_sort_order: SortOrder,
    pub previous_doc_counts: std::collections::HashMap<String, i64>,
    pub previous_sizes: std::collections::HashMap<String, u64>,
    pub selected_detail: Option<IndexDetail>,
    pub detail_loading: bool,
    pub selected_template_detail: Option<IndexTemplateItem>,
    pub template_detail_loading: bool,
    pub selected_component_detail: Option<ComponentTemplateItem>,
    pub component_detail_loading: bool,
}

impl IndicesState {
    pub fn new() -> Self {
        Self {
            selected_cluster: String::new(),
            active_page_tab: IndicesPageTab::DatastreamsAndIndices,
            active_sub_tab: IndicesSubTab::Indices,
            filter: String::new(),
            filter_typed_at: None,
            applied_filter: String::new(),
            hide_system: true,
            indices: Vec::new(),
            datastreams: Vec::new(),
            index_templates: Vec::new(),
            component_templates: Vec::new(),
            is_loading: false,
            error: None,
            selected_indices: HashSet::new(),
            selected_datastreams: HashSet::new(),
            filter_only_selected: false,
            sort_field: IndicesSortField::Name,
            sort_order: SortOrder::Ascending,
            template_sort_field: TemplateSortField::Name,
            template_sort_order: SortOrder::Ascending,
            component_sort_field: ComponentSortField::Name,
            component_sort_order: SortOrder::Ascending,
            previous_doc_counts: std::collections::HashMap::new(),
            previous_sizes: std::collections::HashMap::new(),
            selected_detail: None,
            detail_loading: false,
            selected_template_detail: None,
            template_detail_loading: false,
            selected_component_detail: None,
            component_detail_loading: false,
        }
    }

    pub fn update_data(
        &mut self,
        new_indices: Vec<CatIndex>,
        new_datastreams: Vec<DataStream>,
        new_templates: Vec<IndexTemplateItem>,
        new_components: Vec<ComponentTemplateItem>,
    ) {
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
        self.index_templates = new_templates;
        self.component_templates = new_components;
    }
}

/// Default debounce window for text-input filters.
pub const FILTER_DEBOUNCE_MS: u64 = 300;

/// Returns true if the pending filter input has aged past the debounce window
/// and should be applied now. Pure: takes `now` and `debounce` so it can be
/// exercised without sleeping or rendering.
pub fn filter_debounce_due(
    typed_at: Option<std::time::Instant>,
    now: std::time::Instant,
    debounce: std::time::Duration,
) -> bool {
    typed_at.is_some_and(|t| now.saturating_duration_since(t) >= debounce)
}

/// Returns the duration until the debounce fires, or `None` if no input is pending.
pub fn filter_debounce_remaining(
    typed_at: Option<std::time::Instant>,
    now: std::time::Instant,
    debounce: std::time::Duration,
) -> Option<std::time::Duration> {
    typed_at.and_then(|t| {
        let elapsed = now.saturating_duration_since(t);
        debounce.checked_sub(elapsed)
    })
}

pub fn render_indices_module(
    ui: &mut Ui,
    state: &mut IndicesState,
    clusters: &[String],
    on_refresh: &mut Option<(String, bool)>, // (cluster_name, is_datastream)
    on_fetch_detail: &mut Option<(String, bool)>, // (target_name, is_datastream)
    on_fetch_template: &mut Option<String>,  // template_name
    on_fetch_component: &mut Option<String>, // component_name
) {
    // Apply debounced filter
    let now = std::time::Instant::now();
    let debounce = std::time::Duration::from_millis(FILTER_DEBOUNCE_MS);
    if filter_debounce_due(state.filter_typed_at, now, debounce) {
        state.filter_typed_at = None;
        if state.applied_filter != state.filter {
            state.applied_filter = state.filter.clone();
        }
    } else if let Some(remaining) = filter_debounce_remaining(state.filter_typed_at, now, debounce)
    {
        ui.ctx().request_repaint_after(remaining);
    }

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

    // Top Control Bar: Cluster Selection & Refresh button
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

        if ui.button("🔄 Refresh").clicked() {
            state.is_loading = true;
            *on_refresh = Some((state.selected_cluster.clone(), true));
        }

        if state.is_loading {
            ui.spinner();
        }
    });

    ui.add_space(10.0);

    // Main 3 Tabs Navigation Bar
    ui.horizontal(|ui| {
        for (label, tab) in [
            (
                "📦 Datastreams & Indices",
                IndicesPageTab::DatastreamsAndIndices,
            ),
            ("📋 Index Templates", IndicesPageTab::IndexTemplates),
            ("🧩 Components", IndicesPageTab::Components),
        ] {
            let is_active = state.active_page_tab == tab;
            let text = egui::RichText::new(label).size(13.0);
            let text = if is_active {
                text.color(Theme::accent()).strong()
            } else {
                text.color(Theme::text_secondary())
            };
            if ui.selectable_label(is_active, text).clicked() {
                state.active_page_tab = tab;
            }
        }
    });

    ui.add_space(4.0);
    ui.separator();
    ui.add_space(8.0);

    // Tab-specific controls & Content
    match state.active_page_tab {
        IndicesPageTab::DatastreamsAndIndices => {
            // Sub-tab selectors (Indices vs Data Streams)
            ui.horizontal(|ui| {
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

                // Filter controls
                ui.label("🔍 Filter:");
                let filter_resp = ui.add(
                    egui::TextEdit::singleline(&mut state.filter)
                        .id_source("indices_filter_input")
                        .desired_width(200.0),
                );
                if filter_resp.changed() {
                    state.filter_typed_at = Some(std::time::Instant::now());
                    ui.ctx().request_repaint();
                }
                if !state.filter.is_empty() && ui.small_button("Clear").clicked() {
                    state.filter.clear();
                    state.applied_filter.clear();
                    state.filter_typed_at = None;
                }

                ui.add_space(8.0);

                // "Filter Selected" toggle
                let selected_count = match state.active_sub_tab {
                    IndicesSubTab::Indices => state.selected_indices.len(),
                    IndicesSubTab::DataStreams => state.selected_datastreams.len(),
                };

                let button_text = if state.filter_only_selected {
                    format!("🎯 Show All ({})", selected_count)
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

                ui.add_space(10.0);

                // Hide System checkbox
                ui.checkbox(&mut state.hide_system, "🛡️ Hide System")
                    .on_hover_text("Hide system indices and datastreams (starting with '.')");
            });

            ui.add_space(8.0);

            if let Some(err) = &state.error {
                ui.colored_label(Theme::danger(), format!("Error: {}", err));
                ui.add_space(8.0);
            }

            // Scroll area for listing items
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
                            IndicesSubTab::Indices => {
                                render_indices_table(ui, state, on_fetch_detail)
                            }
                            IndicesSubTab::DataStreams => {
                                render_datastreams_table(ui, state, on_fetch_detail)
                            }
                        });
                });
        }
        IndicesPageTab::IndexTemplates => {
            // Index Templates Header controls
            ui.horizontal(|ui| {
                ui.label("🔍 Filter Templates:");
                let filter_resp = ui.add(
                    egui::TextEdit::singleline(&mut state.filter)
                        .id_source("templates_filter_input")
                        .desired_width(220.0),
                );
                if filter_resp.changed() {
                    state.filter_typed_at = Some(std::time::Instant::now());
                    ui.ctx().request_repaint();
                }
                if !state.filter.is_empty() && ui.small_button("Clear").clicked() {
                    state.filter.clear();
                    state.applied_filter.clear();
                    state.filter_typed_at = None;
                }

                ui.add_space(12.0);

                ui.checkbox(&mut state.hide_system, "🛡️ Hide System")
                    .on_hover_text("Hide internal/built-in templates starting with '.' or '@'");

                ui.add_space(16.0);
                let total = state.index_templates.len();
                ui.label(
                    RichText::new(format!("Total: {} templates", total))
                        .color(Theme::text_muted())
                        .size(11.5),
                );
            });

            ui.add_space(8.0);

            if let Some(err) = &state.error {
                ui.colored_label(Theme::danger(), format!("Error: {}", err));
                ui.add_space(8.0);
            }

            egui::Frame::new()
                .fill(Theme::bg_card())
                .corner_radius(Theme::CARD_ROUNDING)
                .inner_margin(Theme::CARD_PADDING)
                .show(ui, |ui| {
                    let height = ui.available_height() - 16.0;
                    egui::ScrollArea::vertical()
                        .id_salt("index_templates_list_scroll")
                        .max_height(height)
                        .auto_shrink([false, false])
                        .show(ui, |ui| {
                            render_index_templates_table(
                                ui,
                                state,
                                on_fetch_template,
                                on_fetch_component,
                            );
                        });
                });
        }
        IndicesPageTab::Components => {
            // Components Header controls
            ui.horizontal(|ui| {
                ui.label("🔍 Filter Components:");
                let filter_resp = ui.add(
                    egui::TextEdit::singleline(&mut state.filter)
                        .id_source("components_filter_input")
                        .desired_width(220.0),
                );
                if filter_resp.changed() {
                    state.filter_typed_at = Some(std::time::Instant::now());
                    ui.ctx().request_repaint();
                }
                if !state.filter.is_empty() && ui.small_button("Clear").clicked() {
                    state.filter.clear();
                    state.applied_filter.clear();
                    state.filter_typed_at = None;
                }

                ui.add_space(12.0);

                ui.checkbox(&mut state.hide_system, "🛡️ Hide System")
                    .on_hover_text(
                        "Hide internal/built-in component templates starting with '.' or '@'",
                    );

                ui.add_space(16.0);
                let total = state.component_templates.len();
                ui.label(
                    RichText::new(format!("Total: {} components", total))
                        .color(Theme::text_muted())
                        .size(11.5),
                );
            });

            ui.add_space(8.0);

            if let Some(err) = &state.error {
                ui.colored_label(Theme::danger(), format!("Error: {}", err));
                ui.add_space(8.0);
            }

            egui::Frame::new()
                .fill(Theme::bg_card())
                .corner_radius(Theme::CARD_ROUNDING)
                .inner_margin(Theme::CARD_PADDING)
                .show(ui, |ui| {
                    let height = ui.available_height() - 16.0;
                    egui::ScrollArea::vertical()
                        .id_salt("components_list_scroll")
                        .max_height(height)
                        .auto_shrink([false, false])
                        .show(ui, |ui| {
                            render_components_table(ui, state, on_fetch_component);
                        });
                });
        }
    }
}

fn render_indices_table(
    ui: &mut Ui,
    state: &mut IndicesState,
    on_fetch_detail: &mut Option<(String, bool)>,
) {
    // 1. First filter by search text and system prefix
    let text_filtered_indices: Vec<CatIndex> = state
        .indices
        .iter()
        .filter(|idx| {
            if state.hide_system && idx.index.starts_with('.') {
                return false;
            }
            state.applied_filter.is_empty()
                || idx
                    .index
                    .to_lowercase()
                    .contains(&state.applied_filter.to_lowercase())
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
        } else if state.hide_system && !state.indices.is_empty() {
            ui.label(
                RichText::new("No indices found (system indices are hidden by 'Hide System').")
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
                let size_formatted = format_size(size);
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
    // 1. First filter by search text and system prefix
    let text_filtered_streams: Vec<DataStream> = state
        .datastreams
        .iter()
        .filter(|ds| {
            if state.hide_system && ds.name.starts_with('.') {
                return false;
            }
            state.applied_filter.is_empty()
                || ds
                    .name
                    .to_lowercase()
                    .contains(&state.applied_filter.to_lowercase())
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
        } else if state.hide_system && !state.datastreams.is_empty() {
            ui.label(
                RichText::new(
                    "No data streams found (system streams are hidden by 'Hide System').",
                )
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

fn render_index_templates_table(
    ui: &mut Ui,
    state: &mut IndicesState,
    on_fetch_template: &mut Option<String>,
    on_fetch_component: &mut Option<String>,
) {
    let text_filtered: Vec<IndexTemplateItem> = state
        .index_templates
        .iter()
        .filter(|t| {
            if state.hide_system && (t.name.starts_with('.') || t.name.starts_with('@')) {
                return false;
            }
            if state.applied_filter.is_empty() {
                return true;
            }
            let query = state.applied_filter.to_lowercase();
            t.name.to_lowercase().contains(&query)
                || t.index_template
                    .index_patterns
                    .iter()
                    .any(|p| p.to_lowercase().contains(&query))
                || t.index_template
                    .composed_of
                    .iter()
                    .any(|c| c.to_lowercase().contains(&query))
        })
        .cloned()
        .collect();

    if text_filtered.is_empty() {
        if state.hide_system && !state.index_templates.is_empty() {
            ui.label(
                RichText::new(
                    "No index templates match the filter, or all are hidden by 'Hide System'.",
                )
                .color(Theme::text_muted()),
            );
        } else {
            ui.label(
                RichText::new("No index templates found in this cluster.")
                    .color(Theme::text_muted()),
            );
        }
        return;
    }

    let mut display_templates = text_filtered;
    display_templates.sort_by(|a, b| {
        let cmp = match state.template_sort_field {
            TemplateSortField::Name => a.name.to_lowercase().cmp(&b.name.to_lowercase()),
            TemplateSortField::Priority => {
                let p_a = a.index_template.priority.unwrap_or(0);
                let p_b = b.index_template.priority.unwrap_or(0);
                p_a.cmp(&p_b)
            }
            TemplateSortField::ComposedCount => a
                .index_template
                .composed_of
                .len()
                .cmp(&b.index_template.composed_of.len()),
        };
        let final_cmp = if cmp == std::cmp::Ordering::Equal {
            a.name.to_lowercase().cmp(&b.name.to_lowercase())
        } else {
            cmp
        };
        if state.template_sort_order == SortOrder::Descending {
            final_cmp.reverse()
        } else {
            final_cmp
        }
    });

    egui::Grid::new("index_templates_table_grid")
        .num_columns(6)
        .spacing([24.0, 10.0])
        .striped(true)
        .show(ui, |ui| {
            // Headers
            let mut header_btn = |ui: &mut Ui, label: &str, field: TemplateSortField| {
                let is_active = state.template_sort_field == field;
                let text = if is_active {
                    let arrow = match state.template_sort_order {
                        SortOrder::Ascending => " ⏶",
                        SortOrder::Descending => " ⏷",
                    };
                    format!("{}{}", label, arrow)
                } else {
                    label.to_string()
                };
                let color = if is_active {
                    Theme::accent()
                } else {
                    Theme::text_secondary()
                };
                if ui
                    .selectable_label(is_active, RichText::new(text).strong().color(color))
                    .clicked()
                {
                    if is_active {
                        state.template_sort_order = match state.template_sort_order {
                            SortOrder::Ascending => SortOrder::Descending,
                            SortOrder::Descending => SortOrder::Ascending,
                        };
                    } else {
                        state.template_sort_field = field;
                        state.template_sort_order = SortOrder::Ascending;
                    }
                }
            };

            header_btn(ui, "Template Name", TemplateSortField::Name);
            ui.label(
                RichText::new("Index Patterns")
                    .strong()
                    .color(Theme::text_secondary()),
            );
            header_btn(ui, "Composed Components", TemplateSortField::ComposedCount);
            header_btn(ui, "Priority", TemplateSortField::Priority);
            ui.label(
                RichText::new("Type")
                    .strong()
                    .color(Theme::text_secondary()),
            );
            ui.label(
                RichText::new("Version")
                    .strong()
                    .color(Theme::text_secondary()),
            );
            ui.end_row();

            // Rows
            for tmpl in display_templates {
                // Column 1: Name link (opens Index Template modal)
                let name_link = ui
                    .add(egui::Link::new(
                        RichText::new(&tmpl.name)
                            .strong()
                            .color(Theme::text_primary()),
                    ))
                    .on_hover_text(
                        "Click to inspect Index Template details and component hierarchy",
                    );
                if name_link.clicked() {
                    state.selected_template_detail = Some(tmpl.clone());
                    if !state.index_templates.iter().any(|t| t.name == tmpl.name) {
                        *on_fetch_template = Some(tmpl.name.clone());
                    }
                }

                // Column 2: Patterns
                ui.horizontal_wrapped(|ui| {
                    if tmpl.index_template.index_patterns.is_empty() {
                        ui.label(RichText::new("—").color(Theme::text_muted()).size(11.0));
                    } else {
                        for pat in tmpl.index_template.index_patterns.iter().take(3) {
                            ui.add(crate::ui::widgets::StatePill::new(
                                pat,
                                Theme::text_primary(),
                            ));
                        }
                        if tmpl.index_template.index_patterns.len() > 3 {
                            let more = tmpl.index_template.index_patterns.len() - 3;
                            ui.label(
                                RichText::new(format!("+{}", more))
                                    .color(Theme::text_muted())
                                    .size(10.0),
                            );
                        }
                    }
                });

                // Column 3: Composed components (clickable pills!)
                ui.horizontal_wrapped(|ui| {
                    if tmpl.index_template.composed_of.is_empty() {
                        ui.label(RichText::new("—").color(Theme::text_muted()).size(11.0));
                    } else {
                        for (idx, comp_name) in tmpl.index_template.composed_of.iter().enumerate() {
                            let c_btn = ui
                                .add(egui::Link::new(
                                    RichText::new(format!("#{}: {}", idx + 1, comp_name))
                                        .size(11.0)
                                        .color(Theme::accent())
                                        .underline(),
                                ))
                                .on_hover_text("Click to view Component Template details");
                            if c_btn.clicked() {
                                if let Some(c) = state
                                    .component_templates
                                    .iter()
                                    .find(|c| &c.name == comp_name)
                                {
                                    state.selected_component_detail = Some(c.clone());
                                } else {
                                    *on_fetch_component = Some(comp_name.clone());
                                }
                            }
                        }
                    }
                });

                // Column 4: Priority
                let p_str = tmpl
                    .index_template
                    .priority
                    .map(|p| p.to_string())
                    .unwrap_or_else(|| "—".to_string());
                ui.label(RichText::new(p_str).color(Theme::text_muted()).size(11.0));

                // Column 5: Type
                if tmpl.index_template.data_stream.is_some() {
                    ui.add(crate::ui::widgets::StatePill::new(
                        "Data Stream",
                        Theme::accent(),
                    ));
                } else {
                    ui.add(crate::ui::widgets::StatePill::new(
                        "Index",
                        Theme::text_muted(),
                    ));
                }

                // Column 6: Version
                let v_str = tmpl
                    .index_template
                    .version
                    .map(|v| format!("v{}", v))
                    .unwrap_or_else(|| "—".to_string());
                ui.label(RichText::new(v_str).color(Theme::text_muted()).size(11.0));

                ui.end_row();
            }
        });
}

fn render_components_table(
    ui: &mut Ui,
    state: &mut IndicesState,
    on_fetch_component: &mut Option<String>,
) {
    let text_filtered: Vec<ComponentTemplateItem> = state
        .component_templates
        .iter()
        .filter(|c| {
            if state.hide_system && (c.name.starts_with('.') || c.name.starts_with('@')) {
                return false;
            }
            if state.applied_filter.is_empty() {
                return true;
            }
            let query = state.applied_filter.to_lowercase();
            if c.name.to_lowercase().contains(&query) {
                return true;
            }
            if let Some(desc) = c
                .component_template
                ._meta
                .as_ref()
                .and_then(|m| m.get("description"))
                .and_then(|d| d.as_str())
                && desc.to_lowercase().contains(&query)
            {
                return true;
            }
            false
        })
        .cloned()
        .collect();

    if text_filtered.is_empty() {
        if state.hide_system && !state.component_templates.is_empty() {
            ui.label(
                RichText::new(
                    "No component templates match the filter, or all are hidden by 'Hide System'.",
                )
                .color(Theme::text_muted()),
            );
        } else {
            ui.label(
                RichText::new("No component templates found in this cluster.")
                    .color(Theme::text_muted()),
            );
        }
        return;
    }

    // Precompute usage counts: how many index templates include each component
    let mut usage_counts: std::collections::HashMap<String, usize> =
        std::collections::HashMap::new();
    for tmpl in &state.index_templates {
        for comp in &tmpl.index_template.composed_of {
            *usage_counts.entry(comp.clone()).or_insert(0) += 1;
        }
    }

    let mut display_components = text_filtered;
    display_components.sort_by(|a, b| {
        let cmp = match state.component_sort_field {
            ComponentSortField::Name => a.name.to_lowercase().cmp(&b.name.to_lowercase()),
            ComponentSortField::UsedCount => {
                let u_a = usage_counts.get(&a.name).copied().unwrap_or(0);
                let u_b = usage_counts.get(&b.name).copied().unwrap_or(0);
                u_a.cmp(&u_b)
            }
            ComponentSortField::Version => {
                let v_a = a.component_template.version.unwrap_or(0);
                let v_b = b.component_template.version.unwrap_or(0);
                v_a.cmp(&v_b)
            }
        };
        let final_cmp = if cmp == std::cmp::Ordering::Equal {
            a.name.to_lowercase().cmp(&b.name.to_lowercase())
        } else {
            cmp
        };
        if state.component_sort_order == SortOrder::Descending {
            final_cmp.reverse()
        } else {
            final_cmp
        }
    });

    egui::Grid::new("components_table_grid")
        .num_columns(5)
        .spacing([24.0, 10.0])
        .striped(true)
        .show(ui, |ui| {
            // Headers
            let mut header_btn = |ui: &mut Ui, label: &str, field: ComponentSortField| {
                let is_active = state.component_sort_field == field;
                let text = if is_active {
                    let arrow = match state.component_sort_order {
                        SortOrder::Ascending => " ⏶",
                        SortOrder::Descending => " ⏷",
                    };
                    format!("{}{}", label, arrow)
                } else {
                    label.to_string()
                };
                let color = if is_active {
                    Theme::accent()
                } else {
                    Theme::text_secondary()
                };
                if ui
                    .selectable_label(is_active, RichText::new(text).strong().color(color))
                    .clicked()
                {
                    if is_active {
                        state.component_sort_order = match state.component_sort_order {
                            SortOrder::Ascending => SortOrder::Descending,
                            SortOrder::Descending => SortOrder::Ascending,
                        };
                    } else {
                        state.component_sort_field = field;
                        state.component_sort_order = SortOrder::Ascending;
                    }
                }
            };

            header_btn(ui, "Component Name", ComponentSortField::Name);
            header_btn(ui, "Used In", ComponentSortField::UsedCount);
            ui.label(
                RichText::new("Defines")
                    .strong()
                    .color(Theme::text_secondary()),
            );
            header_btn(ui, "Version", ComponentSortField::Version);
            ui.label(
                RichText::new("Description")
                    .strong()
                    .color(Theme::text_secondary()),
            );
            ui.end_row();

            // Rows
            for comp in display_components {
                // Column 1: Name link (opens Component modal)
                let name_link = ui
                    .add(egui::Link::new(
                        RichText::new(&comp.name)
                            .strong()
                            .color(Theme::text_primary()),
                    ))
                    .on_hover_text(
                        "Click to inspect Component Template details, settings, and mappings",
                    );
                if name_link.clicked() {
                    state.selected_component_detail = Some(comp.clone());
                    if !state
                        .component_templates
                        .iter()
                        .any(|c| c.name == comp.name)
                    {
                        *on_fetch_component = Some(comp.name.clone());
                    }
                }

                // Column 2: Used in X templates
                let count = usage_counts.get(&comp.name).copied().unwrap_or(0);
                if count > 0 {
                    ui.label(
                        RichText::new(format!(
                            "{} template{}",
                            count,
                            if count == 1 { "" } else { "s" }
                        ))
                        .color(Theme::success())
                        .size(11.0),
                    );
                } else {
                    ui.label(
                        RichText::new("Not used")
                            .color(Theme::text_muted())
                            .size(11.0),
                    );
                }

                // Column 3: Defines (Settings, Mappings, Aliases)
                ui.horizontal(|ui| {
                    if let Some(ref tmpl) = comp.component_template.template {
                        let has_settings = tmpl.get("settings").is_some();
                        let has_mappings = tmpl.get("mappings").is_some();
                        let has_aliases = tmpl.get("aliases").is_some();

                        if has_settings {
                            ui.add(crate::ui::widgets::StatePill::new(
                                "Settings",
                                Theme::text_primary(),
                            ));
                        }
                        if has_mappings {
                            ui.add(crate::ui::widgets::StatePill::new(
                                "Mappings",
                                Theme::accent(),
                            ));
                        }
                        if has_aliases {
                            ui.add(crate::ui::widgets::StatePill::new(
                                "Aliases",
                                Theme::text_secondary(),
                            ));
                        }
                        if !has_settings && !has_mappings && !has_aliases {
                            ui.label(RichText::new("Empty").color(Theme::text_muted()).size(11.0));
                        }
                    } else {
                        ui.label(RichText::new("—").color(Theme::text_muted()).size(11.0));
                    }
                });

                // Column 4: Version
                let v_str = comp
                    .component_template
                    .version
                    .map(|v| format!("v{}", v))
                    .unwrap_or_else(|| "—".to_string());
                ui.label(RichText::new(v_str).color(Theme::text_muted()).size(11.0));

                // Column 5: Description
                let desc = comp
                    .component_template
                    ._meta
                    .as_ref()
                    .and_then(|m| m.get("description"))
                    .and_then(|d| d.as_str())
                    .unwrap_or("—");
                ui.label(RichText::new(desc).color(Theme::text_muted()).size(11.0));

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
        } else if let Some(pos) = index_name[current_idx..].find(part) {
            current_idx += pos + part.len();
        } else {
            return false;
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
        assert!(state.hide_system);
        assert_eq!(state.active_page_tab, IndicesPageTab::DatastreamsAndIndices);
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
        state.update_data(vec![idx1], vec![], vec![], vec![]);
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

        state.update_data(vec![idx2], vec![], vec![], vec![]);

        assert_eq!(*state.previous_doc_counts.get("logs-1").unwrap(), 100);
        assert_eq!(*state.previous_sizes.get("logs-1").unwrap(), 1024);
    }

    #[test]
    fn test_hide_system_filtering() {
        let mut state = IndicesState::new();
        assert!(state.hide_system);

        let system_idx = CatIndex {
            index: ".kibana_8.1.0_001".to_string(),
            docs_count: Some("10".to_string()),
            docs_deleted: None,
            store_size: Some("1024".to_string()),
            pri_store_size: None,
            health: None,
            status: None,
            pri: None,
            rep: None,
        };
        let user_idx = CatIndex {
            index: "app-logs-2026".to_string(),
            docs_count: Some("500".to_string()),
            docs_deleted: None,
            store_size: Some("2048".to_string()),
            pri_store_size: None,
            health: None,
            status: None,
            pri: None,
            rep: None,
        };

        state.update_data(vec![system_idx, user_idx], vec![], vec![], vec![]);

        // Filter with hide_system = true
        let visible: Vec<&CatIndex> = state
            .indices
            .iter()
            .filter(|i| !state.hide_system || !i.index.starts_with('.'))
            .collect();
        assert_eq!(visible.len(), 1);
        assert_eq!(visible[0].index, "app-logs-2026");

        // Filter with hide_system = false
        state.hide_system = false;
        let visible_all: Vec<&CatIndex> = state
            .indices
            .iter()
            .filter(|i| !state.hide_system || !i.index.starts_with('.'))
            .collect();
        assert_eq!(visible_all.len(), 2);
    }

    #[test]
    fn test_filter_debounce_no_pending_input() {
        let now = std::time::Instant::now();
        let debounce = std::time::Duration::from_millis(FILTER_DEBOUNCE_MS);
        assert!(!filter_debounce_due(None, now, debounce));
        assert_eq!(filter_debounce_remaining(None, now, debounce), None);
    }

    #[test]
    fn test_filter_debounce_within_window() {
        let typed_at = std::time::Instant::now();
        let now = typed_at + std::time::Duration::from_millis(50);
        let debounce = std::time::Duration::from_millis(FILTER_DEBOUNCE_MS);
        assert!(!filter_debounce_due(Some(typed_at), now, debounce));
        let remaining = filter_debounce_remaining(Some(typed_at), now, debounce);
        assert_eq!(remaining, Some(std::time::Duration::from_millis(250)));
    }

    #[test]
    fn test_filter_debounce_at_boundary() {
        let typed_at = std::time::Instant::now();
        let now = typed_at + std::time::Duration::from_millis(FILTER_DEBOUNCE_MS);
        let debounce = std::time::Duration::from_millis(FILTER_DEBOUNCE_MS);
        assert!(filter_debounce_due(Some(typed_at), now, debounce));
        let remaining = filter_debounce_remaining(Some(typed_at), now, debounce);
        let is_zero_or_none = match remaining {
            None => true,
            Some(d) => d.is_zero(),
        };
        assert!(
            is_zero_or_none,
            "expected None or zero, got {:?}",
            remaining
        );
    }

    #[test]
    fn test_filter_debounce_past_window() {
        let typed_at = std::time::Instant::now();
        let now = typed_at + std::time::Duration::from_millis(500);
        let debounce = std::time::Duration::from_millis(FILTER_DEBOUNCE_MS);
        assert!(filter_debounce_due(Some(typed_at), now, debounce));
        assert_eq!(
            filter_debounce_remaining(Some(typed_at), now, debounce),
            None
        );
    }

    #[test]
    fn test_filter_debounce_simulated_typing_burst() {
        let debounce = std::time::Duration::from_millis(FILTER_DEBOUNCE_MS);
        let mut typed_at = std::time::Instant::now();
        for i in 1..=5 {
            std::thread::sleep(std::time::Duration::from_millis(50));
            typed_at = std::time::Instant::now();
            let just_typed = typed_at;
            assert!(
                !filter_debounce_due(Some(just_typed), just_typed, debounce),
                "filter should not be due right after keystroke {}",
                i
            );
        }
        std::thread::sleep(std::time::Duration::from_millis(FILTER_DEBOUNCE_MS + 50));
        let now = std::time::Instant::now();
        assert!(filter_debounce_due(Some(typed_at), now, debounce));
    }

    #[test]
    fn test_index_template_filtering_and_hide_system() {
        use crate::core::es_client::{IndexTemplateContent, IndexTemplateItem};

        let mut state = IndicesState::new();
        let user_tmpl = IndexTemplateItem {
            name: "logs-telematics".to_string(),
            index_template: IndexTemplateContent {
                index_patterns: vec!["logs-telematics-*".to_string()],
                composed_of: vec![
                    "logs-settings".to_string(),
                    "telematics-mappings".to_string(),
                ],
                priority: Some(100),
                version: Some(1),
                _meta: None,
                data_stream: Some(serde_json::json!({})),
                template: None,
            },
        };

        let system_tmpl = IndexTemplateItem {
            name: ".kibana-event-log".to_string(),
            index_template: IndexTemplateContent {
                index_patterns: vec![".kibana-event-log-*".to_string()],
                composed_of: vec![],
                priority: Some(200),
                version: Some(1),
                _meta: None,
                data_stream: None,
                template: None,
            },
        };

        state.update_data(vec![], vec![], vec![user_tmpl, system_tmpl], vec![]);

        // 1. With hide_system = true, system template is filtered out
        let visible: Vec<&IndexTemplateItem> = state
            .index_templates
            .iter()
            .filter(|t| {
                !state.hide_system || (!t.name.starts_with('.') && !t.name.starts_with('@'))
            })
            .collect();
        assert_eq!(visible.len(), 1);
        assert_eq!(visible[0].name, "logs-telematics");

        // 2. With hide_system = false, both are visible
        state.hide_system = false;
        let visible_all: Vec<&IndexTemplateItem> = state
            .index_templates
            .iter()
            .filter(|t| {
                !state.hide_system || (!t.name.starts_with('.') && !t.name.starts_with('@'))
            })
            .collect();
        assert_eq!(visible_all.len(), 2);

        // 3. Filter by pattern match
        state.applied_filter = "telematics".to_string();
        let matched: Vec<&IndexTemplateItem> = state
            .index_templates
            .iter()
            .filter(|t| {
                let query = state.applied_filter.to_lowercase();
                t.name.to_lowercase().contains(&query)
                    || t.index_template
                        .index_patterns
                        .iter()
                        .any(|p| p.to_lowercase().contains(&query))
                    || t.index_template
                        .composed_of
                        .iter()
                        .any(|c| c.to_lowercase().contains(&query))
            })
            .collect();
        assert_eq!(matched.len(), 1);
        assert_eq!(matched[0].name, "logs-telematics");
    }

    #[test]
    fn test_component_template_usage_and_filtering() {
        use crate::core::es_client::{
            ComponentTemplateContent, ComponentTemplateItem, IndexTemplateContent,
            IndexTemplateItem,
        };

        let mut state = IndicesState::new();

        let comp1 = ComponentTemplateItem {
            name: "logs-settings".to_string(),
            component_template: ComponentTemplateContent {
                template: Some(serde_json::json!({"settings": {"number_of_shards": 1}})),
                version: Some(1),
                _meta: Some(serde_json::json!({"description": "Base logs settings"})),
            },
        };

        let comp2 = ComponentTemplateItem {
            name: "telematics-mappings".to_string(),
            component_template: ComponentTemplateContent {
                template: Some(
                    serde_json::json!({"mappings": {"properties": {"speed": {"type": "float"}}}}),
                ),
                version: Some(2),
                _meta: None,
            },
        };

        let comp_sys = ComponentTemplateItem {
            name: ".fleet-agent-settings".to_string(),
            component_template: ComponentTemplateContent {
                template: None,
                version: None,
                _meta: None,
            },
        };

        let tmpl1 = IndexTemplateItem {
            name: "logs-telematics".to_string(),
            index_template: IndexTemplateContent {
                index_patterns: vec!["logs-telematics-*".to_string()],
                composed_of: vec![
                    "logs-settings".to_string(),
                    "telematics-mappings".to_string(),
                ],
                priority: Some(100),
                version: Some(1),
                _meta: None,
                data_stream: None,
                template: None,
            },
        };

        let tmpl2 = IndexTemplateItem {
            name: "logs-metrics".to_string(),
            index_template: IndexTemplateContent {
                index_patterns: vec!["logs-metrics-*".to_string()],
                composed_of: vec!["logs-settings".to_string()],
                priority: Some(50),
                version: Some(1),
                _meta: None,
                data_stream: None,
                template: None,
            },
        };

        state.update_data(
            vec![],
            vec![],
            vec![tmpl1, tmpl2],
            vec![comp1, comp2, comp_sys],
        );

        // Calculate usage counts
        let mut usage_counts: std::collections::HashMap<String, usize> =
            std::collections::HashMap::new();
        for tmpl in &state.index_templates {
            for comp in &tmpl.index_template.composed_of {
                *usage_counts.entry(comp.clone()).or_insert(0) += 1;
            }
        }

        // logs-settings is used in 2 templates
        assert_eq!(usage_counts.get("logs-settings").copied().unwrap_or(0), 2);
        // telematics-mappings is used in 1 template
        assert_eq!(
            usage_counts
                .get("telematics-mappings")
                .copied()
                .unwrap_or(0),
            1
        );
        // system component is used in 0 templates
        assert_eq!(
            usage_counts
                .get(".fleet-agent-settings")
                .copied()
                .unwrap_or(0),
            0
        );

        // Hide system check
        assert!(state.hide_system);
        let visible_comps: Vec<&ComponentTemplateItem> = state
            .component_templates
            .iter()
            .filter(|c| {
                !state.hide_system || (!c.name.starts_with('.') && !c.name.starts_with('@'))
            })
            .collect();
        assert_eq!(visible_comps.len(), 2);
    }
}
