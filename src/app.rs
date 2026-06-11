use std::collections::HashMap;
use std::sync::mpsc::{Receiver, Sender, channel};
use std::sync::{Arc, RwLock};
use std::time::Instant;

use eframe::egui;

use crate::core::cluster_manager::ClusterManager;
use crate::core::config::ClusterConfig;
use crate::core::es_client::{ClusterHealth, EsClient};
use crate::modules::appearance::{AppearanceState, render_appearance_module};
use crate::modules::clusters::{ClustersState, render_clusters_module};
use crate::modules::console::{ConsoleState, render_console_module};
use crate::modules::dashboard::{DashboardState, render_dashboard_module};
use crate::modules::discover::{DiscoverState, render_discover_module};
use crate::modules::indices::{IndicesState, render_indices_module};
use crate::modules::observability::{ObservabilityState, render_observability_module};
use crate::modules::pipeline::{PipelineState, render_pipeline_module};
use crate::modules::snapshot::{
    ClusterSnapshotStatus, SnapshotHistory, fetch_cluster_snapshot, render_snapshot_module,
};
use crate::modules::status::{StatusState, render_status_module};
use crate::modules::tasks::{TasksState, render_tasks_module};
use crate::ui::theme::Theme;
use crate::ui::toasts::Toasts;
use crate::ui::vfx;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Tab {
    Clusters,
    Snapshot,
    Status,
    Dashboard,
    Tasks,
    Console,
    Discover,
    Indices,
    Observability,
    PipelineSimulator,
    Settings,
}

pub enum RefreshMsg {
    SnapshotResult(String, Box<ClusterSnapshotStatus>),
    HealthResult(String, Option<ClusterHealth>),
    HealthError(String, String),
    StatsResult(String, Option<crate::core::es_client::ClusterStats>),
    StatsError(String, String),
    TasksResult(String, Vec<crate::core::es_client::TaskInfo>),
    TasksError(String, String),
    ConsoleResult(Result<String, String>),
    DiscoverResult(Result<String, String>),
    TestResult(String),
    FetchedRepos(String, Vec<String>),
    FetchedSlmPolicies(String, Vec<String>),
    IndicesResult(
        String,
        Vec<crate::core::es_client::CatIndex>,
        Vec<crate::core::es_client::DataStream>,
    ),
    IndicesError(String, String),
    ObservabilityResult(String, Vec<crate::modules::observability::SyntheticMonitor>),
    ObservabilityError(String, String),
    ExplainResult(String, Option<crate::core::es_client::AllocationExplain>),
    ExplainError(String, String),
    HistoryResult(String, Vec<crate::core::es_client::SnapshotInfo>),
    HistoryError(String, String),
    EsVersionResult(String, String),
    KibanaVersionResult(String, String),
    AllocationResult(String, Vec<crate::core::es_client::CatAllocation>),
    HotThreadsResult(String, String, Result<String, String>),
    PendingTasksResult(String, Vec<serde_json::Value>),
    IndexDetailResult(String, Result<crate::modules::indices::IndexDetail, String>),
    NodesResult(String, Vec<crate::core::es_client::CatNode>),
}

pub struct DrasticSmurfApp {
    pub cluster_manager: ClusterManager,
    pub current_tab: Tab,
    pub snapshot_statuses: Vec<ClusterSnapshotStatus>,
    pub snapshot_histories: HashMap<String, SnapshotHistory>,
    pub status_state: StatusState,
    pub dashboard_state: DashboardState,
    pub tasks_state: TasksState,
    pub console_state: ConsoleState,
    pub discover_state: DiscoverState,
    pub indices_state: IndicesState,
    pub observability_state: ObservabilityState,
    pub clusters_state: ClustersState,
    pub appearance_state: AppearanceState,
    pub pipeline_state: PipelineState,
    pub auto_refresh: bool,
    pub refresh_interval_secs: u64,
    pub last_refresh: Option<Instant>,
    pub snapshot_manual_refresh: bool,
    pub show_add_cluster: bool,
    pub editing_cluster: Option<String>,
    pub new_cluster: ClusterConfig,
    pub new_password: String,
    pub test_result: Option<String>,
    pub refresh_tx: Sender<RefreshMsg>,
    pub refresh_rx: Receiver<RefreshMsg>,
    pub pending_delete: Option<String>,
    pub show_history_cluster: Option<String>,
    pub history_loading: bool,
    pub cluster_histories_fetched: HashMap<String, Vec<crate::core::es_client::SnapshotInfo>>,
    pub console_send: Option<(String, String, String, Option<String>, bool)>,
    pub discover_send: Option<(String, String, String)>,
    pub indices_refresh: Option<(String, bool)>,
    pub observability_refresh: Option<(String, String)>,
    pub clusters_import: Option<crate::core::config::AppConfig>,
    pub theme: crate::ui::theme::AppTheme,
    pub vfx: crate::core::config::VfxSettings,
    pub timezone_clocks: Vec<crate::core::config::TimezoneClockConfig>,
    pub window_size: [f32; 2],
    pub window_pos: Option<[f32; 2]>,
    pub toasts: Toasts,
    pub cluster_filter: String,
    pub log_entries: Arc<RwLock<Vec<crate::ui::log_buffer::LogEntry>>>,
    pub show_log_window: bool,
    pub konami_six_count: u32,
    pub title_hovered: bool,
    pub wizard_state: Option<crate::ui::wizard::WizardState>,
    pub hot_threads_node: Option<(String, String)>,
    pub hot_threads_text: Option<String>,
    pub hot_threads_loading: bool,
    pub show_pending_cluster: Option<String>,
}

impl Default for DrasticSmurfApp {
    fn default() -> Self {
        Self::with_log_entries(Arc::new(RwLock::new(Vec::new())))
    }
}

impl DrasticSmurfApp {
    fn with_log_entries(log_entries: Arc<RwLock<Vec<crate::ui::log_buffer::LogEntry>>>) -> Self {
        let (tx, rx) = channel();
        let manager = ClusterManager::new();
        if let Err(e) = manager.load() {
            tracing::warn!("Failed to load config: {}", e);
        }

        let clusters = manager.clusters();
        let cluster_names: Vec<String> = clusters.iter().map(|c| c.name.clone()).collect();

        let mut console_state = ConsoleState::new();
        if let Some(first) = cluster_names.first() {
            console_state.selected_cluster = first.clone();
        }

        let mut config = crate::core::config::AppConfig::load().unwrap_or_default();
        crate::ui::theme::Theme::set(config.theme.clone());

        let show_wizard = !config.wizard_completed;
        if show_wizard {
            config.wizard_completed = true;
            let _ = config.save();
        }

        let mut observability_state = ObservabilityState::new();
        observability_state.pinned_monitor_ids = config.pinned_monitor_ids.clone();
        observability_state.pinned_monitor_layouts = config.pinned_monitor_layouts.clone();

        let mut app = Self {
            cluster_manager: manager.clone(),
            current_tab: if clusters.is_empty() {
                Tab::Clusters
            } else {
                Tab::Status
            },
            snapshot_statuses: Vec::new(),
            snapshot_histories: HashMap::new(),
            status_state: StatusState::default(),
            dashboard_state: DashboardState::default(),
            tasks_state: TasksState::default(),
            console_state,
            discover_state: DiscoverState::default(),
            indices_state: IndicesState::new(),
            observability_state,
            clusters_state: ClustersState::default(),
            appearance_state: AppearanceState {
                selected_preset: config.theme.name.clone(),
                ..Default::default()
            },
            pipeline_state: PipelineState::with_defaults(),
            auto_refresh: manager.auto_refresh(),
            refresh_interval_secs: manager.refresh_interval_secs(),
            last_refresh: None,
            snapshot_manual_refresh: false,
            show_add_cluster: false,
            editing_cluster: None,
            new_cluster: ClusterConfig::default(),
            new_password: String::new(),
            test_result: None,
            refresh_tx: tx,
            refresh_rx: rx,
            pending_delete: None,
            show_history_cluster: None,
            history_loading: false,
            cluster_histories_fetched: HashMap::new(),
            console_send: None,
            discover_send: None,
            indices_refresh: None,
            observability_refresh: None,
            clusters_import: None,
            theme: config.theme,
            vfx: config.vfx,
            timezone_clocks: config.timezone_clocks,
            window_size: [
                config.window_width.unwrap_or(1280.0),
                config.window_height.unwrap_or(800.0),
            ],
            window_pos: match (config.window_pos_x, config.window_pos_y) {
                (Some(x), Some(y)) => Some([x, y]),
                _ => None,
            },
            toasts: Toasts::default(),
            cluster_filter: manager.cluster_filter(),
            log_entries,
            show_log_window: false,
            konami_six_count: 0,
            title_hovered: false,
            wizard_state: if show_wizard {
                Some(crate::ui::wizard::WizardState::default())
            } else {
                None
            },
            hot_threads_node: None,
            hot_threads_text: None,
            hot_threads_loading: false,
            show_pending_cluster: None,
        };

        for cluster in &clusters {
            if let Some(data) = manager.get_cluster_data(&cluster.name) {
                if let Some(latest) = data.status_history.last() {
                    app.status_state
                        .health_data
                        .push((cluster.name.clone(), latest.health.clone()));
                    app.status_state
                        .stats_data
                        .push((cluster.name.clone(), latest.stats.clone()));
                }
                if let Some(latest) = data.tasks_cache.last() {
                    for task in &latest.tasks {
                        app.tasks_state
                            .tasks
                            .push((cluster.name.clone(), task.clone()));
                    }
                }
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
                        slm_policies: latest.slm_policies.clone(),
                        has_repositories: latest.has_repositories,
                        resolved_repo: latest.resolved_repo.clone(),
                        backups: latest.backups.clone(),
                    };
                    app.snapshot_statuses.push(status);
                }
            }
        }

        app
    }

    pub fn new(
        _cc: &eframe::CreationContext<'_>,
        log_entries: Arc<RwLock<Vec<crate::ui::log_buffer::LogEntry>>>,
    ) -> Self {
        Self::with_log_entries(log_entries)
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

                // Snapshot refresh
                let tx2 = tx.clone();
                let name2 = name.clone();
                let ctx2 = ctx.clone();
                let manager2 = manager.clone();
                tokio::spawn(async move {
                    if let Some(client) = manager2.get_client(&name2) {
                        let config = manager2.clusters().into_iter().find(|c| c.name == name2);
                        if let Some(config) = config {
                            let status = fetch_cluster_snapshot(&client, &config).await;
                            let _ = tx2.send(RefreshMsg::SnapshotResult(name2, Box::new(status)));
                        }
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
                        match client.cluster_health().await {
                            Ok(health) => {
                                let _ = tx3.send(RefreshMsg::HealthResult(name3, Some(health)));
                            }
                            Err(e) => {
                                let _ = tx3.send(RefreshMsg::HealthError(name3, e.to_string()));
                            }
                        }
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
                        match client.cluster_stats().await {
                            Ok(stats) => {
                                let _ = tx4.send(RefreshMsg::StatsResult(name4, Some(stats)));
                            }
                            Err(e) => {
                                let _ = tx4.send(RefreshMsg::StatsError(name4, e.to_string()));
                            }
                        }
                    }
                    ctx4.request_repaint();
                });

                // ES version refresh
                let tx_es = tx.clone();
                let name_es = name.clone();
                let ctx_es = ctx.clone();
                let manager_es = manager.clone();
                tokio::spawn(async move {
                    if let Some(client) = manager_es.get_client(&name_es) {
                        if let Ok(ver) = client.get_es_version().await {
                            let _ = tx_es.send(RefreshMsg::EsVersionResult(name_es, ver));
                        }
                    }
                    ctx_es.request_repaint();
                });

                // Kibana version refresh
                let tx_kb = tx.clone();
                let name_kb = name.clone();
                let ctx_kb = ctx.clone();
                let manager_kb = manager.clone();
                tokio::spawn(async move {
                    if let Some(client) = manager_kb.get_client(&name_kb) {
                        let config = manager_kb
                            .clusters()
                            .into_iter()
                            .find(|c| c.name == name_kb);
                        if let Some(config) = config {
                            let kibana_host = if config.kibana_host.is_empty() {
                                if config.host.contains("elastic") {
                                    config.host.replace("elastic", "kibana")
                                } else {
                                    config.host.clone()
                                }
                            } else {
                                let h = config.kibana_host.trim();
                                if h.starts_with("http://") || h.starts_with("https://") {
                                    h.to_string()
                                } else {
                                    format!("http://{}", h)
                                }
                            };
                            if let Ok(ver) = client.get_kibana_version(&kibana_host).await {
                                let _ = tx_kb.send(RefreshMsg::KibanaVersionResult(name_kb, ver));
                            }
                        }
                    }
                    ctx_kb.request_repaint();
                });

                // Allocation refresh
                let tx_alloc = tx.clone();
                let name_alloc = name.clone();
                let ctx_alloc = ctx.clone();
                let manager_alloc = manager.clone();
                tokio::spawn(async move {
                    if let Some(client) = manager_alloc.get_client(&name_alloc) {
                        if let Ok(allocs) = client.cat_allocation().await {
                            let _ = tx_alloc.send(RefreshMsg::AllocationResult(name_alloc, allocs));
                        }
                    }
                    ctx_alloc.request_repaint();
                });

                // Nodes list refresh
                let tx_nodes = tx.clone();
                let name_nodes = name.clone();
                let ctx_nodes = ctx.clone();
                let manager_nodes = manager.clone();
                tokio::spawn(async move {
                    if let Some(client) = manager_nodes.get_client(&name_nodes) {
                        if let Ok(nodes) = client.cat_nodes().await {
                            let _ = tx_nodes.send(RefreshMsg::NodesResult(name_nodes, nodes));
                        }
                    }
                    ctx_nodes.request_repaint();
                });

                // Pending tasks refresh
                let tx_tasks = tx.clone();
                let name_tasks = name.clone();
                let ctx_tasks = ctx.clone();
                let manager_tasks = manager.clone();
                tokio::spawn(async move {
                    if let Some(client) = manager_tasks.get_client(&name_tasks) {
                        if let Ok(tasks_val) = client.get_pending_tasks().await {
                            if let Some(arr) = tasks_val.get("tasks").and_then(|a| a.as_array()) {
                                let _ = tx_tasks
                                    .send(RefreshMsg::PendingTasksResult(name_tasks, arr.clone()));
                            }
                        }
                    }
                    ctx_tasks.request_repaint();
                });

                // Tasks refresh
                let tx5 = tx.clone();
                let name5 = name.clone();
                let ctx5 = ctx.clone();
                let manager5 = manager.clone();
                tokio::spawn(async move {
                    if let Some(client) = manager5.get_client(&name5) {
                        match client.tasks(Some("*reindex*,*snapshot*")).await {
                            Ok(t) => {
                                let items: Vec<_> = t
                                    .nodes
                                    .into_values()
                                    .flat_map(|n| n.tasks.into_values())
                                    .collect();
                                let _ = tx5.send(RefreshMsg::TasksResult(name5, items));
                            }
                            Err(e) => {
                                let _ = tx5.send(RefreshMsg::TasksError(name5, e.to_string()));
                            }
                        }
                    }
                    ctx5.request_repaint();
                });
            });
        }

        self.last_refresh = Some(Instant::now());
    }

    fn process_refresh_results(&mut self, ctx: &egui::Context) {
        while let Ok(msg) = self.refresh_rx.try_recv() {
            match msg {
                RefreshMsg::SnapshotResult(name, status) => {
                    let status = *status;
                    let status_for_cache = status.clone();
                    // Rebuild client on auth failure
                    if status
                        .error_message
                        .as_ref()
                        .map(|e| e.contains("401") || e.to_lowercase().contains("unauthorized"))
                        .unwrap_or(false)
                    {
                        self.cluster_manager.rebuild_client(&name);
                    }
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
                        self.status_state
                            .health_data
                            .push((name.clone(), health.clone()));
                    }
                    self.status_state.errors.remove(&name);
                    // Try to save status snapshot when both health and stats are available
                    let stats = self
                        .status_state
                        .stats_data
                        .iter()
                        .find(|(n, _)| n == &name)
                        .and_then(|(_, s)| s.clone());
                    self.save_status_snapshot(&name, health.clone(), stats);

                    // Diagnose yellow/red unassigned shards in the background
                    if let Some(ref h) = health {
                        if h.status != "green" {
                            let manager = self.cluster_manager.clone();
                            let tx = self.refresh_tx.clone();
                            let name_clone = name.clone();
                            let ctx = ctx.clone();
                            tokio::spawn(async move {
                                if let Some(client) = manager.get_client(&name_clone) {
                                    match client.allocation_explain().await {
                                        Ok(exp) => {
                                            let _ = tx.send(RefreshMsg::ExplainResult(
                                                name_clone,
                                                Some(exp),
                                            ));
                                        }
                                        Err(e) => {
                                            let _ = tx.send(RefreshMsg::ExplainError(
                                                name_clone,
                                                e.to_string(),
                                            ));
                                        }
                                    }
                                }
                                ctx.request_repaint();
                            });
                        } else {
                            self.status_state.explains.insert(name.clone(), None);
                        }
                    } else {
                        self.status_state.explains.insert(name.clone(), None);
                    }
                }
                RefreshMsg::HealthError(name, err) => {
                    if err.contains("401") || err.to_lowercase().contains("unauthorized") {
                        self.cluster_manager.rebuild_client(&name);
                    }
                    if let Some(existing) = self
                        .status_state
                        .health_data
                        .iter_mut()
                        .find(|(n, _)| n == &name)
                    {
                        existing.1 = None;
                    } else {
                        self.status_state.health_data.push((name.clone(), None));
                    }
                    self.status_state.errors.insert(name, err);
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
                        self.status_state
                            .stats_data
                            .push((name.clone(), stats.clone()));
                    }
                    self.status_state.errors.remove(&name);
                    // Try to save status snapshot when both health and stats are available
                    let health = self
                        .status_state
                        .health_data
                        .iter()
                        .find(|(n, _)| n == &name)
                        .and_then(|(_, h)| h.clone());
                    self.save_status_snapshot(&name, health, stats);
                }
                RefreshMsg::StatsError(name, err) => {
                    if err.contains("401") || err.to_lowercase().contains("unauthorized") {
                        self.cluster_manager.rebuild_client(&name);
                    }
                    if let Some(existing) = self
                        .status_state
                        .stats_data
                        .iter_mut()
                        .find(|(n, _)| n == &name)
                    {
                        existing.1 = None;
                    } else {
                        self.status_state.stats_data.push((name.clone(), None));
                    }
                    self.status_state.errors.insert(name, err);
                }
                RefreshMsg::ExplainResult(name, explain) => {
                    self.status_state.explains.insert(name, explain);
                }
                RefreshMsg::ExplainError(name, _err) => {
                    self.status_state.explains.insert(name, None);
                }
                RefreshMsg::TasksResult(name, tasks) => {
                    self.tasks_state.tasks.retain(|(n, _)| n != &name);
                    for task in tasks.iter().cloned() {
                        self.tasks_state.tasks.push((name.clone(), task));
                    }
                    self.tasks_state.errors.remove(&name);
                    self.save_tasks_cache(&name, tasks);
                }
                RefreshMsg::TasksError(name, err) => {
                    if err.contains("401") || err.to_lowercase().contains("unauthorized") {
                        self.cluster_manager.rebuild_client(&name);
                    }
                    self.tasks_state.tasks.retain(|(n, _)| n != &name);
                    self.tasks_state.errors.insert(name, err);
                }
                RefreshMsg::ConsoleResult(result) => {
                    let mut text = match result {
                        Ok(val) => {
                            if let Ok(parsed) = serde_json::from_str::<serde_json::Value>(&val) {
                                serde_json::to_string_pretty(&parsed).unwrap_or(val)
                            } else {
                                val
                            }
                        }
                        Err(e) => format!("Error: {}", e),
                    };
                    self.console_state.full_response = Some(text.clone());
                    
                    if text.len() > 100_000 {
                        text.truncate(100_000);
                        text.push_str("\n\n... [Response truncated for performance. Use 'Copy JSON' to get the full payload] ...");
                    }
                    
                    self.console_state.response = text;
                    self.console_state.is_loading = false;
                }
                RefreshMsg::DiscoverResult(result) => {
                    self.discover_state.is_loading = false;
                    match result {
                        Ok(val) => {
                            if let Ok(parsed) = serde_json::from_str::<serde_json::Value>(&val) {
                                if let Some(hits) = parsed
                                    .get("hits")
                                    .and_then(|h| h.get("hits"))
                                    .and_then(|h| h.as_array())
                                {
                                    self.discover_state.results = hits.clone();
                                    self.discover_state.refresh_fields();
                                } else {
                                    self.discover_state.error = Some(
                                        "Invalid response structure from search API".to_string(),
                                    );
                                }
                            } else {
                                self.discover_state.error =
                                    Some("Failed to parse search response as JSON".to_string());
                            }
                        }
                        Err(e) => {
                            self.discover_state.error = Some(e);
                        }
                    }
                }
                RefreshMsg::IndicesResult(_name, indices, datastreams) => {
                    self.indices_state.update_data(indices, datastreams);
                    self.indices_state.error = None;
                    self.indices_state.is_loading = false;
                }
                RefreshMsg::IndicesError(_name, err) => {
                    self.indices_state.error = Some(err);
                    self.indices_state.is_loading = false;
                }
                RefreshMsg::ObservabilityResult(_name, monitors) => {
                    self.observability_state.monitors = monitors;
                    self.observability_state.error = None;
                    self.observability_state.is_loading = false;
                }
                RefreshMsg::ObservabilityError(_name, err) => {
                    self.observability_state.error = Some(err);
                    self.observability_state.is_loading = false;
                }
                RefreshMsg::TestResult(msg) => {
                    self.test_result = Some(msg.clone());
                    self.clusters_state.test_result = Some(msg);
                }
                RefreshMsg::FetchedRepos(name, repos) => {
                    self.clusters_state.fetched_repos = repos;
                    self.clusters_state.test_result =
                        Some(format!("Fetched repositories for '{}'", name));
                }
                RefreshMsg::FetchedSlmPolicies(name, policies) => {
                    self.clusters_state.fetched_slm_policies = policies;
                    self.clusters_state.test_result =
                        Some(format!("Fetched SLM policies for '{}'", name));
                }
                RefreshMsg::HistoryResult(name, history) => {
                    self.cluster_histories_fetched.insert(name, history);
                    self.history_loading = false;
                }
                RefreshMsg::HistoryError(name, err) => {
                    self.toasts
                        .error(format!("Failed to fetch history for '{}': {}", name, err));
                    self.history_loading = false;
                }
                RefreshMsg::EsVersionResult(name, version) => {
                    self.status_state.es_versions.insert(name, version);
                }
                RefreshMsg::KibanaVersionResult(name, version) => {
                    self.status_state.kibana_versions.insert(name, version);
                }
                RefreshMsg::AllocationResult(name, allocations) => {
                    self.status_state.allocations.insert(name, allocations);
                }
                RefreshMsg::NodesResult(name, nodes) => {
                    self.status_state.nodes.insert(name, nodes);
                }
                RefreshMsg::PendingTasksResult(name, tasks) => {
                    self.status_state.pending_tasks.insert(name, tasks);
                }
                RefreshMsg::HotThreadsResult(cluster_name, node_name, res) => {
                    if let Some((ref active_cluster, ref active_node)) = self.hot_threads_node {
                        if active_cluster == &cluster_name && active_node == &node_name {
                            self.hot_threads_loading = false;
                            match res {
                                Ok(text) => {
                                    self.hot_threads_text = Some(text);
                                }
                                Err(err) => {
                                    self.hot_threads_text =
                                        Some(format!("Error loading Hot Threads: {}", err));
                                }
                            }
                        }
                    }
                }
                RefreshMsg::IndexDetailResult(_name, res) => {
                    self.indices_state.detail_loading = false;
                    match res {
                        Ok(detail) => {
                            self.indices_state.selected_detail = Some(detail);
                        }
                        Err(e) => {
                            self.toasts
                                .error(format!("Failed to fetch index details: {}", e));
                        }
                    }
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
            slm_policies: status.slm_policies.clone(),
            has_repositories: status.has_repositories,
            resolved_repo: status.resolved_repo.clone(),
            backups: status.backups.clone(),
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
    }

    fn cluster_matches_filter(&self, name: &str) -> bool {
        if self.cluster_filter.is_empty() {
            return true;
        }
        name.to_lowercase()
            .contains(&self.cluster_filter.to_lowercase())
    }

    fn get_unfiltered_cluster_names(&self) -> Vec<String> {
        self.cluster_manager
            .clusters()
            .iter()
            .filter(|c| self.cluster_matches_filter(&c.name))
            .map(|c| c.name.clone())
            .collect()
    }

    fn ensure_selected_clusters_match_filter(&mut self) {
        let allowed = self.get_unfiltered_cluster_names();

        // 1. Console
        if !allowed.contains(&self.console_state.selected_cluster) {
            self.console_state.selected_cluster = allowed.first().cloned().unwrap_or_default();
        }

        // 2. Discover
        if !allowed.contains(&self.discover_state.selected_cluster) {
            self.discover_state.selected_cluster = allowed.first().cloned().unwrap_or_default();
        }

        // 3. Indices
        if !allowed.contains(&self.indices_state.selected_cluster) {
            self.indices_state.selected_cluster = allowed.first().cloned().unwrap_or_default();
        }

        // 4. Observability
        if !allowed.contains(&self.observability_state.selected_cluster) {
            self.observability_state.selected_cluster =
                allowed.first().cloned().unwrap_or_default();
        }

        // 4b. Dashboard
        if !allowed.contains(&self.dashboard_state.selected_cluster) {
            self.dashboard_state.selected_cluster = allowed.first().cloned().unwrap_or_default();
        }

        // 5. Clusters selection & edit forms
        if let Some(ref selected) = self.clusters_state.selected_cluster {
            if !allowed.contains(selected) {
                self.clusters_state.selected_cluster = None;
                self.clusters_state.editing_cluster = None;
            }
        }
    }

    fn render_sidebar(&mut self, ui: &mut egui::Ui) {
        let clusters = self.cluster_manager.clusters();

        // 1. Calculate overall status first so we can draw it at the bottom.
        let overall_status = if clusters.is_empty() {
            "offline"
        } else {
            let mut has_contact = false;
            let mut any_yellow = false;
            let mut any_red = false;
            for cluster in &clusters {
                let health_opt = self
                    .status_state
                    .health_data
                    .iter()
                    .find(|(n, _)| n == &cluster.name)
                    .and_then(|(_, h)| h.as_ref());
                if let Some(health) = health_opt {
                    has_contact = true;
                    if health.status == "red" {
                        any_red = true;
                    } else if health.status == "yellow" {
                        any_yellow = true;
                    }
                }
            }
            if !has_contact {
                "offline"
            } else if any_red {
                "red"
            } else if any_yellow {
                "yellow"
            } else {
                "green"
            }
        };

        ui.add_space(16.0);

        // Render logo image
        ui.add(egui::Image::new(egui::include_image!("../drastic.png")).max_width(120.0));

        ui.add_space(8.0);
        let title_response = ui.heading(
            egui::RichText::new("DRASTIC SMURF")
                .color(Theme::accent())
                .size(18.0),
        );
        ui.label(
            egui::RichText::new("ES Multi-Tool")
                .color(Theme::text_muted())
                .size(11.0),
        );

        let currently_hovered = title_response.hovered();
        if currently_hovered && ui.input(|i| i.key_pressed(egui::Key::Num6)) {
            self.konami_six_count += 1;
            if self.konami_six_count >= 3 {
                self.show_log_window = !self.show_log_window;
                self.konami_six_count = 0;
                tracing::info!("Log window toggled via konami code");
            }
        }
        if !currently_hovered {
            self.konami_six_count = 0;
        }
        self.title_hovered = currently_hovered;

        ui.add_space(20.0);

        let screen_height = ui.ctx().screen_rect().height();
        let default_open = screen_height >= 750.0;

        // Calculate the maximum height we want to allocate for the scrollable middle section.
        // We leave exactly 75 pixels for the bottom status footer.
        let scroll_max_height = (ui.available_height() - 75.0).max(100.0);

        egui::ScrollArea::vertical()
            .max_height(scroll_max_height)
            .id_salt("sidebar_scroll")
            .show(ui, |ui| {
                // 🔌 Clusters
                egui::CollapsingHeader::new(
                    egui::RichText::new("🔌 Clusters")
                        .strong()
                        .color(Theme::text_secondary())
                        .size(12.0),
                )
                .default_open(default_open)
                .show(ui, |ui| {
                    ui.add_space(4.0);

                    let filter_res = ui.add(
                        egui::TextEdit::singleline(&mut self.cluster_filter)
                            .hint_text("🔍 Filter clusters...")
                            .desired_width(f32::INFINITY),
                    );
                    if filter_res.changed() {
                        self.cluster_manager
                            .set_cluster_filter(self.cluster_filter.clone());
                        self.ensure_selected_clusters_match_filter();
                    }
                    ui.add_space(4.0);

                    for cluster in &clusters {
                        if !self.cluster_matches_filter(&cluster.name) {
                            continue;
                        }
                        ui.horizontal(|ui| {
                            let health_opt = self
                                .status_state
                                .health_data
                                .iter()
                                .find(|(n, _)| n == &cluster.name)
                                .and_then(|(_, h)| h.as_ref());

                            let dot_color = match health_opt {
                                Some(health) => match health.status.as_str() {
                                    "green" => Theme::success(),
                                    "yellow" => egui::Color32::from_rgb(235, 179, 41), // Vibrant Yellow
                                    "red" => Theme::danger(),
                                    _ => egui::Color32::from_rgb(60, 60, 60), // Dark Grey
                                },
                                None => egui::Color32::from_rgb(60, 60, 60), // Offline
                            };

                            ui.add(
                                crate::ui::widgets::ConnectionDot::new(health_opt.is_some())
                                    .color(dot_color)
                                    .size(8.0),
                            );
                            ui.label(
                                egui::RichText::new(&cluster.name)
                                    .color(Theme::text_primary())
                                    .size(13.0),
                            );
                        });
                    }

                    if clusters.is_empty() {
                        ui.label(
                            egui::RichText::new("No clusters configured")
                                .color(Theme::text_muted())
                                .size(11.0),
                        );
                    }

                    ui.add_space(12.0);
                    if ui.button("+ Add Cluster").clicked() {
                        self.new_cluster = ClusterConfig::default();
                        self.new_password.clear();
                        self.editing_cluster = None;
                        self.test_result = None;
                        self.show_add_cluster = true;
                    }
                });

                self.render_timezone_clocks(ui);

                ui.add_space(8.0);

                let mut auto_refresh_changed = false;
                let mut interval_changed = false;

                egui::CollapsingHeader::new(
                    egui::RichText::new("⚙️ Refresh Settings")
                        .strong()
                        .color(Theme::text_secondary())
                        .size(12.0),
                )
                .default_open(default_open)
                .show(ui, |ui| {
                    ui.add_space(4.0);

                    if let Some(last) = self.last_refresh {
                        let ago = last.elapsed().as_secs();
                        ui.label(
                            egui::RichText::new(format!("Last refresh: {}s ago", ago))
                                .size(10.0)
                                .color(Theme::text_muted()),
                        );
                    }

                    if ui.button("🔄 Refresh Now").clicked() {
                        self.trigger_refresh(ui.ctx());
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

                    if ui
                        .checkbox(&mut self.auto_refresh, "Auto Refresh")
                        .changed()
                    {
                        auto_refresh_changed = true;
                    }
                });

                if auto_refresh_changed || interval_changed {
                    self.cluster_manager.set_auto_refresh(self.auto_refresh);
                    self.cluster_manager
                        .set_refresh_interval_secs(self.refresh_interval_secs);
                }
            });

        // 4. Spacer to push the footer to the bottom
        let remaining_space = ui.available_height() - 75.0;
        if remaining_space > 0.0 {
            ui.add_space(remaining_space);
        }

        // 5. Render footer warning light top-down
        ui.separator();
        ui.add_space(8.0);

        ui.horizontal(|ui| {
            // Add the warning light widget
            ui.add(crate::ui::widgets::WarningLight::new(overall_status));

            // Status panel description next to the light
            ui.vertical(|ui| {
                ui.add_space(2.0);
                let (title, color, desc) = match overall_status {
                    "green" => (
                        "SYSTEM ONLINE",
                        Theme::success(),
                        "All connected clusters healthy.\nStandby mode active.",
                    ),
                    "yellow" => (
                        "MINOR WARNING",
                        egui::Color32::from_rgb(235, 179, 41),
                        "One or more clusters yellow.\nCheck shard allocation.",
                    ),
                    "red" => (
                        "CRITICAL FAILURE",
                        Theme::danger(),
                        "Immediate attention required!\nRed health detected.",
                    ),
                    _ => (
                        "SYSTEM OFFLINE",
                        egui::Color32::from_rgb(120, 120, 125),
                        "No active cluster contact.\nFilament disconnected.",
                    ),
                };

                ui.colored_label(color, egui::RichText::new(title).strong().size(11.0));
                ui.label(
                    egui::RichText::new(desc)
                        .size(8.5)
                        .color(Theme::text_muted()),
                );
            });
        });
        ui.add_space(12.0); // bottom padding
    }

    fn render_timezone_clocks(&self, ui: &mut egui::Ui) {
        use chrono::{Datelike, FixedOffset, NaiveDate, TimeZone, Timelike, Utc, Weekday};

        let now_utc = Utc::now();
        let local_now = chrono::Local::now();

        let get_timezone_time = |utc_now: &chrono::DateTime<Utc>,
                                 zone: &str|
         -> Option<chrono::DateTime<FixedOffset>> {
            let find_nth_sunday = |year: i32, month: u32, n: u32| -> u32 {
                let mut count = 0;
                for day in 1..=31 {
                    if let Some(date) = NaiveDate::from_ymd_opt(year, month, day) {
                        if date.weekday() == Weekday::Sun {
                            count += 1;
                            if count == n {
                                return day;
                            }
                        }
                    }
                }
                let mut last_sun = 1;
                for day in 1..=31 {
                    if let Some(date) = NaiveDate::from_ymd_opt(year, month, day) {
                        if date.weekday() == Weekday::Sun {
                            last_sun = day;
                        }
                    }
                }
                last_sun
            };

            let year = utc_now.year();
            let month = utc_now.month();
            let day = utc_now.day();

            if zone == "Local" || zone == "UTC" {
                return None;
            }

            let offset_hours = match zone {
                "Sydney" => {
                    let end_aedt_day = find_nth_sunday(year, 4, 1);
                    let start_aedt_day = find_nth_sunday(year, 10, 1);

                    let is_aedt = if month > 10 || month < 4 {
                        true
                    } else if month == 4 {
                        day < end_aedt_day || (day == end_aedt_day && utc_now.hour() < 2)
                    } else if month == 10 {
                        day > start_aedt_day || (day == start_aedt_day && utc_now.hour() >= 2)
                    } else {
                        false
                    };

                    if is_aedt { 11 } else { 10 }
                }
                "Germany" => {
                    let start_cest_day = find_nth_sunday(year, 3, 5);
                    let end_cest_day = find_nth_sunday(year, 10, 5);

                    let is_cest = if month > 3 && month < 10 {
                        true
                    } else if month == 3 {
                        day > start_cest_day || (day == start_cest_day && utc_now.hour() >= 1)
                    } else if month == 10 {
                        day < end_cest_day || (day == end_cest_day && utc_now.hour() < 1)
                    } else {
                        false
                    };

                    if is_cest { 2 } else { 1 }
                }
                "Chicago" => {
                    let start_cdt_day = find_nth_sunday(year, 3, 2);
                    let end_cdt_day = find_nth_sunday(year, 11, 1);

                    let is_cdt = if month > 3 && month < 11 {
                        true
                    } else if month == 3 {
                        day > start_cdt_day || (day == start_cdt_day && utc_now.hour() >= 8)
                    } else if month == 11 {
                        day < end_cdt_day || (day == end_cdt_day && utc_now.hour() < 7)
                    } else {
                        false
                    };

                    if is_cdt { -5 } else { -6 }
                }
                custom_offset => {
                    let clean = custom_offset.trim();
                    let sign = if clean.starts_with('-') { -1 } else { 1 };
                    let number_str: String = clean.chars().filter(|c| c.is_ascii_digit()).collect();
                    if let Ok(hours) = number_str.parse::<i32>() {
                        sign * hours
                    } else {
                        0
                    }
                }
            };

            let offset = FixedOffset::east_opt(offset_hours * 3600).unwrap();
            Some(offset.from_utc_datetime(&utc_now.naive_utc()))
        };

        ui.add_space(8.0);

        egui::CollapsingHeader::new(
            egui::RichText::new("🕒 World Clocks")
                .strong()
                .size(12.0)
                .color(Theme::text_secondary()),
        )
        .default_open(true)
        .show(ui, |ui| {
            ui.add_space(4.0);

            let mut rendered_any = false;
            for clock in &self.timezone_clocks {
                if !clock.enabled {
                    continue;
                }
                rendered_any = true;

                let time_str = match clock.zone.as_str() {
                    "Local" => local_now.format("%Y-%m-%dT%H:%M:%S%:z").to_string(),
                    "UTC" => now_utc.format("%Y-%m-%dT%H:%M:%SZ").to_string(),
                    other_zone => {
                        if let Some(t) = get_timezone_time(&now_utc, other_zone) {
                            t.format("%Y-%m-%dT%H:%M:%S%:z").to_string()
                        } else {
                            now_utc.format("%Y-%m-%dT%H:%M:%SZ").to_string()
                        }
                    }
                };

                ui.vertical(|ui| {
                    ui.horizontal(|ui| {
                        ui.label(
                            egui::RichText::new(&clock.label)
                                .strong()
                                .size(10.5)
                                .color(Theme::text_primary()),
                        );
                        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                            if ui
                                .button("📋")
                                .on_hover_text(format!(
                                    "Copy {} ISO 8601 to clipboard",
                                    clock.label
                                ))
                                .clicked()
                            {
                                ui.ctx().copy_text(time_str.clone());
                            }
                        });
                    });

                    ui.add(egui::Label::new(
                        egui::RichText::new(&time_str)
                            .code()
                            .size(10.0)
                            .color(Theme::accent()),
                    ))
                    .on_hover_text(format!(
                        "Click copy button to copy this timezone's exact ISO 8601 string: {}",
                        time_str
                    ));

                    ui.add_space(4.0);
                });
            }

            if !rendered_any {
                ui.colored_label(Theme::text_muted(), "All clocks disabled in Settings.");
            }
        });
    }

    fn render_tabs(&mut self, ui: &mut egui::Ui) {
        egui::ScrollArea::horizontal()
            .id_salt("tabs")
            .show(ui, |ui| {
                ui.horizontal(|ui| {
                    for (label, tab) in [
                        ("Clusters", Tab::Clusters),
                        ("Snapshot", Tab::Snapshot),
                        ("Status", Tab::Status),
                        ("Dashboard", Tab::Dashboard),
                        ("Tasks", Tab::Tasks),
                        ("Console", Tab::Console),
                        ("Discover", Tab::Discover),
                        ("Indices", Tab::Indices),
                        ("Observability", Tab::Observability),
                        ("Pipeline Simulator", Tab::PipelineSimulator),
                        ("Settings", Tab::Settings),
                    ] {
                        let is_active = self.current_tab == tab;
                        let text = egui::RichText::new(label).size(14.0);
                        let text = if is_active {
                            let active_bg = Theme::accent().linear_multiply(0.25);
                            text.color(Theme::contrast_text_color(active_bg)).strong()
                        } else {
                            text.color(Theme::text_secondary())
                        };
                        if ui.selectable_label(is_active, text).clicked() {
                            self.current_tab = tab;
                        }
                    }
                });
            });
        ui.separator();
    }

    fn render_content(&mut self, ui: &mut egui::Ui) {
        let ctx = ui.ctx().clone();
        match self.current_tab {
            Tab::Clusters => {
                let clusters: Vec<_> = self
                    .cluster_manager
                    .clusters()
                    .into_iter()
                    .filter(|c| self.cluster_matches_filter(&c.name))
                    .collect();
                let data = self.cluster_manager.all_cluster_data();
                let mut on_save = None;
                let mut on_delete = None;
                let mut on_test = None;
                let mut on_import = None;
                let mut on_show_dialog = false;
                let mut on_fetch_repos: Option<String> = None;
                let mut on_fetch_slm: Option<String> = None;
                let health_status: std::collections::HashMap<String, Option<String>> = self
                    .status_state
                    .health_data
                    .iter()
                    .map(|(n, h)| (n.clone(), h.as_ref().map(|health| health.status.clone())))
                    .collect();
                render_clusters_module(
                    ui,
                    &mut self.clusters_state,
                    &clusters,
                    &data,
                    &health_status,
                    &mut on_save,
                    &mut on_delete,
                    &mut on_test,
                    &mut on_import,
                    &mut on_show_dialog,
                    &mut on_fetch_repos,
                    &mut on_fetch_slm,
                );
                if on_show_dialog {
                    self.new_cluster = ClusterConfig::default();
                    self.new_password.clear();
                    self.editing_cluster = None;
                    self.test_result = None;
                    self.show_add_cluster = true;
                }
                if let Some((old_name, config, password)) = on_save {
                    if let Err(e) = crate::core::auth::set_password(&config.name, &password) {
                        self.toasts.error(format!("Failed to save password: {}", e));
                    }
                    let mut success = false;
                    if let Some(old) = old_name {
                        if let Err(e) =
                            self.cluster_manager
                                .update_cluster(&old, config, Some(&password))
                        {
                            self.toasts
                                .error(format!("Failed to update cluster: {}", e));
                        } else {
                            success = true;
                        }
                    } else {
                        if let Err(e) = self.cluster_manager.add_cluster(config, Some(&password)) {
                            self.toasts.error(format!("Failed to add cluster: {}", e));
                        } else {
                            success = true;
                        }
                    }
                    if success {
                        self.trigger_refresh(&ctx);
                    }
                }
                if let Some(name) = on_delete {
                    self.pending_delete = Some(name);
                }
                if let Some((name, password)) = on_test {
                    let config = self
                        .cluster_manager
                        .clusters()
                        .into_iter()
                        .find(|c| c.name == name)
                        .unwrap_or_default();
                    if let Ok(client) = EsClient::with_password(&config, &password) {
                        let client_clone = client.clone();
                        let test_name = name.clone();
                        let ctx = ui.ctx().clone();
                        let tx = self.refresh_tx.clone();
                        tokio::spawn(async move {
                            let result = client.test_connection_detailed().await;
                            match &result {
                                Ok(msg) => {
                                    tracing::info!(
                                        "Test connection succeeded for '{}': {}",
                                        test_name,
                                        msg
                                    );
                                }
                                Err(msg) => {
                                    tracing::warn!(
                                        "Test connection failed for '{}': {}",
                                        test_name,
                                        msg
                                    );
                                }
                            }
                            let msg = match result {
                                Ok(msg) => msg,
                                Err(msg) => msg,
                            };
                            let _ = tx.send(RefreshMsg::TestResult(msg));
                            ctx.request_repaint();
                        });
                        self.clusters_state.test_result = Some("Testing...".to_string());

                        // Store the working client so auto-refresh uses it
                        if !password.is_empty() {
                            self.cluster_manager.set_client(&name, client_clone);
                            if let Err(e) = crate::core::auth::set_password(&name, &password) {
                                tracing::warn!(
                                    "Failed to save password after successful test: {}",
                                    e
                                );
                            }
                        }
                    } else {
                        self.clusters_state.test_result =
                            Some("Failed to create client".to_string());
                    }
                }
                if let Some(imported) = on_import {
                    self.clusters_import = Some(imported);
                }
                if let Some(cluster_name) = on_fetch_repos {
                    if let Some(client) = self.cluster_manager.get_client(&cluster_name) {
                        let ctx = ui.ctx().clone();
                        let tx = self.refresh_tx.clone();
                        let name = cluster_name.clone();
                        tokio::spawn(async move {
                            match client
                                .execute(reqwest::Method::GET, "/_snapshot", None)
                                .await
                            {
                                Ok(val) => {
                                    let repos: Vec<String> = match val {
                                        serde_json::Value::Object(map) => {
                                            map.keys().cloned().collect()
                                        }
                                        _ => vec![],
                                    };
                                    let _ = tx.send(RefreshMsg::FetchedRepos(name.clone(), repos));
                                }
                                Err(e) => {
                                    tracing::warn!("Failed to fetch repos for '{}': {}", name, e);
                                }
                            }
                            ctx.request_repaint();
                        });
                        self.clusters_state.test_result =
                            Some("Fetching snapshot repositories...".to_string());
                    } else {
                        self.clusters_state.test_result =
                            Some("No client for this cluster. Test connection first.".to_string());
                    }
                }
                if let Some(cluster_name) = on_fetch_slm {
                    if let Some(client) = self.cluster_manager.get_client(&cluster_name) {
                        let ctx = ui.ctx().clone();
                        let tx = self.refresh_tx.clone();
                        let name = cluster_name.clone();
                        tokio::spawn(async move {
                            match client.slm_policies_all().await {
                                Ok(resp) => {
                                    let policies: Vec<String> =
                                        resp.policies.keys().cloned().collect();
                                    let _ = tx.send(RefreshMsg::FetchedSlmPolicies(
                                        name.clone(),
                                        policies,
                                    ));
                                }
                                Err(e) => {
                                    tracing::warn!(
                                        "Failed to fetch SLM policies for '{}': {}",
                                        name,
                                        e
                                    );
                                }
                            }
                            ctx.request_repaint();
                        });
                        self.clusters_state.test_result =
                            Some("Fetching SLM policies...".to_string());
                    } else {
                        self.clusters_state.test_result =
                            Some("No client for this cluster. Test connection first.".to_string());
                    }
                }
            }
            Tab::Snapshot => {
                let mut on_edit = None;
                let mut on_delete = None;
                let mut on_refresh = false;
                let mut on_show_history = None;
                let filtered_statuses: Vec<_> = self
                    .snapshot_statuses
                    .iter()
                    .filter(|s| self.cluster_matches_filter(&s.config.name))
                    .cloned()
                    .collect();
                render_snapshot_module(
                    ui,
                    &filtered_statuses,
                    &self.snapshot_histories,
                    &mut on_edit,
                    &mut on_delete,
                    &mut on_show_history,
                    self.vfx.shimmer_effects && !self.vfx.reduce_motion,
                    &mut on_refresh,
                );
                if on_refresh {
                    self.snapshot_manual_refresh = true;
                }
                if let Some(name) = on_show_history {
                    self.show_history_cluster = Some(name.clone());
                    self.history_loading = true;
                    let manager = self.cluster_manager.clone();
                    let tx = self.refresh_tx.clone();
                    let name_clone = name.clone();
                    let ctx = ctx.clone();
                    tokio::spawn(async move {
                        if let Some(client) = manager.get_client(&name_clone) {
                            let repo = manager
                                .clusters()
                                .into_iter()
                                .find(|c| c.name == name_clone)
                                .map(|c| c.snapshot_repo.clone())
                                .unwrap_or_default();
                            if !repo.is_empty() {
                                match client.snapshot_all(&repo).await {
                                    Ok(resp) => {
                                        let _ = tx.send(RefreshMsg::HistoryResult(
                                            name_clone,
                                            resp.snapshots,
                                        ));
                                    }
                                    Err(e) => {
                                        let _ = tx.send(RefreshMsg::HistoryError(
                                            name_clone,
                                            e.to_string(),
                                        ));
                                    }
                                }
                            } else {
                                let _ = tx.send(RefreshMsg::HistoryError(
                                    name_clone,
                                    "No snapshot repository configured for this cluster."
                                        .to_string(),
                                ));
                            }
                        } else {
                            let _ = tx.send(RefreshMsg::HistoryError(
                                name_clone,
                                "Failed to get cluster client.".to_string(),
                            ));
                        }
                        ctx.request_repaint();
                    });
                }
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
                let clusters: Vec<_> = self
                    .cluster_manager
                    .clusters()
                    .into_iter()
                    .filter(|c| self.cluster_matches_filter(&c.name))
                    .collect();
                let filtered_state = StatusState {
                    health_data: self
                        .status_state
                        .health_data
                        .iter()
                        .filter(|(n, _)| self.cluster_matches_filter(n))
                        .cloned()
                        .collect(),
                    stats_data: self
                        .status_state
                        .stats_data
                        .iter()
                        .filter(|(n, _)| self.cluster_matches_filter(n))
                        .cloned()
                        .collect(),
                    errors: self
                        .status_state
                        .errors
                        .iter()
                        .filter(|(n, _)| self.cluster_matches_filter(n))
                        .map(|(n, e)| (n.clone(), e.clone()))
                        .collect(),
                    explains: self
                        .status_state
                        .explains
                        .iter()
                        .filter(|(n, _)| self.cluster_matches_filter(n))
                        .map(|(n, e)| (n.clone(), e.clone()))
                        .collect(),
                    es_versions: self
                        .status_state
                        .es_versions
                        .iter()
                        .filter(|(n, _)| self.cluster_matches_filter(n))
                        .map(|(n, v)| (n.clone(), v.clone()))
                        .collect(),
                    kibana_versions: self
                        .status_state
                        .kibana_versions
                        .iter()
                        .filter(|(n, _)| self.cluster_matches_filter(n))
                        .map(|(n, v)| (n.clone(), v.clone()))
                        .collect(),
                    allocations: self
                        .status_state
                        .allocations
                        .iter()
                        .filter(|(n, _)| self.cluster_matches_filter(n))
                        .map(|(n, a)| (n.clone(), a.clone()))
                        .collect(),
                    pending_tasks: self
                        .status_state
                        .pending_tasks
                        .iter()
                        .filter(|(n, _)| self.cluster_matches_filter(n))
                        .map(|(n, t)| (n.clone(), t.clone()))
                        .collect(),
                    nodes: self
                        .status_state
                        .nodes
                        .iter()
                        .filter(|(n, _)| self.cluster_matches_filter(n))
                        .map(|(n, nodes_vec)| (n.clone(), nodes_vec.clone()))
                        .collect(),
                };
                let mut on_hot_threads = None;
                let mut on_show_pending = None;
                render_status_module(
                    ui,
                    &clusters,
                    &filtered_state,
                    &mut on_hot_threads,
                    &mut on_show_pending,
                    self.vfx.hover_effects && !self.vfx.reduce_motion,
                );

                if let Some((cluster_name, node_name)) = on_hot_threads {
                    self.hot_threads_node = Some((cluster_name.clone(), node_name.clone()));
                    self.hot_threads_text = None;
                    self.hot_threads_loading = true;

                    let tx = self.refresh_tx.clone();
                    let ctx = ui.ctx().clone();
                    let manager = self.cluster_manager.clone();
                    tokio::spawn(async move {
                        if let Some(client) = manager.get_client(&cluster_name) {
                            match client.get_node_hot_threads(&node_name).await {
                                Ok(text) => {
                                    let _ = tx.send(RefreshMsg::HotThreadsResult(
                                        cluster_name,
                                        node_name,
                                        Ok(text),
                                    ));
                                }
                                Err(e) => {
                                    let _ = tx.send(RefreshMsg::HotThreadsResult(
                                        cluster_name,
                                        node_name,
                                        Err(e.to_string()),
                                    ));
                                }
                            }
                        } else {
                            let _ = tx.send(RefreshMsg::HotThreadsResult(
                                cluster_name,
                                node_name,
                                Err("Failed to obtain client".to_string()),
                            ));
                        }
                        ctx.request_repaint();
                    });
                }

                if let Some(cluster_name) = on_show_pending {
                    self.show_pending_cluster = Some(cluster_name);
                }
            }
            Tab::Dashboard => {
                let clusters: Vec<_> = self
                    .cluster_manager
                    .clusters()
                    .into_iter()
                    .filter(|c| self.cluster_matches_filter(&c.name))
                    .collect();
                let filtered_state = StatusState {
                    health_data: self
                        .status_state
                        .health_data
                        .iter()
                        .filter(|(n, _)| self.cluster_matches_filter(n))
                        .cloned()
                        .collect(),
                    stats_data: self
                        .status_state
                        .stats_data
                        .iter()
                        .filter(|(n, _)| self.cluster_matches_filter(n))
                        .cloned()
                        .collect(),
                    errors: self
                        .status_state
                        .errors
                        .iter()
                        .filter(|(n, _)| self.cluster_matches_filter(n))
                        .map(|(n, e)| (n.clone(), e.clone()))
                        .collect(),
                    explains: self
                        .status_state
                        .explains
                        .iter()
                        .filter(|(n, _)| self.cluster_matches_filter(n))
                        .map(|(n, e)| (n.clone(), e.clone()))
                        .collect(),
                    es_versions: self
                        .status_state
                        .es_versions
                        .iter()
                        .filter(|(n, _)| self.cluster_matches_filter(n))
                        .map(|(n, v)| (n.clone(), v.clone()))
                        .collect(),
                    kibana_versions: self
                        .status_state
                        .kibana_versions
                        .iter()
                        .filter(|(n, _)| self.cluster_matches_filter(n))
                        .map(|(n, v)| (n.clone(), v.clone()))
                        .collect(),
                    allocations: self
                        .status_state
                        .allocations
                        .iter()
                        .filter(|(n, _)| self.cluster_matches_filter(n))
                        .map(|(n, a)| (n.clone(), a.clone()))
                        .collect(),
                    nodes: self
                        .status_state
                        .nodes
                        .iter()
                        .filter(|(n, _)| self.cluster_matches_filter(n))
                        .map(|(n, nodes_vec)| (n.clone(), nodes_vec.clone()))
                        .collect(),
                    pending_tasks: self
                        .status_state
                        .pending_tasks
                        .iter()
                        .filter(|(n, _)| self.cluster_matches_filter(n))
                        .map(|(n, t)| (n.clone(), t.clone()))
                        .collect(),
                };
                render_dashboard_module(
                    ui,
                    &clusters,
                    &mut self.dashboard_state,
                    &filtered_state,
                    self.vfx.hover_effects && !self.vfx.reduce_motion,
                );
            }
            Tab::Tasks => {
                let mut filtered_tasks_state = TasksState {
                    tasks: self
                        .tasks_state
                        .tasks
                        .iter()
                        .filter(|(n, _)| self.cluster_matches_filter(n))
                        .cloned()
                        .collect(),
                    filter: self.tasks_state.filter.clone(),
                    selected_type: self.tasks_state.selected_type.clone(),
                    errors: self
                        .tasks_state
                        .errors
                        .iter()
                        .filter(|(n, _)| self.cluster_matches_filter(n))
                        .map(|(n, e)| (n.clone(), e.clone()))
                        .collect(),
                    expanded_tasks: self.tasks_state.expanded_tasks.clone(),
                };
                render_tasks_module(ui, &mut filtered_tasks_state);
                self.tasks_state.filter = filtered_tasks_state.filter;
                self.tasks_state.selected_type = filtered_tasks_state.selected_type;
                self.tasks_state.expanded_tasks = filtered_tasks_state.expanded_tasks;
            }
            Tab::Console => {
                let names: Vec<String> = self
                    .cluster_manager
                    .clusters()
                    .iter()
                    .filter(|c| self.cluster_matches_filter(&c.name))
                    .map(|c| c.name.clone())
                    .collect();
                if self.console_state.selected_cluster.is_empty() && !names.is_empty() {
                    self.console_state.selected_cluster = names[0].clone();
                }

                let selected = self.console_state.selected_cluster.clone();

                // Sync variables and saved queries when cluster selection changes
                if self.console_state.last_selected_cluster != selected {
                    if let Some(data) = self.cluster_manager.get_cluster_data(&selected) {
                        self.console_state.variables = data.variables.clone();
                        self.console_state.saved_queries = data.saved_queries.clone();
                    } else {
                        self.console_state.variables.clear();
                        self.console_state.saved_queries.clear();
                    }
                    self.console_state.last_selected_cluster = selected.clone();
                    self.console_state.variables_changed = false;
                }

                let mut on_save_query = None;
                let mut on_delete_query = None;
                render_console_module(
                    ui,
                    &mut self.console_state,
                    &names,
                    &mut self.console_send,
                    &mut on_save_query,
                    &mut on_delete_query,
                );

                if let Some(query) = on_save_query {
                    let cluster = &self.console_state.selected_cluster;
                    if let Some(mut data) = self.cluster_manager.get_cluster_data(cluster) {
                        // Replace if name exists
                        if let Some(idx) =
                            data.saved_queries.iter().position(|q| q.name == query.name)
                        {
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
                    // Also update console state so it shows immediately
                    if let Some(data) = self.cluster_manager.get_cluster_data(cluster) {
                        self.console_state.saved_queries = data.saved_queries;
                    }
                }

                if let Some(query_name) = on_delete_query {
                    let cluster = &self.console_state.selected_cluster;
                    if let Some(mut data) = self.cluster_manager.get_cluster_data(cluster) {
                        data.saved_queries.retain(|q| q.name != query_name);
                        self.cluster_manager.set_cluster_data(cluster, data);
                    }
                    if let Some(data) = self.cluster_manager.get_cluster_data(cluster) {
                        self.console_state.saved_queries = data.saved_queries;
                    }
                    self.toasts.info(format!("Deleted query '{}'", query_name));
                }

                // Persist variable changes back to ClusterData when modified
                if self.console_state.variables_changed && !selected.is_empty() {
                    if let Some(mut data) = self.cluster_manager.get_cluster_data(&selected) {
                        data.variables = self.console_state.variables.clone();
                        self.cluster_manager.set_cluster_data(&selected, data);
                    } else {
                        let mut data = crate::core::config::ClusterData::default();
                        data.variables = self.console_state.variables.clone();
                        self.cluster_manager.set_cluster_data(&selected, data);
                    }
                    self.console_state.variables_changed = false;
                }
            }
            Tab::Discover => {
                let mut search_triggered = None;
                let cluster_names: Vec<String> = self
                    .cluster_manager
                    .clusters()
                    .iter()
                    .filter(|c| self.cluster_matches_filter(&c.name))
                    .map(|c| c.name.clone())
                    .collect();
                render_discover_module(
                    ui,
                    &mut self.discover_state,
                    &cluster_names,
                    &mut search_triggered,
                );
                if let Some((path, body)) = search_triggered {
                    self.discover_send =
                        Some((self.discover_state.selected_cluster.clone(), path, body));
                }
            }
            Tab::Indices => {
                let cluster_names: Vec<String> = self
                    .cluster_manager
                    .clusters()
                    .iter()
                    .filter(|c| self.cluster_matches_filter(&c.name))
                    .map(|c| c.name.clone())
                    .collect();
                let mut on_fetch_detail = None;
                render_indices_module(
                    ui,
                    &mut self.indices_state,
                    &cluster_names,
                    &mut self.indices_refresh,
                    &mut on_fetch_detail,
                );

                if let Some((target_name, is_datastream)) = on_fetch_detail {
                    self.indices_state.detail_loading = true;
                    self.indices_state.selected_detail = None;

                    let tx = self.refresh_tx.clone();
                    let ctx = ui.ctx().clone();
                    let manager = self.cluster_manager.clone();
                    let cluster_name = self.indices_state.selected_cluster.clone();

                    tokio::spawn(async move {
                        if let Some(client) = manager.get_client(&cluster_name) {
                            let mut detail = crate::modules::indices::IndexDetail {
                                name: target_name.clone(),
                                is_datastream,
                                ilm_policy: None,
                                ilm_explain: None,
                                index_template: None,
                                settings: None,
                            };

                            // 1. Fetch settings
                            let settings_path = format!("/{}/_settings", target_name);
                            if let Ok(settings_val) = client
                                .execute(reqwest::Method::GET, &settings_path, None)
                                .await
                            {
                                detail.settings = Some(settings_val.clone());
                                if let Some(obj) = settings_val.get(&target_name).or_else(|| {
                                    settings_val.as_object().and_then(|o| o.values().next())
                                }) {
                                    if let Some(ilm_name) = obj
                                        .pointer("/settings/index/lifecycle/name")
                                        .and_then(|v| v.as_str())
                                    {
                                        detail.ilm_policy = Some(ilm_name.to_string());
                                    }
                                }
                            }

                            // 2. Fetch ILM explain
                            let ilm_path = format!("/{}/_ilm/explain", target_name);
                            if let Ok(ilm_val) =
                                client.execute(reqwest::Method::GET, &ilm_path, None).await
                            {
                                detail.ilm_explain = Some(ilm_val.clone());
                                if detail.ilm_policy.is_none() {
                                    if let Some(policy_name) = ilm_val
                                        .pointer("/indices")
                                        .and_then(|ind| ind.as_object())
                                        .and_then(|obj| obj.values().next())
                                        .and_then(|val| val.get("policy"))
                                        .and_then(|p| p.as_str())
                                    {
                                        detail.ilm_policy = Some(policy_name.to_string());
                                    }
                                }
                            }

                            // 3. Match template
                            if is_datastream {
                                let ds_path = format!("/_data_stream/{}", target_name);
                                if let Ok(ds_val) =
                                    client.execute(reqwest::Method::GET, &ds_path, None).await
                                {
                                    if let Some(template_name) = ds_val
                                        .pointer("/data_streams/0/template")
                                        .and_then(|v| v.as_str())
                                    {
                                        detail.index_template = Some(template_name.to_string());
                                    }
                                    if detail.ilm_policy.is_none() {
                                        if let Some(ilm_name) = ds_val
                                            .pointer("/data_streams/0/ilm_policy")
                                            .and_then(|v| v.as_str())
                                        {
                                            detail.ilm_policy = Some(ilm_name.to_string());
                                        }
                                    }
                                }
                            } else {
                                if let Ok(templates_val) = client
                                    .execute(reqwest::Method::GET, "/_index_template", None)
                                    .await
                                {
                                    if let Some(templates_arr) = templates_val
                                        .get("index_templates")
                                        .and_then(|a| a.as_array())
                                    {
                                        for t in templates_arr {
                                            if let Some(name) =
                                                t.get("name").and_then(|n| n.as_str())
                                            {
                                                if let Some(patterns) = t
                                                    .pointer("/index_template/index_patterns")
                                                    .and_then(|p| p.as_array())
                                                {
                                                    let matches = patterns.iter()
                                                        .filter_map(|p| p.as_str())
                                                        .any(|p| crate::modules::indices::index_pattern_matches(p, &target_name));
                                                    if matches {
                                                        detail.index_template =
                                                            Some(name.to_string());
                                                        break;
                                                    }
                                                }
                                            }
                                        }
                                    }
                                }
                            }

                            let _ = tx.send(RefreshMsg::IndexDetailResult(target_name, Ok(detail)));
                        } else {
                            let _ = tx.send(RefreshMsg::IndexDetailResult(
                                target_name,
                                Err("Failed to build client".to_string()),
                            ));
                        }
                        ctx.request_repaint();
                    });
                }
            }
            Tab::Observability => {
                let cluster_names: Vec<String> = self
                    .cluster_manager
                    .clusters()
                    .iter()
                    .filter(|c| self.cluster_matches_filter(&c.name))
                    .map(|c| c.name.clone())
                    .collect();
                render_observability_module(
                    ui,
                    &mut self.observability_state,
                    &cluster_names,
                    &mut self.observability_refresh,
                );
            }
            Tab::Settings => {
                let mut theme_changed = false;
                let mut vfx_changed = false;
                let mut tour_triggered = false;

                egui::ScrollArea::vertical()
                    .id_salt("settings_scroll")
                    .show(ui, |ui| {
                        render_appearance_module(
                            ui,
                            &mut self.appearance_state,
                            &mut self.theme,
                            &mut self.vfx,
                            &mut theme_changed,
                            &mut vfx_changed,
                            &mut tour_triggered,
                        );

                        ui.add_space(16.0);

                        // Timezone Clocks Manager Card
                        let mut clocks_changed = false;
                        egui::Frame::new()
                            .fill(Theme::bg_card())
                            .corner_radius(Theme::CARD_ROUNDING)
                            .inner_margin(Theme::CARD_PADDING)
                            .show(ui, |ui| {
                                ui.heading("🕒 World Clocks Configuration");
                                ui.add_space(4.0);
                                ui.label(egui::RichText::new("Configure timezones displayed in the left sidebar. Offsets are calculated dynamically based on regional DST rules or custom values.").color(Theme::text_muted()).size(11.0));
                                ui.add_space(12.0);

                                let mut to_delete = None;
                                for (idx, clock) in self.timezone_clocks.iter_mut().enumerate() {
                                    ui.horizontal(|ui| {
                                        if ui.checkbox(&mut clock.enabled, "").changed() {
                                            clocks_changed = true;
                                        }

                                        let label_edit = egui::TextEdit::singleline(&mut clock.label)
                                            .hint_text("Label (e.g. AMER (Chicago))");
                                        let res1 = ui.add_sized([150.0, ui.available_height()], label_edit);

                                        let zone_edit = egui::TextEdit::singleline(&mut clock.zone)
                                            .hint_text("Zone (Local, UTC, Sydney, Germany, Chicago, or +5, -3)");
                                        let res2 = ui.add_sized([180.0, ui.available_height()], zone_edit);

                                        if res1.changed() || res2.changed() {
                                            clocks_changed = true;
                                        }

                                        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                                            if ui.button("🗑").on_hover_text("Delete Clock").clicked() {
                                                to_delete = Some(idx);
                                                clocks_changed = true;
                                            }
                                        });
                                    });
                                    ui.add_space(8.0);
                                }

                                if let Some(idx) = to_delete {
                                    self.timezone_clocks.remove(idx);
                                }

                                ui.add_space(8.0);
                                ui.horizontal(|ui| {
                                    if ui.button("➕ Add Custom Clock").clicked() {
                                        self.timezone_clocks.push(crate::core::config::TimezoneClockConfig {
                                            label: "Custom Clock".to_string(),
                                            zone: "+0".to_string(),
                                            enabled: true,
                                        });
                                        clocks_changed = true;
                                    }

                                    if ui.button("🔄 Reset to Defaults").clicked() {
                                        self.timezone_clocks = crate::core::config::default_timezone_clocks();
                                        clocks_changed = true;
                                    }
                                });
                            });

                        if clocks_changed {
                            if let Err(e) = self.cluster_manager.save_timezone_clocks(self.timezone_clocks.clone()) {
                                self.toasts.error(format!("Failed to save timezone settings: {}", e));
                            }
                        }
                    });

                if tour_triggered {
                    self.wizard_state = Some(crate::ui::wizard::WizardState::default());
                    self.toasts.info("Onboarding tour started!");
                }
                if theme_changed || vfx_changed {
                    if let Err(e) = self
                        .cluster_manager
                        .save_theme_and_vfx(self.theme.clone(), self.vfx.clone())
                    {
                        self.toasts
                            .error(format!("Failed to save appearance settings: {}", e));
                    }
                }
            }
            Tab::PipelineSimulator => {
                render_pipeline_module(ui, &mut self.pipeline_state);
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
                ui.horizontal(|ui| {
                    ui.label("Kibana Host:");
                    ui.text_edit_singleline(&mut self.new_cluster.kibana_host);
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
                                    let result = c.test_connection_detailed().await;
                                    let msg = match result {
                                        Ok(msg) => msg,
                                        Err(msg) => msg,
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
                let mut save_clicked = false;
                ui.horizontal(|ui| {
                    if ui.button("Save").clicked() {
                        save_clicked = true;
                    }
                    if ui.button("Cancel").clicked() {
                        self.show_add_cluster = false;
                        self.editing_cluster = None;
                    }
                });

                let name_empty = self.new_cluster.name.trim().is_empty();
                let host_empty = self.new_cluster.host.trim().is_empty();
                if save_clicked {
                    if name_empty || host_empty {
                        ui.label(
                            egui::RichText::new("Name and Host are required.")
                                .color(Theme::danger())
                                .size(12.0),
                        );
                    } else {
                        let name = self.new_cluster.name.trim().to_string();
                        self.new_cluster.name = name.clone();
                        if let Err(e) = crate::core::auth::set_password(&name, &self.new_password) {
                            self.toasts.error(format!("Failed to save password: {}", e));
                        }

                        let save_ok = if let Some(ref old_name) = self.editing_cluster {
                            self.cluster_manager
                                .update_cluster(
                                    old_name,
                                    self.new_cluster.clone(),
                                    Some(&self.new_password),
                                )
                                .is_ok()
                        } else {
                            self.cluster_manager
                                .add_cluster(self.new_cluster.clone(), Some(&self.new_password))
                                .is_ok()
                        };

                        if save_ok {
                            self.trigger_refresh(ctx);
                            self.show_add_cluster = false;
                            self.editing_cluster = None;
                            self.new_cluster = ClusterConfig::default();
                            self.new_password.clear();
                            self.test_result = None;
                        }
                    }
                } else if name_empty || host_empty {
                    ui.label(
                        egui::RichText::new("Name and Host are required.")
                            .color(Theme::warning())
                            .size(12.0),
                    );
                }
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
                            if let Err(e) = self.cluster_manager.remove_cluster(&name) {
                                self.toasts
                                    .error(format!("Failed to remove cluster: {}", e));
                            }
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
        // Apply theme to egui context
        ctx.set_visuals(self.theme.to_egui_visuals());

        // Process async results
        self.process_refresh_results(ctx);

        // Ensure selected clusters match current filter
        self.ensure_selected_clusters_match_filter();

        // Handle clusters import
        if let Some(imported) = self.clusters_import.take() {
            for cluster in imported.clusters {
                if let Err(e) = self.cluster_manager.add_cluster(cluster, None) {
                    self.toasts
                        .error(format!("Failed to import cluster: {}", e));
                }
            }
            for (name, data) in imported.cluster_data {
                self.cluster_manager.set_cluster_data(&name, data);
            }
        }

        // Handle console send
        if let Some((cluster_name, method, path, body, use_kibana)) = self.console_send.take() {
            if let Some(client) = self.cluster_manager.get_client(&cluster_name) {
                let tx = self.refresh_tx.clone();
                let ctx = ctx.clone();
                let cluster_config = self
                    .cluster_manager
                    .clusters()
                    .into_iter()
                    .find(|c| c.name == cluster_name);
                tokio::spawn(async move {
                    let method = match method.as_str() {
                        "GET" => reqwest::Method::GET,
                        "POST" => reqwest::Method::POST,
                        "PUT" => reqwest::Method::PUT,
                        "DELETE" => reqwest::Method::DELETE,
                        "HEAD" => reqwest::Method::HEAD,
                        _ => reqwest::Method::GET,
                    };
                    let result = if use_kibana {
                        if let Some(ref config) = cluster_config {
                            let kibana_host = if config.kibana_host.is_empty() {
                                if config.host.contains("elastic") {
                                    config.host.replace("elastic", "kibana")
                                } else {
                                    config.host.clone()
                                }
                            } else {
                                let h = config.kibana_host.trim();
                                if h.starts_with("http://") || h.starts_with("https://") {
                                    h.to_string()
                                } else {
                                    format!("http://{}", h)
                                }
                            };
                            client
                                .send_to_host_raw(&kibana_host, method, &path, body)
                                .await
                                .map_err(|e| e.to_string())
                        } else {
                            Err("No cluster config found".to_string())
                        }
                    } else {
                        client
                            .execute_raw(method, &path, body)
                            .await
                            .map_err(|e| e.to_string())
                    };
                    let _ = tx.send(RefreshMsg::ConsoleResult(result));
                    ctx.request_repaint();
                });
            } else {
                let _ = self.refresh_tx.send(RefreshMsg::ConsoleResult(Err(
                    "Failed to obtain cluster client. Ensure the cluster is configured and reachable."
                        .to_string(),
                )));
            }
        }

        // Handle discover send
        if let Some((cluster_name, path, body)) = self.discover_send.take() {
            if let Some(client) = self.cluster_manager.get_client(&cluster_name) {
                let tx = self.refresh_tx.clone();
                let ctx = ctx.clone();
                tokio::spawn(async move {
                    let method = reqwest::Method::POST;
                    let result = client
                        .execute_raw(method, &path, Some(body))
                        .await
                        .map_err(|e| e.to_string());
                    let _ = tx.send(RefreshMsg::DiscoverResult(result));
                    ctx.request_repaint();
                });
            } else {
                let _ = self.refresh_tx.send(RefreshMsg::DiscoverResult(Err(
                    "Failed to obtain cluster client. Ensure the cluster is configured and reachable."
                        .to_string(),
                )));
            }
        }

        // Handle indices refresh
        if let Some((cluster_name, _)) = self.indices_refresh.take() {
            if let Some(client) = self.cluster_manager.get_client(&cluster_name) {
                let tx = self.refresh_tx.clone();
                let ctx = ctx.clone();
                let name = cluster_name.clone();
                tokio::spawn(async move {
                    let indices_res = client.cat_indices().await;
                    let datastreams_res = client.get_data_streams().await;
                    match (indices_res, datastreams_res) {
                        (Ok(indices), Ok(ds_resp)) => {
                            let _ = tx.send(RefreshMsg::IndicesResult(
                                name,
                                indices,
                                ds_resp.data_streams,
                            ));
                        }
                        (Ok(indices), Err(_)) => {
                            let _ = tx.send(RefreshMsg::IndicesResult(name, indices, Vec::new()));
                        }
                        (Err(e), _) => {
                            let _ = tx.send(RefreshMsg::IndicesError(name, e.to_string()));
                        }
                    }
                    ctx.request_repaint();
                });
            } else {
                let _ = self.refresh_tx.send(RefreshMsg::IndicesError(
                    cluster_name,
                    "Failed to obtain cluster client. Ensure the cluster is configured and reachable."
                        .to_string(),
                ));
            }
        }

        // Handle observability refresh
        if let Some((cluster_name, space_id)) = self.observability_refresh.take() {
            if let Some(client) = self.cluster_manager.get_client(&cluster_name) {
                let tx = self.refresh_tx.clone();
                let ctx = ctx.clone();
                let name = cluster_name.clone();
                let space = space_id.clone();
                let cluster_config = self
                    .cluster_manager
                    .clusters()
                    .into_iter()
                    .find(|c| c.name == cluster_name);
                tokio::spawn(async move {
                    if let Some(ref config) = cluster_config {
                        let kibana_host = if config.kibana_host.is_empty() {
                            if config.host.contains("elastic") {
                                config.host.replace("elastic", "kibana")
                            } else {
                                config.host.clone()
                            }
                        } else {
                            let h = config.kibana_host.trim();
                            if h.starts_with("http://") || h.starts_with("https://") {
                                h.to_string()
                            } else {
                                format!("http://{}", h)
                            }
                        };
                        match client
                            .get_kibana_synthetics_monitors(&kibana_host, Some(&space))
                            .await
                        {
                            Ok(val) => {
                                let mut monitors = Vec::new();
                                if let Some(monitors_arr) =
                                    val.get("monitors").and_then(|m| m.as_array())
                                {
                                    for m in monitors_arr {
                                        if let (Some(id), Some(monitor_name)) = (
                                            m.get("id").and_then(|id| id.as_str()),
                                            m.get("name").and_then(|n| n.as_str()),
                                        ) {
                                            let monitor_type = m
                                                .get("type")
                                                .and_then(|t| t.as_str())
                                                .unwrap_or("http")
                                                .to_string();
                                            let status = m
                                                .get("status")
                                                .and_then(|s| s.get("status"))
                                                .and_then(|s| s.as_str())
                                                .unwrap_or("up")
                                                .to_string();
                                            let url = m
                                                .get("url")
                                                .and_then(|u| u.as_str())
                                                .unwrap_or("")
                                                .to_string();

                                            let mut locations = Vec::new();
                                            if let Some(locs) =
                                                m.get("locations").and_then(|l| l.as_array())
                                            {
                                                for l in locs {
                                                    if let Some(l_str) = l.as_str() {
                                                        locations.push(l_str.to_string());
                                                    }
                                                }
                                            }

                                            let latency_ms = m
                                                .get("metrics")
                                                .and_then(|m| m.get("latency"))
                                                .and_then(|l| l.as_u64())
                                                .unwrap_or(50)
                                                as u32;

                                            monitors.push(
                                                crate::modules::observability::SyntheticMonitor {
                                                    id: id.to_string(),
                                                    name: monitor_name.to_string(),
                                                    monitor_type,
                                                    status,
                                                    url,
                                                    locations,
                                                    latency_ms,
                                                    latency_history: vec![
                                                        latency_ms as f32,
                                                        (latency_ms + 5) as f32,
                                                        (latency_ms - 2) as f32,
                                                    ],
                                                    last_checked: "Just now".to_string(),
                                                },
                                            );
                                        }
                                    }
                                }

                                if monitors.is_empty() {
                                    let _ = tx.send(RefreshMsg::ObservabilityError(
                                        name.clone(),
                                        "No monitors configured in this Kibana space.".to_string(),
                                    ));
                                } else {
                                    let _ =
                                        tx.send(RefreshMsg::ObservabilityResult(name, monitors));
                                }
                            }
                            Err(e) => {
                                let _ =
                                    tx.send(RefreshMsg::ObservabilityError(name, e.to_string()));
                            }
                        }
                    } else {
                        let _ = tx.send(RefreshMsg::ObservabilityError(
                            name,
                            "No cluster configuration found".to_string(),
                        ));
                    }
                    ctx.request_repaint();
                });
            } else {
                let _ = self.refresh_tx.send(RefreshMsg::ObservabilityError(
                    cluster_name,
                    "Failed to obtain cluster client. Ensure the cluster is configured and reachable."
                        .to_string(),
                ));
            }
        }

        // Auto refresh
        if self.auto_refresh {
            if self.snapshot_manual_refresh {
                self.snapshot_manual_refresh = false;
                if !self.snapshot_statuses.is_empty() {
                    self.trigger_refresh(ctx);
                }
            }
            let should_refresh = self.last_refresh.map_or(true, |last| {
                last.elapsed().as_secs() >= self.refresh_interval_secs
            });
            if should_refresh {
                self.trigger_refresh(ctx);
            }
        }

        // Background VFX
        let screen_rect = ctx.screen_rect();
        vfx::paint_background(ctx, &self.vfx, screen_rect);
        vfx::paint_cursor_glow(ctx, &self.vfx, screen_rect);

        // Glassmorphism translucent fills if VFX is active
        let has_vfx = self.vfx.background_effect != crate::core::config::BackgroundEffect::None
            && self.vfx.background_intensity > 0.0;
        let sidebar_fill = if has_vfx {
            Theme::bg_darkest().linear_multiply(0.85)
        } else {
            Theme::bg_darkest()
        };
        let central_fill = if has_vfx {
            Theme::bg_dark().linear_multiply(0.88)
        } else {
            Theme::bg_dark()
        };

        // Sidebar
        egui::SidePanel::left("sidebar")
            .resizable(true)
            .default_width(220.0)
            .min_width(180.0)
            .max_width(400.0)
            .frame(
                egui::Frame::new()
                    .fill(sidebar_fill)
                    .stroke(egui::Stroke::new(1.0, Theme::border()))
                    .inner_margin(egui::Margin {
                        left: 12,
                        right: 12,
                        top: 10,
                        bottom: 12,
                    }),
            )
            .show(ctx, |ui| {
                self.render_sidebar(ui);
            });

        // Main content
        egui::CentralPanel::default()
            .frame(
                egui::Frame::new()
                    .fill(central_fill)
                    .stroke(egui::Stroke::new(1.0, Theme::border()))
                    .inner_margin(egui::Vec2::new(16.0, 16.0)),
            )
            .show(ctx, |ui| {
                self.render_tabs(ui);
                self.render_content(ui);
            });

        // Dialogs
        self.render_add_cluster_dialog(ctx);
        self.render_delete_confirmation(ctx);

        // Onboarding Wizard Overlay
        if let Some(mut state) = self.wizard_state.clone() {
            let mut on_dismiss = false;
            crate::ui::wizard::render_wizard_overlay(
                ctx,
                &mut state,
                &mut self.cluster_manager,
                &mut self.current_tab,
                &mut self.toasts,
                &mut on_dismiss,
            );
            self.wizard_state = Some(state);
            if on_dismiss {
                self.wizard_state = None;
                let mut config = crate::core::config::AppConfig::load().unwrap_or_default();
                config.wizard_completed = true;
                let _ = config.save();
                self.toasts.info("Onboarding tour completed!");
            }
        }

        // Track window size/position for persistence
        if let Some(rect) = ctx.input(|i| i.viewport().inner_rect) {
            let size = rect.size();
            self.window_size = [size.x, size.y];
            self.window_pos = Some([rect.min.x, rect.min.y]);
        }

        // Debounced config save
        if let Err(e) = self.cluster_manager.save_if_due() {
            self.toasts.error(format!("Failed to save config: {}", e));
        }

        // Log viewer window (toggled via konami code: hover title + press 6 x3)
        if self.show_log_window {
            let log_entries = self.log_entries.clone();
            ctx.show_viewport_deferred(
                egui::ViewportId::from_hash_of("log_viewer"),
                egui::ViewportBuilder::default()
                    .with_title("Logs")
                    .with_inner_size([700.0, 500.0])
                    .with_min_inner_size([400.0, 300.0]),
                move |ctx, _class| {
                    let entries = log_entries.read().unwrap_or_else(|e| e.into_inner());
                    crate::ui::log_buffer::render_log_viewer(ctx, &entries);
                },
            );
        }

        // Render Snapshot History Popup Window
        if let Some(ref cluster_name) = self.show_history_cluster {
            let mut is_open = true;
            let history_list = self.cluster_histories_fetched.get(cluster_name).cloned();

            egui::Window::new(format!("📜 Snapshot History - {}", cluster_name))
                .open(&mut is_open)
                .collapsible(false)
                .resizable(true)
                .default_size([750.0, 480.0])
                .show(ctx, |ui| {
                    if self.history_loading {
                        ui.horizontal(|ui| {
                            ui.spinner();
                            ui.label("Fetching backup history from cluster...");
                        });
                    } else if let Some(snapshots) = history_list {
                        if snapshots.is_empty() {
                            ui.label(
                                egui::RichText::new("No past snapshots found in this repository.")
                                    .color(Theme::text_muted())
                                    .size(13.0),
                            );
                        } else {
                            ui.label(
                                egui::RichText::new(format!(
                                    "Showing the latest {} snapshot executions in this repository.",
                                    snapshots.len()
                                ))
                                .color(Theme::text_muted())
                                .size(11.0),
                            );
                            ui.add_space(8.0);

                            egui::ScrollArea::vertical()
                                .id_salt("snapshot_history_scroll")
                                .show(ui, |ui| {
                                    use egui_extras::{Column, TableBuilder};

                                    TableBuilder::new(ui)
                                        .striped(true)
                                        .resizable(true)
                                        .cell_layout(egui::Layout::left_to_right(egui::Align::Center))
                                        .column(Column::auto().at_least(180.0)) // Name
                                        .column(Column::auto().at_least(100.0)) // State
                                        .column(Column::auto().at_least(160.0)) // Started
                                        .column(Column::auto().at_least(160.0)) // Finished
                                        .column(Column::auto().at_least(90.0))  // Duration
                                        .column(Column::auto().at_least(100.0)) // Shards
                                        .header(22.0, |mut header| {
                                            header.col(|ui| { ui.strong("Snapshot Name"); });
                                            header.col(|ui| { ui.strong("State"); });
                                            header.col(|ui| { ui.strong("Time Started"); });
                                            header.col(|ui| { ui.strong("Time Finished"); });
                                            header.col(|ui| { ui.strong("Time Elapsed"); });
                                            header.col(|ui| { ui.strong("Shards"); });
                                        })
                                        .body(|body| {
                                            let mut sorted_snapshots = snapshots.clone();
                                            sorted_snapshots.sort_by(|a, b| {
                                                b.start_time_in_millis.cmp(&a.start_time_in_millis)
                                            });

                                            body.rows(20.0, sorted_snapshots.len(), |mut row| {
                                                let idx = row.index();
                                                let snap = &sorted_snapshots[idx];

                                                // Column 1: Name
                                                row.col(|ui| {
                                                    ui.label(
                                                        egui::RichText::new(&snap.snapshot)
                                                            .monospace()
                                                            .size(10.5)
                                                            .color(Theme::text_primary()),
                                                    );
                                                });

                                                // Column 2: State badge
                                                row.col(|ui| {
                                                    let state = crate::modules::snapshot::SnapshotState::from_str(&snap.state);
                                                    ui.add(crate::ui::widgets::StatePill::new(state.as_str(), state.color()));
                                                });

                                                // Column 3: Time Started
                                                row.col(|ui| {
                                                    let start = snap.start_time.as_deref().unwrap_or("—");
                                                    ui.label(
                                                        egui::RichText::new(start)
                                                            .size(10.0)
                                                            .color(Theme::text_muted()),
                                                    );
                                                });

                                                // Column 4: Time Finished
                                                row.col(|ui| {
                                                    let end = snap.end_time.as_deref().unwrap_or("—");
                                                    ui.label(
                                                        egui::RichText::new(end)
                                                            .size(10.0)
                                                            .color(Theme::text_muted()),
                                                    );
                                                });

                                                // Column 5: Time Elapsed
                                                row.col(|ui| {
                                                    let elapsed = snap.duration_in_millis
                                                        .map(|ms| crate::ui::widgets::human_duration((ms / 1000) as u64))
                                                        .unwrap_or_else(|| "—".to_string());
                                                    ui.label(
                                                        egui::RichText::new(elapsed)
                                                            .size(10.0)
                                                            .color(Theme::text_muted()),
                                                    );
                                                });

                                                // Column 6: Shards
                                                row.col(|ui| {
                                                    if let Some(ref shards) = snap.shards {
                                                        let color = if shards.failed > 0 {
                                                            Theme::danger()
                                                        } else {
                                                            Theme::snapshot_success()
                                                        };
                                                        ui.label(
                                                            egui::RichText::new(format!(
                                                                "S: {} / F: {}",
                                                                shards.successful, shards.failed
                                                            ))
                                                            .size(10.0)
                                                            .color(color)
                                                            .strong(),
                                                        );
                                                    } else {
                                                        ui.label(
                                                            egui::RichText::new("—")
                                                                .size(10.0)
                                                                .color(Theme::text_muted()),
                                                        );
                                                    }
                                                });
                                            });
                                        });
                                });
                        }
                    } else {
                        ui.label("Error: Unable to load history data.");
                    }
                });

            if !is_open {
                self.show_history_cluster = None;
            }
        }

        // Render Hot Threads Popup Window
        if let Some((ref cluster_name, ref node_name)) = self.hot_threads_node {
            let mut is_open = true;
            egui::Window::new(format!(
                "🔥 Node Hot Threads - {} ({})",
                node_name, cluster_name
            ))
            .open(&mut is_open)
            .collapsible(false)
            .resizable(true)
            .default_size([750.0, 520.0])
            .show(ctx, |ui| {
                if self.hot_threads_loading {
                    ui.horizontal(|ui| {
                        ui.spinner();
                        ui.label(format!("Sampling hot threads on node {}...", node_name));
                    });
                } else if let Some(ref text) = self.hot_threads_text {
                    egui::ScrollArea::both()
                        .id_salt("hot_threads_scroll")
                        .show(ui, |ui| {
                            ui.add(
                                egui::TextEdit::multiline(&mut text.as_str())
                                    .font(egui::TextStyle::Monospace)
                                    .text_color(Theme::text_primary())
                                    .frame(false)
                                    .layouter(&mut |ui, string, _wrap_width| {
                                        let mut layout_job = egui::text::LayoutJob::default();
                                        layout_job.append(
                                            string,
                                            0.0,
                                            egui::TextFormat {
                                                font_id: egui::FontId::monospace(10.5),
                                                color: Theme::text_primary(),
                                                ..Default::default()
                                            },
                                        );
                                        ui.fonts(|f| f.layout_job(layout_job))
                                    }),
                            );
                        });
                } else {
                    ui.label("No thread dump available.");
                }
            });
            if !is_open {
                self.hot_threads_node = None;
                self.hot_threads_text = None;
                self.hot_threads_loading = false;
            }
        }

        // Render Pending Tasks Popup Window
        if let Some(ref cluster_name) = self.show_pending_cluster {
            let mut is_open = true;
            let pending_list = self.status_state.pending_tasks.get(cluster_name).cloned();

            egui::Window::new(format!("⏳ Pending Master Tasks - {}", cluster_name))
                .open(&mut is_open)
                .collapsible(false)
                .resizable(true)
                .default_size([650.0, 400.0])
                .show(ctx, |ui| {
                    if let Some(tasks) = pending_list {
                        if tasks.is_empty() {
                            ui.label(
                                egui::RichText::new(
                                    "No pending metadata tasks currently in queue.",
                                )
                                .color(Theme::text_muted())
                                .size(13.0),
                            );
                        } else {
                            ui.label(
                                egui::RichText::new(format!(
                                    "{} tasks waiting for master node execution:",
                                    tasks.len()
                                ))
                                .strong()
                                .color(Theme::accent())
                                .size(12.0),
                            );
                            ui.add_space(8.0);

                            egui::ScrollArea::vertical()
                                .id_salt("pending_tasks_scroll")
                                .show(ui, |ui| {
                                    for task in &tasks {
                                        let priority = task
                                            .get("priority")
                                            .and_then(|p| p.as_str())
                                            .unwrap_or("NORMAL");
                                        let insert_order = task
                                            .get("insert_order")
                                            .and_then(|o| o.as_i64())
                                            .unwrap_or(0);
                                        let source = task
                                            .get("source")
                                            .and_then(|s| s.as_str())
                                            .unwrap_or("Unknown source");
                                        let time_in_queue = task
                                            .get("time_in_queue")
                                            .and_then(|t| t.as_str())
                                            .unwrap_or("0s");

                                        let bg_color = match priority {
                                            "HIGH" | "URGENT" => {
                                                Theme::danger().to_opaque().linear_multiply(0.15)
                                            }
                                            _ => Theme::bg_input(),
                                        };

                                        egui::Frame::new()
                                            .fill(bg_color)
                                            .corner_radius(Theme::CARD_ROUNDING)
                                            .inner_margin(8.0)
                                            .stroke(egui::Stroke::new(1.0, Theme::bg_input()))
                                            .show(ui, |ui| {
                                                ui.horizontal(|ui| {
                                                    ui.label(
                                                        egui::RichText::new(format!(
                                                            "#{}",
                                                            insert_order
                                                        ))
                                                        .monospace()
                                                        .size(11.0)
                                                        .color(Theme::text_muted()),
                                                    );
                                                    ui.with_layout(
                                                        egui::Layout::right_to_left(
                                                            egui::Align::Center,
                                                        ),
                                                        |ui| {
                                                            let badge_color = match priority {
                                                                "HIGH" | "URGENT" => {
                                                                    Theme::danger()
                                                                }
                                                                _ => Theme::accent(),
                                                            };
                                                            ui.add(
                                                                crate::ui::widgets::StatePill::new(
                                                                    priority,
                                                                    badge_color,
                                                                ),
                                                            );
                                                        },
                                                    );
                                                });
                                                ui.add_space(4.0);
                                                ui.label(
                                                    egui::RichText::new(source)
                                                        .strong()
                                                        .size(11.5)
                                                        .color(Theme::text_primary()),
                                                );
                                                ui.add_space(2.0);
                                                ui.label(
                                                    egui::RichText::new(format!(
                                                        "Time in queue: {}",
                                                        time_in_queue
                                                    ))
                                                    .size(10.5)
                                                    .color(Theme::text_muted()),
                                                );
                                            });
                                        ui.add_space(8.0);
                                    }
                                });
                        }
                    } else {
                        ui.label("Loading pending tasks or cluster is unreachable...");
                    }
                });

            if !is_open {
                self.show_pending_cluster = None;
            }
        }

        // Render Index/DataStream Details Popup Window
        if self.indices_state.detail_loading {
            let mut is_open = true;
            egui::Window::new("⏳ Loading Details...")
                .open(&mut is_open)
                .collapsible(false)
                .resizable(false)
                .anchor(egui::Align2::CENTER_CENTER, [0.0, 0.0])
                .show(ctx, |ui| {
                    ui.horizontal(|ui| {
                        ui.spinner();
                        ui.label(
                            "Fetching settings, lifecycle configurations, and index templates...",
                        );
                    });
                });
            if !is_open {
                self.indices_state.detail_loading = false;
            }
        }

        if let Some(ref detail) = self.indices_state.selected_detail {
            let mut is_open = true;
            let title = if detail.is_datastream {
                format!("📦 Data Stream Details - {}", detail.name)
            } else {
                format!("🗄️ Index Details - {}", detail.name)
            };

            egui::Window::new(title)
                .open(&mut is_open)
                .collapsible(false)
                .resizable(true)
                .default_size([700.0, 500.0])
                .show(ctx, |ui| {
                    egui::ScrollArea::vertical()
                        .id_salt("index_detail_scroll")
                        .show(ui, |ui| {
                            // Section 1: Quick Header Metrics/Information
                            ui.horizontal(|ui| {
                                ui.label(egui::RichText::new("Target Name:").strong().size(13.0).color(Theme::text_muted()));
                                ui.label(egui::RichText::new(&detail.name).strong().size(13.0).color(Theme::accent()));

                                ui.add_space(20.0);
                                ui.label(egui::RichText::new("Type:").strong().size(13.0).color(Theme::text_muted()));
                                let type_str = if detail.is_datastream { "Data Stream" } else { "Regular Index" };
                                ui.add(crate::ui::widgets::StatePill::new(type_str, Theme::accent()));
                            });
                            ui.add_space(10.0);

                            // Section 2: Index Template Information
                            egui::Frame::new()
                                .fill(Theme::bg_input())
                                .corner_radius(Theme::CARD_ROUNDING)
                                .inner_margin(12.0)
                                .show(ui, |ui| {
                                    ui.horizontal(|ui| {
                                        ui.label(egui::RichText::new("Matched Index Template:").strong().size(12.0).color(Theme::text_primary()));
                                        if let Some(ref template) = detail.index_template {
                                            ui.label(egui::RichText::new(template).strong().size(12.0).color(Theme::success()));
                                        } else {
                                            ui.label(egui::RichText::new("None (Directly created or no matching pattern found)").size(11.5).color(Theme::text_muted()).italics());
                                        }
                                    });
                                });
                            ui.add_space(12.0);

                            // Section 3: Index Lifecycle Management (ILM) Policy
                            egui::Frame::new()
                                .fill(Theme::bg_input())
                                .corner_radius(Theme::CARD_ROUNDING)
                                .inner_margin(12.0)
                                .show(ui, |ui| {
                                    ui.horizontal(|ui| {
                                        ui.label(egui::RichText::new("ILM Policy:").strong().size(12.0).color(Theme::text_primary()));
                                        if let Some(ref policy) = detail.ilm_policy {
                                            ui.label(egui::RichText::new(policy).strong().size(12.0).color(Theme::accent()));
                                        } else {
                                            ui.label(egui::RichText::new("No policy defined").size(11.5).color(Theme::text_muted()).italics());
                                        }
                                    });

                                    if let Some(ref explain) = detail.ilm_explain {
                                        ui.add_space(8.0);
                                        let mut found_explain = false;
                                        if let Some(indices_obj) = explain.pointer("/indices").and_then(|i| i.as_object()) {
                                            if let Some(val) = indices_obj.values().next() {
                                                found_explain = true;
                                                let phase = val.get("phase").and_then(|v| v.as_str()).unwrap_or("—");
                                                let action = val.get("action").and_then(|v| v.as_str()).unwrap_or("—");
                                                let step = val.get("step").and_then(|v| v.as_str()).unwrap_or("—");
                                                let age = val.get("age").and_then(|v| v.as_str()).unwrap_or("");

                                                ui.horizontal(|ui| {
                                                    ui.label(egui::RichText::new("ILM Explain status:").size(11.0).color(Theme::text_muted()));
                                                    ui.add(crate::ui::widgets::StatePill::new(format!("Phase: {}", phase), Theme::accent()));
                                                    ui.add(crate::ui::widgets::StatePill::new(format!("Action: {}", action), Theme::text_secondary()));
                                                    ui.add(crate::ui::widgets::StatePill::new(format!("Step: {}", step), Theme::text_muted()));
                                                    if !age.is_empty() {
                                                        ui.label(egui::RichText::new(format!("(Age: {})", age)).size(10.5).color(Theme::text_muted()));
                                                    }
                                                });
                                            }
                                        }
                                        if !found_explain && explain.get("policy").is_some() {
                                            let phase = explain.get("phase").and_then(|v| v.as_str()).unwrap_or("—");
                                            let action = explain.get("action").and_then(|v| v.as_str()).unwrap_or("—");
                                            let step = explain.get("step").and_then(|v| v.as_str()).unwrap_or("—");
                                            ui.horizontal(|ui| {
                                                ui.label(egui::RichText::new("ILM Explain status:").size(11.0).color(Theme::text_muted()));
                                                ui.add(crate::ui::widgets::StatePill::new(format!("Phase: {}", phase), Theme::accent()));
                                                ui.add(crate::ui::widgets::StatePill::new(format!("Action: {}", action), Theme::text_secondary()));
                                                ui.add(crate::ui::widgets::StatePill::new(format!("Step: {}", step), Theme::text_muted()));
                                            });
                                        }
                                    }
                                });
                            ui.add_space(12.0);

                            // Section 4: Raw Settings / Config Folding Tree
                            ui.collapsing(egui::RichText::new("⚙️ Index Settings").strong().size(13.0).color(Theme::text_primary()), |ui| {
                                if let Some(ref settings) = detail.settings {
                                    let formatted_settings = serde_json::to_string_pretty(settings).unwrap_or_default();
                                    egui::Frame::new()
                                        .fill(Theme::bg_input())
                                        .corner_radius(4.0)
                                        .inner_margin(8.0)
                                        .show(ui, |ui| {
                                            ui.add(
                                                egui::TextEdit::multiline(&mut formatted_settings.as_str())
                                                    .font(egui::TextStyle::Monospace)
                                                    .text_color(Theme::text_primary())
                                                    .frame(false)
                                            );
                                        });
                                } else {
                                    ui.label("Settings not available.");
                                }
                            });
                        });
                });
            if !is_open {
                self.indices_state.selected_detail = None;
            }
        }

        // Render toasts on top of everything
        self.toasts.render(ctx);
    }

    #[cfg(target_os = "windows")]
    fn on_exit(&mut self) {
        if let Err(e) = self.cluster_manager.save() {
            tracing::warn!("Failed to save config on exit: {}", e);
        }
        // Save window state directly
        let mut config = crate::core::config::AppConfig::load().unwrap_or_default();
        config.window_width = Some(self.window_size[0]);
        config.window_height = Some(self.window_size[1]);
        if let Some(pos) = self.window_pos {
            config.window_pos_x = Some(pos[0]);
            config.window_pos_y = Some(pos[1]);
        }
        config.pinned_monitor_ids = self.observability_state.pinned_monitor_ids.clone();
        config.pinned_monitor_layouts = self.observability_state.pinned_monitor_layouts.clone();
        if let Err(e) = config.save() {
            tracing::warn!("Failed to save window state: {}", e);
        }
    }

    #[cfg(not(target_os = "windows"))]
    fn on_exit(&mut self, _gl: Option<&eframe::glow::Context>) {
        if let Err(e) = self.cluster_manager.save() {
            tracing::warn!("Failed to save config on exit: {}", e);
        }
        // Save window state directly
        let mut config = crate::core::config::AppConfig::load().unwrap_or_default();
        config.window_width = Some(self.window_size[0]);
        config.window_height = Some(self.window_size[1]);
        if let Some(pos) = self.window_pos {
            config.window_pos_x = Some(pos[0]);
            config.window_pos_y = Some(pos[1]);
        }
        config.pinned_monitor_ids = self.observability_state.pinned_monitor_ids.clone();
        config.pinned_monitor_layouts = self.observability_state.pinned_monitor_layouts.clone();
        if let Err(e) = config.save() {
            tracing::warn!("Failed to save window state: {}", e);
        }
    }
}
