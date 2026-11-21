use std::collections::{HashMap, HashSet};
use std::path::Path;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use crate::core::config::ClusterConfig;
use crate::core::es_client::EsClient;

const SAVE_DEBOUNCE: Duration = Duration::from_secs(5);

struct SaveSnapshot {
    clusters: Vec<ClusterConfig>,
    cluster_data: HashMap<String, crate::core::config::ClusterData>,
    auto_refresh: bool,
    refresh_interval_secs: u64,
    theme: crate::ui::theme::AppTheme,
    vfx: crate::core::config::VfxSettings,
    timezone_clocks: Vec<crate::core::config::TimezoneClockConfig>,
    cluster_filter: String,
    pipeline_test_presets: Vec<crate::core::config::PipelineTestPreset>,
    god_console_saved_queries: Vec<crate::core::config::GodSavedQuery>,
    llm: crate::core::config::LlmSettings,
    assistant_dock_visible: bool,
    assistant_conversations: Vec<crate::core::config::ChatConversation>,
    assistant_active_conversation_id: Option<String>,
}

#[derive(Debug, Clone)]
pub struct ClusterManager {
    clusters: Arc<Mutex<Vec<ClusterConfig>>>,
    /// SSH-tunnelled clusters' clients own their tunnel (see `EsClient`); dropping a
    /// cluster's last client clone closes it.
    clients: Arc<Mutex<HashMap<String, EsClient>>>,
    cluster_data: Arc<Mutex<HashMap<String, crate::core::config::ClusterData>>>,
    auto_refresh: Arc<Mutex<bool>>,
    refresh_interval_secs: Arc<Mutex<u64>>,
    theme: Arc<Mutex<crate::ui::theme::AppTheme>>,
    vfx: Arc<Mutex<crate::core::config::VfxSettings>>,
    timezone_clocks: Arc<Mutex<Vec<crate::core::config::TimezoneClockConfig>>>,
    cluster_filter: Arc<Mutex<String>>,
    pipeline_test_presets: Arc<Mutex<Vec<crate::core::config::PipelineTestPreset>>>,
    god_console_saved_queries: Arc<Mutex<Vec<crate::core::config::GodSavedQuery>>>,
    llm: Arc<Mutex<crate::core::config::LlmSettings>>,
    assistant_dock_visible: Arc<Mutex<bool>>,
    assistant_conversations: Arc<Mutex<Vec<crate::core::config::ChatConversation>>>,
    assistant_active_conversation_id: Arc<Mutex<Option<String>>>,
    dirty: Arc<AtomicBool>,
    save_after: Arc<Mutex<Option<Instant>>>,
    save_in_flight: Arc<AtomicBool>,
    save_io_lock: Arc<Mutex<()>>,
    last_save_error: Arc<Mutex<Option<String>>>,
    rebuilding_clients: Arc<Mutex<HashSet<String>>>,
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
            cluster_data: Arc::new(Mutex::new(HashMap::new())),
            auto_refresh: Arc::new(Mutex::new(true)),
            refresh_interval_secs: Arc::new(Mutex::new(15)),
            theme: Arc::new(Mutex::new(crate::ui::theme::AppTheme::default())),
            vfx: Arc::new(Mutex::new(crate::core::config::VfxSettings::default())),
            timezone_clocks: Arc::new(Mutex::new(crate::core::config::default_timezone_clocks())),
            cluster_filter: Arc::new(Mutex::new(String::new())),
            pipeline_test_presets: Arc::new(Mutex::new(Vec::new())),
            god_console_saved_queries: Arc::new(Mutex::new(Vec::new())),
            llm: Arc::new(Mutex::new(crate::core::config::LlmSettings::default())),
            assistant_dock_visible: Arc::new(Mutex::new(false)),
            assistant_conversations: Arc::new(Mutex::new(Vec::new())),
            assistant_active_conversation_id: Arc::new(Mutex::new(None)),
            dirty: Arc::new(AtomicBool::new(false)),
            save_after: Arc::new(Mutex::new(None)),
            save_in_flight: Arc::new(AtomicBool::new(false)),
            save_io_lock: Arc::new(Mutex::new(())),
            last_save_error: Arc::new(Mutex::new(None)),
            rebuilding_clients: Arc::new(Mutex::new(HashSet::new())),
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
        {
            let mut tc = self.timezone_clocks.lock().unwrap();
            *tc = config.timezone_clocks;
        }
        {
            let mut cf = self.cluster_filter.lock().unwrap();
            *cf = config.cluster_filter;
        }
        {
            let mut p = self.pipeline_test_presets.lock().unwrap();
            *p = config.pipeline_test_presets;
        }
        {
            let mut g = self.god_console_saved_queries.lock().unwrap();
            *g = config.god_console_saved_queries;
        }
        {
            let mut l = self.llm.lock().unwrap();
            *l = config.llm;
        }
        {
            let mut v = self.assistant_dock_visible.lock().unwrap();
            *v = config.assistant_dock_visible;
        }
        {
            let mut c = self.assistant_conversations.lock().unwrap();
            *c = config.assistant_conversations;
        }
        {
            let mut id = self.assistant_active_conversation_id.lock().unwrap();
            *id = config.assistant_active_conversation_id;
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

    fn snapshot_for_save(&self) -> SaveSnapshot {
        let clusters = self.clusters.lock().unwrap().clone();
        let cluster_data = self.cluster_data.lock().unwrap().clone();
        let auto_refresh = *self.auto_refresh.lock().unwrap();
        let refresh_interval_secs = *self.refresh_interval_secs.lock().unwrap();
        let theme = self.theme.lock().unwrap().clone();
        let vfx = self.vfx.lock().unwrap().clone();
        let timezone_clocks = self.timezone_clocks.lock().unwrap().clone();
        let cluster_filter = self.cluster_filter.lock().unwrap().clone();
        let pipeline_test_presets = self.pipeline_test_presets.lock().unwrap().clone();
        let god_console_saved_queries = self.god_console_saved_queries.lock().unwrap().clone();
        let llm = self.llm.lock().unwrap().clone();
        let assistant_dock_visible = *self.assistant_dock_visible.lock().unwrap();
        let assistant_conversations = self.assistant_conversations.lock().unwrap().clone();
        let assistant_active_conversation_id = self
            .assistant_active_conversation_id
            .lock()
            .unwrap()
            .clone();

        SaveSnapshot {
            clusters,
            cluster_data,
            auto_refresh,
            refresh_interval_secs,
            theme,
            vfx,
            timezone_clocks,
            cluster_filter,
            pipeline_test_presets,
            god_console_saved_queries,
            llm,
            assistant_dock_visible,
            assistant_conversations,
            assistant_active_conversation_id,
        }
    }

    fn write_snapshot_to(&self, snapshot: SaveSnapshot, path: &Path) -> anyhow::Result<()> {
        let _io_guard = self.save_io_lock.lock().unwrap();
        // Propagate load errors: saving defaults over an unreadable file would lose it.
        let mut config = crate::core::config::AppConfig::load_from(path)?;
        config.clusters = snapshot.clusters;
        config.cluster_data = snapshot.cluster_data;
        config.auto_refresh = snapshot.auto_refresh;
        config.refresh_interval_secs = snapshot.refresh_interval_secs;
        config.theme = snapshot.theme;
        config.vfx = snapshot.vfx;
        config.timezone_clocks = snapshot.timezone_clocks;
        config.cluster_filter = snapshot.cluster_filter;
        config.pipeline_test_presets = snapshot.pipeline_test_presets;
        config.god_console_saved_queries = snapshot.god_console_saved_queries;
        config.llm = snapshot.llm;
        config.assistant_dock_visible = snapshot.assistant_dock_visible;
        config.assistant_conversations = snapshot.assistant_conversations;
        config.assistant_active_conversation_id = snapshot.assistant_active_conversation_id;

        if let Some(parent) = path.parent()
            && !parent.as_os_str().is_empty()
        {
            std::fs::create_dir_all(parent)?;
        }
        config.save_to(path)?;
        Ok(())
    }

    pub fn save(&self) -> anyhow::Result<()> {
        let snapshot = self.snapshot_for_save();
        self.write_snapshot_to(snapshot, &crate::core::config::config_file())
    }

    pub fn mark_dirty(&self) {
        self.dirty.store(true, Ordering::Release);
        let mut after = self.save_after.lock().unwrap();
        *after = Some(Instant::now() + SAVE_DEBOUNCE);
    }

    #[allow(dead_code)]
    pub fn save_if_due(&self) -> anyhow::Result<()> {
        if !self.dirty.load(Ordering::Acquire) {
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
            if let Err(e) = self.save() {
                // Retry after the debounce rather than on every frame.
                *self.save_after.lock().unwrap() = Some(Instant::now() + SAVE_DEBOUNCE);
                return Err(e);
            }
            self.dirty.store(false, Ordering::Release);
            let mut after = self.save_after.lock().unwrap();
            *after = None;
        }
        Ok(())
    }

    /// Runs a due debounced save on a background blocking thread so large configs
    /// never stall the UI thread.
    pub fn spawn_save_if_due(&self) {
        if !self.dirty.load(Ordering::Acquire) {
            return;
        }
        let should_save = {
            let after = self.save_after.lock().unwrap();
            match *after {
                Some(t) => Instant::now() >= t,
                None => true,
            }
        };
        if !should_save {
            return;
        }
        if self
            .save_in_flight
            .compare_exchange(false, true, Ordering::AcqRel, Ordering::Acquire)
            .is_err()
        {
            return;
        }

        // Clear dirty state before spawning so mutations made while the background
        // save runs will re-arm the debounce timer instead of being lost.
        self.dirty.store(false, Ordering::Release);
        *self.save_after.lock().unwrap() = None;

        struct ResetFlag(Arc<AtomicBool>);
        impl Drop for ResetFlag {
            fn drop(&mut self) {
                self.0.store(false, Ordering::Release);
            }
        }

        let manager = self.clone();
        let run_save = move || {
            let _guard = ResetFlag(Arc::clone(&manager.save_in_flight));
            let t0 = Instant::now();
            match manager.save() {
                Ok(()) => {
                    tracing::debug!("Saved config in {:?}", t0.elapsed());
                }
                Err(e) => {
                    manager.mark_dirty();
                    *manager.last_save_error.lock().unwrap() = Some(e.to_string());
                }
            }
        };

        if let Ok(handle) = tokio::runtime::Handle::try_current() {
            handle.spawn_blocking(run_save);
        } else {
            run_save();
        }
    }

    /// Returns the most recent background save error, if any.
    pub fn take_save_error(&self) -> Option<String> {
        self.last_save_error.lock().unwrap().take()
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

    #[allow(dead_code)]
    pub fn timezone_clocks(&self) -> Vec<crate::core::config::TimezoneClockConfig> {
        self.timezone_clocks.lock().unwrap().clone()
    }

    pub fn save_timezone_clocks(
        &self,
        clocks: Vec<crate::core::config::TimezoneClockConfig>,
    ) -> anyhow::Result<()> {
        {
            let mut tc = self.timezone_clocks.lock().unwrap();
            *tc = clocks;
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

    pub fn update_cluster(
        &self,
        old_name: &str,
        config: ClusterConfig,
        password: Option<&str>,
    ) -> anyhow::Result<()> {
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
            let previous = clients.remove(old_name);
            let client_result = match password {
                Some(pw) => EsClient::with_password(&config, pw),
                None => EsClient::new(&config),
            };
            match client_result {
                Ok(client) => {
                    let client = match &previous {
                        Some(prev) => client.adopt_tunnel(prev),
                        None => client,
                    };
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
        if old_name != config.name {
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
        let _ = crate::core::auth::delete_password(name);
        self.remove_cluster_data(name);
        self.mark_dirty();
        Ok(())
    }

    /// Flip a cluster's `enabled` flag and persist on the next debounced save.
    pub fn set_cluster_enabled(&self, name: &str, enabled: bool) -> bool {
        let mut changed = false;
        {
            let mut clusters = self.clusters.lock().unwrap();
            if let Some(cluster) = clusters.iter_mut().find(|c| c.name == name)
                && cluster.enabled != enabled
            {
                cluster.enabled = enabled;
                changed = true;
            }
        }
        if changed {
            self.mark_dirty();
        }
        changed
    }

    pub fn get_client(&self, name: &str) -> Option<EsClient> {
        self.clients.lock().unwrap().get(name).cloned()
    }

    /// Replaces the cluster's client outright, tunnel included: after "Test", the test's
    /// freshly opened tunnel becomes the one refreshes use.
    pub fn set_client(&self, name: &str, client: EsClient) {
        self.clients
            .lock()
            .unwrap()
            .insert(name.to_string(), client);
    }

    pub fn rebuild_client(&self, name: &str) {
        let t0 = Instant::now();
        tracing::info!("rebuild_client('{}'): starting", name);
        let config = {
            let clusters = self.clusters.lock().unwrap();
            clusters.iter().find(|c| c.name == name).cloned()
        };

        let previous = self.clients.lock().unwrap().get(name).cloned();
        let existing_password = previous.as_ref().map(|c| c.password().to_string());

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
                }
            };

            match result {
                Ok(client) => {
                    let client = match &previous {
                        Some(prev) => client.adopt_tunnel(prev),
                        None => client,
                    };
                    let mut clients = self.clients.lock().unwrap();
                    clients.insert(name.to_string(), client);
                    tracing::info!(
                        "Successfully rebuilt ES client for '{}' in {:?}",
                        name,
                        t0.elapsed()
                    );
                }
                Err(e) => {
                    tracing::warn!(
                        "Failed to rebuild ES client for '{}' after {:?}: {}",
                        name,
                        t0.elapsed(),
                        e
                    );
                }
            }
        }
    }

    /// Rebuilds a cluster's HTTP client on a background blocking thread, deduplicating
    /// concurrent rebuilds for the same cluster name.
    pub fn rebuild_client_async(&self, name: String) {
        {
            let mut in_flight = self.rebuilding_clients.lock().unwrap();
            if !in_flight.insert(name.clone()) {
                return;
            }
        }

        struct RebuildGuard {
            set: Arc<Mutex<HashSet<String>>>,
            name: String,
        }
        impl Drop for RebuildGuard {
            fn drop(&mut self) {
                self.set.lock().unwrap().remove(&self.name);
            }
        }

        let manager = self.clone();
        let run = move || {
            let _guard = RebuildGuard {
                set: Arc::clone(&manager.rebuilding_clients),
                name: name.clone(),
            };
            manager.rebuild_client(&name);
        };

        if let Ok(handle) = tokio::runtime::Handle::try_current() {
            handle.spawn_blocking(run);
        } else {
            run();
        }
    }

    pub fn auto_refresh(&self) -> bool {
        *self.auto_refresh.lock().unwrap()
    }

    pub fn set_auto_refresh(&self, value: bool) {
        *self.auto_refresh.lock().unwrap() = value;
        self.mark_dirty();
    }

    pub fn cluster_filter(&self) -> String {
        self.cluster_filter.lock().unwrap().clone()
    }

    pub fn set_cluster_filter(&self, value: String) {
        *self.cluster_filter.lock().unwrap() = value;
        self.mark_dirty();
    }

    pub fn pipeline_test_presets(&self) -> Vec<crate::core::config::PipelineTestPreset> {
        self.pipeline_test_presets.lock().unwrap().clone()
    }

    pub fn save_pipeline_test_presets(
        &self,
        presets: Vec<crate::core::config::PipelineTestPreset>,
    ) -> anyhow::Result<()> {
        {
            let mut p = self.pipeline_test_presets.lock().unwrap();
            *p = presets;
        }
        self.mark_dirty();
        Ok(())
    }

    pub fn god_console_saved_queries(&self) -> Vec<crate::core::config::GodSavedQuery> {
        self.god_console_saved_queries.lock().unwrap().clone()
    }

    pub fn save_god_console_saved_queries(&self, queries: Vec<crate::core::config::GodSavedQuery>) {
        *self.god_console_saved_queries.lock().unwrap() = queries;
        self.mark_dirty();
    }

    #[allow(dead_code)]
    pub fn llm_settings(&self) -> crate::core::config::LlmSettings {
        self.llm.lock().unwrap().clone()
    }

    pub fn save_llm_settings(
        &self,
        settings: crate::core::config::LlmSettings,
    ) -> anyhow::Result<()> {
        {
            let mut l = self.llm.lock().unwrap();
            *l = settings;
        }
        self.mark_dirty();
        Ok(())
    }

    #[allow(dead_code)]
    pub fn assistant_dock_visible(&self) -> bool {
        *self.assistant_dock_visible.lock().unwrap()
    }

    pub fn set_assistant_dock_visible(&self, value: bool) -> anyhow::Result<()> {
        *self.assistant_dock_visible.lock().unwrap() = value;
        self.mark_dirty();
        Ok(())
    }

    #[allow(dead_code)]
    pub fn assistant_conversations(&self) -> Vec<crate::core::config::ChatConversation> {
        self.assistant_conversations.lock().unwrap().clone()
    }

    pub fn save_assistant_conversations(
        &self,
        convs: Vec<crate::core::config::ChatConversation>,
        active_id: Option<String>,
    ) -> anyhow::Result<()> {
        {
            let mut c = self.assistant_conversations.lock().unwrap();
            *c = convs;
        }
        {
            let mut id = self.assistant_active_conversation_id.lock().unwrap();
            *id = active_id;
        }
        self.mark_dirty();
        Ok(())
    }

    pub fn refresh_interval_secs(&self) -> u64 {
        *self.refresh_interval_secs.lock().unwrap()
    }

    pub fn set_refresh_interval_secs(&self, value: u64) {
        *self.refresh_interval_secs.lock().unwrap() = value;
        self.mark_dirty();
    }

    /// A cluster's Console variables, without cloning the rest of its cached data.
    pub fn cluster_variables(&self, name: &str) -> Vec<(String, String)> {
        self.cluster_data
            .lock()
            .unwrap()
            .get(name)
            .map(|d| d.variables.clone())
            .unwrap_or_default()
    }

    /// Borrows a cluster's cached data under the lock without cloning it or marking dirty.
    pub fn read_cluster_data<R>(
        &self,
        name: &str,
        f: impl FnOnce(&crate::core::config::ClusterData) -> R,
    ) -> Option<R> {
        let data = self.cluster_data.lock().unwrap();
        data.get(name).map(f)
    }

    /// Mutates a cluster's cached data in place, creating a default entry if absent,
    /// and schedules a debounced config save after releasing the lock.
    pub fn mutate_cluster_data<R>(
        &self,
        name: &str,
        f: impl FnOnce(&mut crate::core::config::ClusterData) -> R,
    ) -> R {
        let out = {
            let mut data = self.cluster_data.lock().unwrap();
            let entry = data.entry(name.to_string()).or_default();
            f(entry)
        };
        self.mark_dirty();
        out
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
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::core::config::{AppConfig, ClusterData, SavedQuery};
    use std::path::PathBuf;
    use std::sync::mpsc;

    fn temp_config_path(name: &str) -> (PathBuf, PathBuf) {
        let dir = std::env::temp_dir().join(format!(
            "drastic-smurf-cm-test-{}-{}",
            name,
            std::process::id()
        ));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("config.json");
        (dir, path)
    }

    fn test_cluster(name: &str, enabled: bool) -> ClusterConfig {
        ClusterConfig {
            enabled,
            ..ClusterConfig::new(name, "http://127.0.0.1:9200", "elastic")
        }
    }

    #[test]
    fn set_cluster_enabled_toggles_and_marks_dirty() {
        let manager = ClusterManager::new();
        manager
            .add_cluster(test_cluster("alpha", false), Some("pw"))
            .unwrap();
        manager.dirty.store(false, Ordering::Release);

        assert!(!manager.set_cluster_enabled("alpha", false));
        assert!(!manager.dirty.load(Ordering::Acquire));

        assert!(manager.set_cluster_enabled("alpha", true));
        assert!(manager.dirty.load(Ordering::Acquire));
        assert!(manager.clusters()[0].enabled);

        manager.dirty.store(false, Ordering::Release);
        assert!(manager.set_cluster_enabled("alpha", false));
        assert!(manager.dirty.load(Ordering::Acquire));
        assert!(!manager.clusters()[0].enabled);

        assert!(!manager.set_cluster_enabled("missing", true));
    }

    #[test]
    fn mutate_and_read_cluster_data_in_place() {
        let manager = ClusterManager::new();
        assert!(manager.read_cluster_data("beta", |_| 1usize).is_none());

        manager.mutate_cluster_data("beta", |data| {
            data.variables.push(("env".into(), "staging".into()));
            data.saved_queries.push(SavedQuery {
                name: "health".into(),
                method: "GET".into(),
                path: "/_cluster/health".into(),
                body: None,
            });
        });
        assert!(manager.dirty.load(Ordering::Acquire));

        manager.dirty.store(false, Ordering::Release);
        let summary = manager.read_cluster_data("beta", |data| {
            (data.variables.clone(), data.saved_queries.len())
        });
        assert_eq!(
            summary,
            Some((vec![("env".into(), "staging".into())], 1usize))
        );
        assert!(!manager.dirty.load(Ordering::Acquire));

        // Mutating one field preserves sibling fields on the same entry.
        manager.mutate_cluster_data("beta", |data: &mut ClusterData| {
            data.variables.push(("region".into(), "apac".into()));
        });
        let counts = manager.read_cluster_data("beta", |data| {
            (data.variables.len(), data.saved_queries.len())
        });
        assert_eq!(counts, Some((2, 1)));
    }

    #[test]
    fn write_snapshot_does_not_block_cluster_reads_or_mutations() {
        let (dir, path) = temp_config_path("io-nonblocking");
        let manager = ClusterManager::new();
        manager
            .add_cluster(test_cluster("gamma", false), Some("pw"))
            .unwrap();
        manager.mutate_cluster_data("gamma", |data| {
            data.variables.push(("k".into(), "v".into()));
        });

        let snapshot = manager.snapshot_for_save();

        // Hold `save_io_lock` on a background thread to simulate slow disk I/O.
        let io_lock = Arc::clone(&manager.save_io_lock);
        let (locked_tx, locked_rx) = mpsc::channel();
        let (release_tx, release_rx) = mpsc::channel();
        let holder = std::thread::spawn(move || {
            let _guard = io_lock.lock().unwrap();
            let _ = locked_tx.send(());
            let _ = release_rx.recv();
        });
        locked_rx.recv().unwrap();

        // Reading, toggling enabled, and mutating cluster_data must not block on `save_io_lock`.
        assert_eq!(manager.clusters().len(), 1);
        assert!(manager.set_cluster_enabled("gamma", true));
        manager.mutate_cluster_data("gamma", |data| {
            data.variables.push(("k2".into(), "v2".into()));
        });

        let _ = release_tx.send(());
        holder.join().unwrap();

        // Verify `write_snapshot_to` round-trips cleanly without touching the real config dir.
        manager.write_snapshot_to(snapshot, &path).unwrap();
        let loaded = AppConfig::load_from(&path).unwrap();
        assert_eq!(loaded.clusters.len(), 1);
        assert_eq!(loaded.clusters[0].name, "gamma");
        assert!(!loaded.clusters[0].enabled);
        assert_eq!(loaded.cluster_data["gamma"].variables.len(), 1);

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn rebuild_client_async_deduplicates_and_clears_guard() {
        let manager = ClusterManager::new();
        manager
            .add_cluster(test_cluster("delta", true), Some("secret-pw"))
            .unwrap();

        // Pre-seeding `rebuilding_clients` causes `rebuild_client_async` to return immediately
        // without touching or clearing the existing marker.
        manager
            .rebuilding_clients
            .lock()
            .unwrap()
            .insert("delta".into());
        manager.rebuild_client_async("delta".into());
        assert!(manager.rebuilding_clients.lock().unwrap().contains("delta"));

        // Once the marker is cleared, `rebuild_client_async` executes and removes its guard.
        manager.rebuilding_clients.lock().unwrap().clear();
        manager.rebuild_client_async("delta".into());
        assert!(manager.rebuilding_clients.lock().unwrap().is_empty());
        assert_eq!(manager.get_client("delta").unwrap().password(), "secret-pw");
    }
}
