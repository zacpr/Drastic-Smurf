use egui::{Ui, Widget};

use crate::core::config::{ClusterConfig, ClusterData};
use crate::ui::theme::Theme;
use crate::ui::widgets::ConnectionDot;

#[derive(Debug, Clone, Default)]
pub struct ClustersState {
    pub selected_cluster: Option<String>,
    pub editing_cluster: Option<String>,
    pub edit_form: ClusterConfig,
    pub edit_password: String,
    pub test_result: Option<String>,
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

pub fn render_clusters_module(
    ui: &mut Ui,
    state: &mut ClustersState,
    clusters: &[ClusterConfig],
    cluster_data: &std::collections::HashMap<String, ClusterData>,
    health_status: &std::collections::HashMap<String, Option<String>>,
    on_save: &mut Option<(Option<String>, ClusterConfig, String)>,
    on_delete: &mut Option<String>,
    on_test: &mut Option<(String, String)>,
    on_import: &mut Option<crate::core::config::AppConfig>,
    on_show_dialog: &mut bool,
    on_fetch_repos: &mut Option<String>,
    on_fetch_slm: &mut Option<String>,
) {
    ui.heading("Clusters");
    ui.add_space(16.0);

    if clusters.is_empty() {
        ui.label("No clusters configured. Click 'Add Cluster' below to get started.");
        ui.add_space(16.0);
    }

    // --- Action buttons ---
    ui.horizontal(|ui| {
        if ui.button("➕ Add Cluster").clicked() {
            *on_show_dialog = true;
        }
        if ui.button("📥 Import").clicked() {
            state.show_import = !state.show_import;
            state.show_export = false;
        }
        if ui.button("📤 Export").clicked() {
            state.show_export = !state.show_export;
            state.show_import = false;
        }
    });

    ui.add_space(16.0);

    // --- Import section ---
    if state.show_import {
        render_import_section(ui, state, clusters, on_import);
        ui.add_space(16.0);
    }

    // --- Export section ---
    if state.show_export {
        render_export_section(ui, state, clusters, cluster_data);
        ui.add_space(16.0);
    }

    // --- Cluster list ---
    egui::ScrollArea::vertical()
        .id_salt("clusters")
        .show(ui, |ui| {
            for cluster in clusters {
                let is_selected = state.selected_cluster.as_ref() == Some(&cluster.name);
                let _is_editing = state.editing_cluster.as_ref() == Some(&cluster.name)
                    || (state.editing_cluster.is_none() && state.selected_cluster.is_none());

                egui::Frame::new()
                    .fill(Theme::bg_card())
                    .corner_radius(Theme::CARD_ROUNDING)
                    .inner_margin(Theme::CARD_PADDING)
                    .stroke(if is_selected {
                        egui::Stroke::new(1.5, Theme::accent())
                    } else {
                        egui::Stroke::NONE
                    })
                    .show(ui, |ui| {
                        ui.horizontal(|ui| {
                            ui.set_width(ui.available_width());

                            // Selection click area
                            let status_opt = health_status.get(&cluster.name).cloned().flatten();
                            let is_connected = status_opt.is_some();
                            let dot_color = match status_opt.as_deref() {
                                Some("green") => Theme::success(),
                                Some("yellow") => Theme::warning(),
                                Some("red") => Theme::danger(),
                                _ => Theme::text_muted(),
                            };
                            let response = ui
                                .horizontal(|ui| {
                                    ConnectionDot::new(is_connected).color(dot_color).ui(ui);
                                    ui.label(
                                        egui::RichText::new(&cluster.name)
                                            .strong()
                                            .size(14.0)
                                            .color(Theme::text_primary()),
                                    );
                                    ui.label(
                                        egui::RichText::new(format!("@ {}", cluster.host))
                                            .size(12.0)
                                            .color(Theme::text_muted()),
                                    );
                                })
                                .response;

                            if response.clicked() {
                                state.selected_cluster = Some(cluster.name.clone());
                                state.editing_cluster = None;
                                state.test_result = None;
                            }

                            ui.with_layout(
                                egui::Layout::right_to_left(egui::Align::Center),
                                |ui| {
                                    if ui.small_button("🗑").clicked() {
                                        *on_delete = Some(cluster.name.clone());
                                    }
                                    if ui.small_button("✏️").clicked() {
                                        state.selected_cluster = Some(cluster.name.clone());
                                        state.editing_cluster = Some(cluster.name.clone());
                                        state.edit_form = cluster.clone();
                                        state.edit_password =
                                            crate::core::auth::get_password(&cluster.name)
                                                .ok()
                                                .flatten()
                                                .unwrap_or_default();
                                        state.test_result = None;
                                    }
                                    if ui.small_button("🔌 Test").clicked() {
                                        let pwd = crate::core::auth::get_password(&cluster.name)
                                            .ok()
                                            .flatten()
                                            .unwrap_or_default();
                                        *on_test = Some((cluster.name.clone(), pwd));
                                    }
                                },
                            );
                        });

                        // Data summary
                        if let Some(data) = cluster_data.get(&cluster.name) {
                            let mut summaries = Vec::new();
                            if !data.saved_queries.is_empty() {
                                summaries.push(format!("{} queries", data.saved_queries.len()));
                            }
                            if !data.status_history.is_empty() {
                                summaries.push(format!(
                                    "{} status snapshots",
                                    data.status_history.len()
                                ));
                            }
                            if !data.tasks_cache.is_empty() {
                                summaries.push(format!("{} task caches", data.tasks_cache.len()));
                            }
                            if !data.snapshot_cache.is_empty() {
                                summaries
                                    .push(format!("{} snapshot caches", data.snapshot_cache.len()));
                            }
                            if !summaries.is_empty() {
                                ui.label(
                                    egui::RichText::new(summaries.join(" · "))
                                        .size(11.0)
                                        .color(Theme::text_muted()),
                                );
                            }
                        }

                        // Edit form inline
                        if is_selected && state.editing_cluster.is_some() {
                            ui.add_space(8.0);
                            ui.separator();
                            ui.add_space(8.0);
                            render_edit_form(ui, state, on_save, on_fetch_repos, on_fetch_slm);
                        }
                    });

                ui.add_space(8.0);
            }

            // Add new cluster form (when no cluster selected and not editing)
            if state.editing_cluster.is_none() && state.selected_cluster.is_none() {
                egui::Frame::new()
                    .fill(Theme::bg_card())
                    .corner_radius(Theme::CARD_ROUNDING)
                    .inner_margin(Theme::CARD_PADDING)
                    .show(ui, |ui| {
                        ui.label(
                            egui::RichText::new("New Cluster")
                                .strong()
                                .size(14.0)
                                .color(Theme::text_primary()),
                        );
                        ui.add_space(8.0);
                        render_edit_form(ui, state, on_save, on_fetch_repos, on_fetch_slm);
                    });
            }
        });
}

fn render_edit_form(
    ui: &mut Ui,
    state: &mut ClustersState,
    on_save: &mut Option<(Option<String>, ClusterConfig, String)>,
    on_fetch_repos: &mut Option<String>,
    on_fetch_slm: &mut Option<String>,
) {
    let form = &mut state.edit_form;

    ui.horizontal(|ui| {
        ui.label("Name:");
        ui.text_edit_singleline(&mut form.name);
    });
    ui.horizontal(|ui| {
        ui.label("Host:");
        ui.text_edit_singleline(&mut form.host);
    });
    ui.horizontal(|ui| {
        ui.label("Username:");
        ui.text_edit_singleline(&mut form.username);
    });
    ui.horizontal(|ui| {
        ui.label("Password:");
        ui.add(egui::TextEdit::singleline(&mut state.edit_password).password(true));
    });
    ui.horizontal(|ui| {
        ui.label("Snapshot Repo(s):");
        ui.text_edit_singleline(&mut form.snapshot_repo);
        if ui.button("🔍 Fetch").clicked() {
            *on_fetch_repos = Some(state.selected_cluster.clone().unwrap_or_default());
        }
    });
    if !state.fetched_repos.is_empty() {
        ui.indent("repo_indent", |ui| {
            ui.label(
                egui::RichText::new("Available repos (select one or more):")
                    .size(11.0)
                    .color(Theme::text_muted()),
            );
            let mut current_repos: Vec<String> = form.snapshot_repo
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
            *on_fetch_slm = Some(state.selected_cluster.clone().unwrap_or_default());
        }
    });
    ui.horizontal(|ui| {
        ui.label("Kibana Host:");
        ui.text_edit_singleline(&mut form.kibana_host);
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
    });

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
        if ui.button("📂 Load").clicked() {
            if !state.ca_cert_import_path.is_empty() {
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
        let is_success = result.contains("Connected")
            || result.contains("connected")
            || result.contains("Fetched")
            || result.contains("fetched")
            || result.contains("successful")
            || result.contains("Success");
        let color = if is_success {
            Theme::success()
        } else {
            Theme::danger()
        };
        ui.label(egui::RichText::new(result).color(color).size(12.0));
    }

    ui.add_space(8.0);
    ui.horizontal(|ui| {
        if ui.button("💾 Save").clicked() {
            let old_name = state.editing_cluster.clone();
            *on_save = Some((old_name, form.clone(), state.edit_password.clone()));
            state.editing_cluster = None;
            state.selected_cluster = Some(form.name.clone());
            state.test_result = None;
        }
        if ui.button("Cancel").clicked() {
            state.editing_cluster = None;
            state.selected_cluster = None;
            state.test_result = None;
        }
    });
}

fn render_import_section(
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

fn render_export_section(
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
