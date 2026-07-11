use slint::{Model, ModelRc, VecModel};
use std::rc::Rc;

slint::include_modules!();

/// Launches the Slint persistent shell (alert bar + sidebar) against dummy
/// data, per the egui->Slint retrofit build order step 3. Not wired to
/// ClusterManager/EsClient yet.
pub fn run() -> Result<(), slint::PlatformError> {
    let app = AppShell::new()?;

    let clusters = Rc::new(VecModel::from(vec![
        ClusterItem { name: "prod-eu".into(), checked: true },
        ClusterItem { name: "prod-us".into(), checked: true },
        ClusterItem { name: "staging".into(), checked: true },
        ClusterItem { name: "dev-local".into(), checked: true },
        ClusterItem { name: "homelab".into(), checked: false },
        ClusterItem { name: "qa".into(), checked: true },
        ClusterItem { name: "analytics".into(), checked: true },
        ClusterItem { name: "logging".into(), checked: false },
    ]));
    app.set_clusters(ModelRc::from(clusters.clone()));
    app.set_focused_checked_count(count_checked(&clusters));

    let alerts = Rc::new(VecModel::from(vec![ClusterAlert {
        name: "prod-eu".into(),
        message: "Status RED — 3 unassigned shards".into(),
    }]));
    app.set_alerts(ModelRc::from(alerts));
    app.set_alerting(true);

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

    app.on_filter_changed(|_text| {
        // Dummy shell: filtering isn't wired to real data yet.
    });

    app.run()
}

fn count_checked(clusters: &VecModel<ClusterItem>) -> i32 {
    (0..clusters.row_count())
        .filter(|&i| clusters.row_data(i).is_some_and(|c| c.checked))
        .count() as i32
}
