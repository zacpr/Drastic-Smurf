use crate::core::config::ClusterConfig;
use crate::ui::theme::Theme;
use crate::ui::widgets::{ConnectionDot, GradientProgressBar, StatePill, human_bytes, human_docs};
use egui::{Color32, Ui};

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DashboardTab {
    Overview,
    Detailed,
}

#[derive(Debug, Clone)]
pub struct DashboardState {
    pub selected_cluster: String,
    pub current_tab: DashboardTab,
}

impl Default for DashboardState {
    fn default() -> Self {
        Self {
            selected_cluster: String::new(),
            current_tab: DashboardTab::Overview,
        }
    }
}

pub fn render_dashboard_module(
    ui: &mut Ui,
    clusters: &[ClusterConfig],
    state: &mut DashboardState,
    status_state: &crate::modules::status::StatusState,
    hover_effects: bool,
) {
    ui.heading("Cluster Dashboard");
    ui.add_space(8.0);

    // Sub-tab navigation
    ui.horizontal(|ui| {
        for (label, tab) in [
            ("Overview Grid", DashboardTab::Overview),
            ("Detailed Single Cluster", DashboardTab::Detailed),
        ] {
            let is_active = state.current_tab == tab;
            let text = egui::RichText::new(label).size(13.0);
            let text = if is_active {
                text.color(Theme::accent()).strong()
            } else {
                text.color(Theme::text_secondary())
            };
            if ui.selectable_label(is_active, text).clicked() {
                state.current_tab = tab;
            }
        }
    });
    ui.add_space(8.0);
    ui.separator();
    ui.add_space(12.0);

    match state.current_tab {
        DashboardTab::Overview => {
            render_overview_grid(ui, clusters, status_state, hover_effects);
        }
        DashboardTab::Detailed => {
            render_detailed_view(ui, clusters, state, status_state);
        }
    }
}

fn render_overview_grid(
    ui: &mut Ui,
    clusters: &[ClusterConfig],
    status_state: &crate::modules::status::StatusState,
    hover_effects: bool,
) {
    let min_card_width = 320.0;
    let card_spacing = 16.0;
    let available_width = ui.available_width();
    let cols = if available_width >= min_card_width * 2.0 + card_spacing {
        2
    } else {
        1
    };
    let col_width = (available_width - (cols - 1) as f32 * card_spacing) / cols as f32;

    egui::ScrollArea::vertical()
        .id_salt("dashboard_overview")
        .show(ui, |ui| {
            if clusters.is_empty() {
                ui.label(
                    egui::RichText::new("No matching clusters configured or matching filter.")
                        .color(Theme::text_muted())
                        .size(13.0),
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
                                    render_overview_card(
                                        ui,
                                        cluster,
                                        status_state,
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

fn render_overview_card(
    ui: &mut Ui,
    config: &ClusterConfig,
    status_state: &crate::modules::status::StatusState,
    col_width: f32,
    hover_effects: bool,
) {
    let name = &config.name;
    let health = status_state
        .health_data
        .iter()
        .find(|(n, _)| n == name)
        .and_then(|(_, h)| h.clone());
    let stats = status_state
        .stats_data
        .iter()
        .find(|(n, _)| n == name)
        .and_then(|(_, s)| s.clone());
    let error = status_state.errors.get(name).cloned();

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
            let dot_color = match health {
                Some(ref h) => match h.status.as_str() {
                    "green" => Theme::success(),
                    "yellow" => Theme::warning(),
                    "red" => Theme::danger(),
                    _ => Theme::text_muted(),
                },
                None => Theme::text_muted(),
            };
            ui.add(ConnectionDot::new(connected).color(dot_color).size(10.0));
            ui.vertical(|ui| {
                ui.label(
                    egui::RichText::new(name)
                        .strong()
                        .size(15.0)
                        .color(Theme::text_primary()),
                );
                ui.label(
                    egui::RichText::new(&config.host)
                        .size(10.0)
                        .color(Theme::text_muted())
                        .monospace(),
                );
            });

            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                if let Some(ref h) = health {
                    let color = Theme::health_color(&h.status);
                    ui.add(StatePill::new(&h.status, color));
                } else {
                    ui.add(StatePill::new("Offline", Theme::danger()));
                }
            });
        });
        ui.add_space(8.0);

        if let Some(ref err) = error {
            ui.colored_label(Theme::danger(), format!("⚠️ {}", err));
            return;
        }

        if let Some(ref h) = health {
            ui.horizontal(|ui| {
                ui.label(
                    egui::RichText::new(format!("Nodes: {}", h.number_of_nodes))
                        .size(11.0)
                        .color(Theme::text_primary()),
                );
                ui.label(
                    egui::RichText::new("|")
                        .size(11.0)
                        .color(Theme::text_muted()),
                );
                ui.label(
                    egui::RichText::new(format!("Shards: {}", h.active_shards))
                        .size(11.0)
                        .color(Theme::text_primary()),
                );
                if h.unassigned_shards > 0 {
                    ui.label(
                        egui::RichText::new("|")
                            .size(11.0)
                            .color(Theme::text_muted()),
                    );
                    ui.label(
                        egui::RichText::new(format!("⚠️ {} Unassigned", h.unassigned_shards))
                            .size(11.0)
                            .color(Theme::danger())
                            .strong(),
                    );
                }
            });
            ui.add_space(6.0);

            if let Some(ref s) = stats {
                if let Some(ref indices) = s.indices {
                    ui.horizontal(|ui| {
                        ui.label(
                            egui::RichText::new(format!("Indices: {}", indices.count))
                                .size(11.0)
                                .color(Theme::text_secondary()),
                        );
                        if let Some(ref docs) = indices.docs {
                            ui.label(
                                egui::RichText::new("|")
                                    .size(11.0)
                                    .color(Theme::text_muted()),
                            );
                            ui.label(
                                egui::RichText::new(format!("Docs: {}", human_docs(docs.count)))
                                    .size(11.0)
                                    .color(Theme::text_secondary()),
                            );
                        }
                        if let Some(ref store) = indices.store {
                            ui.label(
                                egui::RichText::new("|")
                                    .size(11.0)
                                    .color(Theme::text_muted()),
                            );
                            ui.label(
                                egui::RichText::new(format!(
                                    "Size: {}",
                                    human_bytes(store.size_in_bytes)
                                ))
                                .size(11.0)
                                .color(Theme::text_secondary()),
                            );
                        }
                    });
                }

                // JVM heap usage preview bar
                if let Some(ref nodes_stats) = s.nodes
                    && let Some(ref jvm) = nodes_stats.jvm
                    && let Some(ref mem) = jvm.mem
                {
                    let ratio = if mem.heap_max_in_bytes > 0 {
                        mem.heap_used_in_bytes as f32 / mem.heap_max_in_bytes as f32
                    } else {
                        0.0
                    };
                    ui.add_space(6.0);
                    ui.label(
                        egui::RichText::new(format!(
                            "JVM Heap Overall: {} / {} ({:.1}%)",
                            human_bytes(mem.heap_used_in_bytes),
                            human_bytes(mem.heap_max_in_bytes),
                            ratio * 100.0
                        ))
                        .size(10.0)
                        .color(Theme::text_muted()),
                    );
                    ui.add(GradientProgressBar::new(ratio).height(6.0));
                }
            }
        } else {
            ui.colored_label(Theme::text_muted(), "No status data loaded yet.");
        }
    });

    if hover_effects {
        let rect = response.response.rect;
        let hovered = response.response.hovered();
        let glow_alpha = ui.ctx().animate_value_with_time(
            ui.id().with(name).with("hover_glow_dash"),
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

fn render_detailed_view(
    ui: &mut Ui,
    clusters: &[ClusterConfig],
    state: &mut DashboardState,
    status_state: &crate::modules::status::StatusState,
) {
    if clusters.is_empty() {
        ui.label(
            egui::RichText::new("No matching clusters configured or matching filter.")
                .color(Theme::text_muted())
                .size(13.0),
        );
        return;
    }

    // Ensure selected_cluster matches the filtered list, otherwise default to first available
    let cluster_names: Vec<String> = clusters.iter().map(|c| c.name.clone()).collect();
    if state.selected_cluster.is_empty() || !cluster_names.contains(&state.selected_cluster) {
        state.selected_cluster = cluster_names.first().cloned().unwrap_or_default();
    }

    // Top Selector Control
    ui.horizontal(|ui| {
        ui.label(egui::RichText::new("Select Cluster:").strong().size(12.0));
        egui::ComboBox::from_id_salt("dashboard_cluster_select")
            .selected_text(&state.selected_cluster)
            .width(200.0)
            .show_ui(ui, |ui| {
                for name in &cluster_names {
                    ui.selectable_value(&mut state.selected_cluster, name.clone(), name);
                }
            });

        // Trigger manual refresh quick link/button
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            if ui.button("🔄 Refresh Data").clicked() {
                ui.ctx().request_repaint();
            }
        });
    });
    ui.add_space(16.0);

    let active_name = &state.selected_cluster;
    let health = status_state
        .health_data
        .iter()
        .find(|(n, _)| n == active_name)
        .and_then(|(_, h)| h.clone());
    let stats = status_state
        .stats_data
        .iter()
        .find(|(n, _)| n == active_name)
        .and_then(|(_, s)| s.clone());
    let error = status_state.errors.get(active_name).cloned();
    let es_version = status_state.es_versions.get(active_name).cloned();
    let kibana_version = status_state.kibana_versions.get(active_name).cloned();
    let allocations = status_state.allocations.get(active_name).cloned();
    let nodes_list = status_state.nodes.get(active_name).cloned();

    egui::ScrollArea::vertical()
        .id_salt("dashboard_detailed")
        .show(ui, |ui| {
            if let Some(err) = error {
                ui.colored_label(Theme::danger(), format!("Error communicating with cluster: {}", err));
                return;
            }

            if health.is_none() {
                ui.label(
                    egui::RichText::new("Cluster status unreachable or currently offline.")
                        .color(Theme::danger())
                        .size(13.0),
                );
                return;
            }

            let h = health.unwrap();

            // --- 1. OVERVIEW WIDGET ---
            egui::Frame::new()
                .fill(Theme::bg_card())
                .corner_radius(Theme::CARD_ROUNDING)
                .inner_margin(Theme::CARD_PADDING)
                .stroke(egui::Stroke::new(1.0, Theme::bg_input()))
                .show(ui, |ui| {
                    ui.horizontal(|ui| {
                        ui.heading(format!("Dashboard: {}", active_name));
                        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                            let health_color = Theme::health_color(&h.status);
                            ui.add(StatePill::new(&h.status, health_color));
                        });
                    });
                    ui.add_space(4.0);
                    ui.horizontal(|ui| {
                        if let Some(ref es_v) = es_version {
                            ui.label(egui::RichText::new(format!("ES Version: v{}", es_v)).monospace().size(11.0).color(Theme::text_secondary()));
                        }
                        if let Some(ref kb_v) = kibana_version {
                            ui.label(egui::RichText::new("|").size(11.0).color(Theme::text_muted()));
                            ui.label(egui::RichText::new(format!("Kibana: v{}", kb_v)).monospace().size(11.0).color(Theme::text_secondary()));
                        }
                    });

                    ui.add_space(8.0);
                    ui.separator();
                    ui.add_space(8.0);

                    // Grid layout of metrics
                    ui.horizontal(|ui| {
                        let width_box = ui.available_width() / 4.0 - 12.0;

                        // Nodes Box
                        ui.allocate_ui(egui::Vec2::new(width_box, 60.0), |ui| {
                            ui.vertical(|ui| {
                                ui.label(egui::RichText::new("Total Nodes").color(Theme::text_muted()).size(10.5));
                                ui.label(egui::RichText::new(format!("{}", h.number_of_nodes)).strong().size(22.0).color(Theme::text_primary()));
                                ui.label(egui::RichText::new(format!("Data nodes: {}", h.number_of_data_nodes)).color(Theme::text_muted()).size(10.0));
                            });
                        });
                        ui.separator();

                        // Shards Box
                        ui.allocate_ui(egui::Vec2::new(width_box, 60.0), |ui| {
                            ui.vertical(|ui| {
                                ui.label(egui::RichText::new("Active / Primary Shards").color(Theme::text_muted()).size(10.5));
                                ui.label(egui::RichText::new(format!("{} / {}", h.active_shards, h.active_primary_shards)).strong().size(22.0).color(Theme::text_primary()));
                                if h.unassigned_shards > 0 {
                                    ui.label(egui::RichText::new(format!("⚠️ {} Unassigned", h.unassigned_shards)).strong().color(Theme::danger()).size(10.0));
                                } else {
                                    ui.label(egui::RichText::new("All shards assigned").color(Theme::success()).size(10.0));
                                }
                            });
                        });
                        ui.separator();

                        // Doc count Box
                        if let Some(ref s) = stats
                            && let Some(ref indices) = s.indices {
                                ui.allocate_ui(egui::Vec2::new(width_box, 60.0), |ui| {
                                    ui.vertical(|ui| {
                                        ui.label(egui::RichText::new("Documents Count").color(Theme::text_muted()).size(10.5));
                                        if let Some(ref docs) = indices.docs {
                                            ui.label(egui::RichText::new(human_docs(docs.count)).strong().size(22.0).color(Theme::text_primary()));
                                            ui.label(egui::RichText::new(format!("Deleted: {}", human_docs(docs.deleted))).color(Theme::text_muted()).size(10.0));
                                        } else {
                                            ui.label(egui::RichText::new("—").strong().size(22.0).color(Theme::text_primary()));
                                        }
                                    });
                                });
                                ui.separator();

                                // Store Size Box
                                ui.allocate_ui(egui::Vec2::new(width_box, 60.0), |ui| {
                                    ui.vertical(|ui| {
                                        ui.label(egui::RichText::new("Total Store Size").color(Theme::text_muted()).size(10.5));
                                        if let Some(ref store) = indices.store {
                                            ui.label(egui::RichText::new(human_bytes(store.size_in_bytes)).strong().size(22.0).color(Theme::text_primary()));
                                        } else {
                                            ui.label(egui::RichText::new("—").strong().size(22.0).color(Theme::text_primary()));
                                        }
                                        ui.label(egui::RichText::new(format!("Indices: {}", indices.count)).color(Theme::text_muted()).size(10.0));
                                    });
                                });
                            }
                    });
                });
            ui.add_space(16.0);

            // --- 2. NODE LIST TABLE ---
            ui.heading("💻 Node List (Detail)");
            ui.add_space(8.0);
            egui::Frame::new()
                .fill(Theme::bg_card())
                .corner_radius(Theme::CARD_ROUNDING)
                .inner_margin(Theme::CARD_PADDING)
                .stroke(egui::Stroke::new(1.0, Theme::bg_input()))
                .show(ui, |ui| {
                    if let Some(ref nodes) = nodes_list {
                        if nodes.is_empty() {
                            ui.label(egui::RichText::new("No nodes information returned from ES.").color(Theme::text_muted()));
                        } else {
                            // Table headers
                            egui::Grid::new("nodes_grid_detailed")
                                .num_columns(7)
                                .spacing([16.0, 10.0])
                                .striped(true)
                                .show(ui, |ui| {
                                    // Headers
                                    ui.label(egui::RichText::new("Name").strong().color(Theme::text_secondary()).size(11.0));
                                    ui.label(egui::RichText::new("Role").strong().color(Theme::text_secondary()).size(11.0));
                                    ui.label(egui::RichText::new("CPU").strong().color(Theme::text_secondary()).size(11.0));
                                    ui.label(egui::RichText::new("RAM").strong().color(Theme::text_secondary()).size(11.0));
                                    ui.label(egui::RichText::new("JVM Heap").strong().color(Theme::text_secondary()).size(11.0));
                                    ui.label(egui::RichText::new("IP Address").strong().color(Theme::text_secondary()).size(11.0));
                                    ui.label(egui::RichText::new("Status").strong().color(Theme::text_secondary()).size(11.0));
                                    ui.end_row();

                                    for node in nodes {
                                        let name = node.name.as_deref().unwrap_or("Unknown");
                                        let role = node.role.as_deref().unwrap_or("—");
                                        let cpu = node.cpu.as_deref().unwrap_or("0");
                                        let ram = node.ram_percent.as_deref().unwrap_or("0");
                                        let heap = node.heap_percent.as_deref().unwrap_or("0");
                                        let ip = node.ip.as_deref().unwrap_or("—");
                                        let is_master = node.master.as_deref().unwrap_or("-") == "*";

                                        ui.horizontal(|ui| {
                                            ui.label(egui::RichText::new(name).strong().color(Theme::text_primary()).size(11.0));
                                        });

                                        ui.label(egui::RichText::new(role).monospace().color(Theme::text_muted()).size(10.5));

                                        // CPU percentage pill/progress
                                        let cpu_val = cpu.parse::<f32>().unwrap_or(0.0) / 100.0;
                                        ui.horizontal(|ui| {
                                            ui.add(GradientProgressBar::new(cpu_val).height(5.0).width(50.0));
                                            ui.label(egui::RichText::new(format!("{}%", cpu)).size(10.0).monospace().color(Theme::text_primary()));
                                        });

                                        // RAM percentage
                                        let ram_val = ram.parse::<f32>().unwrap_or(0.0) / 100.0;
                                        ui.horizontal(|ui| {
                                            ui.add(GradientProgressBar::new(ram_val).height(5.0).width(50.0));
                                            ui.label(egui::RichText::new(format!("{}%", ram)).size(10.0).monospace().color(Theme::text_primary()));
                                        });

                                        // JVM Heap percentage
                                        let heap_val = heap.parse::<f32>().unwrap_or(0.0) / 100.0;
                                        ui.horizontal(|ui| {
                                            ui.add(GradientProgressBar::new(heap_val).height(5.0).width(50.0));
                                            ui.label(egui::RichText::new(format!("{}%", heap)).size(10.0).monospace().color(Theme::text_primary()));
                                        });

                                        ui.label(egui::RichText::new(ip).monospace().color(Theme::text_secondary()).size(10.5));

                                        if is_master {
                                            ui.add(StatePill::new("👑 Master", Theme::warning()));
                                        } else {
                                            ui.add(StatePill::new("Node", Theme::text_muted()));
                                        }

                                        ui.end_row();
                                    }
                                });
                        }
                    } else {
                        ui.label(egui::RichText::new("Nodes list loading or query failed. Check connection or wait for auto-refresh.").color(Theme::text_muted()));
                    }
                });
            ui.add_space(16.0);

            // --- 3. SHARD ALLOCATION & DISK STORAGE ---
            ui.heading("💾 Shards Allocation & Disk Storage");
            ui.add_space(8.0);
            egui::Frame::new()
                .fill(Theme::bg_card())
                .corner_radius(Theme::CARD_ROUNDING)
                .inner_margin(Theme::CARD_PADDING)
                .stroke(egui::Stroke::new(1.0, Theme::bg_input()))
                .show(ui, |ui| {
                    if let Some(allocs) = &allocations {
                        let data_nodes: Vec<_> = allocs.iter()
                            .filter(|a| a.node.as_deref().unwrap_or("UNASSIGNED") != "UNASSIGNED")
                            .collect();

                        let unassigned_nodes: Vec<_> = allocs.iter()
                            .filter(|a| a.node.as_deref().unwrap_or("UNASSIGNED") == "UNASSIGNED")
                            .collect();

                        if data_nodes.is_empty() && unassigned_nodes.is_empty() {
                            ui.label(egui::RichText::new("No shard allocation data available.").color(Theme::text_muted()));
                        } else {
                            egui::Grid::new("shard_allocations_dashboard")
                                .num_columns(5)
                                .spacing([20.0, 10.0])
                                .striped(true)
                                .show(ui, |ui| {
                                    ui.label(egui::RichText::new("Node").strong().color(Theme::text_secondary()).size(11.0));
                                    ui.label(egui::RichText::new("Shards").strong().color(Theme::text_secondary()).size(11.0));
                                    ui.label(egui::RichText::new("Disk Free / Total").strong().color(Theme::text_secondary()).size(11.0));
                                    ui.label(egui::RichText::new("Disk Usage").strong().color(Theme::text_secondary()).size(11.0));
                                    ui.label(egui::RichText::new("Disk Bar").strong().color(Theme::text_secondary()).size(11.0));
                                    ui.end_row();

                                    for node in &data_nodes {
                                        let node_name = node.node.as_deref().unwrap_or("Unknown");
                                        let shards_str = node.shards.as_deref().unwrap_or("0");
                                        let avail = node.disk_avail.as_deref().unwrap_or("—");
                                        let total = node.disk_total.as_deref().unwrap_or("—");
                                        let percent_str = node.disk_percent.as_deref().unwrap_or("0");

                                        let percent_val = percent_str.parse::<f32>().unwrap_or(0.0) / 100.0;

                                        ui.label(egui::RichText::new(node_name).strong().color(Theme::text_primary()).size(11.0));
                                        ui.label(egui::RichText::new(shards_str).monospace().color(Theme::text_primary()).size(11.0));
                                        ui.label(egui::RichText::new(format!("{} / {}", avail, total)).monospace().color(Theme::text_secondary()).size(10.5));
                                        ui.label(egui::RichText::new(format!("{}%", percent_str)).monospace().color(Theme::text_primary()).size(10.5));
                                        ui.add(GradientProgressBar::new(percent_val).height(5.0).width(80.0));
                                        ui.end_row();
                                    }

                                    for node in &unassigned_nodes {
                                        let shards_str = node.shards.as_deref().unwrap_or("0");
                                        ui.label(egui::RichText::new("UNASSIGNED").strong().color(Theme::danger()).size(11.0));
                                        ui.label(egui::RichText::new(shards_str).monospace().color(Theme::danger()).size(11.0));
                                        ui.label(egui::RichText::new("—").color(Theme::text_muted()).size(10.5));
                                        ui.label(egui::RichText::new("—").color(Theme::text_muted()).size(10.5));
                                        ui.label(egui::RichText::new("—").color(Theme::text_muted()).size(10.5));
                                        ui.end_row();
                                    }
                                });
                        }
                    } else {
                        ui.label(egui::RichText::new("Allocation statistics currently loading...").color(Theme::text_muted()));
                    }
                });
            ui.add_space(16.0);
        });
}
