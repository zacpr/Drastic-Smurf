//! LLM HTTP client with SSE streaming for OpenAI-compatible endpoints.
//!
//! Supports any provider that exposes the OpenAI `/chat/completions` shape,
//! plus GitHub Copilot which is OpenAI-compatible but requires extra headers.

use anyhow::{Context, Result, anyhow};
use futures_util::StreamExt;
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};

use crate::core::config::{LlmProviderKind, LlmSettings};

/// A function call the model asked for (OpenAI `tool_calls` shape).
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct ToolCall {
    pub id: String,
    pub name: String,
    /// JSON-encoded arguments, exactly as the model produced them.
    pub arguments: String,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct ChatMessage {
    pub role: String,
    pub content: String,
    /// Tool calls made by an `assistant` message.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub tool_calls: Vec<ToolCall>,
    /// For a `tool` message: the call it answers.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tool_call_id: Option<String>,
}

#[derive(Debug, Clone)]
#[allow(dead_code)]
pub struct ChatRequest<'a> {
    pub settings: &'a LlmSettings,
    pub api_key: Option<&'a str>,
    pub messages: &'a [ChatMessage],
    pub stream: bool,
    /// OpenAI `tools` array; `None` sends no tools.
    pub tools: Option<&'a Value>,
    /// Send `settings.temperature`. Off for models that only accept their default
    /// (OpenAI reasoning models reject anything else with a 400).
    pub send_temperature: bool,
}

fn message_json(m: &ChatMessage) -> Value {
    let mut v = json!({"role": m.role, "content": m.content});
    if !m.tool_calls.is_empty() {
        v["tool_calls"] = m
            .tool_calls
            .iter()
            .map(|c| {
                json!({
                    "id": c.id,
                    "type": "function",
                    "function": {"name": c.name, "arguments": c.arguments},
                })
            })
            .collect();
    }
    if let Some(id) = &m.tool_call_id {
        v["tool_call_id"] = json!(id);
    }
    v
}

/// Convert our settings into the JSON body for `/chat/completions`.
pub fn build_request_body(req: &ChatRequest<'_>) -> Value {
    let messages: Vec<Value> = req.messages.iter().map(message_json).collect();

    let mut body = json!({
        "model": req.settings.model,
        "messages": messages,
        "stream": req.stream,
    });
    if req.send_temperature {
        body["temperature"] = json!(req.settings.temperature);
    }
    if let Some(tools) = req.tools
        && tools.as_array().is_some_and(|t| !t.is_empty())
    {
        body["tools"] = tools.clone();
    }
    body
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
        tools: None,
        send_temperature: true,
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
        return Err(anyhow!(describe_api_error(status, &text)));
    }
    let parsed: Value = resp
        .json()
        .await
        .with_context(|| format!("Failed to parse LLM response JSON from {}", url))?;
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

/// One incremental piece of a streamed reply.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StreamDelta<'a> {
    /// Answer text (`delta.content`).
    Content(&'a str),
    /// Reasoning ("thinking") text that reasoning models stream before the answer
    /// (`delta.reasoning_content` in LM Studio/DeepSeek/vLLM, `delta.reasoning` in
    /// Ollama/OpenRouter).
    Reasoning(&'a str),
}

/// How a stream ended.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct StreamOutcome {
    /// The provider's `finish_reason` (`stop`, `length`, `tool_calls`, ...), when it sent one.
    pub finish_reason: Option<String>,
    /// Tool calls assembled from the streamed fragments, in call order.
    pub tool_calls: Vec<ToolCall>,
}

/// Merge one chunk's `delta.tool_calls` fragments into `calls`. The id and name
/// arrive in the first fragment for each `index`; `arguments` arrives in pieces.
fn accumulate_tool_calls(value: &Value, calls: &mut Vec<ToolCall>) {
    let Some(fragments) = value
        .pointer("/choices/0/delta/tool_calls")
        .or_else(|| value.pointer("/choices/0/message/tool_calls"))
        .and_then(|v| v.as_array())
    else {
        return;
    };
    for (pos, fragment) in fragments.iter().enumerate() {
        let index = fragment
            .get("index")
            .and_then(|i| i.as_u64())
            .map(|i| i as usize)
            .unwrap_or(pos);
        while calls.len() <= index {
            calls.push(ToolCall::default());
        }
        let call = &mut calls[index];
        if let Some(id) = fragment.get("id").and_then(|v| v.as_str())
            && !id.is_empty()
        {
            call.id = id.to_string();
        }
        if let Some(function) = fragment.get("function") {
            if let Some(name) = function.get("name").and_then(|v| v.as_str()) {
                call.name.push_str(name);
            }
            match function.get("arguments") {
                Some(Value::String(args)) => call.arguments.push_str(args),
                // Some servers send the arguments as an object rather than a string.
                Some(args @ Value::Object(_)) => call.arguments = args.to_string(),
                _ => {}
            }
        }
    }
}

/// Incremental SSE parser. Buffers raw bytes so multi-byte UTF-8 characters split
/// across network chunks survive, and accepts both `\n\n` and `\r\n\r\n` framing.
#[derive(Debug, Default)]
pub struct SseParser {
    buffer: Vec<u8>,
}

/// One parsed SSE event.
#[derive(Debug, Clone, PartialEq)]
pub enum SseEvent {
    Data(Value),
    Done,
}

impl SseParser {
    /// Feed bytes; returns every complete event.
    pub fn push(&mut self, bytes: &[u8]) -> Vec<SseEvent> {
        self.buffer.extend_from_slice(bytes);
        let mut events = Vec::new();
        while let Some((end, sep)) = find_frame_end(&self.buffer) {
            let frame: Vec<u8> = self.buffer.drain(..end + sep).collect();
            parse_frame(&String::from_utf8_lossy(&frame[..end]), &mut events);
        }
        events
    }

    /// Flush a final frame the server didn't terminate with a blank line.
    pub fn finish(&mut self) -> Vec<SseEvent> {
        let rest = std::mem::take(&mut self.buffer);
        let mut events = Vec::new();
        parse_frame(&String::from_utf8_lossy(&rest), &mut events);
        events
    }
}

fn find_frame_end(buf: &[u8]) -> Option<(usize, usize)> {
    let lf = buf.windows(2).position(|w| w == b"\n\n").map(|i| (i, 2));
    let crlf = buf
        .windows(4)
        .position(|w| w == b"\r\n\r\n")
        .map(|i| (i, 4));
    match (lf, crlf) {
        (Some(a), Some(b)) => Some(if a.0 <= b.0 { a } else { b }),
        (a, b) => a.or(b),
    }
}

fn parse_frame(frame: &str, events: &mut Vec<SseEvent>) {
    // Multiple `data:` lines in one event are joined with newlines (SSE spec).
    let data: Vec<&str> = frame
        .lines()
        .filter_map(|line| line.trim_end_matches('\r').strip_prefix("data:"))
        .map(|d| d.strip_prefix(' ').unwrap_or(d))
        .collect();
    if data.is_empty() {
        return;
    }
    let payload = data.join("\n");
    let payload = payload.trim();
    if payload == "[DONE]" {
        events.push(SseEvent::Done);
    } else if !payload.is_empty() {
        match serde_json::from_str::<Value>(payload) {
            Ok(value) => events.push(SseEvent::Data(value)),
            Err(e) => tracing::debug!("Skipping unparseable SSE payload: {}", e),
        }
    }
}

/// Plain-language explanation for quota / billing failures, which providers report
/// with codes like `insufficient_quota` rather than a readable message.
fn quota_hint(body: &str) -> Option<&'static str> {
    let lower = body.to_lowercase();
    [
        "insufficient_quota",
        "credit_balance",
        "no credits",
        "exceeded your current quota",
        "billing",
    ]
    .iter()
    .any(|k| lower.contains(k))
    .then_some(
        "Your provider account is out of credits or quota. Add credits or check billing \
             with the provider, then try again",
    )
}

/// Turn a failed HTTP response into a readable error: a plain-language lead for the
/// common cases (quota, rate limit, bad key), then the provider's own message.
pub fn describe_api_error(status: reqwest::StatusCode, body: &str) -> String {
    let detail = serde_json::from_str::<Value>(body)
        .ok()
        .and_then(|v| stream_error(&v))
        .unwrap_or_else(|| body.trim().to_string());
    let lead = match (status.as_u16(), quota_hint(body)) {
        (_, Some(hint)) => Some(hint.to_string()),
        (429, None) => {
            Some("Rate-limited by the provider (429). Wait a moment and try again".to_string())
        }
        (401 | 403, None) => Some(format!(
            "The provider rejected the API key ({}). Check the key in settings",
            status.as_u16()
        )),
        _ => None,
    };
    let code = format!(
        "{} {}",
        status.as_u16(),
        status.canonical_reason().unwrap_or("")
    );
    match lead {
        Some(lead) => format!("{}. [{}] {}", lead, code.trim(), truncate(&detail, 500)),
        None => format!(
            "LLM API returned {}: {}",
            code.trim(),
            truncate(&detail, 500)
        ),
    }
}

/// Whether a failed request was rejected because of the `temperature` parameter
/// (e.g. "Unsupported value: 'temperature' does not support 0.3 with this model").
pub fn is_temperature_rejection(error: &str) -> bool {
    error.to_lowercase().contains("temperature")
}

/// An error object sent inside the stream (LM Studio, OpenRouter and others report
/// mid-stream failures this way with HTTP 200).
fn stream_error(value: &Value) -> Option<String> {
    let err = value.get("error")?;
    Some(
        err.get("message")
            .and_then(|m| m.as_str())
            .map(|m| m.to_string())
            .unwrap_or_else(|| err.to_string()),
    )
}

/// Stream a chat completion, reporting answer and reasoning deltas as they arrive.
pub async fn chat_stream(
    client: &reqwest::Client,
    settings: &LlmSettings,
    api_key: Option<&str>,
    messages: &[ChatMessage],
    tools: Option<&Value>,
    send_temperature: bool,
    mut on_delta: impl FnMut(StreamDelta<'_>) + Send,
) -> Result<StreamOutcome> {
    let url = resolve_chat_url(&settings.base_url);
    let kind = classify_kind(settings);
    let req = ChatRequest {
        settings,
        api_key,
        messages,
        stream: true,
        tools,
        send_temperature,
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
        return Err(anyhow!(describe_api_error(status, &text)));
    }

    let mut outcome = StreamOutcome::default();

    let mut handle = |events: Vec<SseEvent>, outcome: &mut StreamOutcome| -> Result<bool> {
        for event in events {
            let value = match event {
                SseEvent::Done => return Ok(true),
                SseEvent::Data(value) => value,
            };
            if let Some(message) = stream_error(&value) {
                let message = match quota_hint(&value.to_string()) {
                    Some(hint) => format!("{} ({})", hint, message),
                    None => message,
                };
                return Err(anyhow!("LLM stream error: {}", truncate(&message, 500)));
            }
            if let Some(reason) = value
                .pointer("/choices/0/finish_reason")
                .and_then(|v| v.as_str())
            {
                outcome.finish_reason = Some(reason.to_string());
            }
            accumulate_tool_calls(&value, &mut outcome.tool_calls);
            if let Some(text) = extract_reasoning(&value)
                && !text.is_empty()
            {
                on_delta(StreamDelta::Reasoning(&text));
            }
            if let Some(text) = extract_delta(&value)
                && !text.is_empty()
            {
                on_delta(StreamDelta::Content(&text));
            }
        }
        Ok(false)
    };

    let is_json = resp
        .headers()
        .get(reqwest::header::CONTENT_TYPE)
        .and_then(|v| v.to_str().ok())
        .is_some_and(|v| {
            v.split(';')
                .next()
                .unwrap_or_default()
                .trim()
                .eq_ignore_ascii_case("application/json")
        });
    if is_json {
        let value = resp
            .json::<Value>()
            .await
            .context("Failed to parse LLM completion JSON")?;
        handle(vec![SseEvent::Data(value)], &mut outcome)?;
    } else {
        let mut stream = resp.bytes_stream();
        let mut parser = SseParser::default();
        let mut done = false;
        while let Some(chunk_res) = stream.next().await {
            let chunk = chunk_res.context("Error reading the response stream")?;
            if handle(parser.push(&chunk), &mut outcome)? {
                done = true;
                break;
            }
        }
        if !done {
            handle(parser.finish(), &mut outcome)?;
        }
    }
    // Drop fragments that never got a name; give id-less calls a stable id so the
    // tool result can be matched back to them.
    outcome.tool_calls.retain(|c| !c.name.trim().is_empty());
    static NEXT_CALL_BATCH: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(1);
    let batch = NEXT_CALL_BATCH.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
    for (i, call) in outcome.tool_calls.iter_mut().enumerate() {
        if call.id.is_empty() {
            call.id = format!("call_smurf_{}_{}", batch, i);
        }
    }
    Ok(outcome)
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
        .filter_map(|m| m.get("id").and_then(|v| v.as_str()).map(|s| s.to_string()))
        .collect();
    ids.sort();
    ids.dedup();
    Ok(ids)
}

fn extract_delta(value: &Value) -> Option<String> {
    if let Some(s) = value
        .pointer("/choices/0/delta/content")
        .or_else(|| value.pointer("/choices/0/message/content"))
        .and_then(|v| v.as_str())
    {
        return Some(s.to_string());
    }
    if let Some(s) = value.pointer("/choices/0/text").and_then(|v| v.as_str()) {
        return Some(s.to_string());
    }
    None
}

fn extract_reasoning(value: &Value) -> Option<String> {
    [
        "/choices/0/delta/reasoning_content",
        "/choices/0/delta/reasoning",
    ]
    .iter()
    .find_map(|p| value.pointer(p).and_then(|v| v.as_str()))
    .map(|s| s.to_string())
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
        let mut cut = max;
        while !s.is_char_boundary(cut) {
            cut -= 1;
        }
        let mut t = s[..cut].to_string();
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

    #[tokio::test]
    async fn chat_stream_accepts_json_answers_and_tool_calls() {
        use tokio::io::{AsyncReadExt, AsyncWriteExt};
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let base_url = format!("http://{}", listener.local_addr().unwrap());
        let server = tokio::spawn(async move {
            for _ in 0..2 {
                let (mut socket, _) = listener.accept().await.unwrap();
                let mut request = [0_u8; 8192];
                let received = socket.read(&mut request).await.unwrap();
                assert!(received > 0, "expected an HTTP request");
                let body = json!({"choices":[{"message":{"content":"Hello!", "tool_calls":[{"function":{"name":"list_clusters", "arguments":"{}"}}]}, "finish_reason":"tool_calls"}]}).to_string();
                let response = format!(
                    "HTTP/1.1 200 OK\r\nContent-Type: application/json; charset=utf-8\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
                    body.len(),
                    body
                );
                socket.write_all(response.as_bytes()).await.unwrap();
            }
        });
        let settings = LlmSettings {
            base_url,
            ..Default::default()
        };
        let client = reqwest::Client::builder()
            .no_proxy()
            .timeout(std::time::Duration::from_secs(3))
            .build()
            .unwrap();
        let messages = vec![ChatMessage {
            role: "user".into(),
            content: "Hello".into(),
            ..Default::default()
        }];
        let mut ids = Vec::new();
        for _ in 0..2 {
            let mut answer = String::new();
            let outcome = chat_stream(&client, &settings, None, &messages, None, true, |delta| {
                if let StreamDelta::Content(text) = delta {
                    answer.push_str(text);
                }
            })
            .await
            .unwrap();
            assert_eq!(answer, "Hello!");
            assert_eq!(outcome.finish_reason.as_deref(), Some("tool_calls"));
            assert_eq!(outcome.tool_calls[0].name, "list_clusters");
            ids.push(outcome.tool_calls[0].id.clone());
        }
        assert_ne!(ids[0], ids[1]);
        server.await.unwrap();
    }

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
            resolve_models_url(
                "https://myresource.openai.azure.com/openai/deployments/mydeployment"
            ),
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
            ..Default::default()
        };
        let h = build_auth_headers(&s, Some("sk-test"), &LlmProviderKind::OpenAiCompatible);
        assert!(
            h.iter()
                .any(|(k, v)| k == "Authorization" && v == "Bearer sk-test")
        );
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
            ..Default::default()
        };
        let h = build_auth_headers(&s, Some("ghu_x"), &LlmProviderKind::GitHubCopilot);
        assert!(
            h.iter()
                .any(|(k, v)| k == "Authorization" && v == "Bearer ghu_x")
        );
        assert!(h.iter().any(|(k, _)| k == "Editor-Version"));
        assert!(h.iter().any(|(k, _)| k == "Copilot-Integration-Id"));
    }

    #[test]
    fn request_body_contains_messages() {
        let s = LlmSettings::default();
        let msgs = vec![
            ChatMessage {
                role: "system".into(),
                content: "You are helpful".into(),
                ..Default::default()
            },
            ChatMessage {
                role: "user".into(),
                content: "Hi".into(),
                ..Default::default()
            },
        ];
        let req = ChatRequest {
            settings: &s,
            api_key: None,
            messages: &msgs,
            stream: true,
            tools: None,
            send_temperature: true,
        };
        let body = build_request_body(&req);
        assert!(body.get("temperature").is_some());
        let body_no_temp = build_request_body(&ChatRequest {
            send_temperature: false,
            ..req.clone()
        });
        assert!(body_no_temp.get("temperature").is_none());
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
    fn sse_parser_handles_split_frames_crlf_and_utf8() {
        let mut p = SseParser::default();
        let frame = "data: {\"choices\":[{\"delta\":{\"content\":\"héllo\"}}]}\n\n";
        let bytes = frame.as_bytes();
        // Split inside the two-byte 'é'.
        let cut = frame.find('é').unwrap() + 1;
        assert!(p.push(&bytes[..cut]).is_empty());
        let events = p.push(&bytes[cut..]);
        assert_eq!(events.len(), 1);
        let SseEvent::Data(v) = &events[0] else {
            panic!()
        };
        assert_eq!(extract_delta(v).as_deref(), Some("héllo"));

        let events = p.push(b"data: {\"a\":1}\r\n\r\ndata: [DONE]\r\n\r\n");
        assert_eq!(events.len(), 2);
        assert_eq!(events[1], SseEvent::Done);

        // Unterminated last frame is flushed by finish().
        assert!(p.push(b"data: {\"b\":2}").is_empty());
        assert_eq!(p.finish().len(), 1);
    }

    #[test]
    fn extracts_reasoning_and_stream_errors() {
        let v = serde_json::json!({"choices": [{"delta": {"reasoning_content": "hmm"}}]});
        assert_eq!(extract_reasoning(&v).as_deref(), Some("hmm"));
        assert_eq!(extract_delta(&v), None);
        let v = serde_json::json!({"choices": [{"delta": {"reasoning": "ollama"}}]});
        assert_eq!(extract_reasoning(&v).as_deref(), Some("ollama"));
        let v = serde_json::json!({"error": {"message": "Context length exceeded"}});
        assert_eq!(stream_error(&v).as_deref(), Some("Context length exceeded"));
        let v = serde_json::json!({"error": "boom"});
        assert_eq!(stream_error(&v).as_deref(), Some("\"boom\""));
    }

    #[test]
    fn accumulates_streamed_tool_calls() {
        // LM Studio's shape: id and name first, then argument fragments.
        let chunks = [
            serde_json::json!({"choices":[{"delta":{"tool_calls":[{"index":0,"id":"abc","type":"function","function":{"name":"es_request","arguments":""}}]}}]}),
            serde_json::json!({"choices":[{"delta":{"tool_calls":[{"index":0,"function":{"arguments":"{\"cluster\":\"a\","}}]}}]}),
            serde_json::json!({"choices":[{"delta":{"tool_calls":[{"index":0,"function":{"arguments":"\"path\":\"/\"}"}}]}}]}),
            serde_json::json!({"choices":[{"delta":{"tool_calls":[{"index":1,"id":"def","function":{"name":"list_clusters","arguments":{}}}]}}]}),
        ];
        let mut calls = Vec::new();
        for c in &chunks {
            accumulate_tool_calls(c, &mut calls);
        }
        assert_eq!(calls.len(), 2);
        assert_eq!(calls[0].id, "abc");
        assert_eq!(calls[0].name, "es_request");
        assert_eq!(calls[0].arguments, "{\"cluster\":\"a\",\"path\":\"/\"}");
        assert_eq!(calls[1].name, "list_clusters");
        assert_eq!(calls[1].arguments, "{}");
    }

    #[test]
    fn request_body_serialises_tool_turns() {
        let s = LlmSettings::default();
        let msgs = vec![
            ChatMessage {
                role: "assistant".into(),
                tool_calls: vec![ToolCall {
                    id: "abc".into(),
                    name: "list_clusters".into(),
                    arguments: "{}".into(),
                }],
                ..Default::default()
            },
            ChatMessage {
                role: "tool".into(),
                content: "[]".into(),
                tool_call_id: Some("abc".into()),
                ..Default::default()
            },
        ];
        let tools =
            serde_json::json!([{"type": "function", "function": {"name": "list_clusters"}}]);
        let req = ChatRequest {
            settings: &s,
            api_key: None,
            messages: &msgs,
            stream: true,
            tools: Some(&tools),
            send_temperature: true,
        };
        let body = build_request_body(&req);
        assert_eq!(body["tools"][0]["function"]["name"], "list_clusters");
        assert_eq!(body["messages"][0]["tool_calls"][0]["id"], "abc");
        assert_eq!(body["messages"][0]["tool_calls"][0]["type"], "function");
        assert_eq!(body["messages"][1]["tool_call_id"], "abc");
        assert!(body["messages"][1].get("tool_calls").is_none());
    }

    #[test]
    fn describes_quota_rate_limit_and_auth_errors() {
        use reqwest::StatusCode;
        let quota = r#"{"error":{"message":"You have no credits remaining.","type":"insufficient_quota","code":"credit_balance_exhausted"}}"#;
        let e = describe_api_error(StatusCode::TOO_MANY_REQUESTS, quota);
        assert!(
            e.starts_with("Your provider account is out of credits"),
            "{e}"
        );
        assert!(e.contains("You have no credits remaining."));
        assert!(
            !e.contains("\"type\""),
            "raw JSON should be reduced to the message: {e}"
        );

        let e = describe_api_error(
            StatusCode::TOO_MANY_REQUESTS,
            r#"{"error":{"message":"slow down"}}"#,
        );
        assert!(e.starts_with("Rate-limited"), "{e}");

        let e = describe_api_error(StatusCode::UNAUTHORIZED, "bad key");
        assert!(e.contains("rejected the API key (401)"), "{e}");

        let e = describe_api_error(
            StatusCode::BAD_REQUEST,
            r#"{"error":{"message":"Unsupported value: 'temperature' does not support 0.3 with this model."}}"#,
        );
        assert!(
            e.starts_with("LLM API returned 400 Bad Request: Unsupported value"),
            "{e}"
        );
        assert!(is_temperature_rejection(&e));
        assert!(!is_temperature_rejection("model does not support tools"));
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
