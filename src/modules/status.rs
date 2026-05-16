use crate::core::es_client::{ClusterHealth, ClusterStats};
use crate::ui::theme::Theme;
use crate::ui::widgets::{ConnectionDot, human_bytes};
use egui::{Color32, Ui};

#[derive(Debug, Clone, Default)]
pub struct StatusState {
    pub health_data: Vec<(String, Option<ClusterHealth>)>,
    pub stats_data: Vec<(String, Option<ClusterStats>)>,
}

pub fn render_status_module(ui: &mut Ui, state: &StatusState, hover_effects: bool) {
    ui.heading("Cluster Status");
    ui.add_space(16.0);

    let min_card_width = 400.0;
    let card_spacing = 16.0;
    let available_width = ui.available_width();
    let cols = if available_width >= min_card_width * 2.0 + card_spacing {
        2
    } else {
        1
    };
    let col_width = (available_width - (cols - 1) as f32 * card_spacing) / cols as f32;

    egui::ScrollArea::vertical().show(ui, |ui| {
        if state.health_data.is_empty() {
            ui.label(
                egui::RichText::new("No clusters configured. Add a cluster to begin monitoring.")
                    .color(Theme::text_muted())
                    .size(14.0),
            );
            return;
        }

        ui.horizontal(|ui| {
            for col in 0..cols {
                let col_idx = col;
                ui.allocate_ui_with_layout(
                    egui::Vec2::new(col_width, ui.available_height()),
                    egui::Layout::top_down(egui::Align::Min),
                    |ui| {
                        for (i, (name, health)) in state.health_data.iter().enumerate() {
                            if i % cols == col_idx {
                                let stats = state
                                    .stats_data
                                    .iter()
                                    .find(|(n, _)| n == name)
                                    .and_then(|(_, s)| s.clone());
                                render_status_card(ui, name, health, stats, col_width, hover_effects);
                                ui.add_space(card_spacing);
                            }
                        }
                    },
                );
                if col + 1 < cols {
                    ui.add_space(card_spacing);
                }
            }
        });
    });
}

fn render_status_card(
    ui: &mut Ui,
    name: &str,
    health: &Option<ClusterHealth>,
    stats: Option<ClusterStats>,
    col_width: f32,
    hover_effects: bool,
) {
    let frame = egui::Frame::new()
        .fill(Theme::bg_card())
        .corner_radius(Theme::CARD_ROUNDING)
        .inner_margin(Theme::CARD_PADDING)
        .stroke(egui::Stroke::new(1.0, Theme::bg_input()));

    let response = frame.show(ui, |ui| {
        ui.set_min_width(col_width - Theme::CARD_PADDING.x * 2.0);
        ui.set_max_width(col_width - Theme::CARD_PADDING.x * 2.0);

        // Header
        ui.horizontal(|ui| {
            let connected = health.is_some();
            ui.add(ConnectionDot::new(connected).size(10.0));
            ui.vertical(|ui| {
                ui.label(
                    egui::RichText::new(name)
                        .strong()
                        .size(17.0)
                        .color(Theme::text_primary()),
                );
            });

            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                if let Some(h) = health {
                    let color = Theme::health_color(&h.status);
                    ui.add(crate::ui::widgets::StatePill::new(&h.status, color));
                } else {
                    ui.add(crate::ui::widgets::StatePill::new(
                        "Unreachable",
                        Theme::danger(),
                    ));
                }
            });
        });
        ui.add_space(8.0);

        if let Some(h) = health {
            // Stats grid (2 columns)
            let mut items: Vec<(&str, String)> = Vec::new();
            items.push(("Nodes", h.number_of_nodes.to_string()));
            items.push(("Active Shards", h.active_shards.to_string()));
            items.push(("Unassigned", h.unassigned_shards.to_string()));
            items.push(("Relocating", h.relocating_shards.to_string()));

            if let Some(s) = stats {
                if let Some(ref indices) = s.indices {
                    items.push(("Indices", indices.count.to_string()));
                    if let Some(ref docs) = indices.docs {
                        items.push(("Docs", docs.count.to_string()));
                    }
                    if let Some(ref store) = indices.store {
                        items.push(("Store", human_bytes(store.size_in_bytes)));
                    }
                }
                if let Some(ref nodes) = s.nodes {
                    if let Some(ref count) = nodes.count {
                        items.push(("Data Nodes", count.data.to_string()));
                    }
                    if let Some(ref jvm) = nodes.jvm {
                        items.push((
                            "JVM Heap",
                            format!(
                                "{} / {}",
                                human_bytes(jvm.used_heap_in_bytes),
                                human_bytes(jvm.max_heap_in_bytes)
                            ),
                        ));
                    }
                }
            }

            for pair in items.chunks(2) {
                ui.horizontal(|ui| {
                    ui.set_width(ui.available_width());
                    let half = ui.available_width() / 2.0 - 8.0;
                    for (j, (label, value)) in pair.iter().enumerate() {
                        if j > 0 {
                            ui.add_space(16.0);
                        }
                        ui.allocate_ui_with_layout(
                            egui::Vec2::new(half, 18.0),
                            egui::Layout::left_to_right(egui::Align::Center),
                            |ui| {
                                ui.label(
                                    egui::RichText::new(format!("{}: ", label))
                                        .color(Theme::text_muted())
                                        .size(11.0),
                                );
                                ui.label(
                                    egui::RichText::new(value.clone())
                                        .color(Theme::text_primary())
                                        .size(11.0)
                                        .strong(),
                                );
                            },
                        );
                    }
                });
            }
        } else {
            ui.label(
                egui::RichText::new("Cluster is unreachable")
                    .color(Theme::danger())
                    .size(12.0),
            );
        }
    });

    if hover_effects {
        let rect = response.response.rect;
        let hovered = response.response.hovered();
        let glow_alpha = ui.ctx().animate_value_with_time(
            ui.id().with(name).with("hover_glow"),
            if hovered { 0.12 } else { 0.0 },
            0.15,
        );
        if glow_alpha > 0.0 {
            let accent = Theme::accent();
            let glow_color = Color32::from_rgba_premultiplied(
                accent.r(), accent.g(), accent.b(),
                (glow_alpha * 255.0) as u8,
            );
            ui.painter().rect_stroke(
                rect.expand(1.0),
                Theme::CARD_ROUNDING,
                egui::Stroke::new(1.0, glow_color),
                egui::StrokeKind::Middle,
            );
        }
    }
}
