use std::time::Duration;

use anyhow::{Context, Result};
use reqwest::{Client, ClientBuilder, RequestBuilder, StatusCode};
use serde::de::DeserializeOwned;

use crate::core::auth;
use crate::core::config::{CaCert, ClusterConfig};

#[derive(Debug, thiserror::Error)]
pub enum EsError {
    #[error("Cluster unreachable: {0}")]
    Unreachable(String),
    #[error("HTTP {status}: {message}")]
    Http { status: StatusCode, message: String },
    #[error("JSON parse error: {0}")]
    Parse(String),
    #[error("Missing password for cluster '{0}'")]
    MissingPassword(String),
    #[error("Request failed: {0}")]
    Request(String),
}

#[derive(Debug, Clone)]
pub struct EsClient {
    config: ClusterConfig,
    client: Client,
    password: String,
    tunnel_url: Option<String>,
}

impl EsClient {
    fn build_client(config: &ClusterConfig) -> Result<Client> {
        let mut builder = ClientBuilder::new()
            .timeout(Duration::from_secs(30))
            .connect_timeout(Duration::from_secs(10));

        builder = match &config.ca_cert {
            CaCert::System => builder,
            CaCert::Bundled => {
                // TODO: load bundled CA cert
                builder
            }
            CaCert::Custom(path) => {
                let cert = std::fs::read(path).context("Failed to read custom CA certificate")?;
                let cert = reqwest::Certificate::from_pem(&cert)
                    .context("Invalid custom CA certificate")?;
                builder.add_root_certificate(cert)
            }
        };

        if !config.verify_ssl {
            builder = builder.danger_accept_invalid_certs(true);
        }

        builder.build().context("Failed to build HTTP client")
    }

    pub fn new(config: &ClusterConfig) -> Result<Self> {
        let password = auth::get_password(&config.name)
            .ok()
            .flatten()
            .unwrap_or_default();

        let client = Self::build_client(config)?;

        Ok(Self {
            config: config.clone(),
            client,
            password,
            tunnel_url: None,
        })
    }

    pub fn with_password(config: &ClusterConfig, password: impl Into<String>) -> Result<Self> {
        let client = Self::build_client(config)?;
        Ok(Self {
            config: config.clone(),
            client,
            password: password.into(),
            tunnel_url: None,
        })
    }

    pub fn with_tunnel(mut self, tunnel_url: impl Into<String>) -> Self {
        self.tunnel_url = Some(tunnel_url.into());
        self
    }

    fn request(&self, method: reqwest::Method, path: &str) -> RequestBuilder {
        let host = self
            .tunnel_url
            .as_deref()
            .unwrap_or(self.config.host.trim());
        let host = if host.starts_with("http://") || host.starts_with("https://") {
            host.to_string()
        } else {
            format!("http://{}", host)
        };
        let url = format!("{}{}", host.trim_end_matches('/'), path);
        self.client
            .request(method, &url)
            .basic_auth(&self.config.username, Some(&self.password))
            .header("Content-Type", "application/json")
    }

    async fn exec<T: DeserializeOwned>(&self, req: RequestBuilder) -> Result<T, EsError> {
        let resp = req
            .send()
            .await
            .map_err(|e| EsError::Unreachable(e.to_string()))?;

        let status = resp.status();
        if !status.is_success() {
            let text = resp.text().await.unwrap_or_default();
            return Err(EsError::Http {
                status,
                message: text,
            });
        }

        resp.json().await.map_err(|e| EsError::Parse(e.to_string()))
    }

    pub async fn cluster_health(&self) -> Result<ClusterHealth, EsError> {
        self.exec(self.request(reqwest::Method::GET, "/_cluster/health"))
            .await
    }

    pub async fn snapshot_current(&self, repo: &str) -> Result<SnapshotResponse, EsError> {
        let path = format!("/_snapshot/{}/_current", repo);
        self.exec(self.request(reqwest::Method::GET, &path)).await
    }

    pub async fn snapshot_status_all(&self) -> Result<SnapshotStatusResponse, EsError> {
        self.exec(self.request(reqwest::Method::GET, "/_snapshot/_status"))
            .await
    }

    pub async fn snapshot_status(
        &self,
        repo: &str,
        snapshot: &str,
    ) -> Result<SnapshotStatusResponse, EsError> {
        let path = format!("/_snapshot/{}/{}/_status", repo, snapshot);
        self.exec(self.request(reqwest::Method::GET, &path)).await
    }

    pub async fn slm_policy(&self, policy: &str) -> Result<SlmPolicyResponse, EsError> {
        let path = format!("/_slm/policy/{}", policy);
        self.exec(self.request(reqwest::Method::GET, &path)).await
    }

    pub async fn tasks(&self, actions: Option<&str>) -> Result<TasksResponse, EsError> {
        let mut path = String::from("/_tasks");
        if let Some(a) = actions {
            path.push_str("?actions=");
            path.push_str(a);
        }
        self.exec(self.request(reqwest::Method::GET, &path)).await
    }

    pub async fn cat_indices(&self) -> Result<Vec<CatIndex>, EsError> {
        self.exec(self.request(reqwest::Method::GET, "/_cat/indices?format=json&bytes=b"))
            .await
    }

    pub async fn cluster_stats(&self) -> Result<ClusterStats, EsError> {
        self.exec(self.request(reqwest::Method::GET, "/_cluster/stats"))
            .await
    }

    pub async fn execute(
        &self,
        method: reqwest::Method,
        path: &str,
        body: Option<String>,
    ) -> Result<serde_json::Value, EsError> {
        let mut req = self.request(method, path);
        if let Some(b) = body {
            req = req.body(b);
        }
        self.exec(req).await
    }
}

// --- Response Models ---

#[derive(Debug, Clone, serde::Deserialize)]
pub struct ClusterHealth {
    pub cluster_name: String,
    pub status: String,
    #[serde(rename = "number_of_nodes")]
    pub number_of_nodes: u32,
    #[serde(rename = "number_of_data_nodes")]
    pub number_of_data_nodes: u32,
    #[serde(rename = "active_primary_shards")]
    pub active_primary_shards: u32,
    #[serde(rename = "active_shards")]
    pub active_shards: u32,
    #[serde(rename = "relocating_shards")]
    pub relocating_shards: u32,
    #[serde(rename = "initializing_shards")]
    pub initializing_shards: u32,
    #[serde(rename = "unassigned_shards")]
    pub unassigned_shards: u32,
    #[serde(rename = "timed_out")]
    pub timed_out: Option<bool>,
}

#[derive(Debug, Clone, serde::Deserialize, Default)]
pub struct SnapshotResponse {
    pub snapshots: Vec<SnapshotInfo>,
}

#[derive(Debug, Clone, serde::Deserialize)]
pub struct SnapshotInfo {
    pub snapshot: String,
    pub uuid: String,
    #[serde(rename = "repository")]
    pub repository: String,
    pub state: String,
    #[serde(rename = "start_time")]
    pub start_time: Option<String>,
    #[serde(rename = "start_time_in_millis")]
    pub start_time_in_millis: Option<i64>,
    #[serde(rename = "end_time")]
    pub end_time: Option<String>,
    #[serde(rename = "end_time_in_millis")]
    pub end_time_in_millis: Option<i64>,
    #[serde(rename = "duration_in_millis")]
    pub duration_in_millis: Option<i64>,
    pub indices: Option<Vec<String>>,
    #[serde(rename = "shards")]
    pub shards: Option<ShardStats>,
    #[serde(rename = "failures")]
    pub failures: Option<Vec<serde_json::Value>>,
}

#[derive(Debug, Clone, serde::Deserialize)]
pub struct ShardStats {
    pub total: u32,
    pub failed: u32,
    pub successful: u32,
}

#[derive(Debug, Clone, serde::Deserialize, Default)]
pub struct SnapshotStatusResponse {
    pub snapshots: Vec<SnapshotStatusInfo>,
}

#[derive(Debug, Clone, serde::Deserialize)]
pub struct SnapshotStatusInfo {
    pub snapshot: String,
    pub repository: String,
    pub uuid: String,
    pub state: String,
    #[serde(rename = "include_global_state")]
    pub include_global_state: bool,
    pub shards_stats: Option<ShardsStatsDetail>,
    pub stats: Option<SnapshotStatsDetail>,
    pub indices: Option<serde_json::Value>,
}

#[derive(Debug, Clone, serde::Deserialize)]
pub struct ShardsStatsDetail {
    pub initializing: u32,
    pub started: u32,
    pub finalizing: u32,
    pub done: u32,
    pub failed: u32,
    pub total: u32,
}

#[derive(Debug, Clone, serde::Deserialize)]
pub struct SnapshotStatsDetail {
    #[serde(rename = "number_of_files")]
    pub number_of_files: u32,
    #[serde(rename = "processed_files")]
    pub processed_files: u32,
    #[serde(rename = "total_size_in_bytes")]
    pub total_size_in_bytes: u64,
    #[serde(rename = "processed_size_in_bytes")]
    pub processed_size_in_bytes: u64,
    #[serde(rename = "number_of_chunks")]
    pub number_of_chunks: Option<u32>,
    #[serde(rename = "incremental")]
    pub incremental: Option<IncrementalStats>,
}

#[derive(Debug, Clone, serde::Deserialize)]
pub struct IncrementalStats {
    #[serde(rename = "file_count")]
    pub file_count: u32,
    #[serde(rename = "size_in_bytes")]
    pub size_in_bytes: u64,
}

#[derive(Debug, Clone, serde::Deserialize)]
pub struct SlmPolicyResponse {
    #[serde(flatten)]
    pub policies: std::collections::HashMap<String, SlmPolicyDetail>,
}

#[derive(Debug, Clone, serde::Deserialize)]
pub struct SlmPolicyDetail {
    pub version: Option<u32>,
    pub modified_date: Option<String>,
    pub modified_date_millis: Option<i64>,
    pub policy: Option<SlmPolicyConfig>,
    #[serde(rename = "last_success")]
    pub last_success: Option<SlmExecution>,
    #[serde(rename = "last_failure")]
    pub last_failure: Option<SlmExecution>,
    #[serde(rename = "next_execution")]
    pub next_execution: Option<String>,
    #[serde(rename = "next_execution_millis")]
    pub next_execution_millis: Option<i64>,
    pub stats: Option<SlmStats>,
}

#[derive(Debug, Clone, serde::Deserialize)]
pub struct SlmPolicyConfig {
    pub name: Option<String>,
    pub schedule: Option<String>,
    pub repository: Option<String>,
    pub config: Option<serde_json::Value>,
    pub retention: Option<serde_json::Value>,
}

#[derive(Debug, Clone, serde::Deserialize)]
pub struct SlmExecution {
    pub snapshot_name: Option<String>,
    pub time: Option<String>,
    #[serde(rename = "time_in_millis")]
    pub time_in_millis: Option<i64>,
    pub details: Option<String>,
}

#[derive(Debug, Clone, serde::Deserialize)]
pub struct SlmStats {
    #[serde(rename = "policy_stats")]
    pub policy_stats: Option<serde_json::Value>,
    pub retention_runs: Option<u64>,
    #[serde(rename = "retention_failed")]
    pub retention_failed: Option<u64>,
    #[serde(rename = "retention_timed_out")]
    pub retention_timed_out: Option<u64>,
    #[serde(rename = "retention_deletion_time")]
    pub retention_deletion_time: Option<String>,
    #[serde(rename = "retention_deletion_time_millis")]
    pub retention_deletion_time_millis: Option<i64>,
    pub total_snapshots_taken: Option<u64>,
    pub total_snapshots_failed: Option<u64>,
    pub total_snapshots_deleted: Option<u64>,
    pub total_snapshot_deletion_failures: Option<u64>,
}

#[derive(Debug, Clone, serde::Deserialize)]
pub struct TasksResponse {
    pub nodes: std::collections::HashMap<String, TaskNode>,
}

#[derive(Debug, Clone, serde::Deserialize)]
pub struct TaskNode {
    pub name: String,
    pub tasks: std::collections::HashMap<String, TaskInfo>,
}

#[derive(Debug, Clone, serde::Deserialize)]
pub struct TaskInfo {
    pub node: String,
    pub id: u64,
    #[serde(rename = "type")]
    pub task_type: String,
    pub action: String,
    pub description: Option<String>,
    pub start_time_in_millis: i64,
    pub running_time_in_nanos: u64,
    pub cancellable: bool,
    pub parent_task_id: Option<String>,
    pub headers: Option<serde_json::Value>,
    pub status: Option<serde_json::Value>,
}

#[derive(Debug, Clone, serde::Deserialize)]
pub struct CatIndex {
    pub index: String,
    #[serde(rename = "docs.count")]
    pub docs_count: Option<String>,
    #[serde(rename = "docs.deleted")]
    pub docs_deleted: Option<String>,
    #[serde(rename = "store.size")]
    pub store_size: Option<String>,
    #[serde(rename = "pri.store.size")]
    pub pri_store_size: Option<String>,
    pub health: Option<String>,
    pub status: Option<String>,
    pub pri: Option<String>,
    pub rep: Option<String>,
}

#[derive(Debug, Clone, serde::Deserialize, Default)]
pub struct ClusterStats {
    #[serde(rename = "cluster_name")]
    pub cluster_name: String,
    #[serde(rename = "cluster_uuid")]
    pub cluster_uuid: String,
    pub timestamp: i64,
    pub status: String,
    pub indices: Option<IndicesStats>,
    pub nodes: Option<NodesStats>,
}

#[derive(Debug, Clone, serde::Deserialize, Default)]
pub struct IndicesStats {
    pub count: u32,
    #[serde(rename = "docs")]
    pub docs: Option<DocStats>,
    #[serde(rename = "store")]
    pub store: Option<StoreStats>,
}

#[derive(Debug, Clone, serde::Deserialize)]
pub struct DocStats {
    pub count: u64,
    pub deleted: u64,
}

#[derive(Debug, Clone, serde::Deserialize)]
pub struct StoreStats {
    #[serde(rename = "size_in_bytes")]
    pub size_in_bytes: u64,
}

#[derive(Debug, Clone, serde::Deserialize, Default)]
pub struct NodesStats {
    pub count: Option<NodeCount>,
    #[serde(rename = "jvm")]
    pub jvm: Option<JvmStats>,
    #[serde(rename = "fs")]
    pub fs: Option<FsStats>,
}

#[derive(Debug, Clone, serde::Deserialize)]
pub struct NodeCount {
    pub total: u32,
    #[serde(rename = "data")]
    pub data: u32,
    #[serde(rename = "coordinating_only")]
    pub coordinating_only: u32,
    pub master: u32,
    pub ingest: u32,
}

#[derive(Debug, Clone, serde::Deserialize)]
pub struct JvmStats {
    #[serde(rename = "max_heap_in_bytes")]
    pub max_heap_in_bytes: u64,
    #[serde(rename = "used_heap_in_bytes")]
    pub used_heap_in_bytes: u64,
}

#[derive(Debug, Clone, serde::Deserialize)]
pub struct FsStats {
    #[serde(rename = "total_in_bytes")]
    pub total_in_bytes: u64,
    #[serde(rename = "free_in_bytes")]
    pub free_in_bytes: u64,
    #[serde(rename = "available_in_bytes")]
    pub available_in_bytes: u64,
}
