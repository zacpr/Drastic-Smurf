use std::collections::HashMap;
use std::sync::{Arc, Mutex};

use crate::core::config::ClusterConfig;
use crate::core::es_client::EsClient;

#[derive(Debug, Clone)]
pub struct ClusterManager {
    clusters: Arc<Mutex<Vec<ClusterConfig>>>,
    clients: Arc<Mutex<HashMap<String, EsClient>>>,
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
        }
    }

    pub fn load(&self) -> anyhow::Result<()> {
        let config = crate::core::config::AppConfig::load()?;
        let mut clusters = self.clusters.lock().unwrap();
        *clusters = config.clusters;
        
        let mut clients = self.clients.lock().unwrap();
        clients.clear();
        for cluster in clusters.iter() {
            if let Ok(client) = EsClient::new(cluster) {
                clients.insert(cluster.name.clone(), client);
            }
        }
        Ok(())
    }

    pub fn save(&self) -> anyhow::Result<()> {
        let clusters = self.clusters.lock().unwrap();
        let config = crate::core::config::AppConfig {
            clusters: clusters.clone(),
            auto_refresh: true,
            refresh_interval_secs: 15,
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
            if let Ok(client) = EsClient::new(&config) {
                clients.insert(config.name.clone(), client);
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
            if let Ok(client) = EsClient::new(&config) {
                clients.insert(config.name.clone(), client);
            }
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
        let _ = crate::core::auth::delete_password(name);
        self.save()?;
        Ok(())
    }

    pub fn get_client(&self, name: &str) -> Option<EsClient> {
        self.clients.lock().unwrap().get(name).cloned()
    }
}
