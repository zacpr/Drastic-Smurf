use std::collections::{BTreeMap, BTreeSet, HashSet};
use std::time::{Duration, Instant};

use egui::{RichText, Ui};
use serde_json::{Value, json};

use crate::core::es_client::EsClient;
use crate::ui::theme::Theme;

const API_VERSION: &str = "2023-10-31";
const PAGE_SIZE: usize = 50;
const REFRESH_INTERVAL: Duration = Duration::from_secs(5);

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LiveQueryAction {
    pub action_id: String,
    pub query_action_id: Option<String>,
}

pub fn build_live_query(query: &str, agent_ids: &[String]) -> Result<Value, String> {
    let query = query.trim();
    if query.is_empty() {
        return Err("Enter an Osquery query.".to_string());
    }
    if agent_ids.is_empty() {
        return Err("Select at least one Fleet agent.".to_string());
    }

    Ok(json!({ "query": query, "agent_ids": agent_ids }))
}

pub fn parse_live_query_action(response: &Value) -> Result<LiveQueryAction, String> {
    let data = response
        .get("data")
        .ok_or("Kibana did not return live query details.")?;
    let action_id = data
        .get("action_id")
        .and_then(Value::as_str)
        .filter(|id| !id.is_empty())
        .ok_or("Kibana did not return a live query action ID.")?;
    let query_action_id = data
        .get("queries")
        .and_then(Value::as_array)
        .and_then(|queries| queries.first())
        .and_then(|query| query.get("action_id"))
        .and_then(Value::as_str)
        .filter(|id| !id.is_empty())
        .map(str::to_string);

    Ok(LiveQueryAction {
        action_id: action_id.to_string(),
        query_action_id,
    })
}

#[derive(Debug, Clone)]
struct Agent {
    id: String,
    name: String,
    status: String,
}

#[derive(Debug, Clone)]
pub enum RequestKind {
    Agents(usize),
    Submit(Value),
    Results(LiveQueryAction, usize),
}

#[derive(Debug, Clone)]
pub struct OsqueryRequest {
    pub cluster: String,
    pub space: String,
    pub kind: RequestKind,
}

pub enum OsqueryResponse {
    Agents(usize, Result<Value, String>),
    Submit(Result<Value, String>),
    Results(usize, Result<(Value, Option<Value>), String>),
}

pub fn request_error(request: &OsqueryRequest, error: String) -> OsqueryResponse {
    match &request.kind {
        RequestKind::Agents(page) => OsqueryResponse::Agents(*page, Err(error)),
        RequestKind::Submit(_) => OsqueryResponse::Submit(Err(error)),
        RequestKind::Results(_, page) => OsqueryResponse::Results(*page, Err(error)),
    }
}

pub struct OsqueryState {
    pub selected_cluster: String,
    pub space: String,
    query: String,
    agents: Vec<Agent>,
    selected_agents: HashSet<String>,
    agent_page: usize,
    agent_total: usize,
    action: Option<LiveQueryAction>,
    status: String,
    status_summary: String,
    result_page: usize,
    result_total: usize,
    rows: Vec<BTreeMap<String, String>>,
    raw_results: Option<Value>,
    error: Option<String>,
    busy: bool,
    auto_refresh: bool,
    next_refresh: Option<Instant>,
    submission_retries: u8,
}

impl Default for OsqueryState {
    fn default() -> Self {
        Self {
            selected_cluster: String::new(),
            space: "default".to_string(),
            query: "SELECT * FROM system_info;".to_string(),
            agents: Vec::new(),
            selected_agents: HashSet::new(),
            agent_page: 1,
            agent_total: 0,
            action: None,
            status: String::new(),
            status_summary: String::new(),
            result_page: 0,
            result_total: 0,
            rows: Vec::new(),
            raw_results: None,
            error: None,
            busy: false,
            auto_refresh: true,
            next_refresh: None,
            submission_retries: 0,
        }
    }
}

impl OsqueryState {
    fn reset_target(&mut self) {
        self.agents.clear();
        self.selected_agents.clear();
        self.agent_page = 1;
        self.agent_total = 0;
        self.action = None;
        self.status.clear();
        self.status_summary.clear();
        self.result_page = 0;
        self.result_total = 0;
        self.rows.clear();
        self.raw_results = None;
        self.error = None;
        self.next_refresh = None;
        self.submission_retries = 0;
    }

    fn request(&mut self, kind: RequestKind, output: &mut Option<OsqueryRequest>) {
        self.error = None;
        self.busy = true;
        *output = Some(OsqueryRequest {
            cluster: self.selected_cluster.clone(),
            space: self.space.trim().to_string(),
            kind,
        });
    }

    pub fn receive(&mut self, response: OsqueryResponse) {
        self.busy = false;
        match response {
            OsqueryResponse::Agents(page, result) => match result.and_then(parse_agents) {
                Ok((agents, total)) => {
                    self.agents = agents;
                    self.agent_total = total;
                    self.agent_page = page;
                }
                Err(error) => self.error = Some(error),
            },
            OsqueryResponse::Submit(result) => {
                match result.and_then(|v| parse_live_query_action(&v)) {
                    Ok(action) => {
                        self.action = Some(action);
                        self.status = "submitted".to_string();
                        self.status_summary.clear();
                        self.result_page = 0;
                        self.result_total = 0;
                        self.rows.clear();
                        self.raw_results = None;
                        self.submission_retries = 0;
                        self.next_refresh = Some(Instant::now() + Duration::from_secs(2));
                    }
                    Err(error) => self.error = Some(error),
                }
            }
            OsqueryResponse::Results(page, result) => match result {
                Ok((details, results)) => {
                    if let Ok(mut action) = parse_live_query_action(&details) {
                        if action.query_action_id.is_none() {
                            action.query_action_id = self
                                .action
                                .as_ref()
                                .and_then(|previous| previous.query_action_id.clone());
                        }
                        self.action = Some(action);
                    }
                    let data = &details["data"];
                    self.status = data["status"].as_str().unwrap_or("pending").to_string();
                    let query = &data["queries"][0];
                    self.status_summary = ["successful", "failed", "pending"]
                        .iter()
                        .filter_map(|field| {
                            query[*field]
                                .as_u64()
                                .map(|count| format!("{field}: {count}"))
                        })
                        .collect::<Vec<_>>()
                        .join("  ");
                    if let Some(error) = query["error"].as_str() {
                        self.status_summary.push_str(&format!("  {error}"));
                    }
                    if let Some(results) = results {
                        match parse_results(&results) {
                            Ok((rows, total)) => {
                                self.rows = rows;
                                self.result_total = total;
                                self.result_page = page;
                                self.raw_results = Some(results);
                            }
                            Err(error) => self.error = Some(error),
                        }
                    }
                    self.next_refresh = if self.error.is_none() && !is_finished(&self.status) {
                        Some(Instant::now() + REFRESH_INTERVAL)
                    } else {
                        None
                    };
                }
                Err(error) => {
                    if self.status == "submitted"
                        && error.contains("HTTP 404")
                        && self.submission_retries < 6
                    {
                        self.submission_retries += 1;
                        self.next_refresh = Some(Instant::now() + REFRESH_INTERVAL);
                    } else {
                        self.next_refresh = None;
                    }
                    self.error = Some(error);
                }
            },
        }
    }
}

fn is_finished(status: &str) -> bool {
    matches!(status, "completed" | "failed" | "cancelled" | "expired")
}

fn parse_agents(response: Value) -> Result<(Vec<Agent>, usize), String> {
    let items = response["items"]
        .as_array()
        .ok_or("Kibana did not return a Fleet agent list.")?;
    let agents = items
        .iter()
        .filter_map(|item| {
            let id = item["id"].as_str()?;
            Some(Agent {
                id: id.to_string(),
                name: item["local_metadata"]["host"]["hostname"]
                    .as_str()
                    .unwrap_or(id)
                    .to_string(),
                status: item["status"].as_str().unwrap_or("unknown").to_string(),
            })
        })
        .collect();
    let total = response["total"].as_u64().unwrap_or(items.len() as u64) as usize;
    Ok((agents, total))
}

fn cell_text(value: &Value) -> String {
    match value {
        Value::String(text) => text.clone(),
        Value::Array(values) if values.len() == 1 => cell_text(&values[0]),
        Value::Null => String::new(),
        _ => value.to_string(),
    }
}

fn parse_results(response: &Value) -> Result<(Vec<BTreeMap<String, String>>, usize), String> {
    let data = &response["data"];
    let edges = data["edges"]
        .as_array()
        .ok_or("Kibana did not return live query result rows.")?;
    let rows = edges
        .iter()
        .map(|edge| {
            let mut row = BTreeMap::new();
            if let Some(fields) = edge["fields"].as_object() {
                for (key, value) in fields {
                    row.insert(key.clone(), cell_text(value));
                }
            }
            if let Some(fields) = edge["_source"]["osquery"].as_object() {
                for (key, value) in fields {
                    row.entry(format!("osquery.{key}"))
                        .or_insert_with(|| cell_text(value));
                }
            }
            if let Some(agent) = edge["_source"]["agent"]["id"].as_str() {
                row.entry("agent.id".to_string())
                    .or_insert_with(|| agent.to_string());
            }
            row
        })
        .collect();
    let total = data["total"].as_u64().unwrap_or(edges.len() as u64) as usize;
    Ok((rows, total))
}

fn api_path(space: &str, suffix: &str) -> Result<String, String> {
    let space = space.trim();
    if !space.is_empty()
        && !space
            .chars()
            .all(|ch| ch.is_ascii_alphanumeric() || ch == '-' || ch == '_')
    {
        return Err(
            "The Kibana space ID may contain only letters, numbers, hyphens, and underscores."
                .to_string(),
        );
    }
    if space.is_empty() || space == "default" {
        Ok(format!("/api/{suffix}"))
    } else {
        Ok(format!("/s/{space}/api/{suffix}"))
    }
}

fn valid_action_id(id: &str) -> bool {
    !id.is_empty()
        && id
            .chars()
            .all(|ch| ch.is_ascii_alphanumeric() || ch == '-' || ch == '_')
}

pub async fn execute_request(
    client: &EsClient,
    host: &str,
    request: &OsqueryRequest,
) -> OsqueryResponse {
    let send = |method, path: String, body, version| async move {
        client
            .send_to_host_with_version(host, method, &path, body, version)
            .await
            .map_err(|error| error.to_string())
    };
    match &request.kind {
        RequestKind::Agents(page) => {
            let path = api_path(
                &request.space,
                &format!("fleet/agents?page={page}&perPage={PAGE_SIZE}&showInactive=false"),
            );
            let result = match path {
                Ok(path) => send(reqwest::Method::GET, path, None, None).await,
                Err(error) => Err(error),
            };
            OsqueryResponse::Agents(*page, result)
        }
        RequestKind::Submit(body) => {
            let result = match api_path(&request.space, "osquery/live_queries") {
                Ok(path) => {
                    send(
                        reqwest::Method::POST,
                        path,
                        Some(body.to_string()),
                        Some(API_VERSION),
                    )
                    .await
                }
                Err(error) => Err(error),
            };
            OsqueryResponse::Submit(result)
        }
        RequestKind::Results(action, page) => {
            let result = async {
                if !valid_action_id(&action.action_id) {
                    return Err("Kibana returned an invalid live query action ID.".to_string());
                }
                let path = api_path(
                    &request.space,
                    &format!("osquery/live_queries/{}", action.action_id),
                )?;
                let details = send(reqwest::Method::GET, path, None, Some(API_VERSION)).await?;
                let parsed = parse_live_query_action(&details)?;
                let query_action_id = parsed
                    .query_action_id
                    .or_else(|| action.query_action_id.clone());
                let results = if let Some(query_action_id) = query_action_id {
                    if !valid_action_id(&query_action_id) {
                        return Err("Kibana returned an invalid query action ID.".to_string());
                    }
                    let path = api_path(
                        &request.space,
                        &format!(
                            "osquery/live_queries/{}/results/{}?page={page}&pageSize={PAGE_SIZE}",
                            action.action_id, query_action_id
                        ),
                    )?;
                    Some(send(reqwest::Method::GET, path, None, Some(API_VERSION)).await?)
                } else {
                    None
                };
                Ok((details, results))
            }
            .await;
            OsqueryResponse::Results(*page, result)
        }
    }
}

pub fn render_osquery_module(
    ui: &mut Ui,
    state: &mut OsqueryState,
    clusters: &[String],
    output: &mut Option<OsqueryRequest>,
) {
    ui.heading("Osquery");
    if clusters.is_empty() {
        ui.label("No clusters configured.");
        return;
    }
    if !clusters.contains(&state.selected_cluster) {
        state.selected_cluster = clusters[0].clone();
        state.reset_target();
    }

    ui.add_enabled_ui(!state.busy, |ui| {
        ui.horizontal_wrapped(|ui| {
            ui.label("Cluster:");
            let previous = state.selected_cluster.clone();
            egui::ComboBox::from_id_salt("osquery_cluster")
                .selected_text(&state.selected_cluster)
                .show_ui(ui, |ui| {
                    for cluster in clusters {
                        ui.selectable_value(&mut state.selected_cluster, cluster.clone(), cluster);
                    }
                });
            if previous != state.selected_cluster {
                state.reset_target();
            }
            ui.label("Space:");
            if ui
                .add(egui::TextEdit::singleline(&mut state.space).desired_width(120.0))
                .changed()
            {
                state.reset_target();
            }
            if ui.button("Load agents").clicked() {
                state.request(RequestKind::Agents(1), output);
            }
        });
    });

    ui.add_space(6.0);
    ui.horizontal_wrapped(|ui| {
        ui.label(
            RichText::new(format!(
                "Fleet agents ({} selected)",
                state.selected_agents.len()
            ))
            .strong(),
        );
        ui.add_enabled_ui(!state.busy, |ui| {
            if ui
                .add_enabled(state.agent_page > 1, egui::Button::new("Previous"))
                .clicked()
            {
                state.request(RequestKind::Agents(state.agent_page - 1), output);
            }
            ui.label(format!("Page {}", state.agent_page));
            if ui
                .add_enabled(
                    state.agent_page * PAGE_SIZE < state.agent_total,
                    egui::Button::new("Next"),
                )
                .clicked()
            {
                state.request(RequestKind::Agents(state.agent_page + 1), output);
            }
            if ui
                .add_enabled(
                    !state.selected_agents.is_empty(),
                    egui::Button::new("Clear selection"),
                )
                .clicked()
            {
                state.selected_agents.clear();
            }
        });
    });
    egui::ScrollArea::vertical()
        .id_salt("osquery_agents")
        .max_height(140.0)
        .show(ui, |ui| {
            if state.agents.is_empty() {
                ui.label(RichText::new("No agents loaded.").color(Theme::text_muted()));
            }
            ui.add_enabled_ui(!state.busy, |ui| {
                for agent in &state.agents {
                    let mut selected = state.selected_agents.contains(&agent.id);
                    if ui
                        .checkbox(
                            &mut selected,
                            format!("{}  ({})  [{}]", agent.name, agent.id, agent.status),
                        )
                        .changed()
                    {
                        if selected {
                            state.selected_agents.insert(agent.id.clone());
                        } else {
                            state.selected_agents.remove(&agent.id);
                        }
                    }
                }
            });
        });

    ui.separator();
    ui.label(RichText::new("Query").strong());
    ui.add_enabled(
        !state.busy,
        egui::TextEdit::multiline(&mut state.query)
            .font(egui::TextStyle::Monospace)
            .desired_rows(3)
            .desired_width(f32::INFINITY),
    );
    ui.horizontal_wrapped(|ui| {
        if ui
            .add_enabled(!state.busy, egui::Button::new("Run query"))
            .clicked()
        {
            let mut agents: Vec<_> = state.selected_agents.iter().cloned().collect();
            agents.sort();
            match build_live_query(&state.query, &agents) {
                Ok(body) => state.request(RequestKind::Submit(body), output),
                Err(error) => state.error = Some(error),
            }
        }
        if state.busy {
            ui.spinner();
        }
        if !state.status.is_empty() {
            ui.label(format!("Status: {}", state.status));
        }
        if !state.status_summary.is_empty() {
            ui.label(&state.status_summary);
        }
        ui.checkbox(&mut state.auto_refresh, "Auto-refresh");
        if ui
            .add_enabled(
                !state.busy && state.action.is_some(),
                egui::Button::new("Refresh results"),
            )
            .clicked()
        {
            state.request(
                RequestKind::Results(state.action.clone().unwrap(), state.result_page),
                output,
            );
        }
        if let Some(results) = &state.raw_results
            && ui.button("Copy page").clicked()
        {
            ui.ctx()
                .copy_text(serde_json::to_string_pretty(results).unwrap_or_default());
        }
    });
    if let Some(error) = &state.error {
        ui.colored_label(Theme::danger(), error);
    }

    ui.separator();
    ui.horizontal_wrapped(|ui| {
        ui.label(RichText::new(format!("Results ({})", state.result_total)).strong());
        if ui
            .add_enabled(
                !state.busy && state.result_page > 0,
                egui::Button::new("Previous"),
            )
            .clicked()
        {
            state.request(
                RequestKind::Results(state.action.clone().unwrap(), state.result_page - 1),
                output,
            );
        }
        ui.label(format!("Page {}", state.result_page + 1));
        if ui
            .add_enabled(
                !state.busy && (state.result_page + 1) * PAGE_SIZE < state.result_total,
                egui::Button::new("Next"),
            )
            .clicked()
        {
            state.request(
                RequestKind::Results(state.action.clone().unwrap(), state.result_page + 1),
                output,
            );
        }
    });
    let columns: BTreeSet<_> = state
        .rows
        .iter()
        .flat_map(|row| row.keys().cloned())
        .collect();
    egui::ScrollArea::both()
        .id_salt("osquery_results")
        .auto_shrink([false, false])
        .show(ui, |ui| {
            if state.rows.is_empty() {
                ui.label(RichText::new("No results yet.").color(Theme::text_muted()));
            } else {
                egui::Grid::new("osquery_results_grid")
                    .striped(true)
                    .show(ui, |ui| {
                        for column in &columns {
                            ui.label(RichText::new(column).strong());
                        }
                        ui.end_row();
                        for row in &state.rows {
                            for column in &columns {
                                ui.label(row.get(column).map(String::as_str).unwrap_or(""));
                            }
                            ui.end_row();
                        }
                    });
            }
        });

    if !state.busy
        && state.auto_refresh
        && let (Some(action), Some(next)) = (&state.action, state.next_refresh)
    {
        if Instant::now() >= next {
            state.request(
                RequestKind::Results(action.clone(), state.result_page),
                output,
            );
        } else {
            ui.ctx()
                .request_repaint_after(next.saturating_duration_since(Instant::now()));
        }
    }
    if output.is_some() {
        ui.ctx().request_repaint();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::{Read, Write};
    use std::net::TcpListener;

    use crate::core::config::ClusterConfig;

    #[test]
    fn live_query_requires_query_and_agents() {
        assert!(build_live_query("", &["agent-1".to_string()]).is_err());
        assert!(build_live_query("select * from uptime;", &[]).is_err());
        assert_eq!(
            build_live_query(" select * from uptime; ", &["agent-1".to_string()]).unwrap(),
            json!({"query": "select * from uptime;", "agent_ids": ["agent-1"]})
        );
    }

    #[test]
    fn parses_live_query_action_ids() {
        let response =
            json!({"data": {"action_id": "parent", "queries": [{"action_id": "query"}]}});
        assert_eq!(
            parse_live_query_action(&response).unwrap(),
            LiveQueryAction {
                action_id: "parent".to_string(),
                query_action_id: Some("query".to_string()),
            }
        );
        assert!(parse_live_query_action(&json!({"data": {}})).is_err());
        assert_eq!(
            parse_live_query_action(&json!({"data": {"action_id": "parent"}}))
                .unwrap()
                .query_action_id,
            None
        );
    }

    #[test]
    fn validates_space_and_action_paths() {
        assert_eq!(
            api_path("default", "fleet/agents").unwrap(),
            "/api/fleet/agents"
        );
        assert_eq!(
            api_path("security-team", "fleet/agents").unwrap(),
            "/s/security-team/api/fleet/agents"
        );
        assert!(api_path("../other", "fleet/agents").is_err());
        assert!(valid_action_id("abc-123"));
        assert!(!valid_action_id("../other"));
    }

    #[test]
    fn parses_fleet_agents_and_both_result_shapes() {
        let (agents, total) = parse_agents(json!({
            "total": 2,
            "items": [
                {"id": "agent-1", "status": "online", "local_metadata": {"host": {"hostname": "web-1"}}},
                {"id": "agent-2"}
            ]
        })).unwrap();
        assert_eq!(total, 2);
        assert_eq!(agents[0].name, "web-1");
        assert_eq!(agents[1].name, "agent-2");
        let (rows, total) = parse_results(&json!({"data": {"total": 2, "edges": [
            {"fields": {"agent.id": ["agent-1"], "osquery.pid": [42]}},
            {"_source": {"agent": {"id": "agent-2"}, "osquery": {"pid": "123"}}}
        ]}}))
        .unwrap();
        assert_eq!(total, 2);
        assert_eq!(rows[0]["osquery.pid"], "42");
        assert_eq!(rows[1]["agent.id"], "agent-2");
        assert_eq!(rows[1]["osquery.pid"], "123");
        assert!(parse_results(&json!({"data": {}})).is_err());
    }

    #[test]
    fn tracks_outcomes_and_retries_initial_indexing() {
        let mut state = OsqueryState::default();
        state.receive(OsqueryResponse::Submit(Ok(
            json!({"data": {"action_id": "parent"}}),
        )));
        state.receive(OsqueryResponse::Results(
            0,
            Err("HTTP 404: not indexed".to_string()),
        ));
        assert!(state.next_refresh.is_some());
        state.receive(OsqueryResponse::Results(
            0,
            Ok((
                json!({"data": {"action_id": "parent", "status": "completed", "queries": [
                    {"action_id": "query", "successful": 1, "failed": 1, "pending": 0}
                ]}}),
                Some(json!({"data": {"total": 0, "edges": []}})),
            )),
        ));
        assert!(state.status_summary.contains("failed: 1"));
        assert!(state.next_refresh.is_none());
    }

    #[test]
    fn renders_selected_agents_and_query_results() {
        fn collect_text(shape: &egui::Shape, output: &mut Vec<String>) {
            match shape {
                egui::Shape::Vec(shapes) => {
                    for shape in shapes {
                        collect_text(shape, output);
                    }
                }
                egui::Shape::Text(text) => output.push(text.galley.text().to_string()),
                _ => {}
            }
        }

        let context = egui::Context::default();
        let mut state = OsqueryState {
            selected_cluster: "test".into(),
            agents: vec![Agent {
                id: "agent-1".into(),
                name: "web-1".into(),
                status: "online".into(),
            }],
            selected_agents: HashSet::from(["agent-1".into()]),
            action: Some(LiveQueryAction {
                action_id: "parent".into(),
                query_action_id: Some("query".into()),
            }),
            status: "completed".into(),
            status_summary: "successful: 1  failed: 0".into(),
            result_total: 1,
            rows: vec![BTreeMap::from([("osquery.pid".into(), "42".into())])],
            ..Default::default()
        };
        let mut texts = Vec::new();
        for _ in 0..3 {
            let frame = context.run(
                egui::RawInput {
                    screen_rect: Some(egui::Rect::from_min_size(
                        egui::Pos2::ZERO,
                        egui::vec2(800.0, 600.0),
                    )),
                    ..Default::default()
                },
                |context| {
                    egui::CentralPanel::default().show(context, |ui| {
                        render_osquery_module(ui, &mut state, &["test".into()], &mut None);
                    });
                },
            );
            texts.clear();
            for shape in &frame.shapes {
                collect_text(&shape.shape, &mut texts);
            }
        }
        for expected in ["web-1", "1 selected", "completed", "osquery.pid", "42"] {
            assert!(
                texts.iter().any(|text| text.contains(expected)),
                "missing {expected}"
            );
        }
    }

    #[tokio::test]
    async fn sends_targeted_query_and_fetches_results() {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let address = listener.local_addr().unwrap();
        let server = std::thread::spawn(move || {
            let responses = [
                json!({"items": [{"id": "agent-1", "local_metadata": {"host": {"hostname": "web"}}}], "total": 1}),
                json!({"data": {"action_id": "parent", "queries": [{"action_id": "query"}]}}),
                json!({"data": {"action_id": "parent", "status": "completed", "queries": [{"action_id": "query"}]}}),
                json!({"data": {"total": 1, "edges": [{"fields": {"osquery.pid": [42]}}]}}),
            ];
            let mut requests = Vec::new();
            for response in responses {
                let (mut stream, _) = listener.accept().unwrap();
                stream
                    .set_read_timeout(Some(Duration::from_secs(5)))
                    .unwrap();
                let mut bytes = Vec::new();
                let mut buffer = [0; 4096];
                loop {
                    let count = stream.read(&mut buffer).unwrap();
                    assert!(count > 0);
                    bytes.extend_from_slice(&buffer[..count]);
                    if let Some(header_end) = bytes.windows(4).position(|part| part == b"\r\n\r\n")
                    {
                        let headers = String::from_utf8_lossy(&bytes[..header_end]);
                        let length = headers
                            .lines()
                            .find_map(|line| {
                                let (name, value) = line.split_once(':')?;
                                name.eq_ignore_ascii_case("content-length")
                                    .then(|| value.trim().parse::<usize>().unwrap())
                            })
                            .unwrap_or(0);
                        if bytes.len() >= header_end + 4 + length {
                            break;
                        }
                    }
                }
                requests.push(String::from_utf8(bytes).unwrap());
                let body = response.to_string();
                write!(stream, "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}", body.len(), body).unwrap();
            }
            requests
        });
        let config = ClusterConfig {
            name: "test".into(),
            username: "user".into(),
            ..Default::default()
        };
        let client = EsClient::with_password(&config, "password").unwrap();
        let host = format!("http://{address}");
        let mut request = OsqueryRequest {
            cluster: "test".into(),
            space: "security".into(),
            kind: RequestKind::Agents(1),
        };
        let response = execute_request(&client, &host, &request).await;
        let mut state = OsqueryState::default();
        state.receive(response);
        assert_eq!(state.agents[0].id, "agent-1");
        request.kind = RequestKind::Submit(
            build_live_query("SELECT * FROM processes;", &["agent-1".into()]).unwrap(),
        );
        state.receive(execute_request(&client, &host, &request).await);
        request.kind = RequestKind::Results(state.action.clone().unwrap(), 0);
        state.receive(execute_request(&client, &host, &request).await);
        assert_eq!(state.status, "completed");
        assert_eq!(state.rows[0]["osquery.pid"], "42");

        let requests = server.join().unwrap();
        assert!(requests[0].starts_with("GET /s/security/api/fleet/agents?page=1&perPage=50"));
        assert!(requests[1].starts_with("POST /s/security/api/osquery/live_queries "));
        assert!(
            requests[1]
                .to_lowercase()
                .contains("elastic-api-version: 2023-10-31")
        );
        assert!(requests[1].to_lowercase().contains("kbn-xsrf: true"));
        assert!(requests[1].contains("\"agent_ids\":[\"agent-1\"]"));
        assert!(requests[2].starts_with("GET /s/security/api/osquery/live_queries/parent "));
        assert!(requests[3].starts_with(
            "GET /s/security/api/osquery/live_queries/parent/results/query?page=0&pageSize=50 "
        ));
    }
}
