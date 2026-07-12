use slint::{ComponentHandle, Model, ModelRc, VecModel};
use std::rc::Rc;
use std::sync::mpsc;
use std::time::Duration;

use crate::core::cluster_manager::ClusterManager;
use crate::modules::snapshot::{BackupStatus, ClusterSnapshotStatus, SnapshotState, fetch_cluster_snapshot};
use crate::ui::widgets::human_bytes;

use super::{AppShell, BackupRow, ClusterItem, SlmPolicyRow};

struct SnapshotResult {
    cluster_name: String,
    status: ClusterSnapshotStatus,
}

/// Wires the Snapshot screen: active-cluster-scoped backup/SLM monitoring,
/// replacing the old egui Snapshot tab's all-clusters card grid, per the
/// redesign's single-cluster-scoped rule (see theplan.md).
///
/// Reuses `fetch_cluster_snapshot` verbatim — it's already fully
/// UI-agnostic (no egui types), so the repo-resolution/backup-comparison/
/// SLM-policy orchestration logic isn't duplicated here.
///
/// Known simplification vs. the egui version: no live in-progress-snapshot
/// speed graph / rolling speed history (SnapshotHistory's sampling window)
/// and no continuous auto-refresh loop — this fetches once on cluster
/// switch or an explicit Refresh click, matching the pattern used by the
/// other single-cluster screens in this retrofit.
pub fn wire(app: &AppShell, manager: &ClusterManager, clusters: &Rc<VecModel<ClusterItem>>) {
    let (tx, rx) = mpsc::channel::<SnapshotResult>();

    let initial_cluster = current_focused_names(clusters).first().cloned().unwrap_or_default();
    app.set_snapshot_active_cluster(initial_cluster.clone().into());
    if !initial_cluster.is_empty() {
        spawn_fetch(manager, &initial_cluster, tx.clone());
    }

    let manager_for_change = manager.clone();
    let tx_for_change = tx.clone();
    let app_weak = app.as_weak();
    app.on_snapshot_active_cluster_changed(move |name| {
        let Some(app) = app_weak.upgrade() else { return };
        app.set_snapshot_active_cluster(name.clone());
        app.set_snapshot_error("".into());
        app.set_snapshot_is_loading(true);
        if !name.is_empty() {
            spawn_fetch(&manager_for_change, &name, tx_for_change.clone());
        }
    });

    let manager_for_refresh = manager.clone();
    let tx_for_refresh = tx.clone();
    let app_weak = app.as_weak();
    app.on_snapshot_refresh_clicked(move || {
        let Some(app) = app_weak.upgrade() else { return };
        let cluster = app.get_snapshot_active_cluster().to_string();
        if !cluster.is_empty() {
            app.set_snapshot_is_loading(true);
            spawn_fetch(&manager_for_refresh, &cluster, tx_for_refresh.clone());
        }
    });

    let app_weak = app.as_weak();
    let timer = slint::Timer::default();
    timer.start(slint::TimerMode::Repeated, Duration::from_millis(150), move || {
        let Some(app) = app_weak.upgrade() else { return };
        while let Ok(result) = rx.try_recv() {
            if result.cluster_name != app.get_snapshot_active_cluster().as_str() {
                continue;
            }
            app.set_snapshot_is_loading(false);
            apply_status(&app, result.status);
        }
    });
    std::mem::forget(timer);
}

fn spawn_fetch(manager: &ClusterManager, cluster_name: &str, tx: mpsc::Sender<SnapshotResult>) {
    let Some(client) = manager.get_client(cluster_name) else {
        return;
    };
    let Some(config) = manager.clusters().into_iter().find(|c| c.name == cluster_name) else {
        return;
    };
    let cluster_name = cluster_name.to_string();

    tokio::spawn(async move {
        let status = fetch_cluster_snapshot(&client, &config).await;
        let _ = tx.send(SnapshotResult { cluster_name, status });
    });
}

fn apply_status(app: &AppShell, status: ClusterSnapshotStatus) {
    if !status.reachable {
        app.set_snapshot_error(
            status
                .error_message
                .unwrap_or_else(|| "Cluster unreachable".to_string())
                .into(),
        );
        app.set_snapshot_backups(ModelRc::from(Rc::new(VecModel::from(Vec::<BackupRow>::new()))));
        app.set_snapshot_slm_policies(ModelRc::from(Rc::new(VecModel::from(Vec::<SlmPolicyRow>::new()))));
        return;
    }

    app.set_snapshot_error("".into());
    app.set_snapshot_reachable(true);
    app.set_snapshot_has_repositories(status.has_repositories);
    app.set_snapshot_resolved_repo(status.resolved_repo.unwrap_or_default().into());

    let backups: Vec<BackupRow> = status.backups.iter().map(to_backup_row).collect();
    app.set_snapshot_backups(ModelRc::from(Rc::new(VecModel::from(backups))));

    let policies: Vec<SlmPolicyRow> = status
        .slm_policies
        .iter()
        .map(|(name, detail)| SlmPolicyRow {
            name: name.clone().into(),
            last_run: detail
                .last_success
                .as_ref()
                .and_then(|s| s.time)
                .and_then(chrono::DateTime::from_timestamp_millis)
                .map(|dt| dt.format("%Y-%m-%d %H:%M UTC").to_string())
                .unwrap_or_else(|| "—".to_string())
                .into(),
            next_run: detail
                .next_execution_millis
                .and_then(chrono::DateTime::from_timestamp_millis)
                .map(|dt| dt.format("%Y-%m-%d %H:%M UTC").to_string())
                .unwrap_or_else(|| "—".to_string())
                .into(),
        })
        .collect();
    app.set_snapshot_slm_policies(ModelRc::from(Rc::new(VecModel::from(policies))));
}

fn to_backup_row(b: &BackupStatus) -> BackupRow {
    let state = SnapshotState::from_str(&b.snapshot_info.state);
    let stats = b.snapshot_stats.as_ref();

    BackupRow {
        repository: b.repository.clone().into(),
        snapshot_name: b.snapshot_info.snapshot.clone().into(),
        state: state.as_str().into(),
        is_current: b.is_current,
        progress_ratio: stats.map(|s| s.progress_pct / 100.0).unwrap_or(0.0),
        progress_label: stats
            .map(|s| format!("{:.1}%", s.progress_pct))
            .unwrap_or_else(|| "—".to_string())
            .into(),
        processed_human: stats.map(|s| human_bytes(s.processed_bytes)).unwrap_or_default().into(),
        total_human: stats.map(|s| human_bytes(s.total_bytes)).unwrap_or_default().into(),
        has_byte_stats: stats.map(|s| s.has_byte_stats).unwrap_or(false),
        processed_shards: stats.map(|s| s.processed_shards as i32).unwrap_or(0),
        total_shards: stats.map(|s| s.total_shards as i32).unwrap_or(0),
        start_time_label: stats
            .and_then(|s| s.start_time)
            .map(|dt| dt.format("%Y-%m-%d %H:%M UTC").to_string())
            .unwrap_or_else(|| "—".to_string())
            .into(),
    }
}

fn current_focused_names(clusters: &VecModel<ClusterItem>) -> Vec<String> {
    (0..clusters.row_count())
        .filter_map(|i| clusters.row_data(i))
        .filter(|c| c.checked)
        .map(|c| c.name.to_string())
        .collect()
}
