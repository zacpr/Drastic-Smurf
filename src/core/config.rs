use std::collections::HashMap;
use std::path::PathBuf;

use anyhow::Result;
use directories::ProjectDirs;
use serde::{Deserialize, Serialize};

use crate::ui::theme::AppTheme;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VfxSettings {
    #[serde(default = "default_background_effect")]
    pub background_effect: BackgroundEffect,
    #[serde(default = "default_background_intensity")]
    pub background_intensity: f32,
    #[serde(default = "default_animation_speed")]
    pub animation_speed: f32,
    #[serde(default = "default_true")]
    pub hover_effects: bool,
    #[serde(default = "default_true")]
    pub shimmer_effects: bool,
    #[serde(default = "default_false")]
    pub cursor_glow: bool,
    #[serde(default = "default_parallax")]
    pub parallax_amount: f32,
    #[serde(default = "default_false")]
    pub reduce_motion: bool,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
pub enum BackgroundEffect {
    #[default]
    None,
    Gradient,
    Mesh,
}

impl Default for VfxSettings {
    fn default() -> Self {
        Self {
            background_effect: BackgroundEffect::Gradient,
            background_intensity: 0.15,
            animation_speed: 1.0,
            hover_effects: true,
            shimmer_effects: true,
            cursor_glow: false,
            parallax_amount: 0.2,
            reduce_motion: false,
        }
    }
}

fn default_background_effect() -> BackgroundEffect {
    BackgroundEffect::Gradient
}
fn default_background_intensity() -> f32 {
    0.15
}
fn default_animation_speed() -> f32 {
    1.0
}
fn default_true() -> bool {
    true
}
fn default_false() -> bool {
    false
}
fn default_parallax() -> f32 {
    0.2
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Hash)]
pub enum CaCert {
    System,
    Bundled,
    Custom(PathBuf),
}

impl Default for CaCert {
    fn default() -> Self {
        CaCert::System
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ClusterConfig {
    pub name: String,
    pub host: String,
    pub username: String,
    pub snapshot_repo: String,
    pub slm_policy: String,
    #[serde(default)]
    pub kibana_host: String,
    #[serde(default)]
    pub haproxy_host: String,
    #[serde(default)]
    pub custom_links: Vec<(String, String)>,
    #[serde(default)]
    pub ca_cert_pem: String,
    #[serde(default = "default_verify_ssl")]
    pub verify_ssl: bool,
    #[serde(default)]
    pub ca_cert: CaCert,
    #[serde(default)]
    pub ssh_tunnel: bool,
    pub ssh_host: String,
    pub ssh_user: String,
    #[serde(default = "default_ssh_port")]
    pub ssh_port: u16,
}

fn default_verify_ssl() -> bool {
    true
}

fn default_ssh_port() -> u16 {
    22
}

impl Default for ClusterConfig {
    fn default() -> Self {
        Self {
            name: String::new(),
            host: String::new(),
            username: String::new(),
            snapshot_repo: String::new(),
            slm_policy: String::new(),
            kibana_host: String::new(),
            haproxy_host: String::new(),
            custom_links: Vec::new(),
            ca_cert_pem: String::new(),
            verify_ssl: true,
            ca_cert: CaCert::default(),
            ssh_tunnel: false,
            ssh_host: String::new(),
            ssh_user: String::new(),
            ssh_port: 22,
        }
    }
}

impl ClusterConfig {
    #[allow(dead_code)]
    pub fn new(
        name: impl Into<String>,
        host: impl Into<String>,
        username: impl Into<String>,
    ) -> Self {
        Self {
            name: name.into(),
            host: host.into(),
            username: username.into(),
            ..Default::default()
        }
    }
}

// --- Per-cluster cached module data ---

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct SavedQuery {
    pub name: String,
    pub method: String,
    pub path: String,
    pub body: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct StatusSnapshot {
    pub timestamp: chrono::DateTime<chrono::Utc>,
    pub health: Option<crate::core::es_client::ClusterHealth>,
    pub stats: Option<crate::core::es_client::ClusterStats>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct TaskCacheEntry {
    pub timestamp: chrono::DateTime<chrono::Utc>,
    pub tasks: Vec<crate::core::es_client::TaskInfo>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct SnapshotCacheEntry {
    pub timestamp: chrono::DateTime<chrono::Utc>,
    pub reachable: bool,
    pub error_message: Option<String>,
    pub snapshot_info: Option<crate::core::es_client::SnapshotInfo>,
    pub snapshot_stats: Option<crate::modules::snapshot::SnapshotStats>,
    pub slm_last_run: Option<String>,
    pub slm_next_run: Option<String>,
    pub slm_in_progress: bool,
    #[serde(default)]
    pub slm_policies: Vec<(String, crate::core::es_client::SlmPolicyDetail)>,
    #[serde(default)]
    pub has_repositories: bool,
    #[serde(default)]
    pub resolved_repo: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct ClusterData {
    pub saved_queries: Vec<SavedQuery>,
    pub status_history: Vec<StatusSnapshot>,
    pub tasks_cache: Vec<TaskCacheEntry>,
    pub snapshot_cache: Vec<SnapshotCacheEntry>,
    #[serde(default)]
    pub variables: Vec<(String, String)>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TimezoneClockConfig {
    pub label: String,
    pub zone: String, // "Local", "UTC", "Sydney", "Germany", "Chicago" or custom UTC offset
    pub enabled: bool,
}

pub fn default_timezone_clocks() -> Vec<TimezoneClockConfig> {
    vec![
        TimezoneClockConfig {
            label: "Local".to_string(),
            zone: "Local".to_string(),
            enabled: true,
        },
        TimezoneClockConfig {
            label: "UTC".to_string(),
            zone: "UTC".to_string(),
            enabled: true,
        },
        TimezoneClockConfig {
            label: "Sydney (APAC)".to_string(),
            zone: "Sydney".to_string(),
            enabled: true,
        },
        TimezoneClockConfig {
            label: "EMEA (Germany)".to_string(),
            zone: "Germany".to_string(),
            enabled: true,
        },
        TimezoneClockConfig {
            label: "AMER (Chicago)".to_string(),
            zone: "Chicago".to_string(),
            enabled: true,
        },
    ]
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PinnedMonitorLayout {
    pub x: f32,
    pub y: f32,
    pub w: f32,
    pub h: f32,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct AppConfig {
    pub clusters: Vec<ClusterConfig>,
    #[serde(default)]
    pub cluster_data: HashMap<String, ClusterData>,
    #[serde(default)]
    pub auto_refresh: bool,
    #[serde(default = "default_refresh_interval_secs")]
    pub refresh_interval_secs: u64,
    #[serde(default)]
    pub theme: AppTheme,
    #[serde(default)]
    pub vfx: VfxSettings,
    #[serde(default = "default_timezone_clocks")]
    pub timezone_clocks: Vec<TimezoneClockConfig>,
    #[serde(default)]
    pub cluster_filter: String,
    #[serde(default)]
    pub window_width: Option<f32>,
    #[serde(default)]
    pub window_height: Option<f32>,
    #[serde(default)]
    pub window_pos_x: Option<f32>,
    #[serde(default)]
    pub window_pos_y: Option<f32>,
    #[serde(default)]
    pub wizard_completed: bool,
    #[serde(default)]
    pub pinned_monitor_ids: Vec<String>,
    #[serde(default)]
    pub pinned_monitor_layouts: HashMap<String, PinnedMonitorLayout>,
}

fn default_refresh_interval_secs() -> u64 {
    15
}

pub fn config_dir() -> PathBuf {
    ProjectDirs::from("com", "drastic-smurf", "DrasticSmurf")
        .map(|d| d.config_dir().to_path_buf())
        .unwrap_or_else(|| PathBuf::from(".config/drastic-smurf"))
}

pub fn config_file() -> PathBuf {
    config_dir().join("config.json")
}

impl AppConfig {
    pub fn load() -> Result<Self> {
        let path = config_file();
        if !path.exists() {
            return Ok(Self::default());
        }
        let contents = std::fs::read_to_string(&path)?;
        let config = serde_json::from_str(&contents)?;
        Ok(config)
    }

    pub fn save(&self) -> Result<()> {
        let dir = config_dir();
        std::fs::create_dir_all(&dir)?;
        let path = config_file();
        let contents = serde_json::to_string_pretty(self)?;
        std::fs::write(&path, contents)?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_load_real_config() {
        match AppConfig::load() {
            Ok(cfg) => {
                println!("SUCCESSFULLY LOADED! wizard_completed: {}", cfg.wizard_completed);
            }
            Err(e) => {
                panic!("ERROR LOADING REAL CONFIG: {:?}", e);
            }
        }
    }
}
