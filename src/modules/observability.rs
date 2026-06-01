use crate::ui::theme::Theme;
use egui::{RichText, Ui};

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
    pub pinned_monitor_layouts:
        std::collections::HashMap<String, crate::core::config::PinnedMonitorLayout>,
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
            pinned_monitor_layouts: std::collections::HashMap::new(),
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
                locations: vec![
                    "us-east-1".to_string(),
                    "sa-east-1".to_string(),
                    "eu-west-1".to_string(),
                ],
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
        if self.pinned_monitor_ids.is_empty() {
            self.pinned_monitor_ids = vec![
                "pay_gw".to_string(),
                "user_auth".to_string(),
                "search_cluster".to_string(),
                "billing_queue".to_string(),
            ];
        }
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
                *on_refresh_monitors =
                    Some((state.selected_cluster.clone(), state.space_id.clone()));
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
        ui.label(
            RichText::new("Showing locally simulated/mock monitor states instead.")
                .color(Theme::text_muted())
                .size(11.0),
        );
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
                    let filtered: Vec<&SyntheticMonitor> = state
                        .monitors
                        .iter()
                        .filter(|m| {
                            state.filter.is_empty()
                                || m.name.to_lowercase().contains(&state.filter.to_lowercase())
                        })
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
                        ui.label(
                            RichText::new("Monitor Name")
                                .strong()
                                .color(Theme::text_secondary()),
                        );
                        let avail = ui.available_width() - 80.0;
                        ui.add_space(avail * 0.4);
                        ui.label(
                            RichText::new("Status")
                                .strong()
                                .color(Theme::text_secondary()),
                        );
                        ui.add_space(40.0);
                        ui.label(
                            RichText::new("Type")
                                .strong()
                                .color(Theme::text_secondary()),
                        );
                        ui.add_space(40.0);
                        ui.label(
                            RichText::new("Latency")
                                .strong()
                                .color(Theme::text_secondary()),
                        );
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
                                ui.label(
                                    RichText::new(&m.name).strong().color(Theme::text_primary()),
                                );
                                ui.label(
                                    RichText::new(&m.url).color(Theme::text_muted()).size(10.0),
                                );
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
                            ui.label(
                                RichText::new(m.monitor_type.to_uppercase())
                                    .color(Theme::text_muted())
                                    .size(11.0),
                            );

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

fn render_dashboard_tab(ui: &mut Ui, state: &mut ObservabilityState) {
    let pinned_monitors: Vec<&SyntheticMonitor> = state
        .monitors
        .iter()
        .filter(|m| state.pinned_monitor_ids.contains(&m.id))
        .filter(|m| {
            state.filter.is_empty() || m.name.to_lowercase().contains(&state.filter.to_lowercase())
        })
        .collect();

    if pinned_monitors.is_empty() {
        ui.horizontal(|ui| {
            ui.label(
                RichText::new("No pinned monitors in this dashboard.").color(Theme::text_muted()),
            );
            ui.label(
                RichText::new("Go to 'Browse Monitors' tab to pin some monitors!")
                    .color(Theme::accent()),
            );
        });
        return;
    }

    // Help cue
    ui.horizontal(|ui| {
        ui.label(
            egui::RichText::new("💡 Pinned monitors are fully interactive floating widgets! Drag by title bars to arrange, and drag corners to resize them manually.")
                .color(Theme::text_muted())
                .size(11.0)
                .italics(),
        );
    });
    ui.add_space(8.0);

    let mut to_unpin = None;

    for (idx, m) in pinned_monitors.iter().enumerate() {
        let window_title = format!("🖥️ {} ({})", m.name, m.monitor_type.to_uppercase());
        let id = egui::Id::new(format!("obs_widget_{}", m.id));

        // Use saved layout if present
        let (default_x, default_y) = if let Some(layout) = state.pinned_monitor_layouts.get(&m.id) {
            (layout.x, layout.y)
        } else {
            let col = idx % 3;
            let row = idx / 3;
            (240.0 + (col as f32 * 340.0), 200.0 + (row as f32 * 210.0))
        };

        let (default_w, default_h) = if let Some(layout) = state.pinned_monitor_layouts.get(&m.id) {
            (layout.w, layout.h)
        } else {
            (320.0, 175.0)
        };

        let mut is_open = true;

        egui::Window::new(&window_title)
            .id(id)
            .open(&mut is_open)
            .default_size([default_w, default_h])
            .min_size([250.0, 130.0])
            .default_pos([default_x, default_y])
            .collapsible(true)
            .resizable(true)
            .show(ui.ctx(), |ui| {
                ui.vertical(|ui| {
                    // Top Row: Status Dot, Name/Status, Last Checked
                    ui.horizontal(|ui| {
                        let dot_color = if m.status == "up" {
                            Theme::success()
                        } else {
                            Theme::danger()
                        };
                        ui.add(
                            crate::ui::widgets::ConnectionDot::new(true)
                                .color(dot_color)
                                .size(8.0),
                        );
                        ui.label(
                            egui::RichText::new(m.status.to_uppercase())
                                .strong()
                                .color(dot_color)
                                .size(12.5),
                        );

                        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                            ui.label(
                                egui::RichText::new(&m.last_checked)
                                    .size(10.0)
                                    .color(Theme::text_muted()),
                            );
                        });
                    });

                    ui.add_space(3.0);
                    ui.label(
                        egui::RichText::new(&m.url)
                            .color(Theme::text_muted())
                            .size(10.0),
                    );
                    ui.separator();

                    // Metrics Row
                    ui.horizontal(|ui| {
                        ui.vertical(|ui| {
                            ui.label(
                                egui::RichText::new("Latency")
                                    .color(Theme::text_muted())
                                    .size(9.5),
                            );
                            let lat = if m.status == "up" {
                                format!("{}ms", m.latency_ms)
                            } else {
                                "Offline".to_string()
                            };
                            let lat_color = if m.status == "up" {
                                Theme::text_primary()
                            } else {
                                Theme::danger()
                            };
                            ui.label(
                                egui::RichText::new(lat)
                                    .strong()
                                    .color(lat_color)
                                    .size(13.5),
                            );
                        });

                        ui.add_space(24.0);

                        ui.vertical(|ui| {
                            ui.label(
                                egui::RichText::new("Locations")
                                    .color(Theme::text_muted())
                                    .size(9.5),
                            );
                            let loc_text = m.locations.join(", ");
                            ui.label(
                                egui::RichText::new(loc_text)
                                    .color(Theme::text_primary())
                                    .size(10.0),
                            );
                        });
                    });

                    ui.add_space(6.0);

                    // Latency sparkline
                    if m.status == "up" && !m.latency_history.is_empty() {
                        let history_f64: Vec<f64> =
                            m.latency_history.iter().map(|&x| x as f64).collect();
                        let available_w = ui.available_width();
                        ui.add(
                            crate::ui::widgets::MiniSparkline::new(history_f64)
                                .color(Theme::accent())
                                .width(available_w.max(100.0))
                                .height(22.0),
                        );
                    }
                });
            });

        // Query the window's real-time position/size to store layout persistently
        if let Some(rect) = ui.ctx().memory(|mem| mem.area_rect(id)) {
            state.pinned_monitor_layouts.insert(
                m.id.clone(),
                crate::core::config::PinnedMonitorLayout {
                    x: rect.min.x,
                    y: rect.min.y,
                    w: rect.width(),
                    h: rect.height(),
                },
            );
        }

        if !is_open {
            to_unpin = Some(m.id.clone());
        }
    }

    if let Some(id) = to_unpin {
        state.pinned_monitor_ids.retain(|x| x != &id);
        state.pinned_monitor_layouts.remove(&id);
    }
}
