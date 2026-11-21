use std::collections::HashMap;

use crate::core::config::{ClusterConfig, ClusterData};
use crate::modules::clusters::{
    ClustersState, render_edit_form, render_export_section, render_import_section,
    render_test_result,
};
use crate::modules::telemetry::{
    ClusterLogViewerState, ClusterMetricsViewerState, render_log_viewer_panel,
    render_metrics_viewer_panel,
};
use crate::ui::theme::Theme;
use crate::ui::widgets::{
    ConnectionDot, GradientProgressBar, StatePill, draw_hud_brackets, human_bytes, human_docs,
    open_link,
};
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
    pub open_log_viewer_cluster: Option<String>,
    pub open_metrics_viewer_cluster: Option<String>,
    pub log_viewers: HashMap<String, ClusterLogViewerState>,
    pub metrics_viewers: HashMap<String, ClusterMetricsViewerState>,
}

impl Default for DashboardState {
    fn default() -> Self {
        Self {
            selected_cluster: String::new(),
            current_tab: DashboardTab::Overview,
            open_log_viewer_cluster: None,
            open_metrics_viewer_cluster: None,
            log_viewers: HashMap::new(),
            metrics_viewers: HashMap::new(),
        }
    }
}

#[derive(Debug, Default)]
pub struct DashboardActions {
    pub add_cluster: bool,
    pub toggle_enabled: Option<(String, bool)>,
    pub save_cluster: Option<(Option<String>, ClusterConfig, String)>,
    pub delete_cluster: Option<String>,
    pub test_cluster: Option<(ClusterConfig, String)>,
    pub import_config: Option<crate::core::config::AppConfig>,
    pub fetch_repos: Option<String>,
    pub fetch_slm: Option<String>,
    pub hot_threads: Option<(String, String)>,
    pub show_pending: Option<String>,
    pub manual_refresh: bool,
    pub fetch_logs: Vec<String>,
    pub fetch_metrics: Vec<String>,
}

#[allow(clippy::too_many_arguments)]
pub fn render_dashboard_module(
    ui: &mut Ui,
    clusters: &[ClusterConfig],
    all_cluster_names: &[String],
    cluster_data: &HashMap<String, ClusterData>,
    state: &mut DashboardState,
    clusters_state: &mut ClustersState,
    status_state: &crate::modules::status::StatusState,
    actions: &mut DashboardActions,
    hover_effects: bool,
) {
    ui.horizontal_wrapped(|ui| {
        ui.heading("Cluster Status & Dashboard");
        ui.add_space(12.0);
        if ui.button("➕ Add Cluster").clicked() {
            actions.add_cluster = true;
        }
        if ui.button("📥 Import").clicked() {
            clusters_state.show_import = !clusters_state.show_import;
            clusters_state.show_export = false;
        }
        if ui.button("📤 Export").clicked() {
            clusters_state.show_export = !clusters_state.show_export;
            clusters_state.show_import = false;
        }

        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            for (label, tab) in [
                ("Detailed Single Cluster", DashboardTab::Detailed),
                ("Overview Grid", DashboardTab::Overview),
            ] {
                let is_active = state.current_tab == tab;
                let accent = Theme::accent();
                let text = egui::RichText::new(label).size(13.0);
                let (text, fill) = if is_active {
                    (text.color(accent).strong(), accent.linear_multiply(0.14))
                } else {
                    (text.color(Theme::text_secondary()), Color32::TRANSPARENT)
                };
                let resp = ui.add(
                    egui::Button::new(text)
                        .fill(fill)
                        .stroke(egui::Stroke::NONE)
                        .corner_radius(4.0),
                );
                if is_active {
                    let r = resp.rect;
                    ui.painter().line_segment(
                        [
                            egui::Pos2::new(r.left() + 4.0, r.bottom() - 1.0),
                            egui::Pos2::new(r.right() - 4.0, r.bottom() - 1.0),
                        ],
                        egui::Stroke::new(2.0_f32, accent),
                    );
                }
                if resp.clicked() {
                    state.current_tab = tab;
                }
            }
        });
    });
    ui.add_space(6.0);
    ui.separator();
    ui.add_space(10.0);

    if clusters_state.show_import {
        render_import_section(ui, clusters_state, clusters, &mut actions.import_config);
        ui.add_space(12.0);
    }
    if clusters_state.show_export {
        render_export_section(ui, clusters_state, clusters, cluster_data);
        ui.add_space(12.0);
    }

    match state.current_tab {
        DashboardTab::Overview => {
            render_overview_grid(ui, clusters, state, status_state, actions, hover_effects);
        }
        DashboardTab::Detailed => {
            render_detailed_view(
                ui,
                clusters,
                all_cluster_names,
                state,
                clusters_state,
                status_state,
                actions,
            );
        }
    }
}

fn render_overview_grid(
    ui: &mut Ui,
    clusters: &[ClusterConfig],
    state: &mut DashboardState,
    status_state: &crate::modules::status::StatusState,
    actions: &mut DashboardActions,
    hover_effects: bool,
) {
    let min_card_width = 380.0;
    let card_spacing = 14.0;
    let available_width = (ui.available_width() - 16.0).max(240.0);
    let cols = if available_width >= min_card_width * 2.0 + card_spacing {
        2
    } else {
        1
    };
    let col_width = ((available_width - (cols - 1) as f32 * card_spacing) / cols as f32).floor();

    if clusters.is_empty() {
        ui.label(
            egui::RichText::new(
                "No clusters configured or matching filter. Click '➕ Add Cluster' above to add one.",
            )
            .color(Theme::text_muted())
            .size(13.0),
        );
        return;
    }

    ui.horizontal(|ui| {
        ui.spacing_mut().item_spacing.x = card_spacing;
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
                                state,
                                status_state,
                                actions,
                                col_width,
                                hover_effects,
                            );
                            ui.add_space(card_spacing);
                        }
                    }
                },
            );
        }
    });
}

fn render_kpi_tile(ui: &mut Ui, width: f32, label: &str, value: &str, accent: Color32) {
    egui::Frame::new()
        .fill(Theme::bg_input().linear_multiply(0.55))
        .corner_radius(5.0)
        .inner_margin(egui::Margin::symmetric(8, 6))
        .show(ui, |ui| {
            ui.set_min_width((width - 16.0).max(40.0));
            ui.set_max_width((width - 16.0).max(40.0));
            ui.label(
                egui::RichText::new(label)
                    .size(9.5)
                    .color(Theme::text_muted()),
            );
            ui.label(
                egui::RichText::new(value)
                    .size(13.5)
                    .strong()
                    .monospace()
                    .color(accent),
            );
        });
}

fn render_host_links(ui: &mut Ui, config: &ClusterConfig) {
    ui.horizontal_wrapped(|ui| {
        let clean_url = |url: &str| -> String {
            url.trim_start_matches("http://")
                .trim_start_matches("https://")
                .to_string()
        };
        let normalize_url = |raw: &str| -> String {
            if raw.starts_with("http://") || raw.starts_with("https://") {
                raw.to_string()
            } else {
                format!("http://{}", raw)
            }
        };
        let es_url = normalize_url(&config.host);
        if ui
            .add(egui::Link::new(
                egui::RichText::new(format!("ES: {}", clean_url(&es_url)))
                    .size(10.0)
                    .monospace()
                    .color(Theme::accent()),
            ))
            .clicked()
        {
            open_link(ui.ctx(), &es_url);
        }

        let kb_raw = if config.kibana_host.is_empty() {
            if config.host.contains("elastic") {
                config.host.replace("elastic", "kibana")
            } else {
                String::new()
            }
        } else {
            config.kibana_host.clone()
        };
        if !kb_raw.is_empty() {
            let kb_url = normalize_url(&kb_raw);
            ui.label(
                egui::RichText::new("•")
                    .size(10.0)
                    .color(Theme::text_muted()),
            );
            if ui
                .add(egui::Link::new(
                    egui::RichText::new(format!("KB: {}", clean_url(&kb_url)))
                        .size(10.0)
                        .monospace()
                        .color(Theme::accent()),
                ))
                .clicked()
            {
                open_link(ui.ctx(), &kb_url);
            }
        }

        if !config.haproxy_host.is_empty() {
            let ha_url = normalize_url(&config.haproxy_host);
            ui.label(
                egui::RichText::new("•")
                    .size(10.0)
                    .color(Theme::text_muted()),
            );
            if ui
                .add(egui::Link::new(
                    egui::RichText::new(format!("HA: {}", clean_url(&ha_url)))
                        .size(10.0)
                        .monospace()
                        .color(Theme::accent()),
                ))
                .clicked()
            {
                open_link(ui.ctx(), &ha_url);
            }
        }

        ui.label(
            egui::RichText::new("•")
                .size(10.0)
                .color(Theme::text_muted()),
        );
        ui.label(
            egui::RichText::new(if config.ssh_tunnel {
                "API: overSSH"
            } else {
                "API: Direct"
            })
            .size(10.0)
            .monospace()
            .strong()
            .color(Theme::text_secondary()),
        );
    });
}

fn render_enabled_checkbox(ui: &mut Ui, name: &str, enabled: bool, actions: &mut DashboardActions) {
    let mut flag = enabled;
    if !enabled {
        // Highlighted tick box so disabled (greyed-out) clusters can easily be re-enabled.
        egui::Frame::new()
            .fill(Theme::accent().linear_multiply(0.18))
            .stroke(egui::Stroke::new(1.5_f32, Theme::accent()))
            .corner_radius(4.0)
            .inner_margin(egui::Margin::symmetric(6, 2))
            .show(ui, |ui| {
                if ui
                    .checkbox(
                        &mut flag,
                        egui::RichText::new("Enabled")
                            .strong()
                            .size(11.0)
                            .color(Theme::accent()),
                    )
                    .on_hover_text("Enable cluster monitoring & auto-refresh")
                    .changed()
                {
                    actions.toggle_enabled = Some((name.to_string(), flag));
                }
            });
    } else if ui
        .checkbox(
            &mut flag,
            egui::RichText::new("Enabled")
                .size(11.0)
                .color(Theme::text_secondary()),
        )
        .on_hover_text("Disable to pause status, snapshot & task polling for this cluster")
        .changed()
    {
        actions.toggle_enabled = Some((name.to_string(), flag));
    }
}

fn open_log_viewer_for(state: &mut DashboardState, name: &str, actions: &mut DashboardActions) {
    state.open_log_viewer_cluster = Some(name.to_string());
    let viewer = state.log_viewers.entry(name.to_string()).or_default();
    if !viewer.has_loaded && !viewer.is_loading {
        viewer.is_loading = true;
        actions.fetch_logs.push(name.to_string());
    }
}

fn open_metrics_viewer_for(state: &mut DashboardState, name: &str, actions: &mut DashboardActions) {
    state.open_metrics_viewer_cluster = Some(name.to_string());
    let viewer = state.metrics_viewers.entry(name.to_string()).or_default();
    if !viewer.has_loaded && !viewer.is_loading {
        viewer.is_loading = true;
        actions.fetch_metrics.push(name.to_string());
    }
}

#[allow(clippy::too_many_arguments)]
fn render_overview_card(
    ui: &mut Ui,
    config: &ClusterConfig,
    state: &mut DashboardState,
    status_state: &crate::modules::status::StatusState,
    actions: &mut DashboardActions,
    col_width: f32,
    hover_effects: bool,
) {
    let name = &config.name;
    let is_enabled = config.enabled;
    let health = if is_enabled {
        status_state
            .health_data
            .iter()
            .find(|(n, _)| n == name)
            .and_then(|(_, h)| h.clone())
    } else {
        None
    };
    let stats = if is_enabled {
        status_state
            .stats_data
            .iter()
            .find(|(n, _)| n == name)
            .and_then(|(_, s)| s.clone())
    } else {
        None
    };
    let error = if is_enabled {
        status_state.errors.get(name).cloned()
    } else {
        None
    };
    let explain = if is_enabled {
        status_state.explains.get(name).cloned().flatten()
    } else {
        None
    };
    let es_version = status_state.es_versions.get(name).cloned();
    let kibana_version = status_state.kibana_versions.get(name).cloned();
    let allocations = if is_enabled {
        status_state.allocations.get(name).cloned()
    } else {
        None
    };
    let pending_tasks = if is_enabled {
        status_state.pending_tasks.get(name).cloned()
    } else {
        None
    };

    let card_fill = if is_enabled {
        Theme::bg_card()
    } else {
        Theme::bg_card().linear_multiply(0.45)
    };
    let card_stroke = if is_enabled {
        egui::Stroke::new(1.0_f32, Theme::accent().linear_multiply(0.22))
    } else {
        egui::Stroke::new(1.0_f32, Theme::border().linear_multiply(0.45))
    };

    let frame = egui::Frame::new()
        .fill(card_fill)
        .corner_radius(Theme::CARD_ROUNDING)
        .inner_margin(Theme::CARD_PADDING)
        .stroke(card_stroke);

    let response = frame.show(ui, |ui| {
        let inner_w = (col_width - Theme::CARD_PADDING.x * 2.0 - 4.0).max(200.0);
        ui.set_min_width(inner_w);
        ui.set_max_width(inner_w);

        // Header
        ui.horizontal(|ui| {
            let connected = is_enabled && health.is_some();
            let dot_color = if !is_enabled {
                Theme::text_muted()
            } else {
                match health {
                    Some(ref h) => match h.status.as_str() {
                        "green" => Theme::success(),
                        "yellow" => Theme::warning(),
                        "red" => Theme::danger(),
                        _ => Theme::text_muted(),
                    },
                    None => Theme::text_muted(),
                }
            };
            ui.add(ConnectionDot::new(connected).color(dot_color).size(10.0));
            ui.vertical(|ui| {
                ui.label(
                    egui::RichText::new(name)
                        .strong()
                        .size(16.0)
                        .color(if is_enabled {
                            Theme::text_primary()
                        } else {
                            Theme::text_muted()
                        }),
                );
                if is_enabled {
                    ui.horizontal(|ui| {
                        if let Some(ref es_v) = es_version {
                            ui.label(
                                egui::RichText::new(format!("ES v{}", es_v))
                                    .size(10.0)
                                    .color(Theme::accent())
                                    .monospace(),
                            );
                        }
                        if let Some(ref kb_v) = kibana_version {
                            if es_version.is_some() {
                                ui.label(
                                    egui::RichText::new("|")
                                        .size(10.0)
                                        .color(Theme::text_muted()),
                                );
                            }
                            ui.label(
                                egui::RichText::new(format!("KB v{}", kb_v))
                                    .size(10.0)
                                    .color(Theme::text_secondary())
                                    .monospace(),
                            );
                        }
                    });
                }
            });

            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                if !is_enabled {
                    ui.add(StatePill::new("Disabled", Theme::text_muted()));
                } else if let Some(ref h) = health {
                    let color = Theme::health_color(&h.status);
                    ui.add(StatePill::new(&h.status, color));
                } else {
                    ui.add(StatePill::new("Offline", Theme::danger()));
                }

                if ui
                    .small_button("Inspect >")
                    .on_hover_text("Open detailed nodes, shards & indices view")
                    .clicked()
                {
                    state.selected_cluster = name.clone();
                    state.current_tab = DashboardTab::Detailed;
                }

                render_enabled_checkbox(ui, name, is_enabled, actions);

                if let Some(ref tasks) = pending_tasks
                    && !tasks.is_empty()
                {
                    let danger_color = Theme::danger();
                    let btn = ui
                        .add(
                            egui::Button::new(
                                egui::RichText::new(format!("⚠ {} Pending", tasks.len()))
                                    .size(10.0)
                                    .strong()
                                    .color(Theme::contrast_text_color(danger_color)),
                            )
                            .fill(danger_color)
                            .corner_radius(4.0),
                        )
                        .on_hover_text(
                            "Delayed master metadata updates/actions. Click to inspect pending task queue.",
                        );
                    if btn.clicked() {
                        actions.show_pending = Some(name.to_string());
                    }
                }
            });
        });

        if !is_enabled {
            ui.add_space(6.0);
            ui.label(
                egui::RichText::new(&config.host)
                    .size(10.0)
                    .monospace()
                    .color(Theme::text_muted()),
            );
            ui.add_space(4.0);
            ui.label(
                egui::RichText::new("Cluster disabled — tick Enabled above to resume updates.")
                    .size(11.0)
                    .color(Theme::text_muted()),
            );
            ui.add_space(8.0);
            ui.horizontal(|ui| {
                if ui
                    .add(
                        egui::Button::new(
                            egui::RichText::new("📜 Log Viewer")
                                .size(11.0)
                                .color(Theme::text_muted()),
                        )
                        .fill(Theme::bg_input().linear_multiply(0.5)),
                    )
                    .clicked()
                {
                    open_log_viewer_for(state, name, actions);
                }
                if ui
                    .add(
                        egui::Button::new(
                            egui::RichText::new("📈 Metrics Viewer")
                                .size(11.0)
                                .color(Theme::text_muted()),
                        )
                        .fill(Theme::bg_input().linear_multiply(0.5)),
                    )
                    .clicked()
                {
                    open_metrics_viewer_for(state, name, actions);
                }
            });
            return;
        }

        // Quick host links row
        ui.add_space(4.0);
        render_host_links(ui, config);
        ui.add_space(8.0);

        if let Some(ref err) = error {
            ui.colored_label(Theme::danger(), format!("⚠ {}", err));
            ui.add_space(6.0);
        }

        if let Some(ref h) = health {
            let indices_str = stats
                .as_ref()
                .and_then(|s| s.indices.as_ref())
                .map(|i| i.count.to_string())
                .unwrap_or_else(|| "—".to_string());
            let docs_str = stats
                .as_ref()
                .and_then(|s| s.indices.as_ref())
                .and_then(|i| i.docs.as_ref())
                .map(|d| human_docs(d.count))
                .unwrap_or_else(|| "—".to_string());
            let store_str = stats
                .as_ref()
                .and_then(|s| s.indices.as_ref())
                .and_then(|i| i.store.as_ref())
                .map(|st| human_bytes(st.size_in_bytes))
                .unwrap_or_else(|| "—".to_string());

            let tile_gap = 6.0;
            let tile_w = ((inner_w - tile_gap * 3.0) / 4.0).floor();
            ui.horizontal(|ui| {
                ui.spacing_mut().item_spacing.x = tile_gap;
                render_kpi_tile(
                    ui,
                    tile_w,
                    "NODES",
                    &h.number_of_nodes.to_string(),
                    Theme::text_primary(),
                );
                render_kpi_tile(
                    ui,
                    tile_w,
                    "SHARDS",
                    &h.active_shards.to_string(),
                    if h.unassigned_shards > 0 {
                        Theme::warning()
                    } else {
                        Theme::text_primary()
                    },
                );
                render_kpi_tile(ui, tile_w, "INDICES", &indices_str, Theme::text_primary());
                render_kpi_tile(ui, tile_w, "STORE", &store_str, Theme::accent());
            });

            ui.add_space(6.0);
            ui.label(
                egui::RichText::new(format!("Docs: {}", docs_str))
                    .size(11.0)
                    .monospace()
                    .color(Theme::text_secondary()),
            );
            ui.add_space(6.0);
            ui.horizontal(|ui| {
                ui.spacing_mut().item_spacing.x = tile_gap;
                for (label, value, color) in [
                    (
                        "UNASSIGNED",
                        h.unassigned_shards,
                        if h.unassigned_shards > 0 {
                            Theme::danger()
                        } else {
                            Theme::text_primary()
                        },
                    ),
                    (
                        "RELOCATING",
                        h.relocating_shards,
                        if h.relocating_shards > 0 {
                            Theme::warning()
                        } else {
                            Theme::text_primary()
                        },
                    ),
                    (
                        "DELAYED",
                        h.delayed_unassigned_shards,
                        if h.delayed_unassigned_shards > 0 {
                            Theme::danger()
                        } else {
                            Theme::text_primary()
                        },
                    ),
                    (
                        "INITIALIZING",
                        h.initializing_shards,
                        if h.initializing_shards > 0 {
                            Theme::warning()
                        } else {
                            Theme::text_primary()
                        },
                    ),
                ] {
                    render_kpi_tile(ui, tile_w, label, &value.to_string(), color);
                }
            });

            if let Some(ref s) = stats
                && let Some(ref nodes_stats) = s.nodes
                && let Some(ref jvm) = nodes_stats.jvm
                && let Some(ref mem) = jvm.mem
            {
                let ratio = if mem.heap_max_in_bytes > 0 {
                    mem.heap_used_in_bytes as f32 / mem.heap_max_in_bytes as f32
                } else {
                    0.0
                };
                ui.add_space(8.0);
                ui.horizontal(|ui| {
                    ui.label(
                        egui::RichText::new("JVM HEAP")
                            .size(10.0)
                            .color(Theme::text_muted()),
                    );
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        ui.label(
                            egui::RichText::new(format!(
                                "{} / {} ({:.1}%)",
                                human_bytes(mem.heap_used_in_bytes),
                                human_bytes(mem.heap_max_in_bytes),
                                ratio * 100.0
                            ))
                            .size(10.5)
                            .monospace()
                            .color(Theme::text_secondary()),
                        );
                    });
                });
                ui.add(GradientProgressBar::new(ratio).height(7.0));
            }

            // Disk allocation peak + quick Hot Threads button
            if let Some(ref allocs) = allocations
                && !allocs.is_empty()
                && let Some(top_node) = allocs
                    .iter()
                    .filter_map(|a| {
                        a.disk_percent
                            .as_deref()
                            .and_then(|p| p.parse::<f32>().ok())
                            .map(|pct| (pct, a))
                    })
                    .max_by(|a, b| a.0.total_cmp(&b.0))
            {
                ui.add_space(6.0);
                ui.horizontal(|ui| {
                    let (pct, alloc) = top_node;
                    let node_name = alloc.node.as_deref().unwrap_or("node");
                    let disk_color = if pct >= 85.0 {
                        Theme::danger()
                    } else if pct >= 75.0 {
                        Theme::warning()
                    } else {
                        Theme::text_secondary()
                    };
                    ui.label(
                        egui::RichText::new(format!("Top Disk: {} ({:.0}%)", node_name, pct))
                            .size(10.5)
                            .monospace()
                            .color(disk_color),
                    );
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        if ui
                            .small_button("🔥 Hot Threads")
                            .on_hover_text(format!("Inspect hot threads on {}", node_name))
                            .clicked()
                        {
                            actions.hot_threads = Some((name.clone(), node_name.to_string()));
                        }
                    });
                });
            }

            if let Some(ref exp) = explain
                && let Some(reason) = exp.explanation.as_deref().or(exp.reason.as_deref())
            {
                ui.add_space(6.0);
                egui::Frame::new()
                    .fill(Theme::warning().linear_multiply(0.12))
                    .corner_radius(4.0)
                    .inner_margin(egui::Margin::same(6))
                    .show(ui, |ui| {
                        ui.label(
                            egui::RichText::new(format!("Allocation Explain: {}", reason))
                                .size(10.5)
                                .color(Theme::warning()),
                        );
                    });
            }
        } else {
            ui.colored_label(Theme::text_muted(), "No status data loaded yet.");
        }

        // Telemetry viewer quick-launch buttons
        ui.add_space(8.0);
        ui.separator();
        ui.add_space(6.0);
        ui.horizontal(|ui| {
            if ui
                .button("📜 Log Viewer")
                .on_hover_text("Open Kibana-style Log Viewer for Elasticsearch & Kibana logs")
                .clicked()
            {
                open_log_viewer_for(state, name, actions);
            }
            if ui
                .button("📈 Metrics Viewer")
                .on_hover_text("Open Elasticsearch & Kibana time-series Metrics Viewer")
                .clicked()
            {
                open_metrics_viewer_for(state, name, actions);
            }
        });
    });

    // Subtle cyberpunk HUD corner brackets on every cluster card
    let bracket_color = if is_enabled {
        Theme::accent().linear_multiply(0.45)
    } else {
        Theme::text_muted().linear_multiply(0.3)
    };
    draw_hud_brackets(ui.painter(), response.response.rect, bracket_color, 10.0);

    if hover_effects && is_enabled {
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
                egui::Stroke::new(1.0_f32, glow_color),
                egui::StrokeKind::Middle,
            );
        }
    }
}

#[allow(clippy::too_many_arguments)]
fn render_detailed_view(
    ui: &mut Ui,
    clusters: &[ClusterConfig],
    all_cluster_names: &[String],
    state: &mut DashboardState,
    clusters_state: &mut ClustersState,
    status_state: &crate::modules::status::StatusState,
    actions: &mut DashboardActions,
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
        if ui.button("< Overview").clicked() {
            state.current_tab = DashboardTab::Overview;
        }
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
                actions.manual_refresh = true;
            }
        });
    });
    ui.add_space(12.0);

    let active_name = state.selected_cluster.clone();
    let Some(active_config) = clusters.iter().find(|c| c.name == active_name).cloned() else {
        return;
    };
    let is_enabled = active_config.enabled;

    let health = if is_enabled {
        status_state
            .health_data
            .iter()
            .find(|(n, _)| n == &active_name)
            .and_then(|(_, h)| h.clone())
    } else {
        None
    };
    let stats = if is_enabled {
        status_state
            .stats_data
            .iter()
            .find(|(n, _)| n == &active_name)
            .and_then(|(_, s)| s.clone())
    } else {
        None
    };
    let error = if is_enabled {
        status_state.errors.get(&active_name).cloned()
    } else {
        None
    };
    let explain = if is_enabled {
        status_state.explains.get(&active_name).cloned().flatten()
    } else {
        None
    };
    let es_version = status_state.es_versions.get(&active_name).cloned();
    let kibana_version = status_state.kibana_versions.get(&active_name).cloned();
    let allocations = if is_enabled {
        status_state.allocations.get(&active_name).cloned()
    } else {
        None
    };
    let pending_tasks = if is_enabled {
        status_state.pending_tasks.get(&active_name).cloned()
    } else {
        None
    };
    let nodes_list = if is_enabled {
        status_state.nodes.get(&active_name).cloned()
    } else {
        None
    };

    egui::ScrollArea::vertical()
        .id_salt("dashboard_detailed")
        .show(ui, |ui| {
            // --- TOP CLUSTER MANAGEMENT & DETAILS SECTION ---
            let mgmt_resp = egui::Frame::new()
                .fill(Theme::bg_card())
                .corner_radius(Theme::CARD_ROUNDING)
                .inner_margin(Theme::CARD_PADDING)
                .stroke(egui::Stroke::new(
                    1.0_f32,
                    Theme::accent().linear_multiply(0.28),
                ))
                .show(ui, |ui| {
                    ui.horizontal_wrapped(|ui| {
                        let dot_color = if !is_enabled {
                            Theme::text_muted()
                        } else if let Some(ref h) = health {
                            Theme::health_color(&h.status)
                        } else {
                            Theme::danger()
                        };
                        ui.add(
                            ConnectionDot::new(is_enabled && health.is_some())
                                .color(dot_color)
                                .size(10.0),
                        );
                        ui.heading(format!("Cluster: {}", active_name));

                        if !is_enabled {
                            ui.add(StatePill::new("Disabled", Theme::text_muted()));
                        } else if let Some(ref h) = health {
                            ui.add(StatePill::new(&h.status, Theme::health_color(&h.status)));
                        } else {
                            ui.add(StatePill::new("Offline", Theme::danger()));
                        }

                        render_enabled_checkbox(ui, &active_name, is_enabled, actions);

                        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                            if ui
                                .button("🗑 Delete")
                                .on_hover_text("Delete this cluster configuration")
                                .clicked()
                            {
                                actions.delete_cluster = Some(active_name.clone());
                            }

                            if ui
                                .button("🔌 Test Cluster")
                                .on_hover_text("Test connection to this cluster")
                                .clicked()
                            {
                                let pwd = crate::core::auth::get_password(&active_name)
                                    .ok()
                                    .flatten()
                                    .unwrap_or_default();
                                actions.test_cluster = Some((active_config.clone(), pwd));
                                clusters_state.tested_cluster = Some(active_name.clone());
                            }

                            let is_editing = clusters_state.editing_cluster.as_deref()
                                == Some(active_name.as_str());
                            let edit_label = if is_editing {
                                "✖ Close Edit"
                            } else {
                                "✏️ Edit Cluster"
                            };
                            if ui
                                .button(edit_label)
                                .on_hover_text("Edit this cluster's settings")
                                .clicked()
                            {
                                if is_editing {
                                    clusters_state.editing_cluster = None;
                                    clusters_state.test_result = None;
                                } else {
                                    clusters_state.selected_cluster = Some(active_name.clone());
                                    clusters_state.editing_cluster = Some(active_name.clone());
                                    clusters_state.edit_form = active_config.clone();
                                    clusters_state.edit_password =
                                        crate::core::auth::get_password(&active_name)
                                            .ok()
                                            .flatten()
                                            .unwrap_or_default();
                                    clusters_state.test_result = None;
                                }
                            }
                        });
                    });

                    ui.add_space(4.0);
                    render_host_links(ui, &active_config);
                    ui.add_space(4.0);
                    ui.label(
                        egui::RichText::new(format!(
                            "Telemetry cluster: {}  |  Logs: {}  |  Metrics: {}",
                            active_config.effective_telemetry_cluster(),
                            active_config.effective_logs_index_pattern(),
                            active_config.effective_metrics_index_pattern(),
                        ))
                        .size(10.5)
                        .monospace()
                        .color(Theme::text_muted()),
                    );

                    if clusters_state.editing_cluster.as_deref() == Some(active_name.as_str()) {
                        ui.add_space(8.0);
                        ui.separator();
                        ui.add_space(8.0);
                        render_edit_form(
                            ui,
                            clusters_state,
                            all_cluster_names,
                            &mut actions.save_cluster,
                            &mut actions.test_cluster,
                            &mut actions.fetch_repos,
                            &mut actions.fetch_slm,
                        );
                    } else if clusters_state.tested_cluster.as_deref() == Some(active_name.as_str())
                        && let Some(ref result) = clusters_state.test_result
                    {
                        ui.add_space(8.0);
                        ui.separator();
                        ui.add_space(6.0);
                        render_test_result(ui, result);
                    }
                });
            draw_hud_brackets(
                ui.painter(),
                mgmt_resp.response.rect,
                Theme::accent().linear_multiply(0.45),
                10.0,
            );
            ui.add_space(12.0);

            if !is_enabled {
                ui.colored_label(
                    Theme::text_muted(),
                    "Monitoring is disabled for this cluster. Tick Enabled above to resume status, node, and shard updates.",
                );
                ui.add_space(12.0);
            } else if let Some(ref err) = error {
                ui.colored_label(
                    Theme::danger(),
                    format!("Error communicating with cluster: {}", err),
                );
                ui.add_space(12.0);
            } else if let Some(h) = health {
                // --- 1. OVERVIEW WIDGET ---
                let overview_resp = egui::Frame::new()
                    .fill(Theme::bg_card())
                    .corner_radius(Theme::CARD_ROUNDING)
                    .inner_margin(Theme::CARD_PADDING)
                    .stroke(egui::Stroke::new(
                        1.0_f32,
                        Theme::accent().linear_multiply(0.22),
                    ))
                    .show(ui, |ui| {
                        ui.horizontal(|ui| {
                            let dot_color = Theme::health_color(&h.status);
                            ui.add(ConnectionDot::new(true).color(dot_color).size(10.0));
                            ui.heading(format!("Dashboard: {}", active_name));
                            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                                ui.add(StatePill::new(&h.status, dot_color));
                                if let Some(ref tasks) = pending_tasks
                                    && !tasks.is_empty()
                                {
                                    let danger_color = Theme::danger();
                                    let btn = ui
                                        .add(
                                            egui::Button::new(
                                                egui::RichText::new(format!(
                                                    "⚠ {} Pending",
                                                    tasks.len()
                                                ))
                                                .size(10.5)
                                                .strong()
                                                .color(Theme::contrast_text_color(danger_color)),
                                            )
                                            .fill(danger_color)
                                            .corner_radius(4.0),
                                        )
                                        .on_hover_text(
                                            "Delayed master metadata updates/actions. Click to inspect pending task queue.",
                                        );
                                    if btn.clicked() {
                                        actions.show_pending = Some(active_name.clone());
                                    }
                                }
                            });
                        });
                        ui.add_space(4.0);
                        ui.horizontal(|ui| {
                            if let Some(ref es_v) = es_version {
                                ui.label(
                                    egui::RichText::new(format!("ES Version: v{}", es_v))
                                        .monospace()
                                        .size(11.0)
                                        .color(Theme::accent()),
                                );
                            }
                            if let Some(ref kb_v) = kibana_version {
                                ui.label(
                                    egui::RichText::new("|")
                                        .size(11.0)
                                        .color(Theme::text_muted()),
                                );
                                ui.label(
                                    egui::RichText::new(format!("Kibana: v{}", kb_v))
                                        .monospace()
                                        .size(11.0)
                                        .color(Theme::text_secondary()),
                                );
                            }
                            if h.relocating_shards > 0 {
                                ui.label(
                                    egui::RichText::new("|")
                                        .size(11.0)
                                        .color(Theme::text_muted()),
                                );
                                ui.label(
                                    egui::RichText::new(format!(
                                        "~ {} Relocating",
                                        h.relocating_shards
                                    ))
                                    .size(11.0)
                                    .color(Theme::warning()),
                                );
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
                                    ui.label(
                                        egui::RichText::new("Total Nodes")
                                            .color(Theme::text_muted())
                                            .size(10.5),
                                    );
                                    ui.label(
                                        egui::RichText::new(format!("{}", h.number_of_nodes))
                                            .strong()
                                            .size(22.0)
                                            .color(Theme::text_primary()),
                                    );
                                    ui.label(
                                        egui::RichText::new(format!(
                                            "Data nodes: {}",
                                            h.number_of_data_nodes
                                        ))
                                        .color(Theme::text_muted())
                                        .size(10.0),
                                    );
                                });
                            });
                            ui.separator();

                            // Shards Box
                            ui.allocate_ui(egui::Vec2::new(width_box, 60.0), |ui| {
                                ui.vertical(|ui| {
                                    ui.label(
                                        egui::RichText::new("Active / Primary Shards")
                                            .color(Theme::text_muted())
                                            .size(10.5),
                                    );
                                    ui.label(
                                        egui::RichText::new(format!(
                                            "{} / {}",
                                            h.active_shards, h.active_primary_shards
                                        ))
                                        .strong()
                                        .size(22.0)
                                        .color(Theme::text_primary()),
                                    );
                                    if h.unassigned_shards > 0 {
                                        ui.label(
                                            egui::RichText::new(format!(
                                                "⚠️ {} Unassigned",
                                                h.unassigned_shards
                                            ))
                                            .strong()
                                            .color(Theme::danger())
                                            .size(10.0),
                                        );
                                    } else {
                                        ui.label(
                                            egui::RichText::new("All shards assigned")
                                                .color(Theme::success())
                                                .size(10.0),
                                        );
                                    }
                                });
                            });
                            ui.separator();

                            // Doc count Box
                            if let Some(ref s) = stats
                                && let Some(ref indices) = s.indices
                            {
                                ui.allocate_ui(egui::Vec2::new(width_box, 60.0), |ui| {
                                    ui.vertical(|ui| {
                                        ui.label(
                                            egui::RichText::new("Documents Count")
                                                .color(Theme::text_muted())
                                                .size(10.5),
                                        );
                                        if let Some(ref docs) = indices.docs {
                                            ui.label(
                                                egui::RichText::new(human_docs(docs.count))
                                                    .strong()
                                                    .size(22.0)
                                                    .color(Theme::text_primary()),
                                            );
                                            ui.label(
                                                egui::RichText::new(format!(
                                                    "Deleted: {}",
                                                    human_docs(docs.deleted)
                                                ))
                                                .color(Theme::text_muted())
                                                .size(10.0),
                                            );
                                        } else {
                                            ui.label(
                                                egui::RichText::new("—")
                                                    .strong()
                                                    .size(22.0)
                                                    .color(Theme::text_primary()),
                                            );
                                        }
                                    });
                                });
                                ui.separator();

                                // Store Size Box
                                ui.allocate_ui(egui::Vec2::new(width_box, 60.0), |ui| {
                                    ui.vertical(|ui| {
                                        ui.label(
                                            egui::RichText::new("Total Store Size")
                                                .color(Theme::text_muted())
                                                .size(10.5),
                                        );
                                        if let Some(ref store) = indices.store {
                                            ui.label(
                                                egui::RichText::new(human_bytes(
                                                    store.size_in_bytes,
                                                ))
                                                .strong()
                                                .size(22.0)
                                                .color(Theme::accent()),
                                            );
                                        } else {
                                            ui.label(
                                                egui::RichText::new("—")
                                                    .strong()
                                                    .size(22.0)
                                                    .color(Theme::text_primary()),
                                            );
                                        }
                                        ui.label(
                                            egui::RichText::new(format!(
                                                "Indices: {}",
                                                indices.count
                                            ))
                                            .color(Theme::text_muted())
                                            .size(10.0),
                                        );
                                    });
                                });
                            }
                        });

                        ui.add_space(10.0);
                        ui.label(
                            egui::RichText::new("CLUSTER HEALTH")
                                .size(10.0)
                                .strong()
                                .color(Theme::text_muted()),
                        );
                        ui.add_space(4.0);
                        let health_fields = [
                            ("Cluster name", h.cluster_name.clone()),
                            ("Status", h.status.clone()),
                            ("Nodes", h.number_of_nodes.to_string()),
                            ("Data nodes", h.number_of_data_nodes.to_string()),
                            ("Active shards", h.active_shards.to_string()),
                            ("Active primary shards", h.active_primary_shards.to_string()),
                            (
                                "Active shards percent",
                                format!("{:.1}%", h.active_shards_percent_as_number),
                            ),
                            ("Relocating shards", h.relocating_shards.to_string()),
                            ("Initializing shards", h.initializing_shards.to_string()),
                            ("Unassigned shards", h.unassigned_shards.to_string()),
                            (
                                "Unassigned primary shards",
                                h.unassigned_primary_shards.to_string(),
                            ),
                            (
                                "Delayed unassigned shards",
                                h.delayed_unassigned_shards.to_string(),
                            ),
                            ("In-flight fetches", h.number_of_in_flight_fetch.to_string()),
                            ("Pending tasks", h.number_of_pending_tasks.to_string()),
                            (
                                "Max task queue wait",
                                format!("{} ms", h.task_max_waiting_in_queue_millis),
                            ),
                            (
                                "Timed out",
                                h.timed_out
                                    .map(|value| value.to_string())
                                    .unwrap_or_else(|| "—".to_string()),
                            ),
                        ];
                        egui::Grid::new("cluster_health_fields_detailed")
                            .num_columns(8)
                            .spacing([16.0, 6.0])
                            .striped(true)
                            .show(ui, |ui| {
                                for row in health_fields.chunks(4) {
                                    for (label, value) in row {
                                        ui.label(
                                            egui::RichText::new(*label)
                                                .size(10.5)
                                                .color(Theme::text_muted()),
                                        );
                                        ui.label(
                                            egui::RichText::new(value)
                                                .size(10.5)
                                                .monospace()
                                                .color(Theme::text_primary()),
                                        );
                                    }
                                    ui.end_row();
                                }
                            });

                        // Overall JVM Heap Bar
                        if let Some(ref s) = stats
                            && let Some(ref nodes_stats) = s.nodes
                            && let Some(ref jvm) = nodes_stats.jvm
                            && let Some(ref mem) = jvm.mem
                        {
                            let ratio = if mem.heap_max_in_bytes > 0 {
                                mem.heap_used_in_bytes as f32 / mem.heap_max_in_bytes as f32
                            } else {
                                0.0
                            };
                            ui.add_space(10.0);
                            ui.horizontal(|ui| {
                                ui.label(
                                    egui::RichText::new("JVM HEAP")
                                        .size(10.0)
                                        .color(Theme::text_muted()),
                                );
                                ui.with_layout(
                                    egui::Layout::right_to_left(egui::Align::Center),
                                    |ui| {
                                        ui.label(
                                            egui::RichText::new(format!(
                                                "{} / {} ({:.1}%)",
                                                human_bytes(mem.heap_used_in_bytes),
                                                human_bytes(mem.heap_max_in_bytes),
                                                ratio * 100.0
                                            ))
                                            .size(10.5)
                                            .monospace()
                                            .color(Theme::text_secondary()),
                                        );
                                    },
                                );
                            });
                            ui.add(GradientProgressBar::new(ratio).height(7.0));
                        }

                        // Peak disk allocation + quick Hot Threads button
                        if let Some(ref allocs) = allocations
                            && !allocs.is_empty()
                            && let Some(top_node) = allocs
                                .iter()
                                .filter_map(|a| {
                                    a.disk_percent
                                        .as_deref()
                                        .and_then(|p| p.parse::<f32>().ok())
                                        .map(|pct| (pct, a))
                                })
                                .max_by(|a, b| a.0.total_cmp(&b.0))
                        {
                            ui.add_space(8.0);
                            ui.horizontal(|ui| {
                                let (pct, alloc) = top_node;
                                let node_name = alloc.node.as_deref().unwrap_or("node");
                                let disk_color = if pct >= 85.0 {
                                    Theme::danger()
                                } else if pct >= 75.0 {
                                    Theme::warning()
                                } else {
                                    Theme::text_secondary()
                                };
                                ui.label(
                                    egui::RichText::new(format!(
                                        "Top Disk: {} ({:.0}%)",
                                        node_name, pct
                                    ))
                                    .size(10.5)
                                    .monospace()
                                    .color(disk_color),
                                );
                                ui.with_layout(
                                    egui::Layout::right_to_left(egui::Align::Center),
                                    |ui| {
                                        if ui
                                            .small_button("🔥 Hot Threads")
                                            .on_hover_text(format!(
                                                "Inspect hot threads on {}",
                                                node_name
                                            ))
                                            .clicked()
                                        {
                                            actions.hot_threads = Some((
                                                active_name.clone(),
                                                node_name.to_string(),
                                            ));
                                        }
                                    },
                                );
                            });
                        }

                        // Allocation Explain banner when present
                        if let Some(ref exp) = explain
                            && let Some(reason) =
                                exp.explanation.as_deref().or(exp.reason.as_deref())
                        {
                            ui.add_space(8.0);
                            egui::Frame::new()
                                .fill(Theme::warning().linear_multiply(0.12))
                                .corner_radius(4.0)
                                .inner_margin(egui::Margin::same(6))
                                .show(ui, |ui| {
                                    ui.label(
                                        egui::RichText::new(format!(
                                            "Allocation Explain: {}",
                                            reason
                                        ))
                                        .size(10.5)
                                        .color(Theme::warning()),
                                    );
                                });
                        }
                    });
                draw_hud_brackets(
                    ui.painter(),
                    overview_resp.response.rect,
                    Theme::accent().linear_multiply(0.45),
                    10.0,
                );
                ui.add_space(16.0);

                // --- 2. NODE LIST TABLE ---
                ui.heading("💻 Node List (Detail)");
                ui.add_space(8.0);
                egui::Frame::new()
                    .fill(Theme::bg_card())
                    .corner_radius(Theme::CARD_ROUNDING)
                    .inner_margin(Theme::CARD_PADDING)
                    .stroke(egui::Stroke::new(1.0_f32, Theme::bg_input()))
                    .show(ui, |ui| {
                        if let Some(ref nodes) = nodes_list {
                            if nodes.is_empty() {
                                ui.label(
                                    egui::RichText::new("No nodes information returned from ES.")
                                        .color(Theme::text_muted()),
                                );
                            } else {
                                egui::Grid::new("nodes_grid_detailed")
                                    .num_columns(7)
                                    .spacing([16.0, 10.0])
                                    .striped(true)
                                    .show(ui, |ui| {
                                        ui.label(
                                            egui::RichText::new("Name")
                                                .strong()
                                                .color(Theme::text_secondary())
                                                .size(11.0),
                                        );
                                        ui.label(
                                            egui::RichText::new("Role")
                                                .strong()
                                                .color(Theme::text_secondary())
                                                .size(11.0),
                                        );
                                        ui.label(
                                            egui::RichText::new("CPU")
                                                .strong()
                                                .color(Theme::text_secondary())
                                                .size(11.0),
                                        );
                                        ui.label(
                                            egui::RichText::new("RAM")
                                                .strong()
                                                .color(Theme::text_secondary())
                                                .size(11.0),
                                        );
                                        ui.label(
                                            egui::RichText::new("JVM Heap")
                                                .strong()
                                                .color(Theme::text_secondary())
                                                .size(11.0),
                                        );
                                        ui.label(
                                            egui::RichText::new("IP Address")
                                                .strong()
                                                .color(Theme::text_secondary())
                                                .size(11.0),
                                        );
                                        ui.label(
                                            egui::RichText::new("Status")
                                                .strong()
                                                .color(Theme::text_secondary())
                                                .size(11.0),
                                        );
                                        ui.end_row();

                                        for node in nodes {
                                            let name = node.name.as_deref().unwrap_or("Unknown");
                                            let role = node.role.as_deref().unwrap_or("—");
                                            let cpu = node.cpu.as_deref().unwrap_or("0");
                                            let ram = node.ram_percent.as_deref().unwrap_or("0");
                                            let heap = node.heap_percent.as_deref().unwrap_or("0");
                                            let ip = node.ip.as_deref().unwrap_or("—");
                                            let is_master =
                                                node.master.as_deref().unwrap_or("-") == "*";

                                            ui.horizontal(|ui| {
                                                ui.label(
                                                    egui::RichText::new(name)
                                                        .strong()
                                                        .color(Theme::text_primary())
                                                        .size(11.0),
                                                );
                                            });

                                            ui.label(
                                                egui::RichText::new(role)
                                                    .monospace()
                                                    .color(Theme::text_muted())
                                                    .size(10.5),
                                            );

                                            let cpu_val = cpu.parse::<f32>().unwrap_or(0.0) / 100.0;
                                            ui.horizontal(|ui| {
                                                ui.add(
                                                    GradientProgressBar::new(cpu_val)
                                                        .height(5.0)
                                                        .width(50.0),
                                                );
                                                ui.label(
                                                    egui::RichText::new(format!("{}%", cpu))
                                                        .size(10.0)
                                                        .monospace()
                                                        .color(Theme::text_primary()),
                                                );
                                            });

                                            let ram_val = ram.parse::<f32>().unwrap_or(0.0) / 100.0;
                                            ui.horizontal(|ui| {
                                                ui.add(
                                                    GradientProgressBar::new(ram_val)
                                                        .height(5.0)
                                                        .width(50.0),
                                                );
                                                ui.label(
                                                    egui::RichText::new(format!("{}%", ram))
                                                        .size(10.0)
                                                        .monospace()
                                                        .color(Theme::text_primary()),
                                                );
                                            });

                                            let heap_val =
                                                heap.parse::<f32>().unwrap_or(0.0) / 100.0;
                                            ui.horizontal(|ui| {
                                                ui.add(
                                                    GradientProgressBar::new(heap_val)
                                                        .height(5.0)
                                                        .width(50.0),
                                                );
                                                ui.label(
                                                    egui::RichText::new(format!("{}%", heap))
                                                        .size(10.0)
                                                        .monospace()
                                                        .color(Theme::text_primary()),
                                                );
                                            });

                                            ui.label(
                                                egui::RichText::new(ip)
                                                    .monospace()
                                                    .color(Theme::text_secondary())
                                                    .size(10.5),
                                            );

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
                            ui.label(
                                egui::RichText::new("Nodes list loading or query failed. Check connection or wait for auto-refresh.")
                                    .color(Theme::text_muted()),
                            );
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
                    .stroke(egui::Stroke::new(1.0_f32, Theme::bg_input()))
                    .show(ui, |ui| {
                        if let Some(allocs) = &allocations {
                            let data_nodes: Vec<_> = allocs
                                .iter()
                                .filter(|a| {
                                    a.node.as_deref().unwrap_or("UNASSIGNED") != "UNASSIGNED"
                                })
                                .collect();

                            let unassigned_nodes: Vec<_> = allocs
                                .iter()
                                .filter(|a| {
                                    a.node.as_deref().unwrap_or("UNASSIGNED") == "UNASSIGNED"
                                })
                                .collect();

                            if data_nodes.is_empty() && unassigned_nodes.is_empty() {
                                ui.label(
                                    egui::RichText::new("No shard allocation data available.")
                                        .color(Theme::text_muted()),
                                );
                            } else {
                                egui::Grid::new("shard_allocations_dashboard")
                                    .num_columns(5)
                                    .spacing([20.0, 10.0])
                                    .striped(true)
                                    .show(ui, |ui| {
                                        ui.label(
                                            egui::RichText::new("Node")
                                                .strong()
                                                .color(Theme::text_secondary())
                                                .size(11.0),
                                        );
                                        ui.label(
                                            egui::RichText::new("Shards")
                                                .strong()
                                                .color(Theme::text_secondary())
                                                .size(11.0),
                                        );
                                        ui.label(
                                            egui::RichText::new("Disk Free / Total")
                                                .strong()
                                                .color(Theme::text_secondary())
                                                .size(11.0),
                                        );
                                        ui.label(
                                            egui::RichText::new("Disk Usage")
                                                .strong()
                                                .color(Theme::text_secondary())
                                                .size(11.0),
                                        );
                                        ui.label(
                                            egui::RichText::new("Disk Bar")
                                                .strong()
                                                .color(Theme::text_secondary())
                                                .size(11.0),
                                        );
                                        ui.end_row();

                                        for node in &data_nodes {
                                            let node_name = node.node.as_deref().unwrap_or("Unknown");
                                            let shards_str = node.shards.as_deref().unwrap_or("0");
                                            let avail = node.disk_avail.as_deref().unwrap_or("—");
                                            let total = node.disk_total.as_deref().unwrap_or("—");
                                            let percent_str =
                                                node.disk_percent.as_deref().unwrap_or("0");

                                            let percent_val =
                                                percent_str.parse::<f32>().unwrap_or(0.0) / 100.0;

                                            ui.label(
                                                egui::RichText::new(node_name)
                                                    .strong()
                                                    .color(Theme::text_primary())
                                                    .size(11.0),
                                            );
                                            ui.label(
                                                egui::RichText::new(shards_str)
                                                    .monospace()
                                                    .color(Theme::text_primary())
                                                    .size(11.0),
                                            );
                                            ui.label(
                                                egui::RichText::new(format!("{} / {}", avail, total))
                                                    .monospace()
                                                    .color(Theme::text_secondary())
                                                    .size(10.5),
                                            );
                                            ui.label(
                                                egui::RichText::new(format!("{}%", percent_str))
                                                    .monospace()
                                                    .color(Theme::text_primary())
                                                    .size(10.5),
                                            );
                                            ui.add(
                                                GradientProgressBar::new(percent_val)
                                                    .height(5.0)
                                                    .width(80.0),
                                            );
                                            ui.end_row();
                                        }

                                        for node in &unassigned_nodes {
                                            let shards_str = node.shards.as_deref().unwrap_or("0");
                                            ui.label(
                                                egui::RichText::new("UNASSIGNED")
                                                    .strong()
                                                    .color(Theme::danger())
                                                    .size(11.0),
                                            );
                                            ui.label(
                                                egui::RichText::new(shards_str)
                                                    .monospace()
                                                    .color(Theme::danger())
                                                    .size(11.0),
                                            );
                                            ui.label(
                                                egui::RichText::new("—")
                                                    .color(Theme::text_muted())
                                                    .size(10.5),
                                            );
                                            ui.label(
                                                egui::RichText::new("—")
                                                    .color(Theme::text_muted())
                                                    .size(10.5),
                                            );
                                            ui.label(
                                                egui::RichText::new("—")
                                                    .color(Theme::text_muted())
                                                    .size(10.5),
                                            );
                                            ui.end_row();
                                        }
                                    });
                            }
                        } else {
                            ui.label(
                                egui::RichText::new("Allocation statistics currently loading...")
                                    .color(Theme::text_muted()),
                            );
                        }
                    });
                ui.add_space(16.0);
            } else {
                ui.label(
                    egui::RichText::new("Cluster status unreachable or currently offline.")
                        .color(Theme::danger())
                        .size(13.0),
                );
                ui.add_space(12.0);
            }

            // --- 4. COLLAPSED CLUSTER LOGS SECTION ---
            egui::CollapsingHeader::new(
                egui::RichText::new("📜 Cluster Logs")
                    .strong()
                    .size(14.0)
                    .color(Theme::text_primary()),
            )
            .id_salt(("detail_logs_header", &active_name))
            .default_open(false)
            .show(ui, |ui| {
                let viewer = state.log_viewers.entry(active_name.clone()).or_default();
                if !viewer.has_loaded && !viewer.is_loading {
                    viewer.is_loading = true;
                    actions.fetch_logs.push(active_name.clone());
                }
                if render_log_viewer_panel(
                    ui,
                    &active_config,
                    viewer,
                    &format!("detail_logs_{}", active_name),
                    Some(420.0),
                ) {
                    viewer.is_loading = true;
                    actions.fetch_logs.push(active_name.clone());
                }
            });
            ui.add_space(8.0);

            // --- 5. COLLAPSED CLUSTER METRICS SECTION ---
            egui::CollapsingHeader::new(
                egui::RichText::new("📈 Cluster Metrics")
                    .strong()
                    .size(14.0)
                    .color(Theme::text_primary()),
            )
            .id_salt(("detail_metrics_header", &active_name))
            .default_open(false)
            .show(ui, |ui| {
                let viewer = state.metrics_viewers.entry(active_name.clone()).or_default();
                if !viewer.has_loaded && !viewer.is_loading {
                    viewer.is_loading = true;
                    actions.fetch_metrics.push(active_name.clone());
                }
                if render_metrics_viewer_panel(
                    ui,
                    &active_config,
                    viewer,
                    &format!("detail_metrics_{}", active_name),
                ) {
                    viewer.is_loading = true;
                    actions.fetch_metrics.push(active_name.clone());
                }
            });
            ui.add_space(16.0);
        });
}

/// Render the overview Log Viewer and Metrics Viewer popup windows when open.
pub fn render_dashboard_telemetry_popups(
    ctx: &egui::Context,
    clusters: &[ClusterConfig],
    state: &mut DashboardState,
    actions: &mut DashboardActions,
) {
    if let Some(cluster_name) = state.open_log_viewer_cluster.clone() {
        let mut is_open = true;
        egui::Window::new(format!("📜 Log Viewer — {}", cluster_name))
            .id(egui::Id::new("dashboard_log_viewer_popup"))
            .open(&mut is_open)
            .collapsible(false)
            .resizable(true)
            .default_size([980.0, 640.0])
            .min_size([600.0, 380.0])
            .show(ctx, |ui| {
                if let Some(cfg) = clusters.iter().find(|c| c.name == cluster_name) {
                    let viewer = state.log_viewers.entry(cluster_name.clone()).or_default();
                    if !viewer.has_loaded && !viewer.is_loading {
                        viewer.is_loading = true;
                        actions.fetch_logs.push(cluster_name.clone());
                    }
                    if render_log_viewer_panel(
                        ui,
                        cfg,
                        viewer,
                        &format!("popup_logs_{}", cluster_name),
                        None,
                    ) {
                        viewer.is_loading = true;
                        actions.fetch_logs.push(cluster_name.clone());
                    }
                } else {
                    ui.label("Cluster not found.");
                }
            });
        if !is_open {
            state.open_log_viewer_cluster = None;
        }
    }

    if let Some(cluster_name) = state.open_metrics_viewer_cluster.clone() {
        let mut is_open = true;
        egui::Window::new(format!("📈 Metrics Viewer — {}", cluster_name))
            .id(egui::Id::new("dashboard_metrics_viewer_popup"))
            .open(&mut is_open)
            .collapsible(false)
            .resizable(true)
            .default_size([980.0, 660.0])
            .min_size([600.0, 380.0])
            .show(ctx, |ui| {
                if let Some(cfg) = clusters.iter().find(|c| c.name == cluster_name) {
                    egui::ScrollArea::vertical()
                        .id_salt(("popup_metrics_scroll", &cluster_name))
                        .show(ui, |ui| {
                            let viewer = state
                                .metrics_viewers
                                .entry(cluster_name.clone())
                                .or_default();
                            if !viewer.has_loaded && !viewer.is_loading {
                                viewer.is_loading = true;
                                actions.fetch_metrics.push(cluster_name.clone());
                            }
                            if render_metrics_viewer_panel(
                                ui,
                                cfg,
                                viewer,
                                &format!("popup_metrics_{}", cluster_name),
                            ) {
                                viewer.is_loading = true;
                                actions.fetch_metrics.push(cluster_name.clone());
                            }
                        });
                } else {
                    ui.label("Cluster not found.");
                }
            });
        if !is_open {
            state.open_metrics_viewer_cluster = None;
        }
    }
}
