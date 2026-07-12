use slint::{Model, ModelRc, VecModel};
use std::cell::RefCell;
use std::rc::Rc;
use std::sync::mpsc;
use std::time::Duration;

use crate::core::cluster_manager::ClusterManager;
use crate::core::es_client::{ClusterHealth, ClusterStats};
use crate::ui::widgets::{human_bytes, human_docs};

slint::include_modules!();

mod console;
mod discover;
mod indices;
mod observability;
mod snapshot;
mod status;
mod tasks;

/// Result of one cluster's health+stats fetch, sent back from a tokio task
/// to the Slint UI thread. Kept as plain owned data (Send) because Slint
/// models (VecModel, ModelRc, ...) are not Send and can't be captured into a
/// tokio::spawn'd future — mirrors the `RefreshMsg` channel pattern the egui
/// app uses in app.rs.
struct FetchResult {
    cluster_name: String,
    health: Result<ClusterHealth, String>,
    stats: Result<ClusterStats, String>,
}

/// Launches the Slint shell (alert bar + sidebar + Dashboard) against the
/// real ClusterManager/EsClient data, per the egui->Slint retrofit build
/// order steps 4-5.
pub fn run() -> Result<(), slint::PlatformError> {
    let manager = ClusterManager::new();
    if let Err(e) = manager.load() {
        tracing::warn!("Failed to load config: {}", e);
    }

    let app = AppShell::new()?;
    let cluster_configs = manager.clusters();

    // No persisted "focused" selection exists yet (the old app only has a
    // free-text cluster_filter, not per-cluster checkboxes) — default every
    // configured cluster to focused/checked/visible.
    let clusters = Rc::new(VecModel::from(
        cluster_configs
            .iter()
            .map(|c| ClusterItem {
                name: c.name.clone().into(),
                checked: true,
                visible: true,
            })
            .collect::<Vec<_>>(),
    ));
    app.set_clusters(ModelRc::from(clusters.clone()));
    app.set_focused_checked_count(count_checked(&clusters));

    // The full per-cluster dataset (including unchecked ones) lives here;
    // `dashboard_view` is the checked-only subset actually bound to the UI.
    let dashboard_master: Rc<RefCell<Vec<ClusterCard>>> = Rc::new(RefCell::new(
        cluster_configs
            .iter()
            .map(|c| ClusterCard {
                name: c.name.clone().into(),
                host: c.host.clone().into(),
                connected: false,
                status: "".into(),
                error: "".into(),
                nodes: 0,
                active_shards: 0,
                unassigned_shards: 0,
                indices_count: 0,
                docs_human: "".into(),
                store_human: "".into(),
                heap_human: "".into(),
                heap_ratio: 0.0,
            })
            .collect(),
    ));
    let dashboard_view = Rc::new(VecModel::from(dashboard_master.borrow().clone()));
    app.set_dashboard_cards(ModelRc::from(dashboard_view.clone()));
    app.set_dashboard_hidden_count(0);

    let alerts: Rc<VecModel<ClusterAlert>> = Rc::new(VecModel::from(Vec::new()));
    app.set_alerts(ModelRc::from(alerts.clone()));
    app.set_alerting(false);

    console::wire(&app, &manager, &clusters);
    status::wire(&app, &manager, &clusters);
    observability::wire(&app, &manager, &clusters);
    tasks::wire(&app, &manager, &clusters);
    snapshot::wire(&app, &manager, &clusters);
    discover::wire(&app, &manager, &clusters);
    indices::wire(&app, &manager, &clusters);

    // Checkbox toggle: keep the sidebar list, the dashboard's checked-only
    // view, the Console's focused-cluster dropdown, and the "N hidden" count
    // in sync.
    let clusters_for_toggle = clusters.clone();
    let dashboard_master_for_toggle = dashboard_master.clone();
    let dashboard_view_for_toggle = dashboard_view.clone();
    let app_weak = app.as_weak();
    app.on_cluster_toggled(move |name, checked| {
        for i in 0..clusters_for_toggle.row_count() {
            if let Some(mut item) = clusters_for_toggle.row_data(i) {
                if item.name == name {
                    item.checked = checked;
                    clusters_for_toggle.set_row_data(i, item);
                }
            }
        }
        let hidden = sync_dashboard_view(
            &dashboard_master_for_toggle,
            &dashboard_view_for_toggle,
            &clusters_for_toggle,
        );
        if let Some(app) = app_weak.upgrade() {
            app.set_focused_checked_count(count_checked(&clusters_for_toggle));
            app.set_dashboard_hidden_count(hidden);
            let focused_names: Vec<slint::SharedString> = (0..clusters_for_toggle.row_count())
                .filter_map(|i| clusters_for_toggle.row_data(i))
                .filter(|c| c.checked)
                .map(|c| c.name)
                .collect();
            app.set_focused_cluster_names(ModelRc::from(Rc::new(VecModel::from(focused_names))));
        }
    });

    let clusters_for_filter = clusters.clone();
    app.on_filter_changed(move |text| {
        let needle = text.to_lowercase();
        for i in 0..clusters_for_filter.row_count() {
            if let Some(mut item) = clusters_for_filter.row_data(i) {
                item.visible = needle.is_empty() || item.name.to_lowercase().contains(&needle);
                clusters_for_filter.set_row_data(i, item);
            }
        }
    });

    // Fetch health + stats for every configured cluster concurrently. Each
    // tokio task only ever touches Send data (the EsClient and owned
    // result values); results are handed back over an mpsc channel and
    // applied to the (non-Send) Slint models by a UI-thread timer, same
    // pattern as the egui app's refresh_tx/refresh_rx in app.rs.
    let (tx, rx) = mpsc::channel::<FetchResult>();
    for cluster in cluster_configs {
        let Some(client) = manager.get_client(&cluster.name) else {
            continue;
        };
        let tx = tx.clone();
        let cluster_name = cluster.name.clone();

        tokio::spawn(async move {
            // Concurrent, not sequential: chaining two 10s-timeout calls
            // would take up to 20s to surface a fully unreachable cluster.
            let (health, stats) = tokio::join!(client.cluster_health(), client.cluster_stats());
            let _ = tx.send(FetchResult {
                cluster_name,
                health: health.map_err(|e| e.to_string()),
                stats: stats.map_err(|e| e.to_string()),
            });
        });
    }
    drop(tx);

    let timer = slint::Timer::default();
    let app_weak = app.as_weak();
    timer.start(
        slint::TimerMode::Repeated,
        Duration::from_millis(200),
        move || {
            let Some(app) = app_weak.upgrade() else {
                return;
            };
            let mut dirty = false;
            while let Ok(result) = rx.try_recv() {
                apply_fetch_result(&app, &dashboard_master, &alerts, result);
                dirty = true;
            }
            if dirty {
                let hidden = sync_dashboard_view(&dashboard_master, &dashboard_view, &clusters);
                app.set_dashboard_hidden_count(hidden);
            }
        },
    );

    app.run()
}

fn apply_fetch_result(
    app: &AppShell,
    dashboard_master: &RefCell<Vec<ClusterCard>>,
    alerts: &VecModel<ClusterAlert>,
    result: FetchResult,
) {
    let FetchResult {
        cluster_name,
        health,
        stats,
    } = result;

    let health = match health {
        Ok(h) => h,
        Err(e) => {
            tracing::warn!("Health check failed for '{}': {}", cluster_name, e);
            update_card(dashboard_master, &cluster_name, |card| {
                card.error = e.into();
            });
            return;
        }
    };

    if health.status.eq_ignore_ascii_case("red") {
        alerts.push(ClusterAlert {
            name: cluster_name.clone().into(),
            message: format!(
                "Status RED — {} unassigned shards",
                health.unassigned_shards
            )
            .into(),
        });
        app.set_alerting(true);
    }

    let (docs_human, store_human, indices_count, heap_ratio, heap_human) = match stats {
        Ok(s) => {
            let indices = s.indices.unwrap_or_default();
            let docs = indices
                .docs
                .as_ref()
                .map(|d| human_docs(d.count))
                .unwrap_or_else(|| "—".to_string());
            let store = indices
                .store
                .as_ref()
                .map(|st| human_bytes(st.size_in_bytes))
                .unwrap_or_else(|| "—".to_string());
            let (ratio, heap_label) = s
                .nodes
                .as_ref()
                .and_then(|n| n.jvm.as_ref())
                .and_then(|j| j.mem.as_ref())
                .map(|mem| {
                    let ratio = if mem.heap_max_in_bytes > 0 {
                        mem.heap_used_in_bytes as f32 / mem.heap_max_in_bytes as f32
                    } else {
                        0.0
                    };
                    let label = format!(
                        "{} / {}",
                        human_bytes(mem.heap_used_in_bytes),
                        human_bytes(mem.heap_max_in_bytes)
                    );
                    (ratio, label)
                })
                .unwrap_or((0.0, "—".to_string()));
            (docs, store, indices.count as i32, ratio, heap_label)
        }
        Err(e) => {
            tracing::warn!("Cluster stats failed for '{}': {}", cluster_name, e);
            ("—".to_string(), "—".to_string(), 0, 0.0, "—".to_string())
        }
    };

    update_card(dashboard_master, &cluster_name, |card| {
        card.connected = true;
        card.status = health.status.clone().into();
        card.error = "".into();
        card.nodes = health.number_of_nodes as i32;
        card.active_shards = health.active_shards as i32;
        card.unassigned_shards = health.unassigned_shards as i32;
        card.indices_count = indices_count;
        card.docs_human = docs_human.clone().into();
        card.store_human = store_human.clone().into();
        card.heap_human = heap_human.clone().into();
        card.heap_ratio = heap_ratio;
    });
}

fn update_card(master: &RefCell<Vec<ClusterCard>>, name: &str, f: impl FnOnce(&mut ClusterCard)) {
    if let Some(card) = master.borrow_mut().iter_mut().find(|c| c.name == name) {
        f(card);
    }
}

/// Rebuilds the UI-bound `view` from `master`, keeping only clusters that are
/// currently checked in the sidebar. Returns the number hidden.
fn sync_dashboard_view(
    master: &RefCell<Vec<ClusterCard>>,
    view: &VecModel<ClusterCard>,
    clusters: &VecModel<ClusterItem>,
) -> i32 {
    let checked_names: std::collections::HashSet<String> = (0..clusters.row_count())
        .filter_map(|i| clusters.row_data(i))
        .filter(|c| c.checked)
        .map(|c| c.name.to_string())
        .collect();

    let master = master.borrow();
    let visible: Vec<ClusterCard> = master
        .iter()
        .filter(|c| checked_names.contains(c.name.as_str()))
        .cloned()
        .collect();
    let hidden = master.len() as i32 - visible.len() as i32;
    view.set_vec(visible);
    hidden
}

fn count_checked(clusters: &VecModel<ClusterItem>) -> i32 {
    (0..clusters.row_count())
        .filter(|&i| clusters.row_data(i).is_some_and(|c| c.checked))
        .count() as i32
}
