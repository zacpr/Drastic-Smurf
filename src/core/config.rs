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
    #[serde(default = "default_true")]
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
    Particles,
}

impl Default for VfxSettings {
    fn default() -> Self {
        Self {
            background_effect: BackgroundEffect::None,
            background_intensity: 0.0,
            animation_speed: 0.0,
            hover_effects: true,
            shimmer_effects: true,
            cursor_glow: true,
            parallax_amount: 0.0,
            reduce_motion: false,
        }
    }
}

fn default_background_effect() -> BackgroundEffect {
    BackgroundEffect::None
}
fn default_background_intensity() -> f32 {
    0.0
}
fn default_animation_speed() -> f32 {
    0.0
}
fn default_true() -> bool {
    true
}
fn default_false() -> bool {
    false
}
fn default_parallax() -> f32 {
    0.0
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Hash, Default)]
pub enum PipelineTargetKind {
    #[default]
    Index,
    DataStream,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Hash, Default)]
pub enum PipelineMode {
    #[default]
    Default,
    Loaded,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct PipelineTestPreset {
    pub name: String,
    pub cluster: String,
    pub target_kind: PipelineTargetKind,
    pub target_name: String,
    pub pipeline_mode: PipelineMode,
    pub pipeline_id: String,
    pub pipeline_text: String,
    pub docs_text: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Hash, Default)]
pub enum CaCert {
    #[default]
    System,
    Bundled,
    Custom(PathBuf),
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum LlmProviderKind {
    /// Any OpenAI-compatible chat completions endpoint
    /// (OpenAI, Azure OpenAI, Ollama, LM Studio, vLLM, OpenRouter, etc.)
    OpenAiCompatible,
    /// GitHub Copilot Chat - OpenAI-compatible but needs GitHub token + Copilot headers
    GitHubCopilot,
}

impl Default for LlmProviderKind {
    fn default() -> Self {
        LlmProviderKind::OpenAiCompatible
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LlmProviderPreset {
    pub id: String,
    pub label: String,
    pub kind: LlmProviderKind,
    pub default_base_url: String,
    pub default_model: String,
    pub models: Vec<String>,
    pub api_key_hint: String,
    pub docs_url: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct LlmSettings {
    #[serde(default)]
    pub enabled: bool,
    #[serde(default)]
    pub provider_id: String,
    #[serde(default)]
    pub base_url: String,
    #[serde(default = "default_llm_model")]
    pub model: String,
    #[serde(default = "default_llm_temperature")]
    pub temperature: f32,
    /// Don't send `temperature`; the model uses its own default. Needed for models
    /// that reject any other value (e.g. OpenAI reasoning models).
    #[serde(default)]
    pub use_default_temperature: bool,
    #[serde(default = "default_llm_max_context_chars")]
    pub max_context_chars: usize,
    #[serde(default = "default_true")]
    pub auto_cluster_context: bool,
    #[serde(default)]
    pub extra_headers: Vec<(String, String)>,
    /// Let the assistant act in the app through tool calls (look up cluster data,
    /// load requests into the Console, switch tabs, run approved requests).
    #[serde(default = "default_true")]
    pub tools_enabled: bool,
    /// Run read-only (GET/HEAD) tool requests without asking. Writes always ask.
    #[serde(default = "default_true")]
    pub auto_run_reads: bool,
}

fn default_llm_model() -> String {
    "gpt-4o-mini".to_string()
}
fn default_llm_temperature() -> f32 {
    0.3
}
fn default_llm_max_context_chars() -> usize {
    4000
}

impl Default for LlmSettings {
    fn default() -> Self {
        Self {
            enabled: false,
            provider_id: "openai".to_string(),
            base_url: "https://api.openai.com/v1".to_string(),
            model: default_llm_model(),
            temperature: default_llm_temperature(),
            use_default_temperature: false,
            max_context_chars: default_llm_max_context_chars(),
            auto_cluster_context: true,
            extra_headers: Vec::new(),
            tools_enabled: true,
            auto_run_reads: true,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LogSettings {
    pub index_pattern: String,
    pub timestamp_field: String,
    pub message_field: String,
    pub app_field: String,
    pub hostname_field: String,
    pub severity_field: String,
    pub limit: usize,
}

impl Default for LogSettings {
    fn default() -> Self {
        Self {
            index_pattern: "logs-elasticsearch-*,logs-kibana-*".to_string(),
            timestamp_field: "@timestamp".to_string(),
            message_field: "message".to_string(),
            app_field: "service.name".to_string(),
            hostname_field: "host.name".to_string(),
            severity_field: "log.level".to_string(),
            limit: 1000,
        }
    }
}

pub fn default_llm_providers() -> Vec<LlmProviderPreset> {
    vec![
        LlmProviderPreset {
            id: "openai".to_string(),
            label: "OpenAI".to_string(),
            kind: LlmProviderKind::OpenAiCompatible,
            default_base_url: "https://api.openai.com/v1".to_string(),
            default_model: "gpt-4o-mini".to_string(),
            models: vec![
                "gpt-4o".to_string(),
                "gpt-4o-mini".to_string(),
                "gpt-4-turbo".to_string(),
                "o3-mini".to_string(),
                "o1".to_string(),
            ],
            api_key_hint: "sk-...".to_string(),
            docs_url: "https://platform.openai.com/api-keys".to_string(),
        },
        LlmProviderPreset {
            id: "github_copilot".to_string(),
            label: "GitHub Copilot".to_string(),
            kind: LlmProviderKind::GitHubCopilot,
            default_base_url: "https://api.githubcopilot.com".to_string(),
            default_model: "gpt-4o".to_string(),
            models: vec![
                "gpt-4o".to_string(),
                "gpt-4o-mini".to_string(),
                "o1-preview".to_string(),
                "claude-3.5-sonnet".to_string(),
                "gemini-2.0-flash".to_string(),
            ],
            api_key_hint: "ghu_... (GitHub PAT with 'Copilot Chat' scope)".to_string(),
            docs_url: "https://github.com/settings/personal-access-tokens".to_string(),
        },
        LlmProviderPreset {
            id: "azure_openai".to_string(),
            label: "Azure OpenAI".to_string(),
            kind: LlmProviderKind::OpenAiCompatible,
            default_base_url:
                "https://YOUR_RESOURCE.openai.azure.com/openai/deployments/YOUR_DEPLOYMENT"
                    .to_string(),
            default_model: "gpt-4o".to_string(),
            models: vec![
                "gpt-4o".to_string(),
                "gpt-4o-mini".to_string(),
                "gpt-4-turbo".to_string(),
            ],
            api_key_hint: "Azure OpenAI API key (set api-key header in extra headers)".to_string(),
            docs_url: "https://oai.azure.com/portal/".to_string(),
        },
        LlmProviderPreset {
            id: "openrouter".to_string(),
            label: "OpenRouter".to_string(),
            kind: LlmProviderKind::OpenAiCompatible,
            default_base_url: "https://openrouter.ai/api/v1".to_string(),
            default_model: "openai/gpt-4o-mini".to_string(),
            models: vec![
                "openai/gpt-4o-mini".to_string(),
                "anthropic/claude-3.5-sonnet".to_string(),
                "google/gemini-2.0-flash-exp:free".to_string(),
                "meta-llama/llama-3.3-70b-instruct:free".to_string(),
            ],
            api_key_hint: "sk-or-...".to_string(),
            docs_url: "https://openrouter.ai/keys".to_string(),
        },
        LlmProviderPreset {
            id: "ollama".to_string(),
            label: "Ollama (local)".to_string(),
            kind: LlmProviderKind::OpenAiCompatible,
            default_base_url: "http://localhost:11434/v1".to_string(),
            default_model: "llama3.2".to_string(),
            models: vec![
                "llama3.2".to_string(),
                "llama3.1".to_string(),
                "qwen2.5".to_string(),
                "mistral".to_string(),
                "codellama".to_string(),
                "deepseek-coder-v2".to_string(),
            ],
            api_key_hint: "(any string - ignored by Ollama)".to_string(),
            docs_url: "https://ollama.com/".to_string(),
        },
        LlmProviderPreset {
            id: "lmstudio".to_string(),
            label: "LM Studio (local)".to_string(),
            kind: LlmProviderKind::OpenAiCompatible,
            default_base_url: "http://localhost:1234/v1".to_string(),
            default_model: "local-model".to_string(),
            models: vec!["local-model".to_string()],
            api_key_hint: "(any string - ignored by LM Studio)".to_string(),
            docs_url: "https://lmstudio.ai/".to_string(),
        },
        LlmProviderPreset {
            id: "vllm".to_string(),
            label: "vLLM (self-hosted)".to_string(),
            kind: LlmProviderKind::OpenAiCompatible,
            default_base_url: "http://localhost:8000/v1".to_string(),
            default_model: "meta-llama/Llama-3-8B-Instruct".to_string(),
            models: vec!["meta-llama/Llama-3-8B-Instruct".to_string()],
            api_key_hint: "(empty if no auth configured)".to_string(),
            docs_url: "https://docs.vllm.ai/".to_string(),
        },
        LlmProviderPreset {
            id: "custom".to_string(),
            label: "Custom OpenAI-compatible endpoint".to_string(),
            kind: LlmProviderKind::OpenAiCompatible,
            default_base_url: "https://your-endpoint.example.com/v1".to_string(),
            default_model: "model-name".to_string(),
            models: vec![],
            api_key_hint: "API key (or empty)".to_string(),
            docs_url: "".to_string(),
        },
    ]
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct ChatMessage {
    pub role: String,
    pub content: String,
    pub timestamp: chrono::DateTime<chrono::Utc>,
    /// Reasoning ("thinking") a reasoning model streamed before its answer. Shown in
    /// the chat, never sent back to the model, and not persisted.
    #[serde(skip)]
    pub reasoning: String,
    /// Tools an `assistant` message called.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub tool_calls: Vec<crate::core::llm::ToolCall>,
    /// For a `tool` message: the call it answers.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tool_call_id: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct ChatConversation {
    pub id: String,
    pub title: String,
    pub messages: Vec<ChatMessage>,
    pub created_at: chrono::DateTime<chrono::Utc>,
    pub updated_at: chrono::DateTime<chrono::Utc>,
}

/// How a cluster authenticates. For `ApiKey` the secret kept in the keyring is the
/// Elasticsearch API key (the base64 `encoded` value, or `id:api_key`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum AuthMethod {
    #[default]
    #[serde(alias = "Basic")]
    Basic,
    #[serde(alias = "ApiKey")]
    ApiKey,
}

impl AuthMethod {
    pub fn label(&self) -> &'static str {
        match self {
            AuthMethod::Basic => "Username / Password",
            AuthMethod::ApiKey => "API Key",
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ClusterConfig {
    pub name: String,
    pub host: String,
    pub username: String,
    #[serde(default)]
    pub auth_method: AuthMethod,
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
            auth_method: AuthMethod::Basic,
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

/// A saved God-mode Console request: multi-cluster targets plus loop values.
/// Stored globally (not per cluster) because it spans clusters. Never holds credentials.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct GodSavedQuery {
    pub name: String,
    pub method: String,
    pub path: String,
    #[serde(default)]
    pub body: Option<String>,
    #[serde(default)]
    pub use_kibana: bool,
    #[serde(default)]
    pub loop_items: String,
    /// JSON path extracted from each response into the Extracted values list.
    #[serde(default)]
    pub extract_path: String,
    #[serde(default)]
    pub clusters: Vec<String>,
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
    #[serde(default)]
    pub backups: Vec<crate::modules::snapshot::BackupStatus>,
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
            label: "APAC (Sydney)".to_string(),
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
    #[serde(default)]
    pub pipeline_test_presets: Vec<PipelineTestPreset>,
    #[serde(default)]
    pub god_console_saved_queries: Vec<GodSavedQuery>,
    #[serde(default)]
    pub llm: LlmSettings,
    #[serde(default)]
    pub assistant_dock_visible: bool,
    #[serde(default)]
    pub assistant_conversations: Vec<ChatConversation>,
    #[serde(default)]
    pub assistant_active_conversation_id: Option<String>,
    #[serde(default)]
    pub logs: LogSettings,
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
        let mut config: Self = serde_json::from_str(&contents)?;
        config.migrate();
        Ok(config)
    }

    /// Bring older saved configs in line with renamed defaults, leaving anything the
    /// user customised alone.
    fn migrate(&mut self) {
        for clock in &mut self.timezone_clocks {
            // The Sydney clock was renamed to match "EMEA (Germany)" / "AMER (Chicago)".
            if clock.label == "Sydney (APAC)" && clock.zone == "Sydney" {
                clock.label = "APAC (Sydney)".to_string();
            }
        }
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
                println!(
                    "SUCCESSFULLY LOADED! wizard_completed: {}",
                    cfg.wizard_completed
                );
            }
            Err(e) => {
                panic!("ERROR LOADING REAL CONFIG: {:?}", e);
            }
        }
    }

    #[test]
    fn test_migrate_renames_default_sydney_clock_only() {
        let mut config = AppConfig {
            timezone_clocks: vec![
                TimezoneClockConfig {
                    label: "Sydney (APAC)".to_string(),
                    zone: "Sydney".to_string(),
                    enabled: true,
                },
                TimezoneClockConfig {
                    label: "Sydney (APAC)".to_string(),
                    zone: "UTC+10".to_string(),
                    enabled: true,
                },
            ],
            ..Default::default()
        };
        config.migrate();
        assert_eq!(config.timezone_clocks[0].label, "APAC (Sydney)");
        assert_eq!(config.timezone_clocks[1].label, "Sydney (APAC)");
    }

    #[test]
    fn auth_method_accepts_local_and_released_config_values() {
        for value in ["basic", "Basic"] {
            let method: AuthMethod = serde_json::from_value(serde_json::json!(value)).unwrap();
            assert_eq!(method, AuthMethod::Basic);
            assert_eq!(serde_json::to_value(method).unwrap(), "basic");
        }
        for value in ["api_key", "ApiKey"] {
            let method: AuthMethod = serde_json::from_value(serde_json::json!(value)).unwrap();
            assert_eq!(method, AuthMethod::ApiKey);
            assert_eq!(serde_json::to_value(method).unwrap(), "api_key");
        }
    }

    #[test]
    fn test_app_config_roundtrip() {
        let mut config = AppConfig::default();
        config.clusters.push(ClusterConfig {
            name: "test-cluster".to_string(),
            host: "http://localhost:9200".to_string(),
            username: "admin".to_string(),
            auth_method: AuthMethod::ApiKey,
            snapshot_repo: "backup-repo".to_string(),
            slm_policy: "daily-snapshots".to_string(),
            kibana_host: "http://localhost:5601".to_string(),
            haproxy_host: "http://localhost:8080".to_string(),
            custom_links: vec![("ES Dashboard".to_string(), "http://kibana/dash".to_string())],
            ca_cert_pem: "PEM DATA".to_string(),
            verify_ssl: false,
            ca_cert: CaCert::System,
            ssh_tunnel: true,
            ssh_host: "10.0.0.1".to_string(),
            ssh_user: "ssh-user".to_string(),
            ssh_port: 2222,
        });
        config.cluster_filter = "test".to_string();
        config.wizard_completed = true;

        let serialized = serde_json::to_string(&config).unwrap();
        let deserialized: AppConfig = serde_json::from_str(&serialized).unwrap();

        assert_eq!(deserialized.clusters.len(), 1);
        let cluster = &deserialized.clusters[0];
        assert_eq!(cluster.name, "test-cluster");
        assert_eq!(cluster.host, "http://localhost:9200");
        assert_eq!(cluster.username, "admin");
        assert_eq!(cluster.auth_method, AuthMethod::ApiKey);
        assert_eq!(cluster.snapshot_repo, "backup-repo");
        assert_eq!(cluster.slm_policy, "daily-snapshots");
        assert_eq!(cluster.kibana_host, "http://localhost:5601");
        assert_eq!(cluster.haproxy_host, "http://localhost:8080");
        assert_eq!(
            cluster.custom_links,
            vec![("ES Dashboard".to_string(), "http://kibana/dash".to_string())]
        );
        assert_eq!(cluster.ca_cert_pem, "PEM DATA");
        assert_eq!(cluster.verify_ssl, false);
        assert_eq!(cluster.ca_cert, CaCert::System);
        assert_eq!(cluster.ssh_tunnel, true);
        assert_eq!(cluster.ssh_host, "10.0.0.1");
        assert_eq!(cluster.ssh_user, "ssh-user");
        assert_eq!(cluster.ssh_port, 2222);
        assert_eq!(deserialized.cluster_filter, "test");
        assert_eq!(deserialized.wizard_completed, true);
    }
}
