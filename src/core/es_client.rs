#![allow(dead_code)]

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
    pub(crate) password: String,
    tunnel_url: Option<String>,
}

impl EsClient {
    pub fn password(&self) -> &str {
        &self.password
    }
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

        if !config.verify_ssl || config.ssh_tunnel {
            builder = builder.danger_accept_invalid_certs(true);
        }

        builder.build().context("Failed to build HTTP client")
    }

    pub fn new(config: &ClusterConfig) -> Result<Self> {
        let password = match auth::get_password(&config.name) {
            Ok(Some(pw)) => {
                tracing::info!(
                    "[{}] Password loaded from keyring ({} chars)",
                    config.name,
                    pw.len()
                );
                pw
            }
            Ok(None) => {
                tracing::warn!(
                    "[{}] No password found in keyring — will authenticate as empty",
                    config.name
                );
                String::new()
            }
            Err(e) => {
                tracing::warn!(
                    "[{}] Failed to read password from keyring ({}): {}",
                    config.name,
                    e,
                    e
                );
                String::new()
            }
        };

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

    fn request(&self, method: reqwest::Method, path: &str) -> (RequestBuilder, reqwest::Method, String) {
        let host = self
            .tunnel_url
            .as_deref()
            .unwrap_or(self.config.host.trim());
        let host = if host.starts_with("http://") || host.starts_with("https://") {
            host.to_string()
        } else {
            format!("http://{}", host)
        };
        let host_trimmed = host.trim_end_matches('/');
        let url = if path.starts_with('/') {
            format!("{}{}", host_trimmed, path)
        } else {
            format!("{}/{}", host_trimmed, path)
        };
        let req = self.client
            .request(method.clone(), &url)
            .basic_auth(&self.config.username, Some(&self.password))
            .header("Content-Type", "application/json");
        (req, method, url)
    }

    async fn exec<T: DeserializeOwned>(
        &self,
        req: RequestBuilder,
        method: &reqwest::Method,
        url: &str,
    ) -> Result<T, EsError> {
        tracing::info!("[{}] {} {}", self.config.name, method, url);

        let start = std::time::Instant::now();
        let resp = req
            .send()
            .await
            .map_err(|e| {
                let elapsed = start.elapsed();
                tracing::error!(
                    "[{}] {} {} — FAILED after {}: {}",
                    self.config.name,
                    method,
                    url,
                    elapsed_millis(elapsed),
                    e
                );
                EsError::Unreachable(e.to_string())
            })?;

        let status = resp.status();
        let elapsed = start.elapsed();

        if !status.is_success() {
            let text = resp.text().await.unwrap_or_default();
            tracing::warn!(
                "[{}] {} {} — {} after {}",
                self.config.name,
                method,
                url,
                status,
                elapsed_millis(elapsed),
            );
            if !text.is_empty() {
                tracing::debug!(
                    "[{}] Response body: {}",
                    self.config.name,
                    truncate(&text, 500)
                );
            }
            return Err(EsError::Http {
                status,
                message: text,
            });
        }

        tracing::info!(
            "[{}] {} {} — {} after {}",
            self.config.name,
            method,
            url,
            status,
            elapsed_millis(elapsed),
        );

        resp.json().await.map_err(|e| {
            tracing::error!(
                "[{}] {} {} — Parse error after {}: {}",
                self.config.name,
                method,
                url,
                elapsed_millis(elapsed),
                e
            );
            EsError::Parse(e.to_string())
        })
    }

    async fn exec_raw(
        &self,
        req: RequestBuilder,
        method: &reqwest::Method,
        url: &str,
    ) -> Result<String, EsError> {
        tracing::info!("[{}] {} {}", self.config.name, method, url);

        let start = std::time::Instant::now();
        let resp = req
            .send()
            .await
            .map_err(|e| {
                let elapsed = start.elapsed();
                tracing::error!(
                    "[{}] {} {} — FAILED after {}: {}",
                    self.config.name,
                    method,
                    url,
                    elapsed_millis(elapsed),
                    e
                );
                EsError::Unreachable(e.to_string())
            })?;

        let status = resp.status();
        let elapsed = start.elapsed();

        if !status.is_success() {
            let text = resp.text().await.unwrap_or_default();
            tracing::warn!(
                "[{}] {} {} — {} after {}",
                self.config.name,
                method,
                url,
                status,
                elapsed_millis(elapsed),
            );
            if !text.is_empty() {
                tracing::debug!(
                    "[{}] Response body: {}",
                    self.config.name,
                    truncate(&text, 500)
                );
            }
            return Err(EsError::Http {
                status,
                message: text,
            });
        }

        tracing::info!(
            "[{}] {} {} — {} after {}",
            self.config.name,
            method,
            url,
            status,
            elapsed_millis(elapsed),
        );

        resp.text().await.map_err(|e| {
            tracing::error!(
                "[{}] {} {} — Read error after {}: {}",
                self.config.name,
                method,
                url,
                elapsed_millis(elapsed),
                e
            );
            EsError::Request(e.to_string())
        })
    }

    pub async fn cluster_health(&self) -> Result<ClusterHealth, EsError> {
        let (req, method, url) = self.request(reqwest::Method::GET, "/_cluster/health");
        self.exec(req, &method, &url).await
    }

    pub async fn allocation_explain(&self) -> Result<AllocationExplain, EsError> {
        let (req, method, url) = self.request(reqwest::Method::GET, "/_cluster/allocation/explain");
        let raw: serde_json::Value = self.exec(req, &method, &url).await?;
        Ok(parse_allocation_explain(&raw))
    }

    pub async fn snapshot_current(&self, repo: &str) -> Result<SnapshotResponse, EsError> {
        let path = format!("/_snapshot/{}/_current", repo);
        let (req, method, url) = self.request(reqwest::Method::GET, &path);
        self.exec(req, &method, &url).await
    }

    pub async fn snapshot_status_all(&self) -> Result<SnapshotStatusResponse, EsError> {
        let (req, method, url) = self.request(reqwest::Method::GET, "/_snapshot/_status");
        self.exec(req, &method, &url).await
    }

    pub async fn snapshot_status(
        &self,
        repo: &str,
        snapshot: &str,
    ) -> Result<SnapshotStatusResponse, EsError> {
        let path = format!("/_snapshot/{}/{}/_status", repo, snapshot);
        let (req, method, url) = self.request(reqwest::Method::GET, &path);
        self.exec(req, &method, &url).await
    }

    pub async fn slm_policy(&self, policy: &str) -> Result<SlmPolicyResponse, EsError> {
        let path = format!("/_slm/policy/{}", policy);
        let (req, method, url) = self.request(reqwest::Method::GET, &path);
        self.exec(req, &method, &url).await
    }

    pub async fn slm_policies_all(&self) -> Result<SlmPolicyResponse, EsError> {
        let (req, method, url) = self.request(reqwest::Method::GET, "/_slm/policy");
        self.exec(req, &method, &url).await
    }

    pub async fn snapshot_all(&self, repo: &str) -> Result<SnapshotResponse, EsError> {
        let path = format!("/_snapshot/{}/_all", repo);
        let (req, method, url) = self.request(reqwest::Method::GET, &path);
        self.exec(req, &method, &url).await
    }

    pub async fn tasks(&self, actions: Option<&str>) -> Result<TasksResponse, EsError> {
        let mut path = String::from("/_tasks");
        if let Some(a) = actions {
            path.push_str("?actions=");
            path.push_str(a);
        }
        let (req, method, url) = self.request(reqwest::Method::GET, &path);
        self.exec(req, &method, &url).await
    }

    pub async fn cat_indices(&self) -> Result<Vec<CatIndex>, EsError> {
        let (req, method, url) =
            self.request(reqwest::Method::GET, "/_cat/indices?format=json&bytes=b");
        self.exec(req, &method, &url).await
    }

    pub async fn get_data_streams(&self) -> Result<DataStreamResponse, EsError> {
        let (req, method, url) = self.request(reqwest::Method::GET, "/_data_stream");
        self.exec(req, &method, &url).await
    }

    pub async fn cat_allocation(&self) -> Result<Vec<CatAllocation>, EsError> {
        let (req, method, url) = self.request(reqwest::Method::GET, "/_cat/allocation?format=json");
        self.exec(req, &method, &url).await
    }

    pub async fn get_es_version(&self) -> Result<String, EsError> {
        let res: serde_json::Value = self.execute(reqwest::Method::GET, "/", None).await?;
        if let Some(v) = res.get("version").and_then(|v| v.get("number")).and_then(|n| n.as_str()) {
            Ok(v.to_string())
        } else {
            Err(EsError::Parse("Unable to parse ES version".to_string()))
        }
    }

    pub async fn get_kibana_version(&self, kibana_host: &str) -> Result<String, EsError> {
        let res = self.send_to_host(kibana_host, reqwest::Method::GET, "/api/status", None).await?;
        if let Some(version) = res.get("version").and_then(|v| v.get("number")).and_then(|n| n.as_str()) {
            Ok(version.to_string())
        } else if let Some(version) = res.get("version").and_then(|v| v.as_str()) {
            Ok(version.to_string())
        } else {
            Err(EsError::Parse("Unable to parse Kibana version".to_string()))
        }
    }

    pub async fn get_kibana_synthetics_monitors(&self, kibana_host: &str, space_id: Option<&str>) -> Result<serde_json::Value, EsError> {
        let path = match space_id {
            Some(space) if space != "default" && !space.is_empty() => {
                format!("/s/{}/api/synthetics/monitors", space)
            }
            _ => "/api/synthetics/monitors".to_string(),
        };
        self.send_to_host(kibana_host, reqwest::Method::GET, &path, None).await
    }

    pub async fn get_node_hot_threads(&self, node_name: &str) -> Result<String, EsError> {
        let path = format!("/_nodes/{}/hot_threads", node_name);
        self.execute_raw(reqwest::Method::GET, &path, None).await
    }

    pub async fn get_pending_tasks(&self) -> Result<serde_json::Value, EsError> {
        self.execute(reqwest::Method::GET, "/_cluster/pending_tasks", None).await
    }

    pub async fn cluster_stats(&self) -> Result<ClusterStats, EsError> {
        let (req, method, url) = self.request(reqwest::Method::GET, "/_cluster/stats");
        self.exec(req, &method, &url).await
    }

    pub async fn execute(
        &self,
        method: reqwest::Method,
        path: &str,
        body: Option<String>,
    ) -> Result<serde_json::Value, EsError> {
        let (mut req, m, url) = self.request(method, path);
        if let Some(b) = body {
            req = req.body(b);
        }
        self.exec(req, &m, &url).await
    }

    pub async fn send_to_host(
        &self,
        host: &str,
        method: reqwest::Method,
        path: &str,
        body: Option<String>,
    ) -> Result<serde_json::Value, EsError> {
        let base = host.trim_end_matches('/');
        let url = if path.starts_with('/') {
            format!("{}{}", base, path)
        } else {
            format!("{}/{}", base, path)
        };
        let mut req = self
            .client
            .request(method.clone(), &url)
            .basic_auth(&self.config.username, Some(&self.password))
            .header("kbn-xsrf", "true")
            .header("Content-Type", "application/json");

        if let Some(b) = body {
            req = req.body(b);
        }

        self.exec(req, &method, &url).await
    }

    pub async fn execute_raw(
        &self,
        method: reqwest::Method,
        path: &str,
        body: Option<String>,
    ) -> Result<String, EsError> {
        let (mut req, m, url) = self.request(method, path);
        if let Some(b) = body {
            req = req.body(b);
        }
        self.exec_raw(req, &m, &url).await
    }

    pub async fn send_to_host_raw(
        &self,
        host: &str,
        method: reqwest::Method,
        path: &str,
        body: Option<String>,
    ) -> Result<String, EsError> {
        let base = host.trim_end_matches('/');
        let url = if path.starts_with('/') {
            format!("{}{}", base, path)
        } else {
            format!("{}/{}", base, path)
        };
        let mut req = self
            .client
            .request(method.clone(), &url)
            .basic_auth(&self.config.username, Some(&self.password))
            .header("kbn-xsrf", "true")
            .header("Content-Type", "application/json");

        if let Some(b) = body {
            req = req.body(b);
        }

        self.exec_raw(req, &method, &url).await
    }
}

// --- Response Models ---

#[derive(Debug, Clone, serde::Deserialize, serde::Serialize, Default)]
pub struct AllocationExplain {
    pub index: String,
    pub shard: u32,
    pub primary: bool,
    pub current_state: String,
    pub reason: Option<String>,
    pub details: Option<String>,
    pub explanation: Option<String>,
    pub decider_reasons: Vec<String>,
}

#[derive(Debug, Clone, serde::Deserialize, serde::Serialize)]

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

#[derive(Debug, Clone, serde::Deserialize, serde::Serialize, Default)]

pub struct SnapshotResponse {
    pub snapshots: Vec<SnapshotInfo>,
}

#[derive(Debug, Clone, serde::Deserialize, serde::Serialize)]

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

#[derive(Debug, Clone, serde::Deserialize, serde::Serialize)]

pub struct ShardStats {
    pub total: u32,
    pub failed: u32,
    pub successful: u32,
}

#[derive(Debug, Clone, serde::Deserialize, serde::Serialize, Default)]

pub struct SnapshotStatusResponse {
    pub snapshots: Vec<SnapshotStatusInfo>,
}

#[derive(Debug, Clone, serde::Deserialize, serde::Serialize)]

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

#[derive(Debug, Clone, serde::Deserialize, serde::Serialize)]

pub struct ShardsStatsDetail {
    pub initializing: u32,
    pub started: u32,
    pub finalizing: u32,
    pub done: u32,
    pub failed: u32,
    pub total: u32,
}

#[derive(Debug, Clone, serde::Deserialize, serde::Serialize)]

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

#[derive(Debug, Clone, serde::Deserialize, serde::Serialize)]

pub struct IncrementalStats {
    #[serde(rename = "file_count")]
    pub file_count: u32,
    #[serde(rename = "size_in_bytes")]
    pub size_in_bytes: u64,
}

#[derive(Debug, Clone, serde::Deserialize, serde::Serialize)]

pub struct SlmPolicyResponse {
    #[serde(flatten)]
    pub policies: std::collections::HashMap<String, SlmPolicyDetail>,
}

#[derive(Debug, Clone, serde::Deserialize, serde::Serialize)]

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

#[derive(Debug, Clone, serde::Deserialize, serde::Serialize)]

pub struct SlmPolicyConfig {
    pub name: Option<String>,
    pub schedule: Option<String>,
    pub repository: Option<String>,
    pub config: Option<serde_json::Value>,
    pub retention: Option<serde_json::Value>,
}

#[derive(Debug, Clone, serde::Deserialize, serde::Serialize)]

pub struct SlmExecution {
    pub snapshot_name: Option<String>,
    pub time: Option<String>,
    #[serde(rename = "time_in_millis")]
    pub time_in_millis: Option<i64>,
    pub details: Option<String>,
}

#[derive(Debug, Clone, serde::Deserialize, serde::Serialize)]

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

#[derive(Debug, Clone, serde::Deserialize, serde::Serialize)]

pub struct TasksResponse {
    pub nodes: std::collections::HashMap<String, TaskNode>,
}

#[derive(Debug, Clone, serde::Deserialize, serde::Serialize)]

pub struct TaskNode {
    pub name: String,
    pub tasks: std::collections::HashMap<String, TaskInfo>,
}

#[derive(Debug, Clone, serde::Deserialize, serde::Serialize)]

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

#[derive(Debug, Clone, serde::Deserialize, serde::Serialize, Default)]
pub struct CatAllocation {
    pub shards: Option<String>,
    #[serde(rename = "disk.percent")]
    pub disk_percent: Option<String>,
    #[serde(rename = "disk.avail")]
    pub disk_avail: Option<String>,
    #[serde(rename = "disk.total")]
    pub disk_total: Option<String>,
    pub node: Option<String>,
}

#[derive(Debug, Clone, serde::Deserialize, serde::Serialize)]

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

#[derive(Debug, Clone, serde::Deserialize, serde::Serialize)]
pub struct ClusterStats {
    #[serde(rename = "cluster_name")]
    pub cluster_name: String,
    #[serde(rename = "cluster_uuid", default)]
    pub cluster_uuid: String,
    #[serde(default)]
    pub timestamp: i64,
    #[serde(default)]
    pub status: String,
    #[serde(default)]
    pub indices: Option<IndicesStats>,
    #[serde(default)]
    pub nodes: Option<NodesStats>,
}

#[derive(Debug, Clone, serde::Deserialize, serde::Serialize, Default)]
pub struct IndicesStats {
    #[serde(default)]
    pub count: u32,
    #[serde(default)]
    pub docs: Option<DocStats>,
    #[serde(default)]
    pub store: Option<StoreStats>,
}

#[derive(Debug, Clone, serde::Deserialize, serde::Serialize, Default)]
pub struct DocStats {
    #[serde(default)]
    pub count: u64,
    #[serde(default)]
    pub deleted: u64,
}

#[derive(Debug, Clone, serde::Deserialize, serde::Serialize, Default)]
pub struct StoreStats {
    #[serde(rename = "size_in_bytes", default)]
    pub size_in_bytes: u64,
}

#[derive(Debug, Clone, serde::Deserialize, serde::Serialize, Default)]
pub struct NodesStats {
    #[serde(default)]
    pub count: Option<NodeCount>,
    #[serde(default)]
    pub jvm: Option<JvmStats>,
    #[serde(default)]
    pub fs: Option<FsStats>,
}

#[derive(Debug, Clone, serde::Deserialize, serde::Serialize, Default)]
pub struct NodeCount {
    #[serde(default)]
    pub total: u32,
    #[serde(default)]
    pub data: u32,
    #[serde(rename = "data_cold", default)]
    pub data_cold: u32,
    #[serde(rename = "data_content", default)]
    pub data_content: u32,
    #[serde(rename = "data_frozen", default)]
    pub data_frozen: u32,
    #[serde(rename = "data_hot", default)]
    pub data_hot: u32,
    #[serde(rename = "data_warm", default)]
    pub data_warm: u32,
    #[serde(rename = "coordinating_only", default)]
    pub coordinating_only: u32,
    #[serde(default)]
    pub master: u32,
    #[serde(default)]
    pub ingest: u32,
    #[serde(default)]
    pub ml: u32,
    #[serde(rename = "remote_cluster_client", default)]
    pub remote_cluster_client: u32,
    #[serde(default)]
    pub transform: u32,
    #[serde(rename = "voting_only", default)]
    pub voting_only: u32,
}

#[derive(Debug, Clone, serde::Deserialize, serde::Serialize, Default)]
pub struct JvmStats {
    #[serde(default)]
    pub mem: Option<JvmMem>,
}

#[derive(Debug, Clone, serde::Deserialize, serde::Serialize, Default)]
pub struct JvmMem {
    #[serde(rename = "heap_max_in_bytes", default)]
    pub heap_max_in_bytes: u64,
    #[serde(rename = "heap_used_in_bytes", default)]
    pub heap_used_in_bytes: u64,
}

#[derive(Debug, Clone, serde::Deserialize, serde::Serialize, Default)]
pub struct FsStats {
    #[serde(rename = "total_in_bytes", default)]
    pub total_in_bytes: u64,
    #[serde(rename = "free_in_bytes", default)]
    pub free_in_bytes: u64,
    #[serde(rename = "available_in_bytes", default)]
    pub available_in_bytes: u64,
}

fn elapsed_millis(dur: std::time::Duration) -> String {
    format!("{}ms", dur.as_millis())
}

fn truncate(s: &str, max_len: usize) -> String {
    if s.len() <= max_len {
        s.to_string()
    } else {
        format!("{}...", &s[..max_len])
    }
}

impl EsClient {
    pub async fn test_connection_detailed(&self) -> Result<String, String> {
        use std::process::Stdio;
        use tokio::io::AsyncBufReadExt;

        if !self.config.ssh_tunnel || self.config.ssh_host.is_empty() {
            // Direct connection
            match self.cluster_health().await {
                Ok(h) => Ok(format!(
                    "Connected successfully!\n\n• Cluster: {}\n• Status: {}\n• Nodes: {}\n• Active Shards: {}",
                    h.cluster_name, h.status, h.number_of_nodes, h.active_shards
                )),
                Err(e) => {
                    Err(format!(
                        "HTTP Request Failed!\n\nError: {}\n\nPossible causes:\n1. Elasticsearch is not running at {}\n2. Incorrect username or password\n3. Network/firewall blocking connection\n4. TLS/SSL verification failed (override available in cluster settings)",
                        e, self.config.host
                    ))
                }
            }
        } else {
            // SSH tunnel connection
            let local_port = match Self::find_free_port() {
                Ok(p) => p,
                Err(e) => return Err(format!("Failed to find free local port: {}", e)),
            };

            let es_host = Self::parse_host(&self.config.host);
            let es_port = Self::parse_port(&self.config.host).unwrap_or(9200);

            let ssh_target = if self.config.ssh_user.is_empty() {
                self.config.ssh_host.clone()
            } else {
                format!("{}@{}", self.config.ssh_user, self.config.ssh_host)
            };

            let mut cmd = tokio::process::Command::new("ssh");
            cmd.arg("-N")
                .arg("-v") // Verbose mode is essential for log output!
                .arg("-o").arg("ServerAliveInterval=30")
                .arg("-o").arg("ServerAliveCountMax=2")
                .arg("-o").arg("ExitOnForwardFailure=yes")
                .arg("-L").arg(format!("127.0.0.1:{}:{}:{}", local_port, es_host, es_port))
                .arg("-p").arg(self.config.ssh_port.to_string())
                .arg(&ssh_target)
                .stdin(Stdio::null())
                .stdout(Stdio::null())
                .stderr(Stdio::piped());

            let mut child = match cmd.spawn() {
                Ok(c) => c,
                Err(e) => return Err(format!("Failed to spawn SSH process: {}\nIs 'ssh' installed and in PATH?", e)),
            };

            let stderr = child.stderr.take().unwrap();
            let mut reader = tokio::io::BufReader::new(stderr).lines();
            
            // Collect SSH logs in the background while waiting for connection or timeout
            let log_lines = std::sync::Arc::new(tokio::sync::Mutex::new(Vec::<String>::new()));
            let log_lines_clone = log_lines.clone();
            
            let mut ssh_finished = false;
            let mut child_exit_status = None;

            // Spawn a task to read stderr logs
            let log_reader_handle = tokio::spawn(async move {
                while let Ok(Some(line)) = reader.next_line().await {
                    let mut logs = log_lines_clone.lock().await;
                    // Keep the last 100 lines
                    if logs.len() > 100 {
                        logs.remove(0);
                    }
                    logs.push(line);
                }
            });

            // Wait for a few seconds to let SSH establish connection
            let mut tunnel_ready = false;
            for _ in 0..15 { // Max 3 seconds (15 * 200ms)
                tokio::time::sleep(Duration::from_millis(200)).await;
                
                // Check if child exited early
                if let Ok(Some(status)) = child.try_wait() {
                    ssh_finished = true;
                    child_exit_status = Some(status);
                    break;
                }
                
                // Read current logs to see if connection is established
                let logs = log_lines.lock().await;
                let established = logs.iter().any(|line| {
                    line.contains("Entering interactive session") ||
                    line.contains("Authentication succeeded") ||
                    line.contains("Local connections to") ||
                    line.contains("Local forwarding listening")
                });
                
                if established {
                    tunnel_ready = true;
                    break;
                }
            }

            // If not found in logs but process is still alive, we can also assume it's running
            if !ssh_finished && !tunnel_ready {
                tunnel_ready = true;
            }

            let result = if tunnel_ready {
                // Build a temporary test client pointing to the tunnel's local port
                let mut test_config = self.config.clone();
                let protocol = if self.config.host.starts_with("https://") {
                    "https"
                } else {
                    "http"
                };
                test_config.host = format!("{}://127.0.0.1:{}", protocol, local_port);
                
                if let Ok(test_client) = Self::with_password(&test_config, &self.password) {
                    // Execute HTTP request
                    match test_client.cluster_health().await {
                        Ok(h) => {
                            let ssh_logs = log_lines.lock().await.join("\n");
                            Ok(format!(
                                "Connected successfully through SSH tunnel!\n\n• Cluster: {}\n• Status: {}\n• Nodes: {}\n• Active Shards: {}\n\n=== SSH Connection Logs ===\n{}",
                                h.cluster_name, h.status, h.number_of_nodes, h.active_shards, ssh_logs
                            ))
                        }
                        Err(e) => {
                            let ssh_logs = log_lines.lock().await.join("\n");
                            Err(format!(
                                "SSH Tunnel established, but Elasticsearch HTTP request failed!\n\nHTTP Error: {}\n\nPossible causes:\n1. Elasticsearch is not running on target host at {}\n2. Incorrect Elasticsearch credentials\n\n=== SSH Connection Logs ===\n{}",
                                e, self.config.host, ssh_logs
                            ))
                        }
                    }
                } else {
                    Err("Failed to build internal test client".to_string())
                }
            } else {
                let ssh_logs = log_lines.lock().await.join("\n");
                let exit_desc = match child_exit_status {
                    Some(status) => format!("SSH exited with status: {}", status),
                    None => "SSH connection timed out".to_string(),
                };
                Err(format!(
                    "SSH Connection Failed!\n\nReason: {}\n\n=== SSH Connection Logs ===\n{}",
                    exit_desc, ssh_logs
                ))
            };

            // Clean up: abort reader and kill child process
            log_reader_handle.abort();
            let _ = child.kill().await;

            result
        }
    }

    fn find_free_port() -> Result<u16> {
        let listener = std::net::TcpListener::bind("127.0.0.1:0")?;
        let port = listener.local_addr()?.port();
        drop(listener);
        Ok(port)
    }

    fn parse_host(host: &str) -> String {
        let host = host.trim();
        let host = host
            .strip_prefix("http://")
            .or_else(|| host.strip_prefix("https://"))
            .unwrap_or(host);
        host.split(':').next().unwrap_or(host).to_string()
    }

    fn parse_port(host: &str) -> Option<u16> {
        let host = host.trim();
        let host = host
            .strip_prefix("http://")
            .or_else(|| host.strip_prefix("https://"))
            .unwrap_or(host);
        host.split(':').nth(1)?.parse().ok()
    }
}

#[derive(Debug, Clone, serde::Deserialize, serde::Serialize)]
pub struct DataStreamResponse {
    pub data_streams: Vec<DataStream>,
}

#[derive(Debug, Clone, serde::Deserialize, serde::Serialize)]
pub struct DataStream {
    pub name: String,
    pub timestamp_field: TimestampField,
    pub indices: Vec<DataStreamBackingIndex>,
    pub generation: i64,
    pub status: String,
    pub template: Option<String>,
    pub ilm_policy: Option<String>,
    #[serde(rename = "store_size_bytes", default)]
    pub store_size_bytes: Option<i64>,
}

#[derive(Debug, Clone, serde::Deserialize, serde::Serialize)]
pub struct TimestampField {
    pub name: String,
}

#[derive(Debug, Clone, serde::Deserialize, serde::Serialize)]
pub struct DataStreamBackingIndex {
    pub index_name: String,
    pub index_uuid: String,
}

pub fn parse_allocation_explain(raw: &serde_json::Value) -> AllocationExplain {
    let mut explain = AllocationExplain {
        index: raw["index"].as_str().unwrap_or("unknown").to_string(),
        shard: raw["shard"].as_u64().unwrap_or(0) as u32,
        primary: raw["primary"].as_bool().unwrap_or(false),
        current_state: raw["current_state"].as_str().unwrap_or("unknown").to_string(),
        ..Default::default()
    };
    
    if let Some(ui) = raw.get("unassigned_info") {
        explain.reason = ui.get("reason").and_then(|r| r.as_str()).map(|s| s.to_string());
        explain.details = ui.get("details").and_then(|d| d.as_str()).map(|s| s.to_string());
    }
    
    explain.explanation = raw.get("allocate_explanation")
        .or_else(|| raw.get("explanation"))
        .and_then(|e| e.as_str())
        .map(|s| s.to_string());

    if let Some(decisions) = raw.get("node_decisions").and_then(|d| d.as_array()) {
        for node in decisions {
            let node_name = node["node_name"].as_str().unwrap_or("unknown");
            if let Some(decs) = node["decisions"].as_array() {
                for dec in decs {
                    if dec["decision"].as_str() == Some("NO") {
                        if let Some(exp) = dec["explanation"].as_str() {
                            explain.decider_reasons.push(format!("{}: {}", node_name, exp));
                        }
                    }
                }
            }
        }
    }
    
    explain
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn test_parse_allocation_explain() {
        let raw = json!({
            "index": "logs-app-prod",
            "shard": 2,
            "primary": true,
            "current_state": "unassigned",
            "unassigned_info": {
                "reason": "INDEX_CREATED",
                "details": "Waiting for active shards"
            },
            "allocate_explanation": "Elasticsearch cannot allocate this primary shard because the disk is full.",
            "node_decisions": [
                {
                    "node_name": "node-us-east-1",
                    "decisions": [
                        {
                            "decision": "NO",
                            "explanation": "high watermark reached on this node"
                        }
                    ]
                }
            ]
        });

        let parsed = parse_allocation_explain(&raw);
        assert_eq!(parsed.index, "logs-app-prod");
        assert_eq!(parsed.shard, 2);
        assert!(parsed.primary);
        assert_eq!(parsed.current_state, "unassigned");
        assert_eq!(parsed.reason.as_deref(), Some("INDEX_CREATED"));
        assert_eq!(parsed.details.as_deref(), Some("Waiting for active shards"));
        assert_eq!(parsed.explanation.as_deref(), Some("Elasticsearch cannot allocate this primary shard because the disk is full."));
        assert_eq!(parsed.decider_reasons, vec!["node-us-east-1: high watermark reached on this node".to_string()]);
    }
}
