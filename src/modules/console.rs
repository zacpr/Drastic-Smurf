use egui::Ui;

use crate::core::config::SavedQuery;
use crate::modules::api_examples::{self, PromptChoice};

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
    pub full_response: Option<String>,
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
    pub preset_filter: String,
    /// The response box holds an API doc example rather than a real response.
    pub response_is_example: bool,
    /// Send was pressed while the body still had example markers.
    pub examples_prompt: bool,
    /// Send on the next frame (set by the examples prompt).
    pub send_requested: bool,
    /// (method, path, body text) of the examples last inserted, for "Show examples".
    pub examples_source: Option<(String, String, String)>,
}

impl ConsoleState {
    /// Load a preset, filling the body and response boxes with API doc examples.
    pub fn apply_preset(&mut self, preset: &ConsolePreset) {
        self.method = preset.method.to_string();
        self.path = preset.path.to_string();
        self.body = api_examples::request_text(preset.method, preset.path, preset.body);
        self.examples_source = api_examples::has_examples(&self.body)
            .then(|| (self.method.clone(), self.path.clone(), self.body.clone()));
        self.use_kibana_host = preset.use_kibana;
        self.history_index = None;
        self.json_error = None;
        match api_examples::response_text(preset.method, preset.path) {
            Some(text) => {
                self.response = text;
                self.full_response = None;
                self.response_is_example = true;
            }
            None => self.clear_example_response(),
        }
    }

    /// The examples last inserted, while the request still targets the same API.
    fn available_examples(&self) -> Option<String> {
        self.examples_source
            .as_ref()
            .filter(|(m, p, _)| *m == self.method && *p == self.path)
            .map(|(_, _, text)| text.clone())
    }

    /// Clear the response box if it only holds a doc example.
    pub fn clear_example_response(&mut self) {
        if self.response_is_example {
            self.response.clear();
            self.full_response = None;
            self.response_is_example = false;
        }
    }
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

/// Methods the console can send. Anything else falls back to GET in `DrasticSmurfApp::update`.
pub const CONSOLE_METHODS: &[&str] = &["GET", "POST", "PUT", "DELETE", "HEAD"];

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
    ConsolePreset {
        category: "Cluster Status & Info",
        name: "Health Report",
        method: "GET",
        path: "/_health_report",
        body: None,
        description: "Run the health indicators and list root causes and suggested fixes for any problems found.",
        use_kibana: false,
    },
    ConsolePreset {
        category: "Cluster Status & Info",
        name: "License Info",
        method: "GET",
        path: "/_license",
        body: None,
        description: "Show the active license type, status, and expiry date.",
        use_kibana: false,
    },
    ConsolePreset {
        category: "Cluster Status & Info",
        name: "Deprecation Warnings",
        method: "GET",
        path: "/_migration/deprecations",
        body: None,
        description: "List deprecated settings and features to resolve before the next major upgrade.",
        use_kibana: false,
    },
    ConsolePreset {
        category: "Cluster Status & Info",
        name: "Remote Clusters",
        method: "GET",
        path: "/_remote/info",
        body: None,
        description: "List configured remote clusters and their connection status for CCS and CCR.",
        use_kibana: false,
    },
    ConsolePreset {
        category: "Cluster Status & Info",
        name: "Current Master Node",
        method: "GET",
        path: "/_cat/master?v",
        body: None,
        description: "Show which node is the elected master.",
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
    ConsolePreset {
        category: "Nodes & Allocation",
        name: "Node Stats",
        method: "GET",
        path: "/_nodes/stats?pretty",
        body: None,
        description: "Retrieve detailed JVM, OS, filesystem, thread pool, and indexing statistics per node.",
        use_kibana: false,
    },
    ConsolePreset {
        category: "Nodes & Allocation",
        name: "Hot Threads",
        method: "GET",
        path: "/_nodes/hot_threads",
        body: None,
        description: "Sample the busiest threads on each node to diagnose high CPU usage.",
        use_kibana: false,
    },
    ConsolePreset {
        category: "Nodes & Allocation",
        name: "Thread Pool Rejections",
        method: "GET",
        path: "/_cat/thread_pool?v&h=node_name,name,active,queue,rejected&s=rejected:desc",
        body: None,
        description: "List thread pools by rejection count to spot overloaded nodes.",
        use_kibana: false,
    },
    ConsolePreset {
        category: "Nodes & Allocation",
        name: "Node Attributes",
        method: "GET",
        path: "/_cat/nodeattrs?v",
        body: None,
        description: "List custom node attributes such as data tier or availability zone.",
        use_kibana: false,
    },
    ConsolePreset {
        category: "Nodes & Allocation",
        name: "Retry Failed Allocations",
        method: "POST",
        path: "/_cluster/reroute?retry_failed=true",
        body: None,
        description: "Retry shards that stopped allocating after reaching the maximum number of failed attempts.",
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
        name: "Create Index",
        method: "PUT",
        path: "/{{index}}",
        body: Some(
            r#"{
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
}"#,
        ),
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
        body: Some(
            r#"{
  "query": {
    "match_all": {}
  },
  "size": 10
}"#,
        ),
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
    ConsolePreset {
        category: "Indices & Templates",
        name: "Get Index Settings",
        method: "GET",
        path: "/{{index}}/_settings?pretty",
        body: None,
        description: "Retrieve the settings for one or more indices.",
        use_kibana: false,
    },
    ConsolePreset {
        category: "Indices & Templates",
        name: "Update Index Settings",
        method: "PUT",
        path: "/{{index}}/_settings",
        body: Some(
            r#"{
  "index": {
    "number_of_replicas": 1,
    "refresh_interval": "30s"
  }
}"#,
        ),
        description: "Change dynamic index settings such as the replica count or refresh interval.",
        use_kibana: false,
    },
    ConsolePreset {
        category: "Indices & Templates",
        name: "Get Index Stats",
        method: "GET",
        path: "/{{index}}/_stats?pretty",
        body: None,
        description: "Retrieve document counts, store size, and indexing and search statistics for an index.",
        use_kibana: false,
    },
    ConsolePreset {
        category: "Indices & Templates",
        name: "Count Documents",
        method: "GET",
        path: "/{{index}}/_count",
        body: None,
        description: "Count the documents in an index, data stream, or alias.",
        use_kibana: false,
    },
    ConsolePreset {
        category: "Indices & Templates",
        name: "Refresh Index",
        method: "POST",
        path: "/{{index}}/_refresh",
        body: None,
        description: "Make recently indexed documents visible to search immediately.",
        use_kibana: false,
    },
    ConsolePreset {
        category: "Indices & Templates",
        name: "Force Merge Index",
        method: "POST",
        path: "/{{index}}/_forcemerge?max_num_segments=1",
        body: None,
        description: "Merge an index down to one segment per shard. Only use on indices that are no longer written to.",
        use_kibana: false,
    },
    ConsolePreset {
        category: "Indices & Templates",
        name: "Roll Over Alias",
        method: "POST",
        path: "/{{alias}}/_rollover",
        body: None,
        description: "Create a new write index behind an alias.",
        use_kibana: false,
    },
    ConsolePreset {
        category: "Indices & Templates",
        name: "List Aliases",
        method: "GET",
        path: "/_cat/aliases?v&s=alias",
        body: None,
        description: "List all aliases and the indices they point to.",
        use_kibana: false,
    },
    ConsolePreset {
        category: "Indices & Templates",
        name: "Add Index Alias",
        method: "POST",
        path: "/_aliases",
        body: Some(
            r#"{
  "actions": [
    {
      "add": {
        "index": "{{index}}",
        "alias": "{{alias}}"
      }
    }
  ]
}"#,
        ),
        description: "Point an alias at an index.",
        use_kibana: false,
    },
    ConsolePreset {
        category: "Indices & Templates",
        name: "Resolve Index Pattern",
        method: "GET",
        path: "/_resolve/index/{{index_pattern}}",
        body: None,
        description: "Show which indices, aliases, and data streams a name or wildcard pattern matches.",
        use_kibana: false,
    },
    ConsolePreset {
        category: "Indices & Templates",
        name: "List Index Templates",
        method: "GET",
        path: "/_index_template",
        body: None,
        description: "List all composable index templates.",
        use_kibana: false,
    },
    ConsolePreset {
        category: "Indices & Templates",
        name: "Get Index Template",
        method: "GET",
        path: "/_index_template/{{template_name}}",
        body: None,
        description: "Retrieve one composable index template.",
        use_kibana: false,
    },
    ConsolePreset {
        category: "Indices & Templates",
        name: "Create Index Template",
        method: "PUT",
        path: "/_index_template/{{template_name}}",
        body: Some(
            r#"{
  "index_patterns": ["my-logs-*"],
  "priority": 500,
  "composed_of": [],
  "template": {
    "settings": {
      "index.number_of_shards": 1
    },
    "mappings": {
      "properties": {
        "@timestamp": { "type": "date" }
      }
    }
  }
}"#,
        ),
        description: "Create or replace a composable index template. Add an empty data_stream object to use it for data streams.",
        use_kibana: false,
    },
    ConsolePreset {
        category: "Indices & Templates",
        name: "Delete Index Template",
        method: "DELETE",
        path: "/_index_template/{{template_name}}",
        body: None,
        description: "Delete a composable index template. Existing indices are not affected.",
        use_kibana: false,
    },
    ConsolePreset {
        category: "Indices & Templates",
        name: "List Component Templates",
        method: "GET",
        path: "/_component_template",
        body: None,
        description: "List all component templates.",
        use_kibana: false,
    },
    ConsolePreset {
        category: "Indices & Templates",
        name: "Create Component Template",
        method: "PUT",
        path: "/_component_template/{{component_name}}",
        body: Some(
            r#"{
  "template": {
    "mappings": {
      "properties": {
        "@timestamp": { "type": "date" }
      }
    }
  }
}"#,
        ),
        description: "Create or replace a reusable component template.",
        use_kibana: false,
    },
    ConsolePreset {
        category: "Indices & Templates",
        name: "Delete Component Template",
        method: "DELETE",
        path: "/_component_template/{{component_name}}",
        body: None,
        description: "Delete a component template that no index template uses.",
        use_kibana: false,
    },
    // --- Data Streams ---
    ConsolePreset {
        category: "Data Streams",
        name: "List Data Streams",
        method: "GET",
        path: "/_data_stream",
        body: None,
        description: "List all data streams with their backing indices, template, and ILM policy.",
        use_kibana: false,
    },
    ConsolePreset {
        category: "Data Streams",
        name: "Data Stream Stats",
        method: "GET",
        path: "/_data_stream/_stats?human=true",
        body: None,
        description: "Show backing index counts, store size, and latest timestamp for each data stream.",
        use_kibana: false,
    },
    ConsolePreset {
        category: "Data Streams",
        name: "Create Data Stream",
        method: "PUT",
        path: "/_data_stream/{{data_stream}}",
        body: None,
        description: "Create a data stream. A matching index template with data streams enabled must already exist.",
        use_kibana: false,
    },
    ConsolePreset {
        category: "Data Streams",
        name: "Roll Over Data Stream",
        method: "POST",
        path: "/{{data_stream}}/_rollover",
        body: None,
        description: "Create a new write index for a data stream.",
        use_kibana: false,
    },
    ConsolePreset {
        category: "Data Streams",
        name: "Delete Data Stream",
        method: "DELETE",
        path: "/_data_stream/{{data_stream}}",
        body: None,
        description: "Delete a data stream and all of its backing indices. Danger: non-reversible!",
        use_kibana: false,
    },
    // --- Search & Analytics ---
    ConsolePreset {
        category: "Search & Analytics",
        name: "Terms Aggregation Query",
        method: "POST",
        path: "/{{index}}/_search",
        body: Some(
            r#"{
  "size": 0,
  "aggs": {
    "by_status": {
      "terms": {
        "field": "status",
        "size": 10
      }
    }
  }
}"#,
        ),
        description: "Perform a terms aggregation to count occurrence frequencies of keyword fields.",
        use_kibana: false,
    },
    ConsolePreset {
        category: "Search & Analytics",
        name: "Date Histogram Query",
        method: "POST",
        path: "/{{index}}/_search",
        body: Some(
            r#"{
  "size": 0,
  "aggs": {
    "events_over_time": {
      "date_histogram": {
        "field": "@timestamp",
        "calendar_interval": "1d"
      }
    }
  }
}"#,
        ),
        description: "Group documents by calendar days to view activity trends over time.",
        use_kibana: false,
    },
    ConsolePreset {
        category: "Search & Analytics",
        name: "Boolean Filter Query",
        method: "POST",
        path: "/{{index}}/_search",
        body: Some(
            r#"{
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
}"#,
        ),
        description: "Combine text match search terms with structured filter constraints.",
        use_kibana: false,
    },
    ConsolePreset {
        category: "Search & Analytics",
        name: "ES|QL Query",
        method: "POST",
        path: "/_query",
        body: Some(
            r#"{
  "query": "FROM {{index}} | LIMIT 10"
}"#,
        ),
        description: "Run a piped ES|QL query.",
        use_kibana: false,
    },
    ConsolePreset {
        category: "Search & Analytics",
        name: "SQL Query",
        method: "POST",
        path: "/_sql?format=txt",
        body: Some(
            r#"{
  "query": "SELECT * FROM \"{{index}}\" LIMIT 10"
}"#,
        ),
        description: "Run an Elasticsearch SQL query and return the results as a text table.",
        use_kibana: false,
    },
    // --- Ingest & ILM ---
    ConsolePreset {
        category: "Ingest & ILM",
        name: "Create Ingest Pipeline",
        method: "PUT",
        path: "/_ingest/pipeline/{{pipeline_id}}",
        body: Some(
            r#"{
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
}"#,
        ),
        description: "Define an ingest pipeline with pre-processors that manipulate document values before indexing.",
        use_kibana: false,
    },
    ConsolePreset {
        category: "Ingest & ILM",
        name: "Simulate Ingest Pipeline",
        method: "POST",
        path: "/_ingest/pipeline/{{pipeline_id}}/_simulate",
        body: Some(
            r#"{
  "docs": [
    {
      "_source": {
        "message": "Hello World",
        "status": "RUNNING"
      }
    }
  ]
}"#,
        ),
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
    ConsolePreset {
        category: "Ingest & ILM",
        name: "List Ingest Pipelines",
        method: "GET",
        path: "/_ingest/pipeline",
        body: None,
        description: "Retrieve all ingest pipeline definitions.",
        use_kibana: false,
    },
    ConsolePreset {
        category: "Ingest & ILM",
        name: "Get Ingest Pipeline",
        method: "GET",
        path: "/_ingest/pipeline/{{pipeline_id}}",
        body: None,
        description: "Retrieve one ingest pipeline definition.",
        use_kibana: false,
    },
    ConsolePreset {
        category: "Ingest & ILM",
        name: "Delete Ingest Pipeline",
        method: "DELETE",
        path: "/_ingest/pipeline/{{pipeline_id}}",
        body: None,
        description: "Delete an ingest pipeline.",
        use_kibana: false,
    },
    ConsolePreset {
        category: "Ingest & ILM",
        name: "List ILM Policies",
        method: "GET",
        path: "/_ilm/policy",
        body: None,
        description: "Retrieve all index lifecycle policies and the indices and data streams using them.",
        use_kibana: false,
    },
    ConsolePreset {
        category: "Ingest & ILM",
        name: "Get ILM Policy",
        method: "GET",
        path: "/_ilm/policy/{{ilm_policy}}",
        body: None,
        description: "Retrieve one index lifecycle policy.",
        use_kibana: false,
    },
    ConsolePreset {
        category: "Ingest & ILM",
        name: "Create ILM Policy",
        method: "PUT",
        path: "/_ilm/policy/{{ilm_policy}}",
        body: Some(
            r#"{
  "policy": {
    "phases": {
      "hot": {
        "actions": {
          "rollover": {
            "max_primary_shard_size": "50gb",
            "max_age": "30d"
          }
        }
      },
      "delete": {
        "min_age": "90d",
        "actions": {
          "delete": {}
        }
      }
    }
  }
}"#,
        ),
        description: "Create or replace an index lifecycle policy with hot rollover and delete phases.",
        use_kibana: false,
    },
    ConsolePreset {
        category: "Ingest & ILM",
        name: "Delete ILM Policy",
        method: "DELETE",
        path: "/_ilm/policy/{{ilm_policy}}",
        body: None,
        description: "Delete an index lifecycle policy that no index is using.",
        use_kibana: false,
    },
    ConsolePreset {
        category: "Ingest & ILM",
        name: "List ILM Errors",
        method: "GET",
        path: "/*/_ilm/explain?only_errors=true",
        body: None,
        description: "List only the managed indices whose lifecycle is stuck in the ERROR step.",
        use_kibana: false,
    },
    ConsolePreset {
        category: "Ingest & ILM",
        name: "Retry ILM Step",
        method: "POST",
        path: "/{{index}}/_ilm/retry",
        body: None,
        description: "Retry the failed lifecycle step for an index in the ERROR step.",
        use_kibana: false,
    },
    // --- Snapshot & Backup ---
    ConsolePreset {
        category: "Snapshot & Backup",
        name: "Register Shared FS Repo",
        method: "PUT",
        path: "/_snapshot/{{repo_name}}",
        body: Some(
            r#"{
  "type": "fs",
  "settings": {
    "location": "/mount/backups/elasticsearch",
    "compress": true
  }
}"#,
        ),
        description: "Register a shared file system repository for taking cluster snapshots.",
        use_kibana: false,
    },
    ConsolePreset {
        category: "Snapshot & Backup",
        name: "Take Dynamic Snapshot",
        method: "PUT",
        path: "/_snapshot/{{repo_name}}/{{snapshot_name}}?wait_for_completion=true",
        body: Some(
            r#"{
  "indices": "logs-*,metrics-*",
  "ignore_unavailable": true,
  "include_global_state": false,
  "metadata": {
    "taken_by": "Drastic Smurf Admin Console"
  }
}"#,
        ),
        description: "Initiate and wait for a snapshot copy of specific matching indices to complete.",
        use_kibana: false,
    },
    ConsolePreset {
        category: "Snapshot & Backup",
        name: "Restore Snapshot Index",
        method: "POST",
        path: "/_snapshot/{{repo_name}}/{{snapshot_name}}/_restore",
        body: Some(
            r#"{
  "indices": "logs-*",
  "ignore_unavailable": true,
  "include_global_state": false,
  "rename_pattern": "logs-(.+)",
  "rename_replacement": "restored-logs-$1"
}"#,
        ),
        description: "Restore an index pattern from a snapshot and rename the indices on import.",
        use_kibana: false,
    },
    ConsolePreset {
        category: "Snapshot & Backup",
        name: "List Repositories",
        method: "GET",
        path: "/_snapshot",
        body: None,
        description: "List all registered snapshot repositories and their settings.",
        use_kibana: false,
    },
    ConsolePreset {
        category: "Snapshot & Backup",
        name: "Verify Repository",
        method: "POST",
        path: "/_snapshot/{{repo_name}}/_verify",
        body: None,
        description: "Check that all master and data nodes can access a snapshot repository.",
        use_kibana: false,
    },
    ConsolePreset {
        category: "Snapshot & Backup",
        name: "List Snapshots",
        method: "GET",
        path: "/_cat/snapshots/{{repo_name}}?v&s=start_epoch:desc",
        body: None,
        description: "List the snapshots in a repository, newest first.",
        use_kibana: false,
    },
    ConsolePreset {
        category: "Snapshot & Backup",
        name: "Running Snapshot Status",
        method: "GET",
        path: "/_snapshot/_status",
        body: None,
        description: "Show shard-level progress for all snapshots currently running.",
        use_kibana: false,
    },
    ConsolePreset {
        category: "Snapshot & Backup",
        name: "Delete Snapshot",
        method: "DELETE",
        path: "/_snapshot/{{repo_name}}/{{snapshot_name}}",
        body: None,
        description: "Delete a snapshot from a repository. Danger: non-reversible!",
        use_kibana: false,
    },
    ConsolePreset {
        category: "Snapshot & Backup",
        name: "SLM Status",
        method: "GET",
        path: "/_slm/status",
        body: None,
        description: "Show whether snapshot lifecycle management is running.",
        use_kibana: false,
    },
    ConsolePreset {
        category: "Snapshot & Backup",
        name: "SLM Stats",
        method: "GET",
        path: "/_slm/stats",
        body: None,
        description: "Show snapshot lifecycle totals for snapshots taken, failed, and deleted.",
        use_kibana: false,
    },
    ConsolePreset {
        category: "Snapshot & Backup",
        name: "List SLM Policies",
        method: "GET",
        path: "/_slm/policy",
        body: None,
        description: "Retrieve all snapshot lifecycle policies with their last run and next run times.",
        use_kibana: false,
    },
    ConsolePreset {
        category: "Snapshot & Backup",
        name: "Create SLM Policy",
        method: "PUT",
        path: "/_slm/policy/{{slm_policy}}",
        body: Some(
            r#"{
  "schedule": "0 30 1 * * ?",
  "name": "<nightly-snap-{now/d}>",
  "repository": "{{repo_name}}",
  "config": {
    "indices": ["*"],
    "include_global_state": false
  },
  "retention": {
    "expire_after": "30d",
    "min_count": 5,
    "max_count": 50
  }
}"#,
        ),
        description: "Create or replace a nightly snapshot lifecycle policy with retention rules.",
        use_kibana: false,
    },
    ConsolePreset {
        category: "Snapshot & Backup",
        name: "Run SLM Policy Now",
        method: "PUT",
        path: "/_slm/policy/{{slm_policy}}/_execute",
        body: None,
        description: "Take a snapshot immediately using a snapshot lifecycle policy.",
        use_kibana: false,
    },
    ConsolePreset {
        category: "Snapshot & Backup",
        name: "Delete SLM Policy",
        method: "DELETE",
        path: "/_slm/policy/{{slm_policy}}",
        body: None,
        description: "Delete a snapshot lifecycle policy. Snapshots it already took are kept.",
        use_kibana: false,
    },
    // --- Security & API Keys ---
    ConsolePreset {
        category: "Security & API Keys",
        name: "Create API Key",
        method: "POST",
        path: "/_security/api_key",
        body: Some(
            r#"{
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
}"#,
        ),
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
        body: Some(
            r#"{
  "password" : "new-secure-password",
  "roles" : [ "kibana_admin", "read_all" ],
  "full_name" : "Operational User",
  "email" : "ops@example.com"
}"#,
        ),
        description: "Register a standard local user account with credentials and administrative role mappings.",
        use_kibana: false,
    },
    ConsolePreset {
        category: "Security & API Keys",
        name: "Get Security Roles",
        method: "GET",
        path: "/_security/role?pretty",
        body: None,
        description: "Retrieve definitions for all security roles, including cluster and index privileges.",
        use_kibana: false,
    },
    ConsolePreset {
        category: "Security & API Keys",
        name: "Get Role Mappings",
        method: "GET",
        path: "/_security/role_mapping?pretty",
        body: None,
        description: "Retrieve all configured role mappings that map external users or groups to security roles.",
        use_kibana: false,
    },
    ConsolePreset {
        category: "Security & API Keys",
        name: "Get Single Security Role",
        method: "GET",
        path: "/_security/role/{{role_name}}?pretty",
        body: None,
        description: "Retrieve the privileges and setup for a specific security role.",
        use_kibana: false,
    },
    ConsolePreset {
        category: "Security & API Keys",
        name: "Get Single Role Mapping",
        method: "GET",
        path: "/_security/role_mapping/{{mapping_name}}?pretty",
        body: None,
        description: "Retrieve rules and role assignments for a specific role mapping.",
        use_kibana: false,
    },
    ConsolePreset {
        category: "Security & API Keys",
        name: "Who Am I",
        method: "GET",
        path: "/_security/_authenticate",
        body: None,
        description: "Show the authenticated user, their roles, and the authentication realm.",
        use_kibana: false,
    },
    ConsolePreset {
        category: "Security & API Keys",
        name: "List Users",
        method: "GET",
        path: "/_security/user",
        body: None,
        description: "List all users in the native realm.",
        use_kibana: false,
    },
    ConsolePreset {
        category: "Security & API Keys",
        name: "Delete User",
        method: "DELETE",
        path: "/_security/user/{{username}}",
        body: None,
        description: "Delete a native realm user.",
        use_kibana: false,
    },
    ConsolePreset {
        category: "Security & API Keys",
        name: "Create or Update Role",
        method: "PUT",
        path: "/_security/role/{{role_name}}",
        body: Some(
            r#"{
  "cluster": ["monitor"],
  "indices": [
    {
      "names": ["logs-*"],
      "privileges": ["read", "view_index_metadata"]
    }
  ]
}"#,
        ),
        description: "Create or replace a security role with cluster and index privileges.",
        use_kibana: false,
    },
    ConsolePreset {
        category: "Security & API Keys",
        name: "Invalidate API Key",
        method: "DELETE",
        path: "/_security/api_key",
        body: Some(
            r#"{
  "ids": ["{{api_key_id}}"]
}"#,
        ),
        description: "Revoke one or more API keys by ID.",
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
        body: Some(
            r#"{
  "persistent": {
    "cluster.routing.allocation.enable": "all"
  },
  "transient": {
    "indices.recovery.max_bytes_per_sec": "40mb"
  }
}"#,
        ),
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
        body: Some(
            r#"{
  "source": {
    "index": "source-index"
  },
  "dest": {
    "index": "dest-index"
  }
}"#,
        ),
        description: "Asynchronously copy documents from one index to another.",
        use_kibana: false,
    },
    ConsolePreset {
        category: "Troubleshooting & Maintenance",
        name: "Shrink Index Template",
        method: "POST",
        path: "/{{index}}/_shrink/{{shrunk_index}}",
        body: Some(
            r#"{
  "settings": {
    "index.number_of_shards": 1,
    "index.codec": "best_compression"
  }
}"#,
        ),
        description: "Consolidate existing indices into a new, smaller index with fewer primary shards.",
        use_kibana: false,
    },
    ConsolePreset {
        category: "Troubleshooting & Maintenance",
        name: "Cancel Task",
        method: "POST",
        path: "/_tasks/{{task_id}}/_cancel",
        body: None,
        description: "Cancel a running task. The task must be cancellable.",
        use_kibana: false,
    },
    ConsolePreset {
        category: "Troubleshooting & Maintenance",
        name: "Delete By Query",
        method: "POST",
        path: "/{{index}}/_delete_by_query?conflicts=proceed",
        body: Some(
            r#"{
  "query": {
    "range": {
      "@timestamp": { "lt": "now-30d" }
    }
  }
}"#,
        ),
        description: "Delete every document matching a query. Danger: non-reversible, so review the query first!",
        use_kibana: false,
    },
    // --- Transforms, ML & Watcher ---
    ConsolePreset {
        category: "Transforms, ML & Watcher",
        name: "List Transforms",
        method: "GET",
        path: "/_transform?size=100",
        body: None,
        description: "List transform configurations.",
        use_kibana: false,
    },
    ConsolePreset {
        category: "Transforms, ML & Watcher",
        name: "Transform Stats",
        method: "GET",
        path: "/_transform/_all/_stats?size=100",
        body: None,
        description: "Show state, health, and progress for all transforms.",
        use_kibana: false,
    },
    ConsolePreset {
        category: "Transforms, ML & Watcher",
        name: "Start Transform",
        method: "POST",
        path: "/_transform/{{transform_id}}/_start",
        body: None,
        description: "Start a transform.",
        use_kibana: false,
    },
    ConsolePreset {
        category: "Transforms, ML & Watcher",
        name: "Stop Transform",
        method: "POST",
        path: "/_transform/{{transform_id}}/_stop?wait_for_completion=true",
        body: None,
        description: "Stop a transform and wait for it to finish its current checkpoint work.",
        use_kibana: false,
    },
    ConsolePreset {
        category: "Transforms, ML & Watcher",
        name: "List ML Jobs",
        method: "GET",
        path: "/_ml/anomaly_detectors",
        body: None,
        description: "List machine learning anomaly detection jobs.",
        use_kibana: false,
    },
    ConsolePreset {
        category: "Transforms, ML & Watcher",
        name: "ML Job Stats",
        method: "GET",
        path: "/_ml/anomaly_detectors/_stats",
        body: None,
        description: "Show state, memory, and data counts for anomaly detection jobs.",
        use_kibana: false,
    },
    ConsolePreset {
        category: "Transforms, ML & Watcher",
        name: "CCR Stats",
        method: "GET",
        path: "/_ccr/stats",
        body: None,
        description: "Show cross-cluster replication auto-follow and shard follower statistics.",
        use_kibana: false,
    },
    ConsolePreset {
        category: "Transforms, ML & Watcher",
        name: "Watcher Stats",
        method: "GET",
        path: "/_watcher/stats",
        body: None,
        description: "Show Watcher state and the number of watches on each node.",
        use_kibana: false,
    },
    // --- Fleet: Agent Policies ---
    ConsolePreset {
        category: "Fleet: Agent Policies",
        name: "List Agent Policies",
        method: "GET",
        path: "/api/fleet/agent_policies?perPage=100&withAgentCount=true",
        body: None,
        description: "List Elastic Agent policies with the number of agents enrolled in each.",
        use_kibana: true,
    },
    ConsolePreset {
        category: "Fleet: Agent Policies",
        name: "Get Agent Policy",
        method: "GET",
        path: "/api/fleet/agent_policies/{{agent_policy_id}}",
        body: None,
        description: "Retrieve one agent policy and its integration policies.",
        use_kibana: true,
    },
    ConsolePreset {
        category: "Fleet: Agent Policies",
        name: "Get Full Agent Policy",
        method: "GET",
        path: "/api/fleet/agent_policies/{{agent_policy_id}}/full",
        body: None,
        description: "Retrieve the compiled policy that is sent to enrolled agents.",
        use_kibana: true,
    },
    ConsolePreset {
        category: "Fleet: Agent Policies",
        name: "Create Agent Policy",
        method: "POST",
        path: "/api/fleet/agent_policies?sys_monitoring=true",
        body: Some(
            r#"{
  "name": "My agent policy",
  "namespace": "default",
  "description": "Created from Drastic Smurf",
  "monitoring_enabled": ["logs", "metrics"]
}"#,
        ),
        description: "Create an agent policy and add the System integration to it.",
        use_kibana: true,
    },
    ConsolePreset {
        category: "Fleet: Agent Policies",
        name: "Update Agent Policy",
        method: "PUT",
        path: "/api/fleet/agent_policies/{{agent_policy_id}}",
        body: Some(
            r#"{
  "name": "My agent policy",
  "namespace": "default",
  "description": "Updated from Drastic Smurf",
  "monitoring_enabled": ["logs", "metrics"]
}"#,
        ),
        description: "Update an agent policy's name, namespace, description, and monitoring settings.",
        use_kibana: true,
    },
    ConsolePreset {
        category: "Fleet: Agent Policies",
        name: "Copy Agent Policy",
        method: "POST",
        path: "/api/fleet/agent_policies/{{agent_policy_id}}/copy",
        body: Some(
            r#"{
  "name": "Copy of my agent policy",
  "description": "Copied from Drastic Smurf"
}"#,
        ),
        description: "Duplicate an agent policy and its integration policies.",
        use_kibana: true,
    },
    ConsolePreset {
        category: "Fleet: Agent Policies",
        name: "Delete Agent Policy",
        method: "POST",
        path: "/api/fleet/agent_policies/delete",
        body: Some(
            r#"{
  "agentPolicyId": "{{agent_policy_id}}"
}"#,
        ),
        description: "Delete an agent policy. Fleet rejects this while agents are still enrolled in the policy.",
        use_kibana: true,
    },
    // --- Fleet: Integration Policies ---
    ConsolePreset {
        category: "Fleet: Integration Policies",
        name: "List Integration Policies",
        method: "GET",
        path: "/api/fleet/package_policies?perPage=100",
        body: None,
        description: "List the integration (package) policies attached to agent policies.",
        use_kibana: true,
    },
    ConsolePreset {
        category: "Fleet: Integration Policies",
        name: "Find Policies by Package",
        method: "GET",
        path: "/api/fleet/package_policies?perPage=100&kuery=ingest-package-policies.package.name:{{package_name}}",
        body: None,
        description: "List the integration policies that use a given package.",
        use_kibana: true,
    },
    ConsolePreset {
        category: "Fleet: Integration Policies",
        name: "Get Integration Policy",
        method: "GET",
        path: "/api/fleet/package_policies/{{package_policy_id}}",
        body: None,
        description: "Retrieve one integration policy.",
        use_kibana: true,
    },
    ConsolePreset {
        category: "Fleet: Integration Policies",
        name: "Create Integration Policy",
        method: "POST",
        path: "/api/fleet/package_policies",
        body: Some(
            r#"{
  "name": "nginx-1",
  "namespace": "default",
  "policy_ids": ["{{agent_policy_id}}"],
  "package": {
    "name": "nginx",
    "version": "{{package_version}}"
  },
  "inputs": {}
}"#,
        ),
        description: "Add an integration to an agent policy using the simplified format. Leaving inputs empty uses the package defaults.",
        use_kibana: true,
    },
    ConsolePreset {
        category: "Fleet: Integration Policies",
        name: "Update Integration Policy",
        method: "PUT",
        path: "/api/fleet/package_policies/{{package_policy_id}}",
        body: Some(
            r#"{
  "name": "nginx-1",
  "namespace": "default",
  "enabled": true,
  "policy_ids": ["{{agent_policy_id}}"],
  "package": {
    "name": "nginx",
    "version": "{{package_version}}"
  },
  "inputs": {}
}"#,
        ),
        description: "Update an integration policy using the simplified format.",
        use_kibana: true,
    },
    ConsolePreset {
        category: "Fleet: Integration Policies",
        name: "Delete Integration Policy",
        method: "DELETE",
        path: "/api/fleet/package_policies/{{package_policy_id}}",
        body: None,
        description: "Remove one integration policy from its agent policies.",
        use_kibana: true,
    },
    ConsolePreset {
        category: "Fleet: Integration Policies",
        name: "Bulk Delete Integration Policies",
        method: "POST",
        path: "/api/fleet/package_policies/delete",
        body: Some(
            r#"{
  "packagePolicyIds": ["{{package_policy_id}}"]
}"#,
        ),
        description: "Remove several integration policies in one request.",
        use_kibana: true,
    },
    ConsolePreset {
        category: "Fleet: Integration Policies",
        name: "Dry Run Policy Upgrade",
        method: "POST",
        path: "/api/fleet/package_policies/upgrade/dryrun",
        body: Some(
            r#"{
  "packagePolicyIds": ["{{package_policy_id}}"]
}"#,
        ),
        description: "Preview an integration policy upgrade and report any conflicts without changing anything.",
        use_kibana: true,
    },
    ConsolePreset {
        category: "Fleet: Integration Policies",
        name: "Upgrade Integration Policy",
        method: "POST",
        path: "/api/fleet/package_policies/upgrade",
        body: Some(
            r#"{
  "packagePolicyIds": ["{{package_policy_id}}"]
}"#,
        ),
        description: "Upgrade integration policies to the installed package version.",
        use_kibana: true,
    },
    // --- Fleet: Integration Packages ---
    ConsolePreset {
        category: "Fleet: Integration Packages",
        name: "List Available Integrations",
        method: "GET",
        path: "/api/fleet/epm/packages",
        body: None,
        description: "List the integration packages available from the package registry.",
        use_kibana: true,
    },
    ConsolePreset {
        category: "Fleet: Integration Packages",
        name: "List Installed Integrations",
        method: "GET",
        path: "/api/fleet/epm/packages/installed?perPage=100",
        body: None,
        description: "List the integration packages installed in this Kibana.",
        use_kibana: true,
    },
    ConsolePreset {
        category: "Fleet: Integration Packages",
        name: "Get Integration Package",
        method: "GET",
        path: "/api/fleet/epm/packages/{{package_name}}",
        body: None,
        description: "Retrieve a package's details, including its latest version and install status.",
        use_kibana: true,
    },
    ConsolePreset {
        category: "Fleet: Integration Packages",
        name: "Install Integration Package",
        method: "POST",
        path: "/api/fleet/epm/packages/{{package_name}}/{{package_version}}",
        body: Some(
            r#"{
  "force": false
}"#,
        ),
        description: "Install a package version from the registry, including its assets and templates.",
        use_kibana: true,
    },
    ConsolePreset {
        category: "Fleet: Integration Packages",
        name: "Uninstall Integration Package",
        method: "DELETE",
        path: "/api/fleet/epm/packages/{{package_name}}/{{package_version}}",
        body: None,
        description: "Uninstall a package and its assets. Fleet rejects this while integration policies still use it.",
        use_kibana: true,
    },
    // --- Fleet: Agents & Enrollment ---
    ConsolePreset {
        category: "Fleet: Agents & Enrollment",
        name: "List Agents",
        method: "GET",
        path: "/api/fleet/agents?perPage=100",
        body: None,
        description: "List enrolled Elastic Agents.",
        use_kibana: true,
    },
    ConsolePreset {
        category: "Fleet: Agents & Enrollment",
        name: "Agent Status Summary",
        method: "GET",
        path: "/api/fleet/agent_status",
        body: None,
        description: "Show agent counts by status, such as online, offline, and error.",
        use_kibana: true,
    },
    ConsolePreset {
        category: "Fleet: Agents & Enrollment",
        name: "Get Agent",
        method: "GET",
        path: "/api/fleet/agents/{{agent_id}}",
        body: None,
        description: "Retrieve one agent's status, version, and assigned policy.",
        use_kibana: true,
    },
    ConsolePreset {
        category: "Fleet: Agents & Enrollment",
        name: "Reassign Agent",
        method: "POST",
        path: "/api/fleet/agents/{{agent_id}}/reassign",
        body: Some(
            r#"{
  "policy_id": "{{agent_policy_id}}"
}"#,
        ),
        description: "Move an agent to a different agent policy.",
        use_kibana: true,
    },
    ConsolePreset {
        category: "Fleet: Agents & Enrollment",
        name: "Upgrade Agent",
        method: "POST",
        path: "/api/fleet/agents/{{agent_id}}/upgrade",
        body: Some(
            r#"{
  "version": "{{agent_version}}"
}"#,
        ),
        description: "Upgrade an agent to a specific version.",
        use_kibana: true,
    },
    ConsolePreset {
        category: "Fleet: Agents & Enrollment",
        name: "Unenroll Agent",
        method: "POST",
        path: "/api/fleet/agents/{{agent_id}}/unenroll",
        body: Some(
            r#"{
  "revoke": false
}"#,
        ),
        description: "Unenroll an agent from Fleet. Set revoke to true to invalidate its API keys immediately.",
        use_kibana: true,
    },
    ConsolePreset {
        category: "Fleet: Agents & Enrollment",
        name: "List Enrollment Keys",
        method: "GET",
        path: "/api/fleet/enrollment_api_keys",
        body: None,
        description: "List enrollment tokens. Responses include the secret token values.",
        use_kibana: true,
    },
    ConsolePreset {
        category: "Fleet: Agents & Enrollment",
        name: "Create Enrollment Key",
        method: "POST",
        path: "/api/fleet/enrollment_api_keys",
        body: Some(
            r#"{
  "name": "My enrollment key",
  "policy_id": "{{agent_policy_id}}"
}"#,
        ),
        description: "Create an enrollment token for an agent policy.",
        use_kibana: true,
    },
    ConsolePreset {
        category: "Fleet: Agents & Enrollment",
        name: "Revoke Enrollment Key",
        method: "DELETE",
        path: "/api/fleet/enrollment_api_keys/{{enrollment_key_id}}",
        body: None,
        description: "Revoke an enrollment token so it can no longer enroll agents.",
        use_kibana: true,
    },
    ConsolePreset {
        category: "Fleet: Agents & Enrollment",
        name: "Fleet Setup Status",
        method: "GET",
        path: "/api/fleet/agents/setup",
        body: None,
        description: "Check whether Fleet is ready and list any missing requirements.",
        use_kibana: true,
    },
    ConsolePreset {
        category: "Fleet: Agents & Enrollment",
        name: "List Fleet Outputs",
        method: "GET",
        path: "/api/fleet/outputs",
        body: None,
        description: "List the outputs that agents send data to.",
        use_kibana: true,
    },
    ConsolePreset {
        category: "Fleet: Agents & Enrollment",
        name: "List Fleet Server Hosts",
        method: "GET",
        path: "/api/fleet/fleet_server_hosts",
        body: None,
        description: "List the Fleet Server hosts that agents connect to.",
        use_kibana: true,
    },
    // --- Kibana Data Views ---
    ConsolePreset {
        category: "Kibana Data Views",
        name: "List Data Views",
        method: "GET",
        path: "/api/data_views",
        body: None,
        description: "List all data views in the current space.",
        use_kibana: true,
    },
    ConsolePreset {
        category: "Kibana Data Views",
        name: "Get Data View",
        method: "GET",
        path: "/api/data_views/data_view/{{data_view_id}}",
        body: None,
        description: "Retrieve one data view, including its fields.",
        use_kibana: true,
    },
    ConsolePreset {
        category: "Kibana Data Views",
        name: "Create Data View",
        method: "POST",
        path: "/api/data_views/data_view",
        body: Some(
            r#"{
  "data_view": {
    "title": "logs-*",
    "name": "All logs",
    "timeFieldName": "@timestamp"
  }
}"#,
        ),
        description: "Create a data view. The title is the index pattern to match, and name is the label shown in Kibana.",
        use_kibana: true,
    },
    ConsolePreset {
        category: "Kibana Data Views",
        name: "Update Data View",
        method: "POST",
        path: "/api/data_views/data_view/{{data_view_id}}",
        body: Some(
            r#"{
  "data_view": {
    "name": "All logs",
    "timeFieldName": "@timestamp"
  },
  "refresh_fields": true
}"#,
        ),
        description: "Update a data view and refresh its field list.",
        use_kibana: true,
    },
    ConsolePreset {
        category: "Kibana Data Views",
        name: "Delete Data View",
        method: "DELETE",
        path: "/api/data_views/data_view/{{data_view_id}}",
        body: None,
        description: "Delete a data view. Dashboards and saved searches that use it will stop working.",
        use_kibana: true,
    },
    ConsolePreset {
        category: "Kibana Data Views",
        name: "Get Default Data View",
        method: "GET",
        path: "/api/data_views/default",
        body: None,
        description: "Show which data view is the default in this space.",
        use_kibana: true,
    },
    ConsolePreset {
        category: "Kibana Data Views",
        name: "Set Default Data View",
        method: "POST",
        path: "/api/data_views/default",
        body: Some(
            r#"{
  "data_view_id": "{{data_view_id}}",
  "force": true
}"#,
        ),
        description: "Make a data view the default in this space, replacing any existing default.",
        use_kibana: true,
    },
    ConsolePreset {
        category: "Kibana Data Views",
        name: "Add Runtime Field",
        method: "POST",
        path: "/api/data_views/data_view/{{data_view_id}}/runtime_field",
        body: Some(
            r#"{
  "name": "my_runtime_field",
  "runtimeField": {
    "type": "keyword",
    "script": {
      "source": "emit(doc['host.name'].value)"
    }
  }
}"#,
        ),
        description: "Add a runtime field to a data view.",
        use_kibana: true,
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
        body: Some(
            r##"{
  "id": "engineering",
  "name": "Engineering",
  "description": "Workspace for engineering dashboards",
  "color": "#aabbcc"
}"##,
        ),
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
    ConsolePreset {
        category: "Kibana Spaces & Security",
        name: "Get Space",
        method: "GET",
        path: "/api/spaces/space/{{space_id}}",
        body: None,
        description: "Retrieve one Kibana space.",
        use_kibana: true,
    },
    ConsolePreset {
        category: "Kibana Spaces & Security",
        name: "Delete Space",
        method: "DELETE",
        path: "/api/spaces/space/{{space_id}}",
        body: None,
        description: "Delete a Kibana space and all of the saved objects in it. Danger: non-reversible!",
        use_kibana: true,
    },
    ConsolePreset {
        category: "Kibana Spaces & Security",
        name: "Copy Objects to Space",
        method: "POST",
        path: "/api/spaces/_copy_saved_objects",
        body: Some(
            r#"{
  "objects": [
    { "type": "dashboard", "id": "my-dashboard" }
  ],
  "spaces": ["{{space_id}}"],
  "includeReferences": true
}"#,
        ),
        description: "Copy saved objects and the objects they reference into another space.",
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
        body: Some(
            r#"{
  "type": "dashboard",
  "limit": 100
}"#,
        ),
        description: "Export configured dashboards, maps, or saved searches in NDJSON format.",
        use_kibana: true,
    },
    ConsolePreset {
        category: "Kibana Saved Objects",
        name: "Create Saved Dashboard",
        method: "POST",
        path: "/api/saved_objects/dashboard/my-dashboard-id",
        body: Some(
            r#"{
  "attributes": {
    "title": "My Production Dashboard",
    "description": "Dashboard created programmatically",
    "panelsJSON": "[]",
    "optionsJSON": "{\"darkTheme\":true}",
    "timeRestore": false
  }
}"#,
        ),
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
    // --- Kibana Alerting & Connectors ---
    ConsolePreset {
        category: "Kibana Alerting & Connectors",
        name: "Alerting Health",
        method: "GET",
        path: "/api/alerting/_health",
        body: None,
        description: "Check whether the alerting framework is healthy and has its prerequisites in place.",
        use_kibana: true,
    },
    ConsolePreset {
        category: "Kibana Alerting & Connectors",
        name: "Task Manager Health",
        method: "GET",
        path: "/api/task_manager/_health",
        body: None,
        description: "Show task manager throughput, backlog, and capacity. Useful when rules run late.",
        use_kibana: true,
    },
    ConsolePreset {
        category: "Kibana Alerting & Connectors",
        name: "List Rule Types",
        method: "GET",
        path: "/api/alerting/rule_types",
        body: None,
        description: "List the alerting rule types available in this Kibana.",
        use_kibana: true,
    },
    ConsolePreset {
        category: "Kibana Alerting & Connectors",
        name: "Find Alerting Rules",
        method: "GET",
        path: "/api/alerting/rules/_find?per_page=100",
        body: None,
        description: "List alerting rules with their schedule and last run status.",
        use_kibana: true,
    },
    ConsolePreset {
        category: "Kibana Alerting & Connectors",
        name: "Get Alerting Rule",
        method: "GET",
        path: "/api/alerting/rule/{{rule_id}}",
        body: None,
        description: "Retrieve one alerting rule.",
        use_kibana: true,
    },
    ConsolePreset {
        category: "Kibana Alerting & Connectors",
        name: "Enable Alerting Rule",
        method: "POST",
        path: "/api/alerting/rule/{{rule_id}}/_enable",
        body: None,
        description: "Enable an alerting rule.",
        use_kibana: true,
    },
    ConsolePreset {
        category: "Kibana Alerting & Connectors",
        name: "Disable Alerting Rule",
        method: "POST",
        path: "/api/alerting/rule/{{rule_id}}/_disable",
        body: None,
        description: "Disable an alerting rule.",
        use_kibana: true,
    },
    ConsolePreset {
        category: "Kibana Alerting & Connectors",
        name: "Delete Alerting Rule",
        method: "DELETE",
        path: "/api/alerting/rule/{{rule_id}}",
        body: None,
        description: "Delete an alerting rule.",
        use_kibana: true,
    },
    ConsolePreset {
        category: "Kibana Alerting & Connectors",
        name: "List Connectors",
        method: "GET",
        path: "/api/actions/connectors",
        body: None,
        description: "List the connectors in the current space.",
        use_kibana: true,
    },
    ConsolePreset {
        category: "Kibana Alerting & Connectors",
        name: "List Connector Types",
        method: "GET",
        path: "/api/actions/connector_types",
        body: None,
        description: "List the connector types and whether each is enabled and licensed.",
        use_kibana: true,
    },
    // --- Kibana Cases, SLOs & Security ---
    ConsolePreset {
        category: "Kibana Cases, SLOs & Security",
        name: "Search Cases",
        method: "GET",
        path: "/api/cases/_find?perPage=100",
        body: None,
        description: "List cases in the current space.",
        use_kibana: true,
    },
    ConsolePreset {
        category: "Kibana Cases, SLOs & Security",
        name: "Create Case",
        method: "POST",
        path: "/api/cases",
        body: Some(
            r#"{
  "title": "My case",
  "description": "Created from Drastic Smurf",
  "owner": "cases",
  "tags": [],
  "settings": {
    "syncAlerts": false
  },
  "connector": {
    "id": "none",
    "name": "none",
    "type": ".none",
    "fields": null
  }
}"#,
        ),
        description: "Create a case with no external connector.",
        use_kibana: true,
    },
    ConsolePreset {
        category: "Kibana Cases, SLOs & Security",
        name: "List SLOs",
        method: "GET",
        path: "/s/{{space_id}}/api/observability/slos?perPage=100",
        body: None,
        description: "List service level objectives in a space. Use default for the default space.",
        use_kibana: true,
    },
    ConsolePreset {
        category: "Kibana Cases, SLOs & Security",
        name: "Find Detection Rules",
        method: "GET",
        path: "/api/detection_engine/rules/_find?per_page=100",
        body: None,
        description: "List Elastic Security detection rules.",
        use_kibana: true,
    },
    ConsolePreset {
        category: "Kibana Cases, SLOs & Security",
        name: "List Synthetic Monitors",
        method: "GET",
        path: "/api/synthetics/monitors?per_page=100",
        body: None,
        description: "List Synthetics monitors and their configuration.",
        use_kibana: true,
    },
];

/// Case-insensitive title match. Every whitespace-separated word in the filter must appear.
pub fn preset_matches_filter(name: &str, filter: &str) -> bool {
    let name = name.to_lowercase();
    filter
        .to_lowercase()
        .split_whitespace()
        .all(|word| name.contains(word))
}

/// Render the collapsible, filterable preset catalogue. Returns the preset the user clicked, if any.
/// `id_ns` namespaces the collapsing-header ids so multiple consoles keep independent open state.
pub fn render_preset_list(
    ui: &mut Ui,
    filter: &str,
    id_ns: &str,
) -> Option<&'static ConsolePreset> {
    let mut picked = None;
    let filter_active = !filter.trim().is_empty();
    let mut any_match = false;
    for group in PRESETS.chunk_by(|a, b| a.category == b.category) {
        let matches: Vec<&'static ConsolePreset> = group
            .iter()
            .filter(|p| preset_matches_filter(p.name, filter))
            .collect();
        if matches.is_empty() {
            continue;
        }
        any_match = true;

        let category = group[0].category;
        let header = egui::CollapsingHeader::new(
            egui::RichText::new(format!("{} ({})", category, matches.len()))
                .strong()
                .color(crate::ui::theme::Theme::text_secondary())
                .size(12.0),
        );
        // Filtering forces sections open under a separate id, so clearing
        // the filter restores whatever the user had expanded.
        let header = if filter_active {
            header
                .id_salt((id_ns, "filtered", category))
                .default_open(true)
                .open(Some(true))
        } else {
            header.id_salt((id_ns, category)).default_open(false)
        };
        header.show(ui, |ui| {
            for preset in matches {
                let btn_label = if preset.use_kibana {
                    format!("🎨 {}", preset.name)
                } else {
                    preset.name.to_string()
                };

                let btn =
                    ui.add(egui::Button::new(btn_label).fill(crate::ui::theme::Theme::bg_input()));
                if btn.clicked() {
                    picked = Some(preset);
                }
                btn.on_hover_ui(|ui| {
                    ui.set_max_width(220.0);
                    let prefix = if preset.use_kibana {
                        "🎨 Kibana API"
                    } else {
                        "🔌 Elasticsearch API"
                    };
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
                    if let Some(docs) = api_examples::lookup(preset.method, preset.path) {
                        let mut parts = Vec::new();
                        if !docs.request.is_empty() {
                            parts.push(format!("{} request", docs.request.len()));
                        }
                        if !docs.response.is_empty() {
                            parts.push(format!("{} response", docs.response.len()));
                        }
                        if !parts.is_empty() {
                            ui.label(
                                egui::RichText::new(format!(
                                    "📖 API doc examples: {}",
                                    parts.join(", ")
                                ))
                                .size(11.0)
                                .color(crate::ui::theme::Theme::text_muted()),
                            );
                        }
                        // egui keeps a tooltip open while the pointer moves onto it when
                        // it holds a link, so this is clickable.
                        if let Some(url) = &docs.doc_url {
                            ui.hyperlink_to(
                                egui::RichText::new("🔗 Open API docs ↗").size(11.0),
                                url,
                            )
                            .on_hover_text(url);
                        }
                    }
                });
                ui.add_space(3.0);
            }
        });
    }

    if !any_match {
        ui.label(
            egui::RichText::new("No commands match this filter.")
                .color(crate::ui::theme::Theme::text_muted())
                .size(11.0),
        );
    }
    picked
}

/// Colour used for an HTTP method label.
pub fn method_color(method: &str) -> egui::Color32 {
    match method {
        "GET" => crate::ui::theme::Theme::success(),
        "POST" => crate::ui::theme::Theme::warning(),
        "PUT" => crate::ui::theme::Theme::accent(),
        "DELETE" => crate::ui::theme::Theme::danger(),
        _ => crate::ui::theme::Theme::text_secondary(),
    }
}

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

    if state.examples_prompt
        && let Some(choice) =
            api_examples::render_examples_prompt(ui, "console_examples_prompt", &mut state.body)
    {
        state.examples_prompt = false;
        state.json_error = None;
        if choice == PromptChoice::RemovedAndSend {
            state.send_requested = true;
            ui.ctx().request_repaint();
        }
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
                                let active_bg = crate::ui::theme::Theme::accent().linear_multiply(0.25);
                                text.color(crate::ui::theme::Theme::contrast_text_color(active_bg)).strong()
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

                    // Preset filter stays above the scroll area so it remains visible
                    if state.active_tab == ConsoleTab::Presets {
                        ui.horizontal(|ui| {
                            let clear_width = 24.0;
                            ui.add(
                                egui::TextEdit::singleline(&mut state.preset_filter)
                                    .hint_text("🔍 Filter commands by title...")
                                    .desired_width(ui.available_width() - clear_width - ui.spacing().item_spacing.x)
                            );
                            let has_filter = !state.preset_filter.is_empty();
                            let clear_btn = ui.add_enabled(
                                has_filter,
                                egui::Button::new("✖").min_size(egui::vec2(clear_width, 0.0)),
                            );
                            if clear_btn.clicked() {
                                state.preset_filter.clear();
                            }
                        });
                        ui.add_space(6.0);
                    }

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

                                    if let Some(preset) = render_preset_list(ui, &state.preset_filter, "console_preset") {
                                        state.apply_preset(preset);
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

                                            let is_loaded = state.history_index == Some(rev_idx);
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
                                for &m in CONSOLE_METHODS {
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
                            let accent_color = crate::ui::theme::Theme::accent();
                            let send_btn = ui.add_enabled(
                                !state.is_loading,
                                egui::Button::new(
                                    egui::RichText::new("⚡ Send")
                                        .color(crate::ui::theme::Theme::contrast_text_color(accent_color))
                                )
                                .fill(accent_color),
                            );
                            if state.is_loading {
                                ui.spinner();
                            }

                            let send_now = (send_btn.clicked() || std::mem::take(&mut state.send_requested))
                                && !state.is_loading;
                            if send_now && api_examples::has_examples(&state.body) {
                                state.examples_prompt = true;
                            } else if send_now {
                                state.clear_example_response();
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
                        let available = state.available_examples();
                        if api_examples::examples_toolbar(ui, &mut state.body, available.as_deref()) {
                            state.json_error = None;
                        }
                        if let Some(ref err) = state.json_error {
                            ui.colored_label(crate::ui::theme::Theme::danger(), format!("⚠️ {}", err));
                        }
                        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                            if ui.button("✨ Prettify JSON")
                                .on_hover_text("Format and auto-indent the JSON body")
                                .clicked()
                                && !state.body.trim().is_empty() {
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
                            .desired_rows(4)
                            .layouter(&mut |ui, text, wrap_width| {
                                crate::ui::widgets::json_layouter(ui, text, wrap_width)
                            }),
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
                        if ui.small_button("📋 Copy JSON").clicked() {
                            let text_to_copy = state.full_response.as_ref().unwrap_or(&state.response).clone();
                            ui.ctx().copy_text(text_to_copy);
                        }
                        if ui.small_button("Clear").clicked() {
                            state.response.clear();
                            state.full_response = None;
                            state.response_is_example = false;
                        }
                        if state.response_is_example {
                            ui.label(
                                egui::RichText::new("📖 example from the API docs; cleared when you send")
                                    .small()
                                    .color(crate::ui::theme::Theme::text_muted()),
                            );
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
                                    .desired_rows(6)
                                    .layouter(&mut |ui, text, wrap_width| {
                                        crate::ui::widgets::json_layouter(ui, text, wrap_width)
                                    }),
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
    if api_examples::has_examples(body) {
        return Err("Body contains API doc examples; remove them first (🧹)".to_string());
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
    let parsed =
        serde_json::from_str::<serde_json::Value>(&parsed_ready).map_err(|e| e.to_string())?;

    let mut formatted = serde_json::to_string_pretty(&parsed).map_err(|e| e.to_string())?;

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
    fn test_apply_preset_inserts_doc_examples() {
        let create = PRESETS.iter().find(|p| p.name == "Create Index").unwrap();
        let mut state = ConsoleState::new();
        state.response = "real response".to_string();
        state.apply_preset(create);
        assert!(state.body.starts_with("####### EXAMPLE 0: "));
        assert_eq!(
            api_examples::keep_only(&state.body, 0).as_deref(),
            Some(create.body.unwrap().trim())
        );
        assert_eq!(api_examples::remove_example_blocks(&state.body), "");
        let inserted = state.body.clone();
        state.body.clear();
        assert_eq!(
            state.available_examples().as_deref(),
            Some(inserted.as_str())
        );
        state.path = "/other".to_string();
        assert!(
            state.available_examples().is_none(),
            "examples belong to one API"
        );
        state.path = create.path.to_string();
        state.body = inserted;
        assert!(prettify_json_body(&state.body).is_err());
        // Create Index has no documented response example: the real response stays.
        assert_eq!(state.response, "real response");
        assert!(!state.response_is_example);

        let health = PRESETS.iter().find(|p| p.name == "Cluster Health").unwrap();
        state.apply_preset(health);
        assert_eq!(state.body, "", "GET health has no body examples");
        assert!(state.response_is_example);
        assert!(state.response.starts_with(api_examples::EXAMPLE_MARKER));

        // Switching to a preset without a response example clears the old example.
        state.apply_preset(create);
        assert!(!state.response_is_example);
        assert!(state.response.is_empty());
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

    #[test]
    fn test_preset_matches_filter() {
        assert!(preset_matches_filter("Create Agent Policy", ""));
        assert!(preset_matches_filter("Create Agent Policy", "   "));
        assert!(preset_matches_filter("Create Agent Policy", "agent"));
        assert!(preset_matches_filter("Create Agent Policy", "AGENT policy"));
        assert!(preset_matches_filter(
            "Create Agent Policy",
            "policy create"
        ));
        assert!(!preset_matches_filter("Create Agent Policy", "delete"));
        assert!(!preset_matches_filter(
            "Create Agent Policy",
            "agent delete"
        ));
    }

    #[test]
    fn test_preset_bodies_are_valid_json() {
        for preset in PRESETS {
            if let Some(body) = preset.body {
                assert!(
                    serde_json::from_str::<serde_json::Value>(body).is_ok(),
                    "invalid JSON body in preset '{}'",
                    preset.name
                );
            }
        }
    }

    #[test]
    fn test_preset_methods_can_be_sent() {
        for preset in PRESETS {
            assert!(
                CONSOLE_METHODS.contains(&preset.method),
                "preset '{}' uses unsupported method {}",
                preset.name,
                preset.method
            );
        }
    }

    #[test]
    fn test_preset_categories_are_contiguous_and_names_unique() {
        // A split category would render two collapsing headers with the same id.
        let mut seen = std::collections::HashSet::new();
        for group in PRESETS.chunk_by(|a, b| a.category == b.category) {
            assert!(
                seen.insert(group[0].category),
                "category '{}' is split",
                group[0].category
            );
        }

        let mut names = std::collections::HashSet::new();
        for preset in PRESETS {
            assert!(
                names.insert(preset.name),
                "duplicate preset name '{}'",
                preset.name
            );
        }
    }

    fn collect_text(shape: &egui::Shape, out: &mut Vec<(String, egui::Rect)>) {
        match shape {
            egui::Shape::Vec(shapes) => shapes.iter().for_each(|s| collect_text(s, out)),
            egui::Shape::Text(text) => {
                let rect = egui::Rect::from_min_size(text.pos, text.galley.size());
                out.push((text.galley.text().to_string(), rect));
            }
            _ => {}
        }
    }

    /// Renders the console headlessly and returns the painted text with its position.
    fn render_presets(ctx: &egui::Context, state: &mut ConsoleState) -> Vec<(String, egui::Rect)> {
        let clusters = vec!["test".to_string()];
        let screen = egui::Rect::from_min_size(egui::Pos2::ZERO, egui::vec2(1400.0, 4000.0));
        let mut text = Vec::new();
        for _ in 0..3 {
            let input = egui::RawInput {
                screen_rect: Some(screen),
                ..Default::default()
            };
            let output = ctx.run(input, |ctx| {
                egui::CentralPanel::default().show(ctx, |ui| {
                    render_console_module(ui, state, &clusters, &mut None, &mut None, &mut None);
                });
            });
            text.clear();
            output
                .shapes
                .iter()
                .for_each(|s| collect_text(&s.shape, &mut text));
        }
        text
    }

    fn has_text(text: &[(String, egui::Rect)], wanted: &str) -> bool {
        text.iter().any(|(t, _)| t.contains(wanted))
    }

    #[test]
    fn test_preset_sections_collapsed_until_filtered() {
        let ctx = egui::Context::default();
        let mut state = ConsoleState::new();
        state.selected_cluster = "test".to_string();

        let text = render_presets(&ctx, &mut state);
        assert!(has_text(&text, "Fleet: Agent Policies (7)"));
        assert!(
            !has_text(&text, "Create Agent Policy"),
            "sections must start collapsed"
        );

        state.preset_filter = "create agent".to_string();
        let text = render_presets(&ctx, &mut state);
        assert!(has_text(&text, "Fleet: Agent Policies (1)"));
        assert!(
            has_text(&text, "Create Agent Policy"),
            "matching sections must open"
        );
        assert!(!has_text(&text, "Delete Agent Policy"));
        assert!(
            !has_text(&text, "Cluster Status & Info"),
            "empty sections must be hidden"
        );

        state.preset_filter = "no such command".to_string();
        let text = render_presets(&ctx, &mut state);
        assert!(has_text(&text, "No commands match this filter."));

        state.preset_filter.clear();
        let text = render_presets(&ctx, &mut state);
        assert!(has_text(&text, "Fleet: Agent Policies (7)"));
        assert!(
            !has_text(&text, "Create Agent Policy"),
            "clearing the filter must re-collapse"
        );
    }

    #[test]
    fn test_preset_titles_fit_workspace_panel() {
        let ctx = egui::Context::default();
        let mut state = ConsoleState::new();
        state.selected_cluster = "test".to_string();

        for preset in PRESETS {
            state.preset_filter = preset.name.to_string();
            let text = render_presets(&ctx, &mut state);
            let left = text
                .iter()
                .find(|(t, _)| t == "WORKSPACE")
                .map(|(_, r)| r.left())
                .expect("workspace title should render");
            for wanted in [preset.name, preset.category] {
                let (_, rect) = text
                    .iter()
                    .find(|(t, _)| t.contains(wanted))
                    .unwrap_or_else(|| panic!("'{wanted}' should render"));
                assert!(
                    rect.right() <= left + 280.0,
                    "'{wanted}' is {:.0}px wide and overflows the panel",
                    rect.width()
                );
            }
        }
    }
}
