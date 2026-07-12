use slint::{ComponentHandle, Model, ModelRc, VecModel};
use std::cell::RefCell;
use std::rc::Rc;
use std::sync::mpsc;
use std::time::Duration;

use crate::core::cluster_manager::ClusterManager;
use crate::core::es_client::TaskInfo;
use crate::modules::tasks::get_task_progress_and_eta;
use crate::ui::widgets::human_nanos;

use super::{AppShell, ClusterItem, TaskRow};

struct TasksResult {
    cluster_name: String,
    tasks: Result<Vec<(String, TaskInfo)>, String>,
}

/// Wires the Tasks screen: active-cluster-scoped task monitoring, replacing
/// the old egui Tasks tab's all-clusters aggregated list, per the redesign's
/// single-cluster-scoped rule (see theplan.md).
pub fn wire(app: &AppShell, manager: &ClusterManager, clusters: &Rc<VecModel<ClusterItem>>) {
    let tasks_master: Rc<RefCell<Vec<(String, TaskInfo)>>> = Rc::new(RefCell::new(Vec::new()));
    let tasks_view: Rc<VecModel<TaskRow>> = Rc::new(VecModel::from(Vec::new()));
    app.set_tasks_rows(ModelRc::from(tasks_view.clone()));
    let expanded: Rc<RefCell<std::collections::HashSet<String>>> =
        Rc::new(RefCell::new(std::collections::HashSet::new()));

    let (tx, rx) = mpsc::channel::<TasksResult>();

    let initial_cluster = current_focused_names(clusters).first().cloned().unwrap_or_default();
    app.set_tasks_active_cluster(initial_cluster.clone().into());
    if !initial_cluster.is_empty() {
        spawn_fetch(manager, &initial_cluster, tx.clone());
    }

    let manager_for_change = manager.clone();
    let tx_for_change = tx.clone();
    let app_weak = app.as_weak();
    app.on_tasks_active_cluster_changed(move |name| {
        let Some(app) = app_weak.upgrade() else { return };
        app.set_tasks_active_cluster(name.clone());
        app.set_tasks_error("".into());
        app.set_tasks_is_loading(true);
        if !name.is_empty() {
            spawn_fetch(&manager_for_change, &name, tx_for_change.clone());
        }
    });

    let manager_for_refresh = manager.clone();
    let tx_for_refresh = tx.clone();
    let app_weak = app.as_weak();
    app.on_tasks_refresh_clicked(move || {
        let Some(app) = app_weak.upgrade() else { return };
        let cluster = app.get_tasks_active_cluster().to_string();
        if !cluster.is_empty() {
            app.set_tasks_is_loading(true);
            spawn_fetch(&manager_for_refresh, &cluster, tx_for_refresh.clone());
        }
    });

    let tasks_master_for_filter = tasks_master.clone();
    let tasks_view_for_filter = tasks_view.clone();
    let expanded_for_filter = expanded.clone();
    let app_weak = app.as_weak();
    app.on_tasks_filter_changed(move |t| {
        let Some(app) = app_weak.upgrade() else { return };
        app.set_tasks_filter_text(t.clone());
        let category = app.get_tasks_selected_category().to_string();
        rebuild_view(&tasks_master_for_filter, &tasks_view_for_filter, &expanded_for_filter, &t, &category);
    });

    let tasks_master_for_cat = tasks_master.clone();
    let tasks_view_for_cat = tasks_view.clone();
    let expanded_for_cat = expanded.clone();
    let app_weak = app.as_weak();
    app.on_tasks_category_changed(move |c| {
        let Some(app) = app_weak.upgrade() else { return };
        app.set_tasks_selected_category(c.clone());
        let filter = app.get_tasks_filter_text().to_string();
        rebuild_view(&tasks_master_for_cat, &tasks_view_for_cat, &expanded_for_cat, &filter, &c);
    });

    let tasks_master_for_toggle = tasks_master.clone();
    let tasks_view_for_toggle = tasks_view.clone();
    let expanded_for_toggle = expanded.clone();
    let app_weak = app.as_weak();
    app.on_tasks_task_toggle_expanded(move |key| {
        let Some(app) = app_weak.upgrade() else { return };
        let key = key.to_string();
        {
            let mut set = expanded_for_toggle.borrow_mut();
            if !set.remove(&key) {
                set.insert(key);
            }
        }
        let filter = app.get_tasks_filter_text().to_string();
        let category = app.get_tasks_selected_category().to_string();
        rebuild_view(&tasks_master_for_toggle, &tasks_view_for_toggle, &expanded_for_toggle, &filter, &category);
    });

    let app_weak = app.as_weak();
    let timer = slint::Timer::default();
    timer.start(slint::TimerMode::Repeated, Duration::from_millis(150), move || {
        let Some(app) = app_weak.upgrade() else { return };
        while let Ok(result) = rx.try_recv() {
            if result.cluster_name != app.get_tasks_active_cluster().as_str() {
                continue;
            }
            app.set_tasks_is_loading(false);
            match result.tasks {
                Ok(tasks) => {
                    app.set_tasks_error("".into());
                    *tasks_master.borrow_mut() = tasks;
                }
                Err(e) => {
                    tracing::warn!("Tasks: fetch failed for '{}': {}", result.cluster_name, e);
                    app.set_tasks_error(e.into());
                    tasks_master.borrow_mut().clear();
                }
            }
            let categories = build_categories(&tasks_master.borrow());
            app.set_tasks_categories(ModelRc::from(Rc::new(VecModel::from(
                categories.iter().map(|c| c.clone().into()).collect::<Vec<slint::SharedString>>(),
            ))));
            let filter = app.get_tasks_filter_text().to_string();
            let category = app.get_tasks_selected_category().to_string();
            rebuild_view(&tasks_master, &tasks_view, &expanded, &filter, &category);
        }
    });
    std::mem::forget(timer);
}

fn spawn_fetch(manager: &ClusterManager, cluster_name: &str, tx: mpsc::Sender<TasksResult>) {
    let Some(client) = manager.get_client(cluster_name) else {
        return;
    };
    let cluster_name = cluster_name.to_string();

    tokio::spawn(async move {
        let result = client
            .tasks(None)
            .await
            .map(|resp| {
                resp.nodes
                    .into_values()
                    .flat_map(|node| {
                        let node_name = node.name;
                        node.tasks
                            .into_values()
                            .map(move |t| (node_name.clone(), t))
                    })
                    .collect::<Vec<_>>()
            })
            .map_err(|e| e.to_string());
        let _ = tx.send(TasksResult { cluster_name, tasks: result });
    });
}

fn build_categories(tasks: &[(String, TaskInfo)]) -> Vec<String> {
    let mut categories: std::collections::HashSet<String> = std::collections::HashSet::new();
    for (_, task) in tasks {
        let cat = match task.action.find(':') {
            Some(pos) => task.action[..pos].to_string(),
            None => task.action.clone(),
        };
        categories.insert(cat);
    }
    let mut list: Vec<String> = categories.into_iter().collect();
    list.sort();
    let mut result = vec!["All".to_string()];
    result.extend(list);
    result
}

fn rebuild_view(
    master: &RefCell<Vec<(String, TaskInfo)>>,
    view: &VecModel<TaskRow>,
    expanded: &RefCell<std::collections::HashSet<String>>,
    filter: &str,
    category: &str,
) {
    let needle = filter.to_lowercase();
    let expanded = expanded.borrow();
    let rows: Vec<TaskRow> = master
        .borrow()
        .iter()
        .filter(|(_, task)| {
            if !category.is_empty() && category != "All" {
                let cat = match task.action.find(':') {
                    Some(pos) => &task.action[..pos],
                    None => task.action.as_str(),
                };
                if cat != category {
                    return false;
                }
            }
            if needle.is_empty() {
                return true;
            }
            task.action.to_lowercase().contains(&needle)
                || task.node.to_lowercase().contains(&needle)
                || task
                    .description
                    .as_deref()
                    .unwrap_or_default()
                    .to_lowercase()
                    .contains(&needle)
        })
        .map(|(node_name, task)| to_row(node_name, task, &expanded))
        .collect();
    view.set_vec(rows);
}

fn to_row(node_name: &str, task: &TaskInfo, expanded: &std::collections::HashSet<String>) -> TaskRow {
    let task_id = format!("{}:{}", task.node, task.id);
    let task_key = format!("{}:{}", node_name, task_id);
    let running_mins = task.running_time_in_nanos as f64 / 60_000_000_000.0;
    let progress = get_task_progress_and_eta(task);
    let start_time = chrono::DateTime::from_timestamp(task.start_time_in_millis / 1000, 0)
        .map(|d| d.to_rfc2822())
        .unwrap_or_else(|| "Unknown".to_string());

    TaskRow {
        task_key: task_key.clone().into(),
        action: task.action.clone().into(),
        task_id: task_id.into(),
        node: task.node.clone().into(),
        task_type: task.task_type.clone().into(),
        description: task.description.clone().unwrap_or_default().into(),
        cancellable: task.cancellable,
        running_time_label: human_nanos(task.running_time_in_nanos).into(),
        has_progress: progress.is_some(),
        progress_ratio: progress.as_ref().map(|(p, _)| *p).unwrap_or(0.0),
        eta_label: progress.map(|(_, e)| e).unwrap_or_default().into(),
        is_long_running: running_mins >= 1.0,
        expanded: expanded.contains(&task_key),
        start_time_label: start_time.into(),
        parent_task_id: task.parent_task_id.clone().unwrap_or_default().into(),
    }
}

fn current_focused_names(clusters: &VecModel<ClusterItem>) -> Vec<String> {
    (0..clusters.row_count())
        .filter_map(|i| clusters.row_data(i))
        .filter(|c| c.checked)
        .map(|c| c.name.to_string())
        .collect()
}
