use egui::{Ui, Color32, RichText};
use crate::core::es_client::{CatIndex, DataStream};
use crate::ui::theme::Theme;

#[derive(Debug, Clone)]
pub enum IndicesSubTab {
    Indices,
    DataStreams,
}

pub struct IndicesState {
    pub selected_cluster: String,
    pub active_sub_tab: IndicesSubTab,
    pub filter: String,
    pub indices: Vec<CatIndex>,
    pub datastreams: Vec<DataStream>,
    pub is_loading: bool,
    pub error: Option<String>,
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
        }
    }
}

pub fn render_indices_module(
    ui: &mut Ui,
    state: &mut IndicesState,
    clusters: &[String],
    on_refresh: &mut Option<(String, bool)>, // (cluster_name, is_datastream)
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
            *on_refresh = Some((state.selected_cluster.clone(), true));
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

    // Search and filter textbox
    ui.horizontal(|ui| {
        ui.label("🔍 Filter:");
        ui.text_edit_singleline(&mut state.filter);
        if !state.filter.is_empty() {
            if ui.small_button("Clear").clicked() {
                state.filter.clear();
            }
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
                .show(ui, |ui| {
                    match state.active_sub_tab {
                        IndicesSubTab::Indices => render_indices_table(ui, state),
                        IndicesSubTab::DataStreams => render_datastreams_table(ui, state),
                    }
                });
        });
}

fn render_indices_table(ui: &mut Ui, state: &IndicesState) {
    let filtered_indices: Vec<&CatIndex> = state.indices.iter()
        .filter(|idx| state.filter.is_empty() || idx.index.to_lowercase().contains(&state.filter.to_lowercase()))
        .collect();

    if filtered_indices.is_empty() {
        ui.label(RichText::new("No indices found").color(Theme::text_muted()));
        return;
    }

    // Header row
    ui.horizontal(|ui| {
        ui.set_min_height(24.0);
        ui.label(RichText::new("Index Name").strong().color(Theme::text_secondary()));
        
        let available_w = ui.available_width() - 80.0;
        ui.add_space(available_w * 0.45);
        ui.label(RichText::new("Health").strong().color(Theme::text_secondary()));
        ui.add_space(50.0);
        ui.label(RichText::new("Docs").strong().color(Theme::text_secondary()));
        ui.add_space(60.0);
        ui.label(RichText::new("Size").strong().color(Theme::text_secondary()));
    });
    ui.separator();

    for idx in filtered_indices {
        ui.horizontal(|ui| {
            ui.set_min_height(28.0);
            
            // Health indicator color
            let dot_color = match idx.health.as_deref() {
                Some("green") => Theme::success(),
                Some("yellow") => Color32::from_rgb(235, 179, 41),
                Some("red") => Theme::danger(),
                _ => Color32::from_rgb(100, 100, 100),
            };
            
            // Render index name
            ui.add(crate::ui::widgets::ConnectionDot::new(true).color(dot_color).size(6.0));
            ui.label(RichText::new(&idx.index).strong().color(Theme::text_primary()));

            let available_w = ui.available_width() - 80.0;
            ui.add_space(available_w * 0.45);
            
            // Render Status / Health text
            let status = idx.status.clone().unwrap_or_else(|| "open".to_string());
            ui.label(RichText::new(&status).color(Theme::text_muted()).size(11.0));
            
            ui.add_space(45.0);

            // Doc count
            let docs = idx.docs_count.as_deref().unwrap_or("0");
            let docs_formatted = format_number(docs);
            ui.label(RichText::new(docs_formatted).color(Theme::text_muted()).size(11.0));

            ui.add_space(55.0);

            // Size
            let size = idx.store_size.as_deref().unwrap_or("0b");
            let size_formatted = format_size(size);
            ui.label(RichText::new(size_formatted).color(Theme::text_muted()).size(11.0));
        });
        ui.separator();
    }
}

fn render_datastreams_table(ui: &mut Ui, state: &IndicesState) {
    let filtered_streams: Vec<&DataStream> = state.datastreams.iter()
        .filter(|ds| state.filter.is_empty() || ds.name.to_lowercase().contains(&state.filter.to_lowercase()))
        .collect();

    if filtered_streams.is_empty() {
        ui.label(RichText::new("No data streams found").color(Theme::text_muted()));
        return;
    }

    // Header row
    ui.horizontal(|ui| {
        ui.set_min_height(24.0);
        ui.label(RichText::new("Data Stream Name").strong().color(Theme::text_secondary()));
        
        let available_w = ui.available_width() - 80.0;
        ui.add_space(available_w * 0.45);
        ui.label(RichText::new("Backing Indices").strong().color(Theme::text_secondary()));
        ui.add_space(40.0);
        ui.label(RichText::new("Status").strong().color(Theme::text_secondary()));
        ui.add_space(40.0);
        ui.label(RichText::new("Total Size").strong().color(Theme::text_secondary()));
    });
    ui.separator();

    for ds in filtered_streams {
        ui.horizontal(|ui| {
            ui.set_min_height(28.0);
            
            // Health indicator color
            let dot_color = match ds.status.to_lowercase().as_str() {
                "green" => Theme::success(),
                "yellow" => Color32::from_rgb(235, 179, 41),
                "red" => Theme::danger(),
                _ => Color32::from_rgb(100, 100, 100),
            };

            ui.add(crate::ui::widgets::ConnectionDot::new(true).color(dot_color).size(6.0));
            ui.label(RichText::new(&ds.name).strong().color(Theme::text_primary()));

            let available_w = ui.available_width() - 80.0;
            ui.add_space(available_w * 0.45);

            // Backing indices count
            let indices_count = ds.indices.len().to_string();
            ui.label(RichText::new(indices_count).color(Theme::text_muted()).size(11.0));

            ui.add_space(80.0);

            // Status
            ui.label(RichText::new(&ds.status).color(Theme::text_muted()).size(11.0));

            ui.add_space(60.0);

            // Total Size
            let total_bytes = ds.store_size_bytes.unwrap_or(0);
            let size_formatted = human_bytes(total_bytes as f64);
            ui.label(RichText::new(size_formatted).color(Theme::text_muted()).size(11.0));
        });
        ui.separator();
    }
}

fn format_number(val: &str) -> String {
    if let Ok(n) = val.parse::<i64>() {
        if n >= 1_000_000_000 {
            format!("{:.1}B", n as f64 / 1_000_000_000.0)
        } else if n >= 1_000_000 {
            format!("{:.1}M", n as f64 / 1_000_000.0)
        } else if n >= 1_000 {
            format!("{:.1}K", n as f64 / 1_000.0)
        } else {
            n.to_string()
        }
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
