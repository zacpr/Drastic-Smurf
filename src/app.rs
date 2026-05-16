use std::collections::HashMap;
use std::sync::mpsc::{Receiver, Sender, channel};
use std::time::Instant;

use eframe::egui;

use crate::core::cluster_manager::ClusterManager;
use crate::core::config::ClusterConfig;
use crate::core::es_client::{ClusterHealth, EsClient};
use crate::modules::clusters::{ClustersState, render_clusters_module};
use crate::modules::console::{ConsoleState, render_console_module};
use crate::modules::snapshot::{
    ClusterSnapshotStatus, SnapshotHistory, fetch_cluster_snapshot, render_snapshot_module,
};
use crate::modules::status::{StatusState, render_status_module};
use crate::modules::tasks::{TasksState, render_tasks_module};
use crate::ui::theme::Theme;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Tab {
    Clusters,
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
    pub clusters_state: ClustersState,
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
    pub clusters_import: Option<crate::core::config::AppConfig>,
}

impl Default for DrasticSmurfApp {
    fn default() -> Self {
        let (tx, rx) = channel();
        let manager = ClusterManager::new();
        let _ = manager.load();

        let clusters = manager.clusters();
        let cluster_names: Vec<String> = clusters.iter().map(|c| c.name.clone()).collect();

        let mut console_state = ConsoleState::new();
        if let Some(first) = cluster_names.first() {
            console_state.selected_cluster = first.clone();
        }

        let mut app = Self {
            cluster_manager: manager.clone(),
            current_tab: Tab::Snapshot,
            snapshot_statuses: Vec::new(),
            snapshot_histories: HashMap::new(),
            status_state: StatusState::default(),
            tasks_state: TasksState::default(),
            console_state,
            clusters_state: ClustersState::default(),
            auto_refresh: manager.auto_refresh(),
            refresh_interval_secs: manager.refresh_interval_secs(),
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
            clusters_import: None,
        };

        // Pre-populate module state from cached cluster data
        for cluster in &clusters {
            if let Some(data) = manager.get_cluster_data(&cluster.name) {
                // Status
                if let Some(latest) = data.status_history.last() {
                    app.status_state.health_data.push((cluster.name.clone(), latest.health.clone()));
                    app.status_state.stats_data.push((cluster.name.clone(), latest.stats.clone()));
                }
                // Tasks
                if let Some(latest) = data.tasks_cache.last() {
                    for task in &latest.tasks {
                        app.tasks_state.tasks.push((cluster.name.clone(), task.clone()));
                    }
                }
                // Snapshot
                if let Some(latest) = data.snapshot_cache.last() {
                    let status = ClusterSnapshotStatus {
                        config: cluster.clone(),
                        reachable: latest.reachable,
                        error_message: latest.error_message.clone(),
                        snapshot_info: latest.snapshot_info.clone(),
                        snapshot_stats: latest.snapshot_stats.clone(),
                        slm_last_run: latest.slm_last_run.clone(),
                        slm_next_run: latest.slm_next_run.clone(),
                        slm_in_progress: latest.slm_in_progress,
                    };
                    app.snapshot_statuses.push(status);
                }
            }
        }

        app
    }
}

impl DrasticSmurfApp {
    pub fn new(_cc: &eframe::CreationContext<'_>) -> Self {
        Self::default()
    }

    fn trigger_refresh(&mut self, ctx: &egui::Context) {
        let clusters = self.cluster_manager.clusters();

        for cluster in clusters {
            let manager = self.cluster_manager.clone();
            let ctx = ctx.clone();
            let tx = self.refresh_tx.clone();
            let name = cluster.name.clone();

            tokio::spawn(async move {
                // Ensure SSH tunnel is up if configured
                if let Err(e) = manager.ensure_tunnel(&name).await {
                    tracing::warn!("SSH tunnel failed for '{}': {}", name, e);
                }

                if let Some(client) = manager.get_client(&name) {
                    // Snapshot refresh
                    let tx2 = tx.clone();
                    let name2 = name.clone();
                    let ctx2 = ctx.clone();
                    let manager2 = manager.clone();
                    tokio::spawn(async move {
                        let config = manager2.clusters().into_iter().find(|c| c.name == name2);
                        if let Some(config) = config {
                            let status = fetch_cluster_snapshot(&client, &config).await;
                            let _ = tx2.send(RefreshMsg::SnapshotResult(name2, status));
                        }
                        ctx2.request_repaint();
                    });

                    // Health refresh
                    let tx3 = tx.clone();
                    let name3 = name.clone();
                    let ctx3 = ctx.clone();
                    let manager3 = manager.clone();
                    tokio::spawn(async move {
                        if let Some(client) = manager3.get_client(&name3) {
                            let health = client.cluster_health().await.ok();
                            let _ = tx3.send(RefreshMsg::HealthResult(name3, health));
                        }
                        ctx3.request_repaint();
                    });

                    // Stats refresh
                    let tx4 = tx.clone();
                    let name4 = name.clone();
                    let ctx4 = ctx.clone();
                    let manager4 = manager.clone();
                    tokio::spawn(async move {
                        if let Some(client) = manager4.get_client(&name4) {
                            let stats = client.cluster_stats().await.ok();
                            let _ = tx4.send(RefreshMsg::StatsResult(name4, stats));
                        }
                        ctx4.request_repaint();
                    });

                    // Tasks refresh
                    let name5 = name.clone();
                    let ctx5 = ctx.clone();
                    let manager5 = manager.clone();
                    tokio::spawn(async move {
                        if let Some(client) = manager5.get_client(&name5) {
                            let tasks = client.tasks(Some("*reindex*,*snapshot*")).await.ok();
                            if let Some(t) = tasks {
                                let items: Vec<_> = t
                                    .nodes
                                    .into_values()
                                    .flat_map(|n| n.tasks.into_values())
                                    .collect();
                                let _ = tx.send(RefreshMsg::TasksResult(name5, items));
                            }
                        }
                        ctx5.request_repaint();
                    });
                }
            });
        }

        self.last_refresh = Some(Instant::now());
    }

    fn process_refresh_results(&mut self) {
        while let Ok(msg) = self.refresh_rx.try_recv() {
            match msg {
                RefreshMsg::SnapshotResult(name, status) => {
                    let status_for_cache = status.clone();
                    // Update speed history
                    if let Some(ref stats) = status.snapshot_stats {
                        let history = self.snapshot_histories.entry(name.clone()).or_default();
                        let (bps, _sps) =
                            history.update(stats.processed_bytes, stats.processed_shards);
                        let (window_avg, min_bps, max_bps) = history.window_stats();

                        // Find and update the status
                        if let Some(existing) = self
                            .snapshot_statuses
                            .iter_mut()
                            .find(|s| s.config.name == name)
                        {
                            existing.snapshot_stats =
                                Some(crate::modules::snapshot::SnapshotStats {
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
                        if let Some(existing) = self
                            .snapshot_statuses
                            .iter_mut()
                            .find(|s| s.config.name == name)
                        {
                            *existing = status;
                        } else {
                            self.snapshot_statuses.push(status);
                        }
                    }
                    // Cache snapshot data
                    self.save_snapshot_cache(&name, &status_for_cache);
                }
                RefreshMsg::HealthResult(name, health) => {
                    if let Some(existing) = self
                        .status_state
                        .health_data
                        .iter_mut()
                        .find(|(n, _)| n == &name)
                    {
                        existing.1 = health.clone();
                    } else {
                        self.status_state.health_data.push((name.clone(), health.clone()));
                    }
                    // Try to save status snapshot when both health and stats are available
                    let stats = self.status_state.stats_data.iter().find(|(n, _)| n == &name).and_then(|(_, s)| s.clone());
                    self.save_status_snapshot(&name, health, stats);
                }
                RefreshMsg::StatsResult(name, stats) => {
                    if let Some(existing) = self
                        .status_state
                        .stats_data
                        .iter_mut()
                        .find(|(n, _)| n == &name)
                    {
                        existing.1 = stats.clone();
                    } else {
                        self.status_state.stats_data.push((name.clone(), stats.clone()));
                    }
                    // Try to save status snapshot when both health and stats are available
                    let health = self.status_state.health_data.iter().find(|(n, _)| n == &name).and_then(|(_, h)| h.clone());
                    self.save_status_snapshot(&name, health, stats);
                }
                RefreshMsg::TasksResult(name, tasks) => {
                    self.tasks_state.tasks.retain(|(n, _)| n != &name);
                    for task in tasks.iter().cloned() {
                        self.tasks_state.tasks.push((name.clone(), task));
                    }
                    self.save_tasks_cache(&name, tasks);
                }
                RefreshMsg::ConsoleResult(result) => {
                    self.console_state.response = match result {
                        Ok(val) => {
                            serde_json::to_string_pretty(&val).unwrap_or_else(|e| e.to_string())
                        }
                        Err(e) => format!("Error: {}", e),
                    };
                    self.console_state.is_loading = false;
                }
                RefreshMsg::TestResult(msg) => {
                    self.test_result = Some(msg.clone());
                    self.clusters_state.test_result = Some(msg);
                }
            }
        }
    }

    fn save_status_snapshot(
        &self,
        name: &str,
        health: Option<ClusterHealth>,
        stats: Option<crate::core::es_client::ClusterStats>,
    ) {
        let snapshot = crate::core::config::StatusSnapshot {
            timestamp: chrono::Utc::now(),
            health,
            stats,
        };
        if let Some(mut data) = self.cluster_manager.get_cluster_data(name) {
            data.status_history.push(snapshot);
            while data.status_history.len() > 100 {
                data.status_history.remove(0);
            }
            self.cluster_manager.set_cluster_data(name, data);
        } else {
            let mut data = crate::core::config::ClusterData::default();
            data.status_history.push(snapshot);
            self.cluster_manager.set_cluster_data(name, data);
        }
        let _ = self.cluster_manager.save();
    }

    fn save_tasks_cache(&self, name: &str, tasks: Vec<crate::core::es_client::TaskInfo>) {
        let entry = crate::core::config::TaskCacheEntry {
            timestamp: chrono::Utc::now(),
            tasks,
        };
        if let Some(mut data) = self.cluster_manager.get_cluster_data(name) {
            data.tasks_cache.push(entry);
            while data.tasks_cache.len() > 20 {
                data.tasks_cache.remove(0);
            }
            self.cluster_manager.set_cluster_data(name, data);
        } else {
            let mut data = crate::core::config::ClusterData::default();
            data.tasks_cache.push(entry);
            self.cluster_manager.set_cluster_data(name, data);
        }
        let _ = self.cluster_manager.save();
    }

    fn save_snapshot_cache(&self, name: &str, status: &ClusterSnapshotStatus) {
        let entry = crate::core::config::SnapshotCacheEntry {
            timestamp: chrono::Utc::now(),
            reachable: status.reachable,
            error_message: status.error_message.clone(),
            snapshot_info: status.snapshot_info.clone(),
            snapshot_stats: status.snapshot_stats.clone(),
            slm_last_run: status.slm_last_run.clone(),
            slm_next_run: status.slm_next_run.clone(),
            slm_in_progress: status.slm_in_progress,
        };
        if let Some(mut data) = self.cluster_manager.get_cluster_data(name) {
            data.snapshot_cache.push(entry);
            while data.snapshot_cache.len() > 50 {
                data.snapshot_cache.remove(0);
            }
            self.cluster_manager.set_cluster_data(name, data);
        } else {
            let mut data = crate::core::config::ClusterData::default();
            data.snapshot_cache.push(entry);
            self.cluster_manager.set_cluster_data(name, data);
        }
        let _ = self.cluster_manager.save();
    }

    fn render_sidebar(&mut self, ui: &mut egui::Ui) {
        let sidebar_width = 220.0;
        let frame = egui::Frame::new()
            .fill(Theme::BG_DARKEST)
            .inner_margin(egui::Vec2::new(12.0, 16.0));

        frame.show(ui, |ui| {
            ui.set_min_width(sidebar_width);
            ui.set_max_width(sidebar_width);

            ui.heading(
                egui::RichText::new("DRASTIC SMURF")
                    .color(Theme::ACCENT)
                    .size(18.0),
            );
            ui.label(
                egui::RichText::new("ES Multi-Tool")
                    .color(Theme::TEXT_MUTED)
                    .size(11.0),
            );
            ui.add_space(20.0);

            ui.label(
                egui::RichText::new("Clusters")
                    .strong()
                    .color(Theme::TEXT_SECONDARY)
                    .size(12.0),
            );
            ui.add_space(4.0);

            let clusters = self.cluster_manager.clusters();
            for cluster in &clusters {
                ui.horizontal(|ui| {
                    let connected = self
                        .status_state
                        .health_data
                        .iter()
                        .any(|(n, h)| n == &cluster.name && h.is_some());
                    ui.add(crate::ui::widgets::ConnectionDot::new(connected).size(8.0));
                    ui.label(
                        egui::RichText::new(&cluster.name)
                            .color(Theme::TEXT_PRIMARY)
                            .size(13.0),
                    );
                });
            }

            if clusters.is_empty() {
                ui.label(
                    egui::RichText::new("No clusters configured")
                        .color(Theme::TEXT_MUTED)
                        .size(11.0),
                );
            }

            ui.add_space(12.0);
            if ui.button("+ Add Cluster").clicked() {
                self.current_tab = Tab::Clusters;
                self.clusters_state.selected_cluster = None;
                self.clusters_state.editing_cluster = None;
                self.clusters_state.edit_form = ClusterConfig::default();
                self.clusters_state.edit_password.clear();
                self.clusters_state.test_result = None;
            }

            ui.add_space(20.0);
            ui.separator();
            ui.add_space(8.0);

            let mut auto_refresh_changed = false;
            let mut interval_changed = false;
            let _old_auto = self.auto_refresh;
            let _old_interval = self.refresh_interval_secs;

            if ui.checkbox(&mut self.auto_refresh, "Auto Refresh").changed() {
                auto_refresh_changed = true;
            }
            ui.horizontal(|ui| {
                ui.label("Interval:");
                if ui
                    .add(
                        egui::DragValue::new(&mut self.refresh_interval_secs)
                            .speed(1)
                            .range(5..=300),
                    )
                    .changed()
                {
                    interval_changed = true;
                }
                ui.label("s");
            });

            if auto_refresh_changed || interval_changed {
                self.cluster_manager.set_auto_refresh(self.auto_refresh);
                self.cluster_manager.set_refresh_interval_secs(self.refresh_interval_secs);
                let _ = self.cluster_manager.save();
            }

            if ui.button("🔄 Refresh Now").clicked() {
                self.trigger_refresh(ui.ctx());
            }

            if let Some(last) = self.last_refresh {
                let ago = last.elapsed().as_secs();
                ui.label(
                    egui::RichText::new(format!("Last refresh: {}s ago", ago))
                        .size(10.0)
                        .color(Theme::TEXT_MUTED),
                );
            }
        });
    }

    fn render_tabs(&mut self, ui: &mut egui::Ui) {
        ui.horizontal(|ui| {
            for (label, tab) in [
                ("Clusters", Tab::Clusters),
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
            Tab::Clusters => {
                let clusters = self.cluster_manager.clusters();
                let data = self.cluster_manager.all_cluster_data();
                let mut on_save = None;
                let mut on_delete = None;
                let mut on_test = None;
                let mut on_import = None;
                render_clusters_module(
                    ui,
                    &mut self.clusters_state,
                    &clusters,
                    &data,
                    &mut on_save,
                    &mut on_delete,
                    &mut on_test,
                    &mut on_import,
                );
                if let Some((old_name, config, password)) = on_save {
                    let _ = crate::core::auth::set_password(&config.name, &password);
                    if let Some(old) = old_name {
                        let _ = self.cluster_manager.update_cluster(&old, config);
                    } else {
                        let _ = self.cluster_manager.add_cluster(config);
                    }
                }
                if let Some(name) = on_delete {
                    self.pending_delete = Some(name);
                }
                if let Some((name, password)) = on_test {
                    if let Ok(client) = EsClient::with_password(
                        &self
                            .cluster_manager
                            .clusters()
                            .into_iter()
                            .find(|c| c.name == name)
                            .unwrap_or_default(),
                        &password,
                    ) {
                        let ctx = ui.ctx().clone();
                        let tx = self.refresh_tx.clone();
                        tokio::spawn(async move {
                            let result = client.cluster_health().await;
                            let msg = match result {
                                Ok(h) => format!(
                                    "Connected! Cluster: {}, Status: {}",
                                    h.cluster_name, h.status
                                ),
                                Err(e) => format!("Failed: {}", e),
                            };
                            let _ = tx.send(RefreshMsg::TestResult(msg));
                            ctx.request_repaint();
                        });
                        self.clusters_state.test_result = Some("Testing...".to_string());
                    }
                }
                if let Some(imported) = on_import {
                    self.clusters_import = Some(imported);
                }
            }
            Tab::Snapshot => {
                let mut on_edit = None;
                let mut on_delete = None;
                render_snapshot_module(
                    ui,
                    &self.snapshot_statuses,
                    &self.snapshot_histories,
                    &mut on_edit,
                    &mut on_delete,
                );
                if let Some(name) = on_edit {
                    if let Some(cluster) = self
                        .cluster_manager
                        .clusters()
                        .into_iter()
                        .find(|c| c.name == name)
                    {
                        self.editing_cluster = Some(name.clone());
                        self.new_cluster = cluster;
                        self.new_password = crate::core::auth::get_password(&name)
                            .unwrap_or_default()
                            .unwrap_or_default();
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
                let names: Vec<String> = self
                    .cluster_manager
                    .clusters()
                    .iter()
                    .map(|c| c.name.clone())
                    .collect();
                if self.console_state.selected_cluster.is_empty() && !names.is_empty() {
                    self.console_state.selected_cluster = names[0].clone();
                }
                let mut on_save_query = None;
                render_console_module(
                    ui,
                    &mut self.console_state,
                    &names,
                    &mut self.console_send,
                    &mut on_save_query,
                );
                if let Some(query) = on_save_query {
                    let cluster = &self.console_state.selected_cluster;
                    if let Some(mut data) = self.cluster_manager.get_cluster_data(cluster) {
                        // Replace if name exists
                        if let Some(idx) = data.saved_queries.iter().position(|q| q.name == query.name) {
                            data.saved_queries[idx] = query;
                        } else {
                            data.saved_queries.push(query);
                        }
                        self.cluster_manager.set_cluster_data(cluster, data);
                    } else {
                        let mut data = crate::core::config::ClusterData::default();
                        data.saved_queries.push(query);
                        self.cluster_manager.set_cluster_data(cluster, data);
                    }
                    // Also update console state so the dropdown shows immediately
                    if let Some(data) = self.cluster_manager.get_cluster_data(cluster) {
                        self.console_state.saved_queries = data.saved_queries;
                    }
                    let _ = self.cluster_manager.save();
                }
                // Load saved queries when cluster selection changes
                let selected = &self.console_state.selected_cluster;
                let current_queries: Vec<String> = self.console_state.saved_queries.iter().map(|q| q.name.clone()).collect();
                if let Some(data) = self.cluster_manager.get_cluster_data(selected) {
                    let new_queries: Vec<String> = data.saved_queries.iter().map(|q| q.name.clone()).collect();
                    if current_queries != new_queries {
                        self.console_state.saved_queries = data.saved_queries;
                    }
                } else {
                    if !self.console_state.saved_queries.is_empty() {
                        self.console_state.saved_queries.clear();
                    }
                }
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
                ui.checkbox(&mut self.new_cluster.ssh_tunnel, "SSH Tunnel");
                if self.new_cluster.ssh_tunnel {
                    ui.horizontal(|ui| {
                        ui.label("SSH Host:");
                        ui.text_edit_singleline(&mut self.new_cluster.ssh_host);
                    });
                    ui.horizontal(|ui| {
                        ui.label("SSH User:");
                        ui.text_edit_singleline(&mut self.new_cluster.ssh_user);
                    });
                    ui.horizontal(|ui| {
                        ui.label("SSH Port:");
                        ui.add(
                            egui::DragValue::new(&mut self.new_cluster.ssh_port)
                                .speed(1)
                                .range(1..=65535),
                        );
                    });
                }

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
                                        Ok(h) => format!(
                                            "Connected! Cluster: {}, Status: {}",
                                            h.cluster_name, h.status
                                        ),
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
                                let _ = self
                                    .cluster_manager
                                    .update_cluster(old_name, self.new_cluster.clone());
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
                    ui.label(format!(
                        "Are you sure you want to delete cluster '{}'?",
                        name
                    ));
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

        // Handle clusters import
        if let Some(imported) = self.clusters_import.take() {
            for cluster in imported.clusters {
                let _ = self.cluster_manager.add_cluster(cluster);
            }
            for (name, data) in imported.cluster_data {
                self.cluster_manager.set_cluster_data(&name, data);
            }
            let _ = self.cluster_manager.save();
        }

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
                    let result = client
                        .execute(method, &path, body)
                        .await
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
