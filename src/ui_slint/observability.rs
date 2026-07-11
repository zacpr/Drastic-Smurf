use slint::{ComponentHandle, Model, ModelRc, VecModel};
use std::cell::RefCell;
use std::collections::HashSet;
use std::rc::Rc;
use std::sync::mpsc;
use std::time::Duration;

use crate::core::cluster_manager::ClusterManager;
use crate::modules::observability::{SyntheticMonitor, parse_synthetics_monitors};

use super::{AppShell, ClusterItem, MonitorRow};

struct ObsResult {
    cluster_name: String,
    monitors: Result<Vec<SyntheticMonitor>, String>,
}

/// Wires the Observability screen: Kibana synthetic monitors for the active
/// cluster, with a Browse (filterable table + pin toggle) and Pinned
/// (static card grid) view. The egui version's pinned monitors are
/// free-floating draggable/resizable windows with saved layouts — that
/// interaction model doesn't have a Slint equivalent yet, so pinned
/// monitors render as a plain card list here instead; pin state is
/// in-memory only for the same reason (not persisted to AppConfig like the
/// egui app does). Real fetch/parse/pin-browse behavior is unchanged.
pub fn wire(app: &AppShell, manager: &ClusterManager, clusters: &Rc<VecModel<ClusterItem>>) {
    let monitors_master: Rc<RefCell<Vec<SyntheticMonitor>>> = Rc::new(RefCell::new(Vec::new()));
    let pinned_ids: Rc<RefCell<HashSet<String>>> = Rc::new(RefCell::new(HashSet::new()));

    let monitors_view: Rc<VecModel<MonitorRow>> = Rc::new(VecModel::from(Vec::new()));
    app.set_obs_monitors(ModelRc::from(monitors_view.clone()));
    let pinned_view: Rc<VecModel<MonitorRow>> = Rc::new(VecModel::from(Vec::new()));
    app.set_obs_pinned_monitors(ModelRc::from(pinned_view.clone()));

    let (tx, rx) = mpsc::channel::<ObsResult>();

    let initial_cluster = current_focused_names(clusters).first().cloned().unwrap_or_default();
    app.set_obs_active_cluster(initial_cluster.clone().into());
    if !initial_cluster.is_empty() {
        spawn_fetch(manager, &initial_cluster, "default", tx.clone());
    }

    let manager_for_change = manager.clone();
    let tx_for_change = tx.clone();
    let app_weak = app.as_weak();
    app.on_obs_active_cluster_changed(move |name| {
        let Some(app) = app_weak.upgrade() else { return };
        app.set_obs_active_cluster(name.clone());
        app.set_obs_is_loading(true);
        let space = app.get_obs_space_id().to_string();
        if !name.is_empty() {
            spawn_fetch(&manager_for_change, &name, &space, tx_for_change.clone());
        }
    });

    let app_weak = app.as_weak();
    app.on_obs_space_changed(move |t| {
        if let Some(app) = app_weak.upgrade() {
            app.set_obs_space_id(t);
        }
    });

    let manager_for_apply = manager.clone();
    let tx_for_apply = tx.clone();
    let app_weak = app.as_weak();
    app.on_obs_space_apply(move || {
        let Some(app) = app_weak.upgrade() else { return };
        let cluster = app.get_obs_active_cluster().to_string();
        let space = app.get_obs_space_id().to_string();
        if !cluster.is_empty() {
            app.set_obs_is_loading(true);
            spawn_fetch(&manager_for_apply, &cluster, &space, tx_for_apply.clone());
        }
    });

    let manager_for_refresh = manager.clone();
    let tx_for_refresh = tx.clone();
    let app_weak = app.as_weak();
    app.on_obs_refresh_clicked(move || {
        let Some(app) = app_weak.upgrade() else { return };
        let cluster = app.get_obs_active_cluster().to_string();
        let space = app.get_obs_space_id().to_string();
        if !cluster.is_empty() {
            app.set_obs_is_loading(true);
            spawn_fetch(&manager_for_refresh, &cluster, &space, tx_for_refresh.clone());
        }
    });

    let app_weak = app.as_weak();
    app.on_obs_workspace_tab_changed(move |t| {
        if let Some(app) = app_weak.upgrade() {
            app.set_obs_workspace_tab(t);
        }
    });

    let monitors_master_for_filter = monitors_master.clone();
    let pinned_ids_for_filter = pinned_ids.clone();
    let monitors_view_for_filter = monitors_view.clone();
    let app_weak = app.as_weak();
    app.on_obs_filter_changed(move |t| {
        let Some(app) = app_weak.upgrade() else { return };
        app.set_obs_filter_text(t.clone());
        rebuild_browse_view(&monitors_master_for_filter, &pinned_ids_for_filter, &monitors_view_for_filter, &t);
    });

    let monitors_master_for_pin = monitors_master.clone();
    let pinned_ids_for_pin = pinned_ids.clone();
    let monitors_view_for_pin = monitors_view.clone();
    let pinned_view_for_pin = pinned_view.clone();
    let app_weak = app.as_weak();
    app.on_obs_pin_toggled(move |id, pinned| {
        let Some(app) = app_weak.upgrade() else { return };
        if pinned {
            pinned_ids_for_pin.borrow_mut().insert(id.to_string());
        } else {
            pinned_ids_for_pin.borrow_mut().remove(id.as_str());
        }
        let filter = app.get_obs_filter_text().to_string();
        rebuild_browse_view(&monitors_master_for_pin, &pinned_ids_for_pin, &monitors_view_for_pin, &filter);
        rebuild_pinned_view(&monitors_master_for_pin, &pinned_ids_for_pin, &pinned_view_for_pin);
    });

    let app_weak = app.as_weak();
    let timer = slint::Timer::default();
    timer.start(slint::TimerMode::Repeated, Duration::from_millis(150), move || {
        let Some(app) = app_weak.upgrade() else { return };
        while let Ok(result) = rx.try_recv() {
            if result.cluster_name != app.get_obs_active_cluster().as_str() {
                continue;
            }
            app.set_obs_is_loading(false);
            match result.monitors {
                Ok(list) => {
                    app.set_obs_error("".into());
                    *monitors_master.borrow_mut() = list;
                }
                Err(e) => {
                    app.set_obs_error(e.into());
                    monitors_master.borrow_mut().clear();
                }
            }
            let filter = app.get_obs_filter_text().to_string();
            rebuild_browse_view(&monitors_master, &pinned_ids, &monitors_view, &filter);
            rebuild_pinned_view(&monitors_master, &pinned_ids, &pinned_view);
        }
    });
    std::mem::forget(timer);
}

fn spawn_fetch(manager: &ClusterManager, cluster_name: &str, space_id: &str, tx: mpsc::Sender<ObsResult>) {
    let Some(client) = manager.get_client(cluster_name) else {
        return;
    };
    let Some(config) = manager.clusters().into_iter().find(|c| c.name == cluster_name) else {
        return;
    };
    let kibana_host = config.resolve_kibana_host();
    let cluster_name = cluster_name.to_string();
    let space_id = space_id.to_string();

    tokio::spawn(async move {
        let result = client
            .get_kibana_synthetics_monitors(&kibana_host, Some(&space_id))
            .await
            .map(|v| parse_synthetics_monitors(&v))
            .map_err(|e| e.to_string());
        let result = match result {
            Ok(monitors) if monitors.is_empty() => {
                Err("No monitors configured in this Kibana space.".to_string())
            }
            other => other,
        };
        let _ = tx.send(ObsResult { cluster_name, monitors: result });
    });
}

fn to_row(m: &SyntheticMonitor, pinned: bool) -> MonitorRow {
    MonitorRow {
        id: m.id.clone().into(),
        name: m.name.clone().into(),
        url: m.url.clone().into(),
        monitor_type: m.monitor_type.to_uppercase().into(),
        status: m.status.clone().into(),
        latency_label: if m.status == "up" {
            format!("{} ms", m.latency_ms).into()
        } else {
            "N/A".to_string().into()
        },
        locations: m.locations.join(", ").into(),
        pinned,
    }
}

fn rebuild_browse_view(
    master: &RefCell<Vec<SyntheticMonitor>>,
    pinned_ids: &RefCell<HashSet<String>>,
    view: &VecModel<MonitorRow>,
    filter: &str,
) {
    let needle = filter.to_lowercase();
    let pinned = pinned_ids.borrow();
    let rows: Vec<MonitorRow> = master
        .borrow()
        .iter()
        .filter(|m| needle.is_empty() || m.name.to_lowercase().contains(&needle))
        .map(|m| to_row(m, pinned.contains(&m.id)))
        .collect();
    view.set_vec(rows);
}

fn rebuild_pinned_view(
    master: &RefCell<Vec<SyntheticMonitor>>,
    pinned_ids: &RefCell<HashSet<String>>,
    view: &VecModel<MonitorRow>,
) {
    let pinned = pinned_ids.borrow();
    let rows: Vec<MonitorRow> = master
        .borrow()
        .iter()
        .filter(|m| pinned.contains(&m.id))
        .map(|m| to_row(m, true))
        .collect();
    view.set_vec(rows);
}

fn current_focused_names(clusters: &VecModel<ClusterItem>) -> Vec<String> {
    (0..clusters.row_count())
        .filter_map(|i| clusters.row_data(i))
        .filter(|c| c.checked)
        .map(|c| c.name.to_string())
        .collect()
}
