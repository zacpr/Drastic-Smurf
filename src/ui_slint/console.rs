use slint::{ComponentHandle, Model, ModelRc, VecModel};
use std::cell::RefCell;
use std::rc::Rc;
use std::sync::mpsc;
use std::time::Duration;

use crate::core::cluster_manager::ClusterManager;
use crate::core::config::SavedQuery;
use crate::modules::console::{PRESETS, interpolate_variables};

use super::{
    AppShell, ClusterItem, HistoryEntry, PresetCategory, PresetItem, SavedQueryItem, VariablePair,
};

struct SendResult {
    text: Result<String, String>,
}

/// Wires the Console screen (presets/variables/history/custom tabs + the
/// request/response editor) to real ClusterManager/EsClient data. Kept in
/// its own module since Console is the largest single screen in the
/// egui->Slint retrofit (see theplan.md build order step 6).
pub fn wire(app: &AppShell, manager: &ClusterManager, clusters: &Rc<VecModel<ClusterItem>>) {
    let preset_categories = build_preset_categories();
    app.set_console_preset_categories(ModelRc::from(Rc::new(VecModel::from(preset_categories))));

    let focused_names = current_focused_names(clusters);
    let initial_cluster = focused_names.first().cloned().unwrap_or_default();
    app.set_focused_cluster_names(ModelRc::from(Rc::new(VecModel::from(
        focused_names.iter().map(|n| n.clone().into()).collect::<Vec<slint::SharedString>>(),
    ))));
    app.set_console_active_cluster(initial_cluster.clone().into());
    app.set_console_path("/_cluster/health".into());

    let variables: Rc<VecModel<VariablePair>> = Rc::new(VecModel::from(Vec::new()));
    app.set_console_variables(ModelRc::from(variables.clone()));

    let history_view: Rc<VecModel<HistoryEntry>> = Rc::new(VecModel::from(Vec::new()));
    app.set_console_history(ModelRc::from(history_view.clone()));
    let history_full: Rc<RefCell<Vec<(String, String, String)>>> = Rc::new(RefCell::new(Vec::new()));

    let saved_view: Rc<VecModel<SavedQueryItem>> = Rc::new(VecModel::from(Vec::new()));
    app.set_console_saved_queries(ModelRc::from(saved_view.clone()));
    let saved_full: Rc<RefCell<Vec<SavedQuery>>> = Rc::new(RefCell::new(Vec::new()));

    load_cluster_data(manager, &initial_cluster, &variables, &saved_view, &saved_full);

    // --- Workspace tab / preset selection -----------------------------
    let app_weak = app.as_weak();
    app.on_console_workspace_tab_changed(move |tab| {
        if let Some(app) = app_weak.upgrade() {
            app.set_console_workspace_tab(tab);
        }
    });

    let app_weak = app.as_weak();
    app.on_console_category_toggled(move |name| {
        let Some(app) = app_weak.upgrade() else { return };
        let cats = app.get_console_preset_categories();
        for i in 0..cats.row_count() {
            if let Some(mut cat) = cats.row_data(i) {
                if cat.name == name {
                    cat.expanded = !cat.expanded;
                    cats.set_row_data(i, cat);
                    break;
                }
            }
        }
    });

    let app_weak = app.as_weak();
    app.on_console_preset_selected(move |item: PresetItem| {
        let Some(app) = app_weak.upgrade() else { return };
        app.set_console_method(item.method);
        app.set_console_path(item.path);
        app.set_console_body(item.body);
        app.set_console_use_kibana(item.use_kibana);
    });

    // --- Variables ------------------------------------------------------
    let variables_for_change = variables.clone();
    let manager_for_vars = manager.clone();
    let app_weak = app.as_weak();
    app.on_console_variable_changed(move |i, k, v| {
        if let Some(mut pair) = variables_for_change.row_data(i as usize) {
            pair.key = k;
            pair.value = v;
            variables_for_change.set_row_data(i as usize, pair);
        }
        if let Some(app) = app_weak.upgrade() {
            persist_variables(&manager_for_vars, &app.get_console_active_cluster(), &variables_for_change);
        }
    });

    let variables_for_add = variables.clone();
    app.on_console_variable_added(move || {
        variables_for_add.push(VariablePair {
            key: "".into(),
            value: "".into(),
        });
    });

    let variables_for_remove = variables.clone();
    let manager_for_remove = manager.clone();
    let app_weak = app.as_weak();
    app.on_console_variable_removed(move |i| {
        variables_for_remove.remove(i as usize);
        if let Some(app) = app_weak.upgrade() {
            persist_variables(&manager_for_remove, &app.get_console_active_cluster(), &variables_for_remove);
        }
    });

    // --- History ----------------------------------------------------
    let history_full_for_select = history_full.clone();
    let app_weak = app.as_weak();
    app.on_console_history_selected(move |i| {
        let Some(app) = app_weak.upgrade() else { return };
        if let Some((method, path, body)) = history_full_for_select.borrow().get(i as usize) {
            app.set_console_method(method.clone().into());
            app.set_console_path(path.clone().into());
            app.set_console_body(body.clone().into());
        }
    });

    let history_view_for_clear = history_view.clone();
    let history_full_for_clear = history_full.clone();
    app.on_console_history_cleared(move || {
        history_view_for_clear.set_vec(Vec::new());
        history_full_for_clear.borrow_mut().clear();
    });

    // --- Custom / saved queries --------------------------------------
    let saved_full_for_select = saved_full.clone();
    let app_weak = app.as_weak();
    app.on_console_saved_selected(move |i| {
        let Some(app) = app_weak.upgrade() else { return };
        if let Some(q) = saved_full_for_select.borrow().get(i as usize) {
            app.set_console_method(q.method.clone().into());
            app.set_console_path(q.path.clone().into());
            app.set_console_body(q.body.clone().unwrap_or_default().into());
        }
    });

    let saved_view_for_delete = saved_view.clone();
    let saved_full_for_delete = saved_full.clone();
    let manager_for_delete = manager.clone();
    let app_weak = app.as_weak();
    app.on_console_saved_deleted(move |i| {
        let Some(app) = app_weak.upgrade() else { return };
        let name = saved_full_for_delete.borrow().get(i as usize).map(|q| q.name.clone());
        let Some(name) = name else { return };
        let cluster = app.get_console_active_cluster().to_string();
        if let Some(mut data) = manager_for_delete.get_cluster_data(&cluster) {
            data.saved_queries.retain(|q| q.name != name);
            manager_for_delete.set_cluster_data(&cluster, data);
        }
        saved_view_for_delete.remove(i as usize);
        saved_full_for_delete.borrow_mut().remove(i as usize);
    });

    let app_weak = app.as_weak();
    app.on_console_save_dialog_opened(move || {
        if let Some(app) = app_weak.upgrade() {
            app.set_console_show_save_dialog(true);
            app.set_console_save_name("".into());
        }
    });

    let app_weak = app.as_weak();
    app.on_console_save_name_changed(move |t| {
        if let Some(app) = app_weak.upgrade() {
            app.set_console_save_name(t);
        }
    });

    let app_weak = app.as_weak();
    app.on_console_save_cancelled(move || {
        if let Some(app) = app_weak.upgrade() {
            app.set_console_show_save_dialog(false);
        }
    });

    let saved_view_for_confirm = saved_view.clone();
    let saved_full_for_confirm = saved_full.clone();
    let manager_for_confirm = manager.clone();
    let app_weak = app.as_weak();
    app.on_console_save_confirmed(move || {
        let Some(app) = app_weak.upgrade() else { return };
        let name = app.get_console_save_name().to_string();
        if name.is_empty() {
            return;
        }
        let query = SavedQuery {
            name: name.clone(),
            method: app.get_console_method().to_string(),
            path: app.get_console_path().to_string(),
            body: {
                let b = app.get_console_body().to_string();
                if b.trim().is_empty() { None } else { Some(b) }
            },
        };
        let cluster = app.get_console_active_cluster().to_string();
        let mut data = manager_for_confirm.get_cluster_data(&cluster).unwrap_or_default();
        if let Some(idx) = data.saved_queries.iter().position(|q| q.name == query.name) {
            data.saved_queries[idx] = query.clone();
            saved_full_for_confirm.borrow_mut()[idx] = query.clone();
            saved_view_for_confirm.set_row_data(
                idx,
                SavedQueryItem {
                    name: query.name.clone().into(),
                    method: query.method.clone().into(),
                    path: query.path.clone().into(),
                },
            );
        } else {
            data.saved_queries.push(query.clone());
            saved_full_for_confirm.borrow_mut().push(query.clone());
            saved_view_for_confirm.push(SavedQueryItem {
                name: query.name.into(),
                method: query.method.into(),
                path: query.path.into(),
            });
        }
        manager_for_confirm.set_cluster_data(&cluster, data);
        app.set_console_show_save_dialog(false);
    });

    // --- Active cluster switch: swap per-cluster variables/saved queries.
    // ComboBox's current-value binding is one-way (root.active-cluster ->
    // ComboBox), so the callback must write the new value back onto the
    // property itself or every other handler reading
    // get_console_active_cluster() (e.g. Send) would keep seeing the old one.
    let manager_for_cluster_change = manager.clone();
    let variables_for_cluster_change = variables.clone();
    let saved_view_for_cluster_change = saved_view.clone();
    let saved_full_for_cluster_change = saved_full.clone();
    let app_weak = app.as_weak();
    app.on_console_active_cluster_changed(move |name| {
        let Some(app) = app_weak.upgrade() else { return };
        app.set_console_active_cluster(name.clone());
        load_cluster_data(
            &manager_for_cluster_change,
            &name,
            &variables_for_cluster_change,
            &saved_view_for_cluster_change,
            &saved_full_for_cluster_change,
        );
    });

    // --- Send -----------------------------------------------------------
    let (tx, rx) = mpsc::channel::<SendResult>();
    let manager_for_send = manager.clone();
    let variables_for_send = variables.clone();
    let history_view_for_send = history_view.clone();
    let history_full_for_send = history_full.clone();
    let app_weak = app.as_weak();
    app.on_console_send_clicked(move || {
        let Some(app) = app_weak.upgrade() else { return };
        let cluster_name = app.get_console_active_cluster().to_string();
        if cluster_name.is_empty() {
            return;
        }
        let Some(client) = manager_for_send.get_client(&cluster_name) else {
            app.set_console_response("No client available for this cluster.".into());
            return;
        };
        let method = app.get_console_method().to_string();
        let path = app.get_console_path().to_string();
        let body = app.get_console_body().to_string();
        let use_kibana = app.get_console_use_kibana();

        history_view_for_send.push(HistoryEntry {
            method: method.clone().into(),
            path: path.clone().into(),
        });
        history_full_for_send
            .borrow_mut()
            .push((method.clone(), path.clone(), body.clone()));

        let vars: Vec<(String, String)> = (0..variables_for_send.row_count())
            .filter_map(|i| variables_for_send.row_data(i))
            .map(|v| (v.key.to_string(), v.value.to_string()))
            .collect();
        let interp_path = interpolate_variables(&path, &vars);
        let interp_body = interpolate_variables(&body, &vars);
        let body_opt = if interp_body.trim().is_empty() {
            None
        } else {
            Some(interp_body)
        };

        let cluster_config = manager_for_send
            .clusters()
            .into_iter()
            .find(|c| c.name == cluster_name);

        app.set_console_is_loading(true);
        let tx = tx.clone();

        tokio::spawn(async move {
            let reqwest_method = match method.as_str() {
                "GET" => reqwest::Method::GET,
                "POST" => reqwest::Method::POST,
                "PUT" => reqwest::Method::PUT,
                "DELETE" => reqwest::Method::DELETE,
                "HEAD" => reqwest::Method::HEAD,
                _ => reqwest::Method::GET,
            };
            let result = if use_kibana {
                if let Some(config) = cluster_config {
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
                        .send_to_host_raw(&kibana_host, reqwest_method, &interp_path, body_opt)
                        .await
                        .map_err(|e| e.to_string())
                } else {
                    Err("No cluster config found".to_string())
                }
            } else {
                client
                    .execute_raw(reqwest_method, &interp_path, body_opt)
                    .await
                    .map_err(|e| e.to_string())
            };
            let _ = tx.send(SendResult { text: result });
        });
    });

    let app_weak = app.as_weak();
    let timer = slint::Timer::default();
    timer.start(slint::TimerMode::Repeated, Duration::from_millis(150), move || {
        let Some(app) = app_weak.upgrade() else { return };
        while let Ok(result) = rx.try_recv() {
            app.set_console_is_loading(false);
            match result.text {
                Ok(text) => app.set_console_response(text.into()),
                Err(e) => app.set_console_response(format!("Error: {e}").into()),
            }
        }
    });
    // Timer must outlive `run()`; leak it onto the event loop's lifetime.
    std::mem::forget(timer);
}

fn current_focused_names(clusters: &VecModel<ClusterItem>) -> Vec<String> {
    (0..clusters.row_count())
        .filter_map(|i| clusters.row_data(i))
        .filter(|c| c.checked)
        .map(|c| c.name.to_string())
        .collect()
}

fn load_cluster_data(
    manager: &ClusterManager,
    cluster: &str,
    variables: &VecModel<VariablePair>,
    saved_view: &VecModel<SavedQueryItem>,
    saved_full: &RefCell<Vec<SavedQuery>>,
) {
    let data = manager.get_cluster_data(cluster).unwrap_or_default();
    variables.set_vec(
        data.variables
            .iter()
            .map(|(k, v)| VariablePair {
                key: k.clone().into(),
                value: v.clone().into(),
            })
            .collect::<Vec<_>>(),
    );
    saved_view.set_vec(
        data.saved_queries
            .iter()
            .map(|q| SavedQueryItem {
                name: q.name.clone().into(),
                method: q.method.clone().into(),
                path: q.path.clone().into(),
            })
            .collect::<Vec<_>>(),
    );
    *saved_full.borrow_mut() = data.saved_queries;
}

fn persist_variables(manager: &ClusterManager, cluster: &str, variables: &VecModel<VariablePair>) {
    if cluster.is_empty() {
        return;
    }
    let mut data = manager.get_cluster_data(cluster).unwrap_or_default();
    data.variables = (0..variables.row_count())
        .filter_map(|i| variables.row_data(i))
        .map(|v| (v.key.to_string(), v.value.to_string()))
        .collect();
    manager.set_cluster_data(cluster, data);
}

fn build_preset_categories() -> Vec<PresetCategory> {
    let mut categories: Vec<(String, Vec<PresetItem>)> = Vec::new();
    for preset in PRESETS {
        let item = PresetItem {
            name: preset.name.into(),
            method: preset.method.into(),
            path: preset.path.into(),
            body: preset.body.unwrap_or_default().into(),
            use_kibana: preset.use_kibana,
        };
        if let Some((_, items)) = categories.iter_mut().find(|(cat, _)| cat == preset.category) {
            items.push(item);
        } else {
            categories.push((preset.category.to_string(), vec![item]));
        }
    }
    categories
        .into_iter()
        .map(|(name, items)| PresetCategory {
            name: name.into(),
            expanded: false,
            items: ModelRc::from(Rc::new(VecModel::from(items))),
        })
        .collect()
}
