use std::collections::VecDeque;

use std::time::{Duration, Instant};

use crate::core::config::ClusterConfig;
use crate::core::es_client::{EsClient, SnapshotInfo};
use crate::ui::theme::Theme;
use crate::ui::widgets::{
    ConnectionDot, GradientProgressBar, MiniSparkline, StatePill, human_bytes, human_duration,
    human_speed,
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
                    shards: info.shards_stats.as_ref().map(|s| {
                        crate::core::es_client::ShardStats {
                            total: s.total,
                            failed: s.failed,
                            successful: s.done,
                        }
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
                                start_time: info
                                    .start_time_in_millis
                                    .and_then(|ms| chrono::DateTime::from_timestamp_millis(ms)),
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
    }

    status
}

pub fn render_snapshot_module(
    ui: &mut egui::Ui,
    statuses: &[ClusterSnapshotStatus],
    histories: &std::collections::HashMap<String, SnapshotHistory>,
    on_edit: &mut Option<String>,
    on_delete: &mut Option<String>,
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

    let min_card_width = 420.0;
    let card_spacing = 16.0;
    let available_width = ui.available_width();
    let cols = if available_width >= min_card_width * 2.0 + card_spacing {
        2
    } else {
        1
    };
    let col_width = (available_width - (cols - 1) as f32 * card_spacing) / cols as f32;

    egui::ScrollArea::vertical()
        .id_salt("snapshot")
        .show(ui, |ui| {
        if statuses.is_empty() {
            ui.label(
                egui::RichText::new("No clusters configured. Add a cluster to begin monitoring.")
                    .color(Theme::text_muted())
                    .size(14.0),
            );
            return;
        }

        // Responsive columns: cards fill vertically within each column
        ui.horizontal(|ui| {
            for col in 0..cols {
                let col_idx = col;
                ui.allocate_ui_with_layout(
                    egui::Vec2::new(col_width, ui.available_height()),
                    egui::Layout::top_down(egui::Align::Min),
                    |ui| {
                        for (i, status) in statuses.iter().enumerate() {
                            if i % cols == col_idx {
                                render_cluster_card(
                                    ui, status, histories, on_edit, on_delete, col_width, shimmer,
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

fn render_cluster_card(
    ui: &mut egui::Ui,
    status: &ClusterSnapshotStatus,
    histories: &std::collections::HashMap<String, SnapshotHistory>,
    on_edit: &mut Option<String>,
    on_delete: &mut Option<String>,
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

        // ── Header ──
        ui.horizontal(|ui| {
            ui.add(ConnectionDot::new(status.reachable).size(10.0));
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
            });
        });
        ui.add_space(6.0);

        if let Some(ref err) = status.error_message {
            ui.colored_label(Theme::danger(), format!("⚠ {}", err));
        } else if let Some(ref info) = status.snapshot_info {
            let state = SnapshotState::from_str(&info.state);

            // ── State badge + snapshot name + copy ──
            ui.horizontal(|ui| {
                ui.add(StatePill::new(state.as_str(), state.color()));
                ui.label(
                    egui::RichText::new(&info.snapshot)
                        .monospace()
                        .size(12.0)
                        .strong()
                        .color(Theme::text_secondary()),
                );
                if ui
                    .add(
                        egui::Label::new(
                            egui::RichText::new("Copy")
                                .size(10.0)
                                .color(Theme::text_muted()),
                        )
                        .sense(egui::Sense::click()),
                    )
                    .clicked()
                {
                    ui.ctx().copy_text(info.snapshot.clone());
                }
            });

            if let Some(ref stats) = status.snapshot_stats {
                ui.add_space(8.0);

                // ── Progress bar + percentage ──
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

                // ── Sparkline ──
                if let Some(history) = histories.get(&status.config.name) {
                    let speeds = history.speed_history();
                    if speeds.len() >= 2 {
                        ui.add_space(6.0);
                        ui.add(
                            MiniSparkline::new(speeds)
                                .width(ui.available_width())
                                .height(50.0),
                        );
                    }
                }

                // ── Stats grid (2 columns) ──
                ui.add_space(8.0);
                let mut row_items: Vec<(&str, String)> = Vec::new();

                if stats.has_byte_stats {
                    row_items.push((
                        "Data",
                        format!("{} / {}", stats.processed_human(), stats.total_human()),
                    ));
                    row_items.push((
                        "Files",
                        format!("{} / {}", stats.processed_files, stats.total_files),
                    ));
                }

                if stats.total_shards > 0 {
                    let shards_val = if let Some(ref shards) = info.shards {
                        format!("{}/{}", shards.successful, shards.total)
                    } else {
                        format!("{} / {}", stats.processed_shards, stats.total_shards)
                    };
                    row_items.push(("Shards", shards_val));
                }

                let eta_label = if stats.has_byte_stats {
                    "ETA"
                } else {
                    "ETA (est.)"
                };
                row_items.push((eta_label, stats.eta_human()));
                row_items.push(("Speed", stats.current_speed_human()));
                row_items.push(("Avg", stats.avg_speed_human()));

                if let Some(ref shards) = info.shards {
                    if shards.failed > 0 {
                        row_items.push(("Failed", shards.failed.to_string()));
                    }
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
                                    let color = if *label == "Failed" {
                                        Theme::danger()
                                    } else {
                                        Theme::text_primary()
                                    };
                                    ui.label(
                                        egui::RichText::new(format!("{}: ", label))
                                            .color(Theme::text_muted())
                                            .size(11.0),
                                    );
                                    ui.label(
                                        egui::RichText::new(value.clone())
                                            .color(color)
                                            .size(11.0)
                                            .strong(),
                                    );
                                },
                            );
                        }
                    });
                }
            } else {
                ui.add_space(8.0);
                ui.label(
                    egui::RichText::new("No active snapshot")
                        .color(Theme::text_muted())
                        .size(13.0),
                );
            }

            // ── SLM section ──
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
        } else if status.reachable {
            ui.add_space(8.0);
            ui.label(
                egui::RichText::new("No snapshot in progress")
                    .color(Theme::text_muted())
                    .size(13.0),
            );
        } else {
            ui.label(
                egui::RichText::new("Cluster unreachable")
                    .color(Theme::text_muted())
                    .size(12.0),
            );
        }
    });
}
