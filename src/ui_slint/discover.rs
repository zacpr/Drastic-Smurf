use serde_json::Value;
use slint::{ComponentHandle, Model, ModelRc, VecModel};
use std::rc::Rc;
use std::sync::mpsc;
use std::time::Duration;

use crate::core::cluster_manager::ClusterManager;

use super::{AppShell, ClusterItem, DocRow};

struct SearchResult {
    cluster_name: String,
    result: Result<String, String>,
}

const TIME_PRESETS: &[&str] = &["30m", "1h", "6h", "12h", "24h", "72h"];

/// Wires the Discover screen: active-cluster-scoped ad-hoc search against an
/// index pattern, mirroring the egui Discover tab's query-building logic
/// (query_string + optional @timestamp range filter) but reimplemented here
/// rather than extracted, since the original lives inline inside
/// render_discover_module mixed with egui calls.
///
/// Known simplification vs. the egui version: no custom date-range picker
/// (only the six relative presets) and no dynamic field-column selector —
/// each result renders as an expandable pretty-printed JSON card instead,
/// matching the pattern already used for Tasks.
pub fn wire(app: &AppShell, manager: &ClusterManager, clusters: &Rc<VecModel<ClusterItem>>) {
    let results_view: Rc<VecModel<DocRow>> = Rc::new(VecModel::from(Vec::new()));
    app.set_discover_results(ModelRc::from(results_view.clone()));
    let expanded: Rc<std::cell::RefCell<std::collections::HashSet<String>>> =
        Rc::new(std::cell::RefCell::new(std::collections::HashSet::new()));

    let (tx, rx) = mpsc::channel::<SearchResult>();

    let initial_cluster = current_focused_names(clusters).first().cloned().unwrap_or_default();
    app.set_discover_active_cluster(initial_cluster.into());

    let app_weak = app.as_weak();
    app.on_discover_active_cluster_changed(move |name| {
        if let Some(app) = app_weak.upgrade() {
            app.set_discover_active_cluster(name);
        }
    });

    let app_weak = app.as_weak();
    app.on_discover_time_preset_changed(move |p| {
        if let Some(app) = app_weak.upgrade() {
            app.set_discover_time_preset(p);
        }
    });

    let manager_for_search = manager.clone();
    let tx_for_search = tx.clone();
    let app_weak = app.as_weak();
    app.on_discover_search_clicked(move || {
        let Some(app) = app_weak.upgrade() else { return };
        let cluster = app.get_discover_active_cluster().to_string();
        if cluster.is_empty() {
            return;
        }
        let Some(client) = manager_for_search.get_client(&cluster) else {
            app.set_discover_error("No client available for this cluster.".into());
            return;
        };
        let index_pattern = app.get_discover_index_pattern().to_string();
        let query = app.get_discover_search_query().to_string();
        let preset = app.get_discover_time_preset();

        let path = format!("/{}/_search", index_pattern.trim_start_matches('/'));
        let body = build_search_body(&query, preset);

        app.set_discover_is_loading(true);
        let tx = tx_for_search.clone();
        tokio::spawn(async move {
            let result = client
                .execute_raw(reqwest::Method::POST, &path, Some(body.to_string()))
                .await
                .map_err(|e| e.to_string());
            let _ = tx.send(SearchResult { cluster_name: cluster, result });
        });
    });

    let results_view_for_toggle = results_view.clone();
    let expanded_for_toggle = expanded.clone();
    app.on_discover_doc_toggle_expanded(move |key| {
        let key = key.to_string();
        {
            let mut set = expanded_for_toggle.borrow_mut();
            if !set.remove(&key) {
                set.insert(key.clone());
            }
        }
        for i in 0..results_view_for_toggle.row_count() {
            if let Some(mut row) = results_view_for_toggle.row_data(i) {
                if row.doc_key == key {
                    row.expanded = !row.expanded;
                    results_view_for_toggle.set_row_data(i, row);
                    break;
                }
            }
        }
    });

    let app_weak = app.as_weak();
    let timer = slint::Timer::default();
    timer.start(slint::TimerMode::Repeated, Duration::from_millis(150), move || {
        let Some(app) = app_weak.upgrade() else { return };
        while let Ok(msg) = rx.try_recv() {
            if msg.cluster_name != app.get_discover_active_cluster().as_str() {
                continue;
            }
            app.set_discover_is_loading(false);
            match msg.result {
                Ok(body) => {
                    app.set_discover_error("".into());
                    let rows = parse_hits(&body, &expanded.borrow());
                    results_view.set_vec(rows);
                }
                Err(e) => {
                    tracing::warn!("Discover: search failed for '{}': {}", msg.cluster_name, e);
                    app.set_discover_error(e.into());
                    results_view.set_vec(Vec::new());
                }
            }
        }
    });
    std::mem::forget(timer);
}

fn build_search_body(query: &str, preset: i32) -> Value {
    let range = TIME_PRESETS.get(preset as usize).map(|window| {
        serde_json::json!({
            "@timestamp": { "gte": format!("now-{}", window), "lte": "now" }
        })
    });

    let must = if query.trim().is_empty() {
        serde_json::json!({ "match_all": {} })
    } else {
        serde_json::json!({ "query_string": { "query": query.trim() } })
    };

    let filter: Vec<Value> = match range {
        Some(r) => vec![serde_json::json!({ "range": r })],
        None => Vec::new(),
    };

    serde_json::json!({
        "size": 50,
        "sort": [{ "@timestamp": { "order": "desc", "unmapped_type": "date" } }],
        "query": { "bool": { "must": [must], "filter": filter } }
    })
}

fn parse_hits(body: &str, expanded: &std::collections::HashSet<String>) -> Vec<DocRow> {
    let Ok(parsed) = serde_json::from_str::<Value>(body) else {
        return Vec::new();
    };
    let Some(hits) = parsed.get("hits").and_then(|h| h.get("hits")).and_then(|h| h.as_array()) else {
        return Vec::new();
    };

    hits.iter()
        .enumerate()
        .map(|(i, hit)| {
            let id = hit.get("_id").and_then(|v| v.as_str()).unwrap_or_default();
            let key = if id.is_empty() { format!("row-{i}") } else { id.to_string() };
            let source = hit.get("_source").cloned().unwrap_or(Value::Null);
            let compact = serde_json::to_string(&source).unwrap_or_default();
            let preview = if compact.len() > 180 {
                let cut = compact
                    .char_indices()
                    .map(|(i, _)| i)
                    .take_while(|&i| i <= 180)
                    .last()
                    .unwrap_or(0);
                format!("{}…", &compact[..cut])
            } else {
                compact
            };
            let pretty = serde_json::to_string_pretty(&source).unwrap_or_default();
            DocRow {
                doc_key: key.clone().into(),
                preview: preview.into(),
                full_json: pretty.into(),
                expanded: expanded.contains(&key),
            }
        })
        .collect()
}

fn current_focused_names(clusters: &VecModel<ClusterItem>) -> Vec<String> {
    (0..clusters.row_count())
        .filter_map(|i| clusters.row_data(i))
        .filter(|c| c.checked)
        .map(|c| c.name.to_string())
        .collect()
}
