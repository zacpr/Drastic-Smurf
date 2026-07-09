//! Side dock "AI Assistant" panel for troubleshooting Elasticsearch and API usage.
//!
//! Streams responses from any OpenAI-compatible `/chat/completions` endpoint
//! (OpenAI, GitHub Copilot, Azure OpenAI, Ollama, LM Studio, OpenRouter, etc.).
//!
//! Also parses the assistant's output for actionable Elasticsearch API calls
//! (in JSON or HTTP-line form) and shows consent cards the user must approve
//! before the command is dispatched to the Console.

use std::sync::mpsc::{Receiver, Sender, channel};

use crate::core::auth;
use crate::core::config::{
    self, ChatConversation, ChatMessage, LlmProviderPreset, LlmSettings,
};
use crate::core::llm::{self, ChatMessage as LlmMessage};
use crate::ui::theme::Theme;
use eframe::egui;

/// A single, immutable snapshot of the cluster context we'll send the LLM.
#[derive(Debug, Clone, Default)]
pub struct ClusterContext {
    pub summary: String,
    pub cluster_name: String,
}

#[derive(Debug, Clone, Default)]
pub enum StreamMsg {
    /// Incremental token from the assistant.
    Delta(String),
    /// Streaming finished cleanly.
    #[default]
    Done,
    Error(String),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PanelMode {
    Chat,
    Settings,
}

/// Status of the most recent attempt to load the model list from
/// `<base_url>/models`. Surfaced as a small line under the Model selector.
#[derive(Debug, Clone, Default)]
pub enum ModelsStatus {
    #[default]
    Idle,
    Loading,
    Ok(usize),
    Error(String),
}

/// Result of an async models-list fetch.
#[derive(Debug, Clone)]
pub enum ModelsMsg {
    Ok(Vec<String>),
    Err(String),
}

/// A console command extracted from an assistant message that the user can
/// approve (or reject) before it is sent to the Console tab.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ConsoleAction {
    pub cluster: String,
    pub method: String,
    pub path: String,
    pub body: Option<String>,
}

pub struct AssistantState {
    pub settings: LlmSettings,
    pub api_key_input: String,
    pub api_key_set: bool,
    pub conversations: Vec<ChatConversation>,
    pub active_conversation_id: Option<String>,
    pub draft: String,
    pub mode: PanelMode,
    pub stream_rx: Option<Receiver<StreamMsg>>,
    pub is_streaming: bool,
    pub last_error: Option<String>,
    pub scroll_to_bottom: bool,
    pub last_saved_at: Option<chrono::DateTime<chrono::Utc>>,
    pub last_key_saved_at: Option<chrono::DateTime<chrono::Utc>>,
    pub close_requested: bool,
    // Models discovery
    pub fetched_models: Vec<String>,
    pub models_status: ModelsStatus,
    pub models_rx: Option<Receiver<ModelsMsg>>,
    /// Base URL that produced `fetched_models` / `models_status`. Used to
    /// detect when the URL has changed and a re-fetch should be offered.
    pub models_for_base_url: String,
}

impl AssistantState {
    pub fn new(settings: LlmSettings, conversations: Vec<ChatConversation>) -> Self {
        let api_key_set = !settings.provider_id.is_empty()
            && auth::get_llm_api_key(&settings.provider_id)
                .ok()
                .flatten()
                .map(|s| !s.is_empty())
                .unwrap_or(false);

        let active_id = conversations.first().map(|c| c.id.clone());

        Self {
            settings,
            api_key_input: String::new(),
            api_key_set,
            conversations,
            active_conversation_id: active_id,
            draft: String::new(),
            mode: PanelMode::Chat,
            stream_rx: None,
            is_streaming: false,
            last_error: None,
            scroll_to_bottom: false,
            last_saved_at: None,
            last_key_saved_at: None,
            close_requested: false,
            fetched_models: Vec::new(),
            models_status: ModelsStatus::Idle,
            models_rx: None,
            models_for_base_url: String::new(),
        }
    }

    pub fn active_conversation(&self) -> Option<&ChatConversation> {
        self.active_conversation_id
            .as_ref()
            .and_then(|id| self.conversations.iter().find(|c| &c.id == id))
    }

    pub fn active_conversation_mut(&mut self) -> Option<&mut ChatConversation> {
        let id = self.active_conversation_id.clone()?;
        self.conversations.iter_mut().find(|c| c.id == id)
    }

    pub fn new_conversation(&mut self) {
        let now = chrono::Utc::now();
        let conv = ChatConversation {
            id: format!("conv-{}", now.timestamp_millis()),
            title: "New conversation".to_string(),
            messages: Vec::new(),
            created_at: now,
            updated_at: now,
        };
        let id = conv.id.clone();
        self.conversations.insert(0, conv);
        self.active_conversation_id = Some(id);
        self.draft.clear();
        self.last_error = None;
    }

    pub fn delete_conversation(&mut self, id: &str) {
        self.conversations.retain(|c| c.id != id);
        if self.active_conversation_id.as_deref() == Some(id) {
            self.active_conversation_id = self.conversations.first().map(|c| c.id.clone());
        }
    }

    pub fn providers() -> Vec<LlmProviderPreset> {
        config::default_llm_providers()
    }

    /// Apply a new provider selection, copying its defaults into settings.
    #[allow(dead_code)]
    pub fn apply_provider_defaults(&mut self, provider_id: &str) {
        if let Some(p) = Self::providers().into_iter().find(|p| p.id == provider_id) {
            self.settings.provider_id = p.id.clone();
            self.settings.base_url = p.default_base_url.clone();
            if self.settings.model.is_empty() {
                self.settings.model = p.default_model.clone();
            }
            self.api_key_set = auth::get_llm_api_key(&p.id)
                .ok()
                .flatten()
                .map(|s| !s.is_empty())
                .unwrap_or(false);
        }
    }
}

/// Render the side-dock assistant panel inside `ui`.
#[allow(clippy::too_many_arguments)]
pub fn render_assistant_panel(
    ui: &mut egui::Ui,
    state: &mut AssistantState,
    cluster_ctx: &ClusterContext,
    on_send: &mut Option<SendRequest>,
    on_open_docs: &mut Option<String>,
    on_new_conversation: &mut bool,
    on_delete_conversation: &mut Option<String>,
    on_run_console_action: &mut Option<ConsoleAction>,
    available_clusters: &[String],
) {
    // Drain stream updates first.
    drain_stream(state, ui.ctx());
    drain_models(state, ui.ctx());

    let frame = egui::Frame::new()
        .fill(Theme::bg_card())
        .corner_radius(Theme::CARD_ROUNDING)
        .inner_margin(Theme::CARD_PADDING);

    frame.show(ui, |ui| {
        // ---- Row 1: heading + mode toggle + close button -------------------
        ui.horizontal(|ui| {
            ui.heading(
                egui::RichText::new("AI Assistant")
                    .color(Theme::accent())
                    .size(15.0),
            );
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                if ui
                    .add(
                        egui::Button::new(
                            egui::RichText::new("Close")
                                .color(Theme::text_primary())
                                .size(11.0),
                        )
                        .min_size(egui::Vec2::new(60.0, 24.0)),
                    )
                    .on_hover_text("Hide this dock")
                    .clicked()
                {
                    state.close_requested = true;
                    tracing::info!("AI Assistant: close button clicked");
                }
                ui.add_space(4.0);
                // Mode toggle: a gear when on Chat (opens Settings), a back
                // arrow when on Settings (returns to Chat). Replaces the old
                // full-width tab bar.
                let (label, tooltip, next_mode) = match state.mode {
                    PanelMode::Chat => ("⚙", "Open assistant settings", PanelMode::Settings),
                    PanelMode::Settings => ("← Chat", "Back to chat", PanelMode::Chat),
                };
                if ui
                    .add(
                        egui::Button::new(
                            egui::RichText::new(label)
                                .color(Theme::text_primary())
                                .size(12.0),
                        )
                        .min_size(egui::Vec2::new(60.0, 24.0)),
                    )
                    .on_hover_text(tooltip)
                    .clicked()
                {
                    state.mode = next_mode;
                    tracing::info!("AI Assistant: mode toggle -> {:?}", state.mode);
                }
            });
        });
        ui.add_space(6.0);

        match state.mode {
            PanelMode::Chat => {
                if !state.settings.enabled {
                    ui.add_space(8.0);
                    ui.label(
                        egui::RichText::new("Assistant is disabled.")
                            .color(Theme::text_muted())
                            .size(12.0),
                    );
                    ui.label(
                        egui::RichText::new(
                            "Open Settings, pick a provider, paste your API key, then Enable.",
                        )
                        .color(Theme::text_muted())
                        .size(11.0),
                    );
                    ui.add_space(8.0);
                    let btn = ui.add(
                        egui::Button::new(
                            egui::RichText::new("Open Settings")
                                .color(Theme::contrast_text_color(Theme::accent()))
                                .size(13.0)
                                .strong(),
                        )
                        .fill(Theme::accent())
                        .min_size(egui::Vec2::new(180.0, 30.0)),
                    );
                    if btn.clicked() {
                        state.mode = PanelMode::Settings;
                        tracing::info!("AI Assistant: Open Settings button clicked");
                    }
                    ui.add_space(4.0);
                    egui::Frame::new()
                        .fill(Theme::accent().linear_multiply(0.18))
                        .corner_radius(6.0)
                        .inner_margin(egui::Margin::same(10))
                        .show(ui, |ui| {
                            ui.label(
                                egui::RichText::new(
                                    "Tip: the same settings are also available in the main Settings tab.",
                                )
                                .color(Theme::text_secondary())
                                .size(11.0),
                            );
                        });
                    return;
                }

                render_chat(
                    ui,
                    state,
                    cluster_ctx,
                    on_send,
                    on_new_conversation,
                    on_delete_conversation,
                    on_run_console_action,
                    available_clusters,
                );
            }
            PanelMode::Settings => {
                render_settings_section(ui, state, on_open_docs);
            }
        }
    });
}

fn render_chat(
    ui: &mut egui::Ui,
    state: &mut AssistantState,
    cluster_ctx: &ClusterContext,
    on_send: &mut Option<SendRequest>,
    on_new_conversation: &mut bool,
    on_delete_conversation: &mut Option<String>,
    on_run_console_action: &mut Option<ConsoleAction>,
    available_clusters: &[String],
) {
    ui.horizontal(|ui| {
        if ui.button("➕ New").clicked() {
            *on_new_conversation = true;
        }
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            if let Some(conv) = state.active_conversation()
                && ui.button("🗑").on_hover_text("Delete this conversation").clicked()
            {
                *on_delete_conversation = Some(conv.id.clone());
            }
        });
    });

    if state.conversations.is_empty() {
        ui.add_space(8.0);
        ui.label(
            egui::RichText::new("No conversations yet. Type a question below to start.")
                .color(Theme::text_muted())
                .size(11.0),
        );
    } else {
        egui::ScrollArea::vertical()
            .id_salt("assistant_messages_scroll")
            .max_height(ui.available_height() - 180.0)
            .stick_to_bottom(true)
            .auto_shrink([false, false])
            .show(ui, |ui| {
                ui.set_min_width(ui.available_width());
                if state.scroll_to_bottom {
                    ui.scroll_to_cursor(Some(egui::Align::BOTTOM));
                    state.scroll_to_bottom = false;
                }
                if let Some(conv) = state.active_conversation() {
                    if conv.messages.is_empty() {
                        ui.label(
                            egui::RichText::new(
                                "Ask anything about Elasticsearch, Drastic Smurf, or your clusters.",
                            )
                            .color(Theme::text_muted())
                            .size(11.0),
                        );
                    }
                    let cluster_tag = if !cluster_ctx.cluster_name.is_empty() {
                        cluster_ctx.cluster_name.clone()
                    } else {
                        "(no cluster)".to_string()
                    };
                    ui.label(
                        egui::RichText::new(format!("Context: {}", cluster_tag))
                            .color(Theme::text_muted())
                            .size(10.0),
                    );
                    ui.add_space(4.0);
                    for msg in &conv.messages {
                        render_message(ui, msg);
                        if msg.role == "assistant" {
                            for action in parse_console_actions(&msg.content) {
                                render_action_card(ui, &action, available_clusters, on_run_console_action);
                            }
                        }
                    }
                }
            });
    }

    if let Some(err) = &state.last_error {
        ui.add_space(4.0);
        ui.label(
            egui::RichText::new(format!("⚠ {}", err))
                .color(Theme::danger())
                .size(11.0),
        );
    }

    ui.add_space(6.0);
    ui.separator();
    ui.add_space(4.0);

    ui.horizontal(|ui| {
        ui.label(
            egui::RichText::new(format!("→ {}", state.settings.model))
                .color(Theme::text_muted())
                .size(10.0),
        );
    });
    let response = ui.add(
        egui::TextEdit::multiline(&mut state.draft)
            .hint_text("Ask about cluster health, ES API, error messages, DSL queries...")
            .desired_rows(3)
            .desired_width(f32::INFINITY)
            .id_source("assistant_input"),
    );
    let submit_pressed = ui.input(|i| i.key_pressed(egui::Key::Enter) && i.modifiers.command);

    ui.horizontal(|ui| {
        let send_btn = ui.add_enabled(
            !state.is_streaming && !state.draft.trim().is_empty(),
            egui::Button::new(
                egui::RichText::new(if state.is_streaming { "⏳ Streaming..." } else { "📤 Send" })
                    .color(Theme::contrast_text_color(Theme::accent())),
            )
            .fill(Theme::accent()),
        );
        let clicked_send = send_btn.clicked();

        if (clicked_send || submit_pressed)
            && let Some(conv_id) = state.active_conversation_id.clone()
        {
            let user_text = state.draft.trim().to_string();
            if !user_text.is_empty() {
                *on_send = Some(SendRequest {
                    conversation_id: conv_id,
                    user_text,
                    include_cluster_context: state.settings.auto_cluster_context
                        && !cluster_ctx.summary.is_empty(),
                });
                state.draft.clear();
                state.scroll_to_bottom = true;
            }
        }

        if state.is_streaming && ui.button("✖ Cancel").clicked() {
            state.is_streaming = false;
            state.stream_rx = None;
        }
    });

    if response.has_focus() {
        ui.ctx().request_repaint();
    }
}

fn render_message(ui: &mut egui::Ui, msg: &ChatMessage) {
    let (label, color) = match msg.role.as_str() {
        "user" => ("You", Theme::accent()),
        "assistant" => ("Assistant", Theme::success()),
        "system" => ("System", Theme::text_muted()),
        other => (other, Theme::text_secondary()),
    };
    egui::Frame::new()
        .fill(Theme::bg_input())
        .corner_radius(6.0)
        .inner_margin(egui::Margin::same(8))
        .show(ui, |ui| {
            ui.label(egui::RichText::new(label).color(color).strong().size(11.0));
            ui.add_space(2.0);
            ui.label(
                egui::RichText::new(&msg.content)
                    .color(Theme::text_primary())
                    .size(12.0),
            );
        });
    ui.add_space(4.0);
}

/// Render the assistant settings section. Reusable across the dock panel
/// and the main Settings tab. Settings are auto-saved on every change.
pub fn render_settings_section(
    ui: &mut egui::Ui,
    state: &mut AssistantState,
    on_open_docs: &mut Option<String>,
) {
    let mut enabled = state.settings.enabled;
    if ui.checkbox(&mut enabled, "Enable AI Assistant").changed() {
        state.settings.enabled = enabled;
        state.last_saved_at = Some(chrono::Utc::now());
    }
    ui.add_space(8.0);

    ui.label("Provider:");
    let providers = AssistantState::providers();
    let prev_id = state.settings.provider_id.clone();
    egui::ComboBox::from_id_salt("llm_provider_select")
        .selected_text(
            providers
                .iter()
                .find(|p| p.id == state.settings.provider_id)
                .map(|p| p.label.clone())
                .unwrap_or_else(|| state.settings.provider_id.clone()),
        )
        .show_ui(ui, |ui| {
            for p in &providers {
                ui.selectable_value(&mut state.settings.provider_id, p.id.clone(), &p.label);
            }
        });
    let provider_changed = state.settings.provider_id != prev_id;
    if provider_changed {
        if let Some(p) = providers
            .iter()
            .find(|p| p.id == state.settings.provider_id)
        {
            state.settings.base_url = p.default_base_url.clone();
            if state.settings.model.is_empty() {
                state.settings.model = p.default_model.clone();
            }
            state.api_key_set = auth::get_llm_api_key(&p.id)
                .ok()
                .flatten()
                .map(|s| !s.is_empty())
                .unwrap_or(false);
        }
        state.last_saved_at = Some(chrono::Utc::now());
        // Switching provider invalidates the cached model list.
        state.fetched_models.clear();
        state.models_status = ModelsStatus::Idle;
        state.models_for_base_url.clear();
    }

    ui.add_space(8.0);

    // ---- Base URL ------------------------------------------------------
    let mut base_url_changed_committed = false;
    ui.horizontal(|ui| {
        ui.label("Base URL:");
        let url_resp = ui.add(
            egui::TextEdit::singleline(&mut state.settings.base_url)
                .hint_text("https://api.openai.com/v1")
                .desired_width(ui.available_width()),
        );
        if url_resp.lost_focus() {
            state.last_saved_at = Some(chrono::Utc::now());
            base_url_changed_committed = true;
        }
    });

    // ---- API Key -------------------------------------------------------
    ui.add_space(4.0);
    let mut api_key_just_saved = false;
    ui.horizontal(|ui| {
        ui.label("API Key:");
        let hint = if state.api_key_set {
            "(stored - paste to replace)"
        } else {
            "paste key here..."
        };
        let _key_edit = ui.add(
            egui::TextEdit::singleline(&mut state.api_key_input)
                .password(true)
                .hint_text(hint)
                .desired_width(ui.available_width() - 90.0),
        );
        if ui
            .add(
                egui::Button::new(
                    egui::RichText::new("Save")
                        .color(Theme::contrast_text_color(Theme::accent())),
                )
                .fill(Theme::accent())
                .min_size(egui::Vec2::new(80.0, 22.0)),
            )
            .clicked()
            && !state.api_key_input.is_empty()
        {
            let _ = auth::set_llm_api_key(&state.settings.provider_id, &state.api_key_input);
            state.api_key_input.clear();
            state.api_key_set = true;
            state.last_key_saved_at = Some(chrono::Utc::now());
            api_key_just_saved = true;
        }
    });

    ui.horizontal(|ui| {
        if state.api_key_set && ui.button("Clear stored key").clicked() {
            let _ = auth::delete_llm_api_key(&state.settings.provider_id);
            state.api_key_set = false;
        }
        if let Some(p) = providers
            .iter()
            .find(|p| p.id == state.settings.provider_id)
            && !p.docs_url.is_empty()
            && ui.button("📖 Docs").clicked()
        {
            *on_open_docs = Some(p.docs_url.clone());
        }
    });

    // ---- Model selection ----------------------------------------------
    // Auto-fetch when the base URL settles, when the API key is saved,
    // when the provider changes, or on first visit to the Settings panel
    // while we have a URL configured.
    let url_changed_since_last_fetch =
        state.settings.base_url != state.models_for_base_url;
    let should_auto_fetch = !state.settings.base_url.trim().is_empty()
        && state.models_rx.is_none()
        && (api_key_just_saved
            || (base_url_changed_committed && url_changed_since_last_fetch)
            || provider_changed
            || (matches!(state.models_status, ModelsStatus::Idle)
                && url_changed_since_last_fetch));
    if should_auto_fetch {
        start_models_fetch(state);
    }

    ui.add_space(8.0);
    ui.horizontal(|ui| {
        ui.label("Model:");
        let preset = providers
            .iter()
            .find(|p| p.id == state.settings.provider_id)
            .cloned();
        let prev_model = state.settings.model.clone();

        // Combine fetched models (live) with preset suggestions, deduped.
        let mut options: Vec<String> = state.fetched_models.clone();
        if let Some(p) = &preset {
            for m in &p.models {
                if !options.contains(m) {
                    options.push(m.clone());
                }
            }
        }
        let selected_text = if state.settings.model.is_empty() {
            "(none)".to_string()
        } else {
            state.settings.model.clone()
        };
        egui::ComboBox::from_id_salt("llm_model_select")
            .selected_text(selected_text)
            .show_ui(ui, |ui| {
                if options.is_empty() {
                    ui.label(
                        egui::RichText::new("No models discovered yet")
                            .color(Theme::text_muted())
                            .size(11.0),
                    );
                } else {
                    for m in &options {
                        ui.selectable_value(&mut state.settings.model, m.clone(), m);
                    }
                }
            });
        let model_edit = ui.add(
            egui::TextEdit::singleline(&mut state.settings.model)
                .hint_text("model-name")
                .desired_width(150.0),
        );
        if state.settings.model != prev_model {
            state.last_saved_at = Some(chrono::Utc::now());
        }
        if model_edit.lost_focus() {
            state.last_saved_at = Some(chrono::Utc::now());
        }

        // Refresh button: small, right-aligned-ish.
        let refresh_enabled = !state.settings.base_url.trim().is_empty()
            && !matches!(state.models_status, ModelsStatus::Loading);
        if ui
            .add_enabled(refresh_enabled, egui::Button::new("⟳"))
            .on_hover_text("Re-fetch the model list from the provider")
            .clicked()
        {
            start_models_fetch(state);
        }
    });

    // ---- Models status pill -------------------------------------------
    match &state.models_status {
        ModelsStatus::Idle => {}
        ModelsStatus::Loading => {
            ui.label(
                egui::RichText::new("⟳ Loading models…")
                    .color(Theme::text_muted())
                    .size(10.5),
            );
        }
        ModelsStatus::Ok(n) => {
            ui.label(
                egui::RichText::new(format!("✓ Model load OK ({} available)", n))
                    .color(Theme::success())
                    .size(10.5),
            );
        }
        ModelsStatus::Error(err) => {
            let short = if err.len() > 140 {
                format!("{}…", &err[..140])
            } else {
                err.clone()
            };
            ui.label(
                egui::RichText::new(format!("✗ Could not load models: {}", short))
                    .color(Theme::danger())
                    .size(10.5),
            );
        }
    }

    // ---- Generation parameters ----------------------------------------
    ui.add_space(8.0);
    ui.horizontal(|ui| {
        ui.label("Temperature:");
        if ui
            .add(egui::Slider::new(&mut state.settings.temperature, 0.0..=2.0).step_by(0.05))
            .changed()
        {
            state.last_saved_at = Some(chrono::Utc::now());
        }
    });

    ui.horizontal(|ui| {
        ui.label("Max context chars:");
        if ui
            .add(
                egui::DragValue::new(&mut state.settings.max_context_chars)
                    .range(500..=20000),
            )
            .changed()
        {
            state.last_saved_at = Some(chrono::Utc::now());
        }
    });

    let mut auto_ctx = state.settings.auto_cluster_context;
    if ui
        .checkbox(&mut auto_ctx, "Auto-include concise cluster context")
        .changed()
    {
        state.settings.auto_cluster_context = auto_ctx;
        state.last_saved_at = Some(chrono::Utc::now());
    }

    // ---- Saved indicator ----------------------------------------------
    if let Some(t) = state.last_saved_at {
        let age = (chrono::Utc::now() - t).num_seconds();
        let label = if age < 2 {
            "✓ Saved".to_string()
        } else if age < 60 {
            format!("✓ Saved {}s ago", age)
        } else if age < 3600 {
            format!("✓ Saved {}m ago", age / 60)
        } else {
            format!("✓ Saved {}h ago", age / 3600)
        };
        ui.label(
            egui::RichText::new(label)
                .color(Theme::success())
                .size(10.5),
        );
    }
}

/// Render a single "this assistant wants to run X" consent card.
fn render_action_card(
    ui: &mut egui::Ui,
    action: &ConsoleAction,
    available_clusters: &[String],
    on_run: &mut Option<ConsoleAction>,
) {
    let body_preview = action
        .body
        .as_ref()
        .map(|b| {
            let trimmed = b.trim();
            if trimmed.len() > 80 {
                format!("{}…", &trimmed[..80])
            } else {
                trimmed.to_string()
            }
        })
        .unwrap_or_default();

    egui::Frame::new()
        .fill(Theme::bg_card())
        .stroke(egui::Stroke::new(1.0, Theme::accent()))
        .corner_radius(6.0)
        .inner_margin(egui::Margin::same(8))
        .show(ui, |ui| {
            ui.label(
                egui::RichText::new("⚡ Proposed Console command")
                    .color(Theme::accent())
                    .strong()
                    .size(11.0),
            );
            ui.add_space(2.0);
            ui.horizontal(|ui| {
                ui.label(egui::RichText::new("Cluster:").size(11.0).color(Theme::text_muted()));
                ui.label(
                    egui::RichText::new(
                        if action.cluster.is_empty() {
                            "(default)"
                        } else {
                            action.cluster.as_str()
                        },
                    )
                    .strong()
                    .size(11.0),
                );
                ui.label(
                    egui::RichText::new(format!("  {} {}", action.method, action.path))
                        .code()
                        .size(11.0),
                );
            });
            if !body_preview.is_empty() {
                ui.label(
                    egui::RichText::new(format!("Body: {}", body_preview))
                        .color(Theme::text_muted())
                        .size(10.5),
                );
            }
            ui.add_space(4.0);
            ui.horizontal(|ui| {
                if ui
                    .add(
                        egui::Button::new(
                            egui::RichText::new("▶ Run in Console")
                                .color(Theme::contrast_text_color(Theme::accent())),
                        )
                        .fill(Theme::accent())
                        .min_size(egui::Vec2::new(120.0, 22.0)),
                    )
                    .clicked()
                {
                    let mut resolved = action.clone();
                    if resolved.cluster.is_empty()
                        && let Some(first) = available_clusters.first()
                    {
                        resolved.cluster = first.clone();
                    }
                    *on_run = Some(resolved);
                }
                if available_clusters.is_empty() {
                    ui.label(
                        egui::RichText::new("(no clusters configured)")
                            .color(Theme::text_muted())
                            .size(10.0),
                    );
                } else if !action.cluster.is_empty()
                    && !available_clusters.contains(&action.cluster)
                {
                    ui.label(
                        egui::RichText::new(format!("⚠ cluster '{}' not configured", action.cluster))
                            .color(Theme::danger())
                            .size(10.0),
                    );
                }
            });
        });
    ui.add_space(4.0);
}

/// Drain pending stream updates into the active conversation's last assistant message.
fn drain_stream(state: &mut AssistantState, ctx: &egui::Context) {
    // Take the receiver out temporarily so we can mutate `state` while draining.
    let rx = match state.stream_rx.take() {
        Some(rx) => rx,
        None => return,
    };
    loop {
        match rx.try_recv() {
            Ok(StreamMsg::Delta(delta)) => {
                if let Some(conv) = state.active_conversation_mut()
                    && let Some(last) = conv.messages.last_mut()
                    && last.role == "assistant"
                {
                    last.content.push_str(&delta);
                    conv.updated_at = chrono::Utc::now();
                }
                state.scroll_to_bottom = true;
                ctx.request_repaint();
            }
            Ok(StreamMsg::Done) => {
                state.is_streaming = false;
                ctx.request_repaint();
                return;
            }
            Ok(StreamMsg::Error(err)) => {
                state.is_streaming = false;
                state.last_error = Some(err);
                ctx.request_repaint();
                return;
            }
            Err(std::sync::mpsc::TryRecvError::Empty) => {
                // No more messages right now; put the receiver back and stop.
                state.stream_rx = Some(rx);
                break;
            }
            Err(std::sync::mpsc::TryRecvError::Disconnected) => {
                state.is_streaming = false;
                state.last_error =
                    Some("Stream connection closed unexpectedly".to_string());
                ctx.request_repaint();
                return;
            }
        }
    }
    if state.is_streaming {
        ctx.request_repaint_after(std::time::Duration::from_millis(50));
    }
}

/// What the UI sends up to the host so it can dispatch a network call.
#[derive(Debug, Clone)]
pub struct SendRequest {
    pub conversation_id: String,
    pub user_text: String,
    pub include_cluster_context: bool,
}

/// Build a concise text snapshot of cluster health/stats for the LLM.
pub fn build_cluster_context(
    cluster_name: Option<&str>,
    health: Option<&crate::core::es_client::ClusterHealth>,
    stats: Option<&crate::core::es_client::ClusterStats>,
    es_version: Option<&str>,
    error: Option<&str>,
    max_chars: usize,
) -> ClusterContext {
    let target_name = cluster_name.unwrap_or("").to_string();
    if target_name.is_empty() && health.is_none() && stats.is_none() && error.is_none() {
        return ClusterContext::default();
    }

    let mut lines: Vec<String> = Vec::new();
    lines.push(format!("Cluster: {}", target_name));
    if let Some(v) = es_version {
        lines.push(format!("Elasticsearch version: {}", v));
    }
    if let Some(h) = health {
        lines.push(format!(
            "Health: status={} nodes={} active_shards={} relocating={} unassigned={} initializing={}",
            h.status,
            h.number_of_nodes,
            h.active_shards,
            h.relocating_shards,
            h.unassigned_shards,
            h.initializing_shards
        ));
    } else if let Some(err) = error {
        lines.push(format!("Last error: {}", err));
    }
    if let Some(s) = stats {
        if let Some(indices) = &s.indices {
            let docs = indices.docs.as_ref().map(|d| d.count).unwrap_or(0);
            let store_bytes = indices.store.as_ref().map(|s| s.size_in_bytes).unwrap_or(0);
            lines.push(format!(
                "Indices: count={} docs={} store_bytes={}",
                indices.count, docs, store_bytes
            ));
        }
        if let Some(nodes) = &s.nodes {
            let node_total = nodes.count.as_ref().map(|c| c.total).unwrap_or(0);
            if let Some(jvm) = &nodes.jvm
                && let Some(mem) = &jvm.mem
            {
                lines.push(format!(
                    "JVM heap: used_bytes={} max_bytes={} nodes={}",
                    mem.heap_used_in_bytes, mem.heap_max_in_bytes, node_total
                ));
            } else {
                lines.push(format!("Nodes: total={}", node_total));
            }
        }
    }

    let joined = lines.join("\n");
    let summary = if joined.len() > max_chars {
        let mut t = joined[..max_chars].to_string();
        t.push_str("\n...[context truncated]...");
        t
    } else {
        joined
    };

    ClusterContext {
        summary,
        cluster_name: target_name,
    }
}

const SYSTEM_PROMPT: &str = "You are an expert Elasticsearch administrator and Drastic Smurf assistant. \
You help with cluster troubleshooting, query optimization, index design, API usage, and \
interpreting error messages. Be concise, accurate, and use code blocks for curl/DSL examples. \
If cluster context is provided, reference it specifically.";

/// Build the full `messages` array for the next API call, given the
/// current conversation + optional cluster context.
pub fn build_llm_messages(
    conversation: &ChatConversation,
    cluster_ctx: Option<&ClusterContext>,
    user_text: &str,
) -> Vec<LlmMessage> {
    let mut out: Vec<LlmMessage> = Vec::with_capacity(conversation.messages.len() + 3);

    let mut system = SYSTEM_PROMPT.to_string();
    if let Some(ctx) = cluster_ctx
        && !ctx.summary.is_empty()
    {
        system.push_str("\n\nCurrent cluster context:\n");
        system.push_str(&ctx.summary);
    }
    out.push(LlmMessage {
        role: "system".into(),
        content: system,
    });

    for m in &conversation.messages {
        out.push(LlmMessage {
            role: m.role.clone(),
            content: m.content.clone(),
        });
    }
    out.push(LlmMessage {
        role: "user".into(),
        content: user_text.to_string(),
    });
    out
}

/// Spawn the streaming call on the tokio runtime and wire it into a channel.
pub fn spawn_stream(
    settings: LlmSettings,
    api_key: Option<String>,
    messages: Vec<LlmMessage>,
) -> Receiver<StreamMsg> {
    let (tx, rx): (Sender<StreamMsg>, Receiver<StreamMsg>) = channel();
    tokio::spawn(async move {
        let client = reqwest::Client::builder()
            .timeout(std::time::Duration::from_secs(120))
            .connect_timeout(std::time::Duration::from_secs(10))
            // Some local servers (e.g. LM Studio) close idle connections
            // unexpectedly; fresh connections avoid stale-pool decode errors.
            .pool_max_idle_per_host(0)
            .build()
            .unwrap_or_else(|_| reqwest::Client::new());

        // Forward deltas to the channel.
        let tx_for_delta = tx.clone();
        let stream_result = llm::chat_stream(
            &client,
            &settings,
            api_key.as_deref(),
            &messages,
            move |delta| {
                let _ = tx_for_delta.send(StreamMsg::Delta(delta.to_string()));
            },
        )
        .await;

        match stream_result {
            Ok(()) => {
                let _ = tx.send(StreamMsg::Done);
            }
            Err(e) => {
                tracing::warn!("AI assistant chat stream failed: {:#}", e);
                let _ = tx.send(StreamMsg::Error(format!("{:#}", e)));
            }
        }
    });
    rx
}

/// Spawn an async fetch of the provider's `/models` listing.
pub fn spawn_models_fetch(
    settings: LlmSettings,
    api_key: Option<String>,
) -> Receiver<ModelsMsg> {
    let (tx, rx): (Sender<ModelsMsg>, Receiver<ModelsMsg>) = channel();
    tokio::spawn(async move {
        let client = reqwest::Client::builder()
            .timeout(std::time::Duration::from_secs(15))
            .connect_timeout(std::time::Duration::from_secs(5))
            // Local inference servers sometimes close idle connections
            // abruptly; disable reuse to avoid flaky "decode response body" errors.
            .pool_max_idle_per_host(0)
            .build()
            .unwrap_or_else(|_| reqwest::Client::new());

        let url = llm::resolve_models_url(&settings.base_url);
        tracing::info!("Fetching LLM model list from {}", url);

        // The /models endpoint is cheap and idempotent; retry once to paper
        // over transient local-server hiccups.
        let msg = match llm::list_models(&client, &settings, api_key.as_deref()).await {
            Ok(models) => ModelsMsg::Ok(models),
            Err(e) => {
                tracing::warn!(
                    "First model-list fetch from {} failed, retrying once: {:#}",
                    url,
                    e
                );
                tokio::time::sleep(std::time::Duration::from_millis(500)).await;
                match llm::list_models(&client, &settings, api_key.as_deref()).await {
                    Ok(models) => ModelsMsg::Ok(models),
                    Err(e2) => {
                        tracing::warn!("Failed to fetch LLM model list from {}: {:#}", url, e2);
                        ModelsMsg::Err(format!("{:#}", e2))
                    }
                }
            }
        };
        let _ = tx.send(msg);
    });
    rx
}

/// Drain any pending models-fetch results into `state`.
fn drain_models(state: &mut AssistantState, ctx: &egui::Context) {
    let Some(rx) = state.models_rx.take() else {
        return;
    };
    match rx.try_recv() {
        Ok(ModelsMsg::Ok(models)) => {
            state.models_status = ModelsStatus::Ok(models.len());
            state.fetched_models = models;
            ctx.request_repaint();
        }
        Ok(ModelsMsg::Err(err)) => {
            state.models_status = ModelsStatus::Error(err);
            state.fetched_models.clear();
            ctx.request_repaint();
        }
        Err(std::sync::mpsc::TryRecvError::Empty) => {
            // Still pending: put it back.
            state.models_rx = Some(rx);
            ctx.request_repaint_after(std::time::Duration::from_millis(100));
        }
        Err(std::sync::mpsc::TryRecvError::Disconnected) => {
            state.models_status =
                ModelsStatus::Error("Model fetch task ended unexpectedly".to_string());
            state.fetched_models.clear();
            ctx.request_repaint();
        }
    }
}

/// Kick off a fresh models fetch using the current settings.
fn start_models_fetch(state: &mut AssistantState) {
    if state.settings.base_url.trim().is_empty() {
        state.models_status = ModelsStatus::Idle;
        state.fetched_models.clear();
        state.models_for_base_url.clear();
        return;
    }
    let api_key = auth::get_llm_api_key(&state.settings.provider_id)
        .ok()
        .flatten();
    state.models_status = ModelsStatus::Loading;
    state.models_for_base_url = state.settings.base_url.clone();
    state.models_rx = Some(spawn_models_fetch(state.settings.clone(), api_key));
}

/// Extract proposed console commands from an assistant message.
///
/// Recognizes three forms:
///
/// 1. JSON code blocks with an `action: "run_command"` (or matching fields)
/// 2. HTTP-style lines like `GET /_cluster/health` or `POST /_search`
/// 3. `curl` invocations (host is stripped, method/path/body parsed)
///
/// Results are de-duplicated.
pub fn parse_console_actions(text: &str) -> Vec<ConsoleAction> {
    let mut actions: Vec<ConsoleAction> = Vec::new();
    let mut seen: std::collections::HashSet<String> = std::collections::HashSet::new();

    for raw_block in extract_code_blocks(text) {
        if let Some(action) = parse_action_from_json(&raw_block) {
            push_unique(&mut actions, &mut seen, action);
        }
        if let Some(action) = parse_action_from_http_line(&raw_block) {
            push_unique(&mut actions, &mut seen, action);
        }
        if let Some(action) = parse_action_from_curl(&raw_block) {
            push_unique(&mut actions, &mut seen, action);
        }
    }

    // Also scan plain text outside code blocks for HTTP-style lines
    // (the model often pastes them as `GET /something`).
    for line in text.lines() {
        if let Some(action) = parse_action_from_http_line(line.trim()) {
            push_unique(&mut actions, &mut seen, action);
        }
    }

    actions
}

fn push_unique(
    actions: &mut Vec<ConsoleAction>,
    seen: &mut std::collections::HashSet<String>,
    action: ConsoleAction,
) {
    let key = format!(
        "{}|{}|{}|{}",
        action.cluster,
        action.method,
        action.path,
        action.body.as_deref().unwrap_or("")
    );
    if seen.insert(key) {
        actions.push(action);
    }
}

fn extract_code_blocks(text: &str) -> Vec<String> {
    let mut blocks: Vec<String> = Vec::new();
    let mut rest = text;
    while let Some(start) = rest.find("```") {
        let after_open = &rest[start + 3..];
        // Skip language tag (anything up to the first newline).
        let body_start = after_open.find('\n').map(|i| i + 1).unwrap_or(0);
        let body = &after_open[body_start..];
        if let Some(end) = body.find("```") {
            let block = body[..end].trim().to_string();
            // Drop the language tag from the first line if it looks like one
            // (eg "json" or "http").
            let block = strip_language_tag(&block);
            blocks.push(block);
            rest = &body[end + 3..];
        } else {
            break;
        }
    }
    blocks
}

fn strip_language_tag(block: &str) -> String {
    if let Some(newline_idx) = block.find('\n') {
        let first_line = block[..newline_idx].trim();
        if first_line.chars().all(|c| c.is_ascii_alphabetic())
            && !first_line.is_empty()
            && first_line.len() <= 12
        {
            return block[newline_idx + 1..].to_string();
        }
    }
    block.to_string()
}

fn parse_action_from_json(block: &str) -> Option<ConsoleAction> {
    let trimmed = block.trim();
    if !(trimmed.starts_with('{') && trimmed.ends_with('}')) {
        return None;
    }
    let value: serde_json::Value = serde_json::from_str(trimmed).ok()?;
    // Two accepted shapes:
    //   {"action":"run_command","cluster":"c","method":"GET","path":"/","body":"..."}
    //   {"cluster":"c","method":"GET","path":"/","body":"..."}
    let obj = value.as_object()?;
    let cluster = obj
        .get("cluster")
        .and_then(|v| v.as_str())
        .unwrap_or("")
        .to_string();
    let method = obj
        .get("method")
        .and_then(|v| v.as_str())
        .unwrap_or("GET")
        .to_uppercase();
    let path = obj
        .get("path")
        .and_then(|v| v.as_str())
        .map(|s| s.to_string());
    let path = match path {
        Some(p) if !p.is_empty() => p,
        _ => return None,
    };
    let body = obj
        .get("body")
        .and_then(|v| v.as_str())
        .map(|s| s.to_string())
        .filter(|s| !s.is_empty());
    Some(ConsoleAction {
        cluster,
        method,
        path,
        body,
    })
}

fn parse_action_from_http_line(line: &str) -> Option<ConsoleAction> {
    let line = line.trim();
    if line.is_empty() {
        return None;
    }
    // 1. Try the whole line as an HTTP command.
    if let Some(action) = parse_http_at_start(line) {
        return Some(action);
    }
    // 2. Try each backtick-wrapped inline-code segment. Models often wrap
    //    commands like "Run `GET /_cat/indices?v`" in single backticks.
    let mut start = 0;
    while let Some(open) = line[start..].find('`') {
        let after_open = start + open + 1;
        match line[after_open..].find('`') {
            Some(close) => {
                let inner = &line[after_open..after_open + close];
                if let Some(action) = parse_http_at_start(inner) {
                    return Some(action);
                }
                start = after_open + close + 1;
            }
            None => break,
        }
    }
    None
}

fn parse_http_at_start(line: &str) -> Option<ConsoleAction> {
    let line = line.trim_matches('`').trim();
    let (method, rest) = line.split_once(' ')?;
    let method_upper = method.to_uppercase();
    if !matches!(
        method_upper.as_str(),
        "GET" | "POST" | "PUT" | "DELETE" | "HEAD" | "PATCH"
    ) {
        return None;
    }
    let rest = rest.trim().trim_matches('`').trim();
    let path = strip_url_to_path(rest);
    if !path.starts_with('/') {
        return None;
    }
    Some(ConsoleAction {
        cluster: String::new(),
        method: method_upper,
        path,
        body: None,
    })
}

fn parse_action_from_curl(block: &str) -> Option<ConsoleAction> {
    let first = block.lines().next()?.trim();
    if !first.starts_with("curl ") && !first.contains(" curl ") {
        return None;
    }
    // Pull method from -X METHOD (defaults to GET).
    let mut method = "GET".to_string();
    let mut path: Option<String> = None;
    let mut body: Option<String> = None;
    let mut data: Option<String> = None;

    let mut tokens = shell_split(first);
    // Drop the `curl` token itself.
    if !tokens.is_empty() {
        tokens.remove(0);
    }

    let mut i = 0;
    while i < tokens.len() {
        let tok = &tokens[i];
        match tok.as_str() {
            "-X" | "--request" => {
                if let Some(m) = tokens.get(i + 1) {
                    method = m.to_uppercase();
                    i += 2;
                    continue;
                }
            }
            "-d" | "--data" | "--data-raw" | "--data-binary" => {
                if let Some(d) = tokens.get(i + 1) {
                    data = Some(d.clone());
                    i += 2;
                    continue;
                }
            }
            "-H" | "--header" => {
                i += 2;
                continue;
            }
            url => {
                if path.is_none() && (url.starts_with("http://") || url.starts_with("https://")) {
                    path = Some(strip_url_to_path(url));
                }
            }
        }
        i += 1;
    }

    let path = path?;
    if method != "GET" && data.is_some() {
        body = data;
    }
    Some(ConsoleAction {
        cluster: String::new(),
        method,
        path,
        body,
    })
}

fn strip_url_to_path(url: &str) -> String {
    if let Some(idx) = url.find("://") {
        let after = &url[idx + 3..];
        if let Some(slash_idx) = after.find('/') {
            return after[slash_idx..].to_string();
        }
        return "/".to_string();
    }
    url.to_string()
}

/// Tiny shell-style splitter that handles single quotes. Double quotes are
/// preserved literally (curl bodies often contain `"` and we'd rather keep
/// the user's payload intact than guess at escape semantics).
fn shell_split(input: &str) -> Vec<String> {
    let mut out: Vec<String> = Vec::new();
    let mut current = String::new();
    let mut in_single = false;
    for c in input.chars() {
        match c {
            '\'' => in_single = !in_single,
            c if c.is_whitespace() && !in_single => {
                if !current.is_empty() {
                    out.push(std::mem::take(&mut current));
                }
            }
            c => current.push(c),
        }
    }
    if !current.is_empty() {
        out.push(current);
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cluster_context_truncates_long_output() {
        let long_version = "8.10.0".repeat(20);
        let ctx = build_cluster_context(
            Some("big"),
            None,
            None,
            Some(&long_version),
            None,
            40,
        );
        assert!(ctx.summary.len() <= 80);
        assert!(ctx.summary.contains("Cluster: big"));
        assert!(ctx.summary.contains("...[context truncated]..."));
    }

    #[test]
    fn empty_cluster_context_returns_default() {
        let ctx = build_cluster_context(None, None, None, None, None, 4000);
        assert!(ctx.summary.is_empty());
        assert!(ctx.cluster_name.is_empty());
    }

    #[test]
    fn cluster_context_uses_error_when_no_health() {
        let ctx = build_cluster_context(
            Some("broken"),
            None,
            None,
            None,
            Some("401 Unauthorized"),
            4000,
        );
        assert!(ctx.summary.contains("401 Unauthorized"));
        assert!(!ctx.summary.contains("Health:"));
    }

    #[test]
    fn build_llm_messages_prepends_system_and_appends_user() {
        let conv = ChatConversation {
            id: "c1".into(),
            title: "x".into(),
            messages: vec![ChatMessage {
                role: "user".into(),
                content: "earlier".into(),
                timestamp: chrono::Utc::now(),
            }],
            created_at: chrono::Utc::now(),
            updated_at: chrono::Utc::now(),
        };
        let ctx = ClusterContext {
            summary: "Cluster: foo\nHealth: green".into(),
            cluster_name: "foo".into(),
        };
        let msgs = build_llm_messages(&conv, Some(&ctx), "now?");
        assert_eq!(msgs.len(), 3);
        assert_eq!(msgs[0].role, "system");
        assert!(msgs[0].content.contains("Cluster: foo"));
        assert_eq!(msgs[1].role, "user");
        assert_eq!(msgs[1].content, "earlier");
        assert_eq!(msgs[2].role, "user");
        assert_eq!(msgs[2].content, "now?");
    }

    #[test]
    fn build_llm_messages_skips_context_when_empty() {
        let conv = ChatConversation {
            id: "c".into(),
            title: "x".into(),
            messages: vec![],
            created_at: chrono::Utc::now(),
            updated_at: chrono::Utc::now(),
        };
        let msgs = build_llm_messages(&conv, None, "hi");
        assert_eq!(msgs.len(), 2);
        assert!(!msgs[0].content.contains("Current cluster context"));
    }

    #[test]
    fn parses_http_line_in_prose() {
        let text = "Try `GET /_cat/indices?v` to see them.";
        let actions = parse_console_actions(text);
        assert_eq!(actions.len(), 1);
        assert_eq!(actions[0].method, "GET");
        assert_eq!(actions[0].path, "/_cat/indices?v");
    }

    #[test]
    fn parses_json_command_block() {
        let text = r#"Sure, run this:

```json
{
  "action": "run_command",
  "cluster": "apac-prod-1",
  "method": "POST",
  "path": "/_search",
  "body": "{\"query\":{\"match_all\":{}}}"
}
```
"#;
        let actions = parse_console_actions(text);
        assert_eq!(actions.len(), 1);
        assert_eq!(actions[0].cluster, "apac-prod-1");
        assert_eq!(actions[0].method, "POST");
        assert_eq!(actions[0].path, "/_search");
        assert_eq!(
            actions[0].body.as_deref(),
            Some("{\"query\":{\"match_all\":{}}}")
        );
    }

    #[test]
    fn parses_curl_command() {
        let text = r#"
```
curl -X DELETE 'https://elastic.example.com:9200/old-index'
```
"#;
        let actions = parse_console_actions(text);
        assert_eq!(actions.len(), 1);
        assert_eq!(actions[0].method, "DELETE");
        assert_eq!(actions[0].path, "/old-index");
    }

    #[test]
    fn parses_post_curl_with_data() {
        let text = r#"```bash
curl -X POST 'http://localhost:9200/_refresh'
```"#;
        let actions = parse_console_actions(text);
        assert_eq!(actions.len(), 1);
        assert_eq!(actions[0].method, "POST");
        assert_eq!(actions[0].path, "/_refresh");
    }

    #[test]
    fn deduplicates_repeated_commands() {
        let text = "GET /_cluster/health\nGET /_cluster/health\n";
        let actions = parse_console_actions(text);
        assert_eq!(actions.len(), 1);
    }

    #[test]
    fn ignores_prose_lines_starting_with_uppercase() {
        let text = "GET requests are how we read docs.";
        let actions = parse_console_actions(text);
        assert!(actions.is_empty(), "prose should not be parsed: {:?}", actions);
    }

    #[test]
    fn strip_url_to_path_handles_host_and_path() {
        assert_eq!(
            strip_url_to_path("https://elastic.example.com:9200/_cat/indices"),
            "/_cat/indices"
        );
        assert_eq!(
            strip_url_to_path("http://localhost:9200"),
            "/"
        );
        assert_eq!(strip_url_to_path("/relative"), "/relative");
    }

    #[test]
    fn shell_split_handles_quotes() {
        assert_eq!(
            shell_split("curl -X POST 'http://h/p' -d '{\"a\":1}'"),
            vec!["curl", "-X", "POST", "http://h/p", "-d", "{\"a\":1}"]
        );
    }
}