//! LLM HTTP client with SSE streaming for OpenAI-compatible endpoints.
//!
//! Supports any provider that exposes the OpenAI `/chat/completions` shape,
//! plus GitHub Copilot which is OpenAI-compatible but requires extra headers.

use anyhow::{Context, Result, anyhow};
use futures_util::StreamExt;
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};

use crate::core::config::{LlmProviderKind, LlmSettings};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChatMessage {
    pub role: String,
    pub content: String,
}

#[derive(Debug, Clone)]
#[allow(dead_code)]
pub struct ChatRequest<'a> {
    pub settings: &'a LlmSettings,
    pub api_key: Option<&'a str>,
    pub messages: &'a [ChatMessage],
    pub stream: bool,
}

/// Convert our settings into the JSON body for `/chat/completions`.
pub fn build_request_body(req: &ChatRequest<'_>) -> Value {
    let messages: Vec<Value> = req
        .messages
        .iter()
        .map(|m| json!({"role": m.role, "content": m.content}))
        .collect();

    json!({
        "model": req.settings.model,
        "messages": messages,
        "temperature": req.settings.temperature,
        "stream": req.stream,
    })
}

/// If the user pasted a bare `http://host:port` (no path), assume the API
/// follows the OpenAI convention and lives under `/v1`. Providers such as
/// LM Studio and Ollama expose `http://localhost:1234/v1/...`, but their UIs
/// often show the bare URL, so normalising here saves a common copy-paste mistake.
fn normalize_openai_base(base_url: &str) -> String {
    let trimmed = base_url.trim_end_matches('/');
    let has_path = trimmed
        .strip_prefix("http://")
        .or_else(|| trimmed.strip_prefix("https://"))
        .map(|rest| rest.contains('/'))
        .unwrap_or(false);
    if has_path {
        trimmed.to_string()
    } else {
        format!("{}/v1", trimmed)
    }
}

/// Build the absolute URL for the chat completions endpoint given a base URL.
///
/// Accepts any of these shapes and always returns `<base>/chat/completions`:
///
/// - `https://api.openai.com/v1`
/// - `https://api.openai.com/v1/`
/// - `https://api.openai.com/v1/chat/completions` (returned as-is, no doubling)
/// - `http://localhost:1234` (normalised to `http://localhost:1234/v1/chat/completions`)
pub fn resolve_chat_url(base_url: &str) -> String {
    let trimmed = base_url.trim_end_matches('/');
    if trimmed.ends_with("/chat/completions") {
        trimmed.to_string()
    } else {
        let base = normalize_openai_base(base_url);
        format!("{}/chat/completions", base.trim_end_matches('/'))
    }
}

/// Build the absolute URL for the `/models` listing endpoint given a base URL.
///
/// Mirrors `resolve_chat_url`: strips any `/chat/completions` suffix the user
/// may have pasted and always returns `<base>/models`.
pub fn resolve_models_url(base_url: &str) -> String {
    let mut trimmed = base_url.trim_end_matches('/').to_string();
    if let Some(stripped) = trimmed.strip_suffix("/chat/completions") {
        trimmed = stripped.trim_end_matches('/').to_string();
    }
    let base = normalize_openai_base(&trimmed);
    let base = base.trim_end_matches('/');
    if base.ends_with("/models") {
        base.to_string()
    } else {
        format!("{}/models", base)
    }
}

/// Build the `Authorization` (or equivalent) headers for a request.
pub fn build_auth_headers(
    settings: &LlmSettings,
    api_key: Option<&str>,
    kind: &LlmProviderKind,
) -> Vec<(String, String)> {
    let mut headers: Vec<(String, String)> = Vec::new();
    let key = api_key.unwrap_or("").to_string();
    match kind {
        LlmProviderKind::OpenAiCompatible => {
            if !key.is_empty() {
                headers.push(("Authorization".into(), format!("Bearer {}", key)));
            }
        }
        LlmProviderKind::GitHubCopilot => {
            if !key.is_empty() {
                headers.push(("Authorization".into(), format!("Bearer {}", key)));
                headers.push(("Editor-Version".into(), "vscode/1.95.0".into()));
                headers.push(("Editor-Plugin-Version".into(), "copilot-chat/0.22.0".into()));
                headers.push(("User-Agent".into(), "DrasticSmurf/0.5.10".into()));
                headers.push(("Copilot-Integration-Id".into(), "vscode-chat".into()));
            }
        }
    }
    for (k, v) in &settings.extra_headers {
        if !k.is_empty() {
            headers.push((k.clone(), v.clone()));
        }
    }
    headers
}

#[allow(dead_code)]
pub async fn chat_complete(
    client: &reqwest::Client,
    settings: &LlmSettings,
    api_key: Option<&str>,
    messages: &[ChatMessage],
) -> Result<String> {
    let url = resolve_chat_url(&settings.base_url);
    let kind = classify_kind(settings);
    let req = ChatRequest {
        settings,
        api_key,
        messages,
        stream: false,
    };
    let body = build_request_body(&req);
    let mut req_builder = client
        .post(&url)
        .header("Content-Type", "application/json")
        .json(&body);
    for (k, v) in build_auth_headers(settings, api_key, &kind) {
        req_builder = req_builder.header(&k, &v);
    }

    let resp = req_builder
        .send()
        .await
        .with_context(|| format!("HTTP request failed to {}", url))?;
    let status = resp.status();
    if !status.is_success() {
        let text = resp.text().await.unwrap_or_default();
        return Err(anyhow!(
            "LLM API returned {} {}: {}",
            status.as_u16(),
            status.canonical_reason().unwrap_or(""),
            truncate(&text, 500)
        ));
    }
    let parsed: Value = resp.json().await.with_context(|| {
        format!("Failed to parse LLM response JSON from {}", url)
    })?;
    let content = parsed
        .get("choices")
        .and_then(|c| c.as_array())
        .and_then(|arr| arr.first())
        .and_then(|c| c.get("message"))
        .and_then(|m| m.get("content"))
        .and_then(|v| v.as_str())
        .ok_or_else(|| anyhow!("LLM response missing choices[0].message.content"))?
        .to_string();
    Ok(content)
}

/// Stream a chat completion, yielding incremental text deltas.
pub async fn chat_stream(
    client: &reqwest::Client,
    settings: &LlmSettings,
    api_key: Option<&str>,
    messages: &[ChatMessage],
    mut on_delta: impl FnMut(&str) + Send,
) -> Result<()> {
    let url = resolve_chat_url(&settings.base_url);
    let kind = classify_kind(settings);
    let req = ChatRequest {
        settings,
        api_key,
        messages,
        stream: true,
    };
    let body = build_request_body(&req);
    let mut req_builder = client
        .post(&url)
        .header("Content-Type", "application/json")
        .header("Accept", "text/event-stream")
        .json(&body);
    for (k, v) in build_auth_headers(settings, api_key, &kind) {
        req_builder = req_builder.header(&k, &v);
    }

    let resp = req_builder
        .send()
        .await
        .with_context(|| format!("HTTP request failed to {}", url))?;
    let status = resp.status();
    if !status.is_success() {
        let text = resp.text().await.unwrap_or_default();
        return Err(anyhow!(
            "LLM API returned {} {}: {}",
            status.as_u16(),
            status.canonical_reason().unwrap_or(""),
            truncate(&text, 500)
        ));
    }

    let mut stream = resp.bytes_stream();
    let mut buffer = String::new();

    while let Some(chunk_res) = stream.next().await {
        let chunk = chunk_res.context("Error reading stream chunk")?;
        buffer.push_str(&String::from_utf8_lossy(&chunk));

        while let Some(idx) = buffer.find("\n\n") {
            let frame: String = buffer.drain(..idx + 2).collect();
            for line in frame.lines() {
                let Some(data) = line.strip_prefix("data:") else {
                    continue;
                };
                let data = data.trim_start();
                if data == "[DONE]" {
                    return Ok(());
                }
                if data.is_empty() {
                    continue;
                }
                match serde_json::from_str::<Value>(data) {
                    Ok(value) => {
                        if let Some(delta) = extract_delta(&value)
                            && !delta.is_empty()
                        {
                            on_delta(&delta);
                        }
                    }
                    Err(_err) => {
                        // Malformed JSON: usually means a frame is split
                        // across byte boundaries, so wait for more data.
                    }
                }
            }
        }
    }

    Ok(())
}

/// List available models from the provider's `/models` endpoint.
///
/// Works with any OpenAI-compatible provider that exposes
/// `GET <base>/models` returning `{"data":[{"id":"..."},...]}`.
/// Also accepts a bare top-level array (some local servers).
pub async fn list_models(
    client: &reqwest::Client,
    settings: &LlmSettings,
    api_key: Option<&str>,
) -> Result<Vec<String>> {
    let url = resolve_models_url(&settings.base_url);
    let kind = classify_kind(settings);
    let mut req_builder = client.get(&url).header("Accept", "application/json");
    for (k, v) in build_auth_headers(settings, api_key, &kind) {
        req_builder = req_builder.header(&k, &v);
    }

    let resp = req_builder
        .send()
        .await
        .with_context(|| format!("HTTP request failed to {}", url))?;
    let status = resp.status();
    if !status.is_success() {
        let text = resp.text().await.unwrap_or_default();
        return Err(anyhow!(
            "{} {}: {}",
            status.as_u16(),
            status.canonical_reason().unwrap_or(""),
            truncate(&text, 200)
        ));
    }
    let text = resp
        .text()
        .await
        .with_context(|| format!("Failed to read models response body from {}", url))?;
    if text.trim().is_empty() {
        return Err(anyhow!("Models response body from {} was empty", url));
    }
    let parsed: Value = serde_json::from_str(&text).with_context(|| {
        format!(
            "Failed to parse models response JSON from {}: {}",
            url,
            truncate(&text, 500)
        )
    })?;

    let array = parsed
        .get("data")
        .and_then(|v| v.as_array())
        .or_else(|| parsed.as_array())
        .ok_or_else(|| anyhow!("Models response missing 'data' array"))?;

    let mut ids: Vec<String> = array
        .iter()
        .filter_map(|m| {
            m.get("id")
                .and_then(|v| v.as_str())
                .map(|s| s.to_string())
        })
        .collect();
    ids.sort();
    ids.dedup();
    Ok(ids)
}

fn extract_delta(value: &Value) -> Option<String> {
    if let Some(s) = value
        .pointer("/choices/0/delta/content")
        .and_then(|v| v.as_str())
    {
        return Some(s.to_string());
    }
    if let Some(s) = value
        .pointer("/choices/0/text")
        .and_then(|v| v.as_str())
    {
        return Some(s.to_string());
    }
    None
}

fn classify_kind(settings: &LlmSettings) -> LlmProviderKind {
    crate::core::config::default_llm_providers()
        .into_iter()
        .find(|p| p.id == settings.provider_id)
        .map(|p| p.kind)
        .unwrap_or_default()
}

fn truncate(s: &str, max: usize) -> String {
    if s.len() <= max {
        s.to_string()
    } else {
        let mut t = s[..max].to_string();
        t.push_str("...[truncated]");
        t
    }
}

/// Parse a chunk of SSE bytes into zero or more `data:` payloads.
///
/// Splits on `\n\n` and returns the trimmed payload of each `data:` line.
/// Empty payloads and the `[DONE]` sentinel are filtered out.
#[allow(dead_code)]
pub fn parse_sse_chunk(chunk: &str) -> Vec<String> {
    let mut out = Vec::new();
    for frame in chunk.split("\n\n") {
        for line in frame.lines() {
            let Some(data) = line.strip_prefix("data:") else {
                continue;
            };
            let data = data.trim_start().trim();
            if data.is_empty() || data == "[DONE]" {
                continue;
            }
            out.push(data.to_string());
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn resolves_openai_base() {
        assert_eq!(
            resolve_chat_url("https://api.openai.com/v1"),
            "https://api.openai.com/v1/chat/completions"
        );
        assert_eq!(
            resolve_chat_url("https://api.openai.com/v1/"),
            "https://api.openai.com/v1/chat/completions"
        );
        assert_eq!(
            resolve_chat_url("https://api.openai.com/v1/chat/completions"),
            "https://api.openai.com/v1/chat/completions"
        );
    }

    #[test]
    fn resolves_local_base() {
        assert_eq!(
            resolve_chat_url("http://localhost:11434/v1"),
            "http://localhost:11434/v1/chat/completions"
        );
    }

    #[test]
    fn resolves_models_url_variants() {
        // Standard OpenAI-style base.
        assert_eq!(
            resolve_models_url("https://api.openai.com/v1"),
            "https://api.openai.com/v1/models"
        );
        // Trailing slash.
        assert_eq!(
            resolve_models_url("https://api.openai.com/v1/"),
            "https://api.openai.com/v1/models"
        );
        // User pasted the chat endpoint by mistake — strip it.
        assert_eq!(
            resolve_models_url("https://api.openai.com/v1/chat/completions"),
            "https://api.openai.com/v1/models"
        );
        // Already the models endpoint — leave it alone.
        assert_eq!(
            resolve_models_url("https://api.openai.com/v1/models"),
            "https://api.openai.com/v1/models"
        );
        // Local LM Studio / Ollama style with /v1 included.
        assert_eq!(
            resolve_models_url("http://localhost:1234/v1"),
            "http://localhost:1234/v1/models"
        );
        // Bare host:port (common LM Studio copy-paste) gets /v1 inserted.
        assert_eq!(
            resolve_models_url("http://192.168.1.66:1234"),
            "http://192.168.1.66:1234/v1/models"
        );
        assert_eq!(
            resolve_models_url("http://localhost:11434/"),
            "http://localhost:11434/v1/models"
        );
        // Custom path is respected and not normalised.
        assert_eq!(
            resolve_models_url("https://myresource.openai.azure.com/openai/deployments/mydeployment"),
            "https://myresource.openai.azure.com/openai/deployments/mydeployment/models"
        );
    }

    #[test]
    fn resolves_chat_url_normalises_bare_host() {
        assert_eq!(
            resolve_chat_url("http://192.168.1.66:1234"),
            "http://192.168.1.66:1234/v1/chat/completions"
        );
        assert_eq!(
            resolve_chat_url("http://localhost:11434/"),
            "http://localhost:11434/v1/chat/completions"
        );
        assert_eq!(
            resolve_chat_url("https://api.openai.com/v1"),
            "https://api.openai.com/v1/chat/completions"
        );
        assert_eq!(
            resolve_chat_url("https://api.openai.com/v1/chat/completions"),
            "https://api.openai.com/v1/chat/completions"
        );
    }

    #[test]
    fn openai_headers_have_bearer() {
        let s = LlmSettings {
            enabled: true,
            provider_id: "openai".into(),
            base_url: "https://api.openai.com/v1".into(),
            model: "gpt-4o-mini".into(),
            temperature: 0.3,
            max_context_chars: 4000,
            auto_cluster_context: true,
            extra_headers: vec![],
        };
        let h = build_auth_headers(&s, Some("sk-test"), &LlmProviderKind::OpenAiCompatible);
        assert!(h.iter().any(|(k, v)| k == "Authorization" && v == "Bearer sk-test"));
    }

    #[test]
    fn copilot_headers_include_editor() {
        let s = LlmSettings {
            enabled: true,
            provider_id: "github_copilot".into(),
            base_url: "https://api.githubcopilot.com".into(),
            model: "gpt-4o".into(),
            temperature: 0.3,
            max_context_chars: 4000,
            auto_cluster_context: true,
            extra_headers: vec![],
        };
        let h = build_auth_headers(&s, Some("ghu_x"), &LlmProviderKind::GitHubCopilot);
        assert!(h.iter().any(|(k, v)| k == "Authorization" && v == "Bearer ghu_x"));
        assert!(h.iter().any(|(k, _)| k == "Editor-Version"));
        assert!(h.iter().any(|(k, _)| k == "Copilot-Integration-Id"));
    }

    #[test]
    fn request_body_contains_messages() {
        let s = LlmSettings::default();
        let msgs = vec![
            ChatMessage { role: "system".into(), content: "You are helpful".into() },
            ChatMessage { role: "user".into(), content: "Hi".into() },
        ];
        let req = ChatRequest { settings: &s, api_key: None, messages: &msgs, stream: true };
        let body = build_request_body(&req);
        assert_eq!(body["model"], "gpt-4o-mini");
        assert_eq!(body["stream"], true);
        assert_eq!(body["messages"].as_array().unwrap().len(), 2);
        assert_eq!(body["messages"][1]["role"], "user");
        assert_eq!(body["messages"][1]["content"], "Hi");
    }

    #[test]
    fn parses_multi_event_chunk() {
        let chunk = "data: {\"choices\":[{\"delta\":{\"content\":\"Hel\"}}]}\n\ndata: {\"choices\":[{\"delta\":{\"content\":\"lo\"}}]}\n\ndata: [DONE]\n\n";
        let payloads = parse_sse_chunk(chunk);
        assert_eq!(payloads.len(), 2);
        assert!(payloads[0].contains("Hel"));
        assert!(payloads[1].contains("lo"));
    }

    #[test]
    fn skips_empty_and_done_events() {
        let chunk = "data:\n\ndata: [DONE]\n\n";
        assert!(parse_sse_chunk(chunk).is_empty());
    }

    #[test]
    fn extract_delta_uses_message_content() {
        let v = serde_json::json!({
            "choices": [{"delta": {"content": "abc"}}]
        });
        assert_eq!(extract_delta(&v).as_deref(), Some("abc"));
    }

    #[test]
    fn extract_delta_falls_back_to_text() {
        let v = serde_json::json!({
            "choices": [{"text": "fallback"}]
        });
        assert_eq!(extract_delta(&v).as_deref(), Some("fallback"));
    }

    #[test]
    fn extract_delta_returns_none_when_missing() {
        let v = serde_json::json!({"choices": [{}]});
        assert_eq!(extract_delta(&v), None);
    }

    #[test]
    fn truncate_limits_length() {
        assert_eq!(truncate("hi", 10), "hi");
        let long = "x".repeat(600);
        let t = truncate(&long, 100);
        assert!(t.len() <= 120);
        assert!(t.contains("[truncated]"));
    }
}