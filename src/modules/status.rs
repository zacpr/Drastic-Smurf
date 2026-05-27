use crate::core::config::ClusterConfig;
use crate::core::es_client::{ClusterHealth, ClusterStats};
use crate::ui::theme::Theme;
use crate::ui::widgets::{ConnectionDot, human_bytes, human_docs};
use egui::{Color32, Ui};

#[derive(Debug, Clone, Default)]
pub struct StatusState {
    pub health_data: Vec<(String, Option<ClusterHealth>)>,
    pub stats_data: Vec<(String, Option<ClusterStats>)>,
    pub explains: std::collections::HashMap<String, Option<crate::core::es_client::AllocationExplain>>,
    pub errors: std::collections::HashMap<String, String>,
}

pub fn render_status_module(
    ui: &mut Ui,
    clusters: &[ClusterConfig],
    state: &StatusState,
    hover_effects: bool,
) {
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

    egui::ScrollArea::vertical()
        .id_salt("status")
        .show(ui, |ui| {
        if clusters.is_empty() {
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
                        for (i, cluster) in clusters.iter().enumerate() {
                            if i % cols == col_idx {
                                let health = state
                                    .health_data
                                    .iter()
                                    .find(|(n, _)| n == &cluster.name)
                                    .and_then(|(_, h)| h.clone());
                                let stats = state
                                    .stats_data
                                    .iter()
                                    .find(|(n, _)| n == &cluster.name)
                                    .and_then(|(_, s)| s.clone());
                                let error = state.errors.get(&cluster.name).cloned();
                                let explain = state.explains.get(&cluster.name).cloned().flatten();
                                render_status_card(
                                    ui,
                                    &cluster.name,
                                    &health,
                                    stats,
                                    error,
                                    explain,
                                    col_width,
                                    hover_effects,
                                );
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
    error: Option<String>,
    explain: Option<crate::core::es_client::AllocationExplain>,
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

        if let Some(ref err) = error {
            ui.colored_label(Theme::danger(), format!("⚠ {}", err));
            ui.add_space(4.0);
        }

        if let Some(h) = health {
            let mut items: Vec<(&str, String)> = Vec::new();
            items.push(("Nodes", h.number_of_nodes.to_string()));
            items.push(("Active Shards", h.active_shards.to_string()));
            items.push(("Unassigned", h.unassigned_shards.to_string()));
            items.push(("Relocating", h.relocating_shards.to_string()));

            let mut node_role_items: Vec<(&str, u32)> = Vec::new();
            let mut jvm_heap: Option<(u64, u64)> = None;

            if let Some(s) = stats {
                if let Some(ref indices) = s.indices {
                    items.push(("Indices", indices.count.to_string()));
                    if let Some(ref docs) = indices.docs {
                        items.push(("Docs", human_docs(docs.count)));
                    }
                    if let Some(ref store) = indices.store {
                        items.push(("Store", human_bytes(store.size_in_bytes)));
                    }
                }
                if let Some(ref nodes_stats) = s.nodes {
                    if let Some(ref count) = nodes_stats.count {
                        if count.data > 0 {
                            node_role_items.push(("Data", count.data));
                        }
                        if count.master > 0 {
                            node_role_items.push(("Master", count.master));
                        }
                        if count.ingest > 0 {
                            node_role_items.push(("Ingest", count.ingest));
                        }
                        if count.ml > 0 {
                            node_role_items.push(("ML", count.ml));
                        }
                        if count.coordinating_only > 0 {
                            node_role_items.push(("Coordinating", count.coordinating_only));
                        }
                        if count.data_hot > 0 {
                            node_role_items.push(("Hot", count.data_hot));
                        }
                        if count.data_warm > 0 {
                            node_role_items.push(("Warm", count.data_warm));
                        }
                        if count.data_cold > 0 {
                            node_role_items.push(("Cold", count.data_cold));
                        }
                        if count.data_frozen > 0 {
                            node_role_items.push(("Frozen", count.data_frozen));
                        }
                        if count.data_content > 0 {
                            node_role_items.push(("Content", count.data_content));
                        }
                        if count.remote_cluster_client > 0 {
                            node_role_items.push(("CCR Client", count.remote_cluster_client));
                        }
                        if count.transform > 0 {
                            node_role_items.push(("Transform", count.transform));
                        }
if count.voting_only > 0 {
                            node_role_items.push(("Voting", count.voting_only));
                        }
                    }
                    if let Some(ref jvm) = nodes_stats.jvm {
                        if let Some(ref mem) = jvm.mem {
                            jvm_heap = Some((mem.heap_used_in_bytes, mem.heap_max_in_bytes));
                        }
                    }
                }
            }

            // Main stats grid
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

            if let Some((used, max)) = jvm_heap {
                ui.add_space(8.0);
                ui.label(
                    egui::RichText::new(format!(
                        "JVM Heap: {} / {}",
                        human_bytes(used),
                        human_bytes(max)
                    ))
                    .size(11.0)
                    .color(Theme::text_muted()),
                );
            }

            if !node_role_items.is_empty() {
                ui.add_space(8.0);
                ui.separator();
                ui.add_space(4.0);
                ui.label(
                    egui::RichText::new("Nodes by Role:")
                        .strong()
                        .size(11.0)
                        .color(Theme::text_secondary()),
                );
                ui.add_space(2.0);
                for chunk in node_role_items.chunks(3) {
                    ui.horizontal(|ui| {
                        for (label, count) in chunk {
                            ui.label(
                                egui::RichText::new(format!("{}: {}", label, count))
                                    .size(11.0)
                                    .color(Theme::text_primary()),
                            );
                            ui.add_space(12.0);
                        }
                    });
                }
            }
            if let Some(ref exp) = explain {
                ui.add_space(8.0);
                ui.separator();
                ui.add_space(4.0);
                
                ui.horizontal(|ui| {
                    ui.label(
                        egui::RichText::new("⚠ Diagnostic Report (Unassigned Shards)")
                            .strong()
                            .size(11.0)
                            .color(Theme::warning()),
                    );
                });
                
                ui.add_space(2.0);
                
                let primary_str = if exp.primary { "Primary" } else { "Replica" };
                ui.label(
                    egui::RichText::new(format!(
                        "• Shard: {} #{} ({}) - {}",
                        exp.index, exp.shard, primary_str, exp.current_state.to_uppercase()
                    ))
                    .size(10.5)
                    .color(Theme::text_primary()),
                );

                if let Some(ref reason) = exp.reason {
                    ui.label(
                        egui::RichText::new(format!("• Reason: {}", reason))
                            .size(10.5)
                            .color(Theme::text_muted()),
                    );
                }

                if let Some(ref explain_text) = exp.explanation {
                    ui.label(
                        egui::RichText::new(format!("• Details: {}", explain_text))
                            .size(10.5)
                            .color(Theme::text_muted()),
                    );
                }

                if !exp.decider_reasons.is_empty() {
                    ui.add_space(2.0);
                    ui.label(
                        egui::RichText::new("• Allocation Blockers:")
                            .size(10.5)
                            .color(Theme::danger()),
                    );
                    for dec_reason in &exp.decider_reasons {
                        ui.label(
                            egui::RichText::new(format!("  - {}", dec_reason))
                                .size(10.0)
                                .color(Theme::text_muted()),
                        );
                    }
                }
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
                accent.r(),
                accent.g(),
                accent.b(),
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
