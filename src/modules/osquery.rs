//! Osquery live queries through Kibana, targeted like Kibana's own picker: individual agents
//! (searched by name as you type), All agents, platforms, or Osquery-enabled agent policies.
//! Saved queries from the Osquery app can be picked as a starting point.
//!
//! Requests run in independent lanes (groups, agent search, saved queries, query/results), so
//! searching never blocks a running query. Every request carries the target `generation`, and
//! searches a sequence number, so responses for a previous cluster/space or a superseded search
//! are dropped.

use std::collections::{BTreeMap, BTreeSet, HashMap};
use std::time::{Duration, Instant};

use egui::{RichText, Ui};
use futures_util::future::join_all;
use serde_json::{Value, json};

use crate::core::es_client::EsClient;
use crate::modules::indices::{FILTER_DEBOUNCE_MS, filter_debounce_due, filter_debounce_remaining};
use crate::ui::theme::Theme;

const API_VERSION: &str = "2023-10-31";
/// Live query result rows per page.
const PAGE_SIZE: usize = 50;
/// Matching agents fetched per search; narrower searches beat paging through thousands.
const AGENT_SEARCH_SIZE: usize = 50;
const REFRESH_INTERVAL: Duration = Duration::from_secs(5);
/// Fleet package name of the Osquery Manager integration.
const OSQUERY_PACKAGE: &str = "osquery_manager";
const LIST_PAGE_SIZE: usize = 500;
const MAX_LIST_PAGES: usize = 20;
const SAVED_QUERY_PAGE_SIZE: usize = 100;
/// Common `local_metadata.os.platform` values, counted up front; any others are discovered.
const PLATFORM_HINTS: &[&str] = &[
    "windows", "darwin", "ubuntu", "debian", "rhel", "centos", "amzn",
];
const MAX_PLATFORM_DISCOVERY: usize = 20;
/// Selected-agent chips shown before summarising the rest.
const CHIP_DISPLAY_LIMIT: usize = 100;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LiveQueryAction {
    pub action_id: String,
    pub query_action_id: Option<String>,
}

/// Who a live query runs on. `all` overrides everything else.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct AgentTargets {
    pub all: bool,
    pub agent_ids: Vec<String>,
    pub platforms: Vec<String>,
    pub policy_ids: Vec<String>,
}

impl AgentTargets {
    pub fn is_empty(&self) -> bool {
        !self.all
            && self.agent_ids.is_empty()
            && self.platforms.is_empty()
            && self.policy_ids.is_empty()
    }
}

/// A saved query from Kibana's Osquery app.
#[derive(Debug, Clone, PartialEq)]
pub struct SavedQuery {
    pub id: String,
    pub query: String,
    pub description: String,
    pub ecs_mapping: Option<Value>,
}

/// Build the `POST /api/osquery/live_queries` body.
///
/// The saved query's id (and ECS mapping) are only sent when the query text still matches
/// it; once edited, the query runs ad hoc.
pub fn build_live_query(
    query: &str,
    targets: &AgentTargets,
    saved: Option<&SavedQuery>,
) -> Result<Value, String> {
    let query = query.trim();
    if query.is_empty() {
        return Err("Enter an Osquery query.".to_string());
    }
    if targets.is_empty() {
        return Err("Select at least one agent, platform, policy, or All agents.".to_string());
    }

    let mut body = json!({ "query": query });
    if targets.all {
        body["agent_all"] = json!(true);
    } else {
        if !targets.agent_ids.is_empty() {
            body["agent_ids"] = json!(targets.agent_ids);
        }
        if !targets.platforms.is_empty() {
            body["agent_platforms"] = json!(targets.platforms);
        }
        if !targets.policy_ids.is_empty() {
            body["agent_policy_ids"] = json!(targets.policy_ids);
        }
    }
    if let Some(saved) = saved.filter(|s| s.query.trim() == query) {
        body["saved_query_id"] = json!(saved.id);
        if let Some(mapping) = saved
            .ecs_mapping
            .as_ref()
            .filter(|m| m.as_object().is_some_and(|o| !o.is_empty()))
        {
            body["ecs_mapping"] = mapping.clone();
        }
    }
    Ok(body)
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

#[derive(Debug, Clone, PartialEq, Eq)]
struct Agent {
    id: String,
    name: String,
    status: String,
    platform: String,
    policy_id: String,
}

/// One selectable group (a platform or an agent policy) with its agent count.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Group {
    pub id: String,
    pub name: String,
    pub count: Option<usize>,
}

/// The group targets available for the current cluster/space.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Groups {
    /// Agents enrolled in an Osquery-enabled policy.
    pub all_count: usize,
    pub platforms: Vec<Group>,
    /// Osquery-enabled agent policies.
    pub policies: Vec<Group>,
    /// Agents whose platform couldn't be determined (not targetable by platform).
    pub unlisted: usize,
    pub warnings: Vec<String>,
}

#[derive(Debug, Clone)]
pub enum RequestKind {
    Groups,
    Search {
        seq: u64,
        term: String,
        policy_ids: Vec<String>,
    },
    SavedQueries,
    Submit(Value),
    Results(LiveQueryAction, usize),
}

#[derive(Debug, Clone)]
pub struct OsqueryRequest {
    pub cluster: String,
    pub space: String,
    pub generation: u64,
    pub kind: RequestKind,
}

pub enum ResponseKind {
    Groups(Result<Groups, String>),
    Search(u64, Result<Value, String>),
    SavedQueries(Result<Vec<SavedQuery>, String>),
    Submit(Result<Value, String>),
    Results(usize, Result<(Value, Option<Value>), String>),
}

pub struct OsqueryResponse {
    pub generation: u64,
    pub kind: ResponseKind,
}

pub fn request_error(request: &OsqueryRequest, error: String) -> OsqueryResponse {
    let kind = match &request.kind {
        RequestKind::Groups => ResponseKind::Groups(Err(error)),
        RequestKind::Search { seq, .. } => ResponseKind::Search(*seq, Err(error)),
        RequestKind::SavedQueries => ResponseKind::SavedQueries(Err(error)),
        RequestKind::Submit(_) => ResponseKind::Submit(Err(error)),
        RequestKind::Results(_, page) => ResponseKind::Results(*page, Err(error)),
    };
    OsqueryResponse {
        generation: request.generation,
        kind,
    }
}

pub struct OsqueryState {
    pub selected_cluster: String,
    pub space: String,
    query: String,
    /// Bumped whenever the cluster/space changes; stale responses are ignored.
    generation: u64,
    /// Whether groups/saved queries have been requested for this cluster/space.
    loaded: bool,

    // Agent search
    agent_search: String,
    search_typed_at: Option<Instant>,
    search_seq: u64,
    search_pending: bool,
    search_inflight: bool,
    search_results: Vec<Agent>,
    search_total: usize,
    /// Every agent seen in a search, so selected chips keep their names.
    known_agents: HashMap<String, Agent>,
    selected_agents: BTreeSet<String>,

    // Groups
    groups: Option<Groups>,
    groups_loading: bool,
    target_all: bool,
    selected_platforms: BTreeSet<String>,
    selected_policies: BTreeSet<String>,
    targets_error: Option<String>,

    // Saved queries
    saved_queries: Vec<SavedQuery>,
    saved_loading: bool,
    saved_filter: String,
    selected_saved: Option<String>,
    saved_error: Option<String>,

    // Live query run
    action: Option<LiveQueryAction>,
    status: String,
    status_summary: String,
    result_page: usize,
    result_total: usize,
    rows: Vec<BTreeMap<String, String>>,
    raw_results: Option<Value>,
    error: Option<String>,
    query_busy: bool,
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
            generation: 0,
            loaded: false,
            agent_search: String::new(),
            search_typed_at: None,
            search_seq: 0,
            search_pending: false,
            search_inflight: false,
            search_results: Vec::new(),
            search_total: 0,
            known_agents: HashMap::new(),
            selected_agents: BTreeSet::new(),
            groups: None,
            groups_loading: false,
            target_all: false,
            selected_platforms: BTreeSet::new(),
            selected_policies: BTreeSet::new(),
            targets_error: None,
            saved_queries: Vec::new(),
            saved_loading: false,
            saved_filter: String::new(),
            selected_saved: None,
            saved_error: None,
            action: None,
            status: String::new(),
            status_summary: String::new(),
            result_page: 0,
            result_total: 0,
            rows: Vec::new(),
            raw_results: None,
            error: None,
            query_busy: false,
            auto_refresh: true,
            next_refresh: None,
            submission_retries: 0,
        }
    }
}

impl OsqueryState {
    /// Forget everything tied to the previous cluster/space; keep the query text and options.
    fn reset_target(&mut self) {
        let fresh = Self {
            selected_cluster: std::mem::take(&mut self.selected_cluster),
            space: std::mem::take(&mut self.space),
            query: std::mem::take(&mut self.query),
            auto_refresh: self.auto_refresh,
            generation: self.generation + 1,
            ..Default::default()
        };
        *self = fresh;
    }

    fn queue(&self, kind: RequestKind, output: &mut Vec<OsqueryRequest>) {
        output.push(OsqueryRequest {
            cluster: self.selected_cluster.clone(),
            space: self.space.trim().to_string(),
            generation: self.generation,
            kind,
        });
    }

    fn load(&mut self, output: &mut Vec<OsqueryRequest>) {
        self.loaded = true;
        self.groups_loading = true;
        self.saved_loading = true;
        self.targets_error = None;
        self.saved_error = None;
        self.queue(RequestKind::Groups, output);
        self.queue(RequestKind::SavedQueries, output);
    }

    fn osquery_policy_ids(&self) -> Vec<String> {
        self.groups
            .as_ref()
            .map(|g| g.policies.iter().map(|p| p.id.clone()).collect())
            .unwrap_or_default()
    }

    fn send_search(&mut self, output: &mut Vec<OsqueryRequest>) {
        self.search_seq += 1;
        self.search_inflight = true;
        self.search_pending = false;
        self.search_typed_at = None;
        self.queue(
            RequestKind::Search {
                seq: self.search_seq,
                term: self.agent_search.trim().to_string(),
                policy_ids: self.osquery_policy_ids(),
            },
            output,
        );
    }

    fn targets(&self) -> AgentTargets {
        if self.target_all {
            return AgentTargets {
                all: true,
                ..Default::default()
            };
        }
        AgentTargets {
            all: false,
            agent_ids: self.selected_agents.iter().cloned().collect(),
            platforms: self.selected_platforms.iter().cloned().collect(),
            policy_ids: self.selected_policies.iter().cloned().collect(),
        }
    }

    fn selected_saved_query(&self) -> Option<&SavedQuery> {
        let id = self.selected_saved.as_deref()?;
        self.saved_queries.iter().find(|q| q.id == id)
    }

    fn submit(&mut self, output: &mut Vec<OsqueryRequest>) {
        match build_live_query(&self.query, &self.targets(), self.selected_saved_query()) {
            Ok(body) => {
                self.error = None;
                self.query_busy = true;
                self.queue(RequestKind::Submit(body), output);
            }
            Err(error) => self.error = Some(error),
        }
    }

    fn fetch_results(&mut self, page: usize, output: &mut Vec<OsqueryRequest>) {
        if let Some(action) = self.action.clone() {
            self.error = None;
            self.query_busy = true;
            self.queue(RequestKind::Results(action, page), output);
        }
    }

    pub fn receive(&mut self, response: OsqueryResponse) {
        if response.generation != self.generation {
            return;
        }
        match response.kind {
            ResponseKind::Groups(result) => {
                self.groups_loading = false;
                match result {
                    Ok(groups) => self.groups = Some(groups),
                    Err(error) => {
                        self.targets_error = Some(format!("Couldn't load agent groups: {error}"))
                    }
                }
                // Search once groups are known, so results stay within Osquery policies.
                self.search_pending = true;
            }
            ResponseKind::Search(seq, result) => {
                if seq != self.search_seq {
                    return;
                }
                self.search_inflight = false;
                match result.and_then(parse_agents) {
                    Ok((mut agents, total)) => {
                        agents.sort_by_key(|a| a.name.to_lowercase());
                        for agent in &agents {
                            self.known_agents.insert(agent.id.clone(), agent.clone());
                        }
                        self.search_results = agents;
                        self.search_total = total;
                    }
                    Err(error) => {
                        self.targets_error = Some(format!("Agent search failed: {error}"))
                    }
                }
            }
            ResponseKind::SavedQueries(result) => {
                self.saved_loading = false;
                match result {
                    Ok(queries) => self.saved_queries = queries,
                    Err(error) => self.saved_error = Some(error),
                }
            }
            ResponseKind::Submit(result) => {
                self.query_busy = false;
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
            ResponseKind::Results(page, result) => {
                self.query_busy = false;
                self.receive_results(page, result);
            }
        }
    }

    fn receive_results(&mut self, page: usize, result: Result<(Value, Option<Value>), String>) {
        match result {
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
            let meta = &item["local_metadata"];
            Some(Agent {
                id: id.to_string(),
                name: meta["host"]["hostname"].as_str().unwrap_or(id).to_string(),
                status: item["status"].as_str().unwrap_or("unknown").to_string(),
                platform: meta["os"]["platform"].as_str().unwrap_or("").to_string(),
                policy_id: item["policy_id"].as_str().unwrap_or("").to_string(),
            })
        })
        .collect();
    let total = response["total"].as_u64().unwrap_or(items.len() as u64) as usize;
    Ok((agents, total))
}

/// Agent policy ids that include the Osquery Manager integration, from a package policy list.
/// Handles both `policy_ids` (newer Fleet) and `policy_id` (older).
fn osquery_policy_ids(package_policies: &[Value]) -> Vec<String> {
    let mut ids = Vec::new();
    for item in package_policies
        .iter()
        .filter(|item| item["package"]["name"].as_str() == Some(OSQUERY_PACKAGE))
    {
        let mut push = |id: &str| {
            if !id.is_empty() && !ids.iter().any(|existing| existing == id) {
                ids.push(id.to_string());
            }
        };
        if let Some(list) = item["policy_ids"].as_array() {
            list.iter().filter_map(Value::as_str).for_each(&mut push);
        }
        if let Some(id) = item["policy_id"].as_str() {
            push(id);
        }
    }
    ids
}

/// Parse a saved query list page; handles the current `data: [...]` shape and the older
/// `data.saved_objects[].attributes` shape.
fn parse_saved_queries(response: &Value) -> Result<(Vec<SavedQuery>, usize), String> {
    let data = &response["data"];
    let items: Vec<&Value> = if let Some(list) = data.as_array() {
        list.iter().collect()
    } else if let Some(list) = data["saved_objects"].as_array() {
        list.iter().map(|so| &so["attributes"]).collect()
    } else {
        return Err("Kibana did not return a saved query list.".to_string());
    };
    let queries: Vec<SavedQuery> = items
        .into_iter()
        .filter_map(|item| {
            Some(SavedQuery {
                id: item["id"].as_str()?.to_string(),
                query: item["query"].as_str()?.to_string(),
                description: item["description"].as_str().unwrap_or("").to_string(),
                ecs_mapping: item.get("ecs_mapping").filter(|m| m.is_object()).cloned(),
            })
        })
        .collect();
    let total = response["total"]
        .as_u64()
        .or_else(|| data["total"].as_u64())
        .unwrap_or(queries.len() as u64) as usize;
    Ok((queries, total))
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

// --- KQL / URL helpers -------------------------------------------------------------------

/// Percent-encode a query-string value (RFC 3986 unreserved characters pass through).
fn url_encode(value: &str) -> String {
    let mut out = String::with_capacity(value.len());
    for byte in value.bytes() {
        if byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_' | b'.' | b'~') {
            out.push(byte as char);
        } else {
            out.push_str(&format!("%{byte:02X}"));
        }
    }
    out
}

/// A quoted KQL value.
fn kql_quote(value: &str) -> String {
    format!("\"{}\"", value.replace('\\', "\\\\").replace('"', "\\\""))
}

/// Escape a user term for use inside an unquoted (wildcard) KQL value.
fn kql_escape(term: &str) -> String {
    let mut out = String::with_capacity(term.len());
    for ch in term.chars() {
        if matches!(
            ch,
            '\\' | '(' | ')' | ':' | '<' | '>' | '"' | '*' | '{' | '}'
        ) || ch.is_whitespace()
        {
            out.push('\\');
        }
        out.push(ch);
    }
    out
}

fn kql_any_of<S: AsRef<str>>(field: &str, values: &[S]) -> String {
    let quoted: Vec<String> = values.iter().map(|v| kql_quote(v.as_ref())).collect();
    format!("{field}:({})", quoted.join(" or "))
}

fn policy_kuery(policy_ids: &[String]) -> Option<String> {
    (!policy_ids.is_empty()).then(|| kql_any_of("policy_id", policy_ids))
}

fn and_kuery(base: Option<&str>, extra: &str) -> String {
    match base {
        Some(base) => format!("({base}) and ({extra})"),
        None => extra.to_string(),
    }
}

/// KQL for an agent search: host name or agent id containing the term (as typed and
/// lowercased, since these fields are case-sensitive), within the Osquery policies.
fn search_kuery(term: &str, policy_ids: &[String]) -> Option<String> {
    let base = policy_kuery(policy_ids);
    let term = term.trim();
    if term.is_empty() {
        return base;
    }
    let mut variants = vec![kql_escape(term)];
    let lower = term.to_lowercase();
    if lower != term {
        variants.push(kql_escape(&lower));
    }
    let clauses = variants
        .iter()
        .flat_map(|v| {
            [
                format!("local_metadata.host.hostname:*{v}*"),
                format!("local_metadata.elastic.agent.id:*{v}*"),
            ]
        })
        .collect::<Vec<_>>()
        .join(" or ");
    Some(and_kuery(base.as_deref(), &clauses))
}

fn agents_suffix(per_page: usize, kuery: Option<&str>) -> String {
    let mut suffix = format!("fleet/agents?page=1&perPage={per_page}&showInactive=false");
    if let Some(kuery) = kuery.filter(|k| !k.is_empty()) {
        suffix.push_str("&kuery=");
        suffix.push_str(&url_encode(kuery));
    }
    suffix
}

// --- Kibana requests ---------------------------------------------------------------------

struct Kibana<'a> {
    client: &'a EsClient,
    host: &'a str,
    space: &'a str,
}

impl Kibana<'_> {
    async fn send(
        &self,
        method: reqwest::Method,
        suffix: &str,
        body: Option<String>,
        version: Option<&str>,
    ) -> Result<Value, String> {
        let path = api_path(self.space, suffix)?;
        self.client
            .send_to_host_with_version(self.host, method, &path, body, version)
            .await
            .map_err(|error| error.to_string())
    }

    async fn get(&self, suffix: &str) -> Result<Value, String> {
        self.send(reqwest::Method::GET, suffix, None, None).await
    }

    /// Count active agents matching a KQL filter; also returns the first match.
    async fn count_agents(&self, kuery: Option<String>) -> Result<(usize, Option<Value>), String> {
        let response = self.get(&agents_suffix(1, kuery.as_deref())).await?;
        let total = response["total"].as_u64().unwrap_or(0) as usize;
        let first = response["items"]
            .as_array()
            .and_then(|items| items.first())
            .cloned();
        Ok((total, first))
    }
}

/// Fetch every page of a Fleet list endpoint (`items` + `total`).
async fn fetch_fleet_list(kb: &Kibana<'_>, endpoint: &str) -> Result<Vec<Value>, String> {
    let mut all = Vec::new();
    for page in 1..=MAX_LIST_PAGES {
        let response = kb
            .get(&format!("{endpoint}?page={page}&perPage={LIST_PAGE_SIZE}"))
            .await?;
        let items = response["items"]
            .as_array()
            .ok_or_else(|| format!("Kibana did not return a list from {endpoint}."))?;
        let total = response["total"].as_u64().unwrap_or(0) as usize;
        let done = items.is_empty() || page * LIST_PAGE_SIZE >= total;
        all.extend(items.iter().cloned());
        if done {
            break;
        }
    }
    Ok(all)
}

async fn load_groups(kb: &Kibana<'_>) -> Result<Groups, String> {
    let package_policies = fetch_fleet_list(kb, "fleet/package_policies").await?;
    let policy_ids = osquery_policy_ids(&package_policies);
    let mut warnings = Vec::new();
    let mut names: HashMap<String, String> = HashMap::new();
    if policy_ids.is_empty() {
        warnings.push(
            "No agent policy includes the Osquery Manager integration, so no agents can run live queries."
                .to_string(),
        );
    } else {
        for policy in fetch_fleet_list(kb, "fleet/agent_policies").await? {
            if let (Some(id), Some(name)) = (policy["id"].as_str(), policy["name"].as_str()) {
                names.insert(id.to_string(), name.to_string());
            }
        }
    }
    let base = policy_kuery(&policy_ids);
    let (all_count, _) = kb.count_agents(base.clone()).await?;

    let counts = join_all(
        policy_ids
            .iter()
            .map(|id| kb.count_agents(Some(format!("policy_id:{}", kql_quote(id))))),
    )
    .await;
    let mut policies: Vec<Group> = policy_ids
        .iter()
        .zip(counts)
        .map(|(id, count)| Group {
            id: id.clone(),
            name: names.get(id).cloned().unwrap_or_else(|| id.clone()),
            count: count.ok().map(|(n, _)| n),
        })
        .collect();
    policies.sort_by_key(|p| p.name.to_lowercase());

    let (platforms, unlisted) = match discover_platforms(kb, base.as_deref()).await {
        Ok(found) => found,
        Err(error) => {
            warnings.push(format!("Couldn't list platforms: {error}"));
            (Vec::new(), 0)
        }
    };
    if unlisted > 0 {
        warnings.push(format!(
            "{unlisted} agent(s) report no platform, so they can only be targeted individually, by policy, or with All agents."
        ));
    }
    Ok(Groups {
        all_count,
        platforms,
        policies,
        unlisted,
        warnings,
    })
}

/// Find every `local_metadata.os.platform` value among the targetable agents, with counts.
///
/// Common platforms are counted in parallel; any others are discovered exactly by repeatedly
/// asking for an agent on none of the platforms found so far.
async fn discover_platforms(
    kb: &Kibana<'_>,
    base: Option<&str>,
) -> Result<(Vec<Group>, usize), String> {
    const FIELD: &str = "local_metadata.os.platform";
    let counts = join_all(
        PLATFORM_HINTS
            .iter()
            .map(|p| kb.count_agents(Some(and_kuery(base, &kql_any_of(FIELD, &[p]))))),
    )
    .await;
    let mut platforms = Vec::new();
    for (platform, count) in PLATFORM_HINTS.iter().zip(counts) {
        let (n, _) = count?;
        if n > 0 {
            platforms.push(Group {
                id: platform.to_string(),
                name: platform.to_string(),
                count: Some(n),
            });
        }
    }

    let mut known: Vec<String> = PLATFORM_HINTS.iter().map(|p| p.to_string()).collect();
    let mut unlisted = 0;
    for round in 0..=MAX_PLATFORM_DISCOVERY {
        let kuery = and_kuery(base, &format!("not {}", kql_any_of(FIELD, &known)));
        let (remaining, first) = kb.count_agents(Some(kuery)).await?;
        if remaining == 0 {
            break;
        }
        let platform = first
            .as_ref()
            .and_then(|agent| agent["local_metadata"]["os"]["platform"].as_str())
            .filter(|p| !p.is_empty() && !known.iter().any(|k| k == p))
            .map(str::to_string);
        match platform {
            Some(platform) if round < MAX_PLATFORM_DISCOVERY => {
                let kuery = and_kuery(base, &kql_any_of(FIELD, &[&platform]));
                let (n, _) = kb.count_agents(Some(kuery)).await?;
                platforms.push(Group {
                    id: platform.clone(),
                    name: platform.clone(),
                    count: Some(n),
                });
                known.push(platform);
            }
            _ => {
                unlisted = remaining;
                break;
            }
        }
    }
    platforms.sort_by(|a, b| b.count.cmp(&a.count).then_with(|| a.name.cmp(&b.name)));
    Ok((platforms, unlisted))
}

async fn load_saved_queries(kb: &Kibana<'_>) -> Result<Vec<SavedQuery>, String> {
    let mut all: Vec<SavedQuery> = Vec::new();
    for page in 1..=MAX_LIST_PAGES {
        let response = kb
            .send(
                reqwest::Method::GET,
                &format!("osquery/saved_queries?page={page}&pageSize={SAVED_QUERY_PAGE_SIZE}"),
                None,
                Some(API_VERSION),
            )
            .await?;
        let (queries, total) = parse_saved_queries(&response)?;
        let empty = queries.is_empty();
        all.extend(queries);
        if empty || all.len() >= total {
            break;
        }
    }
    all.sort_by_key(|q| q.id.to_lowercase());
    all.dedup_by(|a, b| a.id == b.id);
    Ok(all)
}

async fn fetch_results(
    kb: &Kibana<'_>,
    action: &LiveQueryAction,
    page: usize,
) -> Result<(Value, Option<Value>), String> {
    if !valid_action_id(&action.action_id) {
        return Err("Kibana returned an invalid live query action ID.".to_string());
    }
    let details = kb
        .send(
            reqwest::Method::GET,
            &format!("osquery/live_queries/{}", action.action_id),
            None,
            Some(API_VERSION),
        )
        .await?;
    let parsed = parse_live_query_action(&details)?;
    let query_action_id = parsed
        .query_action_id
        .or_else(|| action.query_action_id.clone());
    let results = if let Some(query_action_id) = query_action_id {
        if !valid_action_id(&query_action_id) {
            return Err("Kibana returned an invalid query action ID.".to_string());
        }
        Some(
            kb.send(
                reqwest::Method::GET,
                &format!(
                    "osquery/live_queries/{}/results/{}?page={page}&pageSize={PAGE_SIZE}",
                    action.action_id, query_action_id
                ),
                None,
                Some(API_VERSION),
            )
            .await?,
        )
    } else {
        None
    };
    Ok((details, results))
}

pub async fn execute_request(
    client: &EsClient,
    host: &str,
    request: &OsqueryRequest,
) -> OsqueryResponse {
    let kb = Kibana {
        client,
        host,
        space: &request.space,
    };
    let kind = match &request.kind {
        RequestKind::Groups => ResponseKind::Groups(load_groups(&kb).await),
        RequestKind::Search {
            seq,
            term,
            policy_ids,
        } => {
            let kuery = search_kuery(term, policy_ids);
            ResponseKind::Search(
                *seq,
                kb.get(&agents_suffix(AGENT_SEARCH_SIZE, kuery.as_deref()))
                    .await,
            )
        }
        RequestKind::SavedQueries => ResponseKind::SavedQueries(load_saved_queries(&kb).await),
        RequestKind::Submit(body) => ResponseKind::Submit(
            kb.send(
                reqwest::Method::POST,
                "osquery/live_queries",
                Some(body.to_string()),
                Some(API_VERSION),
            )
            .await,
        ),
        RequestKind::Results(action, page) => {
            ResponseKind::Results(*page, fetch_results(&kb, action, *page).await)
        }
    };
    OsqueryResponse {
        generation: request.generation,
        kind,
    }
}

// --- UI ----------------------------------------------------------------------------------

fn small_muted(ui: &mut Ui, text: impl Into<String>) {
    ui.label(
        RichText::new(text.into())
            .color(Theme::text_muted())
            .size(11.0),
    );
}

fn warning_label(ui: &mut Ui, text: impl Into<String>) {
    ui.label(
        RichText::new(format!("⚠️ {}", text.into()))
            .color(Theme::warning())
            .size(11.0),
    );
}

fn count_suffix(count: Option<usize>) -> String {
    count.map(|n| format!(" ({n})")).unwrap_or_default()
}

/// One-line description of the current targets.
fn target_summary(state: &OsqueryState) -> (String, bool) {
    if state.target_all {
        let n = state
            .groups
            .as_ref()
            .map(|g| format!(" ({})", g.all_count))
            .unwrap_or_default();
        return (format!("🎯 All agents{n}"), true);
    }
    let mut parts = Vec::new();
    if !state.selected_agents.is_empty() {
        parts.push(format!("{} agent(s)", state.selected_agents.len()));
    }
    if !state.selected_platforms.is_empty() {
        let names: Vec<&str> = state
            .selected_platforms
            .iter()
            .map(String::as_str)
            .collect();
        parts.push(format!("platforms: {}", names.join(", ")));
    }
    if !state.selected_policies.is_empty() {
        let names: Vec<String> = state
            .selected_policies
            .iter()
            .map(|id| {
                state
                    .groups
                    .as_ref()
                    .and_then(|g| g.policies.iter().find(|p| &p.id == id))
                    .map(|p| p.name.clone())
                    .unwrap_or_else(|| id.clone())
            })
            .collect();
        parts.push(format!("policies: {}", names.join(", ")));
    }
    if parts.is_empty() {
        ("🎯 No targets selected".to_string(), false)
    } else {
        (format!("🎯 {}", parts.join(" · ")), true)
    }
}

fn render_agent_picker(ui: &mut Ui, state: &mut OsqueryState) {
    ui.horizontal(|ui| {
        ui.label(
            RichText::new(format!("Agents ({} selected)", state.selected_agents.len())).strong(),
        );
        if state.search_inflight {
            ui.spinner();
        }
    });
    let search = ui.add_enabled(
        state.loaded,
        egui::TextEdit::singleline(&mut state.agent_search)
            .hint_text("🔍 Search by host name or agent ID…")
            .desired_width(f32::INFINITY),
    );
    if search.changed() {
        state.search_typed_at = Some(Instant::now());
    }

    if !state.selected_agents.is_empty() {
        ui.add_space(2.0);
        egui::ScrollArea::vertical()
            .id_salt("osquery_selected_agents")
            .max_height(64.0)
            .show(ui, |ui| {
                ui.horizontal_wrapped(|ui| {
                    let mut remove = None;
                    for id in state.selected_agents.iter().take(CHIP_DISPLAY_LIMIT) {
                        let name = state
                            .known_agents
                            .get(id)
                            .map(|a| a.name.as_str())
                            .unwrap_or(id);
                        if ui
                            .small_button(format!("{name} ✖"))
                            .on_hover_text(format!("Remove {id}"))
                            .clicked()
                        {
                            remove = Some(id.clone());
                        }
                    }
                    if state.selected_agents.len() > CHIP_DISPLAY_LIMIT {
                        small_muted(
                            ui,
                            format!("+{} more", state.selected_agents.len() - CHIP_DISPLAY_LIMIT),
                        );
                    }
                    if let Some(id) = remove {
                        state.selected_agents.remove(&id);
                    }
                });
            });
    }

    ui.horizontal(|ui| {
        if !state.loaded {
            small_muted(ui, "Load to search agents.");
        } else if state.search_results.len() < state.search_total {
            small_muted(
                ui,
                format!(
                    "{} of {} match(es); keep typing to narrow",
                    state.search_results.len(),
                    state.search_total
                ),
            );
        } else {
            small_muted(ui, format!("{} match(es)", state.search_total));
        }
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            if ui
                .add_enabled(
                    !state.selected_agents.is_empty(),
                    egui::Button::new("Clear").small(),
                )
                .clicked()
            {
                state.selected_agents.clear();
            }
            if ui
                .add_enabled(
                    !state.search_results.is_empty(),
                    egui::Button::new("Select shown").small(),
                )
                .on_hover_text("Select every agent in the list below")
                .clicked()
            {
                for agent in &state.search_results {
                    state.selected_agents.insert(agent.id.clone());
                }
            }
        });
    });
    egui::ScrollArea::vertical()
        .id_salt("osquery_agent_results")
        .max_height(170.0)
        .auto_shrink([false, true])
        .show(ui, |ui| {
            if state.loaded && state.search_results.is_empty() && !state.search_inflight {
                small_muted(ui, "No matching agents.");
            }
            for agent in &state.search_results {
                let mut selected = state.selected_agents.contains(&agent.id);
                let mut label = format!("{}  [{}]", agent.name, agent.status);
                if !agent.platform.is_empty() {
                    label.push_str(&format!(" · {}", agent.platform));
                }
                if ui
                    .checkbox(&mut selected, label)
                    .on_hover_text(format!(
                        "Agent ID: {}\nPolicy: {}",
                        agent.id, agent.policy_id
                    ))
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
}

fn render_group_picker(ui: &mut Ui, state: &mut OsqueryState) {
    ui.horizontal(|ui| {
        ui.label(RichText::new("Groups").strong());
        if state.groups_loading {
            ui.spinner();
        }
    });
    let Some(groups) = &state.groups else {
        small_muted(
            ui,
            if state.groups_loading {
                "Loading platforms and policies…"
            } else {
                "Load to see platforms and policies."
            },
        );
        return;
    };
    egui::ScrollArea::vertical()
        .id_salt("osquery_groups")
        .max_height(250.0)
        .auto_shrink([false, true])
        .show(ui, |ui| {
            ui.checkbox(
                &mut state.target_all,
                RichText::new(format!("All agents ({})", groups.all_count)).strong(),
            )
            .on_hover_text("Every agent enrolled in an Osquery-enabled policy");
            ui.add_enabled_ui(!state.target_all, |ui| {
                ui.add_space(4.0);
                ui.label(RichText::new("Platforms").color(Theme::text_secondary()));
                if groups.platforms.is_empty() {
                    small_muted(ui, "None found.");
                }
                for group in &groups.platforms {
                    let mut on = state.selected_platforms.contains(&group.id);
                    if ui
                        .checkbox(
                            &mut on,
                            format!("{}{}", group.name, count_suffix(group.count)),
                        )
                        .changed()
                    {
                        if on {
                            state.selected_platforms.insert(group.id.clone());
                        } else {
                            state.selected_platforms.remove(&group.id);
                        }
                    }
                }
                ui.add_space(4.0);
                ui.label(RichText::new("Policies").color(Theme::text_secondary()));
                if groups.policies.is_empty() {
                    small_muted(ui, "None found.");
                }
                for group in &groups.policies {
                    let mut on = state.selected_policies.contains(&group.id);
                    if ui
                        .checkbox(
                            &mut on,
                            format!("{}{}", group.name, count_suffix(group.count)),
                        )
                        .on_hover_text(format!("Policy ID: {}", group.id))
                        .changed()
                    {
                        if on {
                            state.selected_policies.insert(group.id.clone());
                        } else {
                            state.selected_policies.remove(&group.id);
                        }
                    }
                }
            });
            for warning in &groups.warnings {
                warning_label(ui, warning);
            }
        });
}

fn render_saved_queries(ui: &mut Ui, state: &mut OsqueryState) {
    let title = format!("📚 Saved queries ({})", state.saved_queries.len());
    let mut pick: Option<usize> = None;
    egui::CollapsingHeader::new(title)
        .id_salt("osquery_saved_queries")
        .show(ui, |ui| {
            if state.saved_loading {
                ui.spinner();
            }
            if let Some(error) = &state.saved_error {
                ui.colored_label(
                    Theme::danger(),
                    format!("Couldn't load saved queries: {error}"),
                );
            }
            if !state.loaded {
                small_muted(ui, "Load to fetch saved queries.");
                return;
            }
            ui.add(
                egui::TextEdit::singleline(&mut state.saved_filter)
                    .hint_text("🔍 Filter by name, description or SQL…")
                    .desired_width(f32::INFINITY),
            );
            let filter = state.saved_filter.trim().to_lowercase();
            egui::ScrollArea::vertical()
                .id_salt("osquery_saved_list")
                .max_height(160.0)
                .auto_shrink([false, true])
                .show(ui, |ui| {
                    let mut shown = 0;
                    for (idx, query) in state.saved_queries.iter().enumerate() {
                        if !filter.is_empty()
                            && ![&query.id, &query.description, &query.query]
                                .iter()
                                .any(|text| text.to_lowercase().contains(&filter))
                        {
                            continue;
                        }
                        shown += 1;
                        let selected = state.selected_saved.as_deref() == Some(query.id.as_str());
                        ui.horizontal(|ui| {
                            let response = ui
                                .selectable_label(selected, RichText::new(&query.id).monospace())
                                .on_hover_text(if query.description.is_empty() {
                                    query.query.clone()
                                } else {
                                    format!("{}\n\n{}", query.description, query.query)
                                });
                            if response.clicked() {
                                pick = Some(idx);
                            }
                            if !query.description.is_empty() {
                                small_muted(ui, &query.description);
                            }
                        });
                    }
                    if shown == 0 {
                        small_muted(ui, "No saved queries match.");
                    }
                });
        });
    if let Some(idx) = pick {
        let query = &state.saved_queries[idx];
        state.query = query.query.clone();
        state.selected_saved = Some(query.id.clone());
    }
}

pub fn render_osquery_module(
    ui: &mut Ui,
    state: &mut OsqueryState,
    clusters: &[String],
    output: &mut Vec<OsqueryRequest>,
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

    // --- Connection row ---
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
        let loading = state.groups_loading || state.saved_loading;
        let label = if state.loaded {
            "🔄 Reload"
        } else {
            "Load agents & queries"
        };
        if ui.add_enabled(!loading, egui::Button::new(label)).clicked() {
            state.load(output);
        }
    });
    if let Some(error) = &state.targets_error {
        ui.colored_label(Theme::danger(), error);
    }
    ui.add_space(6.0);

    // --- Targets: agents (left) and groups (right) ---
    ui.columns(2, |columns| {
        render_agent_picker(&mut columns[0], state);
        render_group_picker(&mut columns[1], state);
    });

    // Debounced agent search.
    let now = Instant::now();
    let debounce = Duration::from_millis(FILTER_DEBOUNCE_MS);
    if state.loaded && !state.groups_loading {
        if state.search_pending || filter_debounce_due(state.search_typed_at, now, debounce) {
            state.send_search(output);
        } else if let Some(remaining) =
            filter_debounce_remaining(state.search_typed_at, now, debounce)
        {
            ui.ctx().request_repaint_after(remaining);
        }
    }

    let (summary, has_targets) = target_summary(state);
    ui.add_space(4.0);
    ui.label(RichText::new(summary).strong().color(if has_targets {
        Theme::accent()
    } else {
        Theme::warning()
    }));

    ui.separator();
    render_saved_queries(ui, state);

    // --- Query editor ---
    ui.horizontal(|ui| {
        ui.label(RichText::new("Query").strong());
        if let Some(saved) = state.selected_saved_query() {
            let edited = saved.query.trim() != state.query.trim();
            let text = if edited {
                format!("from saved query '{}' (edited, runs ad hoc)", saved.id)
            } else {
                format!("saved query '{}'", saved.id)
            };
            small_muted(ui, text);
            if ui
                .small_button("✖")
                .on_hover_text("Detach saved query")
                .clicked()
            {
                state.selected_saved = None;
            }
        }
    });
    ui.add_enabled(
        !state.query_busy,
        egui::TextEdit::multiline(&mut state.query)
            .font(egui::TextStyle::Monospace)
            .desired_rows(3)
            .desired_width(f32::INFINITY),
    );
    ui.horizontal_wrapped(|ui| {
        if ui
            .add_enabled(
                !state.query_busy && has_targets,
                egui::Button::new("Run query"),
            )
            .clicked()
        {
            state.submit(output);
        }
        if state.query_busy {
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
                !state.query_busy && state.action.is_some(),
                egui::Button::new("Refresh results"),
            )
            .clicked()
        {
            state.fetch_results(state.result_page, output);
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

    // --- Results ---
    ui.separator();
    ui.horizontal_wrapped(|ui| {
        ui.label(RichText::new(format!("Results ({})", state.result_total)).strong());
        if ui
            .add_enabled(
                !state.query_busy && state.result_page > 0,
                egui::Button::new("Previous"),
            )
            .clicked()
        {
            state.fetch_results(state.result_page - 1, output);
        }
        ui.label(format!("Page {}", state.result_page + 1));
        if ui
            .add_enabled(
                !state.query_busy && (state.result_page + 1) * PAGE_SIZE < state.result_total,
                egui::Button::new("Next"),
            )
            .clicked()
        {
            state.fetch_results(state.result_page + 1, output);
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

    if !state.query_busy
        && state.auto_refresh
        && state.action.is_some()
        && let Some(next) = state.next_refresh
    {
        if Instant::now() >= next {
            state.fetch_results(state.result_page, output);
        } else {
            ui.ctx()
                .request_repaint_after(next.saturating_duration_since(Instant::now()));
        }
    }
    if !output.is_empty() {
        ui.ctx().request_repaint();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::{Read, Write};
    use std::net::TcpListener;
    use std::sync::{Arc, Mutex};

    use crate::core::config::ClusterConfig;

    fn ids(values: &[&str]) -> Vec<String> {
        values.iter().map(|v| v.to_string()).collect()
    }

    fn response(kind: ResponseKind) -> OsqueryResponse {
        OsqueryResponse {
            generation: 0,
            kind,
        }
    }

    #[test]
    fn live_query_requires_query_and_targets() {
        let agents = AgentTargets {
            agent_ids: ids(&["agent-1"]),
            ..Default::default()
        };
        assert!(build_live_query("", &agents, None).is_err());
        assert!(build_live_query("select 1;", &AgentTargets::default(), None).is_err());
        assert_eq!(
            build_live_query(" select * from uptime; ", &agents, None).unwrap(),
            json!({"query": "select * from uptime;", "agent_ids": ["agent-1"]})
        );
    }

    #[test]
    fn live_query_targets_groups_and_all() {
        let targets = AgentTargets {
            all: false,
            agent_ids: ids(&["a1"]),
            platforms: ids(&["ubuntu", "windows"]),
            policy_ids: ids(&["p1"]),
        };
        assert_eq!(
            build_live_query("select 1;", &targets, None).unwrap(),
            json!({"query": "select 1;", "agent_ids": ["a1"],
                   "agent_platforms": ["ubuntu", "windows"], "agent_policy_ids": ["p1"]})
        );
        let all = AgentTargets {
            all: true,
            ..targets
        };
        assert_eq!(
            build_live_query("select 1;", &all, None).unwrap(),
            json!({"query": "select 1;", "agent_all": true})
        );
    }

    #[test]
    fn saved_query_id_only_sent_while_unedited() {
        let saved = SavedQuery {
            id: "users".into(),
            query: "SELECT * FROM users;".into(),
            description: String::new(),
            ecs_mapping: Some(json!({"user.name": {"field": "username"}})),
        };
        let targets = AgentTargets {
            all: true,
            ..Default::default()
        };
        let body = build_live_query(" SELECT * FROM users; ", &targets, Some(&saved)).unwrap();
        assert_eq!(body["saved_query_id"], "users");
        assert_eq!(body["ecs_mapping"]["user.name"]["field"], "username");
        let body = build_live_query("SELECT uid FROM users;", &targets, Some(&saved)).unwrap();
        assert!(body.get("saved_query_id").is_none());
        assert!(body.get("ecs_mapping").is_none());
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
    fn builds_search_kuery_with_escaping_and_policy_scope() {
        assert_eq!(search_kuery("  ", &[]), None);
        assert_eq!(
            search_kuery("", &ids(&["p1"])).unwrap(),
            r#"policy_id:("p1")"#
        );
        assert_eq!(
            search_kuery("web", &[]).unwrap(),
            "local_metadata.host.hostname:*web* or local_metadata.elastic.agent.id:*web*"
        );
        let k = search_kuery("Web 01:(x)", &ids(&["p1", "p2"])).unwrap();
        assert!(k.starts_with(r#"(policy_id:("p1" or "p2")) and ("#));
        assert!(k.contains(r"local_metadata.host.hostname:*Web\ 01\:\(x\)*"));
        assert!(k.contains(r"local_metadata.host.hostname:*web\ 01\:\(x\)*"));
        assert_eq!(kql_quote(r#"a"b\c"#), r#""a\"b\\c""#);
        assert_eq!(url_encode("a b:*\"é"), "a%20b%3A%2A%22%C3%A9");
        assert_eq!(
            agents_suffix(50, Some("x:1")),
            "fleet/agents?page=1&perPage=50&showInactive=false&kuery=x%3A1"
        );
    }

    #[test]
    fn finds_osquery_policies_in_both_shapes() {
        let items = vec![
            json!({"package": {"name": "osquery_manager"}, "policy_ids": ["p1", "p2"]}),
            json!({"package": {"name": "osquery_manager"}, "policy_id": "p3"}),
            json!({"package": {"name": "system"}, "policy_ids": ["p9"]}),
            json!({"package": {"name": "osquery_manager"}, "policy_ids": ["p1"]}),
        ];
        assert_eq!(osquery_policy_ids(&items), ids(&["p1", "p2", "p3"]));
    }

    #[test]
    fn parses_agents_saved_queries_and_results() {
        let (agents, total) = parse_agents(json!({
            "total": 2,
            "items": [
                {"id": "agent-1", "status": "online", "policy_id": "p1",
                 "local_metadata": {"host": {"hostname": "web-1"}, "os": {"platform": "ubuntu"}}},
                {"id": "agent-2"}
            ]
        }))
        .unwrap();
        assert_eq!(total, 2);
        assert_eq!(agents[0].name, "web-1");
        assert_eq!(agents[0].platform, "ubuntu");
        assert_eq!(agents[1].name, "agent-2");

        let (queries, total) = parse_saved_queries(&json!({"page": 1, "per_page": 100, "total": 3,
            "data": [{"id": "users", "query": "SELECT * FROM users;", "description": "All users"},
                     {"id": "broken"}]}))
        .unwrap();
        assert_eq!(total, 3);
        assert_eq!(queries.len(), 1);
        assert_eq!(queries[0].description, "All users");
        let (queries, _) = parse_saved_queries(&json!({"data": {"total": 1, "saved_objects": [
            {"id": "so-1", "attributes": {"id": "procs", "query": "SELECT * FROM processes;"}}]}}))
        .unwrap();
        assert_eq!(queries[0].id, "procs");
        assert!(parse_saved_queries(&json!({"data": "nope"})).is_err());

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
    fn ignores_stale_generations_and_superseded_searches() {
        let mut state = OsqueryState::default();
        let mut out = Vec::new();
        state.agent_search = "web".into();
        state.send_search(&mut out);
        state.send_search(&mut out);
        assert_eq!(out.len(), 2);
        let agents = |name: &str| {
            Ok(
                json!({"total": 1, "items": [{"id": name, "local_metadata": {"host": {"hostname": name}}}]}),
            )
        };
        state.receive(response(ResponseKind::Search(1, agents("old"))));
        assert!(state.search_results.is_empty());
        state.receive(response(ResponseKind::Search(2, agents("new"))));
        assert_eq!(state.search_results[0].name, "new");

        state.reset_target();
        state.receive(response(ResponseKind::Groups(Ok(Groups::default()))));
        assert!(
            state.groups.is_none(),
            "response from the old cluster was applied"
        );
        assert_eq!(state.query, "SELECT * FROM system_info;");
    }

    #[test]
    fn tracks_outcomes_and_retries_initial_indexing() {
        let mut state = OsqueryState::default();
        state.receive(response(ResponseKind::Submit(Ok(
            json!({"data": {"action_id": "parent"}}),
        ))));
        state.receive(response(ResponseKind::Results(
            0,
            Err("HTTP 404: not indexed".to_string()),
        )));
        assert!(state.next_refresh.is_some());
        state.receive(response(ResponseKind::Results(
            0,
            Ok((
                json!({"data": {"action_id": "parent", "status": "completed", "queries": [
                    {"action_id": "query", "successful": 1, "failed": 1, "pending": 0}
                ]}}),
                Some(json!({"data": {"total": 0, "edges": []}})),
            )),
        )));
        assert!(state.status_summary.contains("failed: 1"));
        assert!(state.next_refresh.is_none());
    }

    #[test]
    fn renders_targets_groups_and_query_results() {
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
        let agent = Agent {
            id: "agent-1".into(),
            name: "web-1".into(),
            status: "online".into(),
            platform: "ubuntu".into(),
            policy_id: "p1".into(),
        };
        let mut state = OsqueryState {
            selected_cluster: "test".into(),
            loaded: true,
            search_results: vec![agent.clone()],
            search_total: 1,
            known_agents: HashMap::from([("agent-1".into(), agent)]),
            selected_agents: BTreeSet::from(["agent-1".into()]),
            groups: Some(Groups {
                all_count: 7,
                platforms: vec![Group {
                    id: "ubuntu".into(),
                    name: "ubuntu".into(),
                    count: Some(5),
                }],
                policies: vec![Group {
                    id: "p1".into(),
                    name: "Osquery prod".into(),
                    count: Some(7),
                }],
                ..Default::default()
            }),
            selected_policies: BTreeSet::from(["p1".into()]),
            action: Some(LiveQueryAction {
                action_id: "parent".into(),
                query_action_id: Some("query".into()),
            }),
            status: "completed".into(),
            status_summary: "successful: 1  failed: 0".into(),
            result_total: 1,
            rows: vec![BTreeMap::from([("osquery.pid".into(), "42".into())])],
            auto_refresh: false,
            ..Default::default()
        };
        let mut texts = Vec::new();
        for _ in 0..3 {
            let mut out = Vec::new();
            let frame = context.run(
                egui::RawInput {
                    screen_rect: Some(egui::Rect::from_min_size(
                        egui::Pos2::ZERO,
                        egui::vec2(1000.0, 800.0),
                    )),
                    ..Default::default()
                },
                |context| {
                    egui::CentralPanel::default().show(context, |ui| {
                        render_osquery_module(ui, &mut state, &["test".into()], &mut out);
                    });
                },
            );
            texts.clear();
            for shape in &frame.shapes {
                collect_text(&shape.shape, &mut texts);
            }
        }
        for expected in [
            "web-1",
            "1 selected",
            "All agents (7)",
            "ubuntu (5)",
            "Osquery prod (7)",
            "1 agent(s) · policies: Osquery prod",
            "completed",
            "osquery.pid",
            "42",
        ] {
            assert!(
                texts.iter().any(|text| text.contains(expected)),
                "missing {expected}: {texts:?}"
            );
        }
    }

    /// Minimal Kibana stand-in: answers by request path, records every request.
    fn spawn_mock_kibana(route: fn(&str) -> Value) -> (String, Arc<Mutex<Vec<String>>>) {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let address = listener.local_addr().unwrap();
        let requests = Arc::new(Mutex::new(Vec::new()));
        let log = requests.clone();
        std::thread::spawn(move || {
            for stream in listener.incoming() {
                let Ok(mut stream) = stream else { break };
                stream
                    .set_read_timeout(Some(Duration::from_secs(5)))
                    .unwrap();
                let mut bytes = Vec::new();
                let mut buffer = [0; 4096];
                loop {
                    let count = stream.read(&mut buffer).unwrap_or(0);
                    if count == 0 {
                        break;
                    }
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
                let request = String::from_utf8_lossy(&bytes).to_string();
                let path = request.split_whitespace().nth(1).unwrap_or("").to_string();
                log.lock().unwrap().push(request);
                let body = route(&path).to_string();
                let _ = write!(
                    stream,
                    "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
                    body.len(),
                    body
                );
            }
        });
        (format!("http://{address}"), requests)
    }

    fn decode(path: &str) -> String {
        let mut out = Vec::new();
        let bytes = path.as_bytes();
        let mut i = 0;
        while i < bytes.len() {
            if bytes[i] == b'%' && i + 2 < bytes.len() {
                out.push(u8::from_str_radix(&path[i + 1..i + 3], 16).unwrap());
                i += 3;
            } else {
                out.push(bytes[i]);
                i += 1;
            }
        }
        String::from_utf8(out).unwrap()
    }

    fn mock_route(path: &str) -> Value {
        let path = decode(path);
        let agent = |id: &str, host: &str, platform: &str| {
            json!({"id": id, "status": "online", "policy_id": "p1",
                   "local_metadata": {"host": {"hostname": host}, "os": {"platform": platform}}})
        };
        if path.contains("/fleet/package_policies") {
            json!({"total": 2, "items": [
                {"package": {"name": "osquery_manager"}, "policy_ids": ["p1"]},
                {"package": {"name": "system"}, "policy_ids": ["p2"]}]})
        } else if path.contains("/fleet/agent_policies") {
            json!({"total": 2, "items": [{"id": "p1", "name": "Osquery prod"}, {"id": "p2", "name": "Other"}]})
        } else if path.contains("/fleet/agents") {
            // Two agents on p1: one ubuntu, one on a platform outside the hints.
            if path.contains("not local_metadata.os.platform") {
                if path.contains("\"nixos\"") {
                    json!({"total": 0, "items": []})
                } else {
                    json!({"total": 1, "items": [agent("a2", "nix-box", "nixos")]})
                }
            } else if path.contains("local_metadata.os.platform:(\"ubuntu\")") {
                json!({"total": 1, "items": [agent("a1", "web-1", "ubuntu")]})
            } else if path.contains("local_metadata.os.platform:(\"nixos\")") {
                json!({"total": 1, "items": [agent("a2", "nix-box", "nixos")]})
            } else if path.contains("local_metadata.os.platform") {
                json!({"total": 0, "items": []})
            } else if path.contains("hostname:*web*") {
                json!({"total": 1, "items": [agent("a1", "web-1", "ubuntu")]})
            } else {
                json!({"total": 2, "items": [agent("a1", "web-1", "ubuntu"), agent("a2", "nix-box", "nixos")]})
            }
        } else if path.contains("/osquery/saved_queries") {
            json!({"total": 1, "data": [{"id": "users", "query": "SELECT * FROM users;"}]})
        } else if path.contains("/results/") {
            json!({"data": {"total": 1, "edges": [{"fields": {"osquery.pid": [42]}}]}})
        } else if path.contains("/osquery/live_queries/") {
            json!({"data": {"action_id": "parent", "status": "completed", "queries": [{"action_id": "query"}]}})
        } else if path.contains("/osquery/live_queries") {
            json!({"data": {"action_id": "parent", "queries": [{"action_id": "query"}]}})
        } else {
            json!({})
        }
    }

    #[tokio::test]
    async fn loads_groups_searches_and_runs_targeted_query() {
        let (host, requests) = spawn_mock_kibana(mock_route);
        let config = ClusterConfig {
            name: "test".into(),
            username: "user".into(),
            ..Default::default()
        };
        let client = EsClient::with_password(&config, "password").unwrap();
        let mut state = OsqueryState {
            selected_cluster: "test".into(),
            space: "security".into(),
            ..Default::default()
        };
        let run = |out: Vec<OsqueryRequest>| {
            let client = client.clone();
            let host = host.clone();
            async move {
                let mut responses = Vec::new();
                for request in &out {
                    responses.push(execute_request(&client, &host, request).await);
                }
                responses
            }
        };

        let mut out = Vec::new();
        state.load(&mut out);
        for r in run(out).await {
            state.receive(r);
        }
        let groups = state.groups.clone().expect("groups loaded");
        assert_eq!(groups.all_count, 2);
        assert_eq!(
            groups.policies,
            vec![Group {
                id: "p1".into(),
                name: "Osquery prod".into(),
                count: Some(2)
            }]
        );
        let platforms: Vec<_> = groups
            .platforms
            .iter()
            .map(|p| (p.name.as_str(), p.count))
            .collect();
        assert_eq!(platforms, vec![("nixos", Some(1)), ("ubuntu", Some(1))]);
        assert_eq!(groups.unlisted, 0);
        assert_eq!(state.saved_queries[0].id, "users");
        assert!(state.search_pending);

        let mut out = Vec::new();
        state.agent_search = "web".into();
        state.send_search(&mut out);
        for r in run(out).await {
            state.receive(r);
        }
        assert_eq!(state.search_results.len(), 1);
        assert_eq!(state.search_results[0].name, "web-1");

        state.selected_agents.insert("a1".into());
        state.selected_platforms.insert("nixos".into());
        state.selected_saved = Some("users".into());
        state.query = "SELECT * FROM users;".into();
        let mut out = Vec::new();
        state.submit(&mut out);
        for r in run(out).await {
            state.receive(r);
        }
        let mut out = Vec::new();
        state.fetch_results(0, &mut out);
        for r in run(out).await {
            state.receive(r);
        }
        assert_eq!(state.status, "completed");
        assert_eq!(state.rows[0]["osquery.pid"], "42");

        let requests = requests.lock().unwrap().clone();
        let search = requests
            .iter()
            .map(|r| decode(r))
            .find(|r| r.contains("hostname:*web*"))
            .expect("search request");
        assert!(search.starts_with("GET /s/security/api/fleet/agents?page=1&perPage=50"));
        assert!(search.contains(r#"(policy_id:("p1")) and ("#));
        let submit = requests
            .iter()
            .find(|r| r.starts_with("POST /s/security/api/osquery/live_queries "))
            .expect("submit request");
        assert!(
            submit
                .to_lowercase()
                .contains("elastic-api-version: 2023-10-31")
        );
        assert!(submit.to_lowercase().contains("kbn-xsrf: true"));
        assert!(submit.contains(r#""agent_ids":["a1"]"#));
        assert!(submit.contains(r#""agent_platforms":["nixos"]"#));
        assert!(submit.contains(r#""saved_query_id":"users""#));
        assert!(requests.iter().any(|r| {
            r.starts_with("GET /s/security/api/osquery/saved_queries?page=1&pageSize=100 ")
        }));
    }
}
