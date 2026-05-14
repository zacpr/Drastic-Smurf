use std::collections::HashMap;
use std::sync::mpsc::{channel, Receiver, Sender};
use std::time::Instant;

use eframe::egui;

use crate::core::cluster_manager::ClusterManager;
use crate::core::config::ClusterConfig;
use crate::core::es_client::{ClusterHealth, EsClient};
use crate::modules::console::{render_console_module, ConsoleState};
use crate::modules::snapshot::{fetch_cluster_snapshot, render_snapshot_module, ClusterSnapshotStatus, SnapshotHistory};
use crate::modules::status::{render_status_module, StatusState};
use crate::modules::tasks::{render_tasks_module, TasksState};
use crate::ui::theme::Theme;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Tab {
    Snapshot,
    Status,
    Tasks,
    Console,
}

pub enum RefreshMsg {
    SnapshotResult(String, ClusterSnapshotStatus),
    HealthResult(String, Option<ClusterHealth>),
    StatsResult(String, Option<crate::core::es_client::ClusterStats>),
    TasksResult(String, Vec<crate::core::es_client::TaskInfo>),
    ConsoleResult(Result<serde_json::Value, String>),
    TestResult(String),
}

pub struct DrasticSmurfApp {
    pub cluster_manager: ClusterManager,
    pub current_tab: Tab,
    pub snapshot_statuses: Vec<ClusterSnapshotStatus>,
    pub snapshot_histories: HashMap<String, SnapshotHistory>,
    pub status_state: StatusState,
    pub tasks_state: TasksState,
    pub console_state: ConsoleState,
    pub auto_refresh: bool,
    pub refresh_interval_secs: u64,
    pub last_refresh: Option<Instant>,
    pub show_add_cluster: bool,
    pub editing_cluster: Option<String>,
    pub new_cluster: ClusterConfig,
    pub new_password: String,
    pub test_result: Option<String>,
    pub refresh_tx: Sender<RefreshMsg>,
    pub refresh_rx: Receiver<RefreshMsg>,
    pub pending_delete: Option<String>,
    pub console_send: Option<(String, String, String, Option<String>)>,
}

impl Default for DrasticSmurfApp {
    fn default() -> Self {
        let (tx, rx) = channel();
        let manager = ClusterManager::new();
        let _ = manager.load();

        let clusters = manager.clusters();
        let _cluster_names: Vec<String> = clusters.iter().map(|c| c.name.clone()).collect();

        Self {
            cluster_manager: manager,
            current_tab: Tab::Snapshot,
            snapshot_statuses: Vec::new(),
            snapshot_histories: HashMap::new(),
            status_state: StatusState::default(),
            tasks_state: TasksState::default(),
            console_state: ConsoleState::new(),
            auto_refresh: true,
            refresh_interval_secs: 15,
            last_refresh: None,
            show_add_cluster: false,
            editing_cluster: None,
            new_cluster: ClusterConfig::default(),
            new_password: String::new(),
            test_result: None,
            refresh_tx: tx,
            refresh_rx: rx,
            pending_delete: None,
            console_send: None,
        }
    }
}

impl DrasticSmurfApp {
    pub fn new(_cc: &eframe::CreationContext<'_>) -> Self {
        Self::default()
    }

    fn trigger_refresh(&mut self, ctx: &egui::Context) {
        let clusters = self.cluster_manager.clusters();

        for cluster in clusters {
            if let Some(client) = self.cluster_manager.get_client(&cluster.name) {
                let ctx1 = ctx.clone();
                let ctx2 = ctx.clone();
                let tx1 = self.refresh_tx.clone();
                let tx2 = self.refresh_tx.clone();
                let name1 = cluster.name.clone();
                let name2 = cluster.name.clone();
                let cluster2 = cluster.clone();

                // Snapshot refresh
                tokio::spawn(async move {
                    let status = fetch_cluster_snapshot(&client, &cluster2).await;
                    let _ = tx1.send(RefreshMsg::SnapshotResult(name1, status));
                    ctx1.request_repaint();
                });

                // Health refresh
                if let Some(client2) = self.cluster_manager.get_client(&cluster.name) {
                    tokio::spawn(async move {
                        let health = client2.cluster_health().await.ok();
                        let _ = tx2.send(RefreshMsg::HealthResult(name2.clone(), health));
                        ctx2.request_repaint();
                    });
                }

                // Stats refresh
                let ctx3 = ctx.clone();
                let tx3 = self.refresh_tx.clone();
                let name3 = cluster.name.clone();
                if let Some(client3) = self.cluster_manager.get_client(&cluster.name) {
                    tokio::spawn(async move {
                        let stats = client3.cluster_stats().await.ok();
                        let _ = tx3.send(RefreshMsg::StatsResult(name3, stats));
                        ctx3.request_repaint();
                    });
                }

                // Tasks refresh
                let ctx4 = ctx.clone();
                let tx4 = self.refresh_tx.clone();
                let name4 = cluster.name.clone();
                if let Some(client4) = self.cluster_manager.get_client(&cluster.name) {
                    tokio::spawn(async move {
                        let tasks = client4.tasks(Some("*reindex*,*snapshot*")).await.ok();
                        if let Some(t) = tasks {
                            let items: Vec<_> = t.nodes.into_values()
                                .flat_map(|n| n.tasks.into_values())
                                .collect();
                            let _ = tx4.send(RefreshMsg::TasksResult(name4, items));
                        }
                        ctx4.request_repaint();
                    });
                }
            }
        }

        self.last_refresh = Some(Instant::now());
    }

    fn process_refresh_results(&mut self) {
        while let Ok(msg) = self.refresh_rx.try_recv() {
            match msg {
                RefreshMsg::SnapshotResult(name, status) => {
                    // Update speed history
                    if let Some(ref stats) = status.snapshot_stats {
                        let history = self.snapshot_histories.entry(name.clone()).or_default();
                        let (bps, _sps) = history.update(stats.processed_bytes, stats.processed_shards);
                        let (window_avg, min_bps, max_bps) = history.window_stats();
                        
                        // Find and update the status
                        if let Some(existing) = self.snapshot_statuses.iter_mut().find(|s| s.config.name == name) {
                            existing.snapshot_stats = Some(crate::modules::snapshot::SnapshotStats {
                                current_speed_bps: bps,
                                avg_speed_bps: window_avg,
                                window_avg_speed_bps: window_avg,
                                min_speed_bps: min_bps,
                                max_speed_bps: max_bps,
                                ..stats.clone()
                            });
                        } else {
                            self.snapshot_statuses.push(status);
                        }
                    } else {
                        if let Some(existing) = self.snapshot_statuses.iter_mut().find(|s| s.config.name == name) {
                            *existing = status;
                        } else {
                            self.snapshot_statuses.push(status);
                        }
                    }
                }
                RefreshMsg::HealthResult(name, health) => {
                    if let Some(existing) = self.status_state.health_data.iter_mut().find(|(n, _)| n == &name) {
                        existing.1 = health;
                    } else {
                        self.status_state.health_data.push((name, health));
                    }
                }
                RefreshMsg::StatsResult(name, stats) => {
                    if let Some(existing) = self.status_state.stats_data.iter_mut().find(|(n, _)| n == &name) {
                        existing.1 = stats;
                    } else {
                        self.status_state.stats_data.push((name, stats));
                    }
                }
                RefreshMsg::TasksResult(name, tasks) => {
                    self.tasks_state.tasks.retain(|(n, _)| n != &name);
                    for task in tasks {
                        self.tasks_state.tasks.push((name.clone(), task));
                    }
                }
                RefreshMsg::ConsoleResult(result) => {
                    self.console_state.response = match result {
                        Ok(val) => serde_json::to_string_pretty(&val).unwrap_or_else(|e| e.to_string()),
                        Err(e) => format!("Error: {}", e),
                    };
                    self.console_state.is_loading = false;
                }
                RefreshMsg::TestResult(msg) => {
                    self.test_result = Some(msg);
                }
            }
        }
    }

    fn render_sidebar(&mut self, ui: &mut egui::Ui) {
        let sidebar_width = 220.0;
        let frame = egui::Frame::new()
            .fill(Theme::BG_DARKEST)
            .inner_margin(egui::Vec2::new(12.0, 16.0));

        frame.show(ui, |ui| {
            ui.set_min_width(sidebar_width);
            ui.set_max_width(sidebar_width);

            ui.heading(egui::RichText::new("DRASTIC SMURF").color(Theme::ACCENT).size(18.0));
            ui.label(egui::RichText::new("ES Multi-Tool").color(Theme::TEXT_MUTED).size(11.0));
            ui.add_space(20.0);

            ui.label(egui::RichText::new("Clusters").strong().color(Theme::TEXT_SECONDARY).size(12.0));
            ui.add_space(4.0);

            let clusters = self.cluster_manager.clusters();
            for cluster in &clusters {
                ui.horizontal(|ui| {
                    let connected = self.status_state.health_data.iter().any(|(n, h)| n == &cluster.name && h.is_some());
                    ui.add(crate::ui::widgets::ConnectionDot::new(connected).size(8.0));
                    ui.label(egui::RichText::new(&cluster.name).color(Theme::TEXT_PRIMARY).size(13.0));
                });
            }

            if clusters.is_empty() {
                ui.label(egui::RichText::new("No clusters configured").color(Theme::TEXT_MUTED).size(11.0));
            }

            ui.add_space(12.0);
            if ui.button("+ Add Cluster").clicked() {
                self.show_add_cluster = true;
                self.editing_cluster = None;
                self.new_cluster = ClusterConfig::default();
                self.new_password = String::new();
                self.test_result = None;
            }

            ui.add_space(20.0);
            ui.separator();
            ui.add_space(8.0);

            ui.checkbox(&mut self.auto_refresh, "Auto Refresh");
            ui.horizontal(|ui| {
                ui.label("Interval:");
                ui.add(egui::DragValue::new(&mut self.refresh_interval_secs).speed(1).range(5..=300));
                ui.label("s");
            });

            if let Some(last) = self.last_refresh {
                let ago = last.elapsed().as_secs();
                ui.label(egui::RichText::new(format!("Last refresh: {}s ago", ago)).size(10.0).color(Theme::TEXT_MUTED));
            }
        });
    }

    fn render_tabs(&mut self, ui: &mut egui::Ui) {
        ui.horizontal(|ui| {
            for (label, tab) in [
                ("Snapshot", Tab::Snapshot),
                ("Status", Tab::Status),
                ("Tasks", Tab::Tasks),
                ("Console", Tab::Console),
            ] {
                let is_active = self.current_tab == tab;
                let text = egui::RichText::new(label).size(14.0);
                let text = if is_active {
                    text.color(Theme::ACCENT).strong()
                } else {
                    text.color(Theme::TEXT_SECONDARY)
                };
                if ui.selectable_label(is_active, text).clicked() {
                    self.current_tab = tab;
                }
            }
        });
        ui.separator();
    }

    fn render_content(&mut self, ui: &mut egui::Ui) {
        match self.current_tab {
            Tab::Snapshot => {
                let mut on_edit = None;
                let mut on_delete = None;
                render_snapshot_module(ui, &self.snapshot_statuses, &self.snapshot_histories, &mut on_edit, &mut on_delete);
                if let Some(name) = on_edit {
                    if let Some(cluster) = self.cluster_manager.clusters().into_iter().find(|c| c.name == name) {
                        self.editing_cluster = Some(name.clone());
                        self.new_cluster = cluster;
                        self.new_password = crate::core::auth::get_password(&name).unwrap_or_default().unwrap_or_default();
                        self.show_add_cluster = true;
                        self.test_result = None;
                    }
                }
                if let Some(name) = on_delete {
                    self.pending_delete = Some(name);
                }
            }
            Tab::Status => {
                render_status_module(ui, &self.status_state);
            }
            Tab::Tasks => {
                render_tasks_module(ui, &mut self.tasks_state);
            }
            Tab::Console => {
                let names: Vec<String> = self.cluster_manager.clusters().iter().map(|c| c.name.clone()).collect();
                if self.console_state.selected_cluster.is_empty() && !names.is_empty() {
                    self.console_state.selected_cluster = names[0].clone();
                }
                render_console_module(ui, &mut self.console_state, &names, &mut self.console_send);
            }
        }
    }

    fn render_add_cluster_dialog(&mut self, ctx: &egui::Context) {
        if !self.show_add_cluster {
            return;
        }

        let title = if self.editing_cluster.is_some() {
            "Edit Cluster"
        } else {
            "Add Cluster"
        };

        egui::Window::new(title)
            .collapsible(false)
            .resizable(false)
            .anchor(egui::Align2::CENTER_CENTER, egui::Vec2::ZERO)
            .show(ctx, |ui| {
                ui.set_min_width(400.0);

                ui.horizontal(|ui| {
                    ui.label("Name:");
                    ui.text_edit_singleline(&mut self.new_cluster.name);
                });
                ui.horizontal(|ui| {
                    ui.label("Host:");
                    ui.text_edit_singleline(&mut self.new_cluster.host);
                });
                ui.horizontal(|ui| {
                    ui.label("Username:");
                    ui.text_edit_singleline(&mut self.new_cluster.username);
                });
                ui.horizontal(|ui| {
                    ui.label("Password:");
                    ui.add(egui::TextEdit::singleline(&mut self.new_password).password(true));
                });
                ui.horizontal(|ui| {
                    ui.label("Snapshot Repo:");
                    ui.text_edit_singleline(&mut self.new_cluster.snapshot_repo);
                });
                ui.horizontal(|ui| {
                    ui.label("SLM Policy:");
                    ui.text_edit_singleline(&mut self.new_cluster.slm_policy);
                });
                ui.checkbox(&mut self.new_cluster.verify_ssl, "Verify SSL");

                ui.add_space(8.0);
                ui.horizontal(|ui| {
                    if ui.button("Test Connection").clicked() {
                        let client = EsClient::with_password(&self.new_cluster, &self.new_password);
                        match client {
                            Ok(c) => {
                                let ctx = ctx.clone();
                                let tx = self.refresh_tx.clone();
                                tokio::spawn(async move {
                                    let result = c.cluster_health().await;
                                    let msg = match result {
                                        Ok(h) => format!("Connected! Cluster: {}, Status: {}", h.cluster_name, h.status),
                                        Err(e) => format!("Failed: {}", e),
                                    };
                                    let _ = tx.send(RefreshMsg::TestResult(msg));
                                    ctx.request_repaint();
                                });
                                self.test_result = Some("Testing...".to_string());
                            }
                            Err(e) => {
                                self.test_result = Some(format!("Client error: {}", e));
                            }
                        }
                    }

                    if let Some(ref result) = self.test_result {
                        ui.label(result);
                    }
                });

                ui.add_space(12.0);
                ui.horizontal(|ui| {
                    if ui.button("Save").clicked() {
                        if !self.new_cluster.name.is_empty() && !self.new_cluster.host.is_empty() {
                            let name = self.new_cluster.name.clone();
                            let _ = crate::core::auth::set_password(&name, &self.new_password);

                            if let Some(ref old_name) = self.editing_cluster {
                                let _ = self.cluster_manager.update_cluster(old_name, self.new_cluster.clone());
                            } else {
                                let _ = self.cluster_manager.add_cluster(self.new_cluster.clone());
                            }
                            self.show_add_cluster = false;
                            self.editing_cluster = None;
                        }
                    }
                    if ui.button("Cancel").clicked() {
                        self.show_add_cluster = false;
                        self.editing_cluster = None;
                    }
                });
            });
    }

    fn render_delete_confirmation(&mut self, ctx: &egui::Context) {
        let name = self.pending_delete.clone();
        if let Some(name) = name {
            egui::Window::new("Confirm Delete")
                .collapsible(false)
                .resizable(false)
                .anchor(egui::Align2::CENTER_CENTER, egui::Vec2::ZERO)
                .show(ctx, |ui| {
                    ui.label(format!("Are you sure you want to delete cluster '{}'?", name));
                    ui.horizontal(|ui| {
                        if ui.button("Delete").clicked() {
                            let _ = self.cluster_manager.remove_cluster(&name);
                            self.snapshot_statuses.retain(|s| s.config.name != name);
                            self.status_state.health_data.retain(|(n, _)| n != &name);
                            self.pending_delete = None;
                        }
                        if ui.button("Cancel").clicked() {
                            self.pending_delete = None;
                        }
                    });
                });
        }
    }
}

impl eframe::App for DrasticSmurfApp {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        // Process async results
        self.process_refresh_results();

        // Handle console send
        if let Some((cluster_name, method, path, body)) = self.console_send.take() {
            if let Some(client) = self.cluster_manager.get_client(&cluster_name) {
                let tx = self.refresh_tx.clone();
                let ctx = ctx.clone();
                tokio::spawn(async move {
                    let method = match method.as_str() {
                        "GET" => reqwest::Method::GET,
                        "POST" => reqwest::Method::POST,
                        "PUT" => reqwest::Method::PUT,
                        "DELETE" => reqwest::Method::DELETE,
                        "HEAD" => reqwest::Method::HEAD,
                        _ => reqwest::Method::GET,
                    };
                    let result = client.execute(method, &path, body).await
                        .map_err(|e| e.to_string());
                    let _ = tx.send(RefreshMsg::ConsoleResult(result));
                    ctx.request_repaint();
                });
            }
        }

        // Auto refresh
        if self.auto_refresh {
            let should_refresh = self.last_refresh.map_or(true, |last| {
                last.elapsed().as_secs() >= self.refresh_interval_secs
            });
            if should_refresh {
                self.trigger_refresh(ctx);
            }
        }

        // Main layout
        egui::CentralPanel::default()
            .frame(egui::Frame::new().fill(Theme::BG_DARK))
            .show(ctx, |ui| {
                ui.horizontal(|ui| {
                    // Sidebar
                    ui.vertical(|ui| {
                        self.render_sidebar(ui);
                    });

                    ui.separator();

                    // Main content
                    ui.vertical(|ui| {
                        ui.set_min_width(ui.available_width());
                        self.render_tabs(ui);
                        self.render_content(ui);
                    });
                });
            });

        // Dialogs
        self.render_add_cluster_dialog(ctx);
        self.render_delete_confirmation(ctx);
    }
}
