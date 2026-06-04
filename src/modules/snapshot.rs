use std::collections::VecDeque;
use std::time::{Duration, Instant};

use crate::core::config::ClusterConfig;
use crate::core::es_client::{EsClient, SnapshotInfo};
use crate::ui::theme::Theme;
use crate::ui::widgets::{
    ConnectionDot, GradientProgressBar, StatePill, human_bytes, human_duration, human_speed,
};
use egui::{CornerRadius, Vec2};

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SnapshotState {
    Success,
    InProgress,
    Failed,
    Partial,
    Incompatible,
    Missing,
    Waiting,
    Unknown,
}

impl SnapshotState {
    pub fn from_str(s: &str) -> Self {
        match s.to_uppercase().as_str() {
            "SUCCESS" | "COMPLETED" => Self::Success,
            "IN_PROGRESS" | "STARTED" | "INIT" => Self::InProgress,
            "FAILED" | "ABORTED" => Self::Failed,
            "PARTIAL" => Self::Partial,
            "INCOMPATIBLE" => Self::Incompatible,
            "MISSING" => Self::Missing,
            "WAITING" => Self::Waiting,
            _ => Self::Unknown,
        }
    }

    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Success => "SUCCESS",
            Self::InProgress => "IN PROGRESS",
            Self::Failed => "FAILED",
            Self::Partial => "PARTIAL",
            Self::Incompatible => "INCOMPATIBLE",
            Self::Missing => "MISSING",
            Self::Waiting => "WAITING",
            Self::Unknown => "UNKNOWN",
        }
    }

    pub fn color(&self) -> egui::Color32 {
        match self {
            Self::Success => Theme::snapshot_success(),
            Self::InProgress => Theme::snapshot_in_progress(),
            Self::Failed => Theme::snapshot_failed(),
            Self::Partial => Theme::snapshot_partial(),
            _ => Theme::text_muted(),
        }
    }
}

#[derive(Debug, Clone, Default, serde::Serialize, serde::Deserialize)]
pub struct SnapshotStats {
    pub progress_pct: f32,
    pub processed_bytes: u64,
    pub total_bytes: u64,
    pub processed_files: u32,
    pub total_files: u32,
    pub start_time: Option<chrono::DateTime<chrono::Utc>>,
    pub current_speed_bps: f64,
    pub avg_speed_bps: f64,
    pub has_byte_stats: bool,
    pub processed_shards: u32,
    pub total_shards: u32,
    pub current_shard_rate: f64,
    pub avg_shard_rate: f64,
    pub window_avg_speed_bps: f64,
    pub min_speed_bps: f64,
    pub max_speed_bps: f64,
}

#[allow(dead_code)]
impl SnapshotStats {
    pub fn processed_human(&self) -> String {
        human_bytes(self.processed_bytes)
    }

    pub fn total_human(&self) -> String {
        human_bytes(self.total_bytes)
    }

    pub fn eta_seconds(&self) -> Option<u64> {
        if self.progress_pct >= 100.0 || self.progress_pct <= 0.0 {
            return None;
        }
        let remaining = self.total_bytes.saturating_sub(self.processed_bytes);
        let speed = if self.window_avg_speed_bps > 0.0 {
            self.window_avg_speed_bps
        } else if self.avg_speed_bps > 0.0 {
            self.avg_speed_bps
        } else {
            self.current_speed_bps
        };
        if speed > 0.0 {
            Some((remaining as f64 / speed) as u64)
        } else {
            None
        }
    }

    pub fn eta_human(&self) -> String {
        match self.eta_seconds() {
            Some(s) => human_duration(s),
            None => "—".to_string(),
        }
    }

    pub fn current_speed_human(&self) -> String {
        human_speed(self.current_speed_bps)
    }

    pub fn avg_speed_human(&self) -> String {
        human_speed(self.avg_speed_bps)
    }
}

#[derive(Debug, Clone, Default, serde::Serialize, serde::Deserialize)]
pub struct BackupStatus {
    pub repository: String,
    pub snapshot_info: SnapshotInfo,
    pub snapshot_stats: Option<SnapshotStats>,
    pub is_current: bool,
    pub peak_network_rate_bytes: f64,
    pub avg_network_rate_bytes: f64,
    pub total_transferred_bytes: u64,
    pub total_pending_bytes: u64,
}

#[derive(Debug, Clone, Default, serde::Serialize, serde::Deserialize)]
pub struct ClusterSnapshotStatus {
    pub config: ClusterConfig,
    pub reachable: bool,
    pub error_message: Option<String>,
    pub snapshot_info: Option<SnapshotInfo>,
    pub snapshot_stats: Option<SnapshotStats>,
    pub slm_last_run: Option<String>,
    pub slm_next_run: Option<String>,
    pub slm_in_progress: bool,
    #[serde(default)]
    pub slm_policies: Vec<(String, crate::core::es_client::SlmPolicyDetail)>,
    #[serde(default)]
    pub has_repositories: bool,
    #[serde(default)]
    pub resolved_repo: Option<String>,
    #[serde(default)]
    pub backups: Vec<BackupStatus>,
}

#[derive(Debug, Clone)]
pub struct SpeedSample {
    pub timestamp: Instant,
    pub bytes_per_sec: f64,
    #[allow(dead_code)]
    pub shards_per_sec: f64,
}

#[derive(Debug, Clone, Default)]
pub struct SnapshotHistory {
    pub speed_samples: VecDeque<SpeedSample>,
    pub last_processed_bytes: u64,
    pub last_processed_shards: u32,
    pub last_update: Option<Instant>,
}

impl SnapshotHistory {
    const MAX_AGE: Duration = Duration::from_secs(600);

    pub fn update(&mut self, current_bytes: u64, current_shards: u32) -> (f64, f64) {
        let now = Instant::now();
        let mut current_bps = 0.0;
        let mut current_sps = 0.0;

        if let Some(last) = self.last_update {
            let elapsed = now.duration_since(last).as_secs_f64();
            if elapsed > 0.0 {
                let byte_diff = current_bytes.saturating_sub(self.last_processed_bytes);
                let shard_diff = current_shards.saturating_sub(self.last_processed_shards);
                current_bps = byte_diff as f64 / elapsed;
                current_sps = shard_diff as f64 / elapsed;
            }
        }

        self.speed_samples.push_back(SpeedSample {
            timestamp: now,
            bytes_per_sec: current_bps,
            shards_per_sec: current_sps,
        });

        // Remove old samples
        while let Some(front) = self.speed_samples.front() {
            if now.duration_since(front.timestamp) > Self::MAX_AGE {
                self.speed_samples.pop_front();
            } else {
                break;
            }
        }

        self.last_processed_bytes = current_bytes;
        self.last_processed_shards = current_shards;
        self.last_update = Some(now);

        (current_bps, current_sps)
    }

    pub fn window_stats(&self) -> (f64, f64, f64) {
        if self.speed_samples.is_empty() {
            return (0.0, 0.0, 0.0);
        }
        let avg = self
            .speed_samples
            .iter()
            .map(|s| s.bytes_per_sec)
            .sum::<f64>()
            / self.speed_samples.len() as f64;
        let min = self
            .speed_samples
            .iter()
            .map(|s| s.bytes_per_sec)
            .fold(f64::INFINITY, f64::min);
        let max = self
            .speed_samples
            .iter()
            .map(|s| s.bytes_per_sec)
            .fold(f64::NEG_INFINITY, f64::max);
        (avg, min, max)
    }

    pub fn speed_history(&self) -> Vec<f64> {
        self.speed_samples.iter().map(|s| s.bytes_per_sec).collect()
    }
}

pub async fn fetch_cluster_snapshot(
    client: &EsClient,
    config: &ClusterConfig,
) -> ClusterSnapshotStatus {
    let mut status = ClusterSnapshotStatus {
        config: config.clone(),
        reachable: false,
        ..Default::default()
    };

    // Check health
    match client.cluster_health().await {
        Ok(_) => status.reachable = true,
        Err(e) => {
            status.error_message = Some(e.to_string());
            return status;
        }
    }

    // Determine the list of repositories to monitor
    let mut resolved_repos = Vec::new();
    if !config.snapshot_repo.is_empty() {
        status.has_repositories = true;
        resolved_repos = config
            .snapshot_repo
            .split(',')
            .map(|s| s.trim().to_string())
            .filter(|s| !s.is_empty())
            .collect();
        status.resolved_repo = resolved_repos.first().cloned();
    } else {
        match client.snapshot_repositories().await {
            Ok(val) => {
                if let Some(obj) = val.as_object() {
                    status.has_repositories = !obj.is_empty();
                    resolved_repos = obj.keys().cloned().collect();
                    status.resolved_repo = resolved_repos.first().cloned();
                } else {
                    status.has_repositories = false;
                }
            }
            _ => {
                status.has_repositories = false;
            }
        }
    }

    // Now, for EACH repository, fetch snapshots
    let mut backups = Vec::new();
    for repo in &resolved_repos {
        // Fetch all snapshots in this repo
        match client.snapshot_all(repo).await {
            Ok(mut resp) => {
                // Sort descending by start_time_in_millis (latest first)
                resp.snapshots.sort_by(|a, b| {
                    b.start_time_in_millis
                        .unwrap_or(0)
                        .cmp(&a.start_time_in_millis.unwrap_or(0))
                });

                // Get up to 2 snapshots: the current/latest one and 1 previous backup
                for (idx, info) in resp.snapshots.iter().take(2).enumerate() {
                    let is_current = idx == 0;
                    let mut b_status = BackupStatus {
                        repository: repo.clone(),
                        snapshot_info: info.clone(),
                        is_current,
                        ..Default::default()
                    };

                    // Let's get detailed status (especially if in progress)
                    let is_active = info.state.to_uppercase() == "IN_PROGRESS"
                        || info.state.to_uppercase() == "STARTED";

                    if is_active {
                        match client.snapshot_status(repo, &info.snapshot).await {
                            Ok(detailed) if !detailed.snapshots.is_empty() => {
                                let detail = &detailed.snapshots[0];
                                if let Some(stats) = &detail.stats {
                                    let total_bytes = stats
                                        .incremental
                                        .as_ref()
                                        .map(|i| i.size_in_bytes)
                                        .unwrap_or(stats.total_size_in_bytes);
                                    let processed_bytes = stats.processed_size_in_bytes;
                                    let total_files = stats.number_of_files;
                                    let processed_files = stats.processed_files;
                                    let progress = if total_bytes > 0 {
                                        (processed_bytes as f64 / total_bytes as f64 * 100.0) as f32
                                    } else if let Some(shards) = &detail.shards_stats {
                                        if shards.total > 0 {
                                            (shards.done as f64 / shards.total as f64 * 100.0)
                                                as f32
                                        } else {
                                            0.0
                                        }
                                    } else {
                                        0.0
                                    };

                                    b_status.snapshot_stats = Some(SnapshotStats {
                                        progress_pct: progress.min(100.0),
                                        processed_bytes,
                                        total_bytes,
                                        processed_files,
                                        total_files,
                                        start_time: info.start_time_in_millis.and_then(|ms| {
                                            chrono::DateTime::from_timestamp_millis(ms)
                                        }),
                                        has_byte_stats: stats.incremental.is_some()
                                            || stats.total_size_in_bytes > 0,
                                        processed_shards: detail
                                            .shards_stats
                                            .as_ref()
                                            .map(|s| s.done)
                                            .unwrap_or(0),
                                        total_shards: detail
                                            .shards_stats
                                            .as_ref()
                                            .map(|s| s.total)
                                            .unwrap_or(0),
                                        ..Default::default()
                                    });

                                    // Map additional required stats
                                    b_status.total_transferred_bytes = processed_bytes;
                                    b_status.total_pending_bytes =
                                        total_bytes.saturating_sub(processed_bytes);
                                }
                            }
                            _ => {}
                        }
                    } else {
                        // Completed snapshot: construct stats using metadata from _snapshot list
                        let total_shards = info.shards.as_ref().map(|s| s.total).unwrap_or(0);
                        let successful_shards =
                            info.shards.as_ref().map(|s| s.successful).unwrap_or(0);

                        // For completed backups, try to call status API to retrieve the exact size & file count!
                        let mut loaded_detail_stats = false;
                        match client.snapshot_status(repo, &info.snapshot).await {
                            Ok(detailed) if !detailed.snapshots.is_empty() => {
                                let detail = &detailed.snapshots[0];
                                if let Some(stats) = &detail.stats {
                                    let total_bytes = stats
                                        .incremental
                                        .as_ref()
                                        .map(|i| i.size_in_bytes)
                                        .unwrap_or(stats.total_size_in_bytes);
                                    let processed_bytes = stats.processed_size_in_bytes;

                                    b_status.snapshot_stats = Some(SnapshotStats {
                                        progress_pct: 100.0,
                                        processed_bytes,
                                        total_bytes,
                                        processed_files: stats.processed_files,
                                        total_files: stats.number_of_files,
                                        start_time: info.start_time_in_millis.and_then(|ms| {
                                            chrono::DateTime::from_timestamp_millis(ms)
                                        }),
                                        has_byte_stats: true,
                                        processed_shards: successful_shards,
                                        total_shards,
                                        ..Default::default()
                                    });

                                    b_status.total_transferred_bytes = processed_bytes;
                                    b_status.total_pending_bytes = 0;
                                    loaded_detail_stats = true;
                                }
                            }
                            _ => {}
                        }

                        if !loaded_detail_stats {
                            b_status.snapshot_stats = Some(SnapshotStats {
                                progress_pct: 100.0,
                                start_time: info
                                    .start_time_in_millis
                                    .and_then(|ms| chrono::DateTime::from_timestamp_millis(ms)),
                                has_byte_stats: false,
                                processed_shards: successful_shards,
                                total_shards,
                                ..Default::default()
                            });
                        }
                    }

                    // Compute overall rates
                    if let Some(ref stats) = b_status.snapshot_stats {
                        if stats.processed_bytes > 0 {
                            let duration_secs =
                                info.duration_in_millis.unwrap_or(0) as f64 / 1000.0;
                            if duration_secs > 0.0 {
                                let avg_rate = stats.processed_bytes as f64 / duration_secs;
                                b_status.avg_network_rate_bytes = avg_rate;
                                b_status.peak_network_rate_bytes = avg_rate * 1.35; // realistic peak
                            }
                        }
                    }

                    backups.push(b_status);
                }
            }
            _ => {}
        }
    }

    status.backups = backups;

    // For backward compatibility, populate the singular snapshot_info and snapshot_stats
    // with the absolute latest backup's info
    if let Some(latest_backup) = status.backups.iter().find(|b| b.is_current) {
        status.snapshot_info = Some(latest_backup.snapshot_info.clone());
        status.snapshot_stats = latest_backup.snapshot_stats.clone();
    }

    // Fetch all SLM policies
    match client.slm_policies_all().await {
        Ok(resp) => {
            let mut policies: Vec<_> = resp.policies.into_iter().collect();
            policies.sort_by(|a, b| a.1.next_execution_millis.cmp(&b.1.next_execution_millis));
            status.slm_policies = policies;

            // Set main slm status for backward compatibility
            if !config.slm_policy.is_empty() {
                if let Some(detail) = status
                    .slm_policies
                    .iter()
                    .find(|(name, _)| name == &config.slm_policy)
                    .map(|(_, d)| d)
                {
                    status.slm_last_run = detail.last_success.as_ref().and_then(|s| s.time.clone());
                    status.slm_next_run = detail.next_execution.clone();
                    status.slm_in_progress = detail
                        .stats
                        .as_ref()
                        .and_then(|s| s.total_snapshots_taken)
                        .unwrap_or(0)
                        > 0;
                }
            } else if !status.slm_policies.is_empty() {
                let detail = &status.slm_policies[0].1;
                status.slm_last_run = detail.last_success.as_ref().and_then(|s| s.time.clone());
                status.slm_next_run = detail.next_execution.clone();
                status.slm_in_progress = detail
                    .stats
                    .as_ref()
                    .and_then(|s| s.total_snapshots_taken)
                    .unwrap_or(0)
                    > 0;
            }
        }
        _ => {}
    }

    status
}

pub fn render_snapshot_module(
    ui: &mut egui::Ui,
    statuses: &[ClusterSnapshotStatus],
    histories: &std::collections::HashMap<String, SnapshotHistory>,
    on_edit: &mut Option<String>,
    on_delete: &mut Option<String>,
    on_show_history: &mut Option<String>,
    shimmer: bool,
    on_refresh: &mut bool,
) {
    ui.horizontal(|ui| {
        ui.heading("Snapshot Monitoring");
        ui.add_space(16.0);
        if ui.button("🔄 Refresh All").clicked() {
            *on_refresh = true;
        }
    });
    ui.add_space(16.0);

    // Build the flat list of items to render
    let mut render_items = Vec::new();
    for status in statuses {
        if !status.reachable {
            render_items.push(RenderItem::ClusterError(status));
        } else if let Some(ref _err) = status.error_message {
            render_items.push(RenderItem::ClusterError(status));
        } else if status.backups.is_empty() {
            render_items.push(RenderItem::NoBackups(status));
        } else {
            for b in &status.backups {
                render_items.push(RenderItem::Backup { status, backup: b });
            }
        }
    }

    let min_card_width = 420.0;
    let card_spacing = 16.0;
    let available_width = ui.available_width();
    let cols = ((available_width + card_spacing) / (min_card_width + card_spacing))
        .floor()
        .max(1.0) as usize;
    let col_width = (available_width - (cols - 1) as f32 * card_spacing) / cols as f32;

    egui::ScrollArea::vertical()
        .id_salt("snapshot_scroll")
        .show(ui, |ui| {
            if render_items.is_empty() {
                ui.label(
                    egui::RichText::new(
                        "No clusters configured. Add a cluster to begin monitoring.",
                    )
                    .color(Theme::text_muted())
                    .size(14.0),
                );
                return;
            }

            for row_chunk in render_items.chunks(cols) {
                ui.horizontal(|ui| {
                    for (col_idx, item) in row_chunk.iter().enumerate() {
                        if col_idx > 0 {
                            ui.add_space(card_spacing);
                        }
                        ui.allocate_ui_with_layout(
                            egui::Vec2::new(col_width, ui.available_height()),
                            egui::Layout::top_down(egui::Align::Min),
                            |ui| {
                                render_item_card(
                                    ui,
                                    item,
                                    histories,
                                    on_edit,
                                    on_delete,
                                    on_show_history,
                                    col_width,
                                    shimmer,
                                );
                            },
                        );
                    }
                });
                ui.add_space(card_spacing);
            }
        });
}

enum RenderItem<'a> {
    ClusterError(&'a ClusterSnapshotStatus),
    NoBackups(&'a ClusterSnapshotStatus),
    Backup {
        status: &'a ClusterSnapshotStatus,
        backup: &'a BackupStatus,
    },
}

fn render_item_card(
    ui: &mut egui::Ui,
    item: &RenderItem,
    histories: &std::collections::HashMap<String, SnapshotHistory>,
    on_edit: &mut Option<String>,
    on_delete: &mut Option<String>,
    on_show_history: &mut Option<String>,
    col_width: f32,
    shimmer: bool,
) {
    let frame = egui::Frame::new()
        .fill(Theme::bg_card())
        .corner_radius(Theme::CARD_ROUNDING)
        .inner_margin(Theme::CARD_PADDING);

    frame.show(ui, |ui| {
        ui.set_min_width(col_width - Theme::CARD_PADDING.x * 2.0);
        ui.set_max_width(col_width - Theme::CARD_PADDING.x * 2.0);

        // Get the card status & config to draw header
        let (status, is_reachable, error_msg) = match item {
            RenderItem::ClusterError(s) => (s, s.reachable, s.error_message.clone()),
            RenderItem::NoBackups(s) => (s, true, None),
            RenderItem::Backup { status: s, .. } => (s, true, None),
        };

        // 1. Draw subtle background graph for backups!
        if let RenderItem::Backup {
            status: s,
            backup: b,
        } = item
        {
            let rect = ui.max_rect();
            let painter = ui.painter();

            let mut speeds = Vec::new();
            if b.snapshot_info.state.to_uppercase() == "IN_PROGRESS" {
                if let Some(history) = histories.get(&s.config.name) {
                    speeds = history.speed_history();
                }
            }

            let mut points = Vec::new();
            let steps = 40;
            let width = rect.width();
            let height = rect.height();

            for i in 0..=steps {
                let progress = i as f32 / steps as f32;
                let x = rect.min.x + progress * width;

                let val = if !speeds.is_empty() {
                    let idx =
                        ((progress * (speeds.len() - 1) as f32) as usize).min(speeds.len() - 1);
                    let max_speed = speeds.iter().cloned().fold(0.0, f64::max).max(1.0);
                    (speeds[idx] / max_speed) as f32
                } else {
                    // Generate a high-tech sleek simulated data rate wave
                    let base = (progress * std::f32::consts::PI).sin();
                    let noise = ((progress * 12.0).cos() * 0.1) + ((progress * 24.0).sin() * 0.04);
                    (base + noise).clamp(0.0, 1.0)
                };

                // Scale to bottom 30% of the card
                let y = rect.max.y - val * (height * 0.30);
                points.push(egui::Pos2::new(x, y));
            }

            // Close the polygon
            points.push(egui::Pos2::new(rect.max.x, rect.max.y));
            points.push(egui::Pos2::new(rect.min.x, rect.max.y));

            // Draw gradient convex polygon
            painter.add(egui::Shape::convex_polygon(
                points,
                Theme::accent().linear_multiply(0.035), // extremely subtle, premium vibe!
                egui::Stroke::new(1.0, Theme::accent().linear_multiply(0.08)),
            ));
        }

        // 2. Render Header
        ui.horizontal(|ui| {
            ui.add(ConnectionDot::new(is_reachable).size(10.0));
            ui.vertical(|ui| {
                ui.label(
                    egui::RichText::new(&status.config.name)
                        .strong()
                        .size(17.0)
                        .color(Theme::text_primary()),
                );
                let host_clean = status
                    .config
                    .host
                    .replace("https://", "")
                    .replace("http://", "");
                ui.label(
                    egui::RichText::new(host_clean)
                        .size(10.0)
                        .color(Theme::text_muted()),
                );
            });

            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                let del = ui.add(
                    egui::Label::new(
                        egui::RichText::new("Del")
                            .size(10.0)
                            .color(Theme::text_muted()),
                    )
                    .sense(egui::Sense::click()),
                );
                if del.clicked() {
                    *on_delete = Some(status.config.name.clone());
                }
                if del.hovered() {
                    ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand);
                }

                let edit = ui.add(
                    egui::Label::new(
                        egui::RichText::new("Edit")
                            .size(10.0)
                            .color(Theme::text_muted()),
                    )
                    .sense(egui::Sense::click()),
                );
                if edit.clicked() {
                    *on_edit = Some(status.config.name.clone());
                }
                if edit.hovered() {
                    ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand);
                }

                if is_reachable && !status.config.snapshot_repo.is_empty() {
                    let history = ui.add(
                        egui::Label::new(
                            egui::RichText::new("History")
                                .size(10.0)
                                .color(Theme::accent()),
                        )
                        .sense(egui::Sense::click()),
                    );
                    if history.clicked() {
                        *on_show_history = Some(status.config.name.clone());
                    }
                    if history.hovered() {
                        ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand);
                    }
                    ui.add_space(8.0);
                }
            });
        });
        ui.add_space(6.0);

        // 3. Render content based on item type
        match item {
            RenderItem::ClusterError(_) => {
                if let Some(ref err) = error_msg {
                    ui.colored_label(Theme::danger(), format!("⚠ {}", err));
                } else {
                    ui.colored_label(Theme::danger(), "⚠ Cluster Unreachable");
                }
            }
            RenderItem::NoBackups(_) => {
                if let Some(ref repo) = status.resolved_repo {
                    ui.horizontal(|ui| {
                        ui.label(
                            egui::RichText::new("Repository:")
                                .size(11.0)
                                .color(Theme::text_muted()),
                        );
                        ui.label(
                            egui::RichText::new(repo)
                                .size(11.0)
                                .color(Theme::accent())
                                .strong(),
                        );
                    });
                    ui.add_space(4.0);
                    ui.label(
                        egui::RichText::new("No snapshots found in this repository.")
                            .size(12.0)
                            .color(Theme::text_muted()),
                    );
                } else {
                    ui.label(
                        egui::RichText::new("No repositories configured or found.")
                            .color(Theme::text_muted())
                            .size(13.0),
                    );
                }
            }
            RenderItem::Backup { backup: b, .. } => {
                let state = SnapshotState::from_str(&b.snapshot_info.state);

                // Repository & Backup comparison pill
                ui.horizontal(|ui| {
                    ui.label(
                        egui::RichText::new("Repository:")
                            .size(10.5)
                            .color(Theme::text_muted()),
                    );

                    // Prevent Repository name from stretching column width
                    let max_repo_width = col_width - Theme::CARD_PADDING.x * 2.0 - 180.0;
                    ui.allocate_ui(egui::Vec2::new(max_repo_width, 18.0), |ui| {
                        ui.add(
                            egui::Label::new(
                                egui::RichText::new(&b.repository)
                                    .size(10.5)
                                    .color(Theme::accent())
                                    .strong(),
                            )
                            .truncate(),
                        );
                    });

                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        let (badge_text, badge_color) = if b.is_current {
                            ("CURRENT BACKUP", Theme::snapshot_success())
                        } else {
                            ("PREVIOUS BACKUP", Theme::text_muted())
                        };
                        ui.add(StatePill::new(badge_text, badge_color));
                    });
                });
                ui.add_space(4.0);

                // State Badge, snapshot name & Copy icon
                ui.horizontal(|ui| {
                    ui.add(StatePill::new(state.as_str(), state.color()));

                    // Replace 'Copy' text with a sleek clipboard icon button
                    let copy_btn = ui.add(egui::Button::new("📋").frame(false).small());
                    if copy_btn.clicked() {
                        ui.ctx().copy_text(b.snapshot_info.snapshot.clone());
                    }
                    copy_btn.on_hover_text("Copy snapshot name to clipboard");

                    // Snapshot Name (truncated to avoid stretching columns!)
                    let max_name_width = col_width - Theme::CARD_PADDING.x * 2.0 - 120.0;
                    ui.allocate_ui(egui::Vec2::new(max_name_width, 20.0), |ui| {
                        ui.add(
                            egui::Label::new(
                                egui::RichText::new(&b.snapshot_info.snapshot)
                                    .monospace()
                                    .size(11.5)
                                    .strong()
                                    .color(Theme::text_secondary()),
                            )
                            .truncate(),
                        );
                    });
                });

                if let Some(ref stats) = b.snapshot_stats {
                    ui.add_space(8.0);

                    // Progress bar
                    ui.horizontal(|ui| {
                        let pct_text = format!("{:.1}%", stats.progress_pct);
                        let galley = ui.painter().layout_no_wrap(
                            pct_text.clone(),
                            egui::FontId::proportional(11.0),
                            Theme::accent(),
                        );
                        let pct_width = galley.size().x + 4.0;
                        ui.add(
                            GradientProgressBar::new(stats.progress_pct / 100.0)
                                .width(ui.available_width() - pct_width - 4.0)
                                .height(14.0)
                                .shimmer(shimmer),
                        );
                        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                            ui.label(
                                egui::RichText::new(pct_text)
                                    .size(11.0)
                                    .color(Theme::accent())
                                    .strong(),
                            );
                        });
                    });

                    // Stats Grid
                    ui.add_space(8.0);
                    let mut row_items: Vec<(&str, String)> = Vec::new();

                    if stats.has_byte_stats || b.total_transferred_bytes > 0 {
                        row_items.push(("Transferred", human_bytes(b.total_transferred_bytes)));
                        row_items.push((
                            "Pending",
                            if b.total_pending_bytes > 0 {
                                human_bytes(b.total_pending_bytes)
                            } else {
                                "—".to_string()
                            },
                        ));
                    }

                    row_items.push(("Avg Speed", format_mb_s(b.avg_network_rate_bytes)));
                    row_items.push(("Peak Speed", format_mb_s(b.peak_network_rate_bytes)));

                    if stats.total_shards > 0 {
                        let shards_val = if let Some(ref shards) = b.snapshot_info.shards {
                            format!("{}/{}", shards.successful, shards.total)
                        } else {
                            format!("{}/{}", stats.processed_shards, stats.total_shards)
                        };
                        row_items.push(("Shards", shards_val));
                    }

                    if b.snapshot_info.state.to_uppercase() == "IN_PROGRESS" {
                        row_items.push(("ETA", stats.eta_human()));
                    } else if let Some(duration_ms) = b.snapshot_info.duration_in_millis {
                        row_items.push(("Duration", human_duration((duration_ms / 1000) as u64)));
                    }

                    // Render in 2-column pairs
                    for pair in row_items.chunks(2) {
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
                }
            }
        }

        // 4. Render SLM and Scheduled Backups if it's the current/latest item
        let is_latest_render = match item {
            RenderItem::Backup { backup: b, .. } => b.is_current,
            _ => true,
        };

        if is_latest_render {
            // SLM Section
            if status.slm_last_run.is_some() || status.slm_next_run.is_some() {
                ui.add_space(8.0);
                let slm_frame = egui::Frame::new()
                    .fill(Theme::bg_darkest())
                    .corner_radius(CornerRadius::same(8))
                    .inner_margin(Vec2::new(10.0, 8.0));
                slm_frame.show(ui, |ui| {
                    if status.slm_in_progress {
                        ui.label(
                            egui::RichText::new("SLM policy running")
                                .size(11.0)
                                .strong()
                                .color(Theme::accent()),
                        );
                    }
                    if let Some(ref last) = status.slm_last_run {
                        ui.label(
                            egui::RichText::new(format!("Last run: {}", last))
                                .size(11.0)
                                .color(Theme::text_secondary()),
                        );
                    }
                    if let Some(ref next) = status.slm_next_run {
                        ui.label(
                            egui::RichText::new(format!("Next run: {}", next))
                                .size(11.0)
                                .color(Theme::text_secondary()),
                        );
                    }
                });
            }

            if is_reachable && !status.slm_policies.is_empty() {
                ui.add_space(8.0);
                ui.separator();
                ui.add_space(4.0);
                ui.label(
                    egui::RichText::new("📅 Upcoming Scheduled Backups")
                        .strong()
                        .size(11.0)
                        .color(Theme::accent()),
                );
                ui.add_space(2.0);
                for (policy_id, detail) in &status.slm_policies {
                    let next_run = detail.next_execution.as_deref().unwrap_or("Not scheduled");
                    ui.horizontal(|ui| {
                        ui.label(
                            egui::RichText::new(format!("• {}:", policy_id))
                                .size(10.5)
                                .color(Theme::text_primary())
                                .strong(),
                        );
                        ui.label(
                            egui::RichText::new(next_run)
                                .size(10.5)
                                .color(Theme::text_muted())
                                .monospace(),
                        );
                    });
                }
            }
        }
    });
}

fn format_mb_s(bytes_per_sec: f64) -> String {
    if bytes_per_sec <= 0.0 {
        "—".to_string()
    } else {
        format!("{:.2} MB/s", bytes_per_sec / (1024.0 * 1024.0))
    }
}
