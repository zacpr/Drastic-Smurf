use slint::{Model, ModelRc, VecModel};
use std::rc::Rc;

use crate::core::cluster_manager::ClusterManager;

slint::include_modules!();

/// Launches the Slint persistent shell (alert bar + sidebar) against the
/// real ClusterManager/EsClient data, per the egui->Slint retrofit build
/// order step 4.
pub fn run() -> Result<(), slint::PlatformError> {
    let manager = ClusterManager::new();
    if let Err(e) = manager.load() {
        tracing::warn!("Failed to load config: {}", e);
    }

    let app = AppShell::new()?;

    // No persisted "focused" selection exists yet (the old app only has a
    // free-text cluster_filter, not per-cluster checkboxes) — default every
    // configured cluster to focused/visible.
    let clusters = Rc::new(VecModel::from(
        manager
            .clusters()
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

    let alerts = Rc::new(VecModel::from(Vec::<ClusterAlert>::new()));
    app.set_alerts(ModelRc::from(alerts));
    app.set_alerting(false);

    let clusters_for_toggle = clusters.clone();
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
        if let Some(app) = app_weak.upgrade() {
            app.set_focused_checked_count(count_checked(&clusters_for_toggle));
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

    // Cross-cluster alert bar: fetch health for every configured cluster and
    // surface the ones in RED. Mirrors the egui app's default "red only"
    // condition — per-cluster configurable alert conditions are a Settings
    // feature that doesn't exist yet (see theplan.md open questions).
    let manager_for_health = manager.clone();
    let app_weak_health = app.as_weak();
    tokio::spawn(async move {
        let cluster_configs = manager_for_health.clusters();
        let mut red = Vec::new();
        for cluster in cluster_configs {
            let Some(client) = manager_for_health.get_client(&cluster.name) else {
                continue;
            };
            match client.cluster_health().await {
                Ok(health) if health.status.eq_ignore_ascii_case("red") => {
                    red.push(ClusterAlert {
                        name: cluster.name.clone().into(),
                        message: format!(
                            "Status RED — {} unassigned shards",
                            health.unassigned_shards
                        )
                        .into(),
                    });
                }
                Ok(_) => {}
                Err(e) => {
                    tracing::warn!("Health check failed for '{}': {}", cluster.name, e);
                }
            }
        }

        let _ = slint::invoke_from_event_loop(move || {
            if let Some(app) = app_weak_health.upgrade() {
                app.set_alerting(!red.is_empty());
                app.set_alerts(ModelRc::from(Rc::new(VecModel::from(red))));
            }
        });
    });

    app.run()
}

fn count_checked(clusters: &VecModel<ClusterItem>) -> i32 {
    (0..clusters.row_count())
        .filter(|&i| clusters.row_data(i).is_some_and(|c| c.checked))
        .count() as i32
}
