use crate::core::es_client::{ClusterHealth, ClusterStats};
use crate::ui::widgets::human_bytes;
use egui::Ui;

#[derive(Debug, Clone, Default)]
pub struct StatusState {
    pub health_data: Vec<(String, Option<ClusterHealth>)>,
    pub stats_data: Vec<(String, Option<ClusterStats>)>,
}

pub fn render_status_module(ui: &mut Ui, state: &StatusState) {
    ui.heading("Cluster Status");
    ui.add_space(16.0);

    egui::ScrollArea::vertical().show(ui, |ui| {
        for (i, (name, health)) in state.health_data.iter().enumerate() {
            let stats = state
                .stats_data
                .iter()
                .find(|(n, _)| n == name)
                .and_then(|(_, s)| s.clone());

            let frame = egui::Frame::new()
                .fill(crate::ui::theme::Theme::BG_CARD)
                .corner_radius(crate::ui::theme::Theme::CARD_ROUNDING)
                .inner_margin(crate::ui::theme::Theme::CARD_PADDING);

            frame.show(ui, |ui| {
                ui.set_min_width(300.0);
                ui.horizontal(|ui| {
                    ui.label(egui::RichText::new(name).strong().size(16.0));
                    if let Some(h) = health {
                        let color = crate::ui::theme::Theme::health_color(&h.status);
                        ui.colored_label(color, &h.status);
                    } else {
                        ui.colored_label(crate::ui::theme::Theme::DANGER, "Unreachable");
                    }
                });

                if let Some(h) = health {
                    ui.horizontal(|ui| {
                        ui.label(format!("Nodes: {}", h.number_of_nodes));
                        ui.label(format!("Active Shards: {}", h.active_shards));
                        ui.label(format!("Unassigned: {}", h.unassigned_shards));
                    });
                }

                if let Some(s) = stats {
                    ui.add_space(4.0);
                    if let Some(ref indices) = s.indices {
                        ui.horizontal(|ui| {
                            ui.label(format!("Indices: {}", indices.count));
                            if let Some(ref docs) = indices.docs {
                                ui.label(format!("Docs: {}", docs.count));
                            }
                            if let Some(ref store) = indices.store {
                                ui.label(format!("Store: {}", human_bytes(store.size_in_bytes)));
                            }
                        });
                    }
                    if let Some(ref nodes) = s.nodes {
                        if let Some(ref count) = nodes.count {
                            ui.horizontal(|ui| {
                                ui.label(format!("Total Nodes: {}", count.total));
                                ui.label(format!("Data: {}", count.data));
                            });
                        }
                        if let Some(ref jvm) = nodes.jvm {
                            ui.horizontal(|ui| {
                                ui.label(format!(
                                    "JVM Heap: {} / {}",
                                    human_bytes(jvm.used_heap_in_bytes),
                                    human_bytes(jvm.max_heap_in_bytes)
                                ));
                            });
                        }
                    }
                }
            });
            ui.add_space(8.0);
        }
    });
}
