use std::collections::VecDeque;

use std::time::{Duration, Instant};

use egui::{CornerRadius, Vec2};
use crate::core::config::ClusterConfig;
use crate::core::es_client::{EsClient, SnapshotInfo};
use crate::ui::theme::Theme;
use crate::ui::widgets::{human_bytes, human_duration, human_speed, ConnectionDot, GradientProgressBar, MiniSparkline, StatePill};

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
            Self::Success => Theme::SNAPSHOT_SUCCESS,
            Self::InProgress => Theme::SNAPSHOT_IN_PROGRESS,
            Self::Failed => Theme::SNAPSHOT_FAILED,
            Self::Partial => Theme::SNAPSHOT_PARTIAL,
            _ => Theme::TEXT_MUTED,
        }
    }
}

#[derive(Debug, Clone, Default)]
pub struct SnapshotStats {
    pub progress_pct: f32,
    pub processed_bytes: u64,
    pub total_bytes: u64,
    pub processed_files: u32,
    pub total_files: u32,
    pub start_time: Option<Instant>,
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

#[derive(Debug, Clone, Default)]
pub struct ClusterSnapshotStatus {
    pub config: ClusterConfig,
    pub reachable: bool,
    pub error_message: Option<String>,
    pub snapshot_info: Option<SnapshotInfo>,
    pub snapshot_stats: Option<SnapshotStats>,
    pub slm_last_run: Option<String>,
    pub slm_next_run: Option<String>,
    pub slm_in_progress: bool,
}

#[derive(Debug, Clone)]
pub struct SpeedSample {
    pub timestamp: Instant,
    pub bytes_per_sec: f64,
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
        let avg = self.speed_samples.iter().map(|s| s.bytes_per_sec).sum::<f64>() / self.speed_samples.len() as f64;
        let min = self.speed_samples.iter().map(|s| s.bytes_per_sec).fold(f64::INFINITY, f64::min);
        let max = self.speed_samples.iter().map(|s| s.bytes_per_sec).fold(f64::NEG_INFINITY, f64::max);
        (avg, min, max)
    }

    pub fn speed_history(&self) -> Vec<f64> {
        self.speed_samples.iter().map(|s| s.bytes_per_sec).collect()
    }
}

pub async fn fetch_cluster_snapshot(client: &EsClient, config: &ClusterConfig) -> ClusterSnapshotStatus {
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

    // Try snapshot current
    let mut snapshot_info: Option<SnapshotInfo> = None;
    if !config.snapshot_repo.is_empty() {
        let repo = &config.snapshot_repo;
        match client.snapshot_current(repo).await {
            Ok(resp) if !resp.snapshots.is_empty() => {
                snapshot_info = Some(resp.snapshots.into_iter().next().unwrap());
            }
            _ => {}
        }
    }

    // Fallback: snapshot status all
    if snapshot_info.is_none() {
        match client.snapshot_status_all().await {
            Ok(resp) if !resp.snapshots.is_empty() => {
                let info = &resp.snapshots[0];
                snapshot_info = Some(SnapshotInfo {
                    snapshot: info.snapshot.clone(),
                    uuid: info.uuid.clone(),
                    repository: info.repository.clone(),
                    state: info.state.clone(),
                    start_time: None,
                    start_time_in_millis: None,
                    end_time: None,
                    end_time_in_millis: None,
                    duration_in_millis: None,
                    indices: None,
                    shards: info.shards_stats.as_ref().map(|s| crate::core::es_client::ShardStats {
                        total: s.total,
                        failed: s.failed,
                        successful: s.done,
                    }),
                    failures: None,
                });
            }
            _ => {}
        }
    }

    // Get detailed status if in progress
    if let Some(ref info) = snapshot_info {
        if info.state.to_uppercase() == "IN_PROGRESS" || info.state.to_uppercase() == "STARTED" {
            if !config.snapshot_repo.is_empty() {
                let repo = &config.snapshot_repo;
                match client.snapshot_status(repo, &info.snapshot).await {
                    Ok(detailed) if !detailed.snapshots.is_empty() => {
                        let detail = &detailed.snapshots[0];
                        if let Some(stats) = &detail.stats {
                            let total_bytes = stats.incremental.as_ref().map(|i| i.size_in_bytes).unwrap_or(stats.total_size_in_bytes);
                            let processed_bytes = stats.processed_size_in_bytes;
                            let total_files = stats.number_of_files;
                            let processed_files = stats.processed_files;
                            let progress = if total_bytes > 0 {
                                (processed_bytes as f64 / total_bytes as f64 * 100.0) as f32
                            } else if let Some(shards) = &detail.shards_stats {
                                if shards.total > 0 {
                                    (shards.done as f64 / shards.total as f64 * 100.0) as f32
                                } else {
                                    0.0
                                }
                            } else {
                                0.0
                            };

                            status.snapshot_stats = Some(SnapshotStats {
                                progress_pct: progress.min(100.0),
                                processed_bytes,
                                total_bytes,
                                processed_files,
                                total_files,
                                start_time: info.start_time_in_millis.map(|ms| Instant::now() - Duration::from_millis(ms as u64)),
                                has_byte_stats: stats.incremental.is_some() || stats.total_size_in_bytes > 0,
                                processed_shards: detail.shards_stats.as_ref().map(|s| s.done).unwrap_or(0),
                                total_shards: detail.shards_stats.as_ref().map(|s| s.total).unwrap_or(0),
                                ..Default::default()
                            });
                        }
                    }
                    _ => {}
                }
            }
        }
    }

    status.snapshot_info = snapshot_info;

    // SLM policy
    if !config.slm_policy.is_empty() {
        let policy = &config.slm_policy;
        match client.slm_policy(policy).await {
            Ok(resp) => {
                if let Some((_, detail)) = resp.policies.into_iter().next() {
                    status.slm_last_run = detail.last_success.and_then(|s| s.time);
                    status.slm_next_run = detail.next_execution;
                    status.slm_in_progress = detail.stats.as_ref().and_then(|s| s.total_snapshots_taken).unwrap_or(0) > 0;
                }
            }
            _ => {}
        }
    }

    status
}

pub fn render_snapshot_module(
    ui: &mut egui::Ui,
    statuses: &[ClusterSnapshotStatus],
    histories: &std::collections::HashMap<String, SnapshotHistory>,
    on_edit: &mut Option<String>,
    on_delete: &mut Option<String>,
) {
    ui.heading("Snapshot Monitoring");
    ui.add_space(16.0);

    egui::ScrollArea::vertical().show(ui, |ui| {
        ui.horizontal_wrapped(|ui| {
            for status in statuses {
                render_cluster_card(ui, status, histories, on_edit, on_delete);
                ui.add_space(12.0);
            }
        });
    });
}

fn render_cluster_card(
    ui: &mut egui::Ui,
    status: &ClusterSnapshotStatus,
    histories: &std::collections::HashMap<String, SnapshotHistory>,
    on_edit: &mut Option<String>,
    on_delete: &mut Option<String>,
) {
    let card_width = 380.0;
    let frame = egui::Frame::new()
        .fill(Theme::BG_CARD)
        .corner_radius(Theme::CARD_ROUNDING)
        .inner_margin(Theme::CARD_PADDING);

    frame.show(ui, |ui| {
        ui.set_min_width(card_width);
        ui.set_max_width(card_width);

        // Header
        ui.horizontal(|ui| {
            ui.add(ConnectionDot::new(status.reachable).size(10.0));
            ui.label(egui::RichText::new(&status.config.name).strong().size(16.0).color(Theme::TEXT_PRIMARY));
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                if ui.small_button("🗑").clicked() {
                    *on_delete = Some(status.config.name.clone());
                }
                if ui.small_button("✏").clicked() {
                    *on_edit = Some(status.config.name.clone());
                }
            });
        });

        ui.label(egui::RichText::new(&status.config.host).size(11.0).color(Theme::TEXT_MUTED));
        ui.add_space(8.0);

        if let Some(ref err) = status.error_message {
            ui.colored_label(Theme::DANGER, format!("Error: {}", err));
        } else if let Some(ref info) = status.snapshot_info {
            let state = SnapshotState::from_str(&info.state);
            ui.horizontal(|ui| {
                ui.add(StatePill::new(state.as_str(), state.color()));
                ui.label(egui::RichText::new(&info.snapshot).monospace().size(11.0).color(Theme::TEXT_SECONDARY));
            });

            if let Some(ref stats) = status.snapshot_stats {
                ui.add_space(8.0);
                ui.add(GradientProgressBar::new(stats.progress_pct / 100.0).width(card_width - 32.0));
                ui.add_space(4.0);
                ui.horizontal(|ui| {
                    ui.label(egui::RichText::new(format!("{:.1}%", stats.progress_pct)).size(11.0).color(Theme::TEXT_SECONDARY));
                });

                ui.add_space(8.0);
                egui::Grid::new(format!("stats_{}", status.config.name))
                    .num_columns(2)
                    .spacing([40.0, 4.0])
                    .show(ui, |ui| {
                        ui.label(egui::RichText::new("Data:").color(Theme::TEXT_MUTED).size(11.0));
                        ui.label(egui::RichText::new(format!("{} / {}", stats.processed_human(), stats.total_human())).color(Theme::TEXT_PRIMARY).size(11.0));
                        ui.end_row();

                        ui.label(egui::RichText::new("Files:").color(Theme::TEXT_MUTED).size(11.0));
                        ui.label(egui::RichText::new(format!("{} / {}", stats.processed_files, stats.total_files)).color(Theme::TEXT_PRIMARY).size(11.0));
                        ui.end_row();

                        if stats.total_shards > 0 {
                            ui.label(egui::RichText::new("Shards:").color(Theme::TEXT_MUTED).size(11.0));
                            ui.label(egui::RichText::new(format!("{} / {}", stats.processed_shards, stats.total_shards)).color(Theme::TEXT_PRIMARY).size(11.0));
                            ui.end_row();
                        }

                        ui.label(egui::RichText::new("ETA:").color(Theme::TEXT_MUTED).size(11.0));
                        ui.label(egui::RichText::new(stats.eta_human()).color(Theme::TEXT_PRIMARY).size(11.0));
                        ui.end_row();

                        ui.label(egui::RichText::new("Speed:").color(Theme::TEXT_MUTED).size(11.0));
                        ui.label(egui::RichText::new(stats.current_speed_human()).color(Theme::TEXT_PRIMARY).size(11.0));
                        ui.end_row();

                        ui.label(egui::RichText::new("Avg:").color(Theme::TEXT_MUTED).size(11.0));
                        ui.label(egui::RichText::new(stats.avg_speed_human()).color(Theme::TEXT_PRIMARY).size(11.0));
                        ui.end_row();

                        if let Some(ref shards) = info.shards {
                            ui.label(egui::RichText::new("Failed:").color(Theme::TEXT_MUTED).size(11.0));
                            ui.label(egui::RichText::new(shards.failed.to_string()).color(Theme::DANGER).size(11.0));
                            ui.end_row();
                        }
                    });

                // Sparkline
                if let Some(history) = histories.get(&status.config.name) {
                    let speeds = history.speed_history();
                    if speeds.len() >= 2 {
                        ui.add_space(8.0);
                        ui.add(MiniSparkline::new(speeds).width(card_width - 32.0).height(40.0));
                    }
                }
            } else {
                ui.label(egui::RichText::new("No active snapshot").color(Theme::TEXT_MUTED).size(12.0));
            }

            // SLM info
            if status.slm_last_run.is_some() || status.slm_next_run.is_some() {
                ui.add_space(8.0);
                let slm_frame = egui::Frame::new()
                    .fill(Theme::BG_DARK)
                    .corner_radius(CornerRadius::same(6))
                    .inner_margin(Vec2::new(8.0, 6.0));
                slm_frame.show(ui, |ui| {
                    ui.horizontal(|ui| {
                        ui.label(egui::RichText::new("SLM:").size(10.0).color(Theme::TEXT_MUTED));
                        if let Some(ref last) = status.slm_last_run {
                            ui.label(egui::RichText::new(format!("Last: {}", last)).size(10.0).color(Theme::TEXT_SECONDARY));
                        }
                        if let Some(ref next) = status.slm_next_run {
                            ui.label(egui::RichText::new(format!("Next: {}", next)).size(10.0).color(Theme::TEXT_SECONDARY));
                        }
                        if status.slm_in_progress {
                            ui.add(StatePill::new("RUNNING", Theme::SNAPSHOT_IN_PROGRESS));
                        }
                    });
                });
            }
        } else {
            ui.label(egui::RichText::new("No snapshot information available").color(Theme::TEXT_MUTED).size(12.0));
        }
    });
}
