#![allow(dead_code)]

use std::sync::Arc;
use std::time::{Duration, Instant};

use anyhow::{Context, Result};
use reqwest::{Client, ClientBuilder, RequestBuilder, StatusCode};
use serde::de::DeserializeOwned;
use serde_json::json;

use crate::core::auth;
use crate::core::config::{AuthMethod, CaCert, ClusterConfig};
use crate::core::ssh_tunnel::{self, SshTunnel, TunnelError, TunnelTarget};

/// After a tunnel fails to open, requests fail fast with that error for this long instead of
/// each starting ssh again.
const TUNNEL_RETRY_COOLDOWN: Duration = Duration::from_secs(10);

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

/// Accepts either the base64 `encoded` API key or a raw `id:api_key` pair
/// and returns the base64 credential for the `ApiKey` Authorization scheme.
pub fn api_key_credentials(key: &str) -> String {
    use base64::Engine;
    let key = key.trim();
    if key.contains(':') {
        base64::engine::general_purpose::STANDARD.encode(key)
    } else {
        key.to_string()
    }
}

#[derive(Clone)]
pub struct EsClient {
    config: ClusterConfig,
    client: Client,
    pub(crate) password: String,
    /// Set for SSH-tunnelled clusters. Shared by all clones, so one ssh process serves them.
    tunnel: Option<Arc<TunnelSlot>>,
}

impl std::fmt::Debug for EsClient {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("EsClient")
            .field("config", &self.config)
            .field("tunnel", &self.tunnel)
            .finish_non_exhaustive()
    }
}

/// An SSH-tunnelled cluster's tunnel: opened on first use, reopened when ssh exits.
struct TunnelSlot {
    target: TunnelTarget,
    state: tokio::sync::Mutex<TunnelState>,
}

#[derive(Default)]
struct TunnelState {
    active: Option<ActiveTunnel>,
    last_failure: Option<(Instant, TunnelError)>,
}

struct ActiveTunnel {
    tunnel: SshTunnel,
    /// Built for this tunnel: resolves the target's domain to the local end of the forward.
    client: Client,
}

impl std::fmt::Debug for TunnelSlot {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("TunnelSlot")
            .field("target", &self.target)
            .finish_non_exhaustive()
    }
}

impl TunnelSlot {
    fn for_config(config: &ClusterConfig) -> Option<Arc<Self>> {
        if !config.ssh_tunnel || config.ssh_host.trim().is_empty() {
            return None;
        }
        match TunnelTarget::from_host(&config.host) {
            Ok(target) => Some(Arc::new(Self {
                target,
                state: tokio::sync::Mutex::new(TunnelState::default()),
            })),
            Err(e) => {
                tracing::warn!("[{}] SSH tunnel disabled: {}", config.name, e);
                None
            }
        }
    }

    /// Open (alive, or being opened), so a rebuilt client can keep using it.
    fn is_usable(&self) -> bool {
        match self.state.try_lock() {
            Ok(state) => state.active.as_ref().is_some_and(|a| a.tunnel.is_alive()),
            Err(_) => true,
        }
    }

    /// The tunnel's HTTP client and local port, opening the tunnel if needed. `fresh` closes
    /// any open tunnel and skips the failure cooldown (used by the connection test).
    async fn connect(
        &self,
        config: &ClusterConfig,
        fresh: bool,
    ) -> Result<(Client, u16), TunnelError> {
        let mut state = self.state.lock().await;
        if fresh {
            state.active = None;
            state.last_failure = None;
        }
        if let Some(active) = &state.active {
            if active.tunnel.is_alive() {
                return Ok((active.client.clone(), active.tunnel.local_port));
            }
            tracing::warn!(
                "[{}] SSH tunnel closed ({}); reconnecting",
                config.name,
                active.tunnel.recent_errors().join(" | ")
            );
            state.active = None;
        }
        if let Some((at, err)) = &state.last_failure
            && at.elapsed() < TUNNEL_RETRY_COOLDOWN
        {
            return Err(err.clone());
        }

        let result = match SshTunnel::open(config, &self.target, fresh).await {
            Ok(tunnel) => {
                match EsClient::tunnel_http_client(config, &self.target, tunnel.local_port) {
                    Ok(client) => Ok(ActiveTunnel { tunnel, client }),
                    Err(e) => Err(TunnelError {
                        summary: format!("Failed to build HTTP client for the tunnel: {:#}", e),
                        command: tunnel.command().to_string(),
                        logs: tunnel.logs(),
                    }),
                }
            }
            Err(e) => Err(e),
        };
        match result {
            Ok(active) => {
                let out = (active.client.clone(), active.tunnel.local_port);
                state.active = Some(active);
                state.last_failure = None;
                Ok(out)
            }
            Err(err) => {
                tracing::warn!("[{}] SSH tunnel failed: {}", config.name, err.summary);
                state.last_failure = Some((Instant::now(), err.clone()));
                Err(err)
            }
        }
    }

    /// ssh's recent stderr and full log, for error messages.
    async fn logs(&self) -> (Vec<String>, Vec<String>) {
        let state = self.state.lock().await;
        match &state.active {
            Some(a) => (a.tunnel.recent_errors(), a.tunnel.logs()),
            None => (Vec::new(), Vec::new()),
        }
    }
}

/// An error with its causes: reqwest's own message ("error sending request for url (…)")
/// leaves out whether the connection was refused, timed out, or failed TLS.
fn error_chain(e: &(dyn std::error::Error + 'static)) -> String {
    let mut out = e.to_string();
    let mut source = e.source();
    while let Some(cause) = source {
        let msg = cause.to_string();
        if !out.contains(&msg) {
            out.push_str(": ");
            out.push_str(&msg);
        }
        source = cause.source();
    }
    out
}

impl EsClient {
    pub fn password(&self) -> &str {
        &self.password
    }

    /// Attach credentials per the cluster's auth method (Basic or API key).
    fn apply_auth(&self, req: RequestBuilder) -> RequestBuilder {
        match self.config.auth_method {
            AuthMethod::Basic => req.basic_auth(&self.config.username, Some(&self.password)),
            AuthMethod::ApiKey => {
                let key = self.password.trim();
                let key = key.strip_prefix("ApiKey ").unwrap_or(key).trim();
                if reqwest::header::HeaderValue::from_str(key).is_err() {
                    return req.header(reqwest::header::AUTHORIZATION, key);
                }
                let value = format!("ApiKey {}", api_key_credentials(key));
                match reqwest::header::HeaderValue::from_str(&value) {
                    Ok(mut value) => {
                        value.set_sensitive(true);
                        req.header(reqwest::header::AUTHORIZATION, value)
                    }
                    Err(_) => req.header(reqwest::header::AUTHORIZATION, value),
                }
            }
        }
    }
    fn build_client(config: &ClusterConfig) -> Result<Client> {
        Self::client_builder(config)?
            .build()
            .context("Failed to build HTTP client")
    }

    /// The HTTP client for requests through a tunnel. A domain target keeps its name in the
    /// URL (so SNI and certificate checks work as on a direct connection) and resolves to the
    /// local end of the forward. An IP target is rewritten to 127.0.0.1, which its
    /// certificate cannot match, so certificate checks are skipped for it.
    fn tunnel_http_client(
        config: &ClusterConfig,
        target: &TunnelTarget,
        local_port: u16,
    ) -> Result<Client> {
        let mut builder = Self::client_builder(config)?.no_proxy();
        if target.is_ip {
            builder = builder.danger_accept_invalid_certs(true);
        } else {
            builder = builder.resolve(&target.host, TunnelTarget::local_addr(local_port));
        }
        builder.build().context("Failed to build HTTP client")
    }

    fn client_builder(config: &ClusterConfig) -> Result<ClientBuilder> {
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

        if !config.ca_cert_pem.is_empty() {
            let cert = reqwest::Certificate::from_pem(config.ca_cert_pem.as_bytes())
                .context("Invalid pasted PEM CA certificate")?;
            builder = builder.add_root_certificate(cert);
        }

        if !config.verify_ssl {
            builder = builder.danger_accept_invalid_certs(true);
        }

        Ok(builder)
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
            tunnel: TunnelSlot::for_config(config),
        })
    }

    pub fn with_password(config: &ClusterConfig, password: impl Into<String>) -> Result<Self> {
        let client = Self::build_client(config)?;
        Ok(Self {
            config: config.clone(),
            client,
            password: password.into(),
            tunnel: TunnelSlot::for_config(config),
        })
    }

    /// Keeps `previous`'s open SSH tunnel when this client would open an identical one, so
    /// rebuilding a client (new password, edited non-SSH settings) doesn't reconnect ssh.
    pub fn adopt_tunnel(mut self, previous: &EsClient) -> Self {
        if let (Some(_), Some(prev)) = (&self.tunnel, &previous.tunnel)
            && Self::same_tunnel(&self.config, &previous.config)
            && prev.is_usable()
        {
            self.tunnel = Some(prev.clone());
        }
        self
    }

    fn same_tunnel(a: &ClusterConfig, b: &ClusterConfig) -> bool {
        a.host.trim() == b.host.trim()
            && a.ssh_host.trim() == b.ssh_host.trim()
            && a.ssh_user.trim() == b.ssh_user.trim()
            && a.ssh_port == b.ssh_port
            // The tunnel's HTTP client carries the TLS settings.
            && a.verify_ssl == b.verify_ssl
            && a.ca_cert == b.ca_cert
            && a.ca_cert_pem == b.ca_cert_pem
    }

    /// Sends a request, through the SSH tunnel if it is for a tunnelled cluster's own host.
    async fn dispatch(&self, req: RequestBuilder) -> Result<reqwest::Response, EsError> {
        let unreachable = |e: reqwest::Error| EsError::Unreachable(error_chain(&e));
        let Some(slot) = &self.tunnel else {
            return req.send().await.map_err(unreachable);
        };
        let request = req.build().map_err(|e| EsError::Request(error_chain(&e)))?;
        if !slot.target.matches(request.url()) {
            return self.client.execute(request).await.map_err(unreachable);
        }
        let (client, local_port) = slot
            .connect(&self.config, false)
            .await
            .map_err(|e| EsError::Unreachable(format!("SSH tunnel failed: {}", e.summary)))?;
        let request = slot.target.redirect(request, local_port);
        match client.execute(request).await {
            Ok(resp) => Ok(resp),
            Err(e) => {
                let (recent, _) = slot.logs().await;
                let mut msg = format!(
                    "{} (via SSH tunnel {})",
                    error_chain(&e),
                    ssh_tunnel::describe(&self.config, &slot.target, local_port)
                );
                if !recent.is_empty() {
                    msg.push_str(&format!("; ssh: {}", recent.join(" | ")));
                }
                Err(EsError::Unreachable(msg))
            }
        }
    }

    fn request(
        &self,
        method: reqwest::Method,
        path: &str,
    ) -> (RequestBuilder, reqwest::Method, String) {
        let host = self.config.host.trim();
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
        let req = self
            .apply_auth(self.client.request(method.clone(), &url))
            .header("Content-Type", "application/json");
        (req, method, url)
    }

    async fn exec<T: DeserializeOwned>(
        &self,
        req: RequestBuilder,
        method: &reqwest::Method,
        url: &str,
    ) -> Result<T, EsError> {
        self.exec_with_timeout::<T>(req, method, url, None).await
    }

    async fn exec_with_timeout<T: DeserializeOwned>(
        &self,
        req: RequestBuilder,
        method: &reqwest::Method,
        url: &str,
        timeout: Option<std::time::Duration>,
    ) -> Result<T, EsError> {
        tracing::info!("[{}] {} {}", self.config.name, method, url);

        let req = if let Some(t) = timeout {
            req.timeout(t)
        } else {
            req
        };

        let start = std::time::Instant::now();
        let resp = self.dispatch(req).await.inspect_err(|e| {
            tracing::error!(
                "[{}] {} {} — FAILED after {}: {}",
                self.config.name,
                method,
                url,
                elapsed_millis(start.elapsed()),
                e
            );
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

        let body = resp.text().await.map_err(|e| {
            tracing::error!(
                "[{}] {} {} — Read error after {}: {}",
                self.config.name,
                method,
                url,
                elapsed_millis(elapsed),
                e
            );
            EsError::Request(e.to_string())
        })?;

        if !body.is_empty() {
            tracing::debug!(
                "[{}] Response body: {}",
                self.config.name,
                truncate(&body, 2000)
            );
        }

        serde_json::from_str(&body).map_err(|e| {
            tracing::error!(
                "[{}] {} {} — Parse error after {}: {} (body: {})",
                self.config.name,
                method,
                url,
                elapsed_millis(elapsed),
                e,
                truncate(&body, 1000)
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
        let resp = self.dispatch(req).await.inspect_err(|e| {
            tracing::error!(
                "[{}] {} {} — FAILED after {}: {}",
                self.config.name,
                method,
                url,
                elapsed_millis(start.elapsed()),
                e
            );
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
        let path = format!("/_snapshot/{}/_all?size=1&sort=start_time&order=desc", repo);
        let (req, method, url) = self.request(reqwest::Method::GET, &path);
        self.exec(req, &method, &url).await
    }
    pub async fn snapshot_repositories(&self) -> Result<serde_json::Value, EsError> {
        let (req, method, url) = self.request(reqwest::Method::GET, "/_snapshot");
        self.exec(req, &method, &url).await
    }

    pub async fn snapshot_status_all(&self) -> Result<SnapshotStatusResponse, EsError> {
        let (req, method, url) = self.request(reqwest::Method::GET, "/_snapshot/_status");
        // Snapshot status can be slow for large S3 repos; give it more time.
        self.exec_with_timeout(
            req,
            &method,
            &url,
            Some(std::time::Duration::from_secs(180)),
        )
        .await
    }

    pub async fn snapshot_status(
        &self,
        repo: &str,
        snapshot: &str,
    ) -> Result<SnapshotStatusResponse, EsError> {
        let path = format!("/_snapshot/{}/{}/_status", repo, snapshot);
        let (req, method, url) = self.request(reqwest::Method::GET, &path);
        // Per-snapshot status may need to read a lot of metadata from the repo.
        self.exec_with_timeout(
            req,
            &method,
            &url,
            Some(std::time::Duration::from_secs(180)),
        )
        .await
    }

    pub async fn search_logs(
        &self,
        index_pattern: &str,
        filters: &LogFilters,
        fields: &LogFieldNames,
        limit: usize,
    ) -> Result<Vec<LogEntry>, EsError> {
        let path = format!("/{}/_search", index_pattern.trim_start_matches('/'));
        let mut must: Vec<serde_json::Value> = Vec::new();
        let mut filter: Vec<serde_json::Value> = Vec::new();

        if !filters.query.trim().is_empty() {
            must.push(json!({ "query_string": { "query": filters.query.trim() } }));
        }
        if !filters.app.trim().is_empty() {
            filter.push(json!({
                "wildcard": { fields.app.trim(): format!("*{}*", filters.app.trim()) }
            }));
        }
        if !filters.hostname.trim().is_empty() {
            filter.push(json!({
                "wildcard": { fields.hostname.trim(): format!("*{}*", filters.hostname.trim()) }
            }));
        }
        if !filters.severity.trim().is_empty() {
            filter.push(json!({
                "term": { fields.severity.trim(): filters.severity.trim() }
            }));
        }

        let body = json!({
            "query": {
                "bool": {
                    "must": must,
                    "filter": filter,
                }
            },
            "sort": [{ fields.timestamp.trim(): "desc" }],
            "size": limit,
        });

        let body_str = body.to_string();
        let (mut req, method, url) = self.request(reqwest::Method::POST, &path);
        req = req
            .body(body_str)
            .header("Content-Type", "application/json");

        let value: serde_json::Value = self.exec(req, &method, &url).await?;
        let hits = value
            .get("hits")
            .and_then(|h| h.get("hits"))
            .and_then(|h| h.as_array())
            .cloned()
            .unwrap_or_default();

        let mut entries = Vec::with_capacity(hits.len());
        for hit in hits {
            let source = hit.get("_source").cloned().unwrap_or_default();
            let as_str = |path: &str| {
                get_dot_path(&source, path)
                    .and_then(|v| v.as_str())
                    .map(|s| s.to_string())
            };
            entries.push(LogEntry {
                timestamp: as_str(&fields.timestamp),
                message: as_str(&fields.message),
                app: as_str(&fields.app),
                hostname: as_str(&fields.hostname),
                severity: as_str(&fields.severity),
                raw_source: source,
            });
        }
        Ok(entries)
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

    pub async fn get_index_templates(&self) -> Result<Vec<IndexTemplateItem>, EsError> {
        let (req, method, url) = self.request(reqwest::Method::GET, "/_index_template");
        let resp: GetIndexTemplatesResponse = self.exec(req, &method, &url).await?;
        Ok(resp.index_templates)
    }

    pub async fn get_component_templates(&self) -> Result<Vec<ComponentTemplateItem>, EsError> {
        let (req, method, url) = self.request(reqwest::Method::GET, "/_component_template");
        let resp: GetComponentTemplatesResponse = self.exec(req, &method, &url).await?;
        Ok(resp.component_templates)
    }

    pub async fn get_index_template(
        &self,
        name: &str,
    ) -> Result<Option<IndexTemplateItem>, EsError> {
        let path = format!("/_index_template/{}", name);
        let (req, method, url) = self.request(reqwest::Method::GET, &path);
        match self
            .exec::<GetIndexTemplatesResponse>(req, &method, &url)
            .await
        {
            Ok(resp) => Ok(resp.index_templates.into_iter().next()),
            Err(EsError::Http { status, .. }) if status == reqwest::StatusCode::NOT_FOUND => {
                Ok(None)
            }
            Err(e) => Err(e),
        }
    }

    pub async fn get_component_template(
        &self,
        name: &str,
    ) -> Result<Option<ComponentTemplateItem>, EsError> {
        let path = format!("/_component_template/{}", name);
        let (req, method, url) = self.request(reqwest::Method::GET, &path);
        match self
            .exec::<GetComponentTemplatesResponse>(req, &method, &url)
            .await
        {
            Ok(resp) => Ok(resp.component_templates.into_iter().next()),
            Err(EsError::Http { status, .. }) if status == reqwest::StatusCode::NOT_FOUND => {
                Ok(None)
            }
            Err(e) => Err(e),
        }
    }

    pub async fn cat_allocation(&self) -> Result<Vec<CatAllocation>, EsError> {
        let (req, method, url) = self.request(reqwest::Method::GET, "/_cat/allocation?format=json");
        self.exec(req, &method, &url).await
    }

    pub async fn cat_nodes(&self) -> Result<Vec<CatNode>, EsError> {
        let (req, method, url) = self.request(
            reqwest::Method::GET,
            "/_cat/nodes?format=json&h=ip,name,role,cpu,ram.percent,heap.percent,master",
        );
        self.exec(req, &method, &url).await
    }

    pub async fn get_es_version(&self) -> Result<String, EsError> {
        let res: serde_json::Value = self.execute(reqwest::Method::GET, "/", None).await?;
        if let Some(v) = res
            .get("version")
            .and_then(|v| v.get("number"))
            .and_then(|n| n.as_str())
        {
            Ok(v.to_string())
        } else {
            Err(EsError::Parse("Unable to parse ES version".to_string()))
        }
    }

    pub async fn get_kibana_version(&self, kibana_host: &str) -> Result<String, EsError> {
        let res = self
            .send_to_host(kibana_host, reqwest::Method::GET, "/api/status", None)
            .await?;
        if let Some(version) = res
            .get("version")
            .and_then(|v| v.get("number"))
            .and_then(|n| n.as_str())
        {
            Ok(version.to_string())
        } else if let Some(version) = res.get("version").and_then(|v| v.as_str()) {
            Ok(version.to_string())
        } else {
            Err(EsError::Parse("Unable to parse Kibana version".to_string()))
        }
    }

    pub async fn get_kibana_synthetics_monitors(
        &self,
        kibana_host: &str,
        space_id: Option<&str>,
    ) -> Result<serde_json::Value, EsError> {
        let path = match space_id {
            Some(space) if space != "default" && !space.is_empty() => {
                format!("/s/{}/api/synthetics/monitors", space)
            }
            _ => "/api/synthetics/monitors".to_string(),
        };
        self.send_to_host(kibana_host, reqwest::Method::GET, &path, None)
            .await
    }

    pub async fn get_node_hot_threads(&self, node_name: &str) -> Result<String, EsError> {
        let path = format!("/_nodes/{}/hot_threads", node_name);
        self.execute_raw(reqwest::Method::GET, &path, None).await
    }

    pub async fn get_pending_tasks(&self) -> Result<serde_json::Value, EsError> {
        self.execute(reqwest::Method::GET, "/_cluster/pending_tasks", None)
            .await
    }

    pub async fn cluster_stats(&self) -> Result<ClusterStats, EsError> {
        let (req, method, url) = self.request(reqwest::Method::GET, "/_cluster/stats");
        self.exec(req, &method, &url).await
    }

    pub async fn get_ingest_pipelines(&self) -> Result<serde_json::Value, EsError> {
        let (req, method, url) = self.request(reqwest::Method::GET, "/_ingest/pipeline");
        self.exec(req, &method, &url).await
    }

    pub async fn get_ingest_pipeline(&self, id: &str) -> Result<serde_json::Value, EsError> {
        let path = format!("/_ingest/pipeline/{}", id);
        let (req, method, url) = self.request(reqwest::Method::GET, &path);
        self.exec(req, &method, &url).await
    }

    pub async fn simulate_ingest(
        &self,
        target: &str,
        body: serde_json::Value,
    ) -> Result<serde_json::Value, EsError> {
        let path = format!("/_ingest/{}/_simulate", target);
        let body_str = body.to_string();
        let (req, method, url) = self.request(reqwest::Method::POST, &path);
        let req = req
            .header("Content-Type", "application/json")
            .body(body_str);
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
        self.send_to_host_with_version(host, method, path, body, None)
            .await
    }

    pub async fn send_to_host_with_version(
        &self,
        host: &str,
        method: reqwest::Method,
        path: &str,
        body: Option<String>,
        api_version: Option<&str>,
    ) -> Result<serde_json::Value, EsError> {
        let base = host.trim_end_matches('/');
        let url = if path.starts_with('/') {
            format!("{}{}", base, path)
        } else {
            format!("{}/{}", base, path)
        };
        let mut req = self
            .apply_auth(self.client.request(method.clone(), &url))
            .header("kbn-xsrf", "true")
            .header("Content-Type", "application/json");

        if let Some(version) = api_version {
            req = req.header("Elastic-Api-Version", version);
        }
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
            .apply_auth(self.client.request(method.clone(), &url))
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

#[derive(Debug, Clone, Default, serde::Deserialize, serde::Serialize)]

pub struct SnapshotInfo {
    pub snapshot: String,
    pub uuid: String,
    #[serde(rename = "repository")]
    pub repository: Option<String>,
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

#[derive(Debug, Clone, Default, serde::Deserialize, serde::Serialize)]

pub struct ShardStats {
    pub total: Option<u32>,
    pub failed: Option<u32>,
    pub successful: Option<u32>,
}

#[derive(Debug, Clone, serde::Deserialize, serde::Serialize, Default)]

pub struct SnapshotStatusResponse {
    pub snapshots: Vec<SnapshotStatusInfo>,
}

#[derive(Debug, Clone, serde::Deserialize, serde::Serialize)]

pub struct SnapshotStatusInfo {
    pub snapshot: String,
    pub repository: Option<String>,
    pub uuid: String,
    pub state: String,
    #[serde(rename = "include_global_state")]
    pub include_global_state: Option<bool>,
    pub shards_stats: Option<ShardsStatsDetail>,
    pub stats: Option<SnapshotStatsDetail>,
    pub indices: Option<serde_json::Value>,
}

#[derive(Debug, Clone, Default, serde::Deserialize, serde::Serialize)]

pub struct ShardsStatsDetail {
    pub initializing: Option<u32>,
    pub started: Option<u32>,
    pub finalizing: Option<u32>,
    pub done: Option<u32>,
    pub failed: Option<u32>,
    pub total: Option<u32>,
}

#[derive(Debug, Clone, Default, serde::Deserialize, serde::Serialize)]

pub struct SnapshotStatsDetail {
    #[serde(rename = "number_of_files")]
    pub number_of_files: Option<u32>,
    #[serde(rename = "processed_files")]
    pub processed_files: Option<u32>,
    #[serde(rename = "total_size_in_bytes")]
    pub total_size_in_bytes: Option<u64>,
    #[serde(rename = "processed_size_in_bytes")]
    pub processed_size_in_bytes: Option<u64>,
    #[serde(rename = "number_of_chunks")]
    pub number_of_chunks: Option<u32>,
    #[serde(rename = "incremental")]
    pub incremental: Option<IncrementalStats>,
}

#[derive(Debug, Clone, Default, serde::Deserialize, serde::Serialize)]

pub struct IncrementalStats {
    #[serde(rename = "file_count")]
    pub file_count: Option<u32>,
    #[serde(rename = "size_in_bytes")]
    pub size_in_bytes: Option<u64>,
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
    #[serde(rename = "start_time")]
    pub start_time: Option<i64>,
    #[serde(rename = "time")]
    pub time: Option<i64>,
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

/// Field names to extract from ECS-style log documents.
#[derive(Debug, Clone, Default)]
pub struct LogFieldNames {
    pub timestamp: String,
    pub message: String,
    pub app: String,
    pub hostname: String,
    pub severity: String,
}

/// Simple filter values for the Logs tab.
#[derive(Debug, Clone, Default)]
pub struct LogFilters {
    pub app: String,
    pub hostname: String,
    pub severity: String,
    pub query: String,
}

/// A single log row, with commonly-used fields extracted and the raw source kept for expansion.
#[derive(Debug, Clone, Default)]
pub struct LogEntry {
    pub timestamp: Option<String>,
    pub message: Option<String>,
    pub app: Option<String>,
    pub hostname: Option<String>,
    pub severity: Option<String>,
    pub raw_source: serde_json::Value,
}

fn get_dot_path<'a>(value: &'a serde_json::Value, path: &str) -> Option<&'a serde_json::Value> {
    let mut current = value;
    for part in path.split('.') {
        current = current.get(part)?;
    }
    Some(current)
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

#[derive(Debug, Clone, serde::Deserialize, serde::Serialize, Default)]
pub struct CatNode {
    pub ip: Option<String>,
    pub name: Option<String>,
    pub role: Option<String>,
    pub cpu: Option<String>,
    #[serde(rename = "ram.percent")]
    pub ram_percent: Option<String>,
    #[serde(rename = "heap.percent")]
    pub heap_percent: Option<String>,
    pub master: Option<String>,
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
        let Some(slot) = &self.tunnel else {
            return match self.cluster_health().await {
                Ok(h) => Ok(format!(
                    "Connected successfully!\n\n• Cluster: {}\n• Status: {}\n• Nodes: {}\n• Active Shards: {}",
                    h.cluster_name, h.status, h.number_of_nodes, h.active_shards
                )),
                Err(e) => Err(format!(
                    "HTTP Request Failed!\n\nError: {}\n\nPossible causes:\n1. Elasticsearch is not running at {}\n2. Incorrect username or password\n3. Network/firewall blocking connection\n4. TLS/SSL verification failed (override available in cluster settings)",
                    e, self.config.host
                )),
            };
        };

        // A fresh, verbose tunnel, so the test reflects the current settings and shows ssh's log.
        let local_port = match slot.connect(&self.config, true).await {
            Ok((_, port)) => port,
            Err(e) => {
                return Err(format!(
                    "SSH tunnel failed!\n\n{}\n\nCommand: {}\n\n=== SSH Connection Logs ===\n{}",
                    e.summary,
                    e.command,
                    e.logs.join("\n")
                ));
            }
        };
        let route = ssh_tunnel::describe(&self.config, &slot.target, local_port);
        let health = self.cluster_health().await;
        let (_, logs) = slot.logs().await;
        let logs = logs.join("\n");
        match health {
            Ok(h) => Ok(format!(
                "Connected successfully through SSH tunnel!\n\n• Tunnel: {}\n• Cluster: {}\n• Status: {}\n• Nodes: {}\n• Active Shards: {}\n\n=== SSH Connection Logs ===\n{}",
                route, h.cluster_name, h.status, h.number_of_nodes, h.active_shards, logs
            )),
            Err(e) => Err(format!(
                "SSH tunnel is up, but the Elasticsearch request through it failed!\n\nError: {}\n\nTunnel: {}\n\nPossible causes:\n1. Elasticsearch is not reachable from {} at {}:{} (an https:// host without a port uses 443, http:// uses 80)\n2. Incorrect username or password\n3. TLS/SSL verification failed (override available in cluster settings)\n\n=== SSH Connection Logs ===\n{}",
                e,
                route,
                self.config.ssh_host.trim(),
                slot.target.host,
                slot.target.port,
                logs
            )),
        }
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

#[derive(Debug, Clone, serde::Deserialize, serde::Serialize)]
pub struct GetIndexTemplatesResponse {
    #[serde(default)]
    pub index_templates: Vec<IndexTemplateItem>,
}

#[derive(Debug, Clone, serde::Deserialize, serde::Serialize)]
pub struct IndexTemplateItem {
    pub name: String,
    #[serde(default)]
    pub index_template: IndexTemplateContent,
}

#[derive(Debug, Clone, Default, serde::Deserialize, serde::Serialize)]
pub struct IndexTemplateContent {
    #[serde(default)]
    pub index_patterns: Vec<String>,
    #[serde(default)]
    pub composed_of: Vec<String>,
    pub priority: Option<i64>,
    pub version: Option<i64>,
    pub _meta: Option<serde_json::Value>,
    pub data_stream: Option<serde_json::Value>,
    pub template: Option<serde_json::Value>,
}

#[derive(Debug, Clone, serde::Deserialize, serde::Serialize)]
pub struct GetComponentTemplatesResponse {
    #[serde(default)]
    pub component_templates: Vec<ComponentTemplateItem>,
}

#[derive(Debug, Clone, serde::Deserialize, serde::Serialize)]
pub struct ComponentTemplateItem {
    pub name: String,
    #[serde(default)]
    pub component_template: ComponentTemplateContent,
}

#[derive(Debug, Clone, Default, serde::Deserialize, serde::Serialize)]
pub struct ComponentTemplateContent {
    pub template: Option<serde_json::Value>,
    pub version: Option<i64>,
    pub _meta: Option<serde_json::Value>,
}

pub fn parse_allocation_explain(raw: &serde_json::Value) -> AllocationExplain {
    let mut explain = AllocationExplain {
        index: raw["index"].as_str().unwrap_or("unknown").to_string(),
        shard: raw["shard"].as_u64().unwrap_or(0) as u32,
        primary: raw["primary"].as_bool().unwrap_or(false),
        current_state: raw["current_state"]
            .as_str()
            .unwrap_or("unknown")
            .to_string(),
        ..Default::default()
    };

    if let Some(ui) = raw.get("unassigned_info") {
        explain.reason = ui
            .get("reason")
            .and_then(|r| r.as_str())
            .map(|s| s.to_string());
        explain.details = ui
            .get("details")
            .and_then(|d| d.as_str())
            .map(|s| s.to_string());
    }

    explain.explanation = raw
        .get("allocate_explanation")
        .or_else(|| raw.get("explanation"))
        .and_then(|e| e.as_str())
        .map(|s| s.to_string());

    if let Some(decisions) = raw.get("node_decisions").and_then(|d| d.as_array()) {
        for node in decisions {
            let node_name = node["node_name"].as_str().unwrap_or("unknown");
            if let Some(decs) = node["decisions"].as_array() {
                for dec in decs {
                    if dec["decision"].as_str() == Some("NO")
                        && let Some(exp) = dec["explanation"].as_str()
                    {
                        explain
                            .decider_reasons
                            .push(format!("{}: {}", node_name, exp));
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
    fn cluster_auth_headers_and_config_roundtrip() {
        let mut config = ClusterConfig {
            name: "test".into(),
            host: "http://example.invalid:9200".into(),
            username: "user".into(),
            ..Default::default()
        };
        let client = EsClient::with_password(&config, "password").unwrap();
        let (req, _, _) = client.request(reqwest::Method::GET, "/_cluster/health");
        let req = req.build().unwrap();
        assert_eq!(
            req.headers()[reqwest::header::AUTHORIZATION],
            "Basic dXNlcjpwYXNzd29yZA=="
        );

        config.auth_method = AuthMethod::ApiKey;
        for (key, expected) in [
            ("encoded-test-key", "encoded-test-key"),
            ("  ApiKey encoded-test-key  ", "encoded-test-key"),
            ("id:key", "aWQ6a2V5"),
            ("ApiKey id:key", "aWQ6a2V5"),
        ] {
            let client = EsClient::with_password(&config, key).unwrap();
            let (req, _, _) = client.request(reqwest::Method::GET, "/_cluster/health");
            let req = req.build().unwrap();
            let header = &req.headers()[reqwest::header::AUTHORIZATION];
            assert_eq!(header, format!("ApiKey {expected}").as_str());
            assert!(header.is_sensitive());
            let kibana = client.apply_auth(client.client.get("http://example.invalid/api/status"));
            assert_eq!(
                kibana.build().unwrap().headers()[reqwest::header::AUTHORIZATION],
                header
            );
            assert!(!format!("{client:?}").contains(expected));
        }
        let serialized = serde_json::to_value(&config).unwrap();
        assert_eq!(serialized["auth_method"], "api_key");
        assert!(!serialized.to_string().contains("encoded-test-key"));
        let mut legacy = serialized;
        legacy.as_object_mut().unwrap().remove("auth_method");
        assert_eq!(
            serde_json::from_value::<ClusterConfig>(legacy)
                .unwrap()
                .auth_method,
            AuthMethod::Basic
        );
    }

    #[test]
    fn api_key_rejects_header_injection() {
        let config = ClusterConfig {
            host: "http://example.invalid".into(),
            auth_method: AuthMethod::ApiKey,
            ..Default::default()
        };
        let client = EsClient::with_password(&config, "key\r\nInjected: value").unwrap();
        let (req, _, _) = client.request(reqwest::Method::GET, "/");
        assert!(req.build().is_err());
    }

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
        assert_eq!(
            parsed.explanation.as_deref(),
            Some("Elasticsearch cannot allocate this primary shard because the disk is full.")
        );
        assert_eq!(
            parsed.decider_reasons,
            vec!["node-us-east-1: high watermark reached on this node".to_string()]
        );
    }
}

#[cfg(test)]
mod api_key_tests {
    use super::api_key_credentials;

    #[test]
    fn encoded_key_passes_through() {
        assert_eq!(api_key_credentials("  abc123==  "), "abc123==");
    }

    #[test]
    fn id_key_pair_is_base64_encoded() {
        assert_eq!(api_key_credentials("id:key"), "aWQ6a2V5");
    }
}
