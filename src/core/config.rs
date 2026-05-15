use std::path::PathBuf;

use anyhow::Result;
use directories::ProjectDirs;
use serde::{Deserialize, Serialize};

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
    #[serde(default, deserialize_with = "string_or_vec")]
    pub snapshot_repos: Vec<String>,
    #[serde(default, deserialize_with = "string_or_vec")]
    pub slm_policies: Vec<String>,
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
    pub kibana_host: String,
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
            snapshot_repos: Vec::new(),
            slm_policies: Vec::new(),
            verify_ssl: true,
            ca_cert: CaCert::default(),
            ssh_tunnel: false,
            ssh_host: String::new(),
            ssh_user: String::new(),
            ssh_port: 22,
            kibana_host: String::new(),
        }
    }
}

impl ClusterConfig {
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

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SavedCommand {
    pub name: String,
    pub target: String, // "es" or "kibana"
    pub method: String,
    pub path: String,
    pub body: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct AppConfig {
    pub clusters: Vec<ClusterConfig>,
    #[serde(default)]
    pub auto_refresh: bool,
    #[serde(default = "default_refresh_interval_secs")]
    pub refresh_interval_secs: u64,
    #[serde(default)]
    pub saved_commands: Vec<SavedCommand>,
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
    pub fn default_commands() -> Vec<SavedCommand> {
        vec![
            SavedCommand {
                name: "Cluster Health".to_string(),
                target: "es".to_string(),
                method: "GET".to_string(),
                path: "/_cluster/health".to_string(),
                body: String::new(),
            },
            SavedCommand {
                name: "Node Stats".to_string(),
                target: "es".to_string(),
                method: "GET".to_string(),
                path: "/_nodes/stats".to_string(),
                body: String::new(),
            },
            SavedCommand {
                name: "Index List".to_string(),
                target: "es".to_string(),
                method: "GET".to_string(),
                path: "/_cat/indices?v".to_string(),
                body: String::new(),
            },
            SavedCommand {
                name: "Snapshot Status".to_string(),
                target: "es".to_string(),
                method: "GET".to_string(),
                path: "/_snapshot/_status".to_string(),
                body: String::new(),
            },
            SavedCommand {
                name: "Kibana Status".to_string(),
                target: "kibana".to_string(),
                method: "GET".to_string(),
                path: "/api/status".to_string(),
                body: String::new(),
            },
            SavedCommand {
                name: "Kibana Spaces".to_string(),
                target: "kibana".to_string(),
                method: "GET".to_string(),
                path: "/api/spaces/space".to_string(),
                body: String::new(),
            },
        ]
    }

    pub fn load() -> Result<Self> {
        let path = config_file();
        if !path.exists() {
            let mut defaults = Self::default();
            defaults.saved_commands = Self::default_commands();
            return Ok(defaults);
        }
        let contents = std::fs::read_to_string(&path)?;
        let mut config: Self = serde_json::from_str(&contents)?;
        if config.saved_commands.is_empty() {
            config.saved_commands = Self::default_commands();
        }
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


fn string_or_vec<'de, D>(deserializer: D) -> Result<Vec<String>, D::Error>
where
    D: serde::Deserializer<'de>,
{
    struct StringOrVec;

    impl<'de> serde::de::Visitor<'de> for StringOrVec {
        type Value = Vec<String>;

        fn expecting(&self, formatter: &mut std::fmt::Formatter) -> std::fmt::Result {
            formatter.write_str("string or list of strings")
        }

        fn visit_str<E>(self, value: &str) -> Result<Vec<String>, E>
        where
            E: serde::de::Error,
        {
            Ok(vec![value.to_owned()])
        }

        fn visit_string<E>(self, value: String) -> Result<Vec<String>, E>
        where
            E: serde::de::Error,
        {
            Ok(vec![value])
        }

        fn visit_seq<S>(self, visitor: S) -> Result<Vec<String>, S::Error>
        where
            S: serde::de::SeqAccess<'de>,
        {
            serde::Deserialize::deserialize(serde::de::value::SeqAccessDeserializer::new(visitor))
        }
    }

    deserializer.deserialize_any(StringOrVec)
}
