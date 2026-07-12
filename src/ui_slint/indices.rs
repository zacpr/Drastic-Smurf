use slint::{ComponentHandle, Model, ModelRc, VecModel};
use std::cell::RefCell;
use std::collections::HashSet;
use std::rc::Rc;
use std::sync::mpsc;
use std::time::Duration;

use crate::core::cluster_manager::ClusterManager;
use crate::core::es_client::{CatIndex, DataStream};
use crate::ui::widgets::{human_bytes, human_docs};

use super::{AppShell, ClusterItem, DataStreamRow, IndexRow};

struct IndicesResult {
    cluster_name: String,
    indices: Result<Vec<CatIndex>, String>,
    datastreams: Result<Vec<DataStream>, String>,
}

#[derive(Clone)]
struct Shared {
    indices_master: Rc<RefCell<Vec<CatIndex>>>,
    datastreams_master: Rc<RefCell<Vec<DataStream>>>,
    checked_indices: Rc<RefCell<HashSet<String>>>,
    checked_datastreams: Rc<RefCell<HashSet<String>>>,
    indices_view: Rc<VecModel<IndexRow>>,
    datastreams_view: Rc<VecModel<DataStreamRow>>,
}

impl Shared {
    fn rebuild(&self, app: &AppShell) {
        let filter = app.get_indices_filter_text().to_string();
        let only_selected = app.get_indices_only_selected();
        let sort_field = app.get_indices_sort_field().to_string();
        let descending = app.get_indices_sort_descending();
        rebuild_indices_view(
            &self.indices_master,
            &self.indices_view,
            &self.checked_indices,
            &filter,
            only_selected,
            &sort_field,
            descending,
        );
        rebuild_datastreams_view(
            &self.datastreams_master,
            &self.datastreams_view,
            &self.checked_datastreams,
            &filter,
            only_selected,
        );
        app.set_indices_selected_count(
            (self.checked_indices.borrow().len() + self.checked_datastreams.borrow().len()) as i32,
        );
    }
}

/// Wires the Indices screen: active-cluster-scoped index/data-stream
/// browsing with filter, sort, and a "selected" pin-set, per the redesign's
/// single-cluster-scoped rule (see theplan.md).
///
/// Known simplification vs. the egui version: no ILM policy / index
/// template / settings drill-down detail panel (IndexDetail in
/// modules/indices.rs — requires several extra fetches per index) and no
/// previous-doc-count/size trend indicators. Both are reasonable follow-ups
/// if index lifecycle inspection turns out to matter day to day.
pub fn wire(app: &AppShell, manager: &ClusterManager, clusters: &Rc<VecModel<ClusterItem>>) {
    let indices_view: Rc<VecModel<IndexRow>> = Rc::new(VecModel::from(Vec::new()));
    app.set_indices_rows(ModelRc::from(indices_view.clone()));
    let datastreams_view: Rc<VecModel<DataStreamRow>> = Rc::new(VecModel::from(Vec::new()));
    app.set_indices_datastream_rows(ModelRc::from(datastreams_view.clone()));

    let shared = Shared {
        indices_master: Rc::new(RefCell::new(Vec::new())),
        datastreams_master: Rc::new(RefCell::new(Vec::new())),
        checked_indices: Rc::new(RefCell::new(HashSet::new())),
        checked_datastreams: Rc::new(RefCell::new(HashSet::new())),
        indices_view,
        datastreams_view,
    };

    let (tx, rx) = mpsc::channel::<IndicesResult>();

    let initial_cluster = current_focused_names(clusters).first().cloned().unwrap_or_default();
    app.set_indices_active_cluster(initial_cluster.clone().into());
    if !initial_cluster.is_empty() {
        spawn_fetch(manager, &initial_cluster, tx.clone());
    }

    let manager_for_change = manager.clone();
    let tx_for_change = tx.clone();
    let app_weak = app.as_weak();
    app.on_indices_active_cluster_changed(move |name| {
        let Some(app) = app_weak.upgrade() else { return };
        app.set_indices_active_cluster(name.clone());
        app.set_indices_error("".into());
        app.set_indices_is_loading(true);
        if !name.is_empty() {
            spawn_fetch(&manager_for_change, &name, tx_for_change.clone());
        }
    });

    let manager_for_refresh = manager.clone();
    let tx_for_refresh = tx.clone();
    let app_weak = app.as_weak();
    app.on_indices_refresh_clicked(move || {
        let Some(app) = app_weak.upgrade() else { return };
        let cluster = app.get_indices_active_cluster().to_string();
        if !cluster.is_empty() {
            app.set_indices_is_loading(true);
            spawn_fetch(&manager_for_refresh, &cluster, tx_for_refresh.clone());
        }
    });

    let app_weak = app.as_weak();
    app.on_indices_tab_changed(move |t| {
        if let Some(app) = app_weak.upgrade() {
            app.set_indices_active_tab(t);
        }
    });

    let shared_for_filter = shared.clone();
    let app_weak = app.as_weak();
    app.on_indices_filter_changed(move |t| {
        let Some(app) = app_weak.upgrade() else { return };
        app.set_indices_filter_text(t);
        shared_for_filter.rebuild(&app);
    });

    let shared_for_only_selected = shared.clone();
    let app_weak = app.as_weak();
    app.on_indices_only_selected_toggled(move || {
        let Some(app) = app_weak.upgrade() else { return };
        app.set_indices_only_selected(!app.get_indices_only_selected());
        shared_for_only_selected.rebuild(&app);
    });

    let shared_for_sort = shared.clone();
    let app_weak = app.as_weak();
    app.on_indices_sort_changed(move |field| {
        let Some(app) = app_weak.upgrade() else { return };
        let current = app.get_indices_sort_field();
        if current == field {
            app.set_indices_sort_descending(!app.get_indices_sort_descending());
        } else {
            app.set_indices_sort_field(field);
            app.set_indices_sort_descending(false);
        }
        shared_for_sort.rebuild(&app);
    });

    let shared_for_index_toggle = shared.clone();
    let app_weak = app.as_weak();
    app.on_indices_index_checked(move |name, checked| {
        let Some(app) = app_weak.upgrade() else { return };
        if checked {
            shared_for_index_toggle.checked_indices.borrow_mut().insert(name.to_string());
        } else {
            shared_for_index_toggle.checked_indices.borrow_mut().remove(name.as_str());
        }
        shared_for_index_toggle.rebuild(&app);
    });

    let shared_for_ds_toggle = shared.clone();
    let app_weak = app.as_weak();
    app.on_indices_datastream_checked(move |name, checked| {
        let Some(app) = app_weak.upgrade() else { return };
        if checked {
            shared_for_ds_toggle.checked_datastreams.borrow_mut().insert(name.to_string());
        } else {
            shared_for_ds_toggle.checked_datastreams.borrow_mut().remove(name.as_str());
        }
        shared_for_ds_toggle.rebuild(&app);
    });

    let shared_for_timer = shared.clone();
    let app_weak = app.as_weak();
    let timer = slint::Timer::default();
    timer.start(slint::TimerMode::Repeated, Duration::from_millis(150), move || {
        let Some(app) = app_weak.upgrade() else { return };
        while let Ok(result) = rx.try_recv() {
            if result.cluster_name != app.get_indices_active_cluster().as_str() {
                continue;
            }
            app.set_indices_is_loading(false);
            match result.indices {
                Ok(list) => {
                    app.set_indices_error("".into());
                    *shared_for_timer.indices_master.borrow_mut() = list;
                }
                Err(e) => {
                    tracing::warn!("Indices: fetch failed for '{}': {}", result.cluster_name, e);
                    app.set_indices_error(e.into());
                    shared_for_timer.indices_master.borrow_mut().clear();
                }
            }
            if let Ok(list) = result.datastreams {
                *shared_for_timer.datastreams_master.borrow_mut() = list;
            }
            shared_for_timer.rebuild(&app);
        }
    });
    std::mem::forget(timer);
}

fn spawn_fetch(manager: &ClusterManager, cluster_name: &str, tx: mpsc::Sender<IndicesResult>) {
    let Some(client) = manager.get_client(cluster_name) else {
        return;
    };
    let cluster_name = cluster_name.to_string();

    tokio::spawn(async move {
        let (indices, datastreams) = tokio::join!(client.cat_indices(), client.get_data_streams());
        let _ = tx.send(IndicesResult {
            cluster_name,
            indices: indices.map_err(|e| e.to_string()),
            datastreams: datastreams
                .map(|r| r.data_streams)
                .map_err(|e| e.to_string()),
        });
    });
}

fn rebuild_indices_view(
    master: &RefCell<Vec<CatIndex>>,
    view: &VecModel<IndexRow>,
    checked: &RefCell<HashSet<String>>,
    filter: &str,
    only_selected: bool,
    sort_field: &str,
    descending: bool,
) {
    let needle = filter.to_lowercase();
    let checked = checked.borrow();
    let mut rows: Vec<CatIndex> = master
        .borrow()
        .iter()
        .filter(|idx| needle.is_empty() || idx.index.to_lowercase().contains(&needle))
        .filter(|idx| !only_selected || checked.contains(&idx.index))
        .cloned()
        .collect();

    rows.sort_by(|a, b| {
        let cmp = match sort_field {
            "docs" => parse_i64(&a.docs_count).cmp(&parse_i64(&b.docs_count)),
            "size" => parse_u64(&a.store_size).cmp(&parse_u64(&b.store_size)),
            _ => a.index.to_lowercase().cmp(&b.index.to_lowercase()),
        };
        if descending { cmp.reverse() } else { cmp }
    });

    let view_rows: Vec<IndexRow> = rows
        .iter()
        .map(|idx| IndexRow {
            name: idx.index.clone().into(),
            health: idx.health.clone().unwrap_or_default().into(),
            status: idx.status.clone().unwrap_or_default().into(),
            docs_count: human_docs(parse_i64(&idx.docs_count).max(0) as u64).into(),
            size: human_bytes(parse_u64(&idx.store_size)).into(),
            checked: checked.contains(&idx.index),
        })
        .collect();
    view.set_vec(view_rows);
}

fn rebuild_datastreams_view(
    master: &RefCell<Vec<DataStream>>,
    view: &VecModel<DataStreamRow>,
    checked: &RefCell<HashSet<String>>,
    filter: &str,
    only_selected: bool,
) {
    let needle = filter.to_lowercase();
    let checked = checked.borrow();
    let rows: Vec<DataStreamRow> = master
        .borrow()
        .iter()
        .filter(|ds| needle.is_empty() || ds.name.to_lowercase().contains(&needle))
        .filter(|ds| !only_selected || checked.contains(&ds.name))
        .map(|ds| DataStreamRow {
            name: ds.name.clone().into(),
            status: ds.status.clone().into(),
            generation: ds.generation as i32,
            backing_indices: ds.indices.len() as i32,
            size: human_bytes(ds.store_size_bytes.unwrap_or(0).max(0) as u64).into(),
            checked: checked.contains(&ds.name),
        })
        .collect();
    view.set_vec(rows);
}

fn parse_i64(s: &Option<String>) -> i64 {
    s.as_deref().unwrap_or("0").parse().unwrap_or(0)
}

fn parse_u64(s: &Option<String>) -> u64 {
    s.as_deref().unwrap_or("0").parse().unwrap_or(0)
}

fn current_focused_names(clusters: &VecModel<ClusterItem>) -> Vec<String> {
    (0..clusters.row_count())
        .filter_map(|i| clusters.row_data(i))
        .filter(|c| c.checked)
        .map(|c| c.name.to_string())
        .collect()
}
