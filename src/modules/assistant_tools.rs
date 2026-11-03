//! Tools the AI Assistant can call to act inside the running app (OpenAI tool calling).
//!
//! The model asks for a tool; [`parse_call`] turns the call into a [`ToolAction`]; the
//! host runs it (after the user approves, for anything that could change a cluster)
//! and the result goes back to the model as a `tool` message. Everything here is pure
//! so it can be tested without a UI or network.

use serde_json::{Value, json};

use crate::core::llm::ToolCall;

/// Tool rounds allowed per user message before the assistant stops and reports back.
pub const MAX_TOOL_STEPS: usize = 8;
/// Longest tool result sent back to the model.
pub const MAX_RESULT_CHARS: usize = 6000;

/// Tabs the assistant may open, as (tool argument, display name).
pub const TABS: &[(&str, &str)] = &[
    ("clusters", "Clusters"),
    ("snapshot", "Snapshot"),
    ("status", "Status"),
    ("dashboard", "Dashboard"),
    ("tasks", "Tasks"),
    ("console", "Console"),
    ("god_console", "God-mode Console"),
    ("discover", "Discover"),
    ("indices", "Indices"),
    ("observability", "Observability"),
    ("osquery", "Osquery"),
    ("pipelines", "Pipelines"),
    ("logs", "Logs"),
    ("painless", "Painless Playground"),
    ("settings", "Settings"),
];

const METHODS: &[&str] = &["GET", "POST", "PUT", "DELETE", "HEAD"];

/// The `tools` array sent with every request while tools are enabled.
pub fn tool_specs() -> Value {
    let request_props = json!({
        "cluster": {"type": "string", "description": "Cluster name exactly as configured in the app."},
        "method": {"type": "string", "enum": METHODS},
        "path": {"type": "string", "description": "Request path starting with '/', with any query string, e.g. /_cluster/health or /_cat/indices?v&s=index"},
        "body": {"type": "string", "description": "Optional JSON request body, as a string."},
        "kibana": {"type": "boolean", "description": "Send to the cluster's Kibana host instead of Elasticsearch (for /api/... Kibana APIs). Default false."}
    });
    json!([
        {
            "type": "function",
            "function": {
                "name": "list_clusters",
                "description": "List the Elasticsearch clusters configured in Drastic Smurf, with health, version and whether a Kibana host is set.",
                "parameters": {"type": "object", "properties": {}}
            }
        },
        {
            "type": "function",
            "function": {
                "name": "es_request",
                "description": "Send a request to a cluster's Elasticsearch (or Kibana) API and get the response. Use GET to look things up instead of guessing. Any request that is not GET/HEAD is shown to the user, who must approve it before it runs.",
                "parameters": {
                    "type": "object",
                    "properties": request_props.clone(),
                    "required": ["cluster", "method", "path"]
                }
            }
        },
        {
            "type": "function",
            "function": {
                "name": "get_console",
                "description": "Read the request and response currently in the app's Console tab (what the user is looking at).",
                "parameters": {"type": "object", "properties": {}}
            }
        },
        {
            "type": "function",
            "function": {
                "name": "open_in_console",
                "description": "Load a request into the app's Console tab and switch to it, without sending it, so the user can review, edit and send it themselves. Prefer this for anything the user should check first.",
                "parameters": {
                    "type": "object",
                    "properties": request_props,
                    "required": ["method", "path"]
                }
            }
        },
        {
            "type": "function",
            "function": {
                "name": "open_tab",
                "description": "Switch the app to one of its tabs.",
                "parameters": {
                    "type": "object",
                    "properties": {
                        "tab": {"type": "string", "enum": TABS.iter().map(|(id, _)| *id).collect::<Vec<_>>()}
                    },
                    "required": ["tab"]
                }
            }
        }
    ])
}

/// A request the assistant wants to send or load into the Console.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ToolRequest {
    /// Empty for `open_in_console` without a cluster (keep the Console's selection).
    pub cluster: String,
    pub method: String,
    pub path: String,
    pub body: Option<String>,
    pub kibana: bool,
}

impl ToolRequest {
    pub fn is_read_only(&self) -> bool {
        matches!(self.method.as_str(), "GET" | "HEAD")
    }

    pub fn summary(&self) -> String {
        let target = if self.cluster.is_empty() {
            String::new()
        } else if self.kibana {
            format!(" on {} (Kibana)", self.cluster)
        } else {
            format!(" on {}", self.cluster)
        };
        format!("{} {}{}", self.method, self.path, target)
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ToolAction {
    ListClusters,
    EsRequest(ToolRequest),
    GetConsole,
    OpenInConsole(ToolRequest),
    /// Tool argument id from [`TABS`].
    OpenTab(String),
}

impl ToolAction {
    /// Whether the user must approve this before it runs. Only cluster requests that
    /// can change something ask; looking at things and moving around the UI don't.
    pub fn needs_approval(&self, auto_run_reads: bool) -> bool {
        match self {
            ToolAction::EsRequest(req) => !(req.is_read_only() && auto_run_reads),
            _ => false,
        }
    }

    /// One-line description for the chat.
    pub fn summary(&self) -> String {
        match self {
            ToolAction::ListClusters => "List clusters".to_string(),
            ToolAction::EsRequest(req) => req.summary(),
            ToolAction::GetConsole => "Read the Console".to_string(),
            ToolAction::OpenInConsole(req) => format!("Load into Console: {}", req.summary()),
            ToolAction::OpenTab(tab) => format!("Open the {} tab", tab_label(tab)),
        }
    }
}

pub fn tab_label(id: &str) -> &str {
    TABS.iter()
        .find(|(t, _)| *t == id)
        .map(|(_, label)| *label)
        .unwrap_or(id)
}

fn arg_str(args: &Value, key: &str) -> Option<String> {
    match args.get(key)? {
        Value::String(s) => Some(s.trim().to_string()).filter(|s| !s.is_empty()),
        Value::Null => None,
        // A body given as a JSON object instead of a string.
        other if key == "body" => serde_json::to_string_pretty(other).ok(),
        other => Some(other.to_string()),
    }
}

fn normalise_path(raw: &str) -> String {
    let mut path = raw.trim().to_string();
    // Accept a full URL and keep only its path.
    if let Some(scheme_end) = path.find("://") {
        let after = &path[scheme_end + 3..];
        path = after
            .find('/')
            .map(|i| after[i..].to_string())
            .unwrap_or_else(|| "/".to_string());
    }
    if !path.starts_with('/') {
        path.insert(0, '/');
    }
    path
}

fn parse_request(args: &Value, need_cluster: bool) -> Result<ToolRequest, String> {
    let cluster = arg_str(args, "cluster").unwrap_or_default();
    if need_cluster && cluster.is_empty() {
        return Err("Missing 'cluster'. Call list_clusters to see the names.".to_string());
    }
    let method = arg_str(args, "method")
        .unwrap_or_else(|| "GET".to_string())
        .to_uppercase();
    if !METHODS.contains(&method.as_str()) {
        return Err(format!(
            "Unsupported method '{}'; use one of {}",
            method,
            METHODS.join(", ")
        ));
    }
    let path = arg_str(args, "path").ok_or("Missing 'path'")?;
    let kibana = args
        .get("kibana")
        .and_then(|v| v.as_bool())
        .unwrap_or(false);
    Ok(ToolRequest {
        cluster,
        method,
        path: normalise_path(&path),
        body: arg_str(args, "body"),
        kibana,
    })
}

/// Turn a model's tool call into an action, or explain (to the model) what was wrong.
pub fn parse_call(call: &ToolCall) -> Result<ToolAction, String> {
    let raw = call.arguments.trim();
    let args: Value = if raw.is_empty() {
        json!({})
    } else {
        serde_json::from_str(raw)
            .map_err(|e| format!("Arguments are not valid JSON ({}): {}", e, raw))?
    };
    match call.name.as_str() {
        "list_clusters" => Ok(ToolAction::ListClusters),
        "es_request" => parse_request(&args, true).map(ToolAction::EsRequest),
        "get_console" => Ok(ToolAction::GetConsole),
        "open_in_console" => parse_request(&args, false).map(ToolAction::OpenInConsole),
        "open_tab" => {
            let tab = arg_str(&args, "tab").ok_or("Missing 'tab'")?.to_lowercase();
            let tab = tab.replace([' ', '-'], "_");
            TABS.iter()
                .find(|(id, label)| {
                    *id == tab || label.to_lowercase().replace([' ', '-'], "_") == tab
                })
                .map(|(id, _)| ToolAction::OpenTab(id.to_string()))
                .ok_or_else(|| {
                    format!(
                        "Unknown tab '{}'; use one of {}",
                        tab,
                        TABS.iter()
                            .map(|(id, _)| *id)
                            .collect::<Vec<_>>()
                            .join(", ")
                    )
                })
        }
        other => Err(format!("Unknown tool '{}'", other)),
    }
}

/// Shrink a tool result for the model: JSON is re-serialised compactly, and anything
/// over [`MAX_RESULT_CHARS`] is cut with a note saying how much was dropped.
pub fn compact_result(text: &str) -> String {
    let compact = serde_json::from_str::<Value>(text)
        .ok()
        .and_then(|v| serde_json::to_string(&v).ok())
        .unwrap_or_else(|| text.to_string());
    let total = compact.chars().count();
    if total <= MAX_RESULT_CHARS {
        return compact;
    }
    let kept: String = compact.chars().take(MAX_RESULT_CHARS).collect();
    format!(
        "{}\n…[truncated: {} more characters. Narrow the request (filter_path, size, _cat columns) to see the rest.]",
        kept,
        total - MAX_RESULT_CHARS
    )
}

/// Extra system-prompt text while tools are on.
pub fn system_prompt_addendum(clusters: &[String]) -> String {
    let names = if clusters.is_empty() {
        "(none configured)".to_string()
    } else {
        clusters.join(", ")
    };
    format!(
        "\n\nYou are running inside the Drastic Smurf desktop app and can act in it with tools: \
         list_clusters, es_request (query a cluster), get_console (see what the user has in the \
         Console), open_in_console (load a request for the user to review and send) and open_tab. \
         Look facts up with GET requests rather than guessing, and keep responses small (use \
         filter_path, size, or _cat APIs with ?v and h=). Requests that change a cluster need the \
         user's approval; if they decline, don't retry the same request. Configured clusters: {}.",
        names
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    fn call(name: &str, args: &str) -> ToolCall {
        ToolCall {
            id: "1".into(),
            name: name.into(),
            arguments: args.into(),
        }
    }

    #[test]
    fn specs_name_every_tool() {
        let specs = tool_specs();
        let names: Vec<&str> = specs
            .as_array()
            .unwrap()
            .iter()
            .map(|t| t["function"]["name"].as_str().unwrap())
            .collect();
        assert_eq!(
            names,
            [
                "list_clusters",
                "es_request",
                "get_console",
                "open_in_console",
                "open_tab"
            ]
        );
    }

    #[test]
    fn parses_requests_and_normalises() {
        let action = parse_call(&call(
            "es_request",
            r#"{"cluster":"apac","method":"get","path":"_cluster/health"}"#,
        ))
        .unwrap();
        let ToolAction::EsRequest(req) = &action else {
            panic!()
        };
        assert_eq!(req.method, "GET");
        assert_eq!(req.path, "/_cluster/health");
        assert!(!action.needs_approval(true));
        assert!(
            action.needs_approval(false),
            "reads ask when auto-run is off"
        );

        let action = parse_call(&call(
            "es_request",
            r#"{"cluster":"apac","method":"PUT","path":"https://es:9200/idx","body":{"settings":{}}}"#,
        ))
        .unwrap();
        let ToolAction::EsRequest(req) = &action else {
            panic!()
        };
        assert_eq!(req.path, "/idx");
        assert!(req.body.as_deref().unwrap().contains("settings"));
        assert!(action.needs_approval(true), "writes always ask");

        assert!(parse_call(&call("es_request", r#"{"method":"GET","path":"/"}"#)).is_err());
        assert!(
            parse_call(&call(
                "es_request",
                r#"{"cluster":"a","method":"PATCH","path":"/"}"#
            ))
            .is_err()
        );
        assert!(parse_call(&call("es_request", "{not json")).is_err());
        assert!(parse_call(&call("rm_rf", "{}")).is_err());
    }

    #[test]
    fn open_in_console_needs_no_cluster_or_approval() {
        let action = parse_call(&call(
            "open_in_console",
            r#"{"method":"DELETE","path":"/idx"}"#,
        ))
        .unwrap();
        assert!(!action.needs_approval(true));
        assert_eq!(action.summary(), "Load into Console: DELETE /idx");
    }

    #[test]
    fn open_tab_accepts_ids_and_labels() {
        assert_eq!(
            parse_call(&call("open_tab", r#"{"tab":"God-mode Console"}"#)).unwrap(),
            ToolAction::OpenTab("god_console".into())
        );
        assert_eq!(
            parse_call(&call("open_tab", r#"{"tab":"logs"}"#)).unwrap(),
            ToolAction::OpenTab("logs".into())
        );
        assert!(parse_call(&call("open_tab", r#"{"tab":"nope"}"#)).is_err());
        assert!(parse_call(&call("list_clusters", "")).is_ok());
    }

    #[test]
    fn compact_result_shrinks_json_and_truncates() {
        assert_eq!(compact_result("{\n  \"a\": 1\n}"), "{\"a\":1}");
        let long = "é".repeat(MAX_RESULT_CHARS + 10);
        let out = compact_result(&long);
        assert!(out.contains("truncated: 10 more characters"));
    }
}
