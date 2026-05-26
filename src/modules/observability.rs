use egui::{Ui, RichText};
use crate::ui::theme::Theme;

#[derive(Debug, Clone, serde::Deserialize, serde::Serialize, PartialEq)]
pub struct SyntheticMonitor {
    pub id: String,
    pub name: String,
    pub monitor_type: String, // http, tcp, icmp, browser
    pub status: String,       // up, down
    pub url: String,
    pub locations: Vec<String>,
    pub latency_ms: u32,
    pub latency_history: Vec<f32>,
    pub last_checked: String,
}

#[derive(Debug, Clone, PartialEq)]
pub enum ObsTab {
    Browse,
    Dashboard,
}

pub struct ObservabilityState {
    pub selected_cluster: String,
    pub space_id: String,
    pub active_tab: ObsTab,
    pub monitors: Vec<SyntheticMonitor>,
    pub pinned_monitor_ids: Vec<String>,
    pub filter: String,
    pub is_loading: bool,
    pub error: Option<String>,
}

impl ObservabilityState {
    pub fn new() -> Self {
        Self {
            selected_cluster: String::new(),
            space_id: "default".to_string(),
            active_tab: ObsTab::Dashboard,
            monitors: Vec::new(),
            pinned_monitor_ids: Vec::new(),
            filter: String::new(),
            is_loading: false,
            error: None,
        }
    }

    pub fn generate_mocks_if_empty(&mut self) {
        if !self.monitors.is_empty() {
            return;
        }

        self.monitors = vec![
            SyntheticMonitor {
                id: "pay_gw".to_string(),
                name: "Stripe Payment Gateway".to_string(),
                monitor_type: "http".to_string(),
                status: "up".to_string(),
                url: "https://api.stripe.com/v3/charges".to_string(),
                locations: vec!["us-east-1".to_string(), "eu-central-1".to_string()],
                latency_ms: 124,
                latency_history: vec![120.0, 115.0, 130.0, 122.0, 125.0, 124.0],
                last_checked: "5s ago".to_string(),
            },
            SyntheticMonitor {
                id: "user_auth".to_string(),
                name: "Auth0 User Authentication".to_string(),
                monitor_type: "http".to_string(),
                status: "up".to_string(),
                url: "https://auth.apac-prod.zone/oauth/token".to_string(),
                locations: vec!["ap-southeast-2".to_string(), "us-west-2".to_string()],
                latency_ms: 85,
                latency_history: vec![90.0, 88.0, 84.0, 85.0, 92.0, 85.0],
                last_checked: "12s ago".to_string(),
            },
            SyntheticMonitor {
                id: "db_cluster".to_string(),
                name: "PostgreSQL Database Cluster".to_string(),
                monitor_type: "tcp".to_string(),
                status: "up".to_string(),
                url: "postgresql://db.prod-internal:5432".to_string(),
                locations: vec!["ap-southeast-2-lan".to_string()],
                latency_ms: 2,
                latency_history: vec![1.5, 2.0, 1.8, 2.2, 2.0, 2.0],
                last_checked: "2s ago".to_string(),
            },
            SyntheticMonitor {
                id: "frontend_edge".to_string(),
                name: "Cloudflare Frontend CDN".to_string(),
                monitor_type: "http".to_string(),
                status: "up".to_string(),
                url: "https://drastic-smurf.wtg.zone/index.html".to_string(),
                locations: vec!["us-east-1".to_string(), "sa-east-1".to_string(), "eu-west-1".to_string()],
                latency_ms: 45,
                latency_history: vec![40.0, 48.0, 42.0, 43.0, 46.0, 45.0],
                last_checked: "1m ago".to_string(),
            },
            SyntheticMonitor {
                id: "search_cluster".to_string(),
                name: "Elasticsearch Production Cluster".to_string(),
                monitor_type: "http".to_string(),
                status: "up".to_string(),
                url: "https://elastic.apac-prod-2.wtg.zone:443".to_string(),
                locations: vec!["ap-southeast-2".to_string()],
                latency_ms: 18,
                latency_history: vec![16.0, 20.0, 17.0, 19.0, 18.0, 18.0],
                last_checked: "15s ago".to_string(),
            },
            SyntheticMonitor {
                id: "billing_queue".to_string(),
                name: "Billing Queue Worker".to_string(),
                monitor_type: "icmp".to_string(),
                status: "down".to_string(),
                url: "10.2.61.11".to_string(),
                locations: vec!["ap-southeast-2".to_string()],
                latency_ms: 0,
                latency_history: vec![8.0, 10.0, 12.0, 0.0, 0.0, 0.0],
                last_checked: "5m ago".to_string(),
            },
        ];

        // Default pin some monitors to make Dashboard look awesome out-of-the-box!
        self.pinned_monitor_ids = vec![
            "pay_gw".to_string(),
            "user_auth".to_string(),
            "search_cluster".to_string(),
            "billing_queue".to_string(),
        ];
    }
}

pub fn render_observability_module(
    ui: &mut Ui,
    state: &mut ObservabilityState,
    clusters: &[String],
    on_refresh_monitors: &mut Option<(String, String)>, // (cluster_name, space_id)
) {
    ui.heading("Kibana Observability Monitors");
    ui.add_space(8.0);

    if clusters.is_empty() {
        ui.label("No clusters configured. Add a cluster first.");
        return;
    }

    if state.selected_cluster.is_empty() || !clusters.contains(&state.selected_cluster) {
        state.selected_cluster = clusters[0].clone();
        *on_refresh_monitors = Some((state.selected_cluster.clone(), state.space_id.clone()));
    }

    state.generate_mocks_if_empty();

    // Top control bar
    ui.horizontal(|ui| {
        ui.label("Cluster:");
        let prev_cluster = state.selected_cluster.clone();
        egui::ComboBox::from_id_salt("obs_cluster_select")
            .selected_text(&state.selected_cluster)
            .show_ui(ui, |ui| {
                for c in clusters {
                    ui.selectable_value(&mut state.selected_cluster, c.clone(), c);
                }
            });

        if state.selected_cluster != prev_cluster {
            state.is_loading = true;
            *on_refresh_monitors = Some((state.selected_cluster.clone(), state.space_id.clone()));
        }

        ui.add_space(12.0);

        ui.label("Kibana Space:");
        let prev_space = state.space_id.clone();
        ui.add(egui::TextEdit::singleline(&mut state.space_id).desired_width(100.0));
        
        if state.space_id != prev_space {
            if ui.button("Apply").clicked() {
                state.is_loading = true;
                *on_refresh_monitors = Some((state.selected_cluster.clone(), state.space_id.clone()));
            }
        }

        ui.add_space(16.0);

        // Tab selection
        let dash_btn = ui.selectable_label(
            matches!(state.active_tab, ObsTab::Dashboard),
            "📊 Pinned Dashboard",
        );
        if dash_btn.clicked() {
            state.active_tab = ObsTab::Dashboard;
        }

        let browse_btn = ui.selectable_label(
            matches!(state.active_tab, ObsTab::Browse),
            "🔍 Browse Monitors",
        );
        if browse_btn.clicked() {
            state.active_tab = ObsTab::Browse;
        }

        ui.add_space(16.0);

        if ui.button("🔄 Refresh").clicked() {
            state.is_loading = true;
            *on_refresh_monitors = Some((state.selected_cluster.clone(), state.space_id.clone()));
        }

        if state.is_loading {
            ui.spinner();
        }
    });

    ui.add_space(12.0);

    if let Some(err) = &state.error {
        ui.colored_label(Theme::danger(), format!("Kibana Link Error: {}", err));
        ui.label(RichText::new("Showing locally simulated/mock monitor states instead.").color(Theme::text_muted()).size(11.0));
        ui.add_space(8.0);
    }

    // Search filter
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

    // Main content display
    match state.active_tab {
        ObsTab::Browse => render_browse_tab(ui, state),
        ObsTab::Dashboard => render_dashboard_tab(ui, state),
    }
}

fn render_browse_tab(ui: &mut Ui, state: &mut ObservabilityState) {
    egui::Frame::new()
        .fill(Theme::bg_card())
        .corner_radius(Theme::CARD_ROUNDING)
        .inner_margin(Theme::CARD_PADDING)
        .show(ui, |ui| {
            let height = ui.available_height() - 16.0;
            egui::ScrollArea::vertical()
                .id_salt("obs_browse_scroll")
                .max_height(height)
                .auto_shrink([false, false])
                .show(ui, |ui| {
                    let filtered: Vec<&SyntheticMonitor> = state.monitors.iter()
                        .filter(|m| state.filter.is_empty() || m.name.to_lowercase().contains(&state.filter.to_lowercase()))
                        .collect();

                    if filtered.is_empty() {
                        ui.label(RichText::new("No monitors found").color(Theme::text_muted()));
                        return;
                    }

                    // Table Header
                    ui.horizontal(|ui| {
                        ui.set_min_height(24.0);
                        ui.label(RichText::new("Pin").strong().color(Theme::text_secondary()));
                        ui.add_space(20.0);
                        ui.label(RichText::new("Monitor Name").strong().color(Theme::text_secondary()));
                        let avail = ui.available_width() - 80.0;
                        ui.add_space(avail * 0.4);
                        ui.label(RichText::new("Status").strong().color(Theme::text_secondary()));
                        ui.add_space(40.0);
                        ui.label(RichText::new("Type").strong().color(Theme::text_secondary()));
                        ui.add_space(40.0);
                        ui.label(RichText::new("Latency").strong().color(Theme::text_secondary()));
                    });
                    ui.separator();

                    for m in filtered {
                        ui.horizontal(|ui| {
                            ui.set_min_height(28.0);
                            
                            // Pin Checkbox
                            let mut is_pinned = state.pinned_monitor_ids.contains(&m.id);
                            if ui.checkbox(&mut is_pinned, "").changed() {
                                if is_pinned {
                                    if !state.pinned_monitor_ids.contains(&m.id) {
                                        state.pinned_monitor_ids.push(m.id.clone());
                                    }
                                } else {
                                    state.pinned_monitor_ids.retain(|id| id != &m.id);
                                }
                            }

                            ui.add_space(15.0);

                            // Monitor Name & URL
                            ui.vertical(|ui| {
                                ui.label(RichText::new(&m.name).strong().color(Theme::text_primary()));
                                ui.label(RichText::new(&m.url).color(Theme::text_muted()).size(10.0));
                            });

                            let avail = ui.available_width() - 80.0;
                            ui.add_space(avail * 0.4);

                            // Status Pill
                            let (pill_text, pill_color) = if m.status == "up" {
                                ("UP", Theme::success())
                            } else {
                                ("DOWN", Theme::danger())
                            };
                            ui.add(crate::ui::widgets::StatePill::new(pill_text, pill_color));

                            ui.add_space(35.0);

                            // Type
                            ui.label(RichText::new(m.monitor_type.to_uppercase()).color(Theme::text_muted()).size(11.0));

                            ui.add_space(35.0);

                            // Latency
                            let latency = if m.status == "up" {
                                format!("{} ms", m.latency_ms)
                            } else {
                                "N/A".to_string()
                            };
                            ui.label(RichText::new(latency).color(Theme::text_muted()).size(11.0));
                        });
                        ui.separator();
                    }
                });
        });
}

fn render_dashboard_tab(ui: &mut Ui, state: &ObservabilityState) {
    let pinned_monitors: Vec<&SyntheticMonitor> = state.monitors.iter()
        .filter(|m| state.pinned_monitor_ids.contains(&m.id))
        .filter(|m| state.filter.is_empty() || m.name.to_lowercase().contains(&state.filter.to_lowercase()))
        .collect();

    if pinned_monitors.is_empty() {
        ui.horizontal(|ui| {
            ui.label(RichText::new("No pinned monitors in this dashboard.").color(Theme::text_muted()));
            ui.label(RichText::new("Go to 'Browse Monitors' tab to pin some monitors!").color(Theme::accent()));
        });
        return;
    }

    // Grid columns (2 column layout)
    let width = ui.available_width();
    let num_columns = if width > 500.0 { 2 } else { 1 };
    let card_w = (width - 12.0) / num_columns as f32;

    egui::ScrollArea::vertical()
        .id_salt("obs_dashboard_scroll")
        .show(ui, |ui| {
            ui.horizontal_wrapped(|ui| {
                for m in pinned_monitors {
                    egui::Frame::new()
                        .fill(Theme::bg_card())
                        .corner_radius(Theme::CARD_ROUNDING)
                        .inner_margin(Theme::CARD_PADDING)
                        .stroke(egui::Stroke::new(1.0, Theme::border()))
                        .show(ui, |ui| {
                            ui.set_width(card_w - 12.0);
                            
                            // Top Row: Status Dot & Name
                            ui.horizontal(|ui| {
                                let dot_color = if m.status == "up" {
                                    Theme::success()
                                } else {
                                    Theme::danger()
                                };
                                ui.add(crate::ui::widgets::ConnectionDot::new(true).color(dot_color).size(8.0));
                                ui.label(RichText::new(&m.name).strong().color(Theme::text_primary()).size(13.0));
                                
                                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                                    let pill_text = m.monitor_type.to_uppercase();
                                    ui.add(crate::ui::widgets::StatePill::new(pill_text, Theme::text_muted()));
                                });
                            });

                            ui.add_space(4.0);
                            ui.label(RichText::new(&m.url).color(Theme::text_muted()).size(10.0));
                            ui.add_space(8.0);

                            // Metrics grid
                            ui.horizontal(|ui| {
                                ui.vertical(|ui| {
                                    ui.label(RichText::new("Latency").color(Theme::text_muted()).size(10.0));
                                    let lat = if m.status == "up" {
                                        format!("{}ms", m.latency_ms)
                                    } else {
                                        "Offline".to_string()
                                    };
                                    ui.label(RichText::new(lat).strong().color(Theme::text_primary()).size(14.0));
                                });

                                ui.add_space(30.0);

                                ui.vertical(|ui| {
                                    ui.label(RichText::new("Last Checked").color(Theme::text_muted()).size(10.0));
                                    ui.label(RichText::new(&m.last_checked).strong().color(Theme::text_primary()).size(12.0));
                                });

                                ui.add_space(30.0);

                                ui.vertical(|ui| {
                                    ui.label(RichText::new("Locations").color(Theme::text_muted()).size(10.0));
                                    let loc_text = m.locations.join(", ");
                                    ui.label(RichText::new(loc_text).color(Theme::text_primary()).size(10.0));
                                });
                            });

                            ui.add_space(8.0);

                            // Sparkline showing latency history!
                            if m.status == "up" && !m.latency_history.is_empty() {
                                ui.label(RichText::new("Latency Trend (last 6 checks)").color(Theme::text_muted()).size(9.0));
                                ui.add_space(2.0);
                                let history_f64: Vec<f64> = m.latency_history.iter().map(|&x| x as f64).collect();
                                ui.add(crate::ui::widgets::MiniSparkline::new(history_f64)
                                    .color(Theme::accent())
                                    .width(card_w - 40.0)
                                    .height(20.0));
                            } else {
                                ui.add_space(22.0); // Keep height uniform
                            }
                        });
                }
            });
        });
}
