use std::collections::HashMap;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use crate::core::config::ClusterConfig;
use crate::core::es_client::EsClient;
use crate::core::ssh_tunnel::SshTunnel;

const SAVE_DEBOUNCE: Duration = Duration::from_secs(5);

#[derive(Debug, Clone)]
pub struct TunnelInfo {
    pub local_port: u16,
    pub pid: u32,
}

#[derive(Debug, Clone)]
pub struct ClusterManager {
    clusters: Arc<Mutex<Vec<ClusterConfig>>>,
    clients: Arc<Mutex<HashMap<String, EsClient>>>,
    tunnels: Arc<Mutex<HashMap<String, TunnelInfo>>>,
    cluster_data: Arc<Mutex<HashMap<String, crate::core::config::ClusterData>>>,
    auto_refresh: Arc<Mutex<bool>>,
    refresh_interval_secs: Arc<Mutex<u64>>,
    theme: Arc<Mutex<crate::ui::theme::AppTheme>>,
    vfx: Arc<Mutex<crate::core::config::VfxSettings>>,
    dirty: Arc<AtomicBool>,
    save_after: Arc<Mutex<Option<Instant>>>,
}

impl Default for ClusterManager {
    fn default() -> Self {
        Self::new()
    }
}

impl ClusterManager {
    pub fn new() -> Self {
        Self {
            clusters: Arc::new(Mutex::new(Vec::new())),
            clients: Arc::new(Mutex::new(HashMap::new())),
            tunnels: Arc::new(Mutex::new(HashMap::new())),
            cluster_data: Arc::new(Mutex::new(HashMap::new())),
            auto_refresh: Arc::new(Mutex::new(true)),
            refresh_interval_secs: Arc::new(Mutex::new(15)),
            theme: Arc::new(Mutex::new(crate::ui::theme::AppTheme::default())),
            vfx: Arc::new(Mutex::new(crate::core::config::VfxSettings::default())),
            dirty: Arc::new(AtomicBool::new(false)),
            save_after: Arc::new(Mutex::new(None)),
        }
    }

    pub fn load(&self) -> anyhow::Result<()> {
        let config = crate::core::config::AppConfig::load()?;
        {
            let mut clusters = self.clusters.lock().unwrap();
            *clusters = config.clusters;
        }
        {
            let mut data = self.cluster_data.lock().unwrap();
            *data = config.cluster_data;
        }
        {
            let mut ar = self.auto_refresh.lock().unwrap();
            *ar = config.auto_refresh;
        }
        {
            let mut ri = self.refresh_interval_secs.lock().unwrap();
            *ri = config.refresh_interval_secs;
        }
        {
            let mut t = self.theme.lock().unwrap();
            *t = config.theme;
        }
        {
            let mut v = self.vfx.lock().unwrap();
            *v = config.vfx;
        }

        let clusters = self.clusters.lock().unwrap();
        let mut clients = self.clients.lock().unwrap();
        clients.clear();
        for cluster in clusters.iter() {
            match EsClient::new(cluster) {
                Ok(client) => {
                    clients.insert(cluster.name.clone(), client);
                }
                Err(e) => {
                    tracing::warn!(
                        "Failed to create client for cluster '{}': {}",
                        cluster.name,
                        e
                    );
                }
            }
        }
        Ok(())
    }

    pub fn save(&self) -> anyhow::Result<()> {
        let clusters = self.clusters.lock().unwrap();
        let data = self.cluster_data.lock().unwrap();
        let auto_refresh = *self.auto_refresh.lock().unwrap();
        let refresh_interval_secs = *self.refresh_interval_secs.lock().unwrap();
        let theme = self.theme.lock().unwrap().clone();
        let vfx = self.vfx.lock().unwrap().clone();
        let config = crate::core::config::AppConfig {
            clusters: clusters.clone(),
            cluster_data: data.clone(),
            auto_refresh,
            refresh_interval_secs,
            theme,
            vfx,
            ..Default::default()
        };
        config.save()?;
        Ok(())
    }

    pub fn mark_dirty(&self) {
        self.dirty.store(true, Ordering::Relaxed);
        let mut after = self.save_after.lock().unwrap();
        *after = Some(Instant::now() + SAVE_DEBOUNCE);
    }

    pub fn save_if_due(&self) -> anyhow::Result<()> {
        if !self.dirty.load(Ordering::Relaxed) {
            return Ok(());
        }
        let should_save = {
            let after = self.save_after.lock().unwrap();
            match *after {
                Some(t) => Instant::now() >= t,
                None => true,
            }
        };
        if should_save {
            self.save()?;
            self.dirty.store(false, Ordering::Relaxed);
            let mut after = self.save_after.lock().unwrap();
            *after = None;
        }
        Ok(())
    }

    pub fn save_theme_and_vfx(
        &self,
        theme: crate::ui::theme::AppTheme,
        vfx: crate::core::config::VfxSettings,
    ) -> anyhow::Result<()> {
        {
            let mut t = self.theme.lock().unwrap();
            *t = theme;
        }
        {
            let mut v = self.vfx.lock().unwrap();
            *v = vfx;
        }
        self.mark_dirty();
        Ok(())
    }

    pub fn clusters(&self) -> Vec<ClusterConfig> {
        self.clusters.lock().unwrap().clone()
    }

    pub fn add_cluster(&self, config: ClusterConfig, password: Option<&str>) -> anyhow::Result<()> {
        {
            let mut clusters = self.clusters.lock().unwrap();
            clusters.push(config.clone());
        }
        {
            let mut clients = self.clients.lock().unwrap();
            let client_result = match password {
                Some(pw) => EsClient::with_password(&config, pw),
                None => EsClient::new(&config),
            };
            match client_result {
                Ok(client) => {
                    clients.insert(config.name.clone(), client);
                }
                Err(e) => {
                    tracing::warn!(
                        "Failed to create client for cluster '{}': {}",
                        config.name,
                        e
                    );
                }
            }
        }
        self.mark_dirty();
        Ok(())
    }

    pub fn update_cluster(&self, old_name: &str, config: ClusterConfig, password: Option<&str>) -> anyhow::Result<()> {
        {
            let mut clusters = self.clusters.lock().unwrap();
            if let Some(idx) = clusters.iter().position(|c| c.name == old_name) {
                clusters[idx] = config.clone();
            } else {
                clusters.push(config.clone());
            }
        }
        {
            let mut clients = self.clients.lock().unwrap();
            clients.remove(old_name);
            let client_result = match password {
                Some(pw) => EsClient::with_password(&config, pw),
                None => EsClient::new(&config),
            };
            match client_result {
                Ok(client) => {
                    clients.insert(config.name.clone(), client);
                }
                Err(e) => {
                    tracing::warn!(
                        "Failed to create client for cluster '{}': {}",
                        config.name,
                        e
                    );
                }
            }
        }
        // Remove old tunnel if name changed
        if old_name != config.name {
            self.kill_tunnel(old_name);
            // Migrate cluster data to new name
            if let Some(data) = self.get_cluster_data(old_name) {
                self.set_cluster_data(&config.name, data);
                self.remove_cluster_data(old_name);
            }
            // Delete old keyring entry (new password already saved by caller)
            let _ = crate::core::auth::delete_password(old_name);
        }
        self.mark_dirty();
        Ok(())
    }

    pub fn remove_cluster(&self, name: &str) -> anyhow::Result<()> {
        {
            let mut clusters = self.clusters.lock().unwrap();
            clusters.retain(|c| c.name != name);
        }
        {
            let mut clients = self.clients.lock().unwrap();
            clients.remove(name);
        }
        self.kill_tunnel(name);
        let _ = crate::core::auth::delete_password(name);
        self.remove_cluster_data(name);
        self.mark_dirty();
        Ok(())
    }

    pub fn get_client(&self, name: &str) -> Option<EsClient> {
        self.clients.lock().unwrap().get(name).cloned()
    }

    pub fn set_client(&self, name: &str, client: EsClient) {
        self.clients.lock().unwrap().insert(name.to_string(), client);
    }

    pub fn rebuild_client(&self, name: &str) {
        let config = {
            let clusters = self.clusters.lock().unwrap();
            clusters.iter().find(|c| c.name == name).cloned()
        };

        let existing_password = {
            let clients = self.clients.lock().unwrap();
            clients.get(name).map(|c| c.password().to_string())
        };

        if let Some(config) = config {
            let result = match &existing_password {
                Some(pw) => {
                    tracing::info!(
                        "Rebuilding ES client for '{}' preserving existing password ({} chars)",
                        name,
                        pw.len()
                    );
                    EsClient::with_password(&config, pw)
                }
                None => {
                    tracing::warn!(
                        "Rebuilding ES client for '{}' — no existing password, reading from keyring",
                        name
                    );
                    EsClient::new(&config)
                },
            };

            match result {
                Ok(client) => {
                    let mut clients = self.clients.lock().unwrap();
                    clients.insert(name.to_string(), client);
                    tracing::info!("Successfully rebuilt ES client for '{}'", name);
                }
                Err(e) => {
                    tracing::warn!("Failed to rebuild ES client for '{}': {}", name, e);
                }
            }
        }
    }

    pub fn auto_refresh(&self) -> bool {
        *self.auto_refresh.lock().unwrap()
    }

    pub fn set_auto_refresh(&self, value: bool) {
        *self.auto_refresh.lock().unwrap() = value;
        self.mark_dirty();
    }

    pub fn refresh_interval_secs(&self) -> u64 {
        *self.refresh_interval_secs.lock().unwrap()
    }

    pub fn set_refresh_interval_secs(&self, value: u64) {
        *self.refresh_interval_secs.lock().unwrap() = value;
        self.mark_dirty();
    }

    pub fn get_cluster_data(&self, name: &str) -> Option<crate::core::config::ClusterData> {
        self.cluster_data.lock().unwrap().get(name).cloned()
    }

    pub fn all_cluster_data(
        &self,
    ) -> std::collections::HashMap<String, crate::core::config::ClusterData> {
        self.cluster_data.lock().unwrap().clone()
    }

    pub fn set_cluster_data(&self, name: &str, data: crate::core::config::ClusterData) {
        self.cluster_data
            .lock()
            .unwrap()
            .insert(name.to_string(), data);
        self.mark_dirty();
    }

    pub fn remove_cluster_data(&self, name: &str) {
        self.cluster_data.lock().unwrap().remove(name);
    }

    pub async fn ensure_tunnel(&self, name: &str) -> anyhow::Result<()> {
        // Check if tunnel already exists
        {
            let tunnels = self.tunnels.lock().unwrap();
            if tunnels.contains_key(name) {
                return Ok(());
            }
        }

        let cluster = {
            let clusters = self.clusters.lock().unwrap();
            clusters.iter().find(|c| c.name == name).cloned()
        };

        if let Some(cluster) = cluster {
            if cluster.ssh_tunnel && !cluster.ssh_host.is_empty() {
                let info = SshTunnel::spawn(&cluster).await?;
                let url = format!("http://127.0.0.1:{}", info.local_port);

                // Double-check after async op
                let mut tunnels = self.tunnels.lock().unwrap();
                if tunnels.contains_key(name) {
                    // Another task already created the tunnel; kill ours
                    SshTunnel::kill_by_pid(info.pid);
                    return Ok(());
                }

                // Recreate client with tunnel URL, preserving existing password
                let existing_password = {
                    let clients = self.clients.lock().unwrap();
                    clients.get(name).map(|c| c.password().to_string())
                };
                match existing_password {
                    Some(pw) => {
                        if let Ok(client) = EsClient::with_password(&cluster, &pw) {
                            let client = client.with_tunnel(&url);
                            self.clients.lock().unwrap().insert(name.to_string(), client);
                        }
                    }
                    None => {
                        if let Ok(client) = EsClient::new(&cluster) {
                            let client = client.with_tunnel(&url);
                            self.clients.lock().unwrap().insert(name.to_string(), client);
                        }
                    }
                }

                tunnels.insert(name.to_string(), info);
            }
        }
        Ok(())
    }

    fn kill_tunnel(&self, name: &str) {
        let mut tunnels = self.tunnels.lock().unwrap();
        if let Some(info) = tunnels.remove(name) {
            SshTunnel::kill_by_pid(info.pid);
        }
    }
}
