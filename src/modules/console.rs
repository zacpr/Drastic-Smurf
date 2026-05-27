use egui::Ui;

use crate::core::config::SavedQuery;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum ConsoleTab {
    #[default]
    Presets,
    Variables,
    History,
    Saved,
}

#[derive(Debug, Clone, Default)]
pub struct ConsoleState {
    pub selected_cluster: String,
    pub last_selected_cluster: String, // Tracks selected cluster to sync variables
    pub method: String,
    pub path: String,
    pub body: String,
    pub response: String,
    pub history: Vec<(String, String, String, String)>,
    pub history_index: Option<usize>,
    pub is_loading: bool,
    pub saved_queries: Vec<SavedQuery>,
    pub query_name_input: String,
    pub show_save_dialog: bool,
    pub use_kibana_host: bool,
    pub variables: Vec<(String, String)>,
    pub variables_changed: bool,
    pub active_tab: ConsoleTab,
    pub body_height: Option<f32>,
    pub json_error: Option<String>,
}

impl ConsoleState {
    pub fn new() -> Self {
        Self {
            method: "GET".to_string(),
            path: "/_cluster/health".to_string(),
            body_height: Some(250.0),
            ..Default::default()
        }
    }
}

pub struct ConsolePreset {
    pub category: &'static str,
    pub name: &'static str,
    pub method: &'static str,
    pub path: &'static str,
    pub body: Option<&'static str>,
    pub description: &'static str,
    pub use_kibana: bool,
}

const PRESETS: &[ConsolePreset] = &[
    // --- Cluster Status & Info ---
    ConsolePreset {
        category: "Cluster Status & Info",
        name: "Welcome / ES Version",
        method: "GET",
        path: "/",
        body: None,
        description: "Fetch basic cluster version information, build info, and tagline.",
        use_kibana: false,
    },
    ConsolePreset {
        category: "Cluster Status & Info",
        name: "Cluster Health",
        method: "GET",
        path: "/_cluster/health?pretty",
        body: None,
        description: "Get cluster status (green/yellow/red), node counts, and shard counts.",
        use_kibana: false,
    },
    ConsolePreset {
        category: "Cluster Status & Info",
        name: "Cluster Stats",
        method: "GET",
        path: "/_cluster/stats?pretty",
        body: None,
        description: "Retrieve comprehensive JVM memory, disk size, CPU usage, and index statistics.",
        use_kibana: false,
    },
    ConsolePreset {
        category: "Cluster Status & Info",
        name: "Pending Cluster Tasks",
        method: "GET",
        path: "/_cluster/pending_tasks?pretty",
        body: None,
        description: "Retrieve cluster level tasks that have not yet run (e.g. index creations, mapping updates).",
        use_kibana: false,
    },
    // --- Nodes & Allocation ---
    ConsolePreset {
        category: "Nodes & Allocation",
        name: "List Nodes (Verbose)",
        method: "GET",
        path: "/_cat/nodes?v&h=id,ip,port,name,role,master,cpu,ram.percent,heap.percent,disk.used_percent",
        body: None,
        description: "List all nodes in the cluster with their roles, resource usage, and master status.",
        use_kibana: false,
    },
    ConsolePreset {
        category: "Nodes & Allocation",
        name: "List Shard Allocation",
        method: "GET",
        path: "/_cat/allocation?v",
        body: None,
        description: "View the number of shards assigned to each node and their disk usage.",
        use_kibana: false,
    },
    ConsolePreset {
        category: "Nodes & Allocation",
        name: "Explain Shard Allocation",
        method: "GET",
        path: "/_cluster/allocation/explain?pretty",
        body: None,
        description: "Explain why a shard is unassigned or why it is remaining on its current node.",
        use_kibana: false,
    },
    ConsolePreset {
        category: "Nodes & Allocation",
        name: "Cat Shards (Unassigned)",
        method: "GET",
        path: "/_cat/shards?v&h=index,shard,prirep,state,unassigned.reason&s=state",
        body: None,
        description: "List shards sorted by state, helping identify unassigned or recovering shards.",
        use_kibana: false,
    },
    // --- Indices & Templates ---
    ConsolePreset {
        category: "Indices & Templates",
        name: "List Indices by Name",
        method: "GET",
        path: "/_cat/indices?v&s=index",
        body: None,
        description: "List all indices in the cluster sorted alphabetically.",
        use_kibana: false,
    },
    ConsolePreset {
        category: "Indices & Templates",
        name: "List Indices by Size",
        method: "GET",
        path: "/_cat/indices?v&s=store.size:desc",
        body: None,
        description: "List all indices sorted by disk space consumed (descending).",
        use_kibana: false,
    },
    ConsolePreset {
        category: "Indices & Templates",
        name: "Create Index Template",
        method: "PUT",
        path: "/{{index}}",
        body: Some(r#"{
  "settings": {
    "index": {
      "number_of_shards": 2,
      "number_of_replicas": 1
    }
  },
  "mappings": {
    "properties": {
      "@timestamp": { "type": "date" },
      "message": { "type": "text" },
      "status": { "type": "keyword" }
    }
  }
}"#),
        description: "Create or replace an index with custom shards, replicas, and mappings configuration.",
        use_kibana: false,
    },
    ConsolePreset {
        category: "Indices & Templates",
        name: "Get Index Mappings",
        method: "GET",
        path: "/{{index}}/_mapping?pretty",
        body: None,
        description: "Retrieve mapping definitions for one or more indices.",
        use_kibana: false,
    },
    ConsolePreset {
        category: "Indices & Templates",
        name: "Delete Index",
        method: "DELETE",
        path: "/{{index}}",
        body: None,
        description: "Delete an index and all associated shard data permanently. Danger: non-reversible!",
        use_kibana: false,
    },
    ConsolePreset {
        category: "Indices & Templates",
        name: "Search Template",
        method: "POST",
        path: "/{{index}}/_search",
        body: Some(r#"{
  "query": {
    "match_all": {}
  },
  "size": 10
}"#),
        description: "A standard match_all search query template.",
        use_kibana: false,
    },
    ConsolePreset {
        category: "Indices & Templates",
        name: "Index Recovery Status",
        method: "GET",
        path: "/_cat/recovery?v&active_only=true",
        body: None,
        description: "Check the status of active index recoveries (e.g. shard relocations or restores).",
        use_kibana: false,
    },
    // --- Search & Analytics ---
    ConsolePreset {
        category: "Search & Analytics",
        name: "Terms Aggregation Query",
        method: "POST",
        path: "/{{index}}/_search",
        body: Some(r#"{
  "size": 0,
  "aggs": {
    "by_status": {
      "terms": {
        "field": "status",
        "size": 10
      }
    }
  }
}"#),
        description: "Perform a terms aggregation to count occurrence frequencies of keyword fields.",
        use_kibana: false,
    },
    ConsolePreset {
        category: "Search & Analytics",
        name: "Date Histogram Query",
        method: "POST",
        path: "/{{index}}/_search",
        body: Some(r#"{
  "size": 0,
  "aggs": {
    "events_over_time": {
      "date_histogram": {
        "field": "@timestamp",
        "calendar_interval": "1d"
      }
    }
  }
}"#),
        description: "Group documents by calendar days to view activity trends over time.",
        use_kibana: false,
    },
    ConsolePreset {
        category: "Search & Analytics",
        name: "Boolean Filter Query",
        method: "POST",
        path: "/{{index}}/_search",
        body: Some(r#"{
  "query": {
    "bool": {
      "must": [
        { "match": { "message": "error" } }
      ],
      "filter": [
        { "term": { "status": "active" } },
        { "range": { "@timestamp": { "gte": "now-1d" } } }
      ]
    }
  }
}"#),
        description: "Combine text match search terms with structured filter constraints.",
        use_kibana: false,
    },
    // --- Ingest & ILM ---
    ConsolePreset {
        category: "Ingest & ILM",
        name: "Create Ingest Pipeline",
        method: "PUT",
        path: "/_ingest/pipeline/{{pipeline_id}}",
        body: Some(r#"{
  "description": "Custom parser pipeline",
  "processors": [
    {
      "lowercase": {
        "field": "status"
      }
    },
    {
      "set": {
        "field": "processed_at",
        "value": "{{{_ingest.timestamp}}}"
      }
    }
  ]
}"#),
        description: "Define an ingest pipeline with pre-processors that manipulate document values before indexing.",
        use_kibana: false,
    },
    ConsolePreset {
        category: "Ingest & ILM",
        name: "Simulate Ingest Pipeline",
        method: "POST",
        path: "/_ingest/pipeline/{{pipeline_id}}/_simulate",
        body: Some(r#"{
  "docs": [
    {
      "_source": {
        "message": "Hello World",
        "status": "RUNNING"
      }
    }
  ]
}"#),
        description: "Test an ingest pipeline against sample input documents to view structural changes.",
        use_kibana: false,
    },
    ConsolePreset {
        category: "Ingest & ILM",
        name: "Get ILM Policy Status",
        method: "GET",
        path: "/_ilm/status",
        body: None,
        description: "Retrieve the current operation state of the Index Lifecycle Management (ILM) runner.",
        use_kibana: false,
    },
    ConsolePreset {
        category: "Ingest & ILM",
        name: "Explain ILM Index Status",
        method: "GET",
        path: "/{{index}}/_ilm/explain",
        body: None,
        description: "Explain the current lifecycle status and active phase state for a managed index.",
        use_kibana: false,
    },
    // --- Snapshot & Backup ---
    ConsolePreset {
        category: "Snapshot & Backup",
        name: "Register Shared FS Repo",
        method: "PUT",
        path: "/_snapshot/{{repo_name}}",
        body: Some(r#"{
  "type": "fs",
  "settings": {
    "location": "/mount/backups/elasticsearch",
    "compress": true
  }
}"#),
        description: "Register a shared file system repository for taking cluster snapshots.",
        use_kibana: false,
    },
    ConsolePreset {
        category: "Snapshot & Backup",
        name: "Take Dynamic Snapshot",
        method: "PUT",
        path: "/_snapshot/{{repo_name}}/{{snapshot_name}}?wait_for_completion=true",
        body: Some(r#"{
  "indices": "logs-*,metrics-*",
  "ignore_unavailable": true,
  "include_global_state": false,
  "metadata": {
    "taken_by": "Drastic Smurf Admin Console"
  }
}"#),
        description: "Initiate and wait for a snapshot copy of specific matching indices to complete.",
        use_kibana: false,
    },
    ConsolePreset {
        category: "Snapshot & Backup",
        name: "Restore Snapshot Index",
        method: "POST",
        path: "/_snapshot/{{repo_name}}/{{snapshot_name}}/_restore",
        body: Some(r#"{
  "indices": "logs-*",
  "ignore_unavailable": true,
  "include_global_state": false,
  "rename_pattern": "logs-(.+)",
  "rename_replacement": "restored-logs-$1"
}"#),
        description: "Restore an index pattern from a snapshot and rename the indices on import.",
        use_kibana: false,
    },
    // --- Security & API Keys ---
    ConsolePreset {
        category: "Security & API Keys",
        name: "Create API Key",
        method: "POST",
        path: "/_security/api_key",
        body: Some(r#"{
  "name": "my-service-key",
  "expiration": "30d",
  "role_descriptors": {
    "role-a": {
      "cluster": ["monitor"],
      "index": [
        {
          "names": ["logs-*"],
          "privileges": ["read"]
        }
      ]
    }
  }
}"#),
        description: "Generate a secure, expiring programmatic access token (API Key) with custom roles.",
        use_kibana: false,
    },
    ConsolePreset {
        category: "Security & API Keys",
        name: "Get API Key Info",
        method: "GET",
        path: "/_security/api_key?owner=true",
        body: None,
        description: "View all security credentials (API Keys) created by the current authenticated user.",
        use_kibana: false,
    },
    ConsolePreset {
        category: "Security & API Keys",
        name: "Create User",
        method: "POST",
        path: "/_security/user/{{username}}",
        body: Some(r#"{
  "password" : "new-secure-password",
  "roles" : [ "kibana_admin", "read_all" ],
  "full_name" : "Operational User",
  "email" : "ops@example.com"
}"#),
        description: "Register a standard local user account with credentials and administrative role mappings.",
        use_kibana: false,
    },
    // --- Troubleshooting & Maintenance ---
    ConsolePreset {
        category: "Troubleshooting & Maintenance",
        name: "Get Running Tasks",
        method: "GET",
        path: "/_tasks?detailed=true&actions=*write*,*search*",
        body: None,
        description: "List currently running write or search tasks across the cluster.",
        use_kibana: false,
    },
    ConsolePreset {
        category: "Troubleshooting & Maintenance",
        name: "Get All Cluster Settings",
        method: "GET",
        path: "/_cluster/settings?flat_settings=true&include_defaults=true",
        body: None,
        description: "View all dynamic and default configuration settings for the cluster.",
        use_kibana: false,
    },
    ConsolePreset {
        category: "Troubleshooting & Maintenance",
        name: "Update Cluster Settings",
        method: "PUT",
        path: "/_cluster/settings",
        body: Some(r#"{
  "persistent": {
    "cluster.routing.allocation.enable": "all"
  },
  "transient": {
    "indices.recovery.max_bytes_per_sec": "40mb"
  }
}"#),
        description: "Template to dynamically update cluster-wide transient or persistent configurations.",
        use_kibana: false,
    },
    ConsolePreset {
        category: "Troubleshooting & Maintenance",
        name: "Clear Cache",
        method: "POST",
        path: "/_cache/clear",
        body: None,
        description: "Clear field data, query cache, and request cache across the cluster.",
        use_kibana: false,
    },
    ConsolePreset {
        category: "Troubleshooting & Maintenance",
        name: "Reindex Template",
        method: "POST",
        path: "/_reindex?wait_for_completion=false",
        body: Some(r#"{
  "source": {
    "index": "source-index"
  },
  "dest": {
    "index": "dest-index"
  }
}"#),
        description: "Asynchronously copy documents from one index to another.",
        use_kibana: false,
    },
    ConsolePreset {
        category: "Troubleshooting & Maintenance",
        name: "Shrink Index Template",
        method: "POST",
        path: "/{{index}}/_shrink/{{shrunk_index}}",
        body: Some(r#"{
  "settings": {
    "index.number_of_shards": 1,
    "index.codec": "best_compression"
  }
}"#),
        description: "Consolidate existing indices into a new, smaller index with fewer primary shards.",
        use_kibana: false,
    },
    // --- Kibana Spaces & Security ---
    ConsolePreset {
        category: "Kibana Spaces & Security",
        name: "Get Kibana Status",
        method: "GET",
        path: "/api/status",
        body: None,
        description: "Retrieve comprehensive running metrics, loaded plugins, and host statuses for Kibana.",
        use_kibana: true,
    },
    ConsolePreset {
        category: "Kibana Spaces & Security",
        name: "List Spaces",
        method: "GET",
        path: "/api/spaces/space",
        body: None,
        description: "Fetch a list of all active user workspaces (Spaces) configured inside the Kibana instance.",
        use_kibana: true,
    },
    ConsolePreset {
        category: "Kibana Spaces & Security",
        name: "Create Space",
        method: "POST",
        path: "/api/spaces/space",
        body: Some(r##"{
  "id": "engineering",
  "name": "Engineering",
  "description": "Workspace for engineering dashboards",
  "color": "#aabbcc"
}"##),
        description: "Generate a custom workspace (Space) inside Kibana with dedicated privileges.",
        use_kibana: true,
    },
    ConsolePreset {
        category: "Kibana Spaces & Security",
        name: "List Security Roles",
        method: "GET",
        path: "/api/security/role",
        body: None,
        description: "Retrieve list of roles that secure access to Kibana resources and Spaces.",
        use_kibana: true,
    },
    ConsolePreset {
        category: "Kibana Spaces & Security",
        name: "Get Current User Profile",
        method: "GET",
        path: "/api/security/logged_in_user",
        body: None,
        description: "Get detailed information about the currently logged-in Kibana user profile.",
        use_kibana: true,
    },
    // --- Kibana Saved Objects ---
    ConsolePreset {
        category: "Kibana Saved Objects",
        name: "Find Saved Objects",
        method: "GET",
        path: "/api/saved_objects/_find?type=dashboard&search_fields=title&search=my-dashboard",
        body: None,
        description: "Retrieve search query results matches for specific Kibana Dashboards, Visualizations, or Maps.",
        use_kibana: true,
    },
    ConsolePreset {
        category: "Kibana Saved Objects",
        name: "Export Saved Objects",
        method: "POST",
        path: "/api/saved_objects/_export",
        body: Some(r#"{
  "type": "dashboard",
  "limit": 100
}"#),
        description: "Export configured dashboards, maps, or saved searches in NDJSON format.",
        use_kibana: true,
    },
    ConsolePreset {
        category: "Kibana Saved Objects",
        name: "Create Saved Dashboard",
        method: "POST",
        path: "/api/saved_objects/dashboard/my-dashboard-id",
        body: Some(r#"{
  "attributes": {
    "title": "My Production Dashboard",
    "description": "Dashboard created programmatically",
    "panelsJSON": "[]",
    "optionsJSON": "{\"darkTheme\":true}",
    "timeRestore": false
  }
}"#),
        description: "Create or overwrite a dashboard saved object directly in Kibana's active Space.",
        use_kibana: true,
    },
    ConsolePreset {
        category: "Kibana Saved Objects",
        name: "Get console settings",
        method: "GET",
        path: "/api/console/settings",
        body: None,
        description: "Get user settings for the Kibana Dev Tools Console.",
        use_kibana: true,
    },
    ConsolePreset {
        category: "Kibana Saved Objects",
        name: "Get Kibana Features",
        method: "GET",
        path: "/api/features",
        body: None,
        description: "List all loaded features, applications, and licensing privileges currently active in Kibana.",
        use_kibana: true,
    },
];

pub fn interpolate_variables(input: &str, variables: &[(String, String)]) -> String {
    let mut output = input.to_string();
    for (k, v) in variables {
        if !k.is_empty() {
            let placeholder = format!("{{{{{}}}}}", k);
            output = output.replace(&placeholder, v);
        }
    }
    output
}

pub fn render_console_module(
    ui: &mut Ui,
    state: &mut ConsoleState,
    clusters: &[String],
    on_send: &mut Option<(String, String, String, Option<String>, bool)>,
    on_save_query: &mut Option<SavedQuery>,
    on_delete_query: &mut Option<String>,
) {
    ui.heading("Elastic Console");
    ui.add_space(8.0);

    if clusters.is_empty() {
        ui.label("No clusters configured. Add a cluster first.");
        return;
    }

    let total_available = ui.available_size() - egui::vec2(0.0, 12.0);
    ui.allocate_ui_with_layout(
        total_available,
        egui::Layout::left_to_right(egui::Align::Min),
        |ui| {
            // --- LEFT COLUMN: Workspace Panel (Presets, Variables, History, Saved) ---
            let left_width = 280.0;

            ui.vertical(|ui| {
                ui.set_width(left_width);
                let height = ui.available_height();

                egui::Frame::new()
                    .fill(crate::ui::theme::Theme::bg_card())
                    .corner_radius(crate::ui::theme::Theme::CARD_ROUNDING)
                    .inner_margin(crate::ui::theme::Theme::CARD_PADDING)
                    .show(ui, |ui| {
                        ui.set_height(height - 32.0);
                    // Title
                    ui.label(
                        egui::RichText::new("WORKSPACE")
                            .strong()
                            .color(crate::ui::theme::Theme::text_secondary())
                            .size(11.0),
                    );
                    ui.add_space(4.0);

                    // Tabs Selector Row
                    ui.horizontal(|ui| {
                        for (tab, label) in [
                            (ConsoleTab::Presets, "📚 Presets"),
                            (ConsoleTab::Variables, "🔑 Vars"),
                            (ConsoleTab::History, "⏱️ Hist"),
                            (ConsoleTab::Saved, "💾 Saved"),
                        ] {
                            let is_active = state.active_tab == tab;
                            let text = egui::RichText::new(label).size(11.0);
                            let text = if is_active {
                                text.color(crate::ui::theme::Theme::accent()).strong()
                            } else {
                                text.color(crate::ui::theme::Theme::text_secondary())
                            };
                            if ui.selectable_label(is_active, text).clicked() {
                                state.active_tab = tab;
                            }
                        }
                    });
                    ui.add_space(6.0);
                    ui.separator();
                    ui.add_space(8.0);

                    // Scrollable Tab Content
                    let scroll_height = (ui.available_height() - 4.0).max(100.0);
                    egui::ScrollArea::vertical()
                        .id_salt("console_workspace_scroll")
                        .max_height(scroll_height)
                        .show(ui, |ui| {
                            match state.active_tab {
                                ConsoleTab::Presets => {
                                    // Documentation Hyperlinks
                                    ui.horizontal(|ui| {
                                        ui.hyperlink_to("📚 ES API Docs", "https://www.elastic.co/docs/api/doc/elasticsearch/");
                                        ui.label("|");
                                        ui.hyperlink_to("🎨 Kibana API Docs", "https://www.elastic.co/docs/api/doc/kibana/");
                                    });
                                    ui.add_space(6.0);
                                    ui.separator();
                                    ui.add_space(4.0);

                                    let mut current_cat = "";
                                    for preset in PRESETS {
                                        if preset.category != current_cat {
                                            current_cat = preset.category;
                                            ui.add_space(8.0);
                                            ui.label(
                                                egui::RichText::new(current_cat)
                                                    .strong()
                                                    .color(crate::ui::theme::Theme::text_secondary())
                                                    .size(12.0),
                                            );
                                            ui.add_space(4.0);
                                        }

                                        let btn_label = if preset.use_kibana {
                                            format!("🎨 {}", preset.name)
                                        } else {
                                            preset.name.to_string()
                                        };

                                        let btn = ui.add(
                                            egui::Button::new(btn_label)
                                                .fill(crate::ui::theme::Theme::bg_input())
                                        );
                                        if btn.clicked() {
                                            state.method = preset.method.to_string();
                                            state.path = preset.path.to_string();
                                            state.body = preset.body.map(|b| b.to_string()).unwrap_or_default();
                                            state.use_kibana_host = preset.use_kibana;
                                            state.history_index = None;
                                        }
                                        btn.on_hover_ui(|ui| {
                                            ui.set_max_width(220.0);
                                            let prefix = if preset.use_kibana { "🎨 Kibana API" } else { "🔌 Elasticsearch API" };
                                            ui.label(
                                                egui::RichText::new(prefix)
                                                    .small()
                                                    .color(crate::ui::theme::Theme::text_muted()),
                                            );
                                            ui.label(
                                                egui::RichText::new(format!("{} {}", preset.method, preset.path))
                                                    .strong()
                                                    .color(crate::ui::theme::Theme::accent()),
                                            );
                                            ui.label(
                                                egui::RichText::new(preset.description)
                                                    .size(11.0)
                                                    .color(crate::ui::theme::Theme::text_primary()),
                                            );
                                        });
                                        ui.add_space(3.0);
                                    }
                                }
                                ConsoleTab::Variables => {
                                    ui.horizontal(|ui| {
                                        ui.label(egui::RichText::new("Variables").strong().size(13.0));
                                        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                                            if ui.button("+ Add").clicked() {
                                                state.variables.push((String::new(), String::new()));
                                                state.variables_changed = true;
                                            }
                                        });
                                    });
                                    ui.add_space(6.0);

                                    let mut to_remove = None;
                                    for (idx, (k, v)) in state.variables.iter_mut().enumerate() {
                                        ui.horizontal(|ui| {
                                            let key_edit = ui.add(
                                                egui::TextEdit::singleline(k)
                                                    .hint_text("key")
                                                    .desired_width(60.0)
                                            );
                                            if key_edit.changed() {
                                                state.variables_changed = true;
                                            }

                                            ui.label("=");

                                            let val_edit = ui.add(
                                                egui::TextEdit::singleline(v)
                                                    .hint_text("value")
                                                    .desired_width(100.0)
                                            );
                                            if val_edit.changed() {
                                                state.variables_changed = true;
                                            }

                                            if ui.button("❌").clicked() {
                                                to_remove = Some(idx);
                                                state.variables_changed = true;
                                            }
                                        });
                                        ui.add_space(4.0);
                                    }

                                    if let Some(idx) = to_remove {
                                        state.variables.remove(idx);
                                    }
                                }
                                ConsoleTab::History => {
                                    ui.horizontal(|ui| {
                                        ui.label(egui::RichText::new("History").strong().size(13.0));
                                        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                                            if ui.button("🗑 Clear").clicked() {
                                                state.history.clear();
                                                state.history_index = None;
                                            }
                                        });
                                    });
                                    ui.add_space(6.0);

                                    if state.history.is_empty() {
                                        ui.label(
                                            egui::RichText::new("No request history.")
                                                .color(crate::ui::theme::Theme::text_muted())
                                                .size(11.0),
                                        );
                                    } else {
                                        for (rev_idx, (cluster, method, path, body)) in state.history.iter().enumerate().rev() {
                                            let label_text = format!("{} {}", method, path);
                                            let text = egui::RichText::new(&label_text).size(12.0);

                                            let is_loaded = state.history_index.map_or(false, |idx| idx == rev_idx);
                                            let text = if is_loaded {
                                                text.color(crate::ui::theme::Theme::accent()).strong()
                                            } else {
                                                text.color(crate::ui::theme::Theme::text_primary())
                                            };

                                            let btn = ui.selectable_label(is_loaded, text);
                                            if btn.clicked() {
                                                state.selected_cluster = cluster.clone();
                                                state.method = method.clone();
                                                state.path = path.clone();
                                                state.body = body.clone();
                                                state.history_index = Some(rev_idx);
                                            }
                                            btn.on_hover_text(format!(
                                                "Cluster: {}\nMethod: {}\nPath: {}\nBody: {}",
                                                cluster, method, path, body
                                            ));
                                            ui.add_space(4.0);
                                        }
                                    }
                                }
                                ConsoleTab::Saved => {
                                    ui.label(egui::RichText::new("Saved Queries").strong().size(13.0));
                                    ui.add_space(6.0);

                                    if state.saved_queries.is_empty() {
                                        ui.label(
                                            egui::RichText::new("No saved queries.")
                                                .color(crate::ui::theme::Theme::text_muted())
                                                .size(11.0),
                                        );
                                    } else {
                                        let mut to_delete = None;
                                        for query in &state.saved_queries {
                                            ui.horizontal(|ui| {
                                                let label_text = format!("{} ({} {})", query.name, query.method, query.path);
                                                if ui.button(&label_text).clicked() {
                                                    state.method = query.method.clone();
                                                    state.path = query.path.clone();
                                                    state.body = query.body.clone().unwrap_or_default();
                                                    state.history_index = None;
                                                }
                                                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                                                    if ui.button("🗑").on_hover_text("Delete Saved Query").clicked() {
                                                        to_delete = Some(query.name.clone());
                                                    }
                                                });
                                            });
                                            ui.add_space(4.0);
                                        }
                                        if let Some(name) = to_delete {
                                            *on_delete_query = Some(name);
                                        }
                                    }
                                }
                            }
                        });
                });
        });

        ui.add_space(8.0);

            // --- RIGHT COLUMN: Editor & Response Viewer ---
            ui.vertical(|ui| {
                ui.set_width(ui.available_width());
                let height = ui.available_height();

                egui::Frame::new()
                    .fill(crate::ui::theme::Theme::bg_card())
                    .corner_radius(crate::ui::theme::Theme::CARD_ROUNDING)
                    .inner_margin(crate::ui::theme::Theme::CARD_PADDING)
                    .show(ui, |ui| {
                        ui.set_height(height - 32.0);
                    // Connection / Method selector row
                    ui.horizontal(|ui| {
                        ui.label("Cluster:");
                        egui::ComboBox::from_id_salt("console_cluster")
                            .selected_text(&state.selected_cluster)
                            .show_ui(ui, |ui| {
                                for cluster in clusters {
                                    ui.selectable_value(
                                        &mut state.selected_cluster,
                                        cluster.clone(),
                                        cluster,
                                    );
                                }
                            });

                        ui.add_space(8.0);

                        ui.label("Method:");
                        let method_color = match state.method.as_str() {
                            "GET" => crate::ui::theme::Theme::success(),
                            "POST" => crate::ui::theme::Theme::warning(),
                            "PUT" => crate::ui::theme::Theme::accent(),
                            "DELETE" => crate::ui::theme::Theme::danger(),
                            _ => crate::ui::theme::Theme::text_secondary(),
                        };

                        egui::ComboBox::from_id_salt("console_method")
                            .selected_text(egui::RichText::new(&state.method).color(method_color).strong())
                            .show_ui(ui, |ui| {
                                for m in ["GET", "POST", "PUT", "DELETE", "HEAD"] {
                                    let color = match m {
                                        "GET" => crate::ui::theme::Theme::success(),
                                        "POST" => crate::ui::theme::Theme::warning(),
                                        "PUT" => crate::ui::theme::Theme::accent(),
                                        "DELETE" => crate::ui::theme::Theme::danger(),
                                        _ => crate::ui::theme::Theme::text_secondary(),
                                    };
                                    ui.selectable_value(
                                        &mut state.method,
                                        m.to_string(),
                                        egui::RichText::new(m).color(color).strong(),
                                    );
                                }
                            });

                        ui.add_space(8.0);
                        ui.checkbox(&mut state.use_kibana_host, "Kibana Host");
                    });

                    ui.add_space(8.0);

                    // Path / Address Row with Actions
                    ui.horizontal(|ui| {
                        ui.label("Path:");
                        
                        // Right-to-Left block for buttons to float right!
                        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                            // Send
                            let send_btn = ui.add_enabled(!state.is_loading, egui::Button::new("⚡ Send").fill(crate::ui::theme::Theme::accent()));
                            if state.is_loading {
                                ui.spinner();
                            }

                            if send_btn.clicked() && !state.is_loading {
                                state.is_loading = true;
                                state.history.push((
                                    state.selected_cluster.clone(),
                                    state.method.clone(),
                                    state.path.clone(),
                                    state.body.clone(),
                                ));
                                state.history_index = Some(state.history.len() - 1);

                                // Variable interpolation
                                let interp_path = interpolate_variables(&state.path, &state.variables);
                                let interp_body = interpolate_variables(&state.body, &state.variables);

                                let body = if interp_body.trim().is_empty() {
                                    None
                                } else {
                                    Some(interp_body)
                                };
                                *on_send = Some((
                                    state.selected_cluster.clone(),
                                    state.method.clone(),
                                    interp_path,
                                    body,
                                    state.use_kibana_host,
                                ));
                            }

                            // Save
                            if ui.button("💾 Save").clicked() {
                                state.show_save_dialog = true;
                                state.query_name_input.clear();
                            }

                            // History Next
                            let has_history = !state.history.is_empty();
                            let next_btn = ui.add_enabled(has_history, egui::Button::new("Next ▶"));
                            if next_btn.clicked() {
                                let next_idx = match state.history_index {
                                    Some(idx) => if idx + 1 < state.history.len() { idx + 1 } else { 0 },
                                    None => 0,
                                };
                                if let Some((cluster, method, path, body)) = state.history.get(next_idx) {
                                    state.selected_cluster = cluster.clone();
                                    state.method = method.clone();
                                    state.path = path.clone();
                                    state.body = body.clone();
                                    state.history_index = Some(next_idx);
                                }
                            }

                            // History Previous
                            let prev_btn = ui.add_enabled(has_history, egui::Button::new("◀ Prev"));
                            if prev_btn.clicked() {
                                let next_idx = match state.history_index {
                                    Some(idx) => if idx > 0 { idx - 1 } else { state.history.len() - 1 },
                                    None => state.history.len() - 1,
                                };
                                if let Some((cluster, method, path, body)) = state.history.get(next_idx) {
                                    state.selected_cluster = cluster.clone();
                                    state.method = method.clone();
                                    state.path = path.clone();
                                    state.body = body.clone();
                                    state.history_index = Some(next_idx);
                                }
                            }

                            // Fill remaining space with Path TextEdit
                            ui.with_layout(egui::Layout::left_to_right(egui::Align::Center), |ui| {
                                ui.add(
                                    egui::TextEdit::singleline(&mut state.path)
                                        .desired_width((ui.available_width() - 8.0).max(100.0))
                                );
                            });
                        });
                    });

                    // Dynamic evaluated path preview helper
                    let contains_vars = state.path.contains("{{") || state.body.contains("{{");
                    if contains_vars {
                        let interp_path = interpolate_variables(&state.path, &state.variables);
                        ui.label(
                            egui::RichText::new(format!("📝 Evaluated path: {}", interp_path))
                                .italics()
                                .color(crate::ui::theme::Theme::text_muted())
                                .size(11.0)
                        );
                    } else {
                        ui.add_space(4.0);
                    }

                    // Save query dialog
                    if state.show_save_dialog {
                        ui.add_space(4.0);
                        ui.horizontal(|ui| {
                            ui.label("Query Name:");
                            ui.text_edit_singleline(&mut state.query_name_input);
                            if ui.button("Save").clicked() && !state.query_name_input.is_empty() {
                                *on_save_query = Some(SavedQuery {
                                    name: state.query_name_input.clone(),
                                    method: state.method.clone(),
                                    path: state.path.clone(),
                                    body: if state.body.trim().is_empty() {
                                        None
                                    } else {
                                        Some(state.body.clone())
                                    },
                                });
                                state.show_save_dialog = false;
                                state.query_name_input.clear();
                            }
                            if ui.button("Cancel").clicked() {
                                state.show_save_dialog = false;
                                state.query_name_input.clear();
                            }
                        });
                    }

                    // Multiline Body Editor
                    ui.add_space(8.0);
                    ui.horizontal(|ui| {
                        ui.label("Body:");
                        if let Some(ref err) = state.json_error {
                            ui.colored_label(crate::ui::theme::Theme::danger(), format!("⚠️ {}", err));
                        }
                        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                            if ui.button("✨ Prettify JSON")
                                .on_hover_text("Format and auto-indent the JSON body")
                                .clicked()
                            {
                                if !state.body.trim().is_empty() {
                                    match prettify_json_body(&state.body) {
                                        Ok(formatted) => {
                                            state.body = formatted;
                                            state.json_error = None;
                                        }
                                        Err(e) => {
                                            state.json_error = Some(e);
                                        }
                                    }
                                }
                            }
                        });
                    });
                    
                    let mut body_h = state.body_height.unwrap_or(220.0);
                    // Constrain body height to leave at least 150px for the response box
                    let max_body_h = (ui.available_height() - 150.0).max(80.0);
                    body_h = body_h.clamp(80.0, max_body_h);

                    let res = ui.add_sized(
                        [ui.available_width(), body_h],
                        egui::TextEdit::multiline(&mut state.body)
                            .code_editor()
                            .desired_rows(4),
                    );
                    if res.changed() {
                        state.json_error = None;
                    }

                    // Draggable Vertical Splitter
                    ui.add_space(4.0);
                    let separator_response = ui.allocate_response(
                        egui::vec2(ui.available_width(), 8.0),
                        egui::Sense::drag(),
                    );
                    
                    let painter = ui.painter();
                    let rect = separator_response.rect;
                    let divider_color = if separator_response.dragged() {
                        crate::ui::theme::Theme::accent()
                    } else if separator_response.hovered() {
                        crate::ui::theme::Theme::accent_hover()
                    } else {
                        crate::ui::theme::Theme::border()
                    };
                    
                    let y = rect.center().y;
                    painter.line_segment(
                        [egui::pos2(rect.min.x, y), egui::pos2(rect.max.x, y)],
                        egui::Stroke::new(2.0, divider_color),
                    );
                    
                    if separator_response.hovered() || separator_response.dragged() {
                        ui.ctx().set_cursor_icon(egui::CursorIcon::ResizeVertical);
                    }
                    
                    if separator_response.dragged() {
                        body_h += separator_response.drag_delta().y;
                        state.body_height = Some(body_h);
                    }
                    ui.add_space(4.0);

                    // Multiline Response Terminal
                    ui.horizontal(|ui| {
                        ui.label("Response:");
                        if ui.small_button("Clear").clicked() {
                            state.response.clear();
                        }
                    });
                    
                    let response_height = (ui.available_height() - 16.0).max(80.0);
                    egui::ScrollArea::vertical()
                        .id_salt("console_response_scroll")
                        .max_height(response_height)
                        .auto_shrink([false, false])
                        .show(ui, |ui| {
                            ui.add(
                                egui::TextEdit::multiline(&mut state.response)
                                    .code_editor()
                                    .desired_width(ui.available_width())
                                    .desired_rows(6),
                            );
                        });
                });
        });
    });
}

pub fn prettify_json_body(body: &str) -> Result<String, String> {
    if body.trim().is_empty() {
        return Ok(String::new());
    }

    // 1. Find all unquoted placeholders like `{{label}}` and temporarily replace them with valid quoted strings.
    let mut temp_body = body.to_string();
    let mut placeholders = Vec::new();
    let mut start_idx = 0;
    
    while let Some(open_pos) = temp_body[start_idx..].find("{{") {
        let abs_open = start_idx + open_pos;
        if let Some(close_pos) = temp_body[abs_open..].find("}}") {
            let abs_close = abs_open + close_pos + 2;
            let placeholder_content = &temp_body[abs_open..abs_close];
            
            // Check if it's already inside quotes
            let is_quoted = if abs_open > 0 && abs_close < temp_body.len() {
                let prev_char = temp_body.as_bytes()[abs_open - 1] as char;
                let next_char = temp_body.as_bytes()[abs_close] as char;
                prev_char == '"' && next_char == '"'
            } else {
                false
            };

            if !is_quoted {
                let dummy_id = format!("__SMURF_UNQUOTED_VAR_{}__", placeholders.len());
                placeholders.push((dummy_id.clone(), placeholder_content.to_string()));
                temp_body = temp_body.replace(placeholder_content, &format!("\"{}\"", dummy_id));
                start_idx = 0;
                continue;
            }
            start_idx = abs_close;
        } else {
            break;
        }
    }

    // 2. Strip comments to be completely loose/tolerant (Kibana style!)
    let mut clean_lines = Vec::new();
    for line in temp_body.lines() {
        let trimmed = line.trim();
        if trimmed.starts_with("//") || trimmed.starts_with("#") {
            continue;
        }
        let mut actual_content = line;
        if let Some(idx) = line.find("//") {
            actual_content = &line[..idx];
        } else if let Some(idx) = line.find("#") {
            actual_content = &line[..idx];
        }
        clean_lines.push(actual_content.to_string());
    }
    let joined = clean_lines.join("\n");
    
    // Strip trailing commas
    let mut parsed_ready = String::new();
    let mut chars = joined.chars().peekable();
    while let Some(c) = chars.next() {
        if c == ',' {
            let mut temp = chars.clone();
            let mut is_trailing = false;
            while let Some(&next_c) = temp.peek() {
                if next_c.is_whitespace() {
                    temp.next();
                } else if next_c == '}' || next_c == ']' {
                    is_trailing = true;
                    break;
                } else {
                    break;
                }
            }
            if is_trailing {
                continue;
            }
        }
        parsed_ready.push(c);
    }

    // 3. Parse and prettify using serde_json
    let parsed = serde_json::from_str::<serde_json::Value>(&parsed_ready)
        .map_err(|e| e.to_string())?;
    
    let mut formatted = serde_json::to_string_pretty(&parsed)
        .map_err(|e| e.to_string())?;

    // 4. Restore the unquoted placeholders!
    for (dummy_id, original) in placeholders.iter().rev() {
        let quoted_dummy = format!("\"{}\"", dummy_id);
        formatted = formatted.replace(&quoted_dummy, original);
    }

    Ok(formatted)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_interpolate_variables() {
        let vars = vec![
            ("index".to_string(), "my-index-123".to_string()),
            ("node".to_string(), "node-blue".to_string()),
        ];

        let path = "/{{index}}/_search?node={{node}}";
        let result = interpolate_variables(path, &vars);
        assert_eq!(result, "/my-index-123/_search?node=node-blue");

        let path_missing = "/{{missing_var}}/docs";
        let result_missing = interpolate_variables(path_missing, &vars);
        assert_eq!(result_missing, "/{{missing_var}}/docs");

        assert_eq!(interpolate_variables("", &vars), "");
    }

    #[test]
    fn test_prettify_json_body() {
        let input = r#"{
            // Some comment
            "index": "logs",
            "shards": {{shards}},
            "nested": {
                "val": "{{my_val}}",
            }
        }"#;

        let res = prettify_json_body(input).unwrap();
        assert!(res.contains("\"shards\": {{shards}}"));
        assert!(res.contains("\"val\": \"{{my_val}}\""));
        assert!(!res.contains("// Some comment"));
    }
}
