use egui::Ui;

use crate::core::config::{AuthMethod, ClusterConfig, ClusterData};
use crate::ui::theme::Theme;

#[derive(Debug, Clone, Default)]
pub struct ClustersState {
    pub selected_cluster: Option<String>,
    pub editing_cluster: Option<String>,
    pub edit_form: ClusterConfig,
    pub edit_password: String,
    pub test_result: Option<String>,
    /// The cluster whose "🔌 Test" produced `test_result`.
    pub tested_cluster: Option<String>,
    pub show_import: bool,
    pub import_path: String,
    pub import_include_data: bool,
    pub import_error: Option<String>,
    pub show_export: bool,
    pub export_path: String,
    pub export_include_queries: bool,
    pub export_include_status: bool,
    pub export_include_tasks: bool,
    pub export_include_snapshots: bool,
    pub export_error: Option<String>,
    pub export_success: Option<String>,
    pub fetched_repos: Vec<String>,
    pub fetched_slm_policies: Vec<String>,
    pub ca_cert_import_path: String,
}

/// Shared UI fields for a cluster's enabled state and Logs/Metrics configuration.
pub fn render_telemetry_config_fields(
    ui: &mut Ui,
    form: &mut ClusterConfig,
    all_cluster_names: &[String],
    id_salt: &str,
) {
    ui.checkbox(
        &mut form.enabled,
        "Enabled (auto-refresh status, snapshots & tasks)",
    );

    ui.horizontal(|ui| {
        ui.label("Logs/Metrics Cluster:");
        let selected_label = if form.logs_metrics_cluster.trim().is_empty() {
            "This cluster (default)".to_string()
        } else {
            form.logs_metrics_cluster.clone()
        };
        egui::ComboBox::from_id_salt((id_salt, "logs_metrics_cluster"))
            .selected_text(selected_label)
            .width(220.0)
            .show_ui(ui, |ui| {
                ui.selectable_value(
                    &mut form.logs_metrics_cluster,
                    String::new(),
                    "This cluster (default)",
                );
                for name in all_cluster_names {
                    ui.selectable_value(&mut form.logs_metrics_cluster, name.clone(), name);
                }
            });
    });

    ui.horizontal(|ui| {
        ui.label("Logs Index Pattern:");
        ui.add(
            egui::TextEdit::singleline(&mut form.logs_index_pattern)
                .hint_text("logs-elasticsearch.*,logs-kibana.*")
                .desired_width(260.0),
        );
    });

    ui.horizontal(|ui| {
        ui.label("Metrics Index Pattern:");
        ui.add(
            egui::TextEdit::singleline(&mut form.metrics_index_pattern)
                .hint_text("metrics-elasticsearch.*,metrics-kibana.*")
                .desired_width(260.0),
        );
    });
}

#[allow(clippy::too_many_arguments)]
pub fn render_edit_form(
    ui: &mut Ui,
    state: &mut ClustersState,
    all_cluster_names: &[String],
    on_save: &mut Option<(Option<String>, ClusterConfig, String)>,
    on_test: &mut Option<(ClusterConfig, String)>,
    on_fetch_repos: &mut Option<String>,
    on_fetch_slm: &mut Option<String>,
) {
    let form = &mut state.edit_form;
    let target_for_fetch = state
        .editing_cluster
        .clone()
        .or_else(|| state.selected_cluster.clone())
        .unwrap_or_else(|| form.name.clone());

    ui.horizontal(|ui| {
        ui.label("Name:");
        ui.text_edit_singleline(&mut form.name);
        crate::ui::widgets::required_tag(ui, "Required");
    });
    ui.horizontal(|ui| {
        ui.label("Host:");
        ui.text_edit_singleline(&mut form.host);
        crate::ui::widgets::required_tag(ui, "Required");
    });
    let credentials_valid = render_auth_fields(ui, form, &mut state.edit_password);
    ui.horizontal(|ui| {
        ui.label("Snapshot Repo(s):");
        ui.text_edit_singleline(&mut form.snapshot_repo);
        if ui.button("🔍 Fetch").clicked() {
            *on_fetch_repos = Some(target_for_fetch.clone());
        }
    });
    if !state.fetched_repos.is_empty() {
        ui.indent("repo_indent", |ui| {
            ui.label(
                egui::RichText::new("Available repos (select one or more):")
                    .size(11.0)
                    .color(Theme::text_muted()),
            );
            let mut current_repos: Vec<String> = form
                .snapshot_repo
                .split(',')
                .map(|s| s.trim().to_string())
                .filter(|s| !s.is_empty())
                .collect();
            for repo in state.fetched_repos.clone() {
                let is_selected = current_repos.contains(&repo);
                let mut checkbox = is_selected;
                if ui.checkbox(&mut checkbox, &repo).clicked() {
                    if checkbox {
                        if !is_selected {
                            current_repos.push(repo);
                        }
                    } else {
                        current_repos.retain(|r| r != &repo);
                    }
                    form.snapshot_repo = current_repos.join(", ");
                }
            }
        });
    }
    ui.horizontal(|ui| {
        ui.label("SLM Policy:");
        ui.text_edit_singleline(&mut form.slm_policy);
        if ui.button("🔍 Fetch").clicked() {
            *on_fetch_slm = Some(target_for_fetch);
        }
    });
    ui.horizontal(|ui| {
        ui.label("Kibana Host:");
        ui.text_edit_singleline(&mut form.kibana_host);
        crate::ui::widgets::optional_tag(ui);
    });
    if !state.fetched_slm_policies.is_empty() {
        ui.indent("slm_indent", |ui| {
            ui.label(
                egui::RichText::new("Available policies:")
                    .size(11.0)
                    .color(Theme::text_muted()),
            );
            for policy in state.fetched_slm_policies.clone() {
                if ui
                    .selectable_label(policy == form.slm_policy, &policy)
                    .clicked()
                {
                    form.slm_policy = policy;
                }
            }
        });
    }
    ui.horizontal(|ui| {
        ui.label("HAProxy Host:");
        ui.text_edit_singleline(&mut form.haproxy_host);
        crate::ui::widgets::optional_tag(ui);
    });

    ui.add_space(4.0);
    render_telemetry_config_fields(ui, form, all_cluster_names, "edit_cluster_form");

    ui.add_space(4.0);
    ui.horizontal(|ui| {
        ui.label(egui::RichText::new("Custom Links:").strong().size(12.0));
        if ui.button("➕ Add Link").clicked() {
            form.custom_links.push((String::new(), String::new()));
        }
    });

    let mut link_to_remove = None;
    for (idx, (name, url)) in form.custom_links.iter_mut().enumerate() {
        ui.horizontal(|ui| {
            ui.label("Name:");
            ui.text_edit_singleline(name);
            ui.label("URL:");
            ui.text_edit_singleline(url);
            if ui.button("🗑").clicked() {
                link_to_remove = Some(idx);
            }
        });
    }
    if let Some(idx) = link_to_remove {
        form.custom_links.remove(idx);
    }

    ui.add_space(4.0);
    ui.label(
        egui::RichText::new("CA Certificate (PEM format):")
            .strong()
            .size(12.0),
    );
    ui.add(
        egui::TextEdit::multiline(&mut form.ca_cert_pem)
            .font(egui::TextStyle::Monospace)
            .desired_rows(4)
            .desired_width(ui.available_width())
            .hint_text("-----BEGIN CERTIFICATE-----\n...\n-----END CERTIFICATE-----"),
    );

    ui.horizontal(|ui| {
        ui.label("Import from File:");
        ui.text_edit_singleline(&mut state.ca_cert_import_path);
        if ui.button("📂 Load").clicked() && !state.ca_cert_import_path.is_empty() {
            match std::fs::read_to_string(&state.ca_cert_import_path) {
                Ok(pem) => {
                    form.ca_cert_pem = pem;
                    state.ca_cert_import_path.clear();
                }
                Err(e) => {
                    state.test_result = Some(format!("Failed to load PEM file: {}", e));
                }
            }
        }
    });
    ui.add_space(4.0);

    ui.horizontal(|ui| {
        ui.checkbox(&mut form.verify_ssl, "Verify SSL");
    });
    ui.horizontal(|ui| {
        ui.checkbox(&mut form.ssh_tunnel, "SSH Tunnel");
    });

    if form.ssh_tunnel {
        ui.horizontal(|ui| {
            ui.label("SSH Host:");
            ui.text_edit_singleline(&mut form.ssh_host);
        });
        ui.horizontal(|ui| {
            ui.label("SSH User:");
            ui.text_edit_singleline(&mut form.ssh_user);
        });
        ui.horizontal(|ui| {
            ui.label("SSH Port:");
            let mut port_str = form.ssh_port.to_string();
            ui.text_edit_singleline(&mut port_str);
            if let Ok(p) = port_str.parse::<u16>() {
                form.ssh_port = p;
            }
        });
    }

    if let Some(ref result) = state.test_result {
        ui.add_space(4.0);
        render_test_result(ui, result);
    }

    ui.add_space(8.0);
    let name_valid = !form.name.trim().is_empty() && !form.host.trim().is_empty();
    ui.horizontal(|ui| {
        if ui
            .add_enabled(credentials_valid, egui::Button::new("🔌 Test Connection"))
            .clicked()
        {
            *on_test = Some((form.clone(), state.edit_password.clone()));
            state.tested_cluster = state
                .editing_cluster
                .clone()
                .or_else(|| Some(form.name.clone()));
        }
        if ui
            .add_enabled(
                credentials_valid && name_valid,
                egui::Button::new("💾 Save"),
            )
            .clicked()
        {
            let old_name = state.editing_cluster.clone();
            let mut updated = form.clone();
            updated.name = updated.name.trim().to_string();
            *on_save = Some((old_name, updated.clone(), state.edit_password.clone()));
            state.editing_cluster = None;
            state.selected_cluster = Some(updated.name);
            state.test_result = None;
        }
        if ui.button("Cancel").clicked() {
            state.editing_cluster = None;
            state.selected_cluster = None;
            state.test_result = None;
        }
    });
}

pub fn render_auth_fields(
    ui: &mut Ui,
    config: &mut ClusterConfig,
    credential: &mut String,
) -> bool {
    let previous = config.auth_method;
    ui.horizontal(|ui| {
        ui.label("Authentication:");
        ui.selectable_value(
            &mut config.auth_method,
            AuthMethod::Basic,
            AuthMethod::Basic.label(),
        );
        ui.selectable_value(
            &mut config.auth_method,
            AuthMethod::ApiKey,
            AuthMethod::ApiKey.label(),
        );
    });
    if config.auth_method != previous {
        credential.clear();
    }
    if config.auth_method == AuthMethod::Basic {
        ui.horizontal(|ui| {
            ui.label("Username:");
            ui.text_edit_singleline(&mut config.username);
            crate::ui::widgets::required_tag(ui, "Required");
        });
    }
    ui.horizontal(|ui| {
        let is_key = config.auth_method == AuthMethod::ApiKey;
        let (label, tag) = crate::ui::widgets::secret_labels(config.auth_method);
        ui.label(label);
        ui.add(
            egui::TextEdit::singleline(credential)
                .password(true)
                .hint_text(if is_key {
                    "Encoded key or id:api_key"
                } else {
                    "Password"
                }),
        );
        crate::ui::widgets::required_tag(ui, tag);
    });
    config.auth_method == AuthMethod::Basic || !credential.trim().is_empty()
}

/// A connection test or fetch result, colored by its first line. Long output (SSH logs)
/// scrolls inside a bounded box instead of growing the page or dialog.
pub fn render_test_result(ui: &mut Ui, result: &str) {
    let headline = result.lines().next().unwrap_or_default();
    let is_success = [
        "Connected",
        "connected",
        "Fetched",
        "fetched",
        "successful",
        "Success",
    ]
    .iter()
    .any(|w| headline.contains(w));
    let color = if result.starts_with("Testing") || result.starts_with("Fetching") {
        Theme::text_muted()
    } else if is_success {
        Theme::success()
    } else {
        Theme::danger()
    };
    egui::ScrollArea::vertical()
        .id_salt("cluster_test_result")
        .max_height(260.0)
        .auto_shrink([false, true])
        .show(ui, |ui| {
            ui.label(egui::RichText::new(result).color(color).size(12.0));
        });
}

pub fn render_import_section(
    ui: &mut Ui,
    state: &mut ClustersState,
    existing_clusters: &[ClusterConfig],
    on_import: &mut Option<crate::core::config::AppConfig>,
) {
    egui::Frame::new()
        .fill(Theme::bg_card())
        .corner_radius(Theme::CARD_ROUNDING)
        .inner_margin(Theme::CARD_PADDING)
        .show(ui, |ui| {
            ui.heading("Import Clusters");
            ui.add_space(8.0);

            ui.horizontal(|ui| {
                ui.label("Path:");
                ui.text_edit_singleline(&mut state.import_path);
            });
            ui.checkbox(
                &mut state.import_include_data,
                "Include module data (queries, history, cache)",
            );

            if let Some(ref err) = state.import_error {
                ui.label(egui::RichText::new(err).color(Theme::danger()));
            }

            ui.add_space(8.0);
            if ui.button("Import").clicked() {
                state.import_error = None;
                match perform_import(state, existing_clusters) {
                    Ok(config) => {
                        *on_import = Some(config);
                        state.import_error = Some(format!(
                            "Imported {} cluster(s).",
                            on_import.as_ref().map(|c| c.clusters.len()).unwrap_or(0)
                        ));
                        state.import_path.clear();
                    }
                    Err(e) => {
                        state.import_error = Some(format!("Import failed: {}", e));
                    }
                }
            }
        });
}

fn perform_import(
    state: &ClustersState,
    existing_clusters: &[ClusterConfig],
) -> anyhow::Result<crate::core::config::AppConfig> {
    let contents = std::fs::read_to_string(&state.import_path)?;
    let mut imported: crate::core::config::AppConfig = serde_json::from_str(&contents)?;

    let existing_names: std::collections::HashSet<String> =
        existing_clusters.iter().map(|c| c.name.clone()).collect();

    // Filter out duplicates
    imported
        .clusters
        .retain(|c| !existing_names.contains(&c.name));
    imported
        .cluster_data
        .retain(|name, _| !existing_names.contains(name));

    // If user doesn't want module data, clear it
    if !state.import_include_data {
        imported.cluster_data.clear();
    }

    Ok(imported)
}

pub fn render_export_section(
    ui: &mut Ui,
    state: &mut ClustersState,
    clusters: &[ClusterConfig],
    cluster_data: &std::collections::HashMap<String, ClusterData>,
) {
    egui::Frame::new()
        .fill(Theme::bg_card())
        .corner_radius(Theme::CARD_ROUNDING)
        .inner_margin(Theme::CARD_PADDING)
        .show(ui, |ui| {
            ui.heading("Export Clusters");
            ui.add_space(8.0);

            ui.horizontal(|ui| {
                ui.label("Path:");
                ui.text_edit_singleline(&mut state.export_path);
            });

            ui.label("Include:");
            ui.checkbox(&mut state.export_include_queries, "Saved queries");
            ui.checkbox(&mut state.export_include_status, "Status history");
            ui.checkbox(&mut state.export_include_tasks, "Tasks cache");
            ui.checkbox(&mut state.export_include_snapshots, "Snapshot cache");

            if let Some(ref err) = state.export_error {
                ui.label(egui::RichText::new(err).color(Theme::danger()));
            }
            if let Some(ref success) = state.export_success {
                ui.label(egui::RichText::new(success).color(Theme::success()));
            }

            ui.add_space(8.0);
            if ui.button("Export All").clicked() {
                state.export_error = None;
                state.export_success = None;
                match perform_export(state, clusters, cluster_data) {
                    Ok(()) => {
                        state.export_success = Some(format!(
                            "Exported {} cluster(s) to {}",
                            clusters.len(),
                            state.export_path
                        ));
                    }
                    Err(e) => {
                        state.export_error = Some(format!("Export failed: {}", e));
                    }
                }
            }
        });
}

fn perform_export(
    state: &ClustersState,
    clusters: &[ClusterConfig],
    cluster_data: &std::collections::HashMap<String, ClusterData>,
) -> anyhow::Result<()> {
    let mut export_data = crate::core::config::AppConfig {
        clusters: clusters.to_vec(),
        cluster_data: std::collections::HashMap::new(),
        auto_refresh: true,
        refresh_interval_secs: 15,
        theme: crate::ui::theme::AppTheme::default(),
        vfx: crate::core::config::VfxSettings::default(),
        ..Default::default()
    };

    for cluster in clusters {
        if let Some(data) = cluster_data.get(&cluster.name) {
            let mut filtered = ClusterData::default();
            if state.export_include_queries {
                filtered.saved_queries = data.saved_queries.clone();
            }
            if state.export_include_status {
                filtered.status_history = data.status_history.clone();
            }
            if state.export_include_tasks {
                filtered.tasks_cache = data.tasks_cache.clone();
            }
            if state.export_include_snapshots {
                filtered.snapshot_cache = data.snapshot_cache.clone();
            }
            export_data
                .cluster_data
                .insert(cluster.name.clone(), filtered);
        }
    }

    let contents = serde_json::to_string_pretty(&export_data)?;
    std::fs::write(&state.export_path, contents)?;
    Ok(())
}
