use crate::core::config::ClusterConfig;
use crate::core::es_client::{ClusterHealth, ClusterStats};
use crate::ui::theme::Theme;
use crate::ui::widgets::{ConnectionDot, human_bytes, human_docs, open_link};
use egui::{Color32, Ui};

#[derive(Debug, Clone, Default)]
pub struct StatusState {
    pub health_data: Vec<(String, Option<ClusterHealth>)>,
    pub stats_data: Vec<(String, Option<ClusterStats>)>,
    pub explains:
        std::collections::HashMap<String, Option<crate::core::es_client::AllocationExplain>>,
    pub es_versions: std::collections::HashMap<String, String>,
    pub kibana_versions: std::collections::HashMap<String, String>,
    pub allocations: std::collections::HashMap<String, Vec<crate::core::es_client::CatAllocation>>,
    pub pending_tasks: std::collections::HashMap<String, Vec<serde_json::Value>>,
    pub errors: std::collections::HashMap<String, String>,
}

pub fn render_status_module(
    ui: &mut Ui,
    clusters: &[ClusterConfig],
    state: &StatusState,
    on_hot_threads: &mut Option<(String, String)>,
    on_show_pending: &mut Option<String>,
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
                    egui::RichText::new(
                        "No clusters configured. Add a cluster to begin monitoring.",
                    )
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
                                    let explain =
                                        state.explains.get(&cluster.name).cloned().flatten();
                                    let es_version = state.es_versions.get(&cluster.name).cloned();
                                    let kibana_version =
                                        state.kibana_versions.get(&cluster.name).cloned();
                                    let allocations = state.allocations.get(&cluster.name).cloned();
                                    let pending_tasks =
                                        state.pending_tasks.get(&cluster.name).cloned();
                                    render_status_card(
                                        ui,
                                        cluster,
                                        &health,
                                        stats,
                                        es_version,
                                        kibana_version,
                                        allocations,
                                        pending_tasks,
                                        error,
                                        explain,
                                        on_hot_threads,
                                        on_show_pending,
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
    config: &ClusterConfig,
    health: &Option<ClusterHealth>,
    stats: Option<ClusterStats>,
    es_version: Option<String>,
    kibana_version: Option<String>,
    allocations: Option<Vec<crate::core::es_client::CatAllocation>>,
    pending_tasks: Option<Vec<serde_json::Value>>,
    error: Option<String>,
    explain: Option<crate::core::es_client::AllocationExplain>,
    on_hot_threads: &mut Option<(String, String)>,
    on_show_pending: &mut Option<String>,
    col_width: f32,
    hover_effects: bool,
) {
    let name = &config.name;
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
                Some(h) => match h.status.as_str() {
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
                        .size(17.0)
                        .color(Theme::text_primary()),
                );
                if es_version.is_some() || kibana_version.is_some() {
                    ui.horizontal(|ui| {
                        if let Some(ref es_v) = es_version {
                            ui.label(
                                egui::RichText::new(format!("ES v{}", es_v))
                                    .size(10.0)
                                    .color(Theme::text_muted())
                                    .monospace(),
                            );
                        }
                        if let Some(ref kb_v) = kibana_version {
                            if es_version.is_some() {
                                ui.label(egui::RichText::new("|").size(10.0).color(Theme::text_muted()));
                            }
                            ui.label(
                                egui::RichText::new(format!("KB v{}", kb_v))
                                    .size(10.0)
                                    .color(Theme::text_muted())
                                    .monospace(),
                            );
                        }
                    });
                }
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

                if let Some(ref tasks) = pending_tasks {
                    if !tasks.is_empty() {
                        let danger_color = Theme::danger();
                        let btn = ui.add(
                            egui::Button::new(
                                egui::RichText::new(format!("⚠️ {} Pending", tasks.len()))
                                    .size(10.0)
                                    .strong()
                                    .color(Theme::contrast_text_color(danger_color))
                            )
                            .fill(danger_color)
                            .corner_radius(4.0)
                        ).on_hover_text("Delayed master metadata updates/actions. Click to inspect pending task queue.");
                        if btn.clicked() {
                            *on_show_pending = Some(name.to_string());
                        }
                    }
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

            // Horizontal split: Left for Stats, Right for Clickable orange links
            ui.horizontal(|ui| {
                ui.set_width(ui.available_width());
                let left_w = ui.available_width() * 0.58;
                let right_w = ui.available_width() - left_w - 12.0;

                // --- LEFT COLUMN: STATS ---
                ui.allocate_ui(egui::Vec2::new(left_w, ui.available_height()), |ui| {
                    ui.vertical(|ui| {
                        for pair in items.chunks(2) {
                            ui.horizontal(|ui| {
                                let item_w = left_w / 2.0 - 8.0;
                                for (j, (label, value)) in pair.iter().enumerate() {
                                    if j > 0 {
                                        ui.add_space(8.0);
                                    }
                                    ui.allocate_ui_with_layout(
                                        egui::Vec2::new(item_w, 18.0),
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
                    });
                });

                ui.add_space(12.0);

                // --- RIGHT COLUMN: ORANGE HYPERLINKS ---
                ui.allocate_ui(egui::Vec2::new(right_w, ui.available_height()), |ui| {
                    ui.with_layout(egui::Layout::top_down(egui::Align::Min), |ui| {
                        let truncate_url = |url: &str| -> String {
                            let clean = url.trim_start_matches("http://").trim_start_matches("https://");
                            if clean.chars().count() > 28 {
                                let mut truncated: String = clean.chars().take(25).collect();
                                truncated.push_str("...");
                                truncated
                            } else {
                                clean.to_string()
                            }
                        };

                        // Elastic link
                        let es_url_raw = config.host.clone();
                        let es_url = if es_url_raw.starts_with("http://") || es_url_raw.starts_with("https://") {
                            es_url_raw.clone()
                        } else {
                            format!("http://{}", es_url_raw)
                        };
                        let es_btn = ui.add(
                            egui::Link::new(
                                egui::RichText::new(truncate_url(&es_url))
                                    .size(11.0)
                                    .color(Theme::accent()),
                            )
                        );
                        if es_btn.clicked() {
                            open_link(ui.ctx(), &es_url);
                        }
                        ui.add_space(2.0);

                        // Kibana link (with auto fallback if empty)
                        let kb_url_raw = if config.kibana_host.is_empty() {
                            if config.host.contains("elastic") {
                                config.host.replace("elastic", "kibana")
                            } else {
                                config.host.clone()
                            }
                        } else {
                            config.kibana_host.clone()
                        };
                        let kb_url = if kb_url_raw.starts_with("http://") || kb_url_raw.starts_with("https://") {
                            kb_url_raw.clone()
                        } else {
                            format!("http://{}", kb_url_raw)
                        };
                        let kb_btn = ui.add(
                            egui::Link::new(
                                egui::RichText::new(truncate_url(&kb_url))
                                    .size(11.0)
                                    .color(Theme::accent()),
                            )
                        );
                        if kb_btn.clicked() {
                            open_link(ui.ctx(), &kb_url);
                        }
                        ui.add_space(2.0);

                        // HAProxy link
                        if !config.haproxy_host.is_empty() {
                            let ha_url_raw = config.haproxy_host.clone();
                            let ha_url = if ha_url_raw.starts_with("http://") || ha_url_raw.starts_with("https://") {
                                ha_url_raw.clone()
                            } else {
                                format!("http://{}", ha_url_raw)
                            };
                            let ha_btn = ui.add(
                                egui::Link::new(
                                    egui::RichText::new(truncate_url(&ha_url))
                                        .size(11.0)
                                        .color(Theme::accent()),
                                )
                            );
                            if ha_btn.clicked() {
                                open_link(ui.ctx(), &ha_url);
                            }
                            ui.add_space(2.0);
                        }

                        // Custom links
                        for (_name, url_raw) in &config.custom_links {
                            if !url_raw.is_empty() {
                                let url = if url_raw.starts_with("http://") || url_raw.starts_with("https://") {
                                    url_raw.clone()
                                } else {
                                    format!("http://{}", url_raw)
                                };
                                let cust_btn = ui.add(
                                    egui::Link::new(
                                        egui::RichText::new(truncate_url(&url))
                                            .size(11.0)
                                            .color(Theme::accent()),
                                    )
                                );
                                if cust_btn.clicked() {
                                    open_link(ui.ctx(), &url);
                                }
                                ui.add_space(2.0);
                            }
                        }
                    });
                });
            });

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

            if let Some(ref allocs) = allocations {
                let data_nodes: Vec<_> = allocs.iter()
                    .filter(|a| a.node.as_deref().unwrap_or("UNASSIGNED") != "UNASSIGNED")
                    .collect();

                if !data_nodes.is_empty() {
                    ui.add_space(8.0);
                    ui.separator();
                    ui.add_space(4.0);
                    ui.label(
                        egui::RichText::new("💾 Data Nodes (Shards & Disk)")
                            .strong()
                            .size(11.0)
                            .color(Theme::text_secondary()),
                    );
                    ui.add_space(4.0);

                    for node in data_nodes {
                        let node_name = node.node.as_deref().unwrap_or("Unknown");
                        let shard_count: u32 = node.shards.as_ref()
                            .and_then(|s| s.parse().ok())
                            .unwrap_or(0);

                        let disk_percent_val: f32 = node.disk_percent.as_ref()
                            .and_then(|p| p.parse::<f32>().ok())
                            .unwrap_or(0.0);
                        let free_percent = 100.0 - disk_percent_val;

                        // Shards ratio: approaches 1000
                        let shard_ratio = (shard_count as f32 / 1000.0).clamp(0.0, 1.0);

                        // Disk ratio: falls to 15%
                        let space_ratio = if free_percent <= 15.0 {
                            1.0
                        } else if free_percent >= 50.0 {
                            0.0
                        } else {
                            (50.0 - free_percent) / (50.0 - 15.0)
                        };

                        let ratio = f32::max(shard_ratio, space_ratio);

                        // Calm green to Yellow to Red interpolation
                        let color = if ratio <= 0.5 {
                            let t = ratio * 2.0;
                            let r = (46.0 * (1.0 - t) + 241.0 * t) as u8;
                            let g = (204.0 * (1.0 - t) + 196.0 * t) as u8;
                            let b = (113.0 * (1.0 - t) + 15.0 * t) as u8;
                            Color32::from_rgb(r, g, b)
                        } else {
                            let t = (ratio - 0.5) * 2.0;
                            let r = (241.0 * (1.0 - t) + 231.0 * t) as u8;
                            let g = (196.0 * (1.0 - t) + 76.0 * t) as u8;
                            let b = (15.0 * (1.0 - t) + 60.0 * t) as u8;
                            Color32::from_rgb(r, g, b)
                        };

                        ui.horizontal(|ui| {
                            let (rect, _) = ui.allocate_exact_size(egui::Vec2::new(8.0, 8.0), egui::Sense::hover());
                            ui.painter().circle_filled(rect.center(), 4.0, color);
                            ui.add_space(2.0);

                            ui.label(
                                egui::RichText::new(node_name)
                                    .size(11.0)
                                    .color(Theme::text_primary()),
                            );

                            ui.add_space(4.0);
                            let hot_btn = ui.add(
                                egui::Link::new(
                                    egui::RichText::new("🔥")
                                        .size(10.5)
                                )
                            ).on_hover_text("View live node thread usage / stack trace diagnostics");
                            if hot_btn.clicked() {
                                *on_hot_threads = Some((name.to_string(), node_name.to_string()));
                            }

                            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                                ui.label(
                                    egui::RichText::new(format!(
                                        "{} shards | {:.1}% free ({})",
                                        shard_count,
                                        free_percent,
                                        node.disk_avail.as_deref().unwrap_or("—")
                                    ))
                                    .size(10.0)
                                    .color(Theme::text_muted())
                                    .monospace(),
                                );
                            });
                        });
                    }
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
