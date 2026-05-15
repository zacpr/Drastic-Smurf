use std::collections::HashMap;
use std::sync::{Arc, Mutex};

use crate::core::config::{ClusterConfig, SavedCommand};
use crate::core::es_client::EsClient;
use crate::core::ssh_tunnel::SshTunnel;

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
    saved_commands: Arc<Mutex<Vec<SavedCommand>>>,
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
            saved_commands: Arc::new(Mutex::new(Vec::new())),
        }
    }

    pub fn load(&self) -> anyhow::Result<()> {
        let config = crate::core::config::AppConfig::load()?;
        let mut clusters = self.clusters.lock().unwrap();
        *clusters = config.clusters;

        {
            let mut saved = self.saved_commands.lock().unwrap();
            *saved = config.saved_commands;
        }

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
        let saved = self.saved_commands.lock().unwrap().clone();
        let config = crate::core::config::AppConfig {
            clusters: clusters.clone(),
            auto_refresh: true,
            refresh_interval_secs: 15,
            saved_commands: saved,
        };
        config.save()?;
        Ok(())
    }

    pub fn clusters(&self) -> Vec<ClusterConfig> {
        self.clusters.lock().unwrap().clone()
    }

    pub fn add_cluster(&self, config: ClusterConfig) -> anyhow::Result<()> {
        {
            let mut clusters = self.clusters.lock().unwrap();
            clusters.push(config.clone());
        }
        {
            let mut clients = self.clients.lock().unwrap();
            match EsClient::new(&config) {
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
        self.save()?;
        Ok(())
    }

    pub fn update_cluster(&self, old_name: &str, config: ClusterConfig) -> anyhow::Result<()> {
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
            match EsClient::new(&config) {
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
        }
        self.save()?;
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
        self.save()?;
        Ok(())
    }

    pub fn get_client(&self, name: &str) -> Option<EsClient> {
        self.clients.lock().unwrap().get(name).cloned()
    }

    pub fn saved_commands(&self) -> Vec<SavedCommand> {
        self.saved_commands.lock().unwrap().clone()
    }

    pub fn add_saved_command(&self, cmd: SavedCommand) -> anyhow::Result<()> {
        {
            let mut saved = self.saved_commands.lock().unwrap();
            // Avoid duplicates by name
            saved.retain(|s| s.name != cmd.name);
            saved.push(cmd);
        }
        self.save()?;
        Ok(())
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

                // Recreate client with tunnel URL
                match EsClient::new(&cluster) {
                    Ok(client) => {
                        let client = client.with_tunnel(&url);
                        self.clients
                            .lock()
                            .unwrap()
                            .insert(name.to_string(), client);
                    }
                    Err(e) => {
                        tracing::warn!(
                            "Failed to create client for tunnelled cluster '{}': {}",
                            name,
                            e
                        );
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
