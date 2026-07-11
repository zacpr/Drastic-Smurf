use slint::{ComponentHandle, Model, ModelRc, VecModel};
use std::rc::Rc;
use std::sync::mpsc;
use std::time::Duration;

use crate::core::cluster_manager::ClusterManager;
use crate::ui::widgets::{human_bytes, human_docs};

use super::{AllocationRow, AppShell, ClusterItem, NodeRow};

struct StatusResult {
    cluster_name: String,
    health: Result<crate::core::es_client::ClusterHealth, String>,
    stats: Result<crate::core::es_client::ClusterStats, String>,
    nodes: Result<Vec<crate::core::es_client::CatNode>, String>,
    allocation: Result<Vec<crate::core::es_client::CatAllocation>, String>,
    es_version: Result<String, String>,
    kibana_version: Option<Result<String, String>>,
}

/// Wires the Status screen: a single active-cluster detail view (health,
/// node list, shard allocation) replacing the old egui Status tab's
/// all-clusters card list, per the redesign's "every screen except
/// Dashboard-overview and Settings is single-cluster-scoped" rule.
pub fn wire(app: &AppShell, manager: &ClusterManager, clusters: &Rc<VecModel<ClusterItem>>) {
    let node_rows: Rc<VecModel<NodeRow>> = Rc::new(VecModel::from(Vec::new()));
    app.set_status_node_rows(ModelRc::from(node_rows.clone()));
    let allocation_rows: Rc<VecModel<AllocationRow>> = Rc::new(VecModel::from(Vec::new()));
    app.set_status_allocation_rows(ModelRc::from(allocation_rows.clone()));

    let (tx, rx) = mpsc::channel::<StatusResult>();

    let initial_cluster = current_focused_names(clusters).first().cloned().unwrap_or_default();
    app.set_status_active_cluster(initial_cluster.clone().into());
    if !initial_cluster.is_empty() {
        spawn_fetch(manager, &initial_cluster, tx.clone());
    }

    let manager_for_change = manager.clone();
    let tx_for_change = tx.clone();
    let app_weak = app.as_weak();
    app.on_status_active_cluster_changed(move |name| {
        let Some(app) = app_weak.upgrade() else { return };
        // ComboBox's current-value binding is one-way; write the selection
        // back onto the property itself (see the identical note in
        // console.rs — Send there hit the same trap).
        app.set_status_active_cluster(name.clone());
        app.set_status_connected(false);
        app.set_status_error("".into());
        if !name.is_empty() {
            spawn_fetch(&manager_for_change, &name, tx_for_change.clone());
        }
    });

    let app_weak = app.as_weak();
    let timer = slint::Timer::default();
    timer.start(slint::TimerMode::Repeated, Duration::from_millis(150), move || {
        let Some(app) = app_weak.upgrade() else { return };
        while let Ok(result) = rx.try_recv() {
            // Stale results (from a cluster the user has since navigated
            // away from) are dropped rather than applied.
            if result.cluster_name != app.get_status_active_cluster().as_str() {
                continue;
            }
            apply_result(&app, &node_rows, &allocation_rows, result);
        }
    });
    std::mem::forget(timer);
}

fn spawn_fetch(manager: &ClusterManager, cluster_name: &str, tx: mpsc::Sender<StatusResult>) {
    let Some(client) = manager.get_client(cluster_name) else {
        return;
    };
    let kibana_host = manager
        .clusters()
        .into_iter()
        .find(|c| c.name == cluster_name)
        .map(|c| c.kibana_host)
        .filter(|h| !h.is_empty());
    let cluster_name = cluster_name.to_string();

    tokio::spawn(async move {
        // Fire all five requests concurrently rather than chaining .await
        // sequentially — five sequential 10s-timeout calls against a fully
        // unreachable cluster would otherwise take up to 50s before this
        // screen shows anything.
        let (health, stats, nodes, allocation, es_version, kibana_version) = tokio::join!(
            client.cluster_health(),
            client.cluster_stats(),
            client.cat_nodes(),
            client.cat_allocation(),
            client.get_es_version(),
            async {
                match &kibana_host {
                    Some(host) => Some(client.get_kibana_version(host).await),
                    None => None,
                }
            }
        );
        let health = health.map_err(|e| e.to_string());
        let stats = stats.map_err(|e| e.to_string());
        let nodes = nodes.map_err(|e| e.to_string());
        let allocation = allocation.map_err(|e| e.to_string());
        let es_version = es_version.map_err(|e| e.to_string());
        let kibana_version = kibana_version.map(|r| r.map_err(|e| e.to_string()));
        let _ = tx.send(StatusResult {
            cluster_name,
            health,
            stats,
            nodes,
            allocation,
            es_version,
            kibana_version,
        });
    });
}

fn apply_result(
    app: &AppShell,
    node_rows: &VecModel<NodeRow>,
    allocation_rows: &VecModel<AllocationRow>,
    result: StatusResult,
) {
    let health = match result.health {
        Ok(h) => h,
        Err(e) => {
            tracing::warn!("Status: health check failed for '{}': {}", result.cluster_name, e);
            app.set_status_error(e.into());
            app.set_status_connected(false);
            return;
        }
    };

    app.set_status_connected(true);
    app.set_status_error("".into());
    app.set_status_status(health.status.clone().into());
    app.set_status_nodes(health.number_of_nodes as i32);
    app.set_status_data_nodes(health.number_of_data_nodes as i32);
    app.set_status_active_shards(health.active_shards as i32);
    app.set_status_primary_shards(health.active_primary_shards as i32);
    app.set_status_unassigned_shards(health.unassigned_shards as i32);

    app.set_status_es_version(result.es_version.unwrap_or_default().into());
    if let Some(kb) = result.kibana_version {
        app.set_status_kibana_version(kb.unwrap_or_default().into());
    } else {
        app.set_status_kibana_version("".into());
    }

    match result.stats {
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
            app.set_status_docs_human(docs.into());
            app.set_status_store_human(store.into());
        }
        Err(e) => {
            tracing::warn!("Status: cluster stats failed for '{}': {}", result.cluster_name, e);
            app.set_status_docs_human("—".into());
            app.set_status_store_human("—".into());
        }
    }

    match result.nodes {
        Ok(nodes) => {
            let rows: Vec<NodeRow> = nodes
                .iter()
                .map(|n| {
                    let cpu = n.cpu.as_deref().unwrap_or("0");
                    let ram = n.ram_percent.as_deref().unwrap_or("0");
                    let heap = n.heap_percent.as_deref().unwrap_or("0");
                    NodeRow {
                        name: n.name.clone().unwrap_or_else(|| "Unknown".to_string()).into(),
                        role: n.role.clone().unwrap_or_else(|| "—".to_string()).into(),
                        ip: n.ip.clone().unwrap_or_else(|| "—".to_string()).into(),
                        is_master: n.master.as_deref() == Some("*"),
                        cpu_ratio: cpu.parse::<f32>().unwrap_or(0.0) / 100.0,
                        cpu_label: format!("{}%", cpu).into(),
                        ram_ratio: ram.parse::<f32>().unwrap_or(0.0) / 100.0,
                        ram_label: format!("{}%", ram).into(),
                        heap_ratio: heap.parse::<f32>().unwrap_or(0.0) / 100.0,
                        heap_label: format!("{}%", heap).into(),
                    }
                })
                .collect();
            node_rows.set_vec(rows);
        }
        Err(e) => {
            tracing::warn!("Status: cat_nodes failed for '{}': {}", result.cluster_name, e);
            node_rows.set_vec(Vec::new());
        }
    }

    match result.allocation {
        Ok(allocs) => {
            let rows: Vec<AllocationRow> = allocs
                .iter()
                .map(|a| {
                    let unassigned = a.node.as_deref().unwrap_or("UNASSIGNED") == "UNASSIGNED";
                    let disk_percent = a.disk_percent.as_deref().unwrap_or("0");
                    AllocationRow {
                        node: a.node.clone().unwrap_or_else(|| "UNASSIGNED".to_string()).into(),
                        shards: a.shards.clone().unwrap_or_else(|| "0".to_string()).into(),
                        disk_avail: a.disk_avail.clone().unwrap_or_else(|| "—".to_string()).into(),
                        disk_total: a.disk_total.clone().unwrap_or_else(|| "—".to_string()).into(),
                        disk_ratio: if unassigned {
                            0.0
                        } else {
                            disk_percent.parse::<f32>().unwrap_or(0.0) / 100.0
                        },
                        disk_label: if unassigned {
                            "—".to_string().into()
                        } else {
                            format!("{}%", disk_percent).into()
                        },
                        unassigned,
                    }
                })
                .collect();
            allocation_rows.set_vec(rows);
        }
        Err(e) => {
            tracing::warn!("Status: cat_allocation failed for '{}': {}", result.cluster_name, e);
            allocation_rows.set_vec(Vec::new());
        }
    }
}

fn current_focused_names(clusters: &VecModel<ClusterItem>) -> Vec<String> {
    (0..clusters.row_count())
        .filter_map(|i| clusters.row_data(i))
        .filter(|c| c.checked)
        .map(|c| c.name.to_string())
        .collect()
}
